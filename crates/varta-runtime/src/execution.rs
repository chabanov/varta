//! One bounded store shared by normal startup and named tool invocation.
use crate::{RunOutput, RuntimeContext, RuntimeError, host, policy};
use std::time::{Duration, Instant};
use varta_common::{CapabilityGrant, abi};
use wasmtime::{Engine, Instance, Module, Store, StoreLimitsBuilder};

pub(crate) struct Execution {
    pub store: Store<RuntimeContext>,
    _deadline: Deadline,
}
impl Execution {
    pub fn new(
        engine: &Engine,
        requested: &[CapabilityGrant],
        ceiling: Option<&[CapabilityGrant]>,
    ) -> Result<Self, RuntimeError> {
        varta_common::validate_grants(requested)?;
        if let Some(ceiling) = ceiling {
            varta_common::validate_grants(ceiling)?;
        }
        let mut store = Store::new(
            engine,
            RuntimeContext {
                policy: match ceiling {
                    Some(ceiling) => policy::Policy::with_ceiling(requested, Some(ceiling)),
                    None => policy::Policy::new(requested),
                },
                limits: StoreLimitsBuilder::new()
                    .memory_size(abi::MAX_MEMORY_BYTES)
                    .memories(1)
                    .tables(1)
                    .table_elements(1024)
                    .instances(1)
                    .build(),
                output: RunOutput::default(),
                output_bytes: 0,
                host_calls: 0,
                http_control: None,
                random: crate::random::RandomSource::default(),
                deadline: Instant::now() + Duration::from_secs(abi::DEADLINE_SECONDS),
            },
        );
        store.limiter(|context| &mut context.limits);
        store.set_fuel(abi::FUEL)?;
        store.set_epoch_deadline(1);
        store.epoch_deadline_callback(|context| {
            Ok(if Instant::now() >= context.data().deadline {
                wasmtime::UpdateDeadline::Interrupt
            } else {
                wasmtime::UpdateDeadline::Continue(1)
            })
        });
        let deadline = Deadline::new(engine.clone(), store.data().deadline)?;
        Ok(Self {
            store,
            _deadline: deadline,
        })
    }
    pub fn instantiate(&mut self, module: &Module) -> Result<Instance, wasmtime::Error> {
        let mut linker = wasmtime::Linker::new(self.store.engine());
        host::register(&mut linker)?;
        let instance = linker.instantiate(&mut self.store, module)?;
        crate::exceptions::initial(&instance, &mut self.store)?;
        Ok(instance)
    }
    pub fn check_deadline(&self) -> Result<(), wasmtime::Error> {
        if Instant::now() >= self.store.data().deadline {
            return Err(wasmtime::Error::msg("execution deadline exceeded"));
        }
        Ok(())
    }
}

struct Deadline {
    stop: std::sync::mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Deadline {
    fn new(engine: Engine, until: Instant) -> Result<Self, wasmtime::Error> {
        let (stop, wait) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("varta-deadline".into())
            .spawn(move || {
                if matches!(
                    wait.recv_timeout(until.saturating_duration_since(Instant::now())),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                ) {
                    engine.increment_epoch();
                }
            })
            .map_err(wasmtime::Error::new)?;
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}
impl Drop for Deadline {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
