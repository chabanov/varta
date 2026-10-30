//! Long-lived application instances (v38 `turn_budgets`, increment P5).
//!
//! An application instance stays alive after `main` while a source is
//! pending: a timer that keeps the loop alive, a network request in flight,
//! an open host event source with a listening frame, or an open permission
//! prompt. The embedder drives it with [`Application::step`], which runs at
//! most one event turn (one task and its microtasks) per call, and feeds it
//! host events with [`Application::deliver`]. Turns never interleave and a
//! turn is never interrupted for fairness. Fuel, host calls, microtasks and
//! the deadline renew per turn; memory and output are per instance.
//!
//! A trap, an exhausted budget, an unhandled rejection after a turn or a
//! fatal host error terminates the instance: no later turn runs, nothing is
//! restarted automatically, and only the embedder can start a new instance
//! (from `main` again). Tool and HTTP entry points stay stateless,
//! fresh-store invocations and are not served by an application instance.
use crate::events::{Delivery, HostEvent, TaskKind};
use crate::execution::{Execution, Mode};
use crate::turns::{self, BudgetReport, TurnKind, TurnUsage};
use crate::{
    GuestExit, PermissionPrompter, PreparedModule, RunOptions, RunReport, RuntimeError,
    VartaRuntime, exceptions, resources, scheduler,
};
use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};
use varta_common::permission::{AuditEntry, StoredDecision};
use wasmtime::Instance;

/// How an application instance is created.
#[derive(Default)]
pub struct ApplicationOptions {
    pub run: RunOptions,
    /// The host prompt seam; `None` for headless and server hosts, which
    /// never prompt.
    pub prompter: Option<Box<dyn PermissionPrompter>>,
    /// An optional bound on the instance's wall-clock lifetime.
    pub lifetime: Option<Duration>,
    /// An optional bound on the number of turns.
    pub max_turns: Option<u64>,
}

/// Typed application failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApplicationError {
    #[error("application instances require profile checked-f64-utf16-v38 or later")]
    Profile,
    #[error("HTTP services stay stateless per request and do not run as application instances")]
    HttpService,
    #[error("the application instance was terminated; start a new instance explicitly")]
    Terminated,
    #[error("the application instance reached its lifetime bound")]
    Lifetime,
    #[error("the application instance reached its turn bound ({0} turns)")]
    TurnLimit(u64),
    #[error("host event rejected: {0}")]
    Event(#[from] varta_common::events::EventError),
}

/// The state of an instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstanceState {
    Running,
    /// Nothing keeps the instance alive any more, or the guest exited.
    Finished {
        exit_code: i32,
    },
    /// A trap, budget or fatal error ended the instance.
    Failed,
}

/// What one [`Application::step`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// One turn ran.
    Turn(TurnUsage),
    /// No task became ready in time; something still keeps the instance
    /// alive. With only host event sources open the step returns at once:
    /// nothing can arrive until the embedder delivers.
    Idle,
    Finished {
        exit_code: i32,
    },
    /// The instance failed; [`Application::failure`] names why.
    Failed,
}

/// A ready task of the next turn, by arrival.
enum Ready {
    Host,
    Timer,
    Network,
}

pub struct Application {
    execution: Execution,
    instance: Instance,
    _module: PreparedModule,
    state: InstanceState,
    failure: Option<RuntimeError>,
    network: VecDeque<crate::net::Event>,
    max_turns: Option<u64>,
}

