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
                host_call_limit: abi::MAX_HOST_CALLS,
                http_control: None,
                function_catalog: None,
                module: varta_common::value::ModuleBounds::LEGACY,
                random: crate::random::RandomSource::default(),
                scheduler: crate::scheduler::Scheduler::default(),
                net: crate::net::Net::default(),
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
        self.instantiate_owned(
            module,
            None,
            (varta_common::value::ModuleBounds::LEGACY, false),
        )
    }
    pub fn instantiate_prepared(
        &mut self,
        module: &crate::PreparedModule,
    ) -> Result<Instance, wasmtime::Error> {
        self.instantiate_owned(
            &module.module,
            module.function_catalog.clone(),
            (module.bounds(), module.profile.program_limits()),
        )
    }
    fn instantiate_owned(
        &mut self,
        module: &Module,
        catalog: Option<std::sync::Arc<varta_common::function::VerifiedFunctionCatalog>>,
        (bounds, uncharged_initialization): (varta_common::value::ModuleBounds, bool),
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
        self.store.data_mut().module = bounds;
        self.store
            .data_mut()
            .scheduler
            .set_bounds(bounds.limits.data);
        let result = (|| {
            let mut linker = wasmtime::Linker::new(self.store.engine());
            host::register(&mut linker)?;
            // Wasmtime meters the data-segment copy of instantiation per byte
            // of the initialized image. Under v38 that copy, bounded by the
            // initial memory and so by the memory budget, gets its own
            // allowance and the guest keeps its whole fuel budget.
            let fuel = self.store.get_fuel()?;
            if uncharged_initialization {
                self.store
                    .set_fuel(fuel.saturating_add(abi::MAX_MEMORY_BYTES as u64))?;
            }
            let instance = linker.instantiate(&mut self.store, module)?;
            if uncharged_initialization {
                self.store.set_fuel(fuel)?;
            }
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
    /// Test-only host-call budget (CLAUDE.md §11) for measurements whose
    /// workload exceeds the production 1024 host calls. Production executions
    /// never call this.
    #[cfg(test)]
    pub fn allow_test_host_calls(&mut self, calls: u32) {
        self.store.data_mut().host_call_limit = calls;
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
