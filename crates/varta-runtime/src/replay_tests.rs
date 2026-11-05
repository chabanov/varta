//! Recorded network replay (v38): each log is recorded from a real native
//! run against a loopback server, then replayed with no server; the replay
//! must print the same lines, end with the same view snapshot and charge
//! every turn exactly what the real transfer's turn used.
use crate::net_loopback::{Loopback, TRICKLE};
use crate::{
    ApplicationOptions, NetworkLog, NetworkMode, ReplayError, RunOptions, RuntimeError, TurnKind,
    VartaRuntime, ViewReport,
};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::time::{Duration, Instant};
use varta_common::budget::BudgetRequest;
use varta_common::language::LanguageProfile;
use varta_common::net::replay::{LoggedEvent, LoggedRead};
use varta_common::{CapabilityGrant, CapabilityValue, RequestScope};

/// The view program of every case: `__BODY__` runs from `main` after the
/// view is mounted; `show` appends a line to the view (one commit per turn)
/// and logs it.
const PROGRAM: &str = r#"
const BASE = '__BASE__';
let label = 0;
let shown = '';
function show(text: string): void {
  shown = shown + text + '|';
  View.setText(label, shown);
  View.commit();
  console.log(text);
}
async function stream(path: string, init: RequestInit): Promise<void> {
  const response = await fetch(BASE + path, init);
  const type = response.headers.get('content-type');
  show(`head ${response.status} ${type === null ? 'none' : type}`);
  const body = response.body;
  if (body === null) {
    return;
  }
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let chunks = 0;
  let bytes = 0;
  let tail = '';
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) {
        break;
      }
      chunks += 1;
      bytes += value.length;
      tail = decoder.decode(value).trim().split('\n').join('/');
    }
    show(`end ${bytes} bytes, last ${tail}`);
  } catch (error) {
    show(`read ${error instanceof DOMException ? error.name : error instanceof TypeError ? 'TypeError' : 'other'} after ${bytes} bytes`);
  }
}
__BODY__
export function main(): void {
  const root = View.node();
  View.createElement(root, 1);
  View.setAttribute(root, 1, 'app');
  label = View.node();
  View.createText(label, 'idle');
  View.insert(root, label, 0);
  View.mount(root);
  run();
}
"#;

