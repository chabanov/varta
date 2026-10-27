use super::*;
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};
use varta_common::{CapabilityGrant, CapabilityValue, http::HttpError};

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
}
impl Server {
    fn start(source: &str) -> Self {
        let bytes = varta_compiler::compile_typescript(source).unwrap();
        let runtime = VartaRuntime::new();
        let inspection = runtime.inspect_module(&bytes).unwrap();
        assert!(inspection.tool_catalog.is_none());
        let endpoint = inspection.http_service.unwrap().bind;
        let (sender, receiver) = mpsc::channel();
        let lines = Arc::new(Mutex::new(Vec::new()));
        let recorded = lines.clone();
        let task = thread::spawn(move || {
            runtime.run_observed(
                &bytes,
                &[grant(endpoint)],
                None,
                Arc::new(move |event| match event {
                    RuntimeEvent::Listening { address, shutdown } => {
                        sender.send((address, shutdown)).unwrap();
                    }
                    RuntimeEvent::Output(line) => recorded.lock().unwrap().push(line),
                    _ => {}
                }),
            )
        });
        let (addr, stop) = receiver
            .recv_timeout(Duration::from_secs(15))
            .expect("listener readiness");
        Self {
            addr,
            stop,
            task: Some(task),
            lines,
        }
    }
    fn request(&self, method: &str, target: &str, body: &[u8]) -> (u16, String, String) {
        let mut wire = format!(
            "{method} {target} HTTP/1.1\r\nHost: example.test\r\nContent-Length: {}\r\n\r\n",
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
    assert_eq!(status, 201);
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
            .raw(b"GET / HTTP/1.1\r\nHost: test\r\nContent-Length: 1\r\nContent-Length: 2\r\n\r\nx")
            .0,
        400
    );
    let wire = format!(
        "GET / HTTP/1.1\r\nHost: test\r\nX-Large: {}\r\n\r\n",
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
    let (status, _, _) = server.raw(b"POST / HTTP/1.1\r\nHost: test\r\nContent-Length: 8\r\n\r\nx");
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
