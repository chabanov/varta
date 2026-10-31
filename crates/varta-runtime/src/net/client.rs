//! The HTTP/1.1 client of one request: a task on the execution's
//! current-thread Tokio runtime. It runs only while the event loop waits, so
//! no network effect happens while guest code runs, and dropping the task
//! (abort or the end of the execution) closes its connection at once.
//!
//! Name resolution, connect, TLS, the request and every body read share the
//! request's deadline (the remaining execution deadline). The task reports
//! the head once, then one bounded chunk per pull, so a slow guest applies
//! backpressure instead of buffering the body in the host.
use super::{Completion, Event};
use hyper::body::{Body, Bytes, Incoming};
use hyper::header::{HeaderName, HeaderValue};
use std::sync::Arc;
use std::task::Poll;
use std::time::Instant;
use tokio::sync::mpsc;
use varta_common::net::{
    CompletionKind, MAX_CHUNK_BYTES, MAX_RESPONSE_BODY_BYTES, MAX_RESPONSE_HEADER_BYTES,
    MAX_RESPONSE_HEADERS, RequestFailure,
};

/// A validated request, ready for the transport.
pub(super) struct Plan {
    pub url: url::Url,
    pub method: hyper::Method,
    pub headers: Vec<(HeaderName, HeaderValue)>,
    pub body: Option<Bytes>,
    pub deadline: Instant,
    /// Present exactly for `https` URLs.
    pub tls: Option<Arc<rustls::ClientConfig>>,
}

pub(super) const HTTPS: &str = "https";
/// The only application protocol offered over TLS.
const HTTP1_ALPN: &[u8] = b"http/1.1";
/// The UTF-8 replacement for invalid or truncated sequences.
const REPLACEMENT: char = '\u{FFFD}';

/// How one pulled chunk is delivered: decoded as UTF-8 text
/// (`net_body_read`), or as its bytes, one code unit per byte
/// (`net_body_read_bytes`, v38).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReadMode {
    Text,
    Bytes,
}

/// TLS client settings: the compiled-in Mozilla roots (plus test-only roots in
/// runtime tests), the ring provider and HTTP/1.1 only.
pub(super) fn tls_config() -> Result<Arc<rustls::ClientConfig>, String> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    #[cfg(test)]
    for root in super::test_roots() {
        roots.add(root).map_err(|error| error.to_string())?;
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| error.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![HTTP1_ALPN.to_vec()];
    Ok(Arc::new(config))
}

/// Run one request to completion, reporting through `events`.
pub(super) async fn run(
    id: u64,
    plan: Plan,
    events: mpsc::UnboundedSender<Event>,
    mut pull: mpsc::Receiver<ReadMode>,
) {
    let deadline = tokio::time::Instant::from_std(plan.deadline);
    let send = |kind: CompletionKind, text: String| {
        // The receiver lives as long as the execution; a closed channel means
        // the execution ended, so there is nobody left to tell.
        let _ = events.send(Event {
            id,
            completion: Completion { kind, text },
        });
    };
    let fail = |failure: RequestFailure| send(CompletionKind::Failed, failure.to_string());
    let response = match tokio::time::timeout_at(deadline, connect_and_send(&plan)).await {
        Ok(Ok(response)) => response,
        Ok(Err(failure)) => return fail(failure),
        Err(_) => return fail(RequestFailure::Timeout),
    };
    let (parts, body, _connection) = response;
    send(CompletionKind::Head, head(&plan.url, &parts));
    let mut reader = BodyReader::new(body);
    loop {
        let Some(mode) = pull.recv().await else {
            return;
        };
        match tokio::time::timeout_at(deadline, reader.next(mode)).await {
            Ok(Ok(Some(text))) => send(CompletionKind::Chunk, text),
            Ok(Ok(None)) => return send(CompletionKind::End, String::new()),
            Ok(Err(failure)) => return fail(failure),
            Err(_) => return fail(RequestFailure::BodyTimeout),
        }
    }
}

/// Aborts the connection task when the request task ends or is dropped.
struct Connection(tokio::task::JoinHandle<()>);
impl Drop for Connection {
    fn drop(&mut self) {
        self.0.abort();
    }
}

type Response = (hyper::http::response::Parts, Incoming, Connection);

