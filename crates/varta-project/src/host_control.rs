//! Bounded loopback control transport shared by trusted tooling hosts.
use http_body_util::{BodyExt, Full};
use hyper::{
    Request, Response, StatusCode,
    body::{Bytes, Incoming},
    header,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use sha2::{Digest, Sha256};
use std::{convert::Infallible, time::Duration};
const CONNECTION_SECONDS: u64 = 5;
const MAX_HEADERS: usize = 32;
const HEADER_BYTES: usize = 8192;
const MAX_JSON_BYTES: usize = 32 * 1024;
const PUBLIC_CSP: &str = "frame-ancestors 'none'; form-action 'none'; base-uri 'none'; object-src 'none'; connect-src 'self'";
const PERMISSIONS: &str = "camera=(), microphone=(), geolocation=(), clipboard-read=(), clipboard-write=(), display-capture=(), fullscreen=(), payment=(), usb=(), serial=(), bluetooth=(), local-fonts=()";

pub(crate) fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn key_valid(s: &str) -> bool {
    (16..=128).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}
pub(crate) fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
pub(crate) fn identity<T: serde::Serialize>(v: &T) -> serde_json::Result<String> {
    Ok(hash(&serde_json::to_vec(v)?))
}
pub(crate) fn random() -> std::io::Result<String> {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).map_err(|_| std::io::Error::other("randomness unavailable"))?;
    Ok(b.iter().map(|v| format!("{v:02x}")).collect())
}
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
pub(crate) async fn signal() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {v=tokio::signal::ctrl_c()=>v,_=term.recv()=>Ok(())}
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await
    }
}
pub(crate) async fn connection<F, Fut>(stream: tokio::net::TcpStream, route: F)
where
    F: Fn(Request<Incoming>) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Response<Full<Bytes>>> + Send,
{
    let service = service_fn(move |r| {
        let f = route(r);
        async move { Ok::<_, Infallible>(f.await) }
    });
    let mut builder = hyper::server::conn::http1::Builder::new();
    builder
        .keep_alive(false)
        .max_headers(MAX_HEADERS)
        .max_buf_size(HEADER_BYTES);
    let _ = tokio::time::timeout(
        Duration::from_secs(CONNECTION_SECONDS),
        builder.serve_connection(TokioIo::new(stream), service),
    )
    .await;
}
pub(crate) fn response(
    status: StatusCode,
    body: impl Into<Bytes>,
    media: &'static str,
) -> Response<Full<Bytes>> {
    let mut r = Response::new(Full::new(body.into()));
    *r.status_mut() = status;
    for (k, v) in [
        (header::CONTENT_TYPE, media),
        (header::CACHE_CONTROL, "no-store"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::CONNECTION, "close"),
        (header::CONTENT_SECURITY_POLICY, PUBLIC_CSP),
    ] {
        r.headers_mut()
            .insert(k, header::HeaderValue::from_static(v));
    }
    r.headers_mut().insert(
        "cross-origin-resource-policy",
        header::HeaderValue::from_static("same-origin"),
    );
    r.headers_mut().insert(
        "permissions-policy",
        header::HeaderValue::from_static(PERMISSIONS),
    );
    r
}
pub(crate) fn json(status: StatusCode, v: impl serde::Serialize) -> Response<Full<Bytes>> {
    match serde_json::to_vec(&v) {
        Ok(b) => response(status, b, "application/json"),
        Err(_) => error("unavailable"),
    }
}
pub(crate) fn error(code: &'static str) -> Response<Full<Bytes>> {
    let status = match code {
        "unauthorized" => StatusCode::UNAUTHORIZED,
        "invalid_channel" => StatusCode::FORBIDDEN,
        "not_found" | "bundle_not_found" | "artifact_not_found" => StatusCode::NOT_FOUND,
        "ended" => StatusCode::GONE,
        "host_mismatch"
        | "idempotency_conflict"
        | "cancelled_request"
        | "chunk_conflict"
        | "not_uploading"
        | "missing_chunks"
        | "already_ready"
        | "not_ready"
        | "already_claimed" => StatusCode::CONFLICT,
        "busy" | "unavailable" => StatusCode::SERVICE_UNAVAILABLE,
        "body_limit" => StatusCode::PAYLOAD_TOO_LARGE,
        _ => StatusCode::BAD_REQUEST,
    };
    json(status, serde_json::json!({"error":code}))
}
pub(crate) fn exact_header<'a>(request: &'a Request<Incoming>, name: &str) -> Option<&'a str> {
    (request.headers().get_all(name).iter().count() == 1)
        .then(|| request.headers().get(name)?.to_str().ok())
        .flatten()
}
pub(crate) fn channel(request: &Request<Incoming>, authority: &str) -> bool {
    request.uri().scheme().is_none()
        && request.uri().query().is_none()
        && exact_header(request, "host") == Some(authority)
}
pub(crate) async fn body(
    request: &mut Request<Incoming>,
    max: usize,
) -> std::result::Result<Vec<u8>, &'static str> {
    let mut out = Vec::new();
    while let Some(frame) = request.body_mut().frame().await {
        let frame = frame.map_err(|_| "invalid_body")?;
        let data = frame.into_data().map_err(|_| "invalid_body")?;
        if out.len() + data.len() > max {
            return Err("body_limit");
        }
        out.extend_from_slice(&data);
    }
    Ok(out)
}
pub(crate) async fn read_json<T: serde::de::DeserializeOwned>(
    r: &mut Request<Incoming>,
) -> std::result::Result<T, &'static str> {
    if exact_header(r, "content-type") != Some("application/json") {
        return Err("json_required");
    }
    serde_json::from_slice(&body(r, MAX_JSON_BYTES).await?).map_err(|_| "invalid_request")
}
pub(crate) fn key(request: &Request<Incoming>) -> std::result::Result<String, &'static str> {
    let key = exact_header(request, "idempotency-key")
        .filter(|k| key_valid(k))
        .ok_or("invalid_idempotency_key")?;
    Ok(key.into())
}