impl VartaRuntime {
    /// Validate, set up and start an application instance: instantiate it,
    /// call `_start` and drain its microtasks (turn 1). A guest failure in
    /// turn 1 leaves the instance [`InstanceState::Failed`]; setup failures
    /// (validation, a refused required permission) are errors.
    pub fn start_application(
        &self,
        bytes: &[u8],
        options: ApplicationOptions,
    ) -> Result<Application, RuntimeError> {
        let profile = crate::language::inspect(bytes)?.profile;
        if !profile.is_some_and(varta_common::language::LanguageProfile::turn_budgets) {
            return Err(ApplicationError::Profile.into());
        }
        if crate::http_contract::read(bytes)?.is_some() {
            return Err(ApplicationError::HttpService.into());
        }
        let mode = Mode::Application {
            lifetime: options.lifetime,
        };
        let prepared = self.prepare_instance(bytes, &options.run, mode, options.prompter)?;
        let crate::Prepared {
            module,
            mut execution,
            ..
        } = prepared;
        let instance = execution.instantiate_prepared(&module)?;
        let mut application = Application {
            execution,
            instance,
            _module: module,
            state: InstanceState::Running,
            failure: None,
            network: VecDeque::new(),
            max_turns: options.max_turns,
        };
        let result = (|| {
            let store = &mut application.execution.store;
            if let Some(start) = instance.get_func(&mut *store, "_start") {
                start.call(&mut *store, &[], &mut [])?;
            }
            exceptions::finish(&instance, store)?;
            application.settle()
        })();
        application.finish_turn(result);
        Ok(application)
    }

    /// Run an application instance until nothing but host event sources
    /// keeps it alive (a host with no event producer, such as the CLI), and
    /// report it like a bounded run.
    #[must_use]
    pub fn run_application(&self, bytes: &[u8], options: ApplicationOptions) -> RunReport {
        let mut report = RunReport::new(crate::artifact_identity(bytes));
        let observer = options.run.observer.clone();
        let mut application = match self.start_application(bytes, options) {
            Ok(application) => application,
            Err(error) => {
                report.failure = Some(error);
                return report;
            }
        };
        // With an observer, output streams after every turn, so a long-lived
        // instance's lines appear while it runs.
        let stream = |application: &mut Application| {
            if let Some(observer) = &observer {
                for line in application.take_output() {
                    observer(crate::RuntimeEvent::Output(line));
                }
            }
        };
        stream(&mut application);
        while let Step::Turn(_) = application.step(None) {
            stream(&mut application);
        }
        stream(&mut application);
        application.report_into(&mut report);
        report
    }
}

impl Application {
    #[must_use]
    pub fn state(&self) -> InstanceState {
        self.state
    }
    /// Why the instance failed.
    #[must_use]
    pub fn failure(&self) -> Option<&RuntimeError> {
        self.failure.as_ref()
    }
    pub fn take_failure(&mut self) -> Option<RuntimeError> {
        self.failure.take()
    }
    /// The output lines captured since the last call.
    pub fn take_output(&mut self) -> Vec<String> {
        std::mem::take(&mut self.execution.store.data_mut().output.lines)
    }
    #[must_use]
    pub fn budget_report(&self) -> BudgetReport {
        let context = self.execution.store.data();
        context.turns.report(context.budgets, context.output_bytes)
    }
    /// The bounded host audit log and the number of entries dropped from it.
    #[must_use]
    pub fn audit(&self) -> (Vec<AuditEntry>, u64) {
        self.execution.store.data().broker.audit_log()
    }
    /// Stored decisions, including the prompt answers of this instance, for
    /// the embedder to persist under [`Self::artifact`].
    #[must_use]
    pub fn decisions(&self) -> BTreeMap<String, StoredDecision> {
        self.execution.store.data().broker.decisions()
    }
    #[must_use]
    pub fn artifact(&self) -> String {
        self.execution.store.data().broker.artifact().to_owned()
    }
    /// Revoke a granted permission: new operations are denied, operations
    /// already authorized keep running. Returns whether the grant exists.
    pub fn revoke(&mut self, capability: &str) -> bool {
        let context = self.execution.store.data_mut();
        context.broker.revoke(capability, &mut context.policy)
    }

