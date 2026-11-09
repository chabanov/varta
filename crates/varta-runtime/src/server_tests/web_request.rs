//! V2 Web `Request` handlers: the method, absolute URL and headers as
//! received, a body that streams from the client while the guest reads it
//! (bounded in size, per wait and in all), turns that renew while the
//! request waits for its body, and host policies that lower server limits.
use super::*;
use std::time::Instant;
use varta_common::{
    budget::BudgetRequest,
    http::{HostLimits, HttpPolicy},
    language::LanguageProfile as P,
};

const SERVICE: &str = r#"
async function handle(request: Request): Promise<Response> {
  const path = new URL(request.url).pathname;
  if (path == "/echo") {
    const text = await request.text();
    const type = request.headers.get("content-type") ?? "";
    return new Response(
      JSON.stringify({ method: request.method, url: request.url, type: type, body: text, used: request.bodyUsed }),
      { headers: { "content-type": "application/json" } },
    );
  }
  if (path == "/none") {
    return new Response(request.body === null ? "none" : "some");
  }
  if (path == "/chunks") {
    const body = request.body;
    if (body === null) {
      return new Response("none", { status: 400 });
    }
    const reader = body.getReader();
    let bytes = 0;
    for (;;) {
      const result = await reader.read();
      if (result.done) {
        break;
      }
      bytes += result.value.length;
    }
    return new Response(String(bytes));
  }
  if (path == "/fail") {
    try {
      await request.text();
      return new Response("read");
    } catch (error) {
      return new Response("failed", { status: 422 });
    }
  }
  if (path == "/copy") {
    const copy = new Request(request);
    const copied = await copy.text();
    return new Response(copy.method + " " + copied);
  }
  if (path == "/flood") {
    const chunk = new Uint8Array(65536);
    const stream = new ReadableStream<Uint8Array>({
      pull: async (controller: ReadableStreamDefaultController<Uint8Array>): Promise<void> => {
        controller.enqueue(chunk);
      },
    });
    return new Response(stream);
  }
  return new Response("not found", { status: 404 });
}
export function main(): void {}
varta.http.serve({ hostname: "127.0.0.1", port: 0 }, handle);
"#;

fn bytes() -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(SERVICE, P::CheckedF64Utf16V38).unwrap()
}

fn start(options: RunOptions) -> Server {
    Server::start_options(bytes(), options)
}

fn lowered(limits: HostLimits) -> RunOptions {
    RunOptions {
        http: Some(HttpPolicy {
            delegation: None,
            limits: Some(limits),
            tls: None,
            session: None,
        }),
        ..RunOptions::default()
    }
}

/// Send `head` (ending in its blank line), then each body piece after a
/// pause, and read the whole response.
fn upload(server: &Server, head: &str, pieces: &[&[u8]], pause: Duration) -> (u16, String) {
    let mut stream = TcpStream::connect(server.addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    stream.write_all(head.as_bytes()).unwrap();
    for piece in pieces {
        thread::sleep(pause);
        // A server that answered early may already have closed.
        if stream.write_all(piece).is_err() {
            break;
        }
    }
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    let (headers, body) = response.split_once("\r\n\r\n").expect(&response);
    let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, body.to_owned())
}

/// The data of a chunked response body.
fn dechunk(body: &str) -> String {
    let mut rest = body;
    let mut data = String::new();
    while let Some((size, tail)) = rest.split_once("\r\n") {
        let size = usize::from_str_radix(size, 16).unwrap();
        if size == 0 {
            break;
        }
        data.push_str(&tail[..size]);
        rest = &tail[size + 2..];
    }
    data
}

#[test]
fn a_web_request_handler_sees_method_url_headers_and_its_streamed_body() {
    let server = start(RunOptions::default());
    let (status, _, body) =
        server.request("POST", "/echo?x=1", r#"{"value":"Привіт🦀"}"#.as_bytes());
    assert_eq!(status, 200, "{body}");
    let value: serde_json::Value = serde_json::from_str(&dechunk(&body)).unwrap();
    assert_eq!(
        value,
        serde_json::json!({
            "method": "POST",
            "url": "http://example.test/echo?x=1",
            "type": "",
            "body": r#"{"value":"Привіт🦀"}"#,
            "used": true,
        })
    );
    // A request without a body has none; one with a body streams it.
    let (status, _, body) = server.request("GET", "/none", b"");
    assert_eq!((status, dechunk(&body).as_str()), (200, "none"));
    let (status, _, body) = server.request("POST", "/none", b"x");
    assert_eq!((status, dechunk(&body).as_str()), (200, "some"));
    // A request made of a received one takes its method and body over.
    let (status, _, body) = server.request("PUT", "/copy", b"moved");
    assert_eq!((status, dechunk(&body).as_str()), (200, "PUT moved"));
    // Without a `Host` header the URL names the listener.
    let (status, _, body) = server.raw(b"POST /echo HTTP/1.0\r\nContent-Length: 2\r\n\r\nhi");
    assert_eq!(status, 200);
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["url"], format!("http://{}/echo", server.addr));
    // An invalid authority is refused before the handler.
    assert_eq!(
        server
            .raw(b"GET /none HTTP/1.1\r\nHost: a b\r\nConnection: close\r\n\r\n")
            .0,
        400
    );
    server.finish();
}

