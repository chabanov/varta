//! Host-driven HTTP lifetime. Every request gets a fresh bounded guest store.
mod handler;
mod transport;
use crate::{RunOutput, RuntimeError, policy};
use std::{
    net::{SocketAddr, TcpListener},
    sync::{Arc, Mutex},
};
use varta_common::{
    CapabilityGrant, abi,
    http::{HttpContract, HttpError},
};
use wasmtime::{Engine, Module};

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
}
struct Shared {
    engine: Engine,
    module: Module,
    contract: HttpContract,
    requested: Vec<CapabilityGrant>,
    ceiling: Option<Vec<CapabilityGrant>>,
    stop: HttpShutdown,
    workers: Arc<tokio::sync::Semaphore>,
    sink: Arc<Sink>,
}

pub(super) struct Start<'a> {
    pub engine: Engine,
    pub module: Module,
    pub contract: HttpContract,
    pub requested: &'a [CapabilityGrant],
    pub ceiling: Option<&'a [CapabilityGrant]>,
    pub output: RunOutput,
    pub observer: Option<RuntimeObserver>,
}
pub(super) fn run(start: Start<'_>) -> Result<RunOutput, RuntimeError> {
    let Start {
        engine,
        module,
        contract,
        requested,
        ceiling,
        output,
        observer,
    } = start;
    // This is the only bind path: authorization precedes every socket effect.
    if !policy::Policy::with_ceiling(requested, ceiling).listen(&contract.bind) {
        return Err(HttpError::Denied(contract.bind).into());
    }
    let listener = TcpListener::bind(&contract.bind).map_err(HttpError::Io)?;
    listener.set_nonblocking(true).map_err(HttpError::Io)?;
    let address = listener.local_addr().map_err(HttpError::Io)?;
    let (sender, _) = tokio::sync::watch::channel(false);
    let stop = HttpShutdown(sender);
    let sink = Arc::new(Sink {
        observer,
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
        requested: requested.to_vec(),
        ceiling: ceiling.map(ToOwned::to_owned),
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
