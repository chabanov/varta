//! The HTTP/1.1 client of one request: a task on the execution's
//! current-thread Tokio runtime. The runtime runs only on the guest's thread
//! while the event loop waits or a host call closes a transfer, so no network
//! effect happens while guest code runs. The connection is a second task,
//! published through a [`ConnectionSlot`]: an abort cancels both tasks and
//! runs the runtime until both are dropped before the abort returns, so the
//! connection closes at once rather than when the event loop next waits; the
//! end of the execution drops the runtime with every task.
//!
//! V37: name resolution, connect, TLS, the request and every body read share
//! the request's deadline (the remaining execution deadline). V38: the head
//! must arrive within one inactivity window of the start, and each wait of a
//! body read within one window of its own start ([`Timing`]). The task
//! reports the head once, then one bounded chunk per pull, so a slow guest
//! applies backpressure instead of buffering the body in the host.
use super::{Completion, Event};
use crate::turns::Inactivity;
use hyper::body::{Body, Bytes, Incoming};
use hyper::header::{HeaderName, HeaderValue};
use std::sync::{Arc, Mutex, PoisonError};
use std::task::Poll;
use std::time::Instant;
use tokio::sync::mpsc;
use varta_common::net::{
    BodyBound, CompletionKind, HeadForm, MAX_CHUNK_BYTES, MAX_RESPONSE_BODY_BYTES,
    MAX_RESPONSE_HEADER_BYTES, MAX_RESPONSE_HEADERS, RequestFailure,
};

/// A validated request, ready for the transport.
pub(super) struct Plan {
    pub url: url::Url,
    pub method: hyper::Method,
    pub headers: Vec<(HeaderName, HeaderValue)>,
    /// How the head record lists the response headers: v37 as received,
    /// v38 canonical with only the visible names.
    pub head: HeadForm,
    pub body: Option<Bytes>,
    /// When name resolution, connect, TLS and the head must be done.
    pub deadline: Instant,
    /// How long the body reads may wait.
    pub timing: Timing,
    /// Which body bytes `net::MAX_RESPONSE_BODY_BYTES` bounds.
    pub bound: BodyBound,
    /// V38: the request asked for the identity coding only, so a response
    /// with another content coding fails ([`RequestFailure::Encoding`]).
    pub identity: bool,
    /// Present exactly for `https` URLs.
    pub tls: Option<Arc<rustls::ClientConfig>>,
}

