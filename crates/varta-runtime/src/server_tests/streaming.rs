//! V2 streaming responses (a handler returning a Web `Response`): chunks
//! reach the client as their pulls complete, other requests are served
//! while a stream is open, a client that leaves or a service shutdown ends
//! the stream, and a failure after the head aborts the body.
use super::*;
use std::io::{BufRead, BufReader};
use std::time::Instant;
use varta_common::language::LanguageProfile as P;

const STREAMING: &str = r#"
function handle(request: VartaHttpRequest): Response {
  if (request.target == "/text") {
    return new Response("plain", { headers: { "content-type": "text/plain" } });
  }
  let count = 0;
  const delay = request.target == "/slow" ? 30000 : 200;
  const stream = new ReadableStream<Uint8Array>({
    pull: async (controller: ReadableStreamDefaultController<Uint8Array>): Promise<void> => {
      count += 1;
      if (count > 3) {
        controller.close();
        return;
      }
      if (count > 1) {
        if (request.target == "/fail") {
          throw new Error("boom");
        }
        await new Promise<void>((resolve) => {
          setTimeout(resolve, delay);
        });
      }
      controller.enqueue(new TextEncoder().encode("data: " + String(count) + "\n\n"));
    },
  });
  return new Response(stream, { headers: { "content-type": "text/event-stream" } });
}
export function main(): void {}
varta.http.serve({ hostname: "127.0.0.1", port: 0 }, handle);
"#;

fn timers() -> CapabilityGrant {
    CapabilityGrant {
        id: "time.timer".into(),
        value: CapabilityValue::Boolean(true),
    }
}

fn start() -> Server {
    Server::start_with_grants(STREAMING, P::CheckedF64Utf16V38, vec![timers()])
}

