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
    _owner: crate::cancellation::CancelOnDrop,
}
impl Body for StreamBody {
    type Data = Bytes;
    type Error = handler::StreamAborted;
    fn poll_frame(
        mut self: std::pin::Pin<&mut Self>,
        context: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Result<hyper::body::Frame<Bytes>, Self::Error>>> {
        self.chunks
            .poll_recv(context)
            .map(|chunk| chunk.map(|chunk| chunk.map(hyper::body::Frame::data)))
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
    response.headers_mut().insert(
        hyper::header::CONNECTION,
        hyper::header::HeaderValue::from_static("close"),
    );
    if head && !streamed && !LENGTHLESS_HEAD_STATUSES.contains(&status) {
        response.headers_mut().insert(
            hyper::header::CONTENT_LENGTH,
            hyper::header::HeaderValue::from_str(&length.to_string()).map_err(|_| fail())?,
        );
    }
    Ok(response)
}
async fn request(req: Request<Incoming>) -> Result<(Vec<u8>, bool), StatusCode> {
    if req.method() == hyper::Method::CONNECT || req.headers().contains_key(hyper::header::UPGRADE)
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    if req.body().size_hint().lower() > http::MAX_BODY_BYTES as u64 {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let head = req.method() == hyper::Method::HEAD;
    let target = req.uri().to_string();
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
    Ok((json, head))
}
/// The worker permit a v2 connection reserved before it was accepted.
type Reserved = Arc<std::sync::Mutex<Option<tokio::sync::OwnedSemaphorePermit>>>;

async fn handle(
    req: Request<Incoming>,
    shared: Arc<Shared>,
    reserved: Reserved,
) -> Result<Reply, Infallible> {
    let (request, head) = tokio::select! {
        _=shared.stop.cancelled()=>return Ok(failure(StatusCode::SERVICE_UNAVAILABLE)),
        value=tokio::time::timeout(Duration::from_secs(http::HEADER_SECONDS), request(req))=> match value {
            Ok(Ok(value))=>value,
            Ok(Err(status))=>return Ok(failure(status)),
            Err(_)=>return Ok(failure(StatusCode::REQUEST_TIMEOUT)),
        }
    };
    if shared.stop.stopped() {
        return Ok(failure(StatusCode::SERVICE_UNAVAILABLE));
    }
    let permit = reserved.lock().unwrap_or_else(|e| e.into_inner()).take();
    let Some(permit) = permit.or_else(|| shared.workers.clone().try_acquire_owned().ok()) else {
        return Ok(failure(StatusCode::SERVICE_UNAVAILABLE));
    };
    let cancellation = (shared.contract.version == http::UTF16_VERSION)
        .then(|| crate::cancellation::Cancellation::new(shared.engine.clone()));
    let mut owner = crate::cancellation::CancelOnDrop(cancellation.clone());
    if shared.contract.streaming {
        return Ok(streaming(shared, request, head, permit, cancellation, owner).await);
    }
    let worker = shared.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        handler::execute(&worker, request, cancellation, None)
    })
    .await;
    owner.disarm();
    let reply = match result {
        Ok(Ok(value)) => response(value, head, None),
        // The owning worker has already delivered bounded output/diagnostics.
        Ok(Err(_)) => return Ok(failure(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(_) => Err(HttpError::Message("handler worker failed".into())),
    };
    Ok(match reply {
        Ok(reply) => reply,
        Err(error) => {
            shared.sink.diagnostic(error.to_string());
            failure(StatusCode::INTERNAL_SERVER_ERROR)
        }
    })
}

/// A streaming service's request: the worker sends the head, then keeps
/// pulling the body into the response for as long as it lives. Service
/// shutdown cancels a worker still pulling.
async fn streaming(
    shared: Arc<Shared>,
    request: Vec<u8>,
    head: bool,
    permit: tokio::sync::OwnedSemaphorePermit,
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
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
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
        );
    });
    let Ok(value) = head_receiver.await else {
        return failure(StatusCode::INTERNAL_SERVER_ERROR);
    };
    let body = StreamBody {
        chunks,
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
        connection(stream, shared.clone(), None).await;
    }
    Ok(())
}

/// Contract v2: up to `limits.connections` connections at once, each with a
/// worker of its own. A connection is accepted only once a worker permit is
/// free, and its request uses that permit, so a further client waits in the
/// listener's backlog (never in an unbounded queue), including right after a
/// client that left, whose cancelled worker keeps its permit until it stops.
/// Shutdown stops accepting and waits for the open connections, which drain
/// or end their responses.
async fn pooled(
    listener: tokio::net::TcpListener,
    shared: Arc<Shared>,
) -> Result<(), RuntimeError> {
    let mut open = tokio::task::JoinSet::new();
    let result = loop {
        while open.try_join_next().is_some() {}
        let permit = tokio::select! {
            _ = shared.stop.cancelled() => break Ok(()),
            permit = shared.workers.clone().acquire_owned() => permit,
        };
        // The semaphore is never closed.
        let Ok(permit) = permit else { break Ok(()) };
        let accepted = tokio::select! { _ = shared.stop.cancelled() => break Ok(()), value = listener.accept() => value };
        let (stream, _) = match accepted {
            Ok(accepted) => accepted,
            Err(error) => break Err(HttpError::Io(error).into()),
        };
        open.spawn(connection(stream, shared.clone(), Some(permit)));
    };
    while open.join_next().await.is_some() {}
    result
}

/// Serve one connection (no keep-alive) under its deadline: the connection
/// bound, plus a streaming response's lifetime for a streaming service.
async fn connection(
    stream: tokio::net::TcpStream,
    shared: Arc<Shared>,
    permit: Option<tokio::sync::OwnedSemaphorePermit>,
) {
    let service = shared.clone();
    let reserved: Reserved = Arc::new(std::sync::Mutex::new(permit));
    let mut builder = http1::Builder::new();
    builder
        .keep_alive(false)
        .max_headers(http::MAX_HEADERS)
        .max_buf_size(http::HTTP_BUFFER_BYTES)
        .timer(TokioTimer::new())
        .header_read_timeout(Duration::from_secs(http::HEADER_SECONDS));
    let entered = Arc::new(AtomicBool::new(false));
    let request_entered = entered.clone();
    let connection = builder.serve_connection(
        TokioIo::new(stream),
        service_fn(move |req| {
            request_entered.store(true, Ordering::Release);
            handle(req, service.clone(), reserved.clone())
        }),
    );
    // Stop reading new requests on shutdown, without dropping an active
    // response future. Its original connection/request deadlines still apply.
    tokio::pin!(connection);
    let mut bound = Duration::from_secs(http::CONNECTION_SECONDS);
    if let Some(limits) = shared
        .contract
        .limits
        .stream
        .filter(|_| shared.contract.streaming)
    {
        bound += Duration::from_millis(limits.lifetime_ms);
    }
    let until = tokio::time::Instant::now() + bound;
    let result = tokio::select! {
        result=tokio::time::timeout_at(until,connection.as_mut())=>result,
        _=shared.stop.cancelled()=>{
            if entered.load(Ordering::Acquire) {
                connection.as_mut().graceful_shutdown();
                tokio::time::timeout_at(until,connection.as_mut()).await
            } else {
                // No request exists to drain. Closing a partial header or
                // an idle accepted socket must not wait for its timeout.
                Ok(Ok(()))
            }
        }
    };
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => shared
            .sink
            .diagnostic(format!("HTTP connection rejected: {error}")),
        Err(_) => shared.sink.diagnostic("HTTP connection deadline".into()),
    }
}
