use super::*;
use http_body_util::{BodyExt, Full};
use hyper::{
    Request, Response, StatusCode,
    body::{Body, Bytes, Incoming},
    server::conn::http1,
    service::service_fn,
};
use hyper_util::rt::{TokioIo, TokioTimer};
use std::{convert::Infallible, time::Duration};
use varta_common::http::{self, HttpHeader, HttpRequest, HttpResponse};

type Reply = Response<Full<Bytes>>;
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
    let mut response = Response::new(Full::new(Bytes::from_static(b"request failed\n")));
    *response.status_mut() = status;
    response.headers_mut().insert(
        hyper::header::CONNECTION,
        hyper::header::HeaderValue::from_static("close"),
    );
    response
}
fn response(value: HttpResponse, head: bool) -> Result<Reply, HttpError> {
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
    let mut response = Response::new(Full::new(if head {
        Bytes::new()
    } else {
        Bytes::from(value.body)
    }));
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
    if head && !LENGTHLESS_HEAD_STATUSES.contains(&status) {
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
async fn handle(req: Request<Incoming>, shared: Arc<Shared>) -> Result<Reply, Infallible> {
    let (request, head) =
        match tokio::time::timeout(Duration::from_secs(http::HEADER_SECONDS), request(req)).await {
            Ok(Ok(value)) => value,
            Ok(Err(status)) => return Ok(failure(status)),
            Err(_) => return Ok(failure(StatusCode::REQUEST_TIMEOUT)),
        };
    let Ok(permit) = shared.workers.clone().try_acquire_owned() else {
        return Ok(failure(StatusCode::SERVICE_UNAVAILABLE));
    };
    let worker = shared.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        handler::execute(&worker, request)
    })
    .await;
    let reply = match result {
        Ok(Ok((value, logs))) => {
            if let Err(error) = shared.sink.lines(logs) {
                shared.sink.diagnostic(error.to_string());
                shared.stop.shutdown();
            }
            response(value, head)
        }
        Ok(Err(error)) => Err(HttpError::Message(error.to_string())),
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
    while !shared.stop.stopped() {
        let accepted = tokio::select! { _ = shared.stop.cancelled() => break, value = listener.accept() => value };
        let (stream, _) = accepted.map_err(HttpError::Io)?;
        let service = shared.clone();
        let mut builder = http1::Builder::new();
        builder
            .keep_alive(false)
            .max_headers(http::MAX_HEADERS)
            .max_buf_size(http::HTTP_BUFFER_BYTES)
            .timer(TokioTimer::new())
            .header_read_timeout(Duration::from_secs(http::HEADER_SECONDS));
        let connection = builder.serve_connection(
            TokioIo::new(stream),
            service_fn(move |req| handle(req, service.clone())),
        );
        // Shutdown stops acceptance first; the current reply drains within this bound.
        match tokio::time::timeout(Duration::from_secs(http::CONNECTION_SECONDS), connection).await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => shared
                .sink
                .diagnostic(format!("HTTP connection rejected: {error}")),
            Err(_) => shared.sink.diagnostic("HTTP connection deadline".into()),
        }
    }
    Ok(())
}
