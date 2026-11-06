use super::*;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};
use varta_common::{CapabilityGrant, CapabilityValue, http::HttpError};

/// How long a test waits for a started service to listen: a safety net far
/// above a normal start, so only a hung start reaches it.
const LISTENER_READINESS: Duration = Duration::from_secs(15);

const HANDLER: &str = r#"
function handle(request:VartaHttpRequest):VartaHttpResponse{
 console.log("handled");
 if(request.target=="/stop"){varta.http.stop();return {status:200,body:"bye",headers:[]};}
 if(request.target=="/bad"){varta.json.parse<number>("bad");}
 if(request.target=="/fuel"){while(true){}}
 if(request.target=="/injection"){return {status:200,body:"bad",headers:[{name:"x-test",value:"bad\r\nInjected: yes"}]};}
 if(request.target=="/no-content"){return {status:204,body:"bad",headers:[]};}
 if(request.target=="/invalid"){return {status:200,body:"bad",headers:[{name:"content-length",value:"1"}]};}
 if(request.method=="POST"){
  const input=varta.json.parse<{value:string}>(request.body);
  return {status:201,body:JSON.stringify({value:input.value,target:request.target}),headers:[{name:"content-type",value:"application/json"}]};
 }
 return {status:200,body:JSON.stringify({method:request.method,target:request.target,headers:request.headers}),headers:[{name:"content-type",value:"application/json"}]};
}
export function main():void{console.log("bootstrap");}
varta.http.serve({hostname:"127.0.0.1",port:0},handle);
"#;
fn grant(endpoint: String) -> CapabilityGrant {
    CapabilityGrant {
        id: "net.listen".into(),
        value: CapabilityValue::ListenEndpoints(vec![endpoint]),
    }
}
struct Server {
    addr: SocketAddr,
    stop: HttpShutdown,
    task: Option<thread::JoinHandle<Result<RunOutput, RuntimeError>>>,
    lines: Arc<Mutex<Vec<String>>>,
    diagnostics: Arc<Mutex<Vec<String>>>,
    requests: Arc<Mutex<Vec<HttpRequestReport>>>,
    report: Arc<Mutex<Option<HttpReport>>>,
}
impl Server {
    fn start(source: &str) -> Self {
        Self::start_profile(source, varta_common::language::LanguageProfile::default())
    }
    fn start_profile(source: &str, profile: varta_common::language::LanguageProfile) -> Self {
        Self::start_with_grants(source, profile, Vec::new())
    }
    fn start_with_grants(
        source: &str,
        profile: varta_common::language::LanguageProfile,
        requested: Vec<CapabilityGrant>,
    ) -> Self {
        let bytes = varta_compiler::compile_typescript_with_profile(source, profile).unwrap();
        Self::start_bytes(bytes, requested)
    }
    fn start_bytes(bytes: Vec<u8>, requested: Vec<CapabilityGrant>) -> Self {
        Self::start_options(
            bytes,
            RunOptions {
                requested,
                ..Default::default()
            },
        )
    }
    /// A service the host serves over TLS (K7 H3), with its TLS grant.
    fn start_tls(bytes: Vec<u8>, policy: varta_common::http::HttpPolicy) -> Self {
        let tls = policy.tls.is_some();
        Self::start_listening(
            bytes,
            RunOptions {
                http: Some(policy),
                ..RunOptions::default()
            },
            tls,
        )
    }
    fn start_options(bytes: Vec<u8>, options: RunOptions) -> Self {
        let tls = options
            .http
            .as_ref()
            .is_some_and(|policy| policy.tls.is_some());
        Self::start_listening(bytes, options, tls)
    }
    fn start_listening(bytes: Vec<u8>, mut options: RunOptions, tls: bool) -> Self {
        let runtime = VartaRuntime::new();
        let inspection = runtime.inspect_module(&bytes).unwrap();
        assert!(inspection.tool_catalog.is_none());
        let endpoint = inspection.http_service.unwrap().bind;
        options
            .requested
            .push(grant(varta_common::http::listen_endpoint(&endpoint, tls)));
        let (sender, receiver) = mpsc::channel();
        let lines = Arc::new(Mutex::new(Vec::new()));
        let recorded = lines.clone();
        let diagnostics = Arc::new(Mutex::new(Vec::new()));
        let recorded_diagnostics = diagnostics.clone();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let recorded_requests = requests.clone();
        let report = Arc::new(Mutex::new(None));
        let recorded_report = report.clone();
        options.observer = Some(Arc::new(move |event| match event {
            RuntimeEvent::Listening {
                address, shutdown, ..
            } => {
                sender.send((address, shutdown)).unwrap();
            }
            RuntimeEvent::Output(line) => recorded.lock().unwrap().push(line),
            RuntimeEvent::Diagnostic(message) => recorded_diagnostics.lock().unwrap().push(message),
            RuntimeEvent::HttpRequest(report) => recorded_requests.lock().unwrap().push(*report),
        }));
        let task = thread::spawn(move || {
            let report = runtime.run_with(&bytes, options);
            *recorded_report.lock().unwrap() = report.http;
            match report.failure {
                Some(error) => Err(error),
                None => Ok(report.output),
            }
        });
        // A service that fails before it listens drops the observer: report
        // its own failure rather than a bare readiness timeout.
        let (addr, stop) = match receiver.recv_timeout(LISTENER_READINESS) {
            Ok(ready) => ready,
            Err(mpsc::RecvTimeoutError::Disconnected) => panic!(
                "the service ended before it listened: {:?}",
                task.join().map(|result| result.err())
            ),
            Err(mpsc::RecvTimeoutError::Timeout) => panic!("listener readiness timed out"),
        };
        Self {
            addr,
            stop,
            task: Some(task),
            lines,
            diagnostics,
            requests,
            report,
        }
    }
    fn request(&self, method: &str, target: &str, body: &[u8]) -> (u16, String, String) {
        let mut wire = format!(
            "{method} {target} HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .into_bytes();
        wire.extend(body);
        self.raw(&wire)
    }
    fn raw(&self, wire: &[u8]) -> (u16, String, String) {
        let mut stream = TcpStream::connect(self.addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
        stream.write_all(wire).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        let (headers, body) = response.split_once("\r\n\r\n").expect(&response);
        let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, headers.to_owned(), body.to_owned())
    }
    fn finish(mut self) {
        self.stop.shutdown();
        self.task.take().unwrap().join().unwrap().unwrap();
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.shutdown();
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
    }
}

#[test]
fn compiled_http_handler_serves_plain_json_headers_repeated_calls_and_shutdown() {
    let server = Server::start(HANDLER);
    let (status, headers, body) =
        server.request("POST", "/echo?x=1", r#"{"value":"Привіт🦀"}"#.as_bytes());
    assert_eq!(status, 201, "{:?}", server.diagnostics.lock().unwrap());
    assert!(
        headers
            .to_ascii_lowercase()
            .contains("content-type: application/json")
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap(),
        serde_json::json!({"value":"Привіт🦀","target":"/echo?x=1"})
    );
    let (status, _, body) = server.request("GET", "/hello", b"");
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&body).unwrap()["target"],
        "/hello"
    );
    let (status, headers, body) = server.request("HEAD", "/hello", b"");
    assert_eq!(status, 200);
    assert!(headers.to_ascii_lowercase().contains("content-length:"));
    assert!(body.is_empty());
    let (status, _, body) = server.request("GET", "/stop", b"");
    assert_eq!(status, 200);
    assert_eq!(body, "bye");
    let lines = server.lines.lock().unwrap().clone();
    assert_eq!(
        lines
            .iter()
            .filter(|s| s.as_str() == "varta log: bootstrap")
            .count(),
        1
    );
    assert_eq!(
        lines
            .iter()
            .filter(|s| s.as_str() == "varta log: handled")
            .count(),
        4
    );
    server.finish();
}
#[test]
fn listener_lifetime_outlives_bootstrap_and_failed_handlers_are_isolated() {
    let server = Server::start(HANDLER);
    thread::sleep(Duration::from_millis((abi::DEADLINE_SECONDS + 1) * 1000));
    for path in ["/bad", "/fuel", "/invalid", "/injection", "/no-content"] {
        let (status, _, body) = server.request("GET", path, b"");
        assert_eq!(status, 500, "{path}");
        assert!(!body.contains("checked JSON"));
        assert_eq!(server.request("GET", "/ok", b"").0, 200);
    }
    server.finish();
}
#[test]
fn http_body_framing_and_header_limits_reject_before_the_handler_and_recover() {
    let server = Server::start(HANDLER);
    assert_eq!(
        server
            .request(
                "POST",
                "/echo",
                &vec![b'x'; varta_common::http::MAX_BODY_BYTES + 1]
            )
            .0,
        413
    );
    assert_eq!(server.request("POST", "/echo", &[0xff]).0, 400);
    assert_eq!(
        server
            .raw(b"GET / HTTP/1.1\r\nHost: test\r\nConnection: close\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nx")
            .0,
        400
    );
    let wire = format!(
        "GET / HTTP/1.1\r\nHost: test\r\nConnection: close\r\nX-Large: {}\r\n\r\n",
        "a".repeat(varta_common::http::MAX_HEADER_BYTES + 1)
    );
    assert_eq!(server.raw(wire.as_bytes()).0, 431);
    assert_eq!(server.request("GET", "/ok", b"").0, 200);
    server.finish();
}
#[test]
fn listener_permissions_check_exact_package_and_host_endpoints_before_bind() {
    let held = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = held.local_addr().unwrap();
    let source = HANDLER.replace("port:0", &format!("port:{}", address.port()));
    let bytes = varta_compiler::compile_typescript(&source).unwrap();
    let runtime = VartaRuntime::new();
    let endpoint = address.to_string();
    let requested = [grant(endpoint.clone())];
    for (package, ceiling) in [
        (&[][..], None),
        (&requested[..], Some(&[][..])),
        (&[][..], Some(&requested[..])),
    ] {
        let error = match ceiling {
            Some(c) => runtime.run_wasm_bytes_with_policy(&bytes, package, c),
            None => runtime.run_wasm_bytes(&bytes, package),
        }
        .unwrap_err();
        assert!(
            matches!(error, RuntimeError::Http(HttpError::Denied(_))),
            "{error}"
        );
    }
    let outbound = [CapabilityGrant {
        id: "net.get".into(),
        value: CapabilityValue::UrlGlobs(vec![format!("http://{address}/**")]),
    }];
    assert!(matches!(
        runtime.run_wasm_bytes(&bytes, &outbound).unwrap_err(),
        RuntimeError::Http(HttpError::Denied(_))
    ));
    assert!(matches!(
        runtime.run_wasm_bytes(&bytes, &requested).unwrap_err(),
        RuntimeError::Http(HttpError::Io(_))
    ));
    let other = [grant("127.0.0.1:0".into())];
    assert!(matches!(
        runtime
            .run_wasm_bytes_with_policy(&bytes, &requested, &other)
            .unwrap_err(),
        RuntimeError::Http(HttpError::Denied(_))
    ));
}

