//! V38 completion ordering and the request checks of the event loop: network
//! completions take their turn by arrival among host tasks and due timers,
//! the pending-request limit fails before the permission broker decides, and
//! a timer due beyond the deadline of a bounded run reports `TimerDeadline`
//! while a request is in flight. Real clock against a loopback server.
use super::*;
use crate::net_loopback::Loopback;
use std::time::Duration;
use varta_common::CapabilityValue;
use varta_common::language::LanguageProfile;
use varta_common::net::{MAX_PENDING_REQUESTS, NetError, REQUEST_CAPABILITY};
use varta_common::scheduler::{SchedulerError, TIMER_CAPABILITY};

const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;
const V37: LanguageProfile = LanguageProfile::CheckedF64Utf16V37;

fn compile_with(source: &str, profile: LanguageProfile) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}
fn grants(server: &Loopback) -> Vec<CapabilityGrant> {
    vec![
        CapabilityGrant {
            id: REQUEST_CAPABILITY.into(),
            value: CapabilityValue::Requests(varta_common::RequestScope {
                urls: vec![format!("{}/**", server.base)],
                methods: None,
            }),
        },
        CapabilityGrant {
            id: TIMER_CAPABILITY.into(),
            value: CapabilityValue::Boolean(true),
        },
    ]
}
fn event(record: &str) -> HostEvent {
    HostEvent {
        source: "ui".into(),
        record: record.into(),
        gesture: false,
        coalesce: false,
    }
}
fn application(bytes: &[u8], requested: Vec<CapabilityGrant>) -> Application {
    VartaRuntime::new()
        .start_application(
            bytes,
            ApplicationOptions {
                run: RunOptions {
                    requested,
                    ..RunOptions::default()
                },
                ..ApplicationOptions::default()
            },
        )
        .unwrap()
}
/// Turns a sustained stream may run before a completion that arrived early
/// in it (the response needs a few event-loop passes to arrive).
const PATIENCE: usize = 60;

/// A host event is always queued, so a host task is ready at every step:
/// the completion still takes its turn once it arrives.
#[test]
fn a_host_event_stream_does_not_starve_a_completion() {
    let server = Loopback::start();
    let source = r#"
let events = 0;
async function load(): Promise<void> {
  const response = await fetch('__BASE__/hello');
  console.log(`completion ${response.status} after ${events}`);
}
export function main(): void {
  load();
  const id: number = varta.events.listen('ui', (record: string) => {
    events += 1;
    if (record === 'stop') { varta.events.close(id); }
  });
}
"#
    .replace("__BASE__", &server.base);
    let mut app = application(&compile_with(&source, V38), grants(&server));
    app.deliver(event("e")).unwrap();
    for _ in 0..PATIENCE * 4 {
        app.deliver(event("e")).unwrap();
        assert!(matches!(app.step(Some(Duration::ZERO)), Step::Turn(_)));
        std::thread::sleep(Duration::from_millis(1));
    }
    let lines = app.take_output();
    let completion = lines
        .iter()
        .find_map(|line| line.strip_prefix("varta log: completion 200 after "))
        .unwrap_or_else(|| panic!("the completion was starved: {lines:?}"));
    let after: usize = completion.parse().unwrap();
    assert!(after < PATIENCE * 4, "{after}");
    app.deliver(event("stop")).unwrap();
    assert_eq!(app.run(), Step::Finished { exit_code: 0 });
}

/// A zero-delay timer chain keeps a timer due at every step: the completion
/// still takes its turn once it arrives.
#[test]
fn a_timer_stream_does_not_starve_a_completion() {
    let server = Loopback::start();
    let source = r#"
let ticks = 0;
let done = false;
async function load(): Promise<void> {
  const response = await fetch('__BASE__/hello');
  done = true;
  console.log(`completion ${response.status} after ${ticks}`);
}
function tick(): void {
  ticks += 1;
  if (!done && ticks < 100000) { setTimeout(tick, 0); }
}
export function main(): void {
  load();
  setTimeout(tick, 0);
}
"#
    .replace("__BASE__", &server.base);
    let mut app = application(&compile_with(&source, V38), grants(&server));
    assert_eq!(app.run(), Step::Finished { exit_code: 0 });
    let lines = app.take_output();
    let completion = lines
        .iter()
        .find_map(|line| line.strip_prefix("varta log: completion 200 after "))
        .unwrap_or_else(|| panic!("no completion: {lines:?}"));
    let after: u64 = completion.parse().unwrap();
    assert!(after < 100_000, "the timer chain ran to its end first");
}

/// The ninth concurrent request fails on the pending-request limit before
/// the broker decides it: no audit entry, decision or prompt for it (v38);
/// v37 keeps its order and audits the request first.
#[test]
fn the_pending_request_limit_fails_before_the_broker() {
    let server = Loopback::start();
    let source = format!(
        r#"
export function main(): void {{
  for (let i = 0; i < {MAX_PENDING_REQUESTS}; i++) {{ void fetch('__BASE__/slow'); }}
  void fetch('https://denied.example/x');
}}
"#
    )
    .replace("__BASE__", &server.base);
    let denied = |report: &RunReport| {
        report
            .audit
            .iter()
            .filter(|entry| entry.resource.contains("denied.example"))
            .count()
    };
    let run = |profile| {
        VartaRuntime::new().run_with(
            &compile_with(&source, profile),
            RunOptions {
                requested: grants(&server),
                ..RunOptions::default()
            },
        )
    };
    let report = run(V38);
    assert!(
        matches!(report.failure, Some(RuntimeError::Net(NetError::Budget))),
        "{:?}",
        report.failure
    );
    assert_eq!(denied(&report), 0, "{:?}", report.audit);
    assert!(report.decisions.is_empty());
    let report = run(V37);
    assert!(matches!(
        report.failure,
        Some(RuntimeError::Net(NetError::Budget))
    ));
    assert_eq!(denied(&report), 1, "{:?}", report.audit);
}

/// A timer due beyond the execution deadline while a request is in flight
/// ends a bounded v38 run with `TimerDeadline` at the deadline, as without a
/// request; v37 keeps the deadline error.
#[test]
fn a_timer_beyond_the_deadline_reports_timer_deadline_while_a_request_is_in_flight() {
    let server = Loopback::start();
    let source = r#"
export async function main(): Promise<void> {
  setTimeout(() => { console.log('late'); }, 60000);
  await fetch('__BASE__/slow');
}
"#
    .replace("__BASE__", &server.base);
    let run = |profile| {
        VartaRuntime::new()
            .run_wasm_bytes(&compile_with(&source, profile), &grants(&server))
            .unwrap_err()
    };
    let error = run(V38);
    assert!(
        matches!(
            error,
            RuntimeError::Scheduler(SchedulerError::TimerDeadline { due_ms: 60000, .. })
        ),
        "{error:?}"
    );
    let error = run(V37);
    assert!(
        !matches!(
            error,
            RuntimeError::Scheduler(SchedulerError::TimerDeadline { .. })
        ),
        "{error:?}"
    );
    assert!(error.to_string().contains("deadline"), "{error}");
}
