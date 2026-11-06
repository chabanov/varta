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
            response_headers: None,
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

const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;

/// `run` of a v38 (`web_bytes`) program.
fn run_v38(source: &str, expected: &[&str]) {
    let server = Loopback::start();
    let source = program(source, &server);
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compile_with(&source, V38), &grants(&server))
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    assert_eq!(output.lines, lines(expected), "{source}");
}

/// V38 byte bodies match Node (`node_runner.mjs --bytes`): reader chunks are
/// `Uint8Array`s, `bytes()`/`arrayBuffer()`/`blob()` keep bytes that are no
/// UTF-8, a pull source wraps another body, and byte, blob and multipart
/// request bodies arrive as sent (the boundary is masked; Varta's is
/// deterministic).
#[test]
fn byte_bodies_match_node() {
    run_v38(
        include_str!("web_bytes_cases/net_bytes.ts"),
        &[
            "9 é€😀 false",
            "bytes 0,255,128,65",
            "arrayBuffer 4 0,255,128,65",
            "blob 4 application/octet-stream 255,128",
            "used true",
            "wrapped 200 text/event-stream 60",
            "sent bytes null 0,255,128,65",
            "sent blob text/plain héllo",
            r#"sent form multipart/form-data "--<b>\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\nvalue\r\n--<b>\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.txt\"\r\nContent-Type: text/plain\r\n\r\nabc\r\n--<b>--\r\n""#,
        ],
    );
}

/// A v38 fetch imports `net_request_start_v2` (descriptor version 2) and
/// `net_body_read_bytes`, both at host ABI 34, and never the v37 start; a
/// relabelled v37 module fails validation before any guest effect.
#[test]
fn byte_imports_need_host_abi_34() {
    use varta_common::HostOperation as H;
    let bytes = compile_with(
        "export async function main(): Promise<void> {
           const r = await fetch('http://127.0.0.1:1/', { method: 'POST', body: new Uint8Array([1]) });
           const b = await r.bytes();
           console.log(String(b.length));
         }",
        V38,
    );
    let module = VartaRuntime::new().validate_module(&bytes).unwrap();
    let imports: Vec<String> = module.imports().map(|i| i.name().to_owned()).collect();
    for operation in [H::NetRequestStartV2, H::NetBodyReadBytes] {
        assert!(
            imports.iter().any(|name| name == operation.name()),
            "{imports:?}"
        );
        assert_eq!(operation.minimum_abi(), abi::PROGRAM_LIMITS_VERSION);
    }
    assert_eq!(H::NetRequestStartV2.capability(), Some(REQUEST_CAPABILITY));
    assert_eq!(H::NetBodyReadBytes.capability(), None);
    assert!(!imports.iter().any(|name| name == H::NetRequestStart.name()));
    assert_eq!(V38.host_operation(H::NetRequestStart), H::NetRequestStartV2);
    assert_eq!(V37.host_operation(H::NetRequestStart), H::NetRequestStart);
    use varta_common::language::{LanguageContract, SCHEMA_VERSION};
    let declared = LanguageContract {
        schema_version: SCHEMA_VERSION,
        profile: V37,
    }
    .to_bytes()
    .unwrap();
    let relabelled = crate::language_tests::replace(&bytes, &declared);
    let downgraded = crate::language_tests::host_abi(&relabelled, Some(abi::NET_REQUEST_VERSION));
    for bytes in [relabelled, downgraded] {
        assert!(VartaRuntime::new().validate_module(&bytes).is_err());
    }
}

/// A byte body is checked against the grant before any connection: the
/// denial is recoverable and no socket opens.
#[test]
fn byte_body_denials_open_no_socket() {
    let server = Loopback::start();
    let source = program(
        "export async function main(): Promise<void> {
           const form = new FormData();
           form.append('f', new Blob([new Uint8Array([1, 2])]), 'x.bin');
           try {
             await fetch('__BASE__/mirror', { method: 'POST', body: form });
           } catch (error) {
             console.log(error instanceof DOMException ? error.name : 'other');
           }
         }",
        &server,
    );
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compile_with(&source, V38), &[])
        .unwrap();
    assert_eq!(
        output.lines,
        [
            format!(
                "varta log: <capability denied: net.request> (POST {}/mirror)",
                server.base
            ),
            "varta log: NotAllowedError".into(),
        ]
    );
    assert_eq!(server.accepted(), 0);
}