#[test]
fn slow_headers_and_bodies_time_out_without_blocking_later_requests() {
    let server = Server::start(HANDLER);
    let mut stream = TcpStream::connect(server.addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(6)))
        .unwrap();
    stream.write_all(b"GET / HTTP/1.1\r\nHost:").unwrap();
    let mut discarded = String::new();
    stream.read_to_string(&mut discarded).unwrap();
    assert_eq!(server.request("GET", "/ok", b"").0, 200);
    let (status, _, _) = server
        .raw(b"POST / HTTP/1.1\r\nHost: test\r\nConnection: close\r\nContent-Length: 8\r\n\r\nx");
    assert_eq!(status, 408);
    assert_eq!(server.request("GET", "/ok", b"").0, 200);
    server.finish();
}
#[test]
fn listener_grants_do_not_give_handlers_environment_or_outbound_authority() {
    let source = HANDLER.replace(
        " console.log(\"handled\");",
        r#"
 const env=varta.result.capture(varta.env.get("VARTA_HTTP_SECRET"));
 const net=varta.result.capture(varta.net.fetch("http://127.0.0.1:1/private"));
 console.log(JSON.stringify({env:env.status,net:net.status}));
 console.log("handled");
"#,
    );
    let server = Server::start(&source);
    assert_eq!(server.request("GET", "/ok", b"").0, 200);
    assert!(
        server
            .lines
            .lock()
            .unwrap()
            .iter()
            .any(|s| s == "varta log: {\"env\":1,\"net\":1}")
    );
    server.finish();
}

