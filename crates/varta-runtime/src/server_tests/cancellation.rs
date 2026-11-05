//! Real request ownership: closed clients release outbound transfers and a
//! shutdown drains exactly the in-flight reply under its original bounds.
use super::*;
use std::io::{BufRead, BufReader};
use varta_common::{RequestScope, language::LanguageProfile as P};

struct Upstream {
    base: String,
    entered: mpsc::Receiver<()>,
    release: mpsc::Sender<Option<Vec<u8>>>,
    closed: mpsc::Receiver<Result<usize, String>>,
    task: thread::JoinHandle<()>,
}
impl Upstream {
    fn start() -> Self {
        Self::start_mode(false)
    }
    fn start_mode(wait_for_body: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let (started, entered) = mpsc::channel();
        let (release, ready) = mpsc::channel::<Option<Vec<u8>>>();
        let (closed, closure) = mpsc::channel();
        let task = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                if line == "\r\n" {
                    break;
                }
            }
            if wait_for_body {
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\nConnection: close\r\n\r\n")
                    .unwrap();
            }
            started.send(()).unwrap();
            if let Some(body) = ready.recv_timeout(Duration::from_secs(2)).unwrap() {
                if !wait_for_body {
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .unwrap();
                }
                stream.write_all(&body).unwrap();
            }
            let mut byte = [0];
            closed
                .send(stream.read(&mut byte).map_err(|e| e.to_string()))
                .unwrap();
        });
        Self {
            base,
            entered,
            release,
            closed: closure,
            task,
        }
    }
    fn grant(&self) -> CapabilityGrant {
        CapabilityGrant {
            id: "net.request".into(),
            value: CapabilityValue::Requests(RequestScope {
                urls: vec![format!("{}/**", self.base)],
                methods: None,
                response_headers: None,
            }),
        }
    }
    fn finish(self) {
        assert_eq!(
            self.closed
                .recv_timeout(Duration::from_secs(1))
                .expect("outbound socket stayed alive")
                .unwrap(),
            0
        );
        self.task.join().unwrap();
    }
}

/// How soon a cancelled worker must release the one worker permit after its
/// client disconnected.
const WORKER_RELEASE: Duration = Duration::from_secs(1);

/// Request `target` right after a client disconnected. Every service here
/// runs HTTP contract v2 (`v38`), whose transport accepts the next connection
/// only once the cancelled worker released the one worker permit, so the
/// request is served and never refused with 503 (contract v1 is frozen with
/// its immediate 503 while busy, and its workers are not cancellable). The
/// answer still comes within [`WORKER_RELEASE`]: the worker stops promptly.
fn request_after_disconnect(server: &Server, target: &str) {
    let started = std::time::Instant::now();
    assert_eq!(server.request("GET", target, b"").0, 200);
    let waited = started.elapsed();
    assert!(
        waited < WORKER_RELEASE,
        "worker kept the permit for {waited:?}"
    );
}

fn synchronous_source(base: &str, operation: &str) -> String {
    r#"
        function handle(r:VartaHttpRequest):VartaHttpResponse{
            if(r.target=="/slow"){
                try{
                    const result=varta.result.capture(varta.net.__OP__("__BASE__/slow"));
                    console.log("continued after call");
                    return {status:200,body:"completed",headers:[]};
                }catch(error){console.log("caught cancellation");return {status:200,body:"caught",headers:[]};}
            }
            return {status:200,body:"ok",headers:[]};
        }
        export function main():void{}
        varta.http.serve({hostname:"127.0.0.1",port:0},handle);
    "#.replace("__BASE__",base).replace("__OP__",operation)
}
fn synchronous_grant(base: &str, operation: &str) -> CapabilityGrant {
    CapabilityGrant {
        id: format!("net.{operation}"),
        value: CapabilityValue::UrlGlobs(vec![format!("{base}/**")]),
    }
}