/// An open chunked response: its status line and headers, read so far.
struct Open {
    reader: BufReader<TcpStream>,
    head: String,
}
impl Open {
    fn get(server: &Server, target: &str) -> Self {
        let mut stream = TcpStream::connect(server.addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(15)))
            .unwrap();
        stream
            .write_all(
                format!("GET {target} HTTP/1.1\r\nHost: example.test\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .unwrap();
        let mut reader = BufReader::new(stream);
        let mut head = String::new();
        loop {
            let mut line = String::new();
            assert_ne!(reader.read_line(&mut line).unwrap(), 0, "{head}");
            if line == "\r\n" {
                break;
            }
            head.push_str(&line);
        }
        Self { reader, head }
    }
    /// The next chunk's data, `Some("")` at the terminating chunk, `None`
    /// when the connection closed without one.
    fn chunk(&mut self) -> Option<String> {
        let mut size = String::new();
        if self.reader.read_line(&mut size).ok()? == 0 {
            return None;
        }
        let size = usize::from_str_radix(size.trim_end(), 16).ok()?;
        let mut data = vec![0; size + 2];
        self.reader.read_exact(&mut data).ok()?;
        data.truncate(size);
        Some(String::from_utf8(data).unwrap())
    }
}

#[test]
fn chunks_reach_the_client_as_their_pulls_complete() {
    let server = start();
    let mut open = Open::get(&server, "/events");
    assert!(open.head.starts_with("HTTP/1.1 200"), "{}", open.head);
    let lower = open.head.to_ascii_lowercase();
    assert!(lower.contains("content-type: text/event-stream"), "{lower}");
    assert!(lower.contains("transfer-encoding: chunked"), "{lower}");
    assert_eq!(open.chunk().as_deref(), Some("data: 1\n\n"));
    let first = Instant::now();
    assert_eq!(open.chunk().as_deref(), Some("data: 2\n\n"));
    assert_eq!(open.chunk().as_deref(), Some("data: 3\n\n"));
    // Two timer waits of 200ms separate the chunks.
    assert!(
        first.elapsed() >= Duration::from_millis(350),
        "{:?}",
        first.elapsed()
    );
    assert_eq!(open.chunk().as_deref(), Some(""));
    // A text body streams too; HEAD sends the head only.
    let (status, headers, body) = server.request("GET", "/text", b"");
    assert_eq!(
        (status, body.as_str()),
        (200, "5\r\nplain\r\n0\r\n\r\n"),
        "{headers}"
    );
    let (status, headers, body) = server.request("HEAD", "/events", b"");
    assert_eq!(status, 200);
    assert!(!headers.to_ascii_lowercase().contains("content-length"));
    assert!(body.is_empty());
    let report = server_report(&server);
    server.finish();
    assert!(
        report.iter().all(|request| matches!(
            request.outcome,
            HttpRequestOutcome::Returned { status: 200 }
        )),
        "{report:?}"
    );
}

fn server_report(server: &Server) -> Vec<HttpRequestReport> {
    server.requests.lock().unwrap().clone()
}

#[test]
fn other_requests_are_served_while_a_stream_is_open_and_a_leaving_client_ends_it() {
    let server = start();
    let mut open = Open::get(&server, "/slow");
    assert_eq!(open.chunk().as_deref(), Some("data: 1\n\n"));
    // The stream now waits 30 seconds for its next chunk.
    let started = Instant::now();
    let (status, _, _) = server.request("GET", "/text", b"");
    assert_eq!(status, 200);
    assert!(started.elapsed() < Duration::from_secs(5));
    drop(open);
    // The cancelled worker stops at its next check and frees its permit:
    // every other slot keeps serving meanwhile.
    for _ in 0..varta_common::http::MAX_CONCURRENT_REQUESTS {
        assert_eq!(server.request("GET", "/text", b"").0, 200);
    }
    server.finish();
}

#[test]
fn shutdown_ends_an_open_stream_and_a_failure_aborts_the_body() {
    let server = start();
    let mut failing = Open::get(&server, "/fail");
    assert_eq!(failing.chunk().as_deref(), Some("data: 1\n\n"));
    // No terminating chunk: the connection closes.
    assert_eq!(failing.chunk(), None);
    let mut open = Open::get(&server, "/slow");
    assert_eq!(open.chunk().as_deref(), Some("data: 1\n\n"));
    let started = Instant::now();
    let stop = server.stop.clone();
    stop.shutdown();
    assert_eq!(open.chunk(), None);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    server.finish();
}

/// One HTTP/1.1 response on a kept-alive connection, read by its
/// `content-length`.
fn exchange(reader: &mut BufReader<TcpStream>, target: &str) -> (String, String) {
    reader
        .get_mut()
        .write_all(format!("GET {target} HTTP/1.1\r\nHost: example.test\r\n\r\n").as_bytes())
        .unwrap();
    let mut head = String::new();
    loop {
        let mut line = String::new();
        assert_ne!(reader.read_line(&mut line).unwrap(), 0, "{head}");
        if line == "\r\n" {
            break;
        }
        head.push_str(&line);
    }
    let lower = head.to_ascii_lowercase();
    let mut body = String::new();
    if let Some(length) = lower
        .lines()
        .find_map(|line| line.strip_prefix("content-length: "))
    {
        let mut bytes = vec![0; length.trim().parse().unwrap()];
        reader.read_exact(&mut bytes).unwrap();
        body = String::from_utf8(bytes).unwrap();
    } else {
        // A chunked body: read chunks until the terminating one.
        loop {
            let mut size = String::new();
            reader.read_line(&mut size).unwrap();
            let size = usize::from_str_radix(size.trim_end(), 16).unwrap();
            let mut data = vec![0; size + 2];
            reader.read_exact(&mut data).unwrap();
            if size == 0 {
                break;
            }
            body.push_str(std::str::from_utf8(&data[..size]).unwrap());
        }
    }
    (head, body)
}

#[test]
fn a_kept_alive_connection_serves_several_requests_and_closes_when_idle() {
    let server = start();
    let stream = TcpStream::connect(server.addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    let mut reader = BufReader::new(stream);
    for _ in 0..3 {
        let (head, body) = exchange(&mut reader, "/text");
        assert!(head.starts_with("HTTP/1.1 200"), "{head}");
        assert!(
            !head.to_ascii_lowercase().contains("connection: close"),
            "{head}"
        );
        assert_eq!(body, "plain");
    }
    // Idle for longer than the keep-alive bound: the server closes.
    let idle = Duration::from_secs(varta_common::http::KEEP_ALIVE_IDLE_SECONDS);
    let started = Instant::now();
    let mut rest = Vec::new();
    assert_eq!(reader.read_to_end(&mut rest).unwrap(), 0);
    let waited = started.elapsed();
    assert!(waited >= idle - Duration::from_millis(500), "{waited:?}");
    assert!(waited < idle + Duration::from_secs(3), "{waited:?}");
    // An idle close is no rejection: nothing is reported.
    assert!(
        !server
            .diagnostics
            .lock()
            .unwrap()
            .iter()
            .any(|line| line.contains("rejected")),
        "{:?}",
        server.diagnostics.lock().unwrap()
    );
    server.finish();
}

#[test]
fn a_request_that_finds_no_worker_in_time_gets_503_with_retry_after() {
    // A host lowers the slots to two and the wait to a second.
    const SLOTS: usize = 2;
    let wait = Duration::from_secs(1);
    let bytes =
        varta_compiler::compile_typescript_with_profile(STREAMING, P::CheckedF64Utf16V38).unwrap();
    let server = Server::start_options(
        bytes,
        RunOptions {
            requested: vec![timers()],
            http: Some(varta_common::http::HttpPolicy {
                limits: Some(varta_common::http::HostLimits {
                    workers: Some(SLOTS),
                    worker_wait_ms: Some(wait.as_millis() as u64),
                    ..Default::default()
                }),
                tls: None,
            }),
            ..RunOptions::default()
        },
    );
    // Every slot holds a stream that waits 30 seconds for its next chunk.
    let open: Vec<Open> = (0..SLOTS)
        .map(|_| {
            let mut open = Open::get(&server, "/slow");
            assert_eq!(open.chunk().as_deref(), Some("data: 1\n\n"));
            open
        })
        .collect();
    let started = Instant::now();
    let (status, headers, _) = server.request("GET", "/text", b"");
    let waited = started.elapsed();
    assert_eq!(status, 503, "{headers}");
    assert!(
        headers.to_ascii_lowercase().contains("retry-after: 1"),
        "{headers}"
    );
    assert!(waited >= wait - Duration::from_millis(100), "{waited:?}");
    drop(open);
    server.finish();
}

/// A handler that keeps `count` distinct strings of 64 Ki code units alive:
/// four fit one request's memory, 256 need far more.
const RETAINING: &str = r#"
function handle(request: VartaHttpRequest): Response {
  const count = request.target == "/large" ? 256 : 4;
  const block = "x".repeat(65536);
  const kept: string[] = [];
  for (let i = 0; i < count; i++) {
    kept.push(`${block}${i}`);
  }
  return new Response(`kept ${kept.length}`);
}
export function main(): void {}
varta.http.serve({ hostname: "127.0.0.1", port: 0 }, handle);
"#;

/// The chunked body of a request that kept four strings.
const KEPT_FOUR: &str = "6\r\nkept 4\r\n0\r\n\r\n";

#[test]
fn a_request_keeps_the_request_memory_under_the_v38_application_memory() {
    let server = Server::start_with_grants(RETAINING, P::CheckedF64Utf16V38, Vec::new());
    let (status, _, body) = server.request("GET", "/small", b"");
    assert_eq!((status, body.as_str()), (200, KEPT_FOUR));
    let (status, _, _) = server.request("GET", "/large", b"");
    assert_eq!(status, 500);
    let (status, _, body) = server.request("GET", "/small", b"");
    assert_eq!((status, body.as_str()), (200, KEPT_FOUR));
    let diagnostics = server.diagnostics.lock().unwrap().join("\n");
    assert!(
        diagnostics.contains("resource limit: memory budget exhausted"),
        "{diagnostics}"
    );
    server.finish();
}
