//! Local, memory-only development delivery. No request is a filesystem path.
use crate::{DecisionFile, HostPolicy, ProjectError, Result};
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::{Method, Request, Response, StatusCode, body::Incoming, header, service::service_fn};
use hyper_util::rt::TokioIo;
use std::{
    convert::Infallible,
    future::Future,
    net::{Ipv4Addr, SocketAddr, TcpListener},
    path::Path,
    sync::{
        Arc, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

/// Stack of a rebuild thread: the 8 MiB main-thread stack the first build
/// runs on (macOS and Linux defaults). The compiler is bounded for that
/// stack; Tokio's 2 MiB blocking-pool default overflowed rebuilding the
/// UARP console.
const REBUILD_STACK_BYTES: usize = 8 * 1024 * 1024;
const MAX_CONNECTIONS: usize = 16;
const MAX_HEADERS: usize = 32;
const HEADER_BUFFER_BYTES: usize = 8192;
const CONNECTION_DEADLINE: Duration = Duration::from_secs(5);
mod bootstrap;
mod rebuild;
mod revisions;
use bootstrap::{install_bootstrap, runtime_inputs};
use rebuild::Rebuilder;
use revisions::{Delivery, Revision};

const LOADER: &str = "varta-host.js";
const INDEX: &str = "index.html";

/// Local browser host options. Policies and decisions are separate runtime inputs.
pub struct BrowserDevOptions {
    pub address: SocketAddr,
    pub application: bool,
    pub report_budgets: bool,
}
impl Default for BrowserDevOptions {
    fn default() -> Self {
        Self {
            address: (Ipv4Addr::LOCALHOST, 0).into(),
            application: false,
            report_budgets: false,
        }
    }
}

struct File {
    bytes: Bytes,
    media: &'static str,
    length: header::HeaderValue,
    etag: header::HeaderValue,
}

/// A checked immutable bundle, served only on loopback. Preparation never executes
/// the guest, binds native roots, writes output, or serves project/source files.
pub struct BrowserDevServer {
    listener: TcpListener,
    address: SocketAddr,
    delivery: Arc<RwLock<Delivery>>,
    rebuilder: Rebuilder,
    cancelled: Arc<AtomicBool>,
    tls: Option<varta_runtime::tls::Http1TlsAcceptor>,
}
impl BrowserDevServer {
    pub fn bind(
        config: &Path,
        policy: Option<&HostPolicy>,
        decisions: Option<&DecisionFile>,
        options: BrowserDevOptions,
    ) -> Result<Self> {
        if !options.address.ip().is_loopback() {
            return Err(ProjectError::Usage(
                "browser development binds only to loopback".into(),
            ));
        }
        let http = policy.and_then(|p| p.manifest().http.as_ref());
        if http.is_some_and(|http| http.limits.is_some() || http.session.is_some()) {
            return Err(ProjectError::Usage("browser development accepts http.tls only; native HTTP limits/session settings do not apply".into()));
        }
        let tls = http
            .and_then(|http| http.tls.as_ref())
            .map(varta_runtime::tls::Http1TlsAcceptor::from_files)
            .transpose()?;
        if let Some(decisions) = decisions {
            decisions.validate()?;
        }
        let address = options.address;
        let cancelled = Arc::new(AtomicBool::new(false));
        let (rebuilder, revision) =
            Rebuilder::new(config, policy, decisions, options, cancelled.clone())?;
        let delivery = Delivery::new(revision);
        let listener = TcpListener::bind(address)?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        Ok(Self {
            listener,
            address,
            delivery: Arc::new(RwLock::new(delivery)),
            rebuilder,
            cancelled,
            tls,
        })
    }
    #[must_use]
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    #[must_use]
    pub fn url(&self) -> String {
        format!("{}://{}/", self.scheme(), self.address)
    }

    fn scheme(&self) -> &'static str {
        if self.tls.is_some() { "https" } else { "http" }
    }

    /// This snapshot's immutable asset directory. The root page refers to it;
    /// bare asset names are deliberately unavailable at the mutable root.
    #[must_use]
    pub fn bundle_url(&self) -> String {
        format!(
            "{}://{}{}",
            self.scheme(),
            self.address,
            self.delivery
                .read()
                .expect("delivery lock is unpoisoned")
                .latest()
                .prefix()
        )
    }

    /// Serve until Ctrl-C, closing active connections before returning.
    pub fn run(self) -> Result<()> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            // Rebuilds compile on the blocking pool; give its threads the
            // stack the first build gets on the main thread.
            .thread_stack_size(REBUILD_STACK_BYTES)
            .build()?;
        let result = runtime.block_on(self.serve_until(tokio::signal::ctrl_c()));
        runtime.shutdown_background();
        result
    }

    /// Embedding/test seam. Poll inside a Tokio runtime; shutdown closes active
    /// connections and releases the listening socket without waiting for clients.
    pub async fn serve_until(
        self,
        shutdown: impl Future<Output = std::io::Result<()>>,
    ) -> Result<()> {
        let scheme = self.scheme();
        let listener = tokio::net::TcpListener::from_std(self.listener)?;
        let tls = self.tls;
        let authority = self.address.to_string();
        let origin = format!("{scheme}://{authority}");
        let mut tasks = tokio::task::JoinSet::new();
        let mut rebuilder = Some(self.rebuilder);
        let mut builds = tokio::task::JoinSet::new();
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        tokio::pin!(shutdown);
        let result = loop {
            tokio::select! {
                biased;
                stopped = &mut shutdown => break stopped.map_err(ProjectError::from),
                completed = builds.join_next(), if !builds.is_empty() => {
                    match completed {
                        Some(Ok((state, result))) => {
                            rebuilder = Some(state);
                            if let Some(result) = result {
                                let Ok(mut delivery) = self.delivery.write() else { break Err(ProjectError::Usage("development delivery lock failed".into())); };
                                delivery.complete(result);
                            }
                        }
                        Some(Err(error)) => break Err(ProjectError::Usage(format!("development rebuild failed: {error}"))),
                        None => {}
                    }
                }
                _ = interval.tick(), if rebuilder.is_some() => {
                    let state = rebuilder.take().expect("one rebuild worker");
                    builds.spawn_blocking(move || state.tick());
                }
                completed = tasks.join_next(), if !tasks.is_empty() => {
                    if let Some(Err(error)) = completed { break Err(ProjectError::Usage(format!("browser development connection failed: {error}"))); }
                }
                accepted = listener.accept(), if tasks.len() < MAX_CONNECTIONS => {
                    let (stream, _) = match accepted { Ok(value) => value, Err(error) => break Err(error.into()) };
                    let delivery = Arc::clone(&self.delivery); let authority = authority.clone(); let origin = origin.clone();
                    let tls = tls.clone();
                    tasks.spawn(async move {
                        let connection = async move {
                            match tls {
                                Some(tls) => {
                                    if let Ok(stream) = tls.accept(stream).await {
                                        serve_connection(stream, delivery, authority, origin).await;
                                    }
                                }
                                None => serve_connection(stream, delivery, authority, origin).await,
                            }
                        };
                        let _ = tokio::time::timeout(CONNECTION_DEADLINE, connection).await;
                    });
                }
            }
        };
        self.cancelled.store(true, Ordering::Release);
        builds.abort_all();
        tasks.shutdown().await;
        result
    }
}