#[test]
fn http_v2_disconnect_cancels_synchronous_head_and_body_waits_without_catch_or_capture() {
    for (operation, body) in [("get", false), ("fetch", false), ("get", true)] {
        let upstream = Upstream::start_mode(body);
        let server = Server::start_with_grants(
            &synchronous_source(&upstream.base, operation),
            P::CheckedF64Utf16V38,
            vec![synchronous_grant(&upstream.base, operation)],
        );
        let client = begin(&server);
        upstream
            .entered
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        upstream.release.send(None).unwrap();
        client.shutdown(std::net::Shutdown::Both).unwrap();
        drop(client);
        upstream.finish();
        request_after_disconnect(&server, "/after");
        assert!(
            server.lines.lock().unwrap().is_empty(),
            "guest continued after cancelled {operation}"
        );
        assert_eq!(
            server.requests.lock().unwrap()[0].outcome,
            HttpRequestOutcome::Cancelled
        );
        server.finish();
    }
}

#[test]
fn http_v2_shutdown_keeps_synchronous_network_deadlines_fatal() {
    for operation in ["get", "fetch"] {
        let upstream = Upstream::start();
        let bytes = varta_compiler::compile_typescript_with_profile(
            &synchronous_source(&upstream.base, operation),
            P::CheckedF64Utf16V38,
        )
        .unwrap();
        let server = Server::start_options(
            bytes,
            RunOptions {
                requested: vec![synchronous_grant(&upstream.base, operation)],
                budgets: varta_common::budget::BudgetRequest {
                    deadline_per_turn_ms: Some(200),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        let mut client = begin(&server);
        upstream
            .entered
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        upstream.release.send(None).unwrap();
        server.stop.shutdown();
        let at = std::time::Instant::now();
        let mut reply = String::new();
        client.read_to_string(&mut reply).unwrap();
        assert!(reply.starts_with("HTTP/1.1 500"), "{reply}");
        upstream.finish();
        assert!(at.elapsed() < Duration::from_secs(1));
        assert!(server.lines.lock().unwrap().is_empty());
        assert_eq!(
            server.requests.lock().unwrap()[0].outcome,
            HttpRequestOutcome::Failed
        );
        assert!(
            server
                .diagnostics
                .lock()
                .unwrap()
                .iter()
                .any(|line| line.contains("deadline"))
        );
        server.finish();
    }
}

#[test]
fn http_v2_synchronous_network_results_and_denials_keep_their_existing_contract() {
    let upstream = crate::net_loopback::Loopback::start();
    let base = upstream.base.replace("127.0.0.1", "localhost");
    let source=r#"
        function handle(r:VartaHttpRequest):VartaHttpResponse{
            const body=varta.result.capture(varta.net.get("__BASE__/hello"));
            const status=varta.result.capture(varta.net.fetch("__BASE__/missing"));
            const denied=varta.result.capture(varta.net.get("__BASE__/private"));
            return {status:200,body:JSON.stringify({text:body.value,code:status.value,denied:denied.status}),headers:[]};
        }
        export function main():void{}
        varta.http.serve({hostname:"127.0.0.1",port:0},handle);
    "#.replace("__BASE__",&base);
    for (host_ceiling, expected_connections) in [(false, 2), (true, 0)] {
        let grants = vec![
            CapabilityGrant {
                id: "net.get".into(),
                value: CapabilityValue::UrlGlobs(vec![format!("{base}/hello")]),
            },
            CapabilityGrant {
                id: "net.fetch".into(),
                value: CapabilityValue::UrlGlobs(vec![format!("{base}/missing")]),
            },
        ];
        let bytes = varta_compiler::compile_typescript_with_profile(&source, P::CheckedF64Utf16V38)
            .unwrap();
        let before = upstream.accepted();
        let server = Server::start_options(
            bytes,
            RunOptions {
                requested: grants,
                ceiling: host_ceiling.then(|| vec![grant("127.0.0.1:0".into())]),
                ..Default::default()
            },
        );
        let (status, _, body) = server.request("GET", "/", b"");
        assert_eq!(status, 200, "{body}");
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            value["denied"],
            varta_common::abi::HostStatus::Denied as i32
        );
        if host_ceiling {
            assert_eq!(value["text"], "");
            assert_eq!(value["code"], -1);
        } else {
            assert_eq!(value["text"], "hello world");
            assert_eq!(value["code"], 404);
        }
        assert_eq!(upstream.accepted() - before, expected_connections);
        server.finish();
    }
}

#[test]
fn http_v2_synchronous_status_fetch_does_not_wait_for_the_response_body() {
    let upstream = Upstream::start_mode(true);
    let server = Server::start_with_grants(
        &synchronous_source(&upstream.base, "fetch"),
        P::CheckedF64Utf16V38,
        vec![synchronous_grant(&upstream.base, "fetch")],
    );
    let mut client = begin(&server);
    upstream
        .entered
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    upstream.release.send(None).unwrap();
    let at = std::time::Instant::now();
    let mut reply = String::new();
    client.read_to_string(&mut reply).unwrap();
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert!(reply.ends_with("completed"), "{reply}");
    upstream.finish();
    assert!(at.elapsed() < Duration::from_secs(1));
    assert_eq!(
        server.requests.lock().unwrap()[0].outcome,
        HttpRequestOutcome::Returned { status: 200 }
    );
    server.finish();
}
fn source(base: &str) -> String {
    r#"
        async function handle(r:VartaHttpRequest):Promise<VartaHttpResponse>{
            console.log("handle "+r.target);
            if(r.target=="/slow"){const upstream=await fetch("__BASE__/slow");return {status:200,body:await upstream.text(),headers:[]};}
            return {status:200,body:"ok",headers:[]};
        }
        export function main():void{}
        varta.http.serve({hostname:"127.0.0.1",port:0},handle);
    "#.replace("__BASE__",base)
}
fn begin(server: &Server) -> TcpStream {
    let mut client = TcpStream::connect(server.addr).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    client
        .write_all(b"GET /slow HTTP/1.1\r\nHost: test\r\nContent-Length: 0\r\n\r\n")
        .unwrap();
    client
}

#[test]
fn http_v2_disconnect_cancels_outbound_work_and_releases_the_worker() {
    let upstream = Upstream::start();
    let server = Server::start_with_grants(
        &source(&upstream.base),
        P::CheckedF64Utf16V38,
        vec![upstream.grant()],
    );
    let client = begin(&server);
    upstream
        .entered
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    upstream.release.send(None).unwrap();
    client.shutdown(std::net::Shutdown::Both).unwrap();
    drop(client);
    upstream.finish();
    request_after_disconnect(&server, "/after");
    assert!(
        server
            .diagnostics
            .lock()
            .unwrap()
            .iter()
            .any(|line| line.contains("cancelled"))
    );
    let reports = server.report.clone();
    let requests = server.requests.clone();
    server.finish();
    assert_eq!(
        requests.lock().unwrap()[0].outcome,
        HttpRequestOutcome::Cancelled
    );
    let reports = reports.lock().unwrap();
    let report = reports.as_ref().unwrap();
    assert_eq!(
        (report.total_requests, report.cancelled, report.returned),
        (2, 1, 1)
    );
}

/// Isolate descriptor accounting from other parallel runtime tests. This is
/// an OS-resource sample, not a claim about all native allocator/RSS behavior.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn sustained_disconnects_release_sockets_and_keep_request_memory_bounded() {
    const CHILD: &str = "VARTA_HTTP_LIFECYCLE_SAMPLE_CHILD";
    if std::env::var_os(CHILD).as_deref() != Some(std::ffi::OsStr::new("1")) {
        let output=std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact","server_tests::cancellation::sustained_disconnects_release_sockets_and_keep_request_memory_bounded","--nocapture","--test-threads=1"])
            .env(CHILD,"1").output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "{stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(stdout.contains("HTTP lifecycle sample:"), "{stdout}");
        print!("{stdout}");
        return;
    }
    const WARMUP: usize = 8;
    const SAMPLES: usize = 40;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (started, entered) = mpsc::channel();
    let (ended, closed) = mpsc::channel();
    let upstream = thread::spawn(move || {
        for _ in 0..WARMUP + SAMPLES {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                if line == "\r\n" {
                    break;
                }
            }
            started.send(()).unwrap();
            let mut byte = [0];
            let eof = stream.read(&mut byte);
            drop(reader);
            drop(stream);
            ended.send(eof).unwrap();
        }
    });
    let program = r#"
      async function handle(r:VartaHttpRequest):Promise<VartaHttpResponse>{
        if(r.target=="/slow"){
          const retained="x".repeat(100000);
          const response=await fetch("__BASE__/held");
          const text=await response.text();
          return {status:200,body:retained+text,headers:[]};
        }
        return {status:200,body:"ok",headers:[]};
      }
      export function main():void{}
      varta.http.serve({hostname:"127.0.0.1",port:0},handle);
    "#
    .replace("__BASE__", &base);
    let server = Server::start_with_grants(
        &program,
        P::CheckedF64Utf16V38,
        vec![CapabilityGrant {
            id: "net.request".into(),
            value: CapabilityValue::Requests(RequestScope {
                urls: vec![format!("{base}/**")],
                methods: None,
                response_headers: None,
            }),
        }],
    );
    let health = || request_after_disconnect(&server, "/health");
    let descriptors = || std::fs::read_dir("/dev/fd").unwrap().count();
    let mut baseline = 0;
    for index in 0..WARMUP + SAMPLES {
        let client = begin(&server);
        entered.recv_timeout(Duration::from_secs(2)).unwrap();
        client.shutdown(std::net::Shutdown::Both).unwrap();
        drop(client);
        assert_eq!(
            closed
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap(),
            0
        );
        let until = std::time::Instant::now() + Duration::from_secs(1);
        loop {
            let complete = server
                .requests
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r.outcome == HttpRequestOutcome::Cancelled)
                .count();
            if complete == index + 1 {
                break;
            }
            assert!(
                std::time::Instant::now() < until,
                "request Store was retained"
            );
            thread::sleep(Duration::from_millis(1));
        }
        // A request report proves that the Store was dropped, but the blocking
        // worker still holds its admission permit until the observer returns.
        // The v2 transport accepts the health request only after that, so a
        // completed health response crosses the boundary before the next slow
        // request.
        health();
        if index + 1 == WARMUP {
            baseline = descriptors();
        }
    }
    upstream.join().unwrap();
    // The upstream listener is now closed, so omit it from the warmed baseline.
    baseline = baseline.saturating_sub(1);
    health();
    let after = descriptors();
    assert!(
        after <= baseline + 2,
        "native descriptors grew: {baseline} -> {after}"
    );
    let events = server.requests.clone();
    let aggregate = server.report.clone();
    server.finish();
    let events = events.lock().unwrap();
    let peaks: Vec<_> = events
        .iter()
        .filter(|r| r.outcome == HttpRequestOutcome::Cancelled)
        .map(|r| {
            let budget = r.budget.as_ref().unwrap();
            assert!(budget.peak_memory_bytes <= budget.budgets.memory_bytes);
            budget.peak_memory_bytes
        })
        .collect();
    assert_eq!(peaks.len(), WARMUP + SAMPLES);
    assert!(peaks.iter().all(|peak| *peak == peaks[0]));
    let aggregate = aggregate.lock().unwrap();
    let report = aggregate.as_ref().unwrap();
    assert_eq!(report.cancelled, (WARMUP + SAMPLES) as u64);
    assert_eq!(report.returned, (WARMUP + SAMPLES + 1) as u64);
    println!(
        "HTTP lifecycle sample: {SAMPLES} cancellations after {WARMUP} warmups; descriptors {baseline} -> {after}; guest peak {} bytes",
        peaks[0]
    );
}