#[test]
fn a_streamed_body_arrives_while_the_guest_reads_and_waits_use_no_turn_deadline() {
    // Each turn may run 300 ms; the client pauses longer between its body
    // chunks, and the request still completes: waits are not turns.
    let server = start(RunOptions {
        budgets: BudgetRequest {
            deadline_per_turn_ms: Some(300),
            ..Default::default()
        },
        ..RunOptions::default()
    });
    let head = "POST /chunks HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\nTransfer-Encoding: chunked\r\n\r\n";
    let started = Instant::now();
    let (status, body) = upload(
        &server,
        head,
        &[b"5\r\nhello\r\n", b"6\r\n world\r\n", b"0\r\n\r\n"],
        Duration::from_millis(700),
    );
    assert!(started.elapsed() >= Duration::from_millis(2100));
    assert_eq!((status, dechunk(&body).as_str()), (200, "11"));
    // A streaming worker reports once its body ended, after the response.
    let waited = Instant::now();
    while server.requests.lock().unwrap().is_empty() && waited.elapsed() < LISTENER_READINESS {
        thread::sleep(Duration::from_millis(10));
    }
    let report = server.requests.lock().unwrap().clone();
    server.finish();
    // The body chunks were turns of their own.
    let budget = report[0].budget.as_ref().unwrap();
    assert!(budget.turns.len() >= 3, "{budget:?}");
}

#[test]
fn request_bodies_are_bounded_in_size_and_per_wait() {
    let server = start(lowered(HostLimits {
        request_body_bytes: Some(8),
        body_idle_ms: Some(300),
        ..HostLimits::default()
    }));
    // A declared length beyond the bound is refused before the handler.
    assert_eq!(server.request("POST", "/fail", b"123456789").0, 413);
    assert_eq!(server.request("POST", "/fail", b"12345678").0, 200);
    // A chunked body that grows beyond it fails the guest's read.
    let head = "POST /fail HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\nTransfer-Encoding: chunked\r\n\r\n";
    let (status, _) = upload(
        &server,
        head,
        &[b"5\r\nhello\r\n", b"5\r\nworld\r\n", b"0\r\n\r\n"],
        Duration::from_millis(20),
    );
    assert_eq!(status, 422);
    // A client that stops sending its body fails the read after the idle
    // bound, not the request lifetime.
    let started = Instant::now();
    let (status, _) = upload(&server, head, &[b"2\r\nhi\r\n"], Duration::from_millis(10));
    assert_eq!(status, 422);
    assert!(started.elapsed() < Duration::from_secs(10));
    server.finish();
}

#[test]
fn a_client_that_stops_reading_loses_its_connection_after_the_write_idle_bound() {
    let server = start(lowered(HostLimits {
        write_idle_ms: Some(300),
        ..HostLimits::default()
    }));
    let mut stream = TcpStream::connect(server.addr).unwrap();
    stream
        .write_all(b"GET /flood HTTP/1.1\r\nHost: example.test\r\n\r\n")
        .unwrap();
    // The endless body fills the socket buffers; the server's write then
    // stalls and the connection closes.
    thread::sleep(Duration::from_millis(2500));
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let mut received = 0usize;
    let mut buffer = vec![0; 65536];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => received += read,
        }
        assert!(
            received < 256 * 1024 * 1024,
            "the server kept writing to a client that did not read"
        );
    }
    assert!(received > 0);
    server.finish();
}

#[test]
fn host_policies_lower_server_limits_and_cannot_raise_them() {
    let server = start(lowered(HostLimits {
        keep_alive_requests: Some(1),
        ..HostLimits::default()
    }));
    // The first answer on a keep-alive connection is its last.
    let (status, headers, _) = server.raw(b"GET /none HTTP/1.1\r\nHost: example.test\r\n\r\n");
    assert_eq!(status, 200);
    assert!(headers.to_ascii_lowercase().contains("connection: close"));
    server.finish();
    // A policy that raises a limit fails before any socket is bound.
    let mut options = lowered(HostLimits {
        workers: Some(varta_common::http::MAX_CONCURRENT_REQUESTS + 1),
        ..HostLimits::default()
    });
    let bytes = bytes();
    let runtime = VartaRuntime::new();
    let endpoint = runtime
        .inspect_module(&bytes)
        .unwrap()
        .http_service
        .unwrap()
        .bind;
    options.requested.push(grant(endpoint));
    let report = runtime.run_with(&bytes, options);
    assert!(
        matches!(
            report.failure,
            Some(RuntimeError::Http(HttpError::Policy(_)))
        ),
        "{:?}",
        report.failure
    );
}

#[test]
fn every_handler_result_form_takes_a_web_request() {
    for (source, expected) in [
        (
            "function handle(request: Request): VartaHttpResponse { return { status: 200, body: request.method + ' ' + request.url, headers: [] }; }",
            "GET http://example.test/form",
        ),
        (
            "async function handle(request: Request): Promise<VartaHttpResponse> { return { status: 200, body: await request.text(), headers: [] }; }",
            "",
        ),
        (
            "function handle(request: Request): Response { return new Response(request.headers.get('x-form') ?? 'missing'); }",
            "missing",
        ),
    ] {
        let source = format!(
            "{source}\nexport function main(): void {{}}\nvarta.http.serve({{ hostname: '127.0.0.1', port: 0 }}, handle);"
        );
        let bytes = varta_compiler::compile_typescript_with_profile(&source, P::CheckedF64Utf16V38)
            .unwrap();
        let contract = VartaRuntime::new()
            .inspect_module(&bytes)
            .unwrap()
            .http_service
            .unwrap();
        assert!(contract.web_request && contract.asynchronous, "{source}");
        let server = Server::start_bytes(bytes, Vec::new());
        let (status, headers, body) = server.request("GET", "/form", b"");
        let body = if headers.to_ascii_lowercase().contains("chunked") {
            dechunk(&body)
        } else {
            body
        };
        assert_eq!((status, body.as_str()), (200, expected), "{source}");
        server.finish();
    }
}