async fn serve_connection<T>(
    stream: T,
    delivery: Arc<RwLock<Delivery>>,
    authority: String,
    origin: String,
) where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let service = service_fn(move |request| {
        let response = match delivery.read() {
            Ok(delivery) => serve(request, &delivery, &authority, &origin),
            Err(_) => error(StatusCode::INTERNAL_SERVER_ERROR),
        };
        async move { Ok::<_, Infallible>(response) }
    });
    let mut http = hyper::server::conn::http1::Builder::new();
    http.keep_alive(false)
        .max_headers(MAX_HEADERS)
        .max_buf_size(HEADER_BUFFER_BYTES);
    let _ = http.serve_connection(TokioIo::new(stream), service).await;
}

fn media_type(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("wasm") => "application/wasm",
        Some("png") => "image/png",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn serve(
    request: Request<Incoming>,
    delivery: &Delivery,
    authority: &str,
    origin: &str,
) -> Response<Full<Bytes>> {
    let mut response = if request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        != Some(authority)
        || request.uri().scheme().is_some()
        || request.uri().authority().is_some()
        || !request.uri().path().starts_with('/')
    {
        error(StatusCode::BAD_REQUEST)
    } else if request
        .headers()
        .get(header::ORIGIN)
        .is_some_and(|v| v.as_bytes() != origin.as_bytes())
        || request.headers().get("sec-fetch-site").is_some_and(|v| {
            ![b"same-origin".as_slice(), b"none".as_slice()].contains(&v.as_bytes())
        })
    {
        error(StatusCode::FORBIDDEN)
    } else if request.method() != Method::GET && request.method() != Method::HEAD {
        let mut response = error(StatusCode::METHOD_NOT_ALLOWED);
        response
            .headers_mut()
            .insert(header::ALLOW, header::HeaderValue::from_static("GET, HEAD"));
        response
    } else if request.headers().contains_key(header::TRANSFER_ENCODING)
        || request
            .headers()
            .get(header::CONTENT_LENGTH)
            .is_some_and(|v| v.as_bytes() != b"0")
    {
        error(StatusCode::BAD_REQUEST)
    } else {
        let status;
        let file = if request.uri().path() == "/__varta/status" {
            status = delivery.status();
            Some(&status)
        } else {
            delivery.file(request.uri().path())
        };
        match file {
            None => error(StatusCode::NOT_FOUND),
            Some(file) => {
                let cached = request.uri().path() != "/__varta/status"
                    && request.headers().get(header::IF_NONE_MATCH) == Some(&file.etag);
                let body = if cached || request.method() == Method::HEAD {
                    Bytes::new()
                } else {
                    file.bytes.clone()
                };
                let mut response = Response::new(Full::new(body));
                if cached {
                    *response.status_mut() = StatusCode::NOT_MODIFIED;
                } else {
                    response
                        .headers_mut()
                        .insert(header::CONTENT_LENGTH, file.length.clone());
                }
                response.headers_mut().insert(
                    header::CONTENT_TYPE,
                    header::HeaderValue::from_static(file.media),
                );
                response
                    .headers_mut()
                    .insert(header::ETAG, file.etag.clone());
                response
            }
        }
    };
    let headers = response.headers_mut();
    for (name, value) in [
        (header::CACHE_CONTROL, "no-store"),
        (header::CONNECTION, "close"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::CONTENT_SECURITY_POLICY, "frame-ancestors 'none'"),
    ] {
        headers.insert(name, header::HeaderValue::from_static(value));
    }
    headers.insert(
        "cross-origin-resource-policy",
        header::HeaderValue::from_static("same-origin"),
    );
    response
}
fn error(status: StatusCode) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from_static(
        status
            .canonical_reason()
            .unwrap_or("request rejected")
            .as_bytes(),
    )));
    *response.status_mut() = status;
    response
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tls_tests;
