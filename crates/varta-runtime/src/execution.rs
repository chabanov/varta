//! One bounded store shared by normal startup and named tool invocation.
use crate::{RunOutput, RuntimeContext, RuntimeError, host, policy};
use std::time::{Duration, Instant};
use varta_common::{CapabilityGrant, abi};
use wasmtime::{Engine, Instance, Module, Store, StoreLimitsBuilder};

pub(crate) struct Execution {
    pub store: Store<RuntimeContext>,
    _deadline: Deadline,
    instantiation_started: bool,
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
                function_catalog: None,
                random: crate::random::RandomSource::default(),
                scheduler: crate::scheduler::Scheduler::default(),
                net: crate::net::Net::default(),
                profiler: None,
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
            instantiation_started: false,
        })
    }
    #[cfg(test)]
    pub fn instantiate(&mut self, module: &Module) -> Result<Instance, wasmtime::Error> {
        self.instantiate_owned(module, None)
    }
    pub fn instantiate_prepared(
        &mut self,
        module: &crate::PreparedModule,
    ) -> Result<Instance, wasmtime::Error> {
        self.instantiate_owned(&module.module, module.function_catalog.clone())
    }
    fn instantiate_owned(
        &mut self,
        module: &Module,
        catalog: Option<std::sync::Arc<varta_common::function::VerifiedFunctionCatalog>>,
    ) -> Result<Instance, wasmtime::Error> {
        // One instance per execution, including failed attempts. Never replace
        // metadata while an existing instance can still call host operations.
        if self.instantiation_started {
            return Err(wasmtime::Error::msg(
                "execution already attempted instantiation",
            ));
        }
        self.instantiation_started = true;
        self.store.data_mut().function_catalog = catalog;
        let result = (|| {
            let mut linker = wasmtime::Linker::new(self.store.engine());
            host::register(&mut linker)?;
            if self.store.data().profiler.is_some() {
                crate::fuel_profile::register(&mut linker)?;
            }
            let instance = linker.instantiate(&mut self.store, module)?;
            crate::exceptions::initial(&instance, &mut self.store)?;
            Ok(instance)
        })();
        if result.is_err() {
            self.store.data_mut().function_catalog.take();
        }
        result
    }
    /// Test-only wall-clock allowance (CLAUDE.md §11) for a run that also
    /// declares a test-only fuel ceiling: under parallel test load such a run
    /// can outlast the production deadline. The deadline thread still wakes
    /// at the production deadline; the epoch callback then lets the run
    /// continue, bounded by its fuel. Production executions never call this.
    #[cfg(test)]
    pub fn allow_test_wall_clock(&mut self, allowance: Duration) {
        self.store.data_mut().deadline = Instant::now() + allowance;
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