async fn connect_and_send(plan: &Plan) -> Result<Response, RequestFailure> {
    let host = plan.url.host_str().ok_or(RequestFailure::NoHost)?;
    let port = plan
        .url
        .port_or_known_default()
        .ok_or(RequestFailure::NoPort)?;
    // An IPv6 host keeps its brackets in the URL; the socket needs the bare address.
    let address = host.trim_start_matches('[').trim_end_matches(']');
    let tcp = tokio::net::TcpStream::connect((address, port))
        .await
        .map_err(|error| RequestFailure::Connect(error.to_string()))?;
    // Latency matters more than packet count for request/response traffic.
    let _ = tcp.set_nodelay(true);
    if let Some(config) = &plan.tls {
        let name = rustls::pki_types::ServerName::try_from(address.to_owned())
            .map_err(|error| RequestFailure::ServerName(error.to_string()))?;
        let tls = tokio_rustls::TlsConnector::from(config.clone())
            .connect(name, tcp)
            .await
            .map_err(|error| RequestFailure::TlsHandshake(error.to_string()))?;
        exchange(plan, hyper_util::rt::TokioIo::new(tls)).await
    } else {
        exchange(plan, hyper_util::rt::TokioIo::new(tcp)).await
    }
}

async fn exchange<T>(plan: &Plan, io: T) -> Result<Response, RequestFailure>
where
    T: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
{
    let mut builder = hyper::client::conn::http1::Builder::new();
    builder
        .max_headers(MAX_RESPONSE_HEADERS)
        .max_buf_size(MAX_RESPONSE_HEADER_BYTES);
    let (mut sender, connection) = builder
        .handshake::<_, http_body_util::Full<Bytes>>(io)
        .await
        .map_err(|error| RequestFailure::HttpHandshake(error.to_string()))?;
    let connection = Connection(tokio::spawn(async move {
        // The outcome reaches the request through its response or body.
        let _ = connection.await;
    }));
    let mut request = hyper::Request::builder()
        .method(plan.method.clone())
        .uri(&plan.url[url::Position::BeforePath..url::Position::AfterQuery]);
    let authority = &plan.url[url::Position::BeforeHost..url::Position::AfterPort];
    request = request.header(hyper::header::HOST, authority);
    for (name, value) in &plan.headers {
        request = request.header(name, value);
    }
    let request = request
        .body(http_body_util::Full::new(
            plan.body.clone().unwrap_or_default(),
        ))
        .map_err(|error| RequestFailure::Request(error.to_string()))?;
    let response = sender
        .send_request(request)
        .await
        .map_err(|error| RequestFailure::Send(error.to_string()))?;
    let (parts, body) = response.into_parts();
    Ok((parts, body, connection))
}

/// The head record (`varta_common::net::head_record`). Names are lowercase;
/// the header map yields each name once at its first field, followed by all
/// its values in arrival order.
fn head(url: &url::Url, parts: &hyper::http::response::Parts) -> String {
    let status = parts.status.as_u16();
    let reason = parts
        .extensions
        .get::<hyper::ext::ReasonPhrase>()
        .map(|reason| latin1(reason.as_bytes()))
        .or_else(|| parts.status.canonical_reason().map(str::to_owned))
        .unwrap_or_default();
    let mut headers = Vec::with_capacity(parts.headers.len() * 2);
    for (name, value) in &parts.headers {
        headers.push(name.as_str().to_owned());
        headers.push(latin1(value.as_bytes()));
    }
    let location = parts
        .headers
        .get(hyper::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| url.join(value).ok())
        .map(String::from)
        .unwrap_or_default();
    varta_common::net::head_record(status, &reason, url.as_str(), &location, headers)
}

/// Each byte is one code unit, as for a WebIDL ByteString.
fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| char::from(*byte)).collect()
}

