//! V37 network requests (`net_request`): `fetch`, `Request`, `Response` and
//! streamed bodies over the exact `net.request` capability, against a
//! loopback server on 127.0.0.1 (no other host is ever contacted). Expected
//! logs were produced by Node 24 running the same programs against
//! `net_cases/server.mjs` (see `net_cases/node_runner.mjs`); they are pinned
//! here, so the tests need no Node installation. These tests use the real
//! clock: completions arrive from real sockets.
use super::*;
use crate::net_loopback::Loopback;
use varta_common::language::LanguageProfile;
use varta_common::net::{MAX_PENDING_REQUESTS, NetError, REQUEST_CAPABILITY};
use varta_common::scheduler::{SchedulerError, TIMER_CAPABILITY};
use varta_common::{CapabilityValue, RequestScope};

const V37: LanguageProfile = LanguageProfile::CheckedF64Utf16V37;
const V36: LanguageProfile = LanguageProfile::CheckedF64Utf16V36;
const BASE: &str = "__BASE__";

fn compile_with(source: &str, profile: LanguageProfile) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}
fn compile(source: &str) -> Vec<u8> {
    compile_with(source, V37)
}
fn requests(urls: &[String], methods: Option<&[&str]>) -> CapabilityGrant {
    CapabilityGrant {
        id: REQUEST_CAPABILITY.into(),
        value: CapabilityValue::Requests(RequestScope {
            urls: urls.to_vec(),
            methods: methods.map(|m| m.iter().map(|m| (*m).into()).collect()),
        }),
    }
}
fn boolean(id: &str) -> CapabilityGrant {
    CapabilityGrant {
        id: id.into(),
        value: CapabilityValue::Boolean(true),
    }
}
/// The server's whole origin, plus timers.
fn grants(server: &Loopback) -> Vec<CapabilityGrant> {
    vec![
        requests(&[format!("{}/**", server.base)], None),
        boolean(TIMER_CAPABILITY),
    ]
}
fn program(source: &str, server: &Loopback) -> String {
    source.replace(BASE, &server.base)
}
fn lines(expected: &[&str]) -> Vec<String> {
    expected.iter().map(|s| format!("varta log: {s}")).collect()
}
/// Test-only fuel for reading a body beyond `MAX_RESPONSE_BODY_BYTES`: each
/// body byte costs guest string work (about 2.6 fuel per byte measured), so
/// the production budget ends such a read before the byte limit does.
const BODY_LIMIT_FUEL: u64 = abi::FUEL * 4;

/// `run` with a test-only fuel budget instead of the production one.
fn run_with_fuel(source: &str, expected: &[&str], fuel: u64) {
    let server = Loopback::start();
    let source = program(source, &server).replace(
        "__LIMIT__",
        &varta_common::net::MAX_RESPONSE_BODY_BYTES.to_string(),
    );
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&compile(&source)).unwrap();
    let mut execution = execution::Execution::new(&runtime.engine, &grants(&server), None).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    store.set_fuel(fuel).unwrap();
    let start = instance.get_func(&mut *store, "_start").unwrap();
    start.call(&mut *store, &[], &mut []).unwrap();
    exceptions::finish(&instance, store).unwrap();
    scheduler::checkpoint(&instance, store).unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(store.data().output.lines, lines(expected), "{source}");
}
fn run(source: &str, expected: &[&str]) {
    let server = Loopback::start();
    let source = program(source, &server).replace(
        "__LIMIT__",
        &varta_common::net::MAX_RESPONSE_BODY_BYTES.to_string(),
    );
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compile(&source), &grants(&server))
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    assert_eq!(output.lines, lines(expected), "{source}");
}

#[test]
fn fetch_basics_match_node() {
    run(
        include_str!("net_cases/basics.ts"),
        &[
            "201 Created true application/json",
            "true 3 true",
            r#"{"method":"POST","auth":"Bearer k","type":"application/json","query":"x=1","body":"{\"a\":1}"}"#,
            r#"{"method":"PUT","auth":null,"type":"application/x-www-form-urlencoded;charset=UTF-8","query":"q=a+b","body":"x=1+2"}"#,
            "404 false Not Found nope",
            "reread: true",
            "done",
        ],
    );
}

#[test]
fn url_and_search_params_match_node() {
    run(
        include_str!("net_cases/url.ts"),
        &[
            "https://example.com/a/c%20d?q=1%202#frag%20ment",
            "https: example.com example.com true /a/c%20d ?q=1%202 #frag%20ment https://example.com",
            "http://h.test:8080/p/x/y?z=%C3%A9",
            "http://other.test/k",
            "http://h.test/p/q?only",
            "http://h.test/p?q#f",
            "true false true",
            "true Invalid URL",
            "4 1 2 x y true null",
            "a=3&b=x+y&c=%C3%A9+%26+%C3%BC",
            "http://h.test/path?x=1&y=two+words",
            "two words ?x=1&y=two+words",
            "name=Ada+Lovelace&tag=*-._%7E",
        ],
    );
}