/// How long a transfer may wait for bytes.
#[derive(Debug, Clone, Copy)]
pub(super) enum Timing {
    /// V37: one end-to-end deadline for the whole transfer.
    Transfer(Instant),
    /// V38: each wait for bytes lasts at most one window.
    Inactivity(Inactivity),
}
impl Timing {
    /// When a wait that starts now ends.
    pub(super) fn until(&self) -> Instant {
        match self {
            Self::Transfer(deadline) => *deadline,
            Self::Inactivity(inactivity) => inactivity.until(),
        }
    }
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

/// Run one request to completion, reporting through `events`; its
/// connection task is published in `connection`.
pub(super) async fn run(
    id: u64,
    plan: Plan,
    events: mpsc::UnboundedSender<Event>,
    mut pull: mpsc::Receiver<ReadMode>,
    connection: ConnectionSlot,
) {
    let deadline = tokio::time::Instant::from_std(plan.deadline);
    let send = |kind: CompletionKind, text: String| {
        // The receiver lives as long as the execution; a closed channel means
        // the execution ended, so there is nobody left to tell.
        let _ = events.send(Event {
            id,
            completion: Completion { kind, text },
            arrived: std::time::Instant::now(),
        });
    };
    let fail = |failure: RequestFailure| send(CompletionKind::Failed, failure.to_string());
    let response =
        match tokio::time::timeout_at(deadline, connect_and_send(&plan, &connection)).await {
            Ok(Ok(response)) => response,
            Ok(Err(failure)) => return fail(failure),
            Err(_) => return fail(RequestFailure::Timeout),
        };
    let (parts, body, _connection) = response;
    if plan.identity {
        let codings: Vec<String> = parts
            .headers
            .get_all(hyper::header::CONTENT_ENCODING)
            .iter()
            .map(|value| latin1(value.as_bytes()))
            .collect();
        if let Some(codings) =
            varta_common::net::transformed_encoding(codings.iter().map(String::as_str))
        {
            return fail(RequestFailure::Encoding(codings));
        }
    }
    send(CompletionKind::Head, head(&plan.url, &parts, &plan.head));
    let mut reader = BodyReader::new(body, plan.timing, plan.bound);
    loop {
        let Some(mode) = pull.recv().await else {
            return;
        };
        match reader.next(mode).await {
            Ok(Some(text)) => send(CompletionKind::Chunk, text),
            Ok(None) => return send(CompletionKind::End, String::new()),
            Err(failure) => return fail(failure),
        }
    }
}

/// The connection task of one request, once its HTTP handshake is done. The
/// request task and the request's owner share it: the request task cancels
/// it when it ends, and the owner takes it to wait until the cancelled task,
/// and with it the socket, is gone (`Transfer::close` in the parent module).
#[derive(Clone, Default)]
pub(super) struct ConnectionSlot(Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>);
impl ConnectionSlot {
    /// The connection task, if one was published and not taken yet.
    pub(super) fn take(&self) -> Option<tokio::task::JoinHandle<()>> {
        self.lock().take()
    }
    fn publish(&self, task: tokio::task::JoinHandle<()>) {
        *self.lock() = Some(task);
    }
    /// Cancel the connection task, leaving it for the owner to wait for.
    fn abort(&self) {
        if let Some(task) = self.lock().as_ref() {
            task.abort();
        }
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<tokio::task::JoinHandle<()>>> {
        // No code panics while holding the lock, and the slot holds a
        // complete `Option` at every point, so a poisoned lock is still sound.
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Cancels the connection task when the request task ends or is dropped.
struct Connection(ConnectionSlot);
impl Drop for Connection {
    fn drop(&mut self) {
        self.0.abort();
    }
}

type Response = (hyper::http::response::Parts, Incoming, Connection);

/// A legacy synchronous net.get/net.fetch inside an owned HTTP v2 request.
/// Its guest call remains synchronous; only native I/O is driven here, under
/// the same cancellation token and without guest jobs or allowance renewal.
pub(crate) fn fetch_cancellable(
    url: &url::Url,
    deadline: Instant,
    read_body: bool,
    cancellation: &crate::cancellation::Cancellation,
) -> Result<(i32, Vec<u8>), crate::http::Failure> {
    use crate::{HostFailure, http::Failure};
    use http_body_util::BodyExt;
    let cancelled = || cancellation.check().map_err(|_| Failure::Cancelled);
    cancelled()?;
    // The caller authorizes a legacy http-only grant before entering here.
    if url.scheme() != "http" {
        return Err(HostFailure::io("synchronous network calls require http").into());
    }
    let plan = Plan {
        url: url.clone(),
        method: hyper::Method::GET,
        headers: Vec::new(),
        head: HeadForm::Arrival,
        body: None,
        deadline,
        timing: Timing::Transfer(deadline),
        bound: BodyBound::Response,
        identity: false,
        tls: None,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(HostFailure::io)?;
    let request = async {
        // This runtime ends with the call, closing every socket it owns.
        let (parts, mut incoming, _connection) = connect_and_send_with_limit(
            &plan,
            varta_common::net::MAX_FETCH_HEADER_BYTES,
            &ConnectionSlot::default(),
        )
        .await
        .map_err(HostFailure::io)?;
        let status = i32::from(parts.status.as_u16());
        let mut bytes = Vec::new();
        if read_body {
            while let Some(frame) = incoming.frame().await {
                let frame = frame.map_err(HostFailure::io)?;
                if let Ok(data) = frame.into_data() {
                    if data.len() > varta_common::abi::MAX_RESULT_BYTES.saturating_sub(bytes.len())
                    {
                        return Err(HostFailure::too_large());
                    }
                    bytes.extend_from_slice(&data);
                }
            }
        }
        Ok((status, bytes))
    };
    let timeout = || {
        HostFailure::io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "request timed out",
        ))
    };
    runtime.block_on(async {
        let request = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), request);
        tokio::pin!(request);
        loop {
            cancelled()?;
            if Instant::now() >= deadline { return Err(timeout().into()); }
            tokio::select! {
                biased;
                _ = tokio::time::sleep_until(tokio::time::Instant::from_std(cancellation.wait_until(deadline))) => {},
                result = &mut request => {
                    cancelled()?;
                    if Instant::now() >= deadline { return Err(timeout().into()); }
                    return result.map_err(|_| timeout())?.map_err(Failure::Host);
                }
            }
        }
    })
}

async fn connect_and_send(
    plan: &Plan,
    connection: &ConnectionSlot,
) -> Result<Response, RequestFailure> {
    connect_and_send_with_limit(plan, MAX_RESPONSE_HEADER_BYTES, connection).await
}

async fn connect_and_send_with_limit(
    plan: &Plan,
    header_bytes: usize,
    connection: &ConnectionSlot,
) -> Result<Response, RequestFailure> {
    let host = plan.url.host_str().ok_or(RequestFailure::NoHost)?;
    let port = plan
        .url
        .port_or_known_default()
        .ok_or(RequestFailure::NoPort)?;
    // An IPv6 host keeps its brackets in the URL; the socket needs the bare address.
    let address = host.trim_start_matches('[').trim_end_matches(']');
    let addresses = super::dns::resolve(address, port, plan.deadline)
        .await
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::TimedOut {
                RequestFailure::Timeout
            } else {
                RequestFailure::Connect(error.to_string())
            }
        })?;
    if Instant::now() >= plan.deadline {
        return Err(RequestFailure::Timeout);
    }
    // A resolved slice never invokes Tokio's per-runtime blocking DNS pool.
    let tcp = tokio::net::TcpStream::connect(addresses.as_slice())
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
        exchange(
            plan,
            hyper_util::rt::TokioIo::new(tls),
            header_bytes,
            connection,
        )
        .await
    } else {
        exchange(
            plan,
            hyper_util::rt::TokioIo::new(tcp),
            header_bytes,
            connection,
        )
        .await
    }
}