fn program(base: &str, body: &str) -> Vec<u8> {
    let source = PROGRAM.replace("__BASE__", base).replace("__BODY__", body);
    let bytes = varta_compiler::compile_typescript_with_profile(
        &source,
        LanguageProfile::CheckedF64Utf16V38,
    )
    .unwrap_or_else(|error| panic!("{error}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}

fn network(bases: &[&str], response_headers: &[&str]) -> CapabilityGrant {
    CapabilityGrant {
        id: varta_common::net::REQUEST_CAPABILITY.into(),
        value: CapabilityValue::Requests(RequestScope {
            urls: bases.iter().map(|base| format!("{base}/**")).collect(),
            methods: None,
            response_headers: Some(
                response_headers
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
            ),
        }),
    }
}
fn timers() -> CapabilityGrant {
    CapabilityGrant {
        id: varta_common::scheduler::TIMER_CAPABILITY.into(),
        value: CapabilityValue::Boolean(true),
    }
}

/// One scripted session.
fn session(
    bytes: &[u8],
    requested: &[CapabilityGrant],
    budgets: BudgetRequest,
    network: NetworkMode,
    script: &str,
) -> ViewReport {
    let run = RunOptions {
        requested: requested.to_vec(),
        budgets,
        ..RunOptions::default()
    };
    session_with(bytes, run, network, script)
}
/// One scripted session with explicit run options.
fn session_with(bytes: &[u8], run: RunOptions, network: NetworkMode, script: &str) -> ViewReport {
    VartaRuntime::new().run_view(
        bytes,
        ApplicationOptions {
            run,
            network,
            ..ApplicationOptions::default()
        },
        &crate::ViewScript::parse(script.as_bytes()).unwrap(),
    )
}

/// What each turn used, apart from its wall-clock time.
fn usage(report: &ViewReport) -> Vec<(u64, TurnKind, u64, u32, u64, usize)> {
    report
        .report
        .budget
        .as_ref()
        .unwrap()
        .turns
        .iter()
        .map(|turn| {
            (
                turn.turn,
                turn.kind,
                turn.fuel,
                turn.host_calls,
                turn.microtasks,
                turn.memory_bytes,
            )
        })
        .collect()
}

/// Record a session against real servers, then replay its log with no
/// server: the same lines, snapshot and per-turn usage. Returns the log and
/// the recorded session.
fn record_and_replay(
    bytes: &[u8],
    requested: &[CapabilityGrant],
    budgets: BudgetRequest,
    script: &str,
) -> (Arc<NetworkLog>, ViewReport) {
    let mut recorded = session(bytes, requested, budgets, NetworkMode::Record, script);
    assert!(
        recorded.report.failure.is_none(),
        "{:?}",
        recorded.report.failure
    );
    let log = Arc::new(recorded.report.network_log.take().unwrap());
    // The log survives its wire form unchanged.
    assert_eq!(NetworkLog::parse(&log.to_json()).unwrap(), *log);
    let replayed = session(
        bytes,
        requested,
        budgets,
        NetworkMode::Replay(log.clone()),
        script,
    );
    assert!(
        replayed.report.failure.is_none(),
        "{:?}",
        replayed.report.failure
    );
    assert_eq!(replayed.report.output.lines, recorded.report.output.lines);
    assert_eq!(replayed.snapshot, recorded.snapshot);
    assert_eq!(usage(&replayed), usage(&recorded));
    assert!(replayed.report.network_log.is_none());
    (log, recorded)
}

const WAIT: &str = r#"{"inputs":[{"input":"wait_network","ms":3000}]}"#;

#[test]
fn server_sent_events_replay_with_the_recorded_chunks_and_usage() {
    let server = Loopback::start();
    let bytes = program(
        &server.base,
        "async function run(): Promise<void> { await stream('/sse', {}); }",
    );
    let grants = [network(&[&server.base], &["content-type"])];
    let (log, recorded) = record_and_replay(&bytes, &grants, BudgetRequest::default(), WAIT);
    let lines = &recorded.report.output.lines;
    assert_eq!(lines[0], "varta log: head 200 text/event-stream");
    assert_eq!(
        lines.last().unwrap(),
        "varta log: end 60 bytes, last id: 3/data: tick 3"
    );
    let chunks = log
        .events
        .iter()
        .filter(|event| {
            matches!(
                event,
                LoggedEvent::Chunk {
                    read: LoggedRead::Bytes,
                    ..
                }
            )
        })
        .count();
    assert!(chunks >= 1);
    assert!(matches!(log.events.last(), Some(LoggedEvent::End { .. })));
    assert!(log.events.iter().all(|event| event.step() == 1));
    assert!(
        usage(&recorded)
            .iter()
            .filter(|turn| turn.1 == TurnKind::Network)
            .count()
            >= 3
    );
    // A replay opens no socket: the server saw only the recording.
    let accepted = server.accepted();
    let replayed = session(
        &bytes,
        &grants,
        BudgetRequest::default(),
        NetworkMode::Replay(log),
        WAIT,
    );
    assert!(replayed.report.failure.is_none());
    assert_eq!(server.accepted(), accepted);
}

#[test]
fn error_statuses_and_transport_failures_replay_as_recorded() {
    let server = Loopback::start();
    // A port with no listener: the transfer fails to connect.
    let closed = {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", listener.local_addr().unwrap())
    };
    let body = format!(
        "async function run(): Promise<void> {{
           await stream('/missing', {{}});
           try {{ await fetch('{closed}/gone'); show('connected'); }}
           catch (error) {{ show(`failed ${{error instanceof TypeError ? 'TypeError' : 'other'}}`); }}
         }}"
    );
    let bytes = program(&server.base, &body);
    let grants = [network(&[&server.base, &closed], &["content-type"])];
    let (log, recorded) = record_and_replay(&bytes, &grants, BudgetRequest::default(), WAIT);
    assert_eq!(
        recorded.report.output.lines,
        [
            "varta log: head 404 text/plain",
            "varta log: end 4 bytes, last nope",
            "varta log: failed TypeError",
        ]
    );
    assert!(matches!(
        &log.events[0],
        LoggedEvent::Head { status: 404, status_text, .. } if status_text == "Not Found"
    ));
    assert!(matches!(
        log.events.last(),
        Some(LoggedEvent::Failed { request: 1, text, .. }) if text.starts_with("connect failed")
    ));
}

#[test]
fn aborts_replay_at_their_recorded_place_and_a_diverging_abort_is_fatal() {
    let server = Loopback::start();
    let grants = [network(&[&server.base], &["content-type"])];
    // A request aborted by its controller while its body streams.
    let source = "async function run(): Promise<void> {
        const controller = new AbortController();
        const response = await fetch(BASE + '__PATH__', { signal: controller.signal });
        const body = response.body;
        if (body === null) { return; }
        const reader = body.getReader();
        const first = await reader.read();
        const size = first.done ? 0 : first.value.length;
        controller.abort();
        let then = 'read again';
        try { await reader.read(); }
        catch (error) { then = error instanceof DOMException ? error.name : 'other'; }
        show(`first ${size} bytes, then ${then}`);
      }"
    .replace("__PATH__", TRICKLE);
    let bytes = program(&server.base, &source);
    let (log, recorded) = record_and_replay(&bytes, &grants, BudgetRequest::default(), WAIT);
    assert_eq!(
        recorded.report.output.lines,
        ["varta log: first 15 bytes, then AbortError"]
    );
    assert!(matches!(
        log.events.last(),
        Some(LoggedEvent::Abort { request: 0, .. })
    ));
    // A log that continues where the guest aborts diverges.
    let mut continued = (*log).clone();
    let last = continued.events.len() - 1;
    continued.events[last] = LoggedEvent::Chunk {
        step: 1,
        request: 0,
        read: LoggedRead::Bytes,
        text: "data: 2\n\n".into(),
    };
    let diverged = session(
        &bytes,
        &grants,
        BudgetRequest::default(),
        NetworkMode::Replay(Arc::new(continued)),
        WAIT,
    );
    assert!(matches!(
        diverged.report.failure,
        Some(RuntimeError::Replay(ReplayError::Abort { request: 0 }))
    ));
    assert!(diverged.report.output.lines.is_empty());
}