#[test]
fn v38_http_v2_preserves_unicode_and_request_trap_isolation() {
    let source = r#"
        function handle(r:VartaHttpRequest):VartaHttpResponse {
            if(r.target=="/fail")throw new TypeError("handler failed");
            if(r.target=="/fuel")while(true){}
            if(r.target=="/fractional")return {status:200.5,body:"invalid",headers:[]};
            return {status:201,body:r.body+" — Привіт🦀",headers:[{name:"content-type",value:"text/plain; charset=utf-8"}]};
        }
        export function main():void {}
        varta.http.serve({hostname:"127.0.0.1",port:0},handle);
    "#;
    let server = Server::start_profile(
        source,
        varta_common::language::LanguageProfile::CheckedF64Utf16V38,
    );
    for text in ["plain", "Привіт🦀", ""] {
        let (status, _, body) = server.request("POST", "/echo", text.as_bytes());
        assert_eq!(status, 201);
        assert_eq!(body, format!("{text} — Привіт🦀"));
    }
    for target in ["/fail", "/fuel", "/fractional"] {
        assert_eq!(server.request("GET", target, b"").0, 500);
    }
    assert_eq!(
        server.request("POST", "/after", b"alive").2,
        "alive — Привіт🦀"
    );
    server.finish();
}

#[test]
fn async_http_v2_awaits_owned_work_and_discards_background_timers() {
    let source = r#"
        let calls=0;
        const settings={suffix:" 🦀"};
        async function handle(r:VartaHttpRequest):Promise<VartaHttpResponse> {
            calls++;
            if(r.target=="/timer")await new Promise<void>((resolve)=>{setTimeout(()=>{resolve();},5);});
            if(r.target=="/hang")await new Promise<void>((resolve)=>{});
            if(r.target=="/background")setInterval(()=>{console.log("unexpected interval");},10000);
            await null;
            if(r.target=="/fail")throw new TypeError("async handler failed");
            return {status:201,body:r.body+settings.suffix+" "+String(calls),headers:[]};
        }
        export function main():void {calls=99;console.log("bootstrap");}
        varta.http.serve({hostname:"127.0.0.1",port:0},handle);
    "#;
    let grants = vec![CapabilityGrant {
        id: "time.timer".into(),
        value: CapabilityValue::Boolean(true),
    }];
    let profile = varta_common::language::LanguageProfile::CheckedF64Utf16V38;
    let normal = varta_compiler::compile_typescript_with_profile(source, profile).unwrap();
    let collecting =
        varta_compiler::json_decoder_test_hook::compile_collecting(source, profile).unwrap();
    for bytes in [normal, collecting] {
        let server = Server::start_bytes(bytes, grants.clone());
        for target in ["/ok", "/timer", "/background"] {
            let (status, _, body) = server.request("POST", target, "Привіт".as_bytes());
            assert_eq!(status, 201, "{target}: {body}");
            assert_eq!(body, "Привіт 🦀 1");
        }
        for target in ["/fail", "/hang"] {
            assert_eq!(server.request("GET", target, b"").0, 500);
        }
        assert_eq!(server.request("POST", "/after", b"alive").2, "alive 🦀 1");
        let logs = server.lines.lock().unwrap().clone();
        assert_eq!(
            logs.iter()
                .filter(|s| s.as_str() == "varta log: bootstrap")
                .count(),
            1
        );
        assert!(!logs.iter().any(|s| s.contains("unexpected interval")));
        server.finish();
    }
}