/// V37 keeps its text-body prelude: `Blob`, `FormData`, `ArrayBuffer`, a
/// typed `ReadableStream<Uint8Array>` and `globalThis` are rejected there.
#[test]
fn v37_rejects_the_byte_body_types() {
    for source in [
        "export function main(): void { const b = new Blob(['x']); console.log(String(b.size)); }",
        "export function main(): void { const f = new FormData(); console.log(String(f.has('a'))); }",
        "export function main(): void { const a = new ArrayBuffer(1); console.log(String(a.byteLength)); }",
        "function f(s: ReadableStream<Uint8Array>): boolean { return s.locked; } export function main(): void {}",
        "export function main(): void { const g = globalThis; }",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, V37).is_err(),
            "{source}"
        );
    }
}

/// Test-only network inactivity window of the long-lived transfer tests (a
/// host-lowered `RunOptions::network_idle`, with the same per-turn
/// deadline), far shorter than the streams; production values are unchanged.
const WINDOW_MS: u64 = 500;
/// The long stream: `EVENT_COUNT` events of `EVENT_BYTES` data bytes, each
/// after `EVENT_PAUSE_MS`, so a chunk arrives inside every window while the
/// stream lasts several windows and carries more than one v37 body. Events
/// stay small, like a session stream's: a turn's guest work grows with the
/// chunk it reads.
const EVENT_COUNT: u64 = 144;
const EVENT_BYTES: usize = 4 * 1024;
const EVENT_PAUSE_MS: u64 = 12;
/// The silence of `/stall`: several windows.
const STALL_GAP_MS: u64 = 3 * WINDOW_MS;
/// How many windows the long stream lasts at least.
const STREAM_WINDOWS: u64 = 3;
const _: () = assert!(EVENT_PAUSE_MS < WINDOW_MS);
const _: () = assert!(EVENT_COUNT * EVENT_PAUSE_MS >= STREAM_WINDOWS * WINDOW_MS);
const _: () =
    assert!(EVENT_COUNT as usize * EVENT_BYTES > varta_common::net::MAX_RESPONSE_BODY_BYTES);

/// `net_cases/v38/long_stream.ts` against `server`.
fn long_stream(server: &Loopback) -> String {
    program(include_str!("net_cases/v38/long_stream.ts"), server)
        .replace("__COUNT__", &EVENT_COUNT.to_string())
        .replace("__SIZE__", &EVENT_BYTES.to_string())
        .replace("__PAUSE__", &EVENT_PAUSE_MS.to_string())
        .replace("__GAP__", &STALL_GAP_MS.to_string())
}
/// Run `bytes` as an application instance with the test-only window.
fn application(
    bytes: &[u8],
    requested: Vec<CapabilityGrant>,
    ceiling: Option<Vec<CapabilityGrant>>,
) -> RunReport {
    let mut run = RunOptions {
        requested,
        ceiling,
        ..RunOptions::default()
    };
    run.budgets.deadline_per_turn_ms = Some(WINDOW_MS);
    run.network_idle = Some(std::time::Duration::from_millis(WINDOW_MS));
    VartaRuntime::new().run_application(
        bytes,
        ApplicationOptions {
            run,
            ..ApplicationOptions::default()
        },
    )
}
/// The lines `long_stream.ts` prints on every host.
fn long_stream_lines() -> Vec<String> {
    let bytes: usize = (1..=EVENT_COUNT)
        .map(|id| crate::net_loopback::event(id, EVENT_BYTES).len())
        .sum();
    lines(&[
        r#"agent: 200 {"guestAgent":false}"#,
        "compressed: 200 compressed text no coding",
        &format!("events: 200 {bytes} bytes, last id: {EVENT_COUNT}"),
        "stall: TypeError terminated after data",
    ])
}

