use super::{contract::*, store::Registry};
use crate::{host_control::*, toolchain::pinned::PinnedWorker};
use http_body_util::Full;
use hyper::{
    Request, Response, StatusCode,
    body::{Bytes, Incoming},
};
use std::{
    net::{SocketAddr, TcpListener},
    path::Path,
    sync::Arc,
    time::Duration,
};
use subtle::ConstantTimeEq;
/// Trusted artifact-only native execution service. Domain state stays outside.
pub struct ExecutionHost {
    listener: TcpListener,
    registry: Arc<Registry>,
    credential: String,
    pinned: PinnedWorker,
}
impl ExecutionHost {
    pub fn bind(c: ExecutionCeiling, token: &str, executable: &Path) -> Result<Self> {
        c.validate()?;
        if !digest(token) {
            return Err(ExecutionError::Invalid("credential"));
        }
        let pinned = PinnedWorker::with_limits(executable, c.address_space_bytes, c.cpu_seconds)?;
        let listener = TcpListener::bind(c.listen)?;
        listener.set_nonblocking(true)?;
        let registry = Registry::new(c, pinned.identity.clone(), pinned.path.clone());
        Ok(Self {
            listener,
            registry,
            credential: hash(format!("Bearer {token}").as_bytes()),
            pinned,
        })
    }
    pub fn address(&self) -> Result<SocketAddr> {
        Ok(self.listener.local_addr()?)
    }
    pub fn host_sha256(&self) -> &str {
        &self.pinned.identity
    }
    pub fn run(self) -> Result<()> {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(self.serve_until(signal()))
    }
    pub async fn serve_until(
        self,
        shutdown: impl std::future::Future<Output = std::io::Result<()>>,
    ) -> Result<()> {
        let authority = self.listener.local_addr()?.to_string();
        let listener = tokio::net::TcpListener::from_std(self.listener)?;
        let guard = Revoke(Arc::clone(&self.registry));
        let mut tasks = tokio::task::JoinSet::new();
        let mut expiry = tokio::time::interval(Duration::from_millis(100));
        tokio::pin!(shutdown);
        let outcome = loop {
            tokio::select! {
                v=&mut shutdown=>break v,
                _=expiry.tick()=>self.registry.expire(),
                _=tasks.join_next(),if !tasks.is_empty()=>{},
                v=listener.accept(),if tasks.len()<8=>{
                    let (stream,_)=match v {Ok(v)=>v,Err(e)=>break Err(e)};
                    let registry=Arc::clone(&self.registry);
                    let credential=self.credential.clone();
                    let authority=authority.clone();
                    tasks.spawn(async move {
                        connection(stream,move |request| {
                            let registry=Arc::clone(&registry);
                            let credential=credential.clone();
                            let authority=authority.clone();
                            async move {route(request,registry,&credential,&authority).await}
                        }).await;
                    });
                },
            }
        };
        tasks.shutdown().await;
        self.registry
            .shutdown()
            .await
            .map_err(ExecutionError::Invalid)?;
        drop(guard);
        drop(self.pinned);
        outcome.map_err(ExecutionError::from)
    }
}
struct Revoke(Arc<Registry>);
impl Drop for Revoke {
    fn drop(&mut self) {
        self.0.revoke();
    }
}
async fn route(
    mut r: Request<Incoming>,
    registry: Arc<Registry>,
    credential: &str,
    authority: &str,
) -> Response<Full<Bytes>> {
    if !channel(&r, authority)
        || r.headers().contains_key("origin")
        || r.headers().contains_key("cookie")
    {
        return error("invalid_channel");
    }
    if !exact_header(&r, "authorization")
        .is_some_and(|v| bool::from(hash(v.as_bytes()).as_bytes().ct_eq(credential.as_bytes())))
    {
        return error("unauthorized");
    }
    let method = r.method().as_str().to_owned();
    let path = r.uri().path().to_owned();
    let parts: Vec<_> = path.split('/').collect();
    if !(method == "PUT"
        || method == "POST"
            && matches!(
                path.as_str(),
                "/v1/artifacts" | "/v1/runs" | "/v1/runs/cancel-by-key"
            ))
        && let Err(e) = body(&mut r, 32768).await
    {
        return error(e);
    }
    if method == "GET" && path == "/v1/info" {
        let c = &registry.ceiling;
        return json(
            StatusCode::OK,
            serde_json::json!({"schemaVersion":1,"hostSha256":registry.host,"target":TARGET,"artifactPolicy":ARTIFACT_POLICY,"policyId":POLICY_ID,"limits":{"maxFiles":2,"maxArtifactBytes":MAX_BYTES,"chunkBytes":CHUNK_BYTES,"maxRuns":c.max_runs,"maxImports":c.max_imports,"maxReceipts":MAX_RECEIPTS,"maxRetainedBytes":MAX_RETAINED,"maxRunSeconds":c.max_run_seconds,"importSeconds":c.import_seconds,"retentionSeconds":c.retention_seconds,"startupDeadlineMs":c.startup_deadline_ms,"validationDeadlineMs":c.validation_deadline_ms}}),
        );
    }
    if method == "POST" && (path == "/v1/artifacts" || path == "/v1/runs") {
        let key = match key(&r) {
            Ok(k) => k,
            Err(e) => return error(e),
        };
        if path == "/v1/artifacts" {
            let request = match read_json::<ImportRequest>(&mut r).await {
                Ok(v) => v,
                Err(e) => return error(e),
            };
            return match registry.prepare(request, &key) {
                Ok(v) => json(StatusCode::ACCEPTED, v),
                Err(e) => error(e),
            };
        }
        let request = match read_json::<StartRequest>(&mut r).await {
            Ok(v) => v,
            Err(e) => return error(e),
        };
        return match registry.start(request, &key) {
            Ok(v) => json(StatusCode::ACCEPTED, v),
            Err(e) => error(e),
        };
    }
    if method == "POST" && path == "/v1/runs/cancel-by-key" {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        struct Cancel {
            schema_version: u32,
            request_id: String,
        }
        let c = match read_json::<Cancel>(&mut r).await {
            Ok(v) => v,
            Err(e) => return error(e),
        };
        if c.schema_version != 1 {
            return error("invalid_request");
        }
        return match registry.cancel_key(&c.request_id) {
            Ok(()) => json(
                StatusCode::OK,
                serde_json::json!({"requestId":c.request_id,"stopped":true}),
            ),
            Err(e) => error(e),
        };
    }
    match parts.as_slice() {
        ["", "v1", "artifacts", id] if digest(id) => match method.as_str() {
            "GET" => registry
                .import_status(id)
                .map(|v| json(StatusCode::OK, v))
                .unwrap_or_else(|| error("not_found")),
            "DELETE" => match registry.cancel_import(id) {
                Ok(v) => json(StatusCode::OK, v),
                Err(e) => error(e),
            },
            _ => error("not_found"),
        },
        ["", "v1", "artifacts", id, "seal"] if method == "POST" && digest(id) => {
            match registry.seal(id) {
                Ok(v) => json(StatusCode::ACCEPTED, v),
                Err(e) => error(e),
            }
        }
        ["", "v1", "artifacts", id, "files", file, "chunks", chunk]
            if method == "PUT" && digest(id) =>
        {
            if exact_header(&r, "content-type") != Some("application/octet-stream") {
                return error("binary_required");
            }
            let (Ok(file), Ok(chunk)) = (file.parse(), chunk.parse()) else {
                return error("invalid_chunk");
            };
            let b = match body(&mut r, CHUNK_BYTES).await {
                Ok(b) => b,
                Err(e) => return error(e),
            };
            match registry.chunk(id, file, chunk, b) {
                Ok(()) => response(
                    StatusCode::NO_CONTENT,
                    Bytes::new(),
                    "application/octet-stream",
                ),
                Err(e) => error(e),
            }
        }
        ["", "v1", "runs", id] if method == "GET" && digest(id) => registry
            .status(id)
            .map(|v| json(StatusCode::OK, v))
            .unwrap_or_else(|| error("not_found")),
        ["", "v1", "runs", id, "stop"] if method == "POST" && digest(id) => match registry.stop(id)
        {
            Ok(v) => json(StatusCode::OK, v),
            Err(e) => error(e),
        },
        _ => error("not_found"),
    }
}