#[test]
fn http_v2_outbound_hostname_uses_system_dns_and_returns_the_response() {
    let mut upstream = Upstream::start();
    upstream.base = upstream.base.replace("127.0.0.1", "localhost");
    let server = Server::start_with_grants(
        &source(&upstream.base),
        P::CheckedF64Utf16V38,
        vec![upstream.grant()],
    );
    let mut client = begin(&server);
    upstream
        .entered
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    upstream.release.send(Some(b"resolved".to_vec())).unwrap();
    let mut reply = String::new();
    client.read_to_string(&mut reply).unwrap();
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert!(reply.ends_with("resolved"), "{reply}");
    upstream.finish();
    server.finish();
}

#[test]
fn http_v2_shutdown_drains_active_response_without_accepting_another_request() {
    let upstream = Upstream::start();
    let server = Server::start_with_grants(
        &source(&upstream.base),
        P::CheckedF64Utf16V38,
        vec![upstream.grant()],
    );
    let mut client = begin(&server);
    upstream
        .entered
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    server.stop.shutdown();
    let late = TcpStream::connect(server.addr);
    upstream.release.send(Some(b"drained".to_vec())).unwrap();
    let mut reply = String::new();
    client.read_to_string(&mut reply).unwrap();
    assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
    assert!(reply.ends_with("drained"), "{reply}");
    upstream.finish();
    let logs = server.lines.clone();
    server.finish();
    assert_eq!(*logs.lock().unwrap(), ["varta log: handle /slow"]);
    if let Ok(mut late) = late {
        // A reset socket can already reject socket options on macOS.
        if late.set_read_timeout(Some(Duration::from_secs(1))).is_err() {
            return;
        }
        let _ = late.write_all(b"GET / HTTP/1.1\r\nHost: test\r\n\r\n");
        let mut reply = String::new();
        let _ = late.read_to_string(&mut reply);
        assert!(reply.is_empty(), "late request ran: {reply}");
    }
}