#[test]
fn synchronous_http_v2_runs_its_microtask_checkpoint() {
    let source = r#"
        async function job(fail:boolean):Promise<void>{await null;if(fail)throw new TypeError("unhandled job");console.log("job");}
        function handle(r:VartaHttpRequest):VartaHttpResponse{job(r.target=="/fail");return {status:200,body:r.body,headers:[]};}
        export function main():void{}
        varta.http.serve({hostname:"127.0.0.1",port:0},handle);
    "#;
    let server = Server::start_profile(
        source,
        varta_common::language::LanguageProfile::CheckedF64Utf16V38,
    );
    assert_eq!(server.request("POST", "/ok", b"one").2, "one");
    assert_eq!(server.request("GET", "/fail", b"").0, 500);
    assert_eq!(server.request("POST", "/after", b"two").2, "two");
    assert_eq!(
        server
            .lines
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.as_str() == "varta log: job")
            .count(),
        2
    );
    server.finish();
}

#[test]
fn async_http_v2_waits_for_outbound_io_and_denies_without_a_socket() {
    use varta_common::{RequestScope, language::LanguageProfile as P};
    let upstream = crate::net_loopback::Loopback::start();
    let source = r#"
        async function handle(r:VartaHttpRequest):Promise<VartaHttpResponse>{
            try {
                const response=await fetch("__BASE__/json");
                return {status:response.status,body:await response.text(),headers:[]};
            } catch(e) {return {status:403,body:e instanceof Error?e.name:"wrong",headers:[]};}
        }
        export function main():void{}
        varta.http.serve({hostname:"127.0.0.1",port:0},handle);
    "#
    .replace("__BASE__", &upstream.base);
    let grants = vec![CapabilityGrant {
        id: "net.request".into(),
        value: CapabilityValue::Requests(RequestScope {
            urls: vec![format!("{}/**", upstream.base)],
            methods: None,
            response_headers: None,
        }),
    }];
    let server = Server::start_with_grants(&source, P::CheckedF64Utf16V38, grants);
    for _ in 0..2 {
        let (status, _, body) = server.request("GET", "/proxy", b"");
        assert_eq!(status, 201, "{body}");
        assert_eq!(body, r#"{"ok":true,"items":[1,2,3]}"#);
    }
    server.finish();
    let before = upstream.accepted();
    let denied = Server::start_profile(&source, P::CheckedF64Utf16V38);
    assert_eq!(denied.request("GET", "/proxy", b"").0, 403);
    assert_eq!(upstream.accepted(), before);
    denied.finish();
}

#[test]
fn http_v2_requests_inherit_package_and_host_budgets() {
    use varta_common::{budget::BudgetRequest, language::LanguageProfile as P};
    let source = r#"
        function handle(r:VartaHttpRequest):VartaHttpResponse {
            if(r.target=="/fuel"){let n=0;for(let i=0;i<20000;i++)n+=i;return {status:200,body:String(n),headers:[]};}
            if(r.target=="/memory"){const a=["a".repeat(400000),"b".repeat(400000),"c".repeat(400000),"d".repeat(400000),"e".repeat(400000),"f".repeat(400000),"g".repeat(400000),"h".repeat(400000)];return {status:200,body:String(a.length),headers:[]};}
            if(r.target=="/calls")for(let i=0;i<20;i++)console.log("call");
            if(r.target=="/output")console.log("xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx");
            return {status:200,body:"ok",headers:[]};
        }
        export function main():void{}
        varta.http.serve({hostname:"127.0.0.1",port:0},handle);
    "#;
    let bytes =
        varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V38).unwrap();
    for (path, budget) in [
        (
            "/fuel",
            BudgetRequest {
                fuel_per_turn: Some(50000),
                ..Default::default()
            },
        ),
        (
            "/memory",
            BudgetRequest {
                memory_bytes: Some(4 * 1024 * 1024),
                ..Default::default()
            },
        ),
        (
            "/calls",
            BudgetRequest {
                host_calls_per_turn: Some(5),
                ..Default::default()
            },
        ),
        (
            "/output",
            BudgetRequest {
                output_bytes: Some(64),
                ..Default::default()
            },
        ),
    ] {
        // Each failure is observable only when the request inherits this
        // particular bound; the default request succeeds for the same source.
        let baseline = Server::start_bytes(bytes.clone(), vec![]);
        assert_eq!(baseline.request("GET", path, b"").0, 200, "baseline {path}");
        baseline.finish();
        for host in [false, true] {
            let options = if host {
                RunOptions {
                    budget_ceiling: Some(budget),
                    ..Default::default()
                }
            } else {
                RunOptions {
                    budgets: budget,
                    ..Default::default()
                }
            };
            let server = Server::start_options(bytes.clone(), options);
            assert_eq!(
                server.request("GET", path, b"").0,
                500,
                "{path} host={host}"
            );
            assert_eq!(server.request("GET", "/after", b"").2, "ok");
            server.finish();
        }
    }
}