/// A server that answers its first request 503 with `Retry-After: 1` and
/// every later one 200.
fn flaky() -> (String, std::thread::JoinHandle<()>) {
    const ANSWERS: usize = 2;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = std::thread::spawn(move || {
        for answer in 0..ANSWERS {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                    break;
                }
            }
            let (status, extra, body) = if answer == 0 {
                ("503 Service Unavailable", "retry-after: 1\r\n", "busy")
            } else {
                ("200 OK", "", "ready")
            };
            write!(
                stream,
                "HTTP/1.1 {status}\r\ncontent-type: text/plain\r\n{extra}content-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    (base, task)
}

const BACKOFF: &str = "
async function attempt(n: number): Promise<void> {
  const response = await fetch(BASE + '/flaky');
  const text = await response.text();
  const after = response.headers.get('retry-after');
  if (response.status === 503) {
    const seconds = after === null ? 0 : Number(after);
    show(`attempt ${n}: 503 ${text}, retry after ${seconds} s`);
    setTimeout(() => { attempt(n + 1); }, seconds * 1000);
    return;
  }
  show(`attempt ${n}: ${response.status} ${text}`);
}
async function run(): Promise<void> { await attempt(1); }
";
const BACKOFF_SCRIPT: &str = r#"{"inputs":[{"input":"wait_network","ms":3000},{"input":"wait","ms":1000},{"input":"wait_network","ms":3000}]}"#;

#[test]
fn backoff_with_retry_after_replays_on_virtual_time() {
    let (base, server) = flaky();
    let bytes = program(&base, BACKOFF);
    let grants = [
        network(&[&base], &["content-type", "retry-after"]),
        timers(),
    ];
    let (log, recorded) =
        record_and_replay(&bytes, &grants, BudgetRequest::default(), BACKOFF_SCRIPT);
    server.join().unwrap();
    assert_eq!(
        recorded.report.output.lines,
        [
            "varta log: attempt 1: 503 busy, retry after 1 s",
            "varta log: attempt 2: 200 ready"
        ]
    );
    assert_eq!(log.requests.len(), 2);
    assert!(matches!(
        &log.events[0],
        LoggedEvent::Head { status: 503, headers, .. }
            if headers == &["content-type", "text/plain", "retry-after", "1"]
    ));
    // The first attempt completed in the first step; the retry, started by
    // the timer of the second step, completed in the second step or (when
    // its real transfer took longer) in the third.
    assert!(log.events[..3].iter().all(|event| event.step() == 1));
    assert!((2..=3).contains(&log.events.last().unwrap().step()));
    assert!(
        usage(&recorded)
            .iter()
            .any(|turn| turn.1 == TurnKind::Timer)
    );
    // A grant that hides `retry-after` hides it from the replay as well: the
    // guest retries at once, and the log cannot serve that retry then.
    let hidden = [network(&[&base], &["content-type"]), timers()];
    let replayed = session(
        &bytes,
        &hidden,
        BudgetRequest::default(),
        NetworkMode::Replay(log),
        BACKOFF_SCRIPT,
    );
    assert_eq!(
        replayed.report.output.lines,
        ["varta log: attempt 1: 503 busy, retry after 0 s"]
    );
    assert!(matches!(
        replayed.report.failure,
        Some(RuntimeError::Replay(ReplayError::Stalled { .. }))
    ));
}

#[test]
fn a_stream_longer_than_the_per_turn_deadline_replays_without_waiting() {
    // Test-only budget: a short per-turn deadline (the inactivity window).
    const WINDOW_MS: u64 = 200;
    const COUNT: u64 = 12;
    const SIZE: u64 = 64;
    const PAUSE_MS: u64 = 40;
    const { assert!(COUNT * PAUSE_MS > 2 * WINDOW_MS && PAUSE_MS < WINDOW_MS) };
    let server = Loopback::start();
    let path = format!("/events?count={COUNT}&size={SIZE}&pause={PAUSE_MS}");
    let bytes = program(
        &server.base,
        &format!("async function run(): Promise<void> {{ await stream('{path}', {{}}); }}"),
    );
    let grants = [network(&[&server.base], &["content-type"])];
    let budgets = BudgetRequest {
        deadline_per_turn_ms: Some(WINDOW_MS),
        ..BudgetRequest::default()
    };
    let started = Instant::now();
    let (log, recorded) = record_and_replay(&bytes, &grants, budgets, WAIT);
    let both = started.elapsed();
    assert!(both >= Duration::from_millis(COUNT * PAUSE_MS));
    assert_eq!(
        recorded.report.output.lines.last().unwrap(),
        &format!(
            "varta log: end {} bytes, last data: {}/id: {COUNT}",
            (1..=COUNT)
                .map(|id| crate::net_loopback::event(id, usize::try_from(SIZE).unwrap()).len())
                .sum::<usize>(),
            "x".repeat(usize::try_from(SIZE).unwrap())
        )
    );
    // The replay served every chunk of a stream that outlived several
    // windows, opening no socket and waiting for no window.
    let accepted = server.accepted();
    let replayed = session(&bytes, &grants, budgets, NetworkMode::Replay(log), WAIT);
    assert!(replayed.report.failure.is_none());
    assert_eq!(replayed.report.output.lines, recorded.report.output.lines);
    assert_eq!(server.accepted(), accepted);
}

#[test]
fn an_ungranted_logged_request_is_refused_before_the_instance_starts() {
    let server = Loopback::start();
    let bytes = program(
        &server.base,
        "async function run(): Promise<void> { await stream('/hello', {}); }",
    );
    let grants = [network(&[&server.base], &["content-type"])];
    let (log, _) = record_and_replay(&bytes, &grants, BudgetRequest::default(), WAIT);
    let accepted = server.accepted();
    let other = network(&["http://127.0.0.1:1"], &["content-type"]);
    let refused = session(
        &bytes,
        &[other],
        BudgetRequest::default(),
        NetworkMode::Replay(log.clone()),
        WAIT,
    );
    assert!(matches!(
        &refused.report.failure,
        Some(RuntimeError::Replay(ReplayError::Ungranted { request: 0, method, url }))
            if method == "GET" && url == &format!("{}/hello", server.base)
    ));
    // Before any effect: no guest output, no turn, no socket.
    assert!(refused.report.output.lines.is_empty());
    assert!(refused.report.budget.is_none());
    assert!(refused.snapshot.is_none());
    assert_eq!(server.accepted(), accepted);
    // A granted log for another program diverges at its first request.
    let other_program = program(
        &server.base,
        "async function run(): Promise<void> { await stream('/json', {}); }",
    );
    let diverged = session(
        &other_program,
        &grants,
        BudgetRequest::default(),
        NetworkMode::Replay(log),
        WAIT,
    );
    assert!(matches!(
        diverged.report.failure,
        Some(RuntimeError::Replay(ReplayError::Request {
            request: 0,
            ..
        }))
    ));
}

#[test]
fn undelivered_logged_events_fail_the_replay() {
    let server = Loopback::start();
    let bytes = program(
        &server.base,
        "async function run(): Promise<void> { await stream('/hello', {}); }",
    );
    let grants = [network(&[&server.base], &["content-type"])];
    let (log, _) = record_and_replay(&bytes, &grants, BudgetRequest::default(), WAIT);
    let mut longer = (*log).clone();
    longer.requests.push(longer.requests[0].clone());
    longer.events.push(LoggedEvent::Failed {
        step: 1,
        request: 1,
        text: "request timed out".into(),
    });
    let replayed = session(
        &bytes,
        &grants,
        BudgetRequest::default(),
        NetworkMode::Replay(Arc::new(longer)),
        WAIT,
    );
    assert!(matches!(
        replayed.report.failure,
        Some(RuntimeError::Replay(ReplayError::Unconsumed {
            remaining: 1
        }))
    ));
}

#[test]
fn recordings_stop_at_their_event_limit() {
    let mut recorder = crate::replay::Recorder::new();
    let request = recorder.request("GET", "http://127.0.0.1:1/a").unwrap();
    let chunk = |text: &str| LoggedEvent::Chunk {
        step: 0,
        request,
        read: LoggedRead::Text,
        text: text.into(),
    };
    for _ in 0..varta_common::net::replay::MAX_LOG_EVENTS {
        recorder.event(chunk("x")).unwrap();
    }
    assert_eq!(recorder.event(chunk("x")), Err(ReplayError::RecordingLimit));
    assert_eq!(
        crate::replay::Recorder::new().event(chunk("")),
        Err(ReplayError::Unrecordable { request })
    );
}

#[test]
fn a_later_request_aborted_in_its_start_turn_keeps_its_logged_place() {
    // A and B start in one turn and B is aborted at once: B claims logged
    // request 1 when it starts, so its abort is the log's first event while
    // A's completions follow. The browser host replays this log
    // (network-replay.test.mjs, scripts/test-browser-replay.mjs).
    let server = Loopback::start();
    let bytes = program(
        &server.base,
        "async function run(): Promise<void> {
           const early = new AbortController();
           const first = fetch(BASE + '/hello');
           const second = fetch(BASE + '/json', { signal: early.signal });
           early.abort();
           let outcome = 'resolved';
           try { await second; } catch (error) { outcome = error instanceof DOMException ? error.name : 'other'; }
           const response = await first;
           const text = await response.text();
           show(`second ${outcome}, first ${response.status} ${text.trim()}`);
         }",
    );
    let grants = [network(&[&server.base], &["content-type"])];
    let (log, recorded) = record_and_replay(&bytes, &grants, BudgetRequest::default(), WAIT);
    assert!(
        recorded.report.output.lines[0].starts_with("varta log: second AbortError, first 200 "),
        "{:?}",
        recorded.report.output.lines
    );
    assert_eq!(
        log.requests
            .iter()
            .map(|request| request.url.clone())
            .collect::<Vec<_>>(),
        [
            format!("{}/hello", server.base),
            format!("{}/json", server.base)
        ]
    );
    assert!(matches!(
        log.events[0],
        LoggedEvent::Abort { request: 1, .. }
    ));
    assert!(matches!(
        log.events[1],
        LoggedEvent::Head { request: 0, .. }
    ));
    // Swapping the two logged requests diverges at A's start.
    let mut swapped = (*log).clone();
    swapped.requests.swap(0, 1);
    let diverged = session(
        &bytes,
        &grants,
        BudgetRequest::default(),
        NetworkMode::Replay(Arc::new(swapped)),
        WAIT,
    );
    assert!(matches!(
        diverged.report.failure,
        Some(RuntimeError::Replay(ReplayError::Request {
            request: 0,
            ..
        }))
    ));
}

#[test]
fn net_get_and_net_fetch_modules_are_refused_before_recording_or_replay() {
    // They reach the network outside the request transport: a recording
    // would miss them and a replay would make them for real.
    let server = Loopback::start();
    let empty = Arc::new(NetworkLog::default());
    for (call, operation) in [
        ("get", varta_common::HostOperation::NetGet16),
        ("fetch", varta_common::HostOperation::NetFetch16),
    ] {
        let bytes = program(
            &server.base,
            &format!(
                "async function run(): Promise<void> {{ console.log(String(varta.net.{call}(BASE + '/hello'))); }}"
            ),
        );
        let grants = [CapabilityGrant {
            id: format!("net.{call}"),
            value: CapabilityValue::UrlGlobs(vec![format!("{}/**", server.base)]),
        }];
        // Live, the import reaches the server.
        let live = session(
            &bytes,
            &grants,
            BudgetRequest::default(),
            NetworkMode::Live,
            WAIT,
        );
        assert!(live.report.failure.is_none(), "{:?}", live.report.failure);
        assert_eq!(live.report.output.lines.len(), 1);
        let accepted = server.accepted();
        for mode in [NetworkMode::Record, NetworkMode::Replay(empty.clone())] {
            let refused = session(&bytes, &grants, BudgetRequest::default(), mode, WAIT);
            assert!(
                matches!(
                    &refused.report.failure,
                    Some(RuntimeError::Replay(ReplayError::Unserved { import }))
                        if import == operation.name()
                ),
                "{:?}",
                refused.report.failure
            );
            // Before any effect: no output, no turn, no log, no socket.
            assert!(refused.report.output.lines.is_empty());
            assert!(refused.report.budget.is_none());
            assert!(refused.report.network_log.is_none());
            assert_eq!(server.accepted(), accepted);
        }
    }
}

#[test]
fn a_request_the_broker_denies_at_start_claims_no_logged_request() {
    use varta_common::permission::PermissionState;
    // The first request is outside the grant: the broker denies it at its
    // start, audits it and completes it at once, while recording and while
    // replaying. It is never logged and claims nothing, so the granted
    // request after it claims logged request 0.
    const UNGRANTED: &str = "http://127.0.0.1:1/x";
    let server = Loopback::start();
    let bytes = program(
        &server.base,
        &format!(
            "async function run(): Promise<void> {{
               let first = 'fetched';
               try {{ await fetch('{UNGRANTED}'); }}
               catch (error) {{ first = error instanceof DOMException ? error.name : 'other'; }}
               const response = await fetch(BASE + '/hello');
               show(`${{first}}, then ${{response.status}}`);
             }}"
        ),
    );
    let grants = [network(&[&server.base], &["content-type"])];
    let (log, recorded) = record_and_replay(&bytes, &grants, BudgetRequest::default(), WAIT);
    assert_eq!(
        recorded.report.output.lines.last().unwrap(),
        "varta log: NotAllowedError, then 200"
    );
    assert_eq!(log.requests.len(), 1);
    assert_eq!(log.requests[0].url, format!("{}/hello", server.base));
    let replayed = session(
        &bytes,
        &grants,
        BudgetRequest::default(),
        NetworkMode::Replay(log),
        WAIT,
    );
    assert!(
        replayed.report.failure.is_none(),
        "{:?}",
        replayed.report.failure
    );
    let denied = format!("GET {UNGRANTED}");
    assert!(
        replayed.report.audit.iter().any(|entry| entry.operation
            == varta_common::net::REQUEST_CAPABILITY
            && entry.resource == denied
            && entry.outcome == PermissionState::Denied),
        "{:?}",
        replayed.report.audit
    );
}
