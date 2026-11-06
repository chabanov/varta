use super::*;
use http_body_util::{BodyExt, Either, Full};
use hyper::{
    Request, Response, StatusCode,
    body::{Body, Bytes, Incoming},
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::{TokioIo, TokioTimer};
use std::{
    convert::Infallible,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use varta_common::http::{self, HttpHeader, HttpRequest, HttpResponse};

type Reply = Response<Either<Full<Bytes>, StreamBody>>;

/// The body of a streaming response: the chunks its worker pulls. Dropping
/// it (a finished response or a client that left) cancels that worker.
pub(super) struct StreamBody {
    chunks: tokio::sync::mpsc::Receiver<Result<Bytes, handler::StreamAborted>>,
    /// An abort received but not yet reported (see `poll_frame`).
    aborted: Option<handler::StreamAborted>,
    _owner: crate::cancellation::CancelOnDrop,
}
impl Body for StreamBody {
    type Data = Bytes;
    type Error = handler::StreamAborted;
    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<hyper::body::Frame<Bytes>, Self::Error>>> {
        use std::task::Poll;
        if let Some(aborted) = self.aborted.take() {
            return Poll::Ready(Some(Err(aborted)));
        }
        match self.chunks.poll_recv(context) {
            // An error makes the transport close at once: first yield once,
            // so the chunks already written are flushed to the client.
            Poll::Ready(Some(Err(aborted))) => {
                self.aborted = Some(aborted);
                context.waker().wake_by_ref();
                Poll::Pending
            }
            other => other.map(|chunk| chunk.map(|chunk| chunk.map(hyper::body::Frame::data))),
        }
    }
}
/// Chunks a streaming worker may pull ahead of the client.
const STREAM_BUFFER: usize = 1;
/// Headers that frame the connection or message; the transport sets them and
/// an adapter response must not.
const FRAMING_HEADERS: [&str; 9] = [
    "connection",
    "content-length",
    "transfer-encoding",
    "upgrade",
    "keep-alive",
    "trailer",
    "te",
    "proxy-authenticate",
    "proxy-authorization",
];
/// Statuses whose HEAD response carries no `content-length`.
const LENGTHLESS_HEAD_STATUSES: [StatusCode; 2] =
    [StatusCode::NO_CONTENT, StatusCode::NOT_MODIFIED];
fn failure(status: StatusCode) -> Reply {
    let mut response = Response::new(Either::Left(Full::new(Bytes::from_static(
        b"request failed\n",
    ))));
    *response.status_mut() = status;
    response.headers_mut().insert(
        hyper::header::CONNECTION,
        hyper::header::HeaderValue::from_static("close"),
    );
    response
}
/// A whole response, or the head of a streaming one (`stream` is its body;
/// its length is unknown, so it has no `content-length`).
fn response(
    value: HttpResponse,
    head: bool,
    stream: Option<StreamBody>,
) -> Result<Reply, HttpError> {
    let fail = || HttpError::Message("response status, body or headers".into());
    if !(http::MIN_STATUS..=http::MAX_STATUS).contains(&value.status)
        || value.body.len() > abi::MAX_RESULT_BYTES
        || value.headers.len() > http::MAX_HEADERS
        || value
            .headers
            .iter()
            .map(|h| h.name.len() + h.value.len())
            .sum::<usize>()
            > http::MAX_HEADER_BYTES
        || http::BODYLESS_STATUSES.contains(&value.status) && !value.body.is_empty()
    {
        return Err(fail());
    }
    let status = StatusCode::from_u16(value.status as u16).map_err(|_| fail())?;
    let length = value.body.len();
    let streamed = stream.is_some();
    let mut response = Response::new(match stream {
        Some(stream) if !head => Either::Right(stream),
        _ => Either::Left(Full::new(if head {
            Bytes::new()
        } else {
            Bytes::from(value.body)
        })),
    });
    *response.status_mut() = status;
    for header in value.headers {
        let name =
            hyper::header::HeaderName::from_bytes(header.name.as_bytes()).map_err(|_| fail())?;
        if FRAMING_HEADERS.contains(&name.as_str()) {
            return Err(fail());
        }
        let value = hyper::header::HeaderValue::from_str(&header.value).map_err(|_| fail())?;
        response.headers_mut().append(name, value);
    }
    if head && !streamed && !LENGTHLESS_HEAD_STATUSES.contains(&status) {
        response.headers_mut().insert(
            hyper::header::CONTENT_LENGTH,
            hyper::header::HeaderValue::from_str(&length.to_string()).map_err(|_| fail())?,
        );
    }
    Ok(response)
}
/// The checks every request passes before dispatch: no CONNECT or upgrade,
/// a body within `body_bytes` as far as its declared length shows, and the
/// target and header bounds. Returns the request's header list.
fn checked_head(req: &Request<Incoming>, body_bytes: usize) -> Result<Vec<HttpHeader>, StatusCode> {
    if req.method() == hyper::Method::CONNECT || req.headers().contains_key(hyper::header::UPGRADE)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.body().size_hint().lower() > body_bytes as u64 {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let target = req.uri().path_and_query().map_or("/", |path| path.as_str());
    if target.len() > http::MAX_TARGET_BYTES || req.headers().len() > http::MAX_HEADERS {
        return Err(StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE);
    }
    let mut headers = Vec::new();
    let mut size = 0;
    for (name, value) in req.headers() {
        size += name.as_str().len() + value.as_bytes().len();
        if size > http::MAX_HEADER_BYTES {
            return Err(StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE);
        }
        headers.push(HttpHeader {
            name: name.to_string(),
            value: value
                .to_str()
                .map_err(|_| StatusCode::BAD_REQUEST)?
                .to_owned(),
        });
    }
    Ok(headers)
}

/// The envelope of a v2 Web `Request` service: the method, the absolute URL
/// (`scheme`, the authority the request names or else the listener's
/// address, and the target's path and query), the headers and the body
/// still unread, which the worker streams to the guest.
fn web_request(
    req: Request<Incoming>,
    scheme: &str,
    address: SocketAddr,
    body_bytes: usize,
) -> Result<(handler::Envelope, bool), StatusCode> {
    let headers = checked_head(&req, body_bytes)?;
    let head = req.method() == hyper::Method::HEAD;
    // An absolute-form target or an HTTP/2 `:authority` names it; else the
    // `Host` header, which must be a valid authority.
    let authority = match req.uri().authority() {
        Some(authority) => authority.to_string(),
        None => match req.headers().get(hyper::header::HOST) {
            Some(host) => host
                .to_str()
                .ok()
                .and_then(|host| host.parse::<hyper::http::uri::Authority>().ok())
                .ok_or(StatusCode::BAD_REQUEST)?
                .to_string(),
            None => address.to_string(),
        },
    };
    let target = req.uri().path_and_query().map_or("/", |path| path.as_str());
    let url = format!("{scheme}://{authority}{target}");
    if url.len() > abi::MAX_INPUT_BYTES {
        return Err(StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE);
    }
    let method = req.method().to_string();
    let incoming = req.into_body();
    let body = (!incoming.is_end_stream()).then_some(incoming);
    let head_record = http::WebRequestHead {
        method,
        url,
        headers,
        body: 0,
    };
    Ok((
        handler::Envelope::Web {
            head: head_record,
            body,
        },
        head,
    ))
}

async fn request(req: Request<Incoming>) -> Result<(handler::Envelope, bool), StatusCode> {
    let headers = checked_head(&req, http::MAX_BODY_BYTES)?;
    let head = req.method() == hyper::Method::HEAD;
    let target = req.uri().to_string();
    if target.len() > http::MAX_TARGET_BYTES {
        return Err(StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE);
    }
    let method = req.method().to_string();
    let mut incoming = req.into_body();
    let mut body = Vec::new();
    while let Some(frame) = incoming.frame().await {
        let frame = frame.map_err(|_| StatusCode::BAD_REQUEST)?;
        if let Some(data) = frame.data_ref() {
            if data.len() > http::MAX_BODY_BYTES - body.len() {
                return Err(StatusCode::PAYLOAD_TOO_LARGE);
            }
            body.extend_from_slice(data);
        } else if frame.trailers_ref().is_some_and(|h| !h.is_empty()) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    let body = String::from_utf8(body).map_err(|_| StatusCode::BAD_REQUEST)?;
    let json = serde_json::to_vec(&HttpRequest {
        method,
        target,
        body,
        headers,
    })
    .map_err(|_| StatusCode::BAD_REQUEST)?;
    if json.len() > abi::MAX_INPUT_BYTES {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    Ok((handler::Envelope::Text(json), head))
}
/// Close the connection after `reply`: always under contract v1, and on a
/// keep-alive connection's last allowed request.
fn closing(mut reply: Reply, close: bool) -> Reply {
    if close {
        reply.headers_mut().insert(
            hyper::header::CONNECTION,
            hyper::header::HeaderValue::from_static("close"),
        );
    }
    reply
}

/// What a running request holds: an instance slot and, in contract v2, its
/// memory budget's pages of the server's memory ceiling. Released when its
/// worker task ends, after its Store was dropped.
struct Admission {
    _slot: tokio::sync::OwnedSemaphorePermit,
    _memory: Option<tokio::sync::OwnedSemaphorePermit>,
}

/// Admit a request: v1 takes its one worker at once or gets 503; a v2
/// request waits at most the worker wait for a slot and its memory, else it
/// gets 503 with `Retry-After`. Shutdown ends the wait.
async fn admit(shared: &Shared, pool: Option<http::PoolLimits>) -> Result<Admission, Refusal> {
    let Some(pool) = pool else {
        return shared
            .workers
            .clone()
            .try_acquire_owned()
            .map(|slot| Admission {
                _slot: slot,
                _memory: None,
            })
            .map_err(|_| Refusal::Unavailable);
    };
    let admitted = async {
        let slot = shared.workers.clone().acquire_owned().await.ok()?;
        let memory = shared
            .memory
            .clone()
            .acquire_many_owned(shared.request_pages)
            .await
            .ok()?;
        Some(Admission {
            _slot: slot,
            _memory: Some(memory),
        })
    };
    tokio::select! {
        _ = shared.stop.cancelled() => Err(Refusal::Unavailable),
        admitted = tokio::time::timeout(Duration::from_millis(pool.worker_wait_ms), admitted) => {
            match admitted {
                Ok(Some(admission)) => Ok(admission),
                _ => Err(Refusal::Busy),
            }
        }
    }
}

/// Why a request was not admitted.
enum Refusal {
    /// The v1 worker is busy, or the service stops: 503.
    Unavailable,
    /// No admission within the worker wait: 503 with `Retry-After`.
    Busy,
}
impl Refusal {
    fn reply(self) -> Reply {
        match self {
            Self::Unavailable => failure(StatusCode::SERVICE_UNAVAILABLE),
            Self::Busy => busy(),
        }
    }
}

/// A 503 for a request that found no worker within the wait bound.
fn busy() -> Reply {
    let mut reply = failure(StatusCode::SERVICE_UNAVAILABLE);
    reply.headers_mut().insert(
        hyper::header::RETRY_AFTER,
        hyper::header::HeaderValue::from(http::BUSY_RETRY_AFTER_SECONDS),
    );
    reply
}

async fn handle(
    req: Request<Incoming>,
    shared: Arc<Shared>,
    served: Arc<std::sync::atomic::AtomicUsize>,
) -> Result<Reply, Infallible> {
    let pool = shared.pool();
    let count = served.fetch_add(1, Ordering::AcqRel) + 1;
    let close = pool.is_none_or(|pool| count >= pool.keep_alive_requests);
    Ok(closing(handle_request(req, shared, pool).await, close))
}

/// The envelope of `req`: a Web `Request` service's head, its body still
/// streaming; otherwise the whole request, read within the header bound.
async fn envelope(
    req: Request<Incoming>,
    shared: &Shared,
) -> Result<(handler::Envelope, bool), StatusCode> {
    match shared.limits.transport {
        Some(transport) if shared.contract.web_request => web_request(
            req,
            shared.scheme,
            shared.address,
            transport.request_body_bytes,
        ),
        _ => tokio::time::timeout(Duration::from_secs(http::HEADER_SECONDS), request(req))
            .await
            .unwrap_or(Err(StatusCode::REQUEST_TIMEOUT)),
    }
}

async fn handle_request(
    req: Request<Incoming>,
    shared: Arc<Shared>,
    pool: Option<http::PoolLimits>,
) -> Reply {
    let (request, head) = tokio::select! {
        _ = shared.stop.cancelled() => return failure(StatusCode::SERVICE_UNAVAILABLE),
        value = envelope(req, &shared) => match value {
            Ok(value) => value,
            Err(status) => return failure(status),
        }
    };
    if shared.stop.stopped() {
        return failure(StatusCode::SERVICE_UNAVAILABLE);
    }
    let admission = match admit(&shared, pool).await {
        Ok(admission) => admission,
        Err(refusal) => return refusal.reply(),
    };
    let result = match (pool, request) {
        // Contract v1 (frozen): a blocking worker that is never cancelled.
        (None, handler::Envelope::Text(request)) => {
            let worker = shared.clone();
            tokio::task::spawn_blocking(move || {
                let _admission = admission;
                handler::execute_legacy(&worker, request)
            })
            .await
        }
        (_, request) => {
            let cancellation = Some(crate::cancellation::Cancellation::new(
                shared.engine.clone(),
            ));
            let mut owner = crate::cancellation::CancelOnDrop(cancellation.clone());
            if shared.contract.streaming {
                return streaming(shared, request, head, admission, cancellation, owner).await;
            }
            // The worker is a task of its own: a connection that goes away
            // drops only its owner, which cancels it; its admission ends
            // with it.
            let worker = shared.clone();
            let result = tokio::spawn(async move {
                let _admission = admission;
                handler::execute(&worker, request, cancellation, None).await
            })
            .await;
            owner.disarm();
            result
        }
    };
    let reply = match result {
        Ok(Ok(value)) => response(value, head, None),
        // The owning worker has already delivered bounded output/diagnostics.
        Ok(Err(_)) => return failure(StatusCode::INTERNAL_SERVER_ERROR),
        Err(_) => Err(HttpError::Message("handler worker failed".into())),
    };
    match reply {
        Ok(reply) => reply,
        Err(error) => {
            shared.sink.diagnostic(error.to_string());
            failure(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// A streaming service's request: the worker sends the head, then keeps
/// pulling the body into the response for as long as it lives. Service
/// shutdown cancels a worker still pulling.
async fn streaming(
    shared: Arc<Shared>,
    request: handler::Envelope,
    head: bool,
    admission: Admission,
    cancellation: Option<crate::cancellation::Cancellation>,
    owner: crate::cancellation::CancelOnDrop,
) -> Reply {
    let (head_sender, head_receiver) = tokio::sync::oneshot::channel();
    let (chunk_sender, chunks) = tokio::sync::mpsc::channel(STREAM_BUFFER);
    let (finished, done) = tokio::sync::oneshot::channel::<()>();
    if let Some(token) = cancellation.clone() {
        let stop = shared.stop.clone();
        tokio::spawn(async move {
            tokio::select! {
                _ = stop.cancelled() => token.cancel(),
                _ = done => {}
            }
        });
    }
    let worker = shared.clone();
    tokio::spawn(async move {
        let _admission = admission;
        let _finished = finished;
        // The worker reports its own outcome; the transport sees its head.
        let _ = handler::execute(
            &worker,
            request,
            cancellation,
            Some(handler::Streaming {
                head: head_sender,
                chunks: chunk_sender,
                bodyless: head,
            }),
        )
        .await;
    });
    let Ok(value) = head_receiver.await else {
        return failure(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let body = StreamBody {
        chunks,
        aborted: None,
        _owner: owner,
    };
    match response(value, head, Some(body)) {
        Ok(reply) => reply,
        Err(error) => {
            shared.sink.diagnostic(error.to_string());
            failure(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub(super) async fn serve(
    listener: TcpListener,
    address: SocketAddr,
    shared: Arc<Shared>,
) -> Result<(), RuntimeError> {
    let listener = tokio::net::TcpListener::from_std(listener).map_err(HttpError::Io)?;
    if let Some(observer) = &shared.sink.observer {
        observer(RuntimeEvent::Listening {
            address,
            shutdown: shared.stop.clone(),
        });
    }
    if shared.contract.version == http::UTF16_VERSION {
        pooled(listener, shared).await
    } else {
        sequential(listener, shared).await
    }
}

/// Contract v1: one connection at a time; a busy worker refuses with 503.
async fn sequential(
    listener: tokio::net::TcpListener,
    shared: Arc<Shared>,
) -> Result<(), RuntimeError> {
    while !shared.stop.stopped() {
        let accepted = tokio::select! { _ = shared.stop.cancelled() => break, value = listener.accept() => value };
        let (stream, _) = accepted.map_err(HttpError::Io)?;
        connection(stream, shared.clone()).await;
    }
    Ok(())
}

/// Contract v2: up to `limits.connections` open keep-alive connections, which
/// cost no worker while idle; each request waits a bounded time for one of
/// the pool's workers. A further connection waits in the listener's backlog,
/// never in an unbounded queue. Shutdown stops accepting and waits for the
/// open connections, which drain or end their responses.
async fn pooled(
    listener: tokio::net::TcpListener,
    shared: Arc<Shared>,
) -> Result<(), RuntimeError> {
    let slots = Arc::new(tokio::sync::Semaphore::new(shared.limits.connections));
    let mut open = tokio::task::JoinSet::new();
    let result = loop {
        while open.try_join_next().is_some() {}
        let slot = tokio::select! {
            _ = shared.stop.cancelled() => break Ok(()),
            slot = slots.clone().acquire_owned() => slot,
        };
        // The semaphore is never closed.
        let Ok(slot) = slot else { break Ok(()) };
        let accepted = tokio::select! { _ = shared.stop.cancelled() => break Ok(()), value = listener.accept() => value };
        let (stream, _) = match accepted {
            Ok(accepted) => accepted,
            Err(error) => break Err(HttpError::Io(error).into()),
        };
        let service = shared.clone();
        open.spawn(async move {
            let _slot = slot;
            connection(stream, service).await;
        });
    };
    while open.join_next().await.is_some() {}
    result
}

/// Serve one connection. Contract v1: one request, then close, within the
/// connection bound. Contract v2: keep-alive requests (the idle time is also
/// the head deadline) within the connection lifetime, after which the
/// connection finishes its current response and closes. A streaming
/// service's bound adds the stream lifetime. A v2 connection whose response
/// write makes no progress for the write idle bound closes.
async fn connection(stream: tokio::net::TcpStream, shared: Arc<Shared>) {
    let pool = shared.pool();
    let write_idle = shared
        .limits
        .transport
        .filter(|_| pool.is_some())
        .map(|transport| Duration::from_millis(transport.write_idle_ms));
    let service = shared.clone();
    let served = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let answered = served.clone();
    let mut builder = http1::Builder::new();
    builder
        .keep_alive(pool.is_some())
        .max_headers(http::MAX_HEADERS)
        .max_buf_size(http::HTTP_BUFFER_BYTES)
        .timer(TokioTimer::new())
        .header_read_timeout(
            pool.map_or(Duration::from_secs(http::HEADER_SECONDS), |pool| {
                Duration::from_millis(pool.keep_alive_idle_ms)
            }),
        );
    let entered = Arc::new(AtomicBool::new(false));
    let request_entered = entered.clone();
    let connection = builder.serve_connection(
        TokioIo::new(io::WriteIdle::new(stream, write_idle)),
        service_fn(move |req| {
            request_entered.store(true, Ordering::Release);
            handle(req, service.clone(), served.clone())
        }),
    );
    // Stop reading new requests on shutdown, without dropping an active
    // response future. Its original connection/request deadlines still apply.
    tokio::pin!(connection);
    let stream_lifetime = shared
        .limits
        .stream
        .filter(|_| shared.contract.streaming)
        .map_or(Duration::ZERO, |limits| {
            Duration::from_millis(limits.lifetime_ms)
        });
    let bound = pool.map_or(Duration::from_secs(http::CONNECTION_SECONDS), |pool| {
        Duration::from_millis(pool.connection_lifetime_ms)
    }) + stream_lifetime;
    let until = tokio::time::Instant::now() + bound;
    // The current response of a connection whose lifetime ended may finish
    // within its own bound: a Web `Request` service's request lifetime.
    let drain = match shared.limits.transport {
        Some(transport) if shared.contract.web_request => {
            Duration::from_millis(transport.request_lifetime_ms) + stream_lifetime
        }
        _ => Duration::from_secs(http::CONNECTION_SECONDS),
    };
    let result = tokio::select! {
        result = connection.as_mut() => Some(result),
        _ = tokio::time::sleep_until(until) => {
            if pool.is_some() {
                // The lifetime ends: finish the current response, then close.
                connection.as_mut().graceful_shutdown();
                tokio::time::timeout(drain, connection.as_mut()).await.ok()
            } else {
                None
            }
        }
        _=shared.stop.cancelled()=>{
            if entered.load(Ordering::Acquire) {
                // Drain the current response within the connection bound.
                connection.as_mut().graceful_shutdown();
                tokio::time::timeout_at(until, connection.as_mut()).await.ok()
            } else {
                // No request exists to drain. Closing a partial header or
                // an idle accepted socket must not wait for its timeout.
                Some(Ok(()))
            }
        }
    };
    match result {
        Some(Ok(())) => {}
        // A kept-alive connection that idled past its bound closes: not a
        // rejection.
        Some(Err(error)) if error.is_timeout() && answered.load(Ordering::Acquire) > 0 => {}
        Some(Err(error)) => shared
            .sink
            .diagnostic(format!("HTTP connection rejected: {error}")),
        None => shared.sink.diagnostic("HTTP connection deadline".into()),
    }
}
