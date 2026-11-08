use super::*;
use varta_common::{language::LanguageProfile, session as contract};
const SOURCE: &str = r#"
export function main():void{}
async function handler(request:VartaHttpRequest):Promise<VartaHttpResponse>{
 if(request.target==='/set'){varta.http.session.set('synthetic-private-key');return {status:201,body:'public',headers:[]};}
 if(request.target==='/other-set'){varta.http.session.set('other-private-key');return {status:201,body:'public',headers:[]};}
 if(request.target==='/rotate'){varta.http.session.set('replacement-private-key');return {status:200,body:'rotated',headers:[]};}
 if(request.target==='/bad-set'){varta.http.session.set('bad-private-key');return {status:500,body:'refused',headers:[]};}
 if(request.target==='/trap-set'){varta.http.session.set('bad-private-key');throw new Error('handler failed');}
 if(request.target==='/invalid-set'){varta.http.session.set('bad-private-key');return {status:200,body:'bad',headers:[{name:'content-length',value:'1'}]};}
 if(request.target==='/clear'){varta.http.session.clear();return {status:200,body:'cleared',headers:[]};}
 if(request.target==='/clear-trap'){varta.http.session.clear();throw new Error('after clear');}
 let secret:string|null=null;try{secret=varta.http.session.get();}catch(error){return {status:403,body:'channel-required',headers:[]};}
 if(secret===null){return {status:401,body:'anonymous',headers:[]};}
 if(request.target==='/identity'){return {status:200,body:secret==='other-private-key'?'B':'A',headers:[]};}
 const valid=secret==='synthetic-private-key'||secret==='replacement-private-key';
 return {status:valid?200:500,body:JSON.stringify(request.headers),headers:[]};
}
varta.http.serve({hostname:'127.0.0.1',port:0},handler);
"#;
fn options() -> RunOptions {
    let requested = [
        contract::READ_CAPABILITY,
        contract::WRITE_CAPABILITY,
        contract::CLEAR_CAPABILITY,
    ]
    .map(|id| CapabilityGrant {
        id: id.into(),
        value: CapabilityValue::Boolean(true),
    })
    .to_vec();
    let mut ceiling = requested.clone();
    ceiling.push(grant("127.0.0.1:0".into()));
    RunOptions {
        requested,
        ceiling: Some(ceiling),
        http: Some(varta_common::http::HttpPolicy {
            limits: None,
            tls: None,
            session: Some(contract::ServerPolicy {
                origin: "http://127.0.0.1:8787".into(),
                client_origin: "http://127.0.0.1:8788".into(),
                max_age_seconds: 3600,
                idle_seconds: 900,
                max_sessions: 2,
                same_site: contract::CookieSite::Strict,
                allow_insecure_loopback: true,
            }),
        }),
        ..Default::default()
    }
}
fn call(
    server: &Server,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    extra: &str,
) -> (u16, String, String) {
    let cookie = cookie
        .map(|v| format!("Cookie: {v}\r\n"))
        .unwrap_or_default();
    server.raw(format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:8787\r\nOrigin: http://127.0.0.1:8788\r\nX-Varta-Session: 1\r\n{cookie}{extra}Content-Length: 0\r\nConnection: close\r\n\r\n").as_bytes())
}
fn cookie(headers: &str) -> String {
    headers
        .lines()
        .find_map(|line| line.strip_prefix("set-cookie: "))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}
#[test]
fn protected_session_rotates_only_valid_success_and_revokes_across_request_stores() {
    let bytes = varta_compiler::compile_typescript_with_profile(
        SOURCE,
        LanguageProfile::CheckedF64Utf16V40,
    )
    .unwrap();
    let server = Server::start_options(bytes.clone(), options());
    let (status, headers, body) = call(&server, "POST", "/set", None, "");
    assert_eq!(
        status,
        201,
        "{body}; {:?}",
        server.diagnostics.lock().unwrap()
    );
    assert_eq!(body, "public");
    assert!(headers.contains("HttpOnly"));
    assert!(headers.contains("SameSite=Strict"));
    assert!(!headers.contains("private-key"));
    let first = cookie(&headers);
    let (_, headers, _) = call(&server, "POST", "/other-set", None, "");
    let other = cookie(&headers);
    assert_eq!(call(&server, "GET", "/identity", Some(&first), "").2, "A");
    assert_eq!(call(&server, "GET", "/identity", Some(&other), "").2, "B");
    let (status, _, body) = call(&server, "GET", "/read", Some(&first), "");
    assert_eq!(status, 200);
    assert!(!body.to_lowercase().contains("cookie"));
    assert!(!body.contains("private-key"));
    assert!(!body.contains("x-varta-session"));
    for path in ["/bad-set", "/trap-set", "/invalid-set"] {
        assert_eq!(call(&server, "POST", path, Some(&first), "").0, 500);
        assert_eq!(call(&server, "GET", "/read", Some(&first), "").0, 200);
    }
    let (status, headers, _) = call(&server, "POST", "/rotate", Some(&first), "");
    assert_eq!(status, 200);
    let second = cookie(&headers);
    assert_ne!(first, second);
    assert_eq!(call(&server, "GET", "/read", Some(&first), "").0, 401);
    assert_eq!(call(&server, "GET", "/read", Some(&second), "").0, 200);
    assert_eq!(
        call(&server, "POST", "/clear-trap", Some(&second), "").0,
        500
    );
    assert_eq!(call(&server, "GET", "/read", Some(&second), "").0, 401);
    assert_eq!(call(&server, "GET", "/identity", Some(&other), "").2, "B");
    let (_, headers, _) = call(&server, "POST", "/set", None, "");
    let before_restart = cookie(&headers);
    server.finish();
    let server = Server::start_options(bytes, options());
    assert_eq!(call(&server, "GET", "/identity", Some(&other), "").0, 401);
    assert_eq!(
        call(&server, "GET", "/read", Some(&before_restart), "").0,
        401
    );
    server.finish();
}
#[test]
fn protected_session_csrf_preflight_conflicts_and_host_ceiling_fail_before_secrets() {
    let bytes = varta_compiler::compile_typescript_with_profile(
        SOURCE,
        LanguageProfile::CheckedF64Utf16V40,
    )
    .unwrap();
    let server = Server::start_options(bytes.clone(), options());
    let (status,headers,_)=server.raw(b"OPTIONS /set HTTP/1.1\r\nHost: 127.0.0.1:8787\r\nOrigin: http://127.0.0.1:8788\r\nAccess-Control-Request-Method: POST\r\nAccess-Control-Request-Headers: content-type, x-varta-session\r\nConnection: close\r\n\r\n");
    assert_eq!(status, 204);
    assert!(headers.contains("access-control-allow-credentials: true"));
    assert!(!headers.contains("set-cookie"));
    for request in [
        "POST /set HTTP/1.1\r\nHost: 127.0.0.1:8787\r\nOrigin: https://evil.test\r\nX-Varta-Session: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        "POST /set HTTP/1.1\r\nHost: 127.0.0.1:8787\r\nOrigin: null\r\nX-Varta-Session: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        "POST http://evil.test/set HTTP/1.1\r\nHost: 127.0.0.1:8787\r\nOrigin: http://127.0.0.1:8788\r\nX-Varta-Session: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    ] {
        let (status, headers, _) = server.raw(request.as_bytes());
        assert_eq!(status, 403);
        assert!(headers.contains("access-control-allow-origin: http://127.0.0.1:8788"));
        assert!(headers.contains("cache-control: no-store"));
        assert!(!headers.contains("set-cookie"));
    }
    assert_eq!(
        call(
            &server,
            "POST",
            "/set",
            None,
            "Authorization: Bearer ambiguous\r\n"
        )
        .0,
        403
    );
    // Even an anonymous lookup must validate the channel before app OTP work.
    for headers in [
        "Origin: http://127.0.0.1:8788\r\n",
        "X-Varta-Session: 1\r\n",
        "",
    ] {
        let request = format!(
            "GET /read HTTP/1.1\r\nHost: 127.0.0.1:8787\r\n{headers}Connection: close\r\n\r\n"
        );
        assert_eq!(server.raw(request.as_bytes()).0, 403);
    }
    // Safe methods cannot mint or clear sessions, even with a valid channel.
    for path in ["/set", "/clear"] {
        let (status, headers, _) = call(&server, "GET", path, None, "");
        assert_eq!(status, 500);
        assert!(!headers.contains("set-cookie"));
    }
    server.finish();
    let mut denied = options();
    denied
        .ceiling
        .as_mut()
        .unwrap()
        .retain(|g| g.id != contract::WRITE_CAPABILITY);
    let server = Server::start_options(bytes, denied);
    let (status, headers, _) = call(&server, "POST", "/set", None, "");
    assert_eq!(status, 500);
    assert!(!headers.contains("set-cookie"));
    server.finish();
}

#[test]
fn protected_session_logout_cancels_an_authenticated_request_waiting_on_io() {
    authenticated_wait(false);
}

#[test]
fn protected_session_expiry_cancels_waiting_io_without_another_request() {
    authenticated_wait(true);
}

fn authenticated_wait(expiry: bool) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap();
    let (arrived, ready) = mpsc::channel();
    let upstream = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
        let mut bytes = [0u8; 4096];
        assert!(socket.read(&mut bytes).unwrap() > 0);
        arrived.send(()).unwrap();
        // No response: only cancellation of the request closes this transfer.
        let mut rest = Vec::new();
        socket.read_to_end(&mut rest).unwrap();
    });
    let wait = format!(
        "const secret=varta.http.session.get();if(secret===null){{return {{status:401,body:'anonymous',headers:[]}};}} if(request.target==='/wait'){{await fetch('http://{endpoint}/wait');}}"
    );
    let source = SOURCE.replace("let secret:string|null=null;try{secret=varta.http.session.get();}catch(error){return {status:403,body:'channel-required',headers:[]};}", &wait);
    let bytes = varta_compiler::compile_typescript_with_profile(
        &source,
        LanguageProfile::CheckedF64Utf16V40,
    )
    .unwrap();
    let mut configured = options();
    if expiry {
        let session = configured.http.as_mut().unwrap().session.as_mut().unwrap();
        session.max_age_seconds = 2;
        session.idle_seconds = 1;
    }
    let request = CapabilityGrant {
        id: "net.request".into(),
        value: CapabilityValue::Requests(varta_common::RequestScope {
            urls: vec![format!("http://{endpoint}/**")],
            methods: Some(vec!["GET".into()]),
            response_headers: None,
        }),
    };
    configured.requested.push(request.clone());
    configured.ceiling.as_mut().unwrap().push(request);
    let server = Server::start_options(bytes, configured);
    let (_, headers, _) = call(&server, "POST", "/set", None, "");
    let token = cookie(&headers);
    let address = server.addr;
    let captured = token.clone();
    let worker = thread::spawn(move || {
        let mut socket = TcpStream::connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
        write!(socket,"GET /wait HTTP/1.1\r\nHost: 127.0.0.1:8787\r\nOrigin: http://127.0.0.1:8788\r\nX-Varta-Session: 1\r\nCookie: {captured}\r\nConnection: close\r\n\r\n").unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        response
    });
    ready.recv_timeout(Duration::from_secs(15)).unwrap();
    if !expiry {
        assert_eq!(call(&server, "POST", "/clear", Some(&token), "").0, 200);
    }
    // Expiry must interrupt the transfer on its own, without a read/clear
    // or provider response waking the request's Store.

    assert!(worker.join().unwrap().starts_with("HTTP/1.1 500"));
    upstream.join().unwrap();
    assert!(
        server
            .diagnostics
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.contains("execution cancelled"))
    );
    server.finish();
}

