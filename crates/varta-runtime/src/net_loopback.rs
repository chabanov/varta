//! Test-only loopback HTTP/1.1 server for the v37 and v38 network request
//! tests. It
//! binds 127.0.0.1 on an ephemeral port, never reaches another host, and
//! counts accepted connections so denial tests can prove that no socket was
//! opened. Routes mirror `net_cases/server.mjs`, which serves the same
//! responses to Node 24 for the pinned expectations; `/held`, `/trickle` and
//! `/gate` exist only here, for the connection-close tests, and its trace
//! ([`Seen`]) records when each request arrived and when the client closed a
//! held connection.
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// Bytes of the oversized body route, beyond the response body limit.
pub(crate) const BIG_BODY_BYTES: usize = varta_common::net::MAX_RESPONSE_BODY_BYTES + 1;
/// Value bytes of the `x-pad` field of the `/pad1` route.
pub(crate) const PAD_BYTES: usize = 4096;
/// Pause between the chunks of the streaming routes.
const CHUNK_PAUSE: Duration = Duration::from_millis(20);
/// How long a route that never answers keeps its connection.
pub(crate) const SILENCE: Duration = Duration::from_secs(8);
/// The `User-Agent` marker `/agent` looks for: present only when a host sent
/// the guest's own value.
pub(crate) const AGENT_MARKER: &str = "varta-test-sdk";

/// One server-sent event of the `/events` and `/stall` routes: `size` data
/// bytes, then its id, so the stream's last bytes name its last event.
pub(crate) fn event(id: u64, size: usize) -> Vec<u8> {
    format!("data: {}\nid: {id}\n\n", "x".repeat(size)).into_bytes()
}

/// One observation of the server, in [`Loopback::trace`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Seen {
    /// A request for this path (without its query) was read.
    Arrived(String),
    /// The client ended the held connection of this path (its end of
    /// stream or a reset; a read timeout records nothing).
    Closed(String),
    /// The server answered this path (only [`GATE`] records it).
    Answered(String),
}
/// The path of the route that never answers.
pub(crate) const HELD: &str = "/held";
/// The path of the route that sends one event and then keeps silent.
pub(crate) const TRICKLE: &str = "/trickle";
/// The path of the route that answers once a request for [`HELD`] arrived.
pub(crate) const GATE: &str = "/gate";
/// The trace, and the signal of each new observation (`/gate` waits on it).
#[derive(Default)]
struct TraceLog {
    seen: Mutex<Vec<(Seen, Instant)>>,
    changed: Condvar,
}
impl TraceLog {
    fn lock(&self) -> MutexGuard<'_, Vec<(Seen, Instant)>> {
        // Recording only pushes a complete entry, so a poisoned lock still
        // holds a sound trace.
        self.seen.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
type Trace = Arc<TraceLog>;

pub(crate) struct Loopback {
    pub base: String,
    accepted: Arc<AtomicUsize>,
    trace: Trace,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    tls: Option<Arc<rustls::ServerConfig>>,
}

impl Loopback {
    pub(crate) fn start() -> Self {
        Self::serve(None)
    }
    /// An HTTPS server with a test-generated certificate for 127.0.0.1 and
    /// localhost; returns the server and the DER root to trust.
    pub(crate) fn start_tls() -> (Self, rustls::pki_types::CertificateDer<'static>) {
        let certified =
            rcgen::generate_simple_self_signed(vec!["127.0.0.1".into(), "localhost".into()])
                .unwrap();
        let der = certified.cert.der().clone();
        let key =
            rustls::pki_types::PrivateKeyDer::Pkcs8(certified.key_pair.serialize_der().into());
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![der.clone()], key)
            .unwrap();
        (Self::serve(Some(Arc::new(config))), der)
    }
    fn serve(tls: Option<Arc<rustls::ServerConfig>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let scheme = if tls.is_some() { "https" } else { "http" };
        let accepted = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let trace = Trace::default();
        let thread = {
            let (accepted, stop, tls) = (accepted.clone(), stop.clone(), tls.clone());
            let trace = trace.clone();
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    let Ok(stream) = stream else { continue };
                    accepted.fetch_add(1, Ordering::SeqCst);
                    // No read waits longer than a route that never answers.
                    let _ = stream.set_read_timeout(Some(SILENCE));
                    let (tls, trace) = (tls.clone(), trace.clone());
                    std::thread::spawn(move || match tls {
                        None => answer(stream, &trace),
                        Some(config) => {
                            let connection = rustls::ServerConnection::new(config).unwrap();
                            answer(rustls::StreamOwned::new(connection, stream), &trace);
                        }
                    });
                }
            })
        };
        Self {
            base: format!("{scheme}://{address}"),
            accepted,
            trace,
            stop,
            thread: Some(thread),
            tls,
        }
    }
    /// Connections accepted so far.
    pub(crate) fn accepted(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
    }
    /// Everything seen so far, in the order it was recorded.
    pub(crate) fn trace(&self) -> Vec<(Seen, Instant)> {
        self.trace.lock().clone()
    }
}

