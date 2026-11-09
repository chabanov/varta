use super::{contract::*, store::Registry};
use crate::host_control::{
    body, channel, connection, error, exact_header, json, key, read_json, response, signal,
};
use crate::toolchain::pinned::PinnedWorker;
use base64::Engine;
use http_body_util::Full;
use hyper::{
    Request, Response, StatusCode,
    body::{Bytes, Incoming},
    header,
};
use sha2::{Digest, Sha256};
use std::{
    net::{SocketAddr, TcpListener},
    path::Path,
    sync::Arc,
    time::Duration,
};
use subtle::ConstantTimeEq;

const MAX_CONTROL_CONNECTIONS: usize = 8;
const MAX_PUBLIC_CONNECTIONS: usize = 16;
const ACTIVATION_JS: &str = include_str!("activation.js");

/// Trusted local, source-free browser activation and lifecycle service.
pub struct PreviewHost {
    control: TcpListener,
    public: TcpListener,
    registry: Arc<Registry>,
    credential: String,
    pinned: PinnedWorker,
}
impl PreviewHost {
    /// The executable is host-owned and must implement the hidden validator.
    pub fn bind(ceiling: PreviewCeiling, token: &str, executable: &Path) -> Result<Self> {
        Self::bind_inner(ceiling, token, executable, None)
    }
    pub fn bind_application(
        ceiling: crate::application::ApplicationCeiling,
        token: &str,
        executable: &Path,
    ) -> Result<Self> {
        let upstream = crate::application::Upstream::new(ceiling.clone())?;
        Self::bind_inner(ceiling.host, token, executable, Some(upstream))
    }
    fn bind_inner(
        ceiling: PreviewCeiling,
        token: &str,
        executable: &Path,
        application: Option<crate::application::Upstream>,
    ) -> Result<Self> {
        ceiling.validate()?;
        if !digest(token) {
            return Err(PreviewError::Invalid("credential"));
        }
        let pinned = PinnedWorker::with_limits(
            executable,
            ceiling.address_space_bytes,
            ceiling.cpu_seconds,
        )?;
        let control = TcpListener::bind(ceiling.control_listen)?;
        let public = TcpListener::bind(ceiling.public_listen)?;
        control.set_nonblocking(true)?;
        public.set_nonblocking(true)?;
        let origin = application.as_ref().map_or_else(
            || {
                format!(
                    "http://{}",
                    public.local_addr().expect("bound public listener")
                )
            },
            |upstream| upstream.ceiling.public_origin.clone(),
        );
        let registry = Registry::with_application(
            ceiling,
            pinned.identity.clone(),
            origin,
            pinned.path.clone(),
            application,
        )?;
        Ok(Self {
            control,
            public,
            registry,
            credential: hash(format!("Bearer {token}").as_bytes()),
            pinned,
        })
    }
    pub fn control_address(&self) -> Result<SocketAddr> {
        Ok(self.control.local_addr()?)
    }
    pub fn public_address(&self) -> Result<SocketAddr> {
        Ok(self.public.local_addr()?)
    }
    pub fn public_origin(&self) -> &str {
        &self.registry.origin
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
        let control_authority = self.control.local_addr()?.to_string();
        let public_authority = self
            .registry
            .origin
            .split_once("://")
            .map(|(_, authority)| authority.to_owned())
            .ok_or(PreviewError::Invalid("public authority"))?;
        let control = tokio::net::TcpListener::from_std(self.control)?;
        let public = tokio::net::TcpListener::from_std(self.public)?;
        let guard = Revoke(Arc::clone(&self.registry));
        let mut control_tasks = tokio::task::JoinSet::new();
        let mut public_tasks = tokio::task::JoinSet::new();
        let mut expiry = tokio::time::interval(Duration::from_secs(1));
        tokio::pin!(shutdown);
        let outcome = loop {
            tokio::select! {
                result=&mut shutdown=>break result,
                _=expiry.tick()=>self.registry.expire(),
                _=control_tasks.join_next(),if !control_tasks.is_empty()=>{},
                _=public_tasks.join_next(),if !public_tasks.is_empty()=>{},
                accepted=control.accept(),if control_tasks.len()<MAX_CONTROL_CONNECTIONS=>{
                    let (stream,_)=match accepted{Ok(v)=>v,Err(e)=>break Err(e)};
                    let registry=Arc::clone(&self.registry);let credential=self.credential.clone();let authority=control_authority.clone();
                    control_tasks.spawn(async move{connection(stream,move|request|{
                        let registry=Arc::clone(&registry);let credential=credential.clone();let authority=authority.clone();
                        async move{control_route(request,registry,&credential,&authority).await}
                    }).await;});
                },
                accepted=public.accept(),if public_tasks.len()<MAX_PUBLIC_CONNECTIONS=>{
                    let (stream,_)=match accepted{Ok(v)=>v,Err(e)=>break Err(e)};
                    let registry=Arc::clone(&self.registry);let authority=public_authority.clone();
                    public_tasks.spawn(async move{connection(stream,move|request|{
                        let registry=Arc::clone(&registry);let authority=authority.clone();
                        async move{public_route(request,registry,&authority).await}
                    }).await;});
                }
            }
        };
        control_tasks.shutdown().await;
        public_tasks.shutdown().await;
        self.registry.shutdown().await;
        drop(guard);
        drop(self.pinned);
        outcome.map_err(PreviewError::from)
    }
}
struct Revoke(Arc<Registry>);
impl Drop for Revoke {
    fn drop(&mut self) {
        self.0.revoke();
    }
}
fn run_reply(registry: &Registry, status: StatusCode, run: RunStatus) -> Response<Full<Bytes>> {
    if registry.application.is_none() {
        return json(status, run);
    }
    match registry.channel_status(&run.id) {
        Ok(channel) => match serde_json::to_value(run) {
            Ok(mut value) => {
                value["channel"] = channel;
                json(status, value)
            }
            Err(_) => error("unavailable"),
        },
        Err(e) => error(e),
    }
}
async fn control_route(
    mut request: Request<Incoming>,
    registry: Arc<Registry>,
    credential: &str,
    authority: &str,
) -> Response<Full<Bytes>> {
    if !channel(&request, authority)
        || request.headers().contains_key("origin")
        || request.headers().contains_key("cookie")
    {
        return error("invalid_channel");
    }
    if !exact_header(&request, "authorization")
        .is_some_and(|v| bool::from(hash(v.as_bytes()).as_bytes().ct_eq(credential.as_bytes())))
    {
        return error("unauthorized");
    }
    let path = request.uri().path().to_owned();
    let parts: Vec<_> = path.split('/').collect();
    let method = request.method().as_str().to_owned();
    // Drain bounded metadata bodies before replying/closing the connection.
    if !(method == "PUT"
        || method == "POST"
            && matches!(
                path.as_str(),
                "/v1/bundles" | "/v1/runs" | "/v1/runs/cancel-by-key"
            ))
        && let Err(e) = body(&mut request, MAX_JSON_BYTES).await
    {
        return error(e);
    }
    if method == "GET" && path == "/v1/info" {
        let c = &registry.ceiling;
        let mut info = serde_json::json!({"schemaVersion":SCHEMA_VERSION,"hostSha256":registry.host,"target":TARGET,"bundlePolicy":if registry.application.is_some(){"official-app-browser-v1"}else{BUNDLE_POLICY},"policyId":if registry.application.is_some(){crate::application::POLICY_ID}else{POLICY_ID},"publicOrigin":registry.origin,
            "limits":{"maxFiles":MAX_FILES,"maxBundleBytes":MAX_BYTES,"chunkBytes":CHUNK_BYTES,"maxImports":c.max_imports,"maxRuns":c.max_runs,"maxReceipts":MAX_RECEIPTS,"maxRetainedBytes":MAX_RETAINED_BYTES,"maxRunSeconds":c.max_run_seconds,"importSeconds":c.import_seconds,"retentionSeconds":c.retention_seconds,"validationDeadlineMs":c.validation_deadline_ms,"ticketSeconds":TICKET_MS/POLL_MS,"leaseMs":LEASE_MS,"pollMs":POLL_MS}});
        if registry.application.is_some() {
            info["hostInstanceId"] = serde_json::json!(registry.host_instance);
        }
        return json(StatusCode::OK, info);
    }
    if method == "POST" && (path == "/v1/bundles" || path == "/v1/runs") {
        let key = match key(&request) {
            Ok(v) => v,
            Err(e) => return error(e),
        };
        if path == "/v1/bundles" {
            let body = match read_json::<ImportRequest>(&mut request).await {
                Ok(v) => v,
                Err(e) => return error(e),
            };
            return match registry.prepare(body, &key) {
                Ok(s) => json(StatusCode::ACCEPTED, s),
                Err(e) => error(e),
            };
        }
        if registry.application.is_some() {
            let body = match read_json::<crate::application::ChannelStart>(&mut request).await {
                Ok(v) => v,
                Err(e) => return error(e),
            };
            return match registry.start_application(body, &key).await {
                Ok(s) => run_reply(&registry, StatusCode::ACCEPTED, s),
                Err(e) => error(e),
            };
        }
        let body = match read_json::<StartRequest>(&mut request).await {
            Ok(v) => v,
            Err(e) => return error(e),
        };
        return match registry.start(body, &key) {
            Ok(s) => json(StatusCode::ACCEPTED, s),
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
        let c = match read_json::<Cancel>(&mut request).await {
            Ok(v) => v,
            Err(e) => return error(e),
        };
        if c.schema_version != SCHEMA_VERSION {
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
        ["", "v1", "runs", id, "readiness"]
            if method == "GET" && digest(id) && registry.application.is_some() =>
        {
            match registry.readiness(id).await {
                Ok(value) => json(StatusCode::OK, value),
                Err(e) => error(e),
            }
        }
        ["", "v1", "bundles", id] if digest(id) => match method.as_str() {
            "GET" => registry
                .import_status(id)
                .map(|s| json(StatusCode::OK, s))
                .unwrap_or_else(|| error("not_found")),
            "DELETE" => match registry.cancel_import(id) {
                Ok(s) => json(StatusCode::OK, s),
                Err(e) => error(e),
            },
            _ => error("not_found"),
        },
        ["", "v1", "bundles", id, "seal"] if method == "POST" && digest(id) => {
            match registry.seal(id) {
                Ok(s) => json(StatusCode::ACCEPTED, s),
                Err(e) => error(e),
            }
        }
        ["", "v1", "bundles", id, "files", file, "chunks", chunk]
            if method == "PUT" && digest(id) =>
        {
            if exact_header(&request, "content-type") != Some("application/octet-stream") {
                return error("binary_required");
            }
            let (Ok(file), Ok(chunk)) = (file.parse(), chunk.parse()) else {
                return error("invalid_chunk");
            };
            let b = match body(&mut request, CHUNK_BYTES).await {
                Ok(v) => v,
                Err(e) => return error(e),
            };
            match registry.chunk(id, file, chunk, Bytes::from(b)) {
                Ok(()) => response(
                    StatusCode::NO_CONTENT,
                    Bytes::new(),
                    "application/octet-stream",
                ),
                Err(e) => error(e),
            }
        }
        ["", "v1", "runs", id] if method == "GET" && digest(id) => {
            if registry.application.is_some()
                && registry.status(id).is_some_and(|s| !s.state.terminal())
            {
                let _ = registry.check_application(id).await;
            }
            registry
                .status(id)
                .map(|s| run_reply(&registry, StatusCode::OK, s))
                .unwrap_or_else(|| error("not_found"))
        }
        ["", "v1", "runs", id, "stop"] if method == "POST" && digest(id) => match registry.stop(id)
        {
            Ok(s) => run_reply(&registry, StatusCode::OK, s),
            Err(e) => error(e),
        },
        ["", "v1", "runs", id, "tickets"] if method == "POST" && digest(id) => {
            if let Err(e) = registry.check_application(id).await {
                return error(e);
            }
            match registry.ticket(id) {
                Ok((launch_url, expires_at_ms)) => json(
                    StatusCode::OK,
                    serde_json::json!({"launchUrl":launch_url,"expiresAtMs":expires_at_ms}),
                ),
                Err(e) => error(e),
            }
        }
        _ => error("not_found"),
    }
}
fn cookie<'a>(request: &'a Request<Incoming>, registry: &Registry) -> Option<&'a str> {
    let value = exact_header(request, "cookie")?;
    session_cookie(
        value,
        registry.cookie_name(),
        registry.application.is_some(),
    )
}
fn session_cookie<'a>(header: &'a str, name: &str, application: bool) -> Option<&'a str> {
    if !application {
        let (key, value) = header.split_once('=')?;
        return (key == name && digest(value)).then_some(value);
    }
    let mut found = None;
    for part in header.split(';') {
        let part = part.trim();
        let Some((key, value)) = part.split_once('=') else {
            if part == name {
                return None;
            }
            continue;
        };
        if key.trim() == name {
            if key != name || found.is_some() || !digest(value) {
                return None;
            }
            found = Some(value);
        }
    }
    found
}
fn media(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".js") {
        "text/javascript; charset=utf-8"
    } else if path.ends_with(".wasm") {
        "application/wasm"
    } else if path.ends_with(".json") {
        "application/json"
    } else {
        "application/octet-stream"
    }
}
async fn public_route(
    mut request: Request<Incoming>,
    registry: Arc<Registry>,
    authority: &str,
) -> Response<Full<Bytes>> {
    let app_data = registry.application.is_some()
        && request.uri().path().split('/').nth(3) == Some("data")
        && request.uri().path().starts_with("/runs/");
    if !channel(&request, authority)
        && !(app_data
            && request.uri().scheme().is_none()
            && exact_header(&request, "host") == Some(authority))
    {
        return error("invalid_channel");
    }
    if request.headers().contains_key("origin")
        && exact_header(&request, "origin") != Some(&registry.origin)
    {
        return error("invalid_channel");
    }
    let method = request.method().as_str().to_owned();
    let path = request.uri().path().to_owned();
    if path == "/activate" && method == "POST" {
        if exact_header(&request, "origin") != Some(&registry.origin)
            || (registry.application.is_none() && request.headers().contains_key("cookie"))
        {
            return error("invalid_channel");
        }
        let Some(ticket) =
            exact_header(&request, "authorization").and_then(|v| v.strip_prefix("Bearer "))
        else {
            return error("unauthorized");
        };
        let ticket = ticket.to_owned();
        if let Err(e) = body(&mut request, MAX_JSON_BYTES).await {
            return error(e);
        }
        if registry.application.is_some() {
            let id = match registry.ticket_run(&ticket) {
                Ok(id) => id,
                Err(e) => return error(e),
            };
            if let Err(e) = registry.check_application(&id).await {
                return error(e);
            }
        }
        return match registry.activate(&ticket) {
            Ok((id, token, seconds)) => {
                let mut r = json(
                    StatusCode::OK,
                    serde_json::json!({"url":format!("/runs/{id}/index.html")}),
                );
                let cookie = format!(
                    "{}={token}; Path=/runs/{id}/; HttpOnly; SameSite=Strict; Max-Age={seconds}",
                    registry.cookie_name()
                );
                match header::HeaderValue::from_str(&cookie) {
                    Ok(v) => {
                        r.headers_mut().insert(header::SET_COOKIE, v);
                        r
                    }
                    Err(_) => error("unavailable"),
                }
            }
            Err(e) => error(e),
        };
    }
    if request.headers().contains_key("authorization") {
        return error("invalid_channel");
    }
    if method == "GET" && (path == "/" || path == "/activation.js") {
        if registry.application.is_none() && request.headers().contains_key("cookie") {
            return error("invalid_channel");
        }
        if path == "/activation.js" {
            return response(
                StatusCode::OK,
                ACTIVATION_JS,
                "text/javascript; charset=utf-8",
            );
        }
        let sri = format!(
            "sha256-{}",
            base64::engine::general_purpose::STANDARD.encode(Sha256::digest(ACTIVATION_JS))
        );
        let page = format!(
            "<!doctype html><html lang=en><meta charset=utf-8><title>Varta preview</title><p id=status>Opening preview…</p><script type=module src=/activation.js integrity=\"{sri}\" crossorigin=anonymous></script></html>"
        );
        let mut r = response(StatusCode::OK, page, "text/html; charset=utf-8");
        let csp = format!(
            "default-src 'none'; script-src '{sri}'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'"
        );
        if let Ok(v) = header::HeaderValue::from_str(&csp) {
            r.headers_mut().insert(header::CONTENT_SECURITY_POLICY, v);
        }
        return r;
    }
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() >= 5
        && parts[0].is_empty()
        && parts[1] == "runs"
        && parts[3] == "data"
        && registry.application.is_some()
    {
        return application_data(request, registry, parts[2]).await;
    }
    let ["", "runs", id, file] = parts.as_slice() else {
        return error("not_found");
    };
    if !digest(id) {
        return error("not_found");
    }
    let Some(cookie) = cookie(&request, &registry).map(str::to_owned) else {
        return error("unauthorized");
    };
    if let Err(e) = registry.authorize_browser(id, &cookie) {
        return error(e);
    }
    if let Err(e) = registry.check_application(id).await {
        return error(e);
    }
    if method == "GET" {
        let name = if file.is_empty() { "index.html" } else { file };
        return match registry.file(id, &cookie, name) {
            Ok(b) => response(StatusCode::OK, b, media(name)),
            Err(e) => error(e),
        };
    }
    if method != "POST" || exact_header(&request, "origin") != Some(&registry.origin) {
        return error("invalid_channel");
    }
    let client = exact_header(&request, "x-varta-preview-client")
        .unwrap_or("")
        .to_owned();
    if *file != "report"
        && let Err(e) = body(&mut request, MAX_JSON_BYTES).await
    {
        return error(e);
    }
    match *file {
        "claim" => match registry.claim(id, &cookie) {
            Ok((token, ms)) => {
                let mut value = serde_json::json!({"leaseToken":token,"leaseMs":ms});
                if registry.application.is_some() {
                    match registry.application_inputs(id) {
                        Ok(inputs) => value["runtime"] = inputs,
                        Err(e) => return error(e),
                    }
                }
                json(StatusCode::OK, value)
            }
            Err(e) => error(e),
        },
        "lease" => match registry.lease(id, &cookie, &client, None) {
            Ok(ms) => json(StatusCode::OK, serde_json::json!({"leaseMs":ms})),
            Err(e) => error(e),
        },
        "report" => {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields, rename_all = "camelCase")]
            struct Report {
                guest_state: String,
            }
            let report = match read_json::<Report>(&mut request).await {
                Ok(v) => v,
                Err(e) => return error(e),
            };
            let guest = match report.guest_state.as_str() {
                "ready" => GuestState::Ready,
                "closed" => GuestState::Closed,
                "failed" => GuestState::Failed,
                _ => return error("invalid_request"),
            };
            match registry.lease(id, &cookie, &client, Some(guest)) {
                Ok(ms) => json(StatusCode::OK, serde_json::json!({"leaseMs":ms})),
                Err(e) => error(e),
            }
        }
        _ => error("not_found"),
    }
}