#[test]
fn protected_session_stream_head_seals_mutations_but_allows_checked_reads() {
    let source = r#"
export function main():void{}
function handler(request:VartaHttpRequest):Response {
 if(request.target==='/set') { varta.http.session.set('synthetic-private-key');return new Response('created'); }
 const stream=new ReadableStream<Uint8Array>({pull:async (controller:ReadableStreamDefaultController<Uint8Array>):Promise<void> => {
  let denied=0;
  try{varta.http.session.set('late-key');}catch(error){if(error instanceof DOMException&&error.name==='InvalidStateError')denied+=1;}
  try{varta.http.session.clear();}catch(error){if(error instanceof DOMException&&error.name==='InvalidStateError')denied+=1;}
  const secret=varta.http.session.get();
  controller.enqueue(new TextEncoder().encode(String(denied)+':'+String(secret==='synthetic-private-key')));controller.close();
 }});
 return new Response(stream);
}
varta.http.serve({hostname:'127.0.0.1',port:0},handler);
"#;
    let bytes = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V40,
    )
    .unwrap();
    let server = Server::start_options(bytes, options());
    let (_, headers, _) = call(&server, "POST", "/set", None, "");
    let token = cookie(&headers);
    let (status, headers, body) = call(&server, "POST", "/stream", Some(&token), "");
    assert_eq!(status, 200, "{body}");
    assert!(!headers.contains("set-cookie"));
    assert!(
        body.contains("2:true"),
        "{body}; {:?}",
        server.diagnostics.lock().unwrap()
    );
    assert_eq!(call(&server, "POST", "/stream", Some(&token), "").0, 200);
    server.finish();
}