/// V38: a server-sent event stream that lasts several per-turn deadlines
/// and carries more than 512 KiB completes, because each wait for bytes
/// lasts one window and every chunk renews it; a stream silent for longer
/// than one window fails the waiting read with `response body timed out`;
/// the guest's `User-Agent` never reaches the server.
#[test]
fn v38_long_lived_streams_are_bounded_by_inactivity() {
    let server = Loopback::start();
    let bytes = compile_with(&long_stream(&server), V38);
    let started = std::time::Instant::now();
    let report = application(&bytes, grants(&server), None);
    assert!(
        report.failure.is_none(),
        "{:?} {:?}",
        report.failure,
        report.budget.map(|budget| budget.lines())
    );
    assert_eq!(report.output.lines, long_stream_lines());
    // The events, then one window of silence before the stall fails.
    let streamed = std::time::Duration::from_millis(EVENT_COUNT * EVENT_PAUSE_MS + WINDOW_MS);
    let elapsed = started.elapsed();
    assert!(elapsed >= streamed, "{elapsed:?}");
    assert!(report.budget.unwrap().deadline_per_turn);
}

/// V38: the network has its own inactivity window
/// (`net::READ_INACTIVITY_SECONDS`), not the per-turn deadline. A stream
/// silent for several per-turn deadlines (a live session while an agent
/// works) keeps its connection and completes, where a read bounded by the
/// turn deadline failed with `terminated` and made clients reconnect.
#[test]
fn v38_network_silence_is_not_bounded_by_the_turn_deadline() {
    let server = Loopback::start();
    let bytes = compile_with(&long_stream(&server), V38);
    let mut run = RunOptions {
        requested: grants(&server),
        ..RunOptions::default()
    };
    run.budgets.deadline_per_turn_ms = Some(WINDOW_MS);
    let report = VartaRuntime::new().run_application(
        &bytes,
        ApplicationOptions {
            run,
            ..ApplicationOptions::default()
        },
    );
    assert!(report.failure.is_none(), "{:?}", report.failure);
    let stalled = crate::net_loopback::event(1, 1).len() + crate::net_loopback::event(2, 1).len();
    assert_eq!(
        report.output.lines.last(),
        Some(&format!(
            "varta log: stall: 200 {stalled} bytes, last id: 2"
        ))
    );
}

/// V38: the head must arrive within one window of the start.
#[test]
fn v38_heads_time_out_after_one_window() {
    let server = Loopback::start();
    let source = program(
        r#"
export async function main(): Promise<void> {
  try {
    await fetch('__BASE__/slow');
  } catch (error) {
    console.log(`${error instanceof TypeError} ${error instanceof Error ? error.message : ''}`);
  }
}
"#,
        &server,
    );
    let bytes = compile_with(&source, V38);
    let started = std::time::Instant::now();
    let report = application(&bytes, grants(&server), None);
    assert!(report.failure.is_none(), "{:?}", report.failure);
    assert_eq!(report.output.lines, lines(&["true fetch failed"]));
    // The window ends the wait, long before the route closes its connection.
    let waited = started.elapsed();
    assert!(
        waited >= std::time::Duration::from_millis(WINDOW_MS),
        "{waited:?}"
    );
    assert!(waited < crate::net_loopback::SILENCE, "{waited:?}");
}