/// Reads a response body in chunks of at most [`MAX_CHUNK_BYTES`] and decodes
/// them as UTF-8, completing a sequence split between chunks from the next
/// one; a sequence still incomplete at the end becomes U+FFFD. A chunk takes
/// every frame already received (up to the bound) without waiting for more,
/// so small frames cost one guest read together while a quiet stream (SSE)
/// still delivers each event as soon as it arrives.
struct BodyReader {
    body: Incoming,
    buffer: Vec<u8>,
    carry: Vec<u8>,
    total: usize,
    ended: bool,
}
impl BodyReader {
    fn new(body: Incoming) -> Self {
        Self {
            body,
            buffer: Vec::new(),
            carry: Vec::new(),
            total: 0,
            ended: false,
        }
    }
    /// The next nonempty chunk (text, or bytes as code units), or `None` at
    /// the end of the body.
    async fn next(&mut self, mode: ReadMode) -> Result<Option<String>, RequestFailure> {
        loop {
            if mode == ReadMode::Bytes && (!self.buffer.is_empty() || !self.carry.is_empty()) {
                // Bytes an earlier text read kept for an unfinished sequence
                // come first, undecoded.
                let mut bytes = std::mem::take(&mut self.carry);
                let take = self
                    .buffer
                    .len()
                    .min(MAX_CHUNK_BYTES.saturating_sub(bytes.len()));
                let rest = self.buffer.split_off(take);
                bytes.extend_from_slice(&std::mem::replace(&mut self.buffer, rest));
                return Ok(Some(latin1(&bytes)));
            }
            if !self.buffer.is_empty() {
                let rest = self
                    .buffer
                    .split_off(self.buffer.len().min(MAX_CHUNK_BYTES));
                let bytes = std::mem::replace(&mut self.buffer, rest);
                let text = self.decode(&bytes);
                if !text.is_empty() {
                    return Ok(Some(text));
                }
                continue;
            }
            if self.ended {
                if self.carry.is_empty() {
                    return Ok(None);
                }
                self.carry.clear();
                return Ok(Some(REPLACEMENT.to_string()));
            }
            std::future::poll_fn(|context| self.fill(context)).await?;
        }
    }
    /// Take every frame that is ready, until the chunk bound or the end; wait
    /// only while nothing was taken.
    fn fill(&mut self, context: &mut std::task::Context<'_>) -> Poll<Result<(), RequestFailure>> {
        while self.buffer.len() < MAX_CHUNK_BYTES {
            match std::pin::Pin::new(&mut self.body).poll_frame(context) {
                Poll::Pending if self.buffer.is_empty() => return Poll::Pending,
                Poll::Pending => break,
                Poll::Ready(None) => {
                    self.ended = true;
                    break;
                }
                Poll::Ready(Some(Err(error))) => {
                    return Poll::Ready(Err(RequestFailure::Body(error.to_string())));
                }
                Poll::Ready(Some(Ok(frame))) => {
                    // Trailers carry no body bytes.
                    let Ok(data) = frame.into_data() else {
                        continue;
                    };
                    self.total = self.total.saturating_add(data.len());
                    if self.total > MAX_RESPONSE_BODY_BYTES {
                        return Poll::Ready(Err(RequestFailure::BodyLimit));
                    }
                    self.buffer.extend_from_slice(&data);
                }
            }
        }
        Poll::Ready(Ok(()))
    }
    fn decode(&mut self, bytes: &[u8]) -> String {
        let mut input = std::mem::take(&mut self.carry);
        input.extend_from_slice(bytes);
        // Keep an incomplete trailing sequence (at most three bytes) for the
        // next chunk; everything before it decodes now.
        let keep = incomplete_suffix(&input);
        self.carry = input.split_off(input.len() - keep);
        String::from_utf8_lossy(&input).into_owned()
    }
}

/// The length of a trailing UTF-8 sequence that is a valid but unfinished
/// prefix (so more bytes could complete it), or 0.
fn incomplete_suffix(bytes: &[u8]) -> usize {
    for back in 1..=bytes.len().min(3) {
        let lead = bytes[bytes.len() - back];
        if lead & 0b1100_0000 == 0b1000_0000 {
            continue;
        }
        let width = match lead {
            0xC2..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF4 => 4,
            _ => return 0,
        };
        let tail = &bytes[bytes.len() - back..];
        // A prefix that can never become valid decodes (lossily) now.
        let mut padded = tail.to_vec();
        padded.resize(width, 0x80);
        let complete = match std::str::from_utf8(&padded) {
            Ok(_) => true,
            Err(error) => error.error_len().is_none(),
        };
        return if back < width && complete { back } else { 0 };
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_records_group_repeated_fields_under_their_first_name() {
        let response = hyper::Response::builder()
            .status(201)
            .header("Set-Cookie", "a=1")
            .header("X-Mark", "\u{e9}".as_bytes())
            .header("set-cookie", "b=2")
            .header("Location", "../next")
            .body(())
            .unwrap();
        let (mut parts, ()) = response.into_parts();
        let url = url::Url::parse("http://a.test/dir/page").unwrap();
        assert_eq!(
            head(&url, &parts),
            concat!(
                r#"{"headers":["set-cookie","a=1","set-cookie","b=2","x-mark","#,
                "\"\u{c3}\u{a9}\",",
                r#""location","../next"],"location":"http://a.test/next","status":201,"#,
                r#""statusText":"Created","url":"http://a.test/dir/page"}"#
            )
        );
        // A received reason phrase other than the canonical one is kept as
        // received, an empty one included.
        parts
            .extensions
            .insert(hyper::ext::ReasonPhrase::from_static(b""));
        assert!(head(&url, &parts).contains(r#""statusText":"","#));
    }

    #[test]
    fn split_sequences_wait_for_their_continuation() {
        let euro = "€".as_bytes();
        assert_eq!(incomplete_suffix(&euro[..1]), 1);
        assert_eq!(incomplete_suffix(&euro[..2]), 2);
        assert_eq!(incomplete_suffix(euro), 0);
        let face = "😀".as_bytes();
        assert_eq!(incomplete_suffix(&face[..3]), 3);
        assert_eq!(incomplete_suffix(b"ab"), 0);
        // A lead that can never be valid, or an overlong prefix, is not kept.
        assert_eq!(incomplete_suffix(&[0xFF]), 0);
        assert_eq!(incomplete_suffix(&[0xE0, 0x80]), 0);
        assert_eq!(incomplete_suffix(&[0xED, 0xA0]), 0);
    }
}