async fn application_data(
    mut request: Request<Incoming>,
    registry: Arc<Registry>,
    id: &str,
) -> Response<Full<Bytes>> {
    let origin = exact_header(&request, "origin");
    let same_origin = origin == Some(&registry.origin)
        || origin.is_none() && exact_header(&request, "sec-fetch-site") == Some("same-origin");
    if !digest(id)
        || !same_origin
        || exact_header(&request, varta_common::session::HEADER)
            != Some(varta_common::session::HEADER_VALUE)
        || request.headers().contains_key("x-varta-request-proof")
    {
        return error("invalid_channel");
    }
    let Some(cookie) = cookie(&request, &registry).map(str::to_owned) else {
        return error("unauthorized");
    };
    if let Err(e) = registry.authorize_browser(id, &cookie) {
        return error(e);
    }
    if let Err(e) = registry.check_application(id).await {
        return error(e);
    }
    let method = request.method().as_str().to_owned();
    let (scope, cancel) = match registry.data_scope(id, &cookie, &method) {
        Ok(v) => v,
        Err(e) => return error(e),
    };
    let prefix = format!("/runs/{id}/data");
    let target = request.uri().path_and_query().map_or("", |v| v.as_str());
    let Some(suffix) = target.strip_prefix(&prefix).filter(|s| s.starts_with('/')) else {
        return error("invalid_request");
    };
    let native_target = format!("{}{suffix}", scope.path_prefix);
    if !varta_common::session::delegation::target_allowed(&native_target, &scope.path_prefix) {
        return error("invalid_request");
    }
    let mut headers = Vec::new();
    for name in ["content-type", "accept", "idempotency-key"] {
        if request.headers().get_all(name).iter().count() > 1 {
            return error("invalid_request");
        }
        if let Some(value) = exact_header(&request, name) {
            headers.push((name.to_owned(), value.to_owned()));
        }
    }
    if method != "GET" && exact_header(&request, "content-type") != Some("application/json") {
        return error("invalid_request");
    }
    let b = match body(&mut request, 256 * 1024).await {
        Ok(b) => b,
        Err(e) => return error(e),
    };
    if method == "GET" && !b.is_empty() {
        return error("invalid_request");
    }
    let Some(upstream) = &registry.application else {
        return error("invalid_request");
    };
    let path = format!(
        "/__varta/session-delegation/v1/relay/{}{native_target}",
        scope.id
    );
    match upstream.exchange(&path, &method, b, headers, cancel).await {
        Ok((status, bytes)) => {
            if let Err(e) = registry.check_application(id).await {
                return error(e);
            }
            if registry.data_scope(id, &cookie, &method).is_err() {
                return error("ended");
            }
            match StatusCode::from_u16(status) {
                Ok(status) => response(status, bytes, "application/json"),
                Err(_) => error("unavailable"),
            }
        }
        Err(e) => error(e),
    }
}

#[cfg(test)]
mod cookie_tests {
    use super::*;
    #[test]
    fn application_cookie_ignores_ambient_values_but_never_accepts_ambiguous_authentication() {
        let value = "a".repeat(64);
        let header = format!("unrelated=x=y; varta_application={value}; another=ordinary");
        assert_eq!(
            session_cookie(&header, "varta_application", true),
            Some(value.as_str())
        );
        for bad in [
            format!("varta_application={value}; varta_application={value}"),
            format!("varta_application=bad; varta_application={value}"),
            format!("varta_application; varta_application={value}"),
            format!("varta_application ={value}; varta_application={value}"),
            "unrelated=ordinary".into(),
            "varta_application=bad".into(),
        ] {
            assert_eq!(session_cookie(&bad, "varta_application", true), None);
        }
        assert_eq!(
            session_cookie(&format!("varta_preview={value}"), "varta_preview", false),
            Some(value.as_str())
        );
        assert_eq!(
            session_cookie(
                &format!("unrelated=x; varta_preview={value}"),
                "varta_preview",
                false
            ),
            None
        );
    }
}