#[test]
fn redirects_match_node_with_error_as_the_default() {
    run(
        include_str!("net_cases/redirects.ts"),
        &[
            "error: true fetch failed",
            "manual: 302 /hello false",
            "follow: 200 true true hello world",
        ],
    );
}

#[test]
fn streamed_bodies_match_node() {
    run(
        include_str!("net_cases/streams.ts"),
        &["text/event-stream 60 32 true", "é€😀 4 true", "false"],
    );
}

#[test]
fn aborts_and_timeouts_match_node() {
    run(
        include_str!("net_cases/abort.ts"),
        &[
            "aborted: AbortError: This operation was aborted",
            "pre-aborted: true",
            "timeout: TimeoutError: The operation was aborted due to timeout",
            "first: false true",
            "body: Error: enough",
            "done",
        ],
    );
}

/// The grant names another port: every request is denied before any
/// connection, the denial is catchable and logged, and the program goes on.
#[test]
fn denials_are_recoverable_and_open_no_socket() {
    let server = Loopback::start();
    let source = program(include_str!("net_cases/denial.ts"), &server);
    let other = [requests(&["http://127.0.0.1:1/**".into()], None)];
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compile(&source), &other)
        .unwrap();
    let denial = |method: &str, path: &str| {
        format!(
            "varta log: <capability denied: net.request> ({method} {}{path})",
            server.base
        )
    };
    assert_eq!(
        output.lines,
        [
            denial("GET", "/hello"),
            "varta log: denied: NotAllowedError true".into(),
            denial("DELETE", "/echo"),
            "varta log: method: NotAllowedError".into(),
            "varta log: invalid: true".into(),
            "varta log: continued".into(),
        ]
    );
    assert_eq!(server.accepted(), 0);
    // No grant at all denies the same way.
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compile(&source), &[])
        .unwrap();
    assert_eq!(output.lines.last().unwrap(), "varta log: continued");
    assert_eq!(server.accepted(), 0);
}

/// The host ceiling narrows the package's grant: URL and method must satisfy
/// both, and an empty ceiling denies everything, recoverably.
#[test]
fn host_ceilings_deny_recoverably_before_any_socket() {
    let server = Loopback::start();
    let source = program(include_str!("net_cases/denial.ts"), &server);
    let bytes = compile(&source);
    let requested = [requests(&[format!("{}/**", server.base)], None)];
    for ceiling in [
        vec![],
        vec![requests(&[format!("{}/public/**", server.base)], None)],
        vec![requests(&[format!("{}/**", server.base)], Some(&["POST"]))],
    ] {
        let output = VartaRuntime::new()
            .run_wasm_bytes_with_policy(&bytes, &requested, &ceiling)
            .unwrap();
        assert_eq!(
            output.lines[1], "varta log: denied: NotAllowedError true",
            "{ceiling:?}"
        );
        assert_eq!(output.lines.last().unwrap(), "varta log: continued");
    }
    assert_eq!(server.accepted(), 0);
    // The method list admits GET only: `/hello` is fetched, DELETE is denied.
    let get = [requests(&[format!("{}/**", server.base)], Some(&["GET"]))];
    let output = VartaRuntime::new()
        .run_wasm_bytes_with_policy(&bytes, &requested, &get)
        .unwrap();
    assert_eq!(
        output.lines[1..],
        lines(&["method: NotAllowedError", "invalid: true", "continued"])
    );
    assert!(output.lines[0].contains("(DELETE "), "{:?}", output.lines);
    assert_eq!(server.accepted(), 1);
}