#[test]
fn http_v2_shutdown_keeps_the_active_request_deadline_and_closes_its_socket() {
    let upstream = Upstream::start();
    let bytes = varta_compiler::compile_typescript_with_profile(
        &source(&upstream.base),
        P::CheckedF64Utf16V38,
    )
    .unwrap();
    let server = Server::start_options(
        bytes,
        RunOptions {
            requested: vec![upstream.grant()],
            budgets: varta_common::budget::BudgetRequest {
                deadline_per_turn_ms: Some(200),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let mut client = begin(&server);
    upstream
        .entered
        .recv_timeout(Duration::from_secs(2))
        .unwrap();
    server.stop.shutdown();
    upstream.release.send(None).unwrap();
    let started = std::time::Instant::now();
    let mut reply = String::new();
    client.read_to_string(&mut reply).unwrap();
    assert!(reply.starts_with("HTTP/1.1 500"), "{reply}");
    upstream.finish();
    server.finish();
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn http_v2_guest_stop_preserves_its_awaited_reply() {
    let source = r#"
        async function handle(r:VartaHttpRequest):Promise<VartaHttpResponse>{
            varta.http.stop();await new Promise<void>((resolve)=>{setTimeout(()=>{resolve();},20);});
            return {status:200,body:"drained",headers:[]};
        }
        export function main():void{}
        varta.http.serve({hostname:"127.0.0.1",port:0},handle);
    "#;
    let timer = CapabilityGrant {
        id: "time.timer".into(),
        value: CapabilityValue::Boolean(true),
    };
    let server = Server::start_with_grants(source, P::CheckedF64Utf16V38, vec![timer]);
    assert_eq!(server.request("GET", "/stop", b"").2, "drained");
    server.finish();
}

#[test]
fn http_v2_shutdown_closes_an_idle_or_partial_header_connection() {
    for bytes in [&b""[..], &b"GET / HTTP/1.1\r\nHost: "[..]] {
        let server = Server::start_profile(HANDLER, P::CheckedF64Utf16V38);
        let mut client = TcpStream::connect(server.addr).unwrap();
        client.write_all(bytes).unwrap();
        // Let the reactor accept the socket; it has no complete request yet.
        thread::sleep(Duration::from_millis(50));
        let started = std::time::Instant::now();
        let logs = server.lines.clone();
        server.finish();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(*logs.lock().unwrap(), ["varta log: bootstrap"]);
    }
}