/// V38: the host ceiling still denies before any connection, recoverably.
#[test]
fn v38_long_lived_ceiling_denials_open_no_socket() {
    let server = Loopback::start();
    let source = program(
        r#"
export async function main(): Promise<void> {
  try {
    await fetch('__BASE__/events?count=1&size=1&pause=0');
  } catch (error) {
    console.log(error instanceof DOMException ? error.name : 'other');
  }
}
"#,
        &server,
    );
    let bytes = compile_with(&source, V38);
    for ceiling in [
        vec![],
        vec![requests(&[format!("{}/public/**", server.base)], None)],
    ] {
        let report = application(&bytes, grants(&server), Some(ceiling));
        assert!(report.failure.is_none(), "{:?}", report.failure);
        assert_eq!(
            report.output.lines.last().map(String::as_str),
            Some("varta log: NotAllowedError")
        );
    }
    assert_eq!(server.accepted(), 0);
}

/// The program of the header and encoding tests.
const HOST_HEADERS: &str = r#"
export async function main(): Promise<void> {
  const headers = new Headers();
  headers.set('User-Agent', 'varta-test-sdk/1.0');
  headers.set('Accept-Encoding', 'gzip');
  const sent = await fetch('__BASE__/sent', { headers });
  console.log(await sent.text());
  try {
    const coded = await fetch('__BASE__/coded');
    console.log(`${coded.status} ${await coded.text()}`);
  } catch (error) {
    console.log(`coded: ${error instanceof TypeError ? error.message : 'other'}`);
  }
}
"#;

/// V38 owns `User-Agent` and `Accept-Encoding`: the guest's values are
/// checked, then ignored; the native host asks for the identity coding and
/// fails a response coded otherwise. V37 sends both as given and passes a
/// coded body through unchanged.
#[test]
fn host_owned_request_headers_and_identity_encoding() {
    let server = Loopback::start();
    let source = program(HOST_HEADERS, &server);
    let v38 = VartaRuntime::new()
        .run_wasm_bytes(&compile_with(&source, V38), &grants(&server))
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        v38.lines,
        lines(&[
            r#"{"agent":null,"encoding":"identity"}"#,
            "coded: fetch failed",
        ])
    );
    let v37 = VartaRuntime::new()
        .run_wasm_bytes(&compile(&source), &grants(&server))
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        v37.lines,
        lines(&[
            r#"{"agent":"varta-test-sdk/1.0","encoding":"gzip"}"#,
            "200 coded"
        ])
    );
    // A host-owned header is still checked like any other header first.
    let invalid = program(
        r#"
export async function main(): Promise<void> {
  const headers = new Headers();
  headers.set('User-Agent', 'a\u0007b');
  try {
    await fetch('__BASE__/sent', { headers });
    console.log('sent');
  } catch (error) {
    console.log(error instanceof TypeError ? error.message : 'other');
  }
}
"#,
        &server,
    );
    let accepted = server.accepted();
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compile_with(&invalid, V38), &grants(&server))
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(output.lines, lines(&["fetch failed"]));
    assert_eq!(server.accepted(), accepted);
}

/// V37 keeps one bound across the whole body: the long stream, read in a
/// bounded run without pauses, fails once it passes 512 KiB.
#[test]
fn v37_long_bodies_keep_the_response_bound() {
    run_with_fuel(
        &format!(
            r#"
export async function main(): Promise<void> {{
  const response = await fetch('__BASE__/events?count={EVENT_COUNT}&size={EVENT_BYTES}&pause=0');
  const body = response.body;
  if (body === null) {{
    return;
  }}
  const reader = body.getReader();
  let units = 0;
  try {{
    for (;;) {{
      const {{ done, value }} = await reader.read();
      if (done) {{
        break;
      }}
      units += value.length;
    }}
    console.log(`complete ${{units}}`);
  }} catch (error) {{
    console.log(`${{units > 0 && units <= __LIMIT__}} ${{error instanceof TypeError}} ${{error instanceof Error ? error.message : ''}}`);
  }}
}}
"#
        ),
        &["true true terminated"],
        BODY_LIMIT_FUEL,
    );
}

/// The burst of `net_cases/v38/burst.ts`: back-to-back events far larger
/// than one read's delivery cap (the reviewed reproduction).
const BURST_COUNT: u64 = 12;
const BURST_BYTES: usize = 60_000;
const _: () = assert!(BURST_BYTES > varta_common::net::MAX_READ_DELIVERY_BYTES);