fn record(trace: &Trace, seen: Seen) {
    trace.lock().push((seen, Instant::now()));
    trace.changed.notify_all();
}

/// Keep the connection until the client ends it, then record that; a client
/// that keeps it for [`SILENCE`] (the read timeout) is not recorded.
fn hold<S: Read>(stream: &mut S, trace: &Trace, path: String) {
    let mut byte = [0];
    loop {
        match stream.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return;
            }
            // A reset ends the connection as well.
            Err(_) => break,
        }
    }
    record(trace, Seen::Closed(path));
}

/// Whether a request for [`HELD`] arrived, waiting up to [`SILENCE`].
fn held_arrived(trace: &Trace) -> bool {
    let arrived = Seen::Arrived(HELD.to_owned());
    let (seen, _) = trace
        .changed
        .wait_timeout_while(trace.lock(), SILENCE, |seen| {
            !seen.iter().any(|(observed, _)| *observed == arrived)
        })
        .unwrap_or_else(PoisonError::into_inner);
    seen.iter().any(|(observed, _)| *observed == arrived)
}

impl Drop for Loopback {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the accept loop so it observes the stop flag.
        let address = self.base.rsplit('/').next().unwrap_or_default().to_owned();
        let _ = TcpStream::connect(address);
        let _ = self.tls.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
    /// The body as received (`/mirror` answers with it).
    raw: Vec<u8>,
}

fn read_request<S: Read>(stream: S) -> Option<Request> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_owned();
    let path = parts.next()?.to_owned();
    let mut headers = Vec::new();
    let mut length = 0;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':')?;
        let (name, value) = (name.trim().to_ascii_lowercase(), value.trim().to_owned());
        if name == "content-length" {
            length = value.parse().ok()?;
        }
        headers.push((name, value));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(Request {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
        raw: body,
    })
}

fn header<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

fn json_text(value: Option<&str>) -> String {
    serde_json::to_string(&value).unwrap()
}