#[test]
fn async_http_v2_keeps_one_deadline_and_fuel_allowance_for_the_whole_request() {
    use varta_common::{budget::BudgetRequest, language::LanguageProfile as P};
    let source = r#"
        async function handle(r:VartaHttpRequest):Promise<VartaHttpResponse> {
            if(r.target=="/deadline")await new Promise<void>((resolve)=>{setTimeout(()=>{resolve();},1000);});
            if(r.target=="/contract")await new Promise<void>((resolve)=>{setTimeout(()=>{resolve();},6000);});
            if(r.target=="/aggregate")for(let i=0;i<4;i++){
                let n=0;for(let j=0;j<2000;j++)n+=j;
                await new Promise<void>((resolve)=>{setTimeout(()=>{resolve();},1);});
            }
            return {status:200,body:"ok",headers:[]};
        }
        export function main():void{}
        varta.http.serve({hostname:"127.0.0.1",port:0},handle);
    "#;
    let bytes =
        varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V38).unwrap();
    let timer = CapabilityGrant {
        id: "time.timer".into(),
        value: CapabilityValue::Boolean(true),
    };
    for (path, budget) in [
        (
            "/deadline",
            BudgetRequest {
                deadline_per_turn_ms: Some(200),
                ..Default::default()
            },
        ),
        (
            "/contract",
            BudgetRequest {
                deadline_per_turn_ms: Some(60000),
                ..Default::default()
            },
        ),
        (
            "/aggregate",
            BudgetRequest {
                fuel_per_turn: Some(100000),
                ..Default::default()
            },
        ),
    ] {
        let server = Server::start_options(
            bytes.clone(),
            RunOptions {
                requested: vec![timer.clone()],
                budgets: budget,
                budget_ceiling: Some(budget),
                ..Default::default()
            },
        );
        assert_eq!(server.request("GET", path, b"").0, 500, "{path}");
        assert_eq!(server.request("GET", "/after", b"").0, 200);
        server.finish();
    }
}

