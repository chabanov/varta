//! Host-driven HTTP lifetime. Every request gets a fresh bounded guest store.
mod connection;
mod engine;
mod handler;
mod io;
mod report;
mod tls;
mod transport;
use crate::{RunOutput, RuntimeError, policy};
pub use report::{HttpReport, HttpRequestOutcome, HttpRequestReport};
use std::{
    net::{SocketAddr, TcpListener},
    sync::{Arc, Mutex},
};
use varta_common::{
    CapabilityGrant, abi,
    http::{HttpContract, HttpError, HttpLimits, HttpPolicy},
};
use wasmtime::Engine;

#[derive(Clone, Debug)]
pub struct HttpShutdown(tokio::sync::watch::Sender<bool>);
impl HttpShutdown {
    pub fn shutdown(&self) {
        self.0.send_replace(true);
    }
    fn stopped(&self) -> bool {
        *self.0.borrow()
    }
    async fn cancelled(&self) {
        let mut receiver = self.0.subscribe();
        if *receiver.borrow() {
            return;
        }
        let _ = receiver.changed().await;
    }
}
#[derive(Clone, Debug)]
pub enum RuntimeEvent {
    Listening {
        address: SocketAddr,
        shutdown: HttpShutdown,
        /// `https` when the host serves TLS (K7 H3), else `http`.
        scheme: &'static str,
    },
    Output(String),
    Diagnostic(String),
    /// V2 worker evidence, emitted after releasing guest/network resources.
    HttpRequest(Box<HttpRequestReport>),
}
/// Trusted synchronous event sink; return promptly. Slow sinks apply backpressure.
pub type RuntimeObserver = Arc<dyn Fn(RuntimeEvent) + Send + Sync>;

struct Captured {
    output: RunOutput,
    bytes: usize,
}
struct Sink {
    observer: Option<RuntimeObserver>,
    captured: Mutex<Captured>,
    requests: Arc<Mutex<HttpReport>>,
}
impl Sink {
    fn lines(&self, lines: Vec<String>) -> Result<(), RuntimeError> {
        if let Some(observer) = &self.observer {
            for line in lines {
                observer(RuntimeEvent::Output(line));
            }
        } else {
            let mut state = self
                .captured
                .lock()
                .map_err(|_| HttpError::Message("output lock".into()))?;
            for line in lines {
                state.bytes = state.bytes.saturating_add(line.len() + 1);
                if state.bytes > abi::MAX_OUTPUT_BYTES {
                    return Err(HttpError::Message(
                        "aggregate captured output limit; use an observer for long-lived output"
                            .into(),
                    )
                    .into());
                }
                state.output.lines.push(line);
            }
        }
        Ok(())
    }
    fn diagnostic(&self, message: String) {
        if let Some(observer) = &self.observer {
            observer(RuntimeEvent::Diagnostic(message));
        }
    }
    fn request(&self, mut request: HttpRequestReport) {
        self.requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record(&mut request);
        if let Some(observer) = &self.observer {
            observer(RuntimeEvent::HttpRequest(Box::new(request)));
        }
    }
}
struct Shared {
    engine: Engine,
    module: crate::PreparedModule,
    contract: HttpContract,
    /// The contract's limits as the host policy lowered them.
    limits: HttpLimits,
    /// The listener's actual address: a Web `Request`'s authority when the
    /// request names none.
    address: SocketAddr,
    /// The scheme of a Web `Request`'s URL.
    scheme: &'static str,
    /// The `net.listen` pattern the service holds (its TLS form when the
    /// host serves TLS), checked again by `http_stop`.
    listen: String,
    /// The host's TLS 1.3 configuration (K7 H3); `None` serves cleartext.
    tls: Option<tokio_rustls::TlsAcceptor>,
    request_setup: crate::execution::RequestSetup,
    requested: Vec<CapabilityGrant>,
    ceiling: Option<Vec<CapabilityGrant>>,
    files: policy::FileMode,
    stop: HttpShutdown,
    /// The module linked once to the host operations (H2); every request
    /// instantiates its own Store from it.
    linked: wasmtime::InstancePre<crate::RuntimeContext>,
    /// Instance slots: one per running request (the one worker of v1).
    workers: Arc<tokio::sync::Semaphore>,
    /// The server memory ceiling in Wasm pages (v2): a request holds its
    /// memory budget's pages while it runs.
    memory: Arc<tokio::sync::Semaphore>,
    /// The pages of one request's memory budget.
    request_pages: u32,
    sink: Arc<Sink>,
}
impl Shared {
    /// The keep-alive pool of a contract v2 service.
    fn pool(&self) -> Option<varta_common::http::PoolLimits> {
        self.limits
            .pool
            .filter(|_| self.contract.version == varta_common::http::UTF16_VERSION)
    }
}
/// The scheme of a cleartext listener, and of a TLS one.
const HTTP_SCHEME: &str = "http";
const HTTPS_SCHEME: &str = "https";