async fn exchange<T>(
    plan: &Plan,
    io: T,
    header_bytes: usize,
    slot: &ConnectionSlot,
) -> Result<Response, RequestFailure>
where
    T: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
{
    let mut builder = hyper::client::conn::http1::Builder::new();
    builder
        .max_headers(MAX_RESPONSE_HEADERS)
        .max_buf_size(header_bytes);
    let (mut sender, connection) = builder
        .handshake::<_, http_body_util::Full<Bytes>>(io)
        .await
        .map_err(|error| RequestFailure::HttpHandshake(error.to_string()))?;
    slot.publish(tokio::spawn(async move {
        // The outcome reaches the request through its response or body.
        let _ = connection.await;
    }));
    let connection = Connection(slot.clone());
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
/// its values in arrival order, which `form` keeps (v37) or sorts, filters
/// and combines (v38). Undeclared headers are dropped here, before the
/// record exists, so no later charge or copy ever sees them.
fn head(url: &url::Url, parts: &hyper::http::response::Parts, form: &HeadForm) -> String {
    let status = parts.status.as_u16();
    let reason = parts
        .extensions
        .get::<hyper::ext::ReasonPhrase>()
        .map(|reason| latin1(reason.as_bytes()))
        .or_else(|| parts.status.canonical_reason().map(str::to_owned))
        .unwrap_or_default();
    let headers = form.headers(
        parts
            .headers
            .iter()
            .map(|(name, value)| (name.as_str().to_owned(), latin1(value.as_bytes()))),
    );
    // The redirect target is no response header the guest sees: it only
    // lets the prelude follow a redirect, which re-checks the grant.
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

/// Reads a response body in chunks of at most [`BodyBound::delivery`] bytes
/// ([`MAX_CHUNK_BYTES`] for v37, `net::MAX_READ_DELIVERY_BYTES` for v38) and
/// decodes them as UTF-8, completing a sequence split between chunks from the
/// next one; a sequence still incomplete at the end becomes U+FFFD. A chunk
/// takes every frame already received (up to the bound) without waiting for
/// more, so small frames cost one guest read together while a quiet stream
/// (SSE) still delivers each event as soon as it arrives; received bytes
/// beyond the bound wait for the next reads, so host memory holds at most one
/// received frame beyond the bound plus a split sequence. Every wait for frames
/// ends at [`Timing::until`] of its own start, and the received bytes are
/// counted against [`MAX_RESPONSE_BODY_BYTES`] per [`BodyBound`].
struct BodyReader<B = Incoming> {
    body: B,
    timing: Timing,
    bound: BodyBound,
    buffer: Vec<u8>,
    carry: Vec<u8>,
    /// Bytes received: of the whole body ([`BodyBound::Response`]), or
    /// while serving the current read ([`BodyBound::Read`]).
    counted: usize,
    ended: bool,
}
impl<B> BodyReader<B>
where
    B: Body<Data = Bytes> + Unpin,
    B::Error: std::fmt::Display,
{
    fn new(body: B, timing: Timing, bound: BodyBound) -> Self {
        Self {
            body,
            timing,
            bound,
            buffer: Vec::new(),
            carry: Vec::new(),
            counted: 0,
            ended: false,
        }
    }
    /// The next nonempty chunk (text, or bytes as code units), or `None` at
    /// the end of the body.
    async fn next(&mut self, mode: ReadMode) -> Result<Option<String>, RequestFailure> {
        if self.bound == BodyBound::Read {
            self.counted = 0;
        }
        loop {
            if mode == ReadMode::Bytes && (!self.buffer.is_empty() || !self.carry.is_empty()) {
                // Bytes an earlier text read kept for an unfinished sequence
                // come first, undecoded.
                let mut bytes = std::mem::take(&mut self.carry);
                let take = self
                    .buffer
                    .len()
                    .min(self.bound.delivery().saturating_sub(bytes.len()));
                let rest = self.buffer.split_off(take);
                bytes.extend_from_slice(&std::mem::replace(&mut self.buffer, rest));
                return Ok(Some(latin1(&bytes)));
            }
            if !self.buffer.is_empty() {
                let rest = self
                    .buffer
                    .split_off(self.buffer.len().min(self.text_take()));
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
            // A received frame ends the wait: the next one starts afresh.
            let until = tokio::time::Instant::from_std(self.timing.until());
            tokio::time::timeout_at(until, std::future::poll_fn(|context| self.fill(context)))
                .await
                .map_err(|_| RequestFailure::BodyTimeout)??;
        }
    }
    /// The buffered bytes a text read decodes: the chunk (v37, the held-back
    /// bytes of a split sequence added), or the cap less those bytes (v38),
    /// so a v38 read never decodes more than its cap.
    fn text_take(&self) -> usize {
        match self.bound {
            BodyBound::Response => MAX_CHUNK_BYTES,
            BodyBound::Read => self.bound.delivery().saturating_sub(self.carry.len()),
        }
    }
    /// Take every frame that is ready, until the chunk bound or the end; wait
    /// only while nothing was taken.
    fn fill(&mut self, context: &mut std::task::Context<'_>) -> Poll<Result<(), RequestFailure>> {
        while self.buffer.len() < self.bound.delivery() {
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
                    self.counted = self.counted.saturating_add(data.len());
                    if self.counted > MAX_RESPONSE_BODY_BYTES {
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
        let arrival = HeadForm::Arrival;
        assert_eq!(
            head(&url, &parts, &arrival),
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
        assert!(head(&url, &parts, &arrival).contains(r#""statusText":"","#));
    }

    #[test]
    fn canonical_heads_show_only_visible_names_sorted_and_combined() {
        let response = hyper::Response::builder()
            .status(302)
            .header("X-B", "1")
            .header("Set-Cookie", "a=1")
            .header("Content-Type", "text/plain")
            .header("x-b", "2")
            .header("X-Hidden", "secret")
            .header("Location", "../next")
            .body(())
            .unwrap();
        let (parts, ()) = response.into_parts();
        let url = url::Url::parse("http://a.test/dir/page").unwrap();
        let visible = ["x-b", "content-type", "set-cookie"]
            .map(str::to_owned)
            .into_iter()
            .collect();
        // The redirect target stays available to the prelude while the
        // undeclared `location` header is not shown.
        assert_eq!(
            head(&url, &parts, &HeadForm::Canonical(visible)),
            concat!(
                r#"{"headers":["content-type","text/plain","x-b","1, 2"],"#,
                r#""location":"http://a.test/next","status":302,"statusText":"Found","#,
                r#""url":"http://a.test/dir/page"}"#
            )
        );
    }

    /// A body that yields `frames`, then ends or (`stall`) never answers.
    struct Frames {
        frames: std::collections::VecDeque<Bytes>,
        stall: bool,
    }
    impl Body for Frames {
        type Data = Bytes;
        type Error = std::convert::Infallible;
        fn poll_frame(
            mut self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> Poll<Option<Result<hyper::body::Frame<Bytes>, Self::Error>>> {
            match self.frames.pop_front() {
                Some(data) => Poll::Ready(Some(Ok(hyper::body::Frame::data(data)))),
                None if self.stall => Poll::Pending,
                None => Poll::Ready(None),
            }
        }
    }
    fn frames(sizes: &[usize], stall: bool) -> Frames {
        Frames {
            frames: sizes
                .iter()
                .map(|size| Bytes::from(vec![b'a'; *size]))
                .collect(),
            stall,
        }
    }
    /// Test-only window: long enough for any read of a ready body.
    const WINDOW: std::time::Duration = std::time::Duration::from_secs(5);
    fn inactive(window: std::time::Duration) -> Timing {
        Timing::Inactivity(Inactivity {
            window,
            bound: None,
        })
    }
    /// Read `body` to its end, as bytes; the delivered byte count or the
    /// failure.
    fn drain(body: Frames, timing: Timing, bound: BodyBound) -> Result<usize, RequestFailure> {
        reads(body, timing, bound, ReadMode::Bytes).map(|chunks| chunks.iter().sum())
    }
    /// Read `body` to its end in `mode`: the UTF-8 length of every chunk
    /// (for a byte read or valid UTF-8 text, the bytes it delivered), each
    /// checked against the bound's delivery cap.
    fn reads<B>(
        body: B,
        timing: Timing,
        bound: BodyBound,
        mode: ReadMode,
    ) -> Result<Vec<usize>, RequestFailure>
    where
        B: Body<Data = Bytes> + Unpin,
        B::Error: std::fmt::Display,
    {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut reader = BodyReader::new(body, timing, bound);
            let mut chunks = Vec::new();
            while let Some(chunk) = reader.next(mode).await? {
                let delivered = match mode {
                    ReadMode::Bytes => chunk.chars().count(),
                    ReadMode::Text => chunk.len(),
                };
                assert!(delivered <= bound.delivery(), "{delivered}");
                chunks.push(delivered);
            }
            Ok(chunks)
        })
    }

    /// V38 delivers at most `MAX_READ_DELIVERY_BYTES` per read, splitting a
    /// larger received frame (or ready frames together) across reads, in
    /// both read modes and across split UTF-8 sequences; v37 keeps its
    /// `MAX_CHUNK_BYTES` chunks unchanged.
    #[test]
    fn v38_reads_never_exceed_the_delivery_cap() {
        use varta_common::net::MAX_READ_DELIVERY_BYTES as CAP;
        let sizes = [MAX_CHUNK_BYTES + 3, 1, CAP - 1, CAP + 1, 3 * CAP];
        let total: usize = sizes.iter().sum();
        for mode in [ReadMode::Bytes, ReadMode::Text] {
            let chunks = reads(
                frames(&sizes, false),
                inactive(WINDOW),
                BodyBound::Read,
                mode,
            )
            .unwrap();
            assert_eq!(chunks.iter().sum::<usize>(), total);
            assert_eq!(chunks.iter().max(), Some(&CAP));
            assert!(chunks.len() >= total.div_ceil(CAP));
        }
        // Three-byte characters never fit a cap evenly, so text reads hold
        // back a split sequence; every read still stays within the cap.
        let euro = "\u{20ac}".repeat(MAX_CHUNK_BYTES);
        let body = Frames {
            frames: [Bytes::from(euro.clone().into_bytes())].into(),
            stall: false,
        };
        let chunks = reads(body, inactive(WINDOW), BodyBound::Read, ReadMode::Text).unwrap();
        assert_eq!(chunks.iter().sum::<usize>(), euro.len());
        assert!(chunks.len() > euro.len() / CAP);
        // V37: frames are coalesced and split at the frozen chunk bound.
        let timing = Timing::Transfer(Instant::now() + WINDOW);
        let chunks = reads(
            frames(&[MAX_CHUNK_BYTES + 3, 1], false),
            timing,
            BodyBound::Response,
            ReadMode::Bytes,
        )
        .unwrap();
        assert_eq!(chunks, [MAX_CHUNK_BYTES, 3, 1]);
    }

    /// V37 bounds the whole body; v38 bounds what one read receives, so a
    /// long body in chunk-sized frames passes and one oversized frame still
    /// fails the read that receives it.
    #[test]
    fn the_body_limit_applies_per_response_or_per_read() {
        let frame = MAX_CHUNK_BYTES;
        let count = MAX_RESPONSE_BODY_BYTES / frame + 2;
        let long = vec![frame; count];
        let total = frame * count;
        assert!(total > MAX_RESPONSE_BODY_BYTES);
        let timing = Timing::Transfer(Instant::now() + WINDOW);
        assert_eq!(
            drain(frames(&long, false), timing, BodyBound::Response),
            Err(RequestFailure::BodyLimit)
        );
        assert_eq!(
            drain(frames(&long, false), inactive(WINDOW), BodyBound::Read),
            Ok(total)
        );
        let oversized = [frame, MAX_RESPONSE_BODY_BYTES + 1];
        assert_eq!(
            drain(frames(&oversized, false), inactive(WINDOW), BodyBound::Read),
            Err(RequestFailure::BodyLimit)
        );
        assert_eq!(
            drain(
                frames(&[MAX_RESPONSE_BODY_BYTES], false),
                inactive(WINDOW),
                BodyBound::Read
            ),
            Ok(MAX_RESPONSE_BODY_BYTES)
        );
    }

    /// A stalled body fails the waiting read after one window (v38) or at
    /// the transfer deadline (v37), never earlier than the window.
    #[test]
    fn stalled_bodies_time_out() {
        const SHORT: std::time::Duration = std::time::Duration::from_millis(50);
        let started = Instant::now();
        assert_eq!(
            drain(frames(&[1, 2], true), inactive(SHORT), BodyBound::Read),
            Err(RequestFailure::BodyTimeout)
        );
        assert!(started.elapsed() >= SHORT);
        let timing = Timing::Transfer(Instant::now() + SHORT);
        assert_eq!(
            drain(frames(&[1], true), timing, BodyBound::Response),
            Err(RequestFailure::BodyTimeout)
        );
        // The instance lifetime bounds every window.
        let bounded = Timing::Inactivity(Inactivity {
            window: WINDOW,
            bound: Some(Instant::now() + SHORT),
        });
        let started = Instant::now();
        assert_eq!(
            drain(frames(&[], true), bounded, BodyBound::Read),
            Err(RequestFailure::BodyTimeout)
        );
        assert!(started.elapsed() < WINDOW);
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