fn answer<S: Read + Write>(mut stream: S, trace: &Trace) {
    let Some(request) = read_request(&mut stream) else {
        return;
    };
    let fixed = |stream: &mut S, status: &str, headers: &str, body: &[u8]| {
        let head = format!(
            "HTTP/1.1 {status}\r\n{headers}content-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.write_all(body);
        let _ = stream.flush();
    };
    let chunked = |stream: &mut S, content_type: &str, chunks: &[&[u8]]| {
        let head = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: {content_type}\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n"
        );
        let _ = stream.write_all(head.as_bytes());
        let _ = stream.flush();
        for chunk in chunks {
            std::thread::sleep(CHUNK_PAUSE);
            let _ = stream.write_all(format!("{:x}\r\n", chunk.len()).as_bytes());
            let _ = stream.write_all(chunk);
            let _ = stream.write_all(b"\r\n");
            let _ = stream.flush();
        }
        let _ = stream.write_all(b"0\r\n\r\n");
        let _ = stream.flush();
    };
    let path = request
        .path
        .split('?')
        .next()
        .unwrap_or_default()
        .to_owned();
    record(trace, Seen::Arrived(path.clone()));
    match path.as_str() {
        "/hello" => fixed(
            &mut stream,
            "200 OK",
            "content-type: text/plain\r\nx-test: a\r\n",
            b"hello world",
        ),
        "/json" => fixed(
            &mut stream,
            "201 Created",
            "content-type: application/json\r\n",
            br#"{"ok":true,"items":[1,2,3]}"#,
        ),
        "/echo" => {
            let body = format!(
                r#"{{"method":{},"auth":{},"type":{},"query":{},"body":{}}}"#,
                json_text(Some(&request.method)),
                json_text(header(&request, "authorization")),
                json_text(header(&request, "content-type")),
                json_text(request.path.split_once('?').map(|(_, query)| query)),
                json_text(Some(&request.body)),
            );
            fixed(
                &mut stream,
                "200 OK",
                "content-type: application/json\r\n",
                body.as_bytes(),
            );
        }
        "/sse" => chunked(
            &mut stream,
            "text/event-stream",
            &[
                b"id: 1\ndata: tick 1\n\n",
                b"id: 2\ndata: tick 2\n\n",
                b"id: 3\ndata: tick 3\n\n",
            ],
        ),
        "/utf8" => {
            let bytes = "é€😀".as_bytes();
            chunked(
                &mut stream,
                "text/plain; charset=utf-8",
                &[&bytes[..1], &bytes[1..4], &bytes[4..]],
            );
        }
        // V38 byte bodies: bytes that are no UTF-8, and the request body
        // echoed with its content type (none when the request had none).
        "/bytes" => fixed(
            &mut stream,
            "200 OK",
            "content-type: application/octet-stream\r\n",
            &[0x00, 0xFF, 0x80, 0x41],
        ),
        "/mirror" => {
            let content_type = header(&request, "content-type")
                .map(|value| format!("content-type: {value}\r\n"))
                .unwrap_or_default();
            fixed(&mut stream, "200 OK", &content_type, &request.raw);
        }
        // V38 canonical heads: names out of order and in mixed case, repeats
        // split by other fields, cookies and an undeclared field.
        "/headers" => fixed(
            &mut stream,
            "200 OK",
            "x-b: 1\r\ncontent-type: text/plain\r\nset-cookie: a=1\r\nx-a: z\r\nX-B: 2\r\nset-cookie: b=2\r\ncache-control: no-store\r\nx-hidden: secret\r\n",
            b"h",
        ),
        // The same head with (`/pad1`) and without (`/pad0`) a large field.
        "/pad0" | "/pad1" => {
            let pad = if path == "/pad1" {
                format!("x-pad: {}\r\n", "p".repeat(PAD_BYTES))
            } else {
                String::new()
            };
            fixed(
                &mut stream,
                "200 OK",
                &format!("content-type: text/plain\r\n{pad}"),
                b"h",
            );
        }
        "/redirect" => fixed(&mut stream, "302 Found", "location: /hello\r\n", b""),
        "/missing" => fixed(
            &mut stream,
            "404 Not Found",
            "content-type: text/plain\r\n",
            b"nope",
        ),
        "/big" => fixed(
            &mut stream,
            "200 OK",
            "content-type: text/plain\r\n",
            &vec![b'a'; BIG_BODY_BYTES],
        ),
        "/sized" => {
            let size = request
                .path
                .split_once("?n=")
                .and_then(|(_, n)| n.parse().ok())
                .unwrap_or(0);
            fixed(
                &mut stream,
                "200 OK",
                "content-type: text/plain\r\n",
                &vec![b'a'; size],
            );
        }
        "/slow" => std::thread::sleep(SILENCE),
        // Never answers; records when the client ends the connection.
        HELD => hold(&mut stream, trace, path),
        // Sends a head and one event, then keeps silent until the client
        // ends the connection, which it records.
        TRICKLE => {
            let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n";
            let chunk = event(1, 1);
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(format!("{:x}\r\n", chunk.len()).as_bytes());
            let _ = stream.write_all(&chunk);
            let _ = stream.write_all(b"\r\n");
            let _ = stream.flush();
            hold(&mut stream, trace, path);
        }
        // Answers once a request for `/held` arrived, so a client can act on
        // a held request the server has certainly seen.
        GATE => {
            if held_arrived(trace) {
                record(trace, Seen::Answered(path));
                fixed(
                    &mut stream,
                    "200 OK",
                    "content-type: text/plain\r\n",
                    b"open",
                );
            } else {
                fixed(&mut stream, "504 Gateway Timeout", "", b"");
            }
        }
        // V38 long-lived streams: `count` events of `size` data bytes, one
        // every `pause` ms; `/stall` sends one event, keeps silent for `gap`
        // ms, then sends a second.
        "/events" => {
            let pause = Duration::from_millis(parameter(&request, "pause"));
            let size = usize::try_from(parameter(&request, "size")).unwrap();
            let events: Vec<(Duration, Vec<u8>)> = (1..=parameter(&request, "count"))
                .map(|id| (pause, event(id, size)))
                .collect();
            event_stream(&mut stream, &events);
        }
        "/stall" => {
            let gap = Duration::from_millis(parameter(&request, "gap"));
            event_stream(
                &mut stream,
                &[(Duration::ZERO, event(1, 1)), (gap, event(2, 1))],
            );
        }
        // V38 host-owned request headers: whether the guest's `User-Agent`
        // arrived (`/agent`), and the values that did arrive (`/sent`).
        "/agent" => {
            let sent = header(&request, "user-agent").is_some_and(|ua| ua.contains(AGENT_MARKER));
            fixed(
                &mut stream,
                "200 OK",
                "content-type: application/json\r\n",
                format!(r#"{{"guestAgent":{sent}}}"#).as_bytes(),
            );
        }
        "/sent" => {
            let body = format!(
                r#"{{"agent":{},"encoding":{}}}"#,
                json_text(header(&request, "user-agent")),
                json_text(header(&request, "accept-encoding")),
            );
            fixed(
                &mut stream,
                "200 OK",
                "content-type: application/json\r\n",
                body.as_bytes(),
            );
        }
        // A content coding the request did not ask for (the body is not
        // really compressed: only the head is inspected).
        "/coded" => fixed(
            &mut stream,
            "200 OK",
            "content-type: text/plain\r\ncontent-encoding: gzip\r\n",
            b"coded",
        ),
        // `server.mjs` compresses this text when the request accepts gzip;
        // the native host asks for the identity coding, so only that branch
        // exists here and any other request is refused.
        "/compressed" => {
            let identity = header(&request, "accept-encoding")
                .is_some_and(|value| value == varta_common::net::IDENTITY_ENCODING);
            if identity {
                fixed(
                    &mut stream,
                    "200 OK",
                    "content-type: text/plain\r\n",
                    b"compressed text",
                );
            } else {
                fixed(&mut stream, "406 Not Acceptable", "", b"");
            }
        }
        _ => fixed(&mut stream, "500 Internal Server Error", "", b""),
    }
}

/// A numeric query parameter of the request (0 when absent).
fn parameter(request: &Request, name: &str) -> u64 {
    let query = request.path.split_once('?').map_or("", |(_, query)| query);
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == name)
        .and_then(|(_, value)| value.parse().ok())
        .unwrap_or(0)
}

/// A chunked `text/event-stream`: each chunk after its pause.
fn event_stream<S: Write>(stream: &mut S, chunks: &[(Duration, Vec<u8>)]) {
    let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n";
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.flush();
    for (pause, chunk) in chunks {
        std::thread::sleep(*pause);
        let _ = stream.write_all(format!("{:x}\r\n", chunk.len()).as_bytes());
        let _ = stream.write_all(chunk);
        let _ = stream.write_all(b"\r\n");
        let _ = stream.flush();
    }
    let _ = stream.write_all(b"0\r\n\r\n");
    let _ = stream.flush();
}
