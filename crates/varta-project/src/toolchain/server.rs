use super::pinned::PinnedWorker;
use super::{contract::*, store::Registry};
use http_body_util::{BodyExt, Full};
use hyper::{
    Request, Response, StatusCode,
    body::{Bytes, Incoming},
    header,
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use std::{
    convert::Infallible,
    net::{SocketAddr, TcpListener},
    path::Path,
    sync::Arc,
    time::Duration,
};
use subtle::ConstantTimeEq;

const MAX_CONNECTIONS: usize = 16;
const CONNECTION_SECONDS: u64 = 5;
/// An explicit local service for a trusted server-side client. No guest API or
/// source authority is installed in the native runtime or execution launcher.
pub struct ToolchainHost {
    listener: TcpListener,
    registry: Arc<Registry>,
    credential_hash: String,
    pinned: PinnedWorker,
}
impl ToolchainHost {
    /// `executable` is trusted host configuration and must implement the hidden
    /// worker entry point. A private immutable copy is hashed before binding.
    pub fn bind(ceiling: ToolchainCeiling, token: &str, executable: &Path) -> Result<Self> {
        ceiling.validate()?;
        if !digest_valid(token) {
            return Err(ToolchainError::Invalid("toolchain credential"));
        }
        let pinned = PinnedWorker::new(executable, &ceiling)?;
        let listener = TcpListener::bind(ceiling.listen)?;
        listener.set_nonblocking(true)?;
        let registry = Registry::new(ceiling, pinned.identity.clone(), pinned.path.clone());
        Ok(Self {
            listener,
            registry,
            credential_hash: hash(format!("Bearer {token}").as_bytes()),
            pinned,
        })
    }
    pub fn address(&self) -> Result<SocketAddr> {
        Ok(self.listener.local_addr()?)
    }
    pub fn compiler_sha256(&self) -> &str {
        &self.pinned.identity
    }
    pub fn run(self) -> Result<()> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime.block_on(self.serve_until(shutdown_signal()))
    }
    pub async fn serve_until(
        self,
        shutdown: impl std::future::Future<Output = std::io::Result<()>>,
    ) -> Result<()> {
        let authority = self.listener.local_addr()?.to_string();
        let listener = tokio::net::TcpListener::from_std(self.listener)?;
        let guard = Shutdown(Arc::clone(&self.registry));
        let mut tasks = tokio::task::JoinSet::new();
        let mut expiry = tokio::time::interval(Duration::from_secs(1));
        tokio::pin!(shutdown);
        let result = loop {
            tokio::select! {
                stopped=&mut shutdown=>break stopped.map_err(ToolchainError::from),
                _=expiry.tick()=>self.registry.expire(),
                _=tasks.join_next(),if !tasks.is_empty()=>{},
                accepted=listener.accept(),if tasks.len()<MAX_CONNECTIONS=>{
                    let (stream,_)=match accepted {Ok(pair)=>pair,Err(error)=>break Err(error.into())};
                    let registry=Arc::clone(&self.registry);let expected=self.credential_hash.clone();let authority=authority.clone();
                    tasks.spawn(async move {
                        let service=service_fn(move |request| {let registry=Arc::clone(&registry);let expected=expected.clone();let authority=authority.clone();async move {Ok::<_,Infallible>(route(request,registry,&expected,&authority).await)}});
                        let mut http=hyper::server::conn::http1::Builder::new();http.keep_alive(false).max_headers(32).max_buf_size(8192);
                        let _=tokio::time::timeout(Duration::from_secs(CONNECTION_SECONDS),http.serve_connection(TokioIo::new(stream),service)).await;
                    });
                }
            }
        };
        tasks.shutdown().await;
        self.registry.shutdown().await;
        drop(guard);
        drop(self.pinned);
        result
    }
}
struct Shutdown(Arc<Registry>);
async fn shutdown_signal() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {result=tokio::signal::ctrl_c()=>result,_=terminate.recv()=>Ok(())}
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await
    }
}
impl Drop for Shutdown {
    fn drop(&mut self) {
        self.0.cancel_all();
    }
}
fn response(status: StatusCode, body: Bytes) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(body));
    *response.status_mut() = status;
    for (name, value) in [
        (header::CONTENT_TYPE, "application/json"),
        (header::CACHE_CONTROL, "no-store"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; frame-ancestors 'none'",
        ),
        (header::CONTENT_DISPOSITION, "attachment"),
        (header::CONNECTION, "close"),
    ] {
        response
            .headers_mut()
            .insert(name, header::HeaderValue::from_static(value));
    }
    response
}
fn json(status: StatusCode, value: impl serde::Serialize) -> Response<Full<Bytes>> {
    match serde_json::to_vec(&value) {
        Ok(bytes) => response(status, Bytes::from(bytes)),
        Err(_) => response(StatusCode::INTERNAL_SERVER_ERROR, Bytes::from_static(b"{}")),
    }
}
fn error(status: StatusCode, code: &'static str) -> Response<Full<Bytes>> {
    json(status, serde_json::json!({"error":code}))
}
async fn route(
    mut request: Request<Incoming>,
    registry: Arc<Registry>,
    expected: &str,
    authority: &str,
) -> Response<Full<Bytes>> {
    let headers = request.headers();
    if request.uri().scheme().is_some()
        || request.uri().query().is_some()
        || headers.contains_key(header::ORIGIN)
        || headers.contains_key(header::COOKIE)
        || headers.get_all(header::HOST).iter().count() != 1
        || headers.get(header::HOST).and_then(|v| v.to_str().ok()) != Some(authority)
    {
        return error(StatusCode::FORBIDDEN, "invalid_channel");
    }
    if headers.get_all(header::AUTHORIZATION).iter().count() != 1
        || headers.get(header::AUTHORIZATION).is_none_or(|value| {
            !bool::from(hash(value.as_bytes()).as_bytes().ct_eq(expected.as_bytes()))
        })
    {
        return error(StatusCode::UNAUTHORIZED, "unauthorized");
    }
    let path = request.uri().path().to_owned();
    let method = request.method().as_str();
    if method == "GET" && path == "/v1/info" {
        let mut info = serde_json::json!({"schemaVersion":SCHEMA_VERSION,"compilerSha256":registry.compiler,"target":TARGET,"dependencyPolicy":DEPENDENCY_POLICY,"operations":registry.ceiling.operations,"profiles":registry.ceiling.profiles,"limits":{"maxFiles":MAX_FILES,"maxFileBytes":MAX_FILE_BYTES,"maxSourceBytes":MAX_SOURCE_BYTES,"maxRequestBytes":MAX_REQUEST_BYTES,"maxArtifactBytes":MAX_ARTIFACT_BYTES,"maxJobs":registry.ceiling.max_jobs,"maxReceipts":MAX_RECEIPTS,"maxRetainedBytes":MAX_RETAINED_BYTES,"deadlineMs":registry.ceiling.deadline_ms,"retentionSeconds":registry.ceiling.retention_seconds}});
        if let Some(origins) = &registry.ceiling.application_origins {
            info["applicationPolicy"] = serde_json::json!({
                "dependencyPolicy": crate::application::DEPENDENCY_POLICY,
                "publicOrigins": origins,
                "environmentName": crate::application::DATA_URL_ENV,
                "access": ["read", "read-write"]
            });
        }
        return json(StatusCode::OK, info);
    }
    if method == "POST" && path == "/v1/jobs" {
        let key = request
            .headers()
            .get("idempotency-key")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        if request.headers().get_all("idempotency-key").iter().count() != 1
            || !idempotency_valid(&key)
        {
            return error(StatusCode::BAD_REQUEST, "invalid_idempotency_key");
        }
        if request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            != Some("application/json")
        {
            return error(StatusCode::UNSUPPORTED_MEDIA_TYPE, "json_required");
        }
        let mut bytes = Vec::new();
        while let Some(frame) = request.body_mut().frame().await {
            let Ok(frame) = frame else {
                return error(StatusCode::BAD_REQUEST, "invalid_body");
            };
            if let Ok(data) = frame.into_data() {
                if bytes.len() + data.len() > MAX_REQUEST_BYTES {
                    return error(StatusCode::PAYLOAD_TOO_LARGE, "body_limit");
                }
                bytes.extend_from_slice(&data);
            } else {
                return error(StatusCode::BAD_REQUEST, "body_trailers_refused");
            }
        }
        let Ok(request) = serde_json::from_slice::<CompileRequest>(&bytes) else {
            return error(StatusCode::BAD_REQUEST, "invalid_request");
        };
        return match registry.submit(request, &key) {
            Ok(job) => json(StatusCode::ACCEPTED, job),
            Err(code) => error(
                match code {
                    "compiler_mismatch" | "idempotency_conflict" => StatusCode::CONFLICT,
                    "denied" => StatusCode::FORBIDDEN,
                    "busy" => StatusCode::SERVICE_UNAVAILABLE,
                    "invalid_request" => StatusCode::BAD_REQUEST,
                    _ => StatusCode::INTERNAL_SERVER_ERROR,
                },
                code,
            ),
        };
    }
    if let Some(tail) = path.strip_prefix("/v1/jobs/") {
        let (id, cancel) = tail
            .strip_suffix("/cancel")
            .map_or((tail, false), |id| (id, true));
        if !digest_valid(id) {
            return error(StatusCode::NOT_FOUND, "not_found");
        }
        if method == "DELETE" && !cancel {
            return match registry.forget(id) {
                Ok(true) => json(StatusCode::OK, serde_json::json!({"forgotten":true})),
                Ok(false) => error(StatusCode::NOT_FOUND, "not_found"),
                Err(code) => error(StatusCode::CONFLICT, code),
            };
        }
        let job = if method == "POST" && cancel {
            registry.cancel(id)
        } else if method == "GET" && !cancel {
            registry.status(id)
        } else {
            return error(StatusCode::METHOD_NOT_ALLOWED, "method_not_allowed");
        };
        return job.map_or_else(
            || error(StatusCode::NOT_FOUND, "not_found"),
            |job| json(StatusCode::OK, job),
        );
    }
    if method == "GET"
        && let Some(tail) = path.strip_prefix("/v1/artifacts/")
        && let Some((digest, path)) = tail.split_once('/')
        && digest_valid(digest)
        && valid_path(path)
        && let Some(bytes) = registry.file(digest, path)
    {
        let mut reply = response(StatusCode::OK, bytes);
        reply.headers_mut().insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/octet-stream"),
        );
        return reply;
    }
    error(StatusCode::NOT_FOUND, "not_found")
}