#[test]
fn http_v2_optional_listener_permission_is_checked_before_bind() {
    use varta_common::{language::LanguageProfile as P, permission::Consent};
    let held = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = held.local_addr().unwrap();
    let bytes = varta_compiler::compile_typescript_with_profile(
        &HANDLER.replace("port:0", &format!("port:{}", address.port())),
        P::CheckedF64Utf16V38,
    )
    .unwrap();
    let report = VartaRuntime::new().run_with(
        &bytes,
        RunOptions {
            requested: vec![grant(address.to_string())],
            consents: std::collections::BTreeMap::from([("net.listen".into(), Consent::default())]),
            ..Default::default()
        },
    );
    assert!(
        matches!(
            report.failure,
            Some(RuntimeError::Http(HttpError::Denied(_)))
        ),
        "{:?}",
        report.failure
    );
}

#[test]
fn http_v2_requests_preserve_headless_consents_and_artifact_scoped_decisions() {
    use varta_common::{
        RequestScope,
        language::LanguageProfile as P,
        permission::{ApplicationDecisions, Consent, DecisionFile, StoredDecision},
    };
    let upstream = crate::net_loopback::Loopback::start();
    let source=r#"
        async function handle(r:VartaHttpRequest):Promise<VartaHttpResponse>{
            try {const response=await fetch("__BASE__/hello");return {status:200,body:await response.text(),headers:[]};}
            catch(e){return {status:403,body:e instanceof Error?e.name:"wrong",headers:[]};}
        }
        export function main():void{}
        varta.http.serve({hostname:"127.0.0.1",port:0},handle);
    "#.replace("__BASE__",&upstream.base);
    let bytes =
        varta_compiler::compile_typescript_with_profile(&source, P::CheckedF64Utf16V38).unwrap();
    let request = CapabilityGrant {
        id: "net.request".into(),
        value: CapabilityValue::Requests(RequestScope {
            urls: vec![format!("{}/**", upstream.base)],
            methods: None,
            response_headers: None,
        }),
    };
    for (decision, matching, allowed, ceiling) in [
        (None, true, false, false),
        (Some(StoredDecision::Deny), true, false, false),
        (Some(StoredDecision::Allow), true, true, false),
        (Some(StoredDecision::Allow), false, false, false),
        (Some(StoredDecision::Allow), true, false, true),
    ] {
        let mut options = RunOptions {
            requested: vec![request.clone()],
            consents: std::collections::BTreeMap::from([(
                "net.request".into(),
                Consent::default(),
            )]),
            ..Default::default()
        };
        if let Some(decision) = decision {
            let artifact = if matching {
                artifact_identity(&bytes)
            } else {
                format!("sha256:{}", "0".repeat(64))
            };
            options.decisions = Some(DecisionFile {
                schema_version: 1,
                applications: vec![ApplicationDecisions {
                    artifact,
                    decisions: vec![varta_common::permission::DecisionRecord {
                        id: "net.request".into(),
                        decision,
                    }],
                }],
            });
        }
        if ceiling {
            options.ceiling = Some(vec![grant("127.0.0.1:0".into())]);
        }
        let before = upstream.accepted();
        let server = Server::start_options(bytes.clone(), options);
        for _ in 0..2 {
            let (status, _, body) = server.request("GET", "/proxy", b"");
            assert_eq!(status, if allowed { 200 } else { 403 }, "{body}");
        }
        if !allowed {
            assert_eq!(upstream.accepted(), before);
        }
        server.finish();
    }
}

