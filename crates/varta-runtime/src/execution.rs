//! One bounded store shared by normal startup and named tool invocation.
use crate::{RunOutput, RuntimeContext, RuntimeError, host, policy};
use std::time::Instant;
use varta_common::CapabilityGrant;
use varta_common::budget::{Budgets, CappedBudget};
use wasmtime::{Engine, Instance, Module, Store, StoreLimitsBuilder};

/// How an instance is driven.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// One bounded execution: the whole run, every turn included, ends at
    /// one wall-clock deadline (the deadline budget after it started).
    Bounded,
    /// A long-lived application instance (v38): the deadline renews per
    /// turn, and the instance has no lifetime bound unless the embedder sets
    /// one.
    Application {
        lifetime: Option<std::time::Duration>,
    },
}

/// Everything an instance is created from, besides the module.
pub(crate) struct Setup<'a> {
    pub requested: &'a [CapabilityGrant],
    pub ceiling: Option<&'a [CapabilityGrant]>,
    pub budgets: Budgets,
    pub capped: Vec<CappedBudget>,
    /// Renew fuel, host calls and microtasks per turn (v38).
    pub renew: bool,
    pub mode: Mode,
    pub broker: crate::permission::BrokerSetup,
}
impl<'a> Setup<'a> {
    /// The production setup of every earlier entry point.
    pub fn production(
        requested: &'a [CapabilityGrant],
        ceiling: Option<&'a [CapabilityGrant]>,
    ) -> Self {
        Self {
            requested,
            ceiling,
            budgets: Budgets::PRODUCTION,
            capped: Vec::new(),
            renew: false,
            mode: Mode::Bounded,
            broker: crate::permission::BrokerSetup::default(),
        }
    }
}

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
        Self::with_setup(engine, Setup::production(requested, ceiling))
    }

    pub fn with_setup(engine: &Engine, setup: Setup<'_>) -> Result<Self, RuntimeError> {
        varta_common::validate_grants(setup.requested)?;
        if let Some(ceiling) = setup.ceiling {
            varta_common::validate_grants(ceiling)?;
        }
        let mut policy = match setup.ceiling {
            Some(ceiling) => policy::Policy::with_ceiling(setup.requested, Some(ceiling)),
            None => policy::Policy::new(setup.requested),
        };
        let started = Instant::now();
        // Required grants are decided before any guest code runs.
        let broker = crate::permission::Broker::new(setup.broker, &mut policy, started)?;
        let budgets = setup.budgets;
        let (deadline, lifetime, renew_deadline) = match setup.mode {
            Mode::Bounded => {
                let end = started + budgets.deadline_per_turn;
                (end, Some(end), false)
            }
            Mode::Application { lifetime } => {
                let lifetime = lifetime.map(|lifetime| started + lifetime);
                let turn = started + budgets.deadline_per_turn;
                (lifetime.map_or(turn, |end| turn.min(end)), lifetime, true)
            }
        };
        let control = Deadline::new(engine.clone(), deadline)?;
        let mut store = Store::new(
            engine,
            RuntimeContext {
                policy,
                limits: StoreLimitsBuilder::new()
                    .memory_size(budgets.memory_bytes)
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
                deadline,
                started,
                lifetime,
                budgets,
                turns: crate::turns::Turns::new(setup.renew, renew_deadline, setup.capped),
                broker,
                events: crate::events::HostEvents::default(),
                rearm: Some(control.control.clone()),
            },
        );
        store.limiter(|context| &mut context.limits);
        store.set_fuel(budgets.fuel_per_turn)?;
        store.set_epoch_deadline(1);
        store.epoch_deadline_callback(|context| {
            Ok(if Instant::now() >= context.data().deadline {
                wasmtime::UpdateDeadline::Interrupt
            } else {
                wasmtime::UpdateDeadline::Continue(1)
            })
        });
        Ok(Self {
            store,
            _deadline: control,
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
    pub fn allow_test_wall_clock(&mut self, allowance: std::time::Duration) {
        self.store.data_mut().deadline = Instant::now() + allowance;
    }
    pub fn check_deadline(&self) -> Result<(), wasmtime::Error> {
        if Instant::now() >= self.store.data().deadline {
            return Err(wasmtime::Error::msg(crate::turns::DEADLINE_EXCEEDED));
        }
        Ok(())
    }
}

/// The epoch thread of one store: it increments the engine epoch when the
/// current deadline passes, then waits to be rearmed for the next turn.
struct Deadline {
    control: std::sync::mpsc::Sender<Option<Instant>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Deadline {
    fn new(engine: Engine, until: Instant) -> Result<Self, wasmtime::Error> {
        let (control, wait) = std::sync::mpsc::channel::<Option<Instant>>();
        let thread = std::thread::Builder::new()
            .name("varta-deadline".into())
            .spawn(move || {
                use std::sync::mpsc::RecvTimeoutError;
                let mut until = Some(until);
                loop {
                    let message = match until {
                        Some(at) => wait.recv_timeout(at.saturating_duration_since(Instant::now())),
                        None => wait.recv().map_err(|_| RecvTimeoutError::Disconnected),
                    };
                    match message {
                        Ok(Some(next)) => until = Some(next),
                        Err(RecvTimeoutError::Timeout) => {
                            engine.increment_epoch();
                            until = None;
                        }
                        Ok(None) | Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
            })
            .map_err(wasmtime::Error::new)?;
        Ok(Self {
            control,
            thread: Some(thread),
        })
    }
}
impl Drop for Deadline {
    fn drop(&mut self) {
        let _ = self.control.send(None);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