/// V37 timers deny recoverably too: `setTimeout` throws a catchable
/// `NotAllowedError` synchronously, the sleep pattern rejects, and nothing
/// was armed. V36 keeps its fatal denial.
#[test]
fn v37_timer_denials_are_recoverable_and_v36_stays_fatal() {
    let source = r#"
export async function main(): Promise<void> {
  let ran = false;
  try {
    setTimeout(() => { ran = true; }, 5);
  } catch (error) {
    console.log(`timeout: ${error instanceof DOMException ? error.name + ' ' + error.message : 'other'}`);
  }
  try {
    await new Promise<void>((resolve) => setTimeout(resolve, 5));
  } catch (error) {
    console.log(`sleep: ${error instanceof DOMException ? error.name : 'other'}`);
  }
  try {
    AbortSignal.timeout(5);
  } catch (error) {
    console.log(`signal: ${error instanceof DOMException ? error.name : 'other'}`);
  }
  console.log(`ran: ${ran}`);
}
"#;
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compile(source), &[])
        .unwrap();
    let denial = "varta log: <capability denied: time.timer> (timer)";
    assert_eq!(
        output.lines,
        [
            denial.to_owned(),
            "varta log: timeout: NotAllowedError <capability denied: time.timer> (setTimeout)"
                .into(),
            denial.into(),
            "varta log: sleep: NotAllowedError".into(),
            denial.into(),
            "varta log: signal: NotAllowedError".into(),
            "varta log: ran: false".into(),
        ]
    );
    // With the grant the same program arms its timers.
    let granted = VartaRuntime::new()
        .run_wasm_bytes(&compile(source), &[boolean(TIMER_CAPABILITY)])
        .unwrap();
    assert_eq!(granted.lines, ["varta log: ran: true"]);
    let v36 = "export function main(): void { setTimeout(() => {}, 5); }";
    let error = VartaRuntime::new()
        .run_wasm_bytes(&compile_with(v36, V36), &[])
        .unwrap_err();
    assert!(
        matches!(error, RuntimeError::Scheduler(SchedulerError::TimerDenied)),
        "{error:?}"
    );
}

/// HTTPS through rustls: a test-generated certificate verifies against a
/// test-only root; without that root the handshake fails and `fetch`
/// rejects with `TypeError: fetch failed`.
#[test]
fn https_requests_verify_the_server_certificate() {
    let (server, root) = Loopback::start_tls();
    let source = program(
        r#"
export async function main(): Promise<void> {
  try {
    const response = await fetch('__BASE__/hello');
    console.log(`${response.status} ${response.url.startsWith('https://')} ${await response.text()}`);
  } catch (error) {
    console.log(`${error instanceof TypeError} ${error instanceof Error ? error.message : ''}`);
  }
}
"#,
        &server,
    );
    let bytes = compile(&source);
    let trusted = crate::net::with_test_root(root, || {
        VartaRuntime::new().run_wasm_bytes(&bytes, &grants(&server))
    })
    .unwrap();
    assert_eq!(trusted.lines, lines(&["200 true hello world"]));
    let untrusted = VartaRuntime::new()
        .run_wasm_bytes(&bytes, &grants(&server))
        .unwrap();
    assert_eq!(untrusted.lines, lines(&["true fetch failed"]));
}

/// A body beyond `MAX_RESPONSE_BODY_BYTES` fails the read with Node's
/// `TypeError: terminated`, and the pending-request bound is fatal.
#[test]
fn response_body_and_pending_request_limits() {
    run_with_fuel(
        r#"
export async function main(): Promise<void> {
  const response = await fetch('__BASE__/big');
  const body = response.body;
  if (body === null) {
    return;
  }
  const reader = body.getReader();
  let bytes = 0;
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) {
        break;
      }
      bytes += value.length;
    }
  } catch (error) {
    console.log(`${bytes > 0 && bytes <= __LIMIT__} ${error instanceof TypeError} ${error instanceof Error ? error.message : ''}`);
  }
}
"#,
        &["true true terminated"],
        BODY_LIMIT_FUEL,
    );
    let server = Loopback::start();
    let source = program(
        &format!(
            r#"
export function main(): void {{
  for (let i = 0; i <= {MAX_PENDING_REQUESTS}; i++) {{
    void fetch('__BASE__/slow');
  }}
}}
"#
        ),
        &server,
    );
    let error = VartaRuntime::new()
        .run_wasm_bytes(&compile(&source), &grants(&server))
        .unwrap_err();
    assert!(
        matches!(error, RuntimeError::Net(NetError::Budget)),
        "{error:?}"
    );
}

/// A request that never answers ends the execution at the deadline, like a
/// timer due too late.
#[test]
fn a_request_in_flight_ends_at_the_execution_deadline() {
    let server = Loopback::start();
    let source = program(
        "export async function main(): Promise<void> { await fetch('__BASE__/slow'); }",
        &server,
    );
    let started = std::time::Instant::now();
    let error = VartaRuntime::new()
        .run_wasm_bytes(&compile(&source), &grants(&server))
        .unwrap_err();
    assert!(error.to_string().contains("deadline"), "{error}");
    assert!(started.elapsed() >= std::time::Duration::from_secs(abi::DEADLINE_SECONDS));
}