#[test]
fn http_v2_keeps_output_before_failure_and_delivers_each_diagnostic_once() {
    use varta_common::language::LanguageProfile as P;
    let source = r#"
        async function handle(r:VartaHttpRequest):Promise<VartaHttpResponse>{
            console.log("request "+r.target);await null;
            if(r.target=="/fail")throw new TypeError("after output");
            return {status:200,body:"ok",headers:[]};
        }
        export function main():void{}
        varta.http.serve({hostname:"127.0.0.1",port:0},handle);
    "#;
    let server = Server::start_profile(source, P::CheckedF64Utf16V38);
    assert_eq!(server.request("GET", "/fail", b"").0, 500);
    assert_eq!(server.request("GET", "/after", b"").0, 200);
    assert_eq!(
        *server.lines.lock().unwrap(),
        ["varta log: request /fail", "varta log: request /after"]
    );
    let diagnostics = server.diagnostics.lock().unwrap().clone();
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert!(diagnostics[0].contains("after output"), "{diagnostics:?}");
    server.finish();
}

#[test]
fn http_v1_keeps_legacy_budget_and_consent_semantics() {
    use varta_common::{budget::BudgetRequest, permission::Consent};
    let bytes = varta_compiler::compile_typescript(HANDLER).unwrap();
    let budget = BudgetRequest {
        fuel_per_turn: Some(1),
        memory_bytes: Some(1),
        host_calls_per_turn: Some(1),
        output_bytes: Some(1),
        deadline_per_turn_ms: Some(1),
        ..Default::default()
    };
    let server = Server::start_options(
        bytes,
        RunOptions {
            budgets: budget,
            budget_ceiling: Some(budget),
            consents: std::collections::BTreeMap::from([("net.listen".into(), Consent::default())]),
            ..Default::default()
        },
    );
    assert_eq!(server.request("GET", "/hello", b"").0, 200);
    server.finish();
}

mod cancellation;
mod reports;
mod streaming;
mod tasks;
mod tls;
mod web_request;