pub(super) struct Start<'a> {
    /// The validated module, compiled again for the server's engine.
    pub bytes: &'a [u8],
    pub contract: HttpContract,
    pub request_setup: crate::execution::RequestSetup,
    pub requested: &'a [CapabilityGrant],
    pub ceiling: Option<&'a [CapabilityGrant]>,
    /// The `fs.*` mode of every request's fresh store.
    pub files: policy::FileMode,
    pub output: RunOutput,
    pub observer: Option<RuntimeObserver>,
    /// The host policy's HTTP settings.
    pub policy: Option<&'a HttpPolicy>,
}
pub(super) struct Finished {
    pub result: Result<RunOutput, RuntimeError>,
    pub requests: Option<HttpReport>,
}
pub(super) fn run(start: Start<'_>) -> Finished {
    let v2 = start.contract.version == varta_common::http::UTF16_VERSION;
    let requests = Arc::new(Mutex::new(HttpReport::default()));
    let result = run_owned(start, requests.clone());
    let requests =
        v2.then(|| std::mem::take(&mut *requests.lock().unwrap_or_else(|e| e.into_inner())));
    Finished { result, requests }
}
fn run_owned(
    start: Start<'_>,
    requests: Arc<Mutex<HttpReport>>,
) -> Result<RunOutput, RuntimeError> {
    let Start {
        bytes,
        contract,
        mut request_setup,
        requested,
        ceiling,
        files,
        output,
        observer,
        policy,
    } = start;
    // A host may only lower the registry's limits; an invalid policy fails
    // before any socket exists.
    let limits = match policy.and_then(|policy| policy.limits.as_ref()) {
        Some(host) => contract.limits.lowered(host)?,
        None => contract.limits.clone(),
    };
    // This is the only bind path: authorization precedes every socket effect.
    let mut bind_policy = policy::Policy::with_files(requested, ceiling, &files);
    let _broker = crate::permission::Broker::new(
        request_setup.broker.setup(),
        &mut bind_policy,
        std::time::Instant::now(),
    )?;
    // A host that configures TLS serves it; the grant must name the TLS
    // endpoint in the package and in the ceiling (and a cleartext endpoint
    // otherwise). Contract v1 is frozen without TLS.
    let tls_files = policy.and_then(|policy| policy.tls.as_ref());
    if tls_files.is_some() && contract.version != varta_common::http::UTF16_VERSION {
        return Err(HttpError::Policy("TLS requires an HTTP contract v2 service".into()).into());
    }
    let listen = varta_common::http::listen_endpoint(&contract.bind, tls_files.is_some());
    if !bind_policy.listen(&listen) {
        return Err(HttpError::Denied(listen).into());
    }
    // The host reads its certificate and key before any socket exists; they
    // never reach the guest.
    let tls = tls_files
        .map(tls::config)
        .transpose()?
        .map(tokio_rustls::TlsAcceptor::from);
    // A raised application deadline cannot raise the fixed HTTP contract limit.
    let maximum_deadline = std::time::Duration::from_millis(contract.limits.handler_deadline_ms);
    if request_setup.budgets.deadline_per_turn > maximum_deadline {
        request_setup
            .capped
            .push(varta_common::budget::CappedBudget {
                kind: varta_common::budget::BudgetKind::Deadline,
                requested: request_setup
                    .budgets
                    .value(varta_common::budget::BudgetKind::Deadline),
                allowed: contract.limits.handler_deadline_ms,
            });
        request_setup.budgets.deadline_per_turn = maximum_deadline;
    }
    // Each request of the pool keeps the per-request memory bound.
    let request_memory = varta_common::http::REQUEST_MEMORY_BYTES;
    if request_setup.budgets.memory_bytes > request_memory {
        request_setup
            .capped
            .push(varta_common::budget::CappedBudget {
                kind: varta_common::budget::BudgetKind::Memory,
                requested: request_setup
                    .budgets
                    .value(varta_common::budget::BudgetKind::Memory),
                allowed: request_memory as u64,
            });
        request_setup.budgets.memory_bytes = request_memory;
    }
    // A v1 service has one worker; a v2 service its slots and its memory
    // ceiling, which must hold at least one request.
    let workers = limits.pool.map_or(limits.connections, |pool| pool.workers);
    let page = abi::WASM_PAGE_BYTES;
    let request_pages = request_setup.budgets.memory_bytes.div_ceil(page);
    let memory_pages = limits
        .pool
        .map_or(request_pages, |pool| pool.memory_bytes / page);
    if request_pages > memory_pages {
        return Err(HttpError::Policy(
            "the server memory ceiling is below one request's memory budget".into(),
        )
        .into());
    }
    // The server engine: the module compiled again with the runtime's
    // settings, a pool of `workers` instance slots, linked once.
    let engine = engine::engine(workers)?;
    let module = crate::validate::module(&engine, bytes)?.0;
    let mut linker = wasmtime::Linker::new(&engine);
    crate::host::register(&mut linker)?;
    let linked = linker.instantiate_pre(module.module())?;
    let _ticker = engine::Ticker::start(engine.clone()).map_err(HttpError::Io)?;
    let listener = TcpListener::bind(&contract.bind).map_err(HttpError::Io)?;
    listener.set_nonblocking(true).map_err(HttpError::Io)?;
    let address = listener.local_addr().map_err(HttpError::Io)?;
    let (sender, _) = tokio::sync::watch::channel(false);
    let stop = HttpShutdown(sender);
    let sink = Arc::new(Sink {
        observer,
        requests,
        captured: Mutex::new(Captured {
            output: RunOutput::default(),
            bytes: 0,
        }),
    });
    sink.lines(output.lines)?;
    let shared = Arc::new(Shared {
        engine,
        module,
        contract,
        limits,
        address,
        scheme: if tls.is_some() {
            HTTPS_SCHEME
        } else {
            HTTP_SCHEME
        },
        listen,
        tls,
        request_setup,
        requested: requested.to_vec(),
        ceiling: ceiling.map(ToOwned::to_owned),
        files,
        stop,
        linked,
        workers: Arc::new(tokio::sync::Semaphore::new(workers)),
        memory: Arc::new(tokio::sync::Semaphore::new(memory_pages)),
        request_pages: u32::try_from(request_pages)
            .map_err(|_| HttpError::Message("request memory pages".into()))?,
        sink: sink.clone(),
    });
    // A dedicated reactor also supports embedders already running Tokio.
    // Requests are tasks of its worker threads, one per available CPU; a
    // blocking host operation takes a thread of its own for its duration.
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    std::thread::Builder::new()
        .name("varta-http".into())
        .spawn(move || -> Result<(), RuntimeError> {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(threads)
                .max_blocking_threads(workers)
                .thread_name("varta-http-worker")
                .enable_all()
                .build()
                .map_err(HttpError::Io)?;
            runtime.block_on(connection::serve(listener, address, shared))
        })
        .map_err(HttpError::Io)?
        .join()
        .map_err(|_| HttpError::Message("HTTP reactor thread failed".into()))??;
    let mut captured = sink
        .captured
        .lock()
        .map_err(|_| HttpError::Message("output lock".into()))?;
    if captured.bytes > abi::MAX_OUTPUT_BYTES {
        return Err(HttpError::Message("aggregate captured output limit".into()).into());
    }
    Ok(std::mem::take(&mut captured.output))
}