/// Callbacks, promises and streams reached only through frames waiting on
/// requests stay rooted: a full collection runs before every job.
#[test]
fn waiting_frames_keep_requests_rooted_through_collections() {
    use varta_compiler::suspension_test_hook as hook;
    let server = Loopback::start();
    let source = program(
        r#"
class Box { label: string; constructor(label: string) { this.label = label; } }
async function get(path: string, box: Box): Promise<string> {
  const response = await fetch('__BASE__' + path);
  const text = await response.text();
  return box.label + ':' + text.length;
}
export async function main(): Promise<void> {
  const results = await Promise.all([get('/hello', new Box('a')), get('/sse', new Box('b'))]);
  console.log(results[0] ?? '');
  console.log(results[1] ?? '');
}
"#,
        &server,
    );
    let bytes = hook::compile_with_profile(&source, V37).unwrap_or_else(|e| panic!("{e}"));
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&bytes).unwrap();
    let mut execution = execution::Execution::new(&runtime.engine, &grants(&server), None).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    let start = instance.get_func(&mut *store, "_start").unwrap();
    start.call(&mut *store, &[], &mut []).unwrap();
    exceptions::finish(&instance, store).unwrap();
    let collector = instance
        .get_typed_func::<(), ()>(&mut *store, hook::COLLECT)
        .unwrap();
    let resume = instance
        .get_typed_func::<(i64, i32, i64), i32>(
            &mut *store,
            varta_common::function::frame::RESUME_EXPORT,
        )
        .unwrap();
    let mut collections = 0;
    loop {
        while let Some(token) = store.data_mut().scheduler.next().unwrap() {
            // Test-only budget: forced collections are paid for outside the
            // execution's production fuel.
            let remaining = store.get_fuel().unwrap();
            store.set_fuel(abi::FUEL).unwrap();
            collector.call(&mut *store, ()).unwrap();
            store.set_fuel(remaining).unwrap();
            collections += 1;
            resume.call(&mut *store, (token, 0, 0)).unwrap();
        }
        if !store.data().net.alive() {
            break;
        }
        let deadline = store.data().deadline;
        crate::net::wait(store.data_mut(), deadline).unwrap();
    }
    assert!(collections > 10);
    assert_eq!(store.data().output.lines, lines(&["a:11", "b:60"]));
}

/// Program modules cannot reach the intrinsics or the engine-internal
/// members of the prelude: the network is reachable only through `fetch`.
#[test]
fn programs_cannot_call_the_network_intrinsics() {
    for source in [
        "export function main(): void { varta.net.start('x'); }",
        "export function main(): void { varta.net.watch(1, () => {}); }",
        "export function main(): void { new ReadableStream().stateOf(); }",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(source, V37).unwrap_err();
        let text = error.to_string();
        assert!(
            text.contains("unsupported") || text.contains("unknown class method"),
            "{source}: {text}"
        );
    }
    // V36 names no fetch prelude: `fetch` is an unknown call there.
    let error = varta_compiler::compile_typescript_with_profile(
        "export async function main(): Promise<void> { await fetch('http://127.0.0.1:1/'); }",
        V36,
    )
    .unwrap_err();
    assert!(error.to_string().contains("unsupported"), "{error}");
}

/// A fetch program imports the net operations at host ABI 33; relabelled
/// as v36 it fails validation before any guest effect.
#[test]
fn net_imports_need_host_abi_33() {
    let bytes = compile(
        "export async function main(): Promise<void> { await fetch('http://127.0.0.1:1/'); }",
    );
    let module = VartaRuntime::new().validate_module(&bytes).unwrap();
    let imports: Vec<String> = module.imports().map(|i| i.name().to_owned()).collect();
    for operation in [
        varta_common::HostOperation::NetRequestStart,
        varta_common::HostOperation::NetRequestWatch,
        varta_common::HostOperation::NetRequestTake,
        varta_common::HostOperation::NetRequestText,
    ] {
        assert!(
            imports.iter().any(|name| name == operation.name()),
            "{imports:?}"
        );
        assert_eq!(operation.minimum_abi(), abi::NET_REQUEST_VERSION);
    }
    use varta_common::language::{LanguageContract, SCHEMA_VERSION};
    let declared = LanguageContract {
        schema_version: SCHEMA_VERSION,
        profile: V36,
    }
    .to_bytes()
    .unwrap();
    let relabelled = crate::language_tests::replace(&bytes, &declared);
    let downgraded = crate::language_tests::host_abi(&relabelled, Some(abi::TIMERS_VERSION));
    for bytes in [relabelled, downgraded] {
        assert!(VartaRuntime::new().validate_module(&bytes).is_err());
    }
}
