//! Test-only loopback HTTP/1.1 server for the v37 network request tests. It
//! binds 127.0.0.1 on an ephemeral port, never reaches another host, and
//! counts accepted connections so denial tests can prove that no socket was
//! opened. Routes mirror `net_cases/server.mjs`, which serves the same
//! responses to Node 24 for the pinned expectations.
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

/// Bytes of the oversized body route, beyond the response body limit.
pub(crate) const BIG_BODY_BYTES: usize = varta_common::net::MAX_RESPONSE_BODY_BYTES + 1;
/// Value bytes of the `x-pad` field of the `/pad1` route.
pub(crate) const PAD_BYTES: usize = 4096;
/// Pause between the chunks of the streaming routes.
const CHUNK_PAUSE: Duration = Duration::from_millis(20);
/// How long a route that never answers keeps its connection.
const SILENCE: Duration = Duration::from_secs(8);

pub(crate) struct Loopback {
    pub base: String,
    accepted: Arc<AtomicUsize>,
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
        let thread = {
            let (accepted, stop, tls) = (accepted.clone(), stop.clone(), tls.clone());
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    let Ok(stream) = stream else { continue };
                    accepted.fetch_add(1, Ordering::SeqCst);
                    let tls = tls.clone();
                    std::thread::spawn(move || match tls {
                        None => answer(stream),
                        Some(config) => {
                            let connection = rustls::ServerConnection::new(config).unwrap();
                            answer(rustls::StreamOwned::new(connection, stream));
                        }
                    });
                }
            })
        };
        Self {
            base: format!("{scheme}://{address}"),
            accepted,
            stop,
            thread: Some(thread),
            tls,
        }
    }
    /// Connections accepted so far.
    pub(crate) fn accepted(&self) -> usize {
        self.accepted.load(Ordering::SeqCst)
    }
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

fn answer<S: Read + Write>(mut stream: S) {
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
        _ => fixed(&mut stream, "500 Internal Server Error", "", b""),
    }
}