    /// Queue a host event for its source. It runs in a later turn; nothing
    /// runs now. A full queue drops a coalescible record and fails the
    /// instance for any other.
    pub fn deliver(&mut self, event: HostEvent) -> Result<Delivery, ApplicationError> {
        if self.state != InstanceState::Running {
            return Err(ApplicationError::Terminated);
        }
        match self.execution.store.data_mut().events.push(event) {
            Ok(delivery) => Ok(delivery),
            Err(varta_common::events::EventError::QueueFull) => {
                self.fail(varta_common::events::EventError::QueueFull.into());
                Err(varta_common::events::EventError::QueueFull.into())
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Run until the instance finishes, fails, or only host event sources
    /// keep it alive.
    pub fn run(&mut self) -> Step {
        loop {
            match self.step(None) {
                Step::Turn(_) => {}
                other => return other,
            }
        }
    }

    /// Run at most one event turn: answer open prompts (between turns), then
    /// take the earliest ready task by arrival (host tasks, due timers and
    /// network completions), waiting up to `timeout` (`None`: until a timer
    /// or a network completion is due).
    pub fn step(&mut self, timeout: Option<Duration>) -> Step {
        match self.state {
            InstanceState::Finished { exit_code } => return Step::Finished { exit_code },
            InstanceState::Failed => return Step::Failed,
            InstanceState::Running => {}
        }
        if let Err(error) = self.answer_prompts() {
            self.fail(error);
            return Step::Failed;
        }
        let until = timeout.map(|timeout| Instant::now() + timeout);
        loop {
            if let Some(error) = self.limits() {
                self.fail(error);
                return Step::Failed;
            }
            if let Some(ready) = self.ready() {
                return self.turn(ready);
            }
            if !self.alive() {
                self.state = InstanceState::Finished { exit_code: 0 };
                return Step::Finished { exit_code: 0 };
            }
            match self.wait(until) {
                Ok(true) => {}
                Ok(false) => return Step::Idle,
                Err(error) => {
                    self.fail(error);
                    return Step::Failed;
                }
            }
        }
    }

    fn limits(&self) -> Option<RuntimeError> {
        let context = self.execution.store.data();
        if context.lifetime.is_some_and(|end| Instant::now() >= end) {
            return Some(ApplicationError::Lifetime.into());
        }
        if let Some(max) = self.max_turns
            && context.turns.current() >= max
        {
            return Some(ApplicationError::TurnLimit(max).into());
        }
        None
    }

    fn answer_prompts(&mut self) -> Result<(), RuntimeError> {
        let context = self.execution.store.data_mut();
        if !context.broker.prompts_pending() {
            return Ok(());
        }
        // The prompt runs between turns: no guest code, fuel or turn deadline.
        for (source, state) in context.broker.resolve(&mut context.policy) {
            context.events.complete(source, state)?;
        }
        Ok(())
    }

    /// The earliest ready task by arrival, if any.
    fn ready(&mut self) -> Option<Ready> {
        let context = self.execution.store.data_mut();
        while let Some(event) = context.net.poll() {
            self.network.push_back(event);
        }
        let now = Instant::now();
        let host = context.events.next_arrival();
        let timer = context
            .scheduler
            .next_due(true)
            .map(|due| scheduler::target(context, due))
            .filter(|at| *at <= now);
        let network = self.network.front().map(|_| now);
        [
            (host, Ready::Host),
            (timer, Ready::Timer),
            (network, Ready::Network),
        ]
        .into_iter()
        .filter_map(|(at, ready)| at.map(|at| (at, ready)))
        .min_by_key(|(at, _)| *at)
        .map(|(_, ready)| ready)
    }

    /// Whether anything can still produce a task.
    fn alive(&self) -> bool {
        let context = self.execution.store.data();
        context.scheduler.next_due(false).is_some()
            || context.net.alive()
            || !self.network.is_empty()
            || context.events.listening()
            || context.events.queued() > 0
            || context.broker.prompts_pending()
    }

    /// Wait for the next timer or network completion, never past `until`.
    /// Returns false when nothing can arrive before `until` (or at all
    /// without the embedder).
    fn wait(&mut self, until: Option<Instant>) -> Result<bool, RuntimeError> {
        let context = self.execution.store.data_mut();
        let timer = context
            .scheduler
            .next_due(true)
            .map(|due| scheduler::target(context, due));
        let busy = context.net.alive();
        if timer.is_none() && !busy {
            return Ok(false);
        }
        // Only network completions can arrive without a bound: wait for them
        // in slices, so lifetime and turn bounds are checked between slices.
        let slice = Instant::now() + context.budgets.deadline_per_turn;
        let deadline = [timer, until, context.lifetime, Some(slice)]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(slice);
        if busy {
            if let Some(event) = context.net.wait(deadline) {
                self.network.push_back(event);
                return Ok(true);
            }
        } else if deadline > Instant::now() {
            std::thread::sleep(deadline - Instant::now());
        }
        Ok(until.is_none_or(|until| Instant::now() < until))
    }

    fn turn(&mut self, ready: Ready) -> Step {
        let kind = match &ready {
            Ready::Host => TurnKind::HostEvent,
            Ready::Timer => TurnKind::Timer,
            Ready::Network => TurnKind::Network,
        };
        let result = (|| {
            let store = &mut self.execution.store;
            match ready {
                Ready::Host => {
                    let Some(task) = store.data_mut().events.pop() else {
                        return Ok(());
                    };
                    let kind = match task.kind {
                        TaskKind::HostEvent => TurnKind::HostEvent,
                        TaskKind::Permission => TurnKind::Permission,
                    };
                    turns::begin(store, kind)?;
                    let context = store.data_mut();
                    context.broker.gesture = task.gesture;
                    context.events.deliver(&mut context.scheduler, task)?;
                }
                Ready::Timer => {
                    turns::begin(store, kind)?;
                    store
                        .data_mut()
                        .scheduler
                        .fire()
                        .map_err(wasmtime::Error::new)?;
                }
                Ready::Network => {
                    turns::begin(store, kind)?;
                    if let Some(event) = self.network.pop_front() {
                        let context = store.data_mut();
                        context.net.deliver(&mut context.scheduler, event)?;
                        scheduler::observed_wall_clock(context);
                    }
                }
            }
            self.settle()
        })();
        self.finish_turn(result);
        match self.state {
            InstanceState::Running => self
                .execution
                .store
                .data()
                .turns
                .last()
                .map_or(Step::Idle, Step::Turn),
            InstanceState::Finished { exit_code } => Step::Finished { exit_code },
            InstanceState::Failed => Step::Failed,
        }
    }

    /// Drain the turn's microtasks and apply the unhandled-rejection policy.
    fn settle(&mut self) -> Result<(), RuntimeError> {
        let store = &mut self.execution.store;
        scheduler::drain(&self.instance, store)?;
        if let Some(error) = store.data().scheduler.unhandled_rejection() {
            return Err(error.into());
        }
        Ok(())
    }

    fn finish_turn(&mut self, result: Result<(), RuntimeError>) {
        let store = &mut self.execution.store;
        store.data_mut().broker.gesture = false;
        let instance = &self.instance;
        let result = result
            .map_err(scheduler::typed)
            .map_err(|error| resources::classify(instance, store, error))
            .map_err(|error| turns::classify(error, store.data()));
        turns::close(Some(instance), store);
        if let Err(error) = result {
            if let RuntimeError::Execution(source) = &error
                && let Some(exit) = source.downcast_ref::<GuestExit>()
            {
                let exit_code = exit.0;
                store.data_mut().output.exit_code = exit_code;
                self.state = InstanceState::Finished { exit_code };
            } else {
                self.fail(error);
            }
        }
    }

    fn fail(&mut self, error: RuntimeError) {
        self.state = InstanceState::Failed;
        self.failure = Some(error);
    }

    fn report_into(mut self, report: &mut RunReport) {
        report.failure = self.failure.take();
        report.collect(self.execution.store.data_mut());
        if let InstanceState::Finished { exit_code } = self.state {
            report.output.exit_code = exit_code;
        }
    }
}