/// V38 under the production budgets: a burst of large events completes in
/// an application instance, because each read (one event turn) delivers at
/// most `MAX_READ_DELIVERY_BYTES` and larger received data waits for the
/// next reads; every network turn stays within the byte path's stated share
/// of the turn fuel. Before the cap, a read coalesced up to
/// `MAX_CHUNK_BYTES` and exhausted `budget::TURN_FUEL`.
#[test]
fn v38_bursts_split_reads_within_the_turn_fuel() {
    use varta_common::budget::TURN_FUEL;
    use varta_common::net::{MAX_READ_DELIVERY_BYTES, READ_FUEL_SHARE};
    let server = Loopback::start();
    let source = program(include_str!("net_cases/v38/burst.ts"), &server)
        .replace("__COUNT__", &BURST_COUNT.to_string())
        .replace("__SIZE__", &BURST_BYTES.to_string())
        .replace("__CAP__", &MAX_READ_DELIVERY_BYTES.to_string());
    let bytes = compile_with(&source, V38);
    let run = RunOptions {
        requested: grants(&server),
        ..RunOptions::default()
    };
    let report = VartaRuntime::new().run_application(
        &bytes,
        ApplicationOptions {
            run,
            ..ApplicationOptions::default()
        },
    );
    let budget = report.budget.expect("an instance ran");
    assert!(
        report.failure.is_none(),
        "{:?} {:?}",
        report.failure,
        budget.lines()
    );
    assert_eq!(budget.budgets.fuel_per_turn, TURN_FUEL);
    let total: usize = (1..=BURST_COUNT)
        .map(|id| crate::net_loopback::event(id, BURST_BYTES).len())
        .sum();
    assert_eq!(
        report.output.lines,
        lines(&[
            &format!("bytes: 200 {total} bytes, last id: {BURST_COUNT}, reads within cap true"),
            &format!("text: 200 {total} units, last id: {BURST_COUNT}"),
        ])
    );
    let network: Vec<u64> = budget
        .turns
        .iter()
        .filter(|turn| turn.kind == crate::turns::TurnKind::Network)
        .map(|turn| turn.fuel)
        .collect();
    assert!(network.len() as u64 > 2 * total as u64 / MAX_READ_DELIVERY_BYTES as u64);
    // A re-measure signal, not a network invariant: the burst's read turns
    // use about 1.6-1.7M of this 2M share, so guest codegen or prelude growth
    // of about 15% on the byte path fails here first; re-measure
    // `READ_BYTE_FUEL` and the cap before changing anything else.
    let share = TURN_FUEL / READ_FUEL_SHARE;
    assert!(network.iter().all(|fuel| *fuel <= share), "{network:?}");
}

/// The program's pause after the abort or failure, with no request in flight.
const PAUSE_MS: u64 = 600;
/// How much of the pause may pass before the server sees the connection
/// close: half of it, so a close deferred to the guest's next wake (pilot
/// finding F2) fails by the other half.
const CLOSE_BOUND: std::time::Duration = std::time::Duration::from_millis(PAUSE_MS / 2);
/// The path of the client's request after its pause: its wake.
const WAKE: &str = "/hello";
/// How long a test waits for a Varta HTTP service to listen.
const LISTEN_READINESS: std::time::Duration = std::time::Duration::from_secs(15);

/// `net_cases/v38/abort_close.ts` against `base`, aborting once `loopback`
/// holds a request for `/held` (its gate answers) and pausing `pause_ms`
/// after the abort.
fn abort_close(base: &str, loopback: &Loopback, pause_ms: u64) -> String {
    use crate::net_loopback::GATE;
    include_str!("net_cases/v38/abort_close.ts")
        .replace(BASE, base)
        .replace("__GATE__", &format!("{}{GATE}", loopback.base))
        .replace("__PAUSE__", &pause_ms.to_string())
}

