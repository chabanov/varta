//! Host-driven HTTP lifetime. Every request gets a fresh bounded guest store.
mod handler;
mod report;
mod transport;
use crate::{RunOutput, RuntimeError, policy};
pub use report::{HttpReport, HttpRequestOutcome, HttpRequestReport};
use std::{
    net::{SocketAddr, TcpListener},
    sync::{Arc, Mutex},
};
use varta_common::{
    CapabilityGrant, abi,
    http::{HttpContract, HttpError},
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
    request_setup: crate::execution::RequestSetup,
    requested: Vec<CapabilityGrant>,
    ceiling: Option<Vec<CapabilityGrant>>,
    files: policy::FileMode,
    stop: HttpShutdown,
    workers: Arc<tokio::sync::Semaphore>,
    sink: Arc<Sink>,
}

pub(super) struct Start<'a> {
    pub engine: Engine,
    pub module: crate::PreparedModule,
    pub contract: HttpContract,
    pub request_setup: crate::execution::RequestSetup,
    pub requested: &'a [CapabilityGrant],
    pub ceiling: Option<&'a [CapabilityGrant]>,
    /// The `fs.*` mode of every request's fresh store.
    pub files: policy::FileMode,
    pub output: RunOutput,
    pub observer: Option<RuntimeObserver>,
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
        engine,
        module,
        contract,
        mut request_setup,
        requested,
        ceiling,
        files,
        output,
        observer,
    } = start;
    // This is the only bind path: authorization precedes every socket effect.
    let mut bind_policy = policy::Policy::with_files(requested, ceiling, &files);
    let _broker = crate::permission::Broker::new(
        request_setup.broker.setup(),
        &mut bind_policy,
        std::time::Instant::now(),
    )?;
    if !bind_policy.listen(&contract.bind) {
        return Err(HttpError::Denied(contract.bind).into());
    }
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
        request_setup,
        requested: requested.to_vec(),
        ceiling: ceiling.map(ToOwned::to_owned),
        files,
        stop,
        workers: Arc::new(tokio::sync::Semaphore::new(1)),
        sink: sink.clone(),
    });
    // A dedicated reactor also supports embedders already running Tokio.
    std::thread::Builder::new()
        .name("varta-http".into())
        .spawn(move || -> Result<(), RuntimeError> {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .max_blocking_threads(1)
                .build()
                .map_err(HttpError::Io)?;
            runtime.block_on(transport::serve(listener, address, shared))
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