/// The loopback trace shows that the client closed its `path` connection
/// after the request arrived and before its [`WAKE`] request, sent when its
/// [`PAUSE_MS`] timer fired, with at most [`CLOSE_BOUND`] of the pause passed.
/// The order comes from the trace itself; only the margin is wall time, and
/// load can only widen it (a late wake), except for the server thread's own
/// delay in seeing the end of stream. Returns when the close was seen.
fn assert_closed_before_wake(server: &Loopback, path: &str, label: &str) -> std::time::Instant {
    use crate::net_loopback::Seen;
    let trace = server.trace();
    let position = |seen: Seen| {
        trace
            .iter()
            .position(|(observed, _)| *observed == seen)
            .unwrap_or_else(|| panic!("{label}: no {seen:?} in {trace:?}"))
    };
    let arrived = position(Seen::Arrived(path.into()));
    let closed = position(Seen::Closed(path.into()));
    let woke = position(Seen::Arrived(WAKE.into()));
    assert!(arrived < closed && closed < woke, "{label}: {trace:?}");
    let (closed, woke) = (trace[closed].1, trace[woke].1);
    let before_wake = woke.saturating_duration_since(closed);
    assert!(
        before_wake >= std::time::Duration::from_millis(PAUSE_MS) - CLOSE_BOUND,
        "{label}: closed {before_wake:?} before the guest woke"
    );
    closed
}

/// Pilot finding F2: aborting a request whose head is pending closes its
/// connection at once (the documented cancellation rule), observed as the
/// server's end of stream, long before the guest's next wake, which used to
/// be the first moment the runtime dropped the cancelled transfer. The abort
/// follows the loopback gate's answer, so the server holds the request when
/// it comes (an ordering, not a timer); the close then comes within
/// [`CLOSE_BOUND`] of that answer. Both profiles with `fetch` share the
/// transfer.
#[test]
fn aborted_requests_close_their_connection_before_the_guest_wakes() {
    use crate::net_loopback::{GATE, HELD, Seen};
    for profile in [V37, V38] {
        let server = Loopback::start();
        let source = abort_close(&server.base, &server, PAUSE_MS);
        let output = VartaRuntime::new()
            .run_wasm_bytes(&compile_with(&source, profile), &grants(&server))
            .unwrap_or_else(|e| panic!("{profile:?}: {e:?}"));
        assert_eq!(
            output.lines,
            lines(&["gate: 200 open", "held: AbortError", "hello: 200"])
        );
        let label = format!("{profile:?}");
        let closed = assert_closed_before_wake(&server, HELD, &label);
        let trace = server.trace();
        let (_, opened) = trace
            .iter()
            .find(|(seen, _)| *seen == Seen::Answered(GATE.into()))
            .unwrap_or_else(|| panic!("{label}: the gate never answered: {trace:?}"));
        let after_gate = closed.saturating_duration_since(*opened);
        assert!(after_gate < CLOSE_BOUND, "{label}: {after_gate:?}");
    }
}

/// The inactivity window of [`application`] ends a transfer on its own: a
/// head that never comes (`/held`) or a body that stops after one event
/// (`/trickle`). The failed transfer closes its connection before the
/// failure reaches the guest, so a guest that then waits on a timer alone no
/// longer keeps the socket until it wakes (the terminal-transfer case of
/// pilot finding F2).
#[test]
fn transfers_that_fail_on_their_own_close_their_connection_before_the_guest_wakes() {
    use crate::net_loopback::{HELD, TRICKLE};
    for (path, failure) in [(HELD, "fetch failed"), (TRICKLE, "terminated")] {
        let server = Loopback::start();
        let source = program(include_str!("net_cases/v38/timeout_close.ts"), &server)
            .replace("__PATH__", path)
            .replace("__PAUSE__", &PAUSE_MS.to_string());
        let report = application(&compile_with(&source, V38), grants(&server), None);
        assert!(report.failure.is_none(), "{path}: {:?}", report.failure);
        assert_eq!(
            report.output.lines,
            lines(&[&format!("held: {failure}"), "hello: 200"]),
            "{path}"
        );
        assert_closed_before_wake(&server, path, path);
    }
}

/// How long the test's observer keeps a cancelled worker, and so its worker
/// permit, before the worker ends: a worker that takes a while to stop after
/// its client left.
const DRAIN: std::time::Duration = std::time::Duration::from_millis(300);

/// Pilot finding F2, server side: a v2 HTTP service whose client aborted
/// (closing the connection, so the worker is cancelled) serves the client's
/// next request, sent at once, instead of refusing it with the runtime's own
/// 503: on another worker of its pool while the cancelled one still drains,
/// so the two reports arrive in either order. The service's
/// `/held` holds a request to the loopback server, whose gate lets the client
/// abort only once the worker is busy with it; the cancelled worker then
/// closes that upstream connection as well.
#[test]
fn aborted_requests_leave_the_server_worker_free_for_the_next_request() {
    use crate::net_loopback::HELD;
    use std::sync::{Arc, Mutex, mpsc};
    let upstream = Loopback::start();
    let service = compile_with(
        &include_str!("net_cases/v38/held_service.ts")
            .replace("__HELD__", &format!("{}{HELD}", upstream.base)),
        V38,
    );
    let runtime = VartaRuntime::new();
    let endpoint = runtime
        .inspect_module(&service)
        .unwrap()
        .http_service
        .unwrap()
        .bind;
    let (listening, ready) = mpsc::channel();
    let outcomes = Arc::new(Mutex::new(Vec::new()));
    let recorded = outcomes.clone();
    let observer: RuntimeObserver = Arc::new(move |event| match event {
        RuntimeEvent::Listening { address, shutdown } => {
            let _ = listening.send((address, shutdown));
        }
        RuntimeEvent::HttpRequest(report) => {
            if report.outcome == HttpRequestOutcome::Cancelled {
                std::thread::sleep(DRAIN);
            }
            recorded.lock().unwrap().push(report.outcome);
        }
        RuntimeEvent::Output(_) | RuntimeEvent::Diagnostic(_) => {}
    });
    let options = RunOptions {
        requested: vec![
            CapabilityGrant {
                id: varta_common::http::LISTEN_CAPABILITY.into(),
                value: CapabilityValue::ListenEndpoints(vec![endpoint]),
            },
            requests(&[format!("{}/**", upstream.base)], None),
        ],
        observer: Some(observer),
        ..RunOptions::default()
    };
    let server = std::thread::spawn(move || runtime.run_with(&service, options));
    let (address, shutdown) = ready
        .recv_timeout(LISTEN_READINESS)
        .expect("listener readiness");
    let base = format!("http://{address}");
    let client = compile_with(&abort_close(&base, &upstream, 0), V38);
    let grants = [
        requests(
            &[format!("{base}/**"), format!("{}/**", upstream.base)],
            None,
        ),
        boolean(TIMER_CAPABILITY),
    ];
    let output = VartaRuntime::new().run_wasm_bytes(&client, &grants);
    shutdown.shutdown();
    let report = server.join().unwrap();
    assert!(report.failure.is_none(), "{:?}", report.failure);
    let output = output.unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        output.lines,
        lines(&["gate: 200 open", "held: AbortError", "hello: 200"])
    );
    assert!(
        upstream
            .trace()
            .iter()
            .any(|(seen, _)| *seen == crate::net_loopback::Seen::Closed(HELD.into())),
        "{:?}",
        upstream.trace()
    );
    let mut outcomes = outcomes.lock().unwrap().clone();
    outcomes.sort_by_key(|outcome| matches!(outcome, HttpRequestOutcome::Returned { .. }));
    assert_eq!(
        outcomes,
        [
            HttpRequestOutcome::Cancelled,
            HttpRequestOutcome::Returned { status: 200 }
        ]
    );
}
