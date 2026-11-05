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
//!
//! An instance hosts its view (v38 `view_protocol`): the embedder performs
//! trusted platform inputs between turns ([`Application::view_input`]), and
//! every record the renderer produces is a host task that runs as one turn
//! ([`TurnKind::View`]); [`VartaRuntime::run_view`] drives a scripted
//! headless session this way, on virtual time: timers fire when the script's
//! steps reach their due time, never by the wall clock.
use crate::events::{Delivery, HostEvent, Payload, TaskKind};
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
    /// A simulated headless platform for the view (`varta dev
    /// --view-script`); `None` hosts the view without one.
    pub view: Option<ViewPlatform>,
    /// Optional native geometry used by scroll commands from the first turn.
    pub view_layout: Option<ViewLayout>,
    /// A native host's clipboard and system browser: granted `copy_text`
    /// and external link effects are performed there instead of being
    /// simulated. Ignored with a simulated [`Self::view`].
    pub platform_effects: Option<std::sync::Arc<dyn varta_view::PlatformEffects>>,
}

/// Explicit host geometry and optional font for layout-dependent view commands.
pub struct ViewLayout {
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) font: Option<varta_view::TextMeasurer>,
    pub(crate) font_size: f32,
    pub(crate) dark: Option<bool>,
    pub(crate) root_font_size: Option<f64>,
}

impl ViewLayout {
    pub fn new(
        width: f32,
        height: f32,
        font: Option<varta_view::TextMeasurer>,
        font_size: f32,
    ) -> Result<Self, varta_view::LayoutError> {
        Self::validate(width, height, font_size)?;
        Ok(Self {
            width,
            height,
            font,
            font_size,
            dark: None,
            root_font_size: None,
        })
    }

    /// Set the initial system color scheme before the first guest turn.
    #[must_use]
    pub fn with_dark_mode(mut self, dark: bool) -> Self {
        self.dark = Some(dark);
        self
    }

    /// Set the host document-root reference for native `rem` lengths before
    /// the first guest turn. This is independent of the view's own font size.
    pub fn with_root_font_size(mut self, size: f64) -> Result<Self, varta_view::LayoutError> {
        varta_view::Environment::default().with_root_font_size(size)?;
        self.root_font_size = Some(size);
        Ok(self)
    }

    pub(crate) fn validate(
        width: f32,
        height: f32,
        font_size: f32,
    ) -> Result<(), varta_view::LayoutError> {
        if [width, height, font_size]
            .into_iter()
            .any(|value| !value.is_finite() || !(0.01..=1_000_000.0).contains(&value))
        {
            return Err(varta_view::LayoutError::Viewport);
        }
        Ok(())
    }
}

/// The simulated headless platform of an instance's view (v38
/// `view_protocol`): its viewport and how its prompts and platform effects
/// answer.
#[derive(Debug, Clone, Default)]
pub struct ViewPlatform {
    pub environment: varta_view::Environment,
    pub platform: varta_view::Platform,
}

/// A scripted headless view session ([`VartaRuntime::run_view`]).
pub struct ViewReport {
    /// Everything the instance produced, as for [`VartaRuntime::run_with`].
    pub report: RunReport,
    /// The tree and accessibility snapshot after the last turn; `None` when
    /// the instance could not start.
    pub snapshot: Option<serde_json::Value>,
    /// Batches applied.
    pub commits: usize,
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
    #[error(
        "view script step {step} did not settle within {} turns: a task keeps re-arming with the minimum timer delay",
        varta_view::Script::MAX_STEP_TURNS
    )]
    /// `step` is 1-based; the settling after the last step is the step
    /// after it.
    Unsettled { step: usize },
    #[error("view script step {step} did not settle its network work within {timeout:?}")]
    NetworkWait { step: usize, timeout: Duration },
    #[error("host event rejected: {0}")]
    Event(#[from] varta_common::events::EventError),
}

/// Failure of a host-requested, geometry-dependent view scroll.
#[derive(Debug, thiserror::Error)]
pub enum ApplicationViewScrollError {
    #[error(transparent)]
    Application(#[from] ApplicationError),
    #[error(transparent)]
    Scroll(#[from] varta_view::ScrollIntoViewError),
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

/// A network completion as the instance received it: its place in the
/// arrival order shared with host tasks and the loop time then (the order a
/// scripted session compares on virtual time).
struct Arrival {
    event: crate::net::Event,
    order: u64,
    loop_time: f64,
}

/// A ready task of the next turn, by arrival.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ready {
    Host,
    Timer,
    Network,
}

/// The next task on the wall clock: the earliest instant among the oldest
/// host task's arrival, the due time of a timer that has passed and the
/// oldest completion's arrival; ties go to the host task, then the timer.
fn wall_next(
    host: Option<Instant>,
    timer: Option<Instant>,
    network: Option<Instant>,
) -> Option<Ready> {
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

/// The next task on the virtual clock of a scripted session, from arrival
/// orders and loop times only: the oldest completion (its order and the loop
/// time it arrived at) runs before a host task received after it and before
/// a timer (due by the horizon) that became due after it arrived; otherwise
/// host tasks come first, then the timer, then the completion.
fn virtual_next(
    host: Option<u64>,
    timer: Option<f64>,
    network: Option<(u64, f64)>,
) -> Option<Ready> {
    if let Some((order, arrived)) = network
        && host.is_none_or(|host| order < host)
        && timer.is_none_or(|due| due > arrived)
    {
        return Some(Ready::Network);
    }
    if host.is_some() {
        Some(Ready::Host)
    } else if timer.is_some() {
        Some(Ready::Timer)
    } else {
        network.map(|_| Ready::Network)
    }
}

pub struct Application {
    execution: Execution,
    instance: Instance,
    _module: PreparedModule,
    state: InstanceState,
    failure: Option<RuntimeError>,
    /// Network completions received, in arrival order.
    network: VecDeque<Arrival>,
    max_turns: Option<u64>,
    /// A scripted headless session's virtual clock; `None`: timers follow
    /// the wall clock.
    horizon: Option<Horizon>,
    /// The current idle period already ran its maintenance (v38
    /// `incremental_collection`); a turn starts the next period.
    maintained: bool,
    /// Wall bound of an explicit scripted wait; it never changes the
    /// instance lifetime or a transfer's original operation deadline.
    network_wait: Option<NetworkWait>,
}

#[derive(Clone, Copy)]
struct NetworkWait {
    step: usize,
    until: Instant,
    timeout: Duration,
}
impl NetworkWait {
    fn error(self) -> ApplicationError {
        ApplicationError::NetworkWait {
            step: self.step,
            timeout: self.timeout,
        }
    }
}

/// When a timer is ready on the virtual clock of a scripted session; nothing
/// waits for the wall clock.
#[derive(Debug, Clone, Copy)]
enum Horizon {
    /// Before an input: timers due by the loop time, and, while a timer with
    /// the minimum delay is armed, the earliest timer in (due, sequence)
    /// order, so zero-delay timers and their chains fire first in Node's
    /// order.
    Input,
    /// During a wait: timers due at or before this loop time.
    Until(f64),
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
        execution.store.data_mut().view = match options.view {
            Some(view) => crate::view::Session::simulated(view.environment, view.platform),
            None => crate::view::Session::hosted(options.platform_effects),
        };
        if let Some(layout) = options.view_layout {
            execution.store.data_mut().view.set_layout(layout)?;
        }
        let instance = execution.instantiate_prepared(&module)?;
        let mut application = Application {
            execution,
            instance,
            _module: module,
            state: InstanceState::Running,
            failure: None,
            network: VecDeque::new(),
            max_turns: options.max_turns,
            horizon: None,
            maintained: false,
            network_wait: None,
        };
        let result = (|| {
            let store = &mut application.execution.store;
            if let Some(start) = instance.get_func(&mut *store, varta_common::abi::START_EXPORT) {
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

    /// Run a view application headlessly (v38 `view_protocol()`): an
    /// application instance whose simulated platform performs `script`'s
    /// inputs, as trusted platform inputs between turns, on virtual time.
    /// An input is performed only when no task is runnable: every host task
    /// and arrived completion has run and every timer due within the minimum
    /// timer delay of the last turn's loop time has fired (so zero-delay
    /// timers and their chains run first), each as its own turn. A `wait`
    /// step advances loop time by its milliseconds, firing every timer due by
    /// then. An explicit `wait_network` waits under its wall-clock bound for
    /// armed I/O and reactions, keeping virtual timer time fixed. Other steps
    /// never wait for requests implicitly. After the last
    /// step the session settles once more and reports the final tree and
    /// accessibility snapshot.
    #[must_use]
    pub fn run_view(
        &self,
        bytes: &[u8],
        mut options: ApplicationOptions,
        script: &varta_view::Script,
    ) -> ViewReport {
        let mut report = RunReport::new(crate::artifact_identity(bytes));
        if let Err(error) = script.validate() {
            report.failure = Some(error.into());
            return ViewReport {
                report,
                snapshot: None,
                commits: 0,
            };
        }
        options.view = Some(ViewPlatform {
            environment: script.environment(),
            platform: script.platform.clone(),
        });
        let mut application = match self.start_application(bytes, options) {
            Ok(application) => application,
            Err(error) => {
                report.failure = Some(error);
                return ViewReport {
                    report,
                    snapshot: None,
                    commits: 0,
                };
            }
        };
        // Virtual time: loop time stays program-defined, as under main.
        application.horizon = Some(Horizon::Input);
        for (index, step) in script.inputs.iter().enumerate() {
            let number = index + 1;
            match *step {
                varta_view::ScriptStep::Input(ref input) => {
                    if !application.settle_step(number, Horizon::Input)
                        || application.view_input(input).is_err()
                    {
                        break;
                    }
                }
                varta_view::ScriptStep::Wait { ms } => {
                    let end = application.loop_time() + ms;
                    if !application.settle_step(number, Horizon::Until(end)) {
                        break;
                    }
                    application.advance_loop_time(end);
                }
                varta_view::ScriptStep::WaitNetwork { ms } => {
                    if !application.wait_network(number, Duration::from_secs_f64(ms / 1000.0)) {
                        break;
                    }
                }
            }
        }
        // After the last step the session settles once more and ends; a
        // timer no wait reached never fires.
        application.settle_step(script.inputs.len() + 1, Horizon::Input);
        let snapshot = Some(application.view_snapshot());
        let commits = application.view_commits();
        application.report_into(&mut report);
        ViewReport {
            report,
            snapshot,
            commits,
        }
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

    /// Perform one trusted platform input on the instance's view between
    /// turns (host work: no guest code, fuel or turn deadline). The records
    /// it produces run in later turns. A failure (an input naming no
    /// attached element, an overflowing record queue) terminates the
    /// instance; [`Self::failure`] names it.
    pub fn view_input(&mut self, input: &varta_view::Input) -> Result<(), ApplicationError> {
        if self.state != InstanceState::Running {
            return Err(ApplicationError::Terminated);
        }
        let result = crate::view::input(self.execution.store.data_mut(), input);
        if let Err(error) = result {
            self.fail(error);
            return Err(ApplicationError::Terminated);
        }
        Ok(())
    }

    /// Capture the current field for host-owned copy/cut/paste. The binding holds
    /// source/selection identities, never clipboard contents or password text.
    pub fn view_clipboard_target(&self) -> Option<varta_view::ClipboardTarget> {
        (self.state == InstanceState::Running)
            .then(|| self.execution.store.data().view.clipboard_target())
            .flatten()
    }
    pub fn view_clipboard_pending(&self) -> bool {
        self.execution.store.data().view.clipboard_pending()
    }
    /// Produce bounded platform work after default-action/target validation.
    /// Perform the I/O outside the application/UI loop, then complete below.
    pub fn view_begin_clipboard(
        &mut self,
        target: Option<&varta_view::ClipboardTarget>,
        key: &varta_view::ClipboardKey,
    ) -> Result<Option<varta_view::ClipboardTask>, ApplicationError> {
        if self.state != InstanceState::Running {
            return Err(ApplicationError::Terminated);
        }
        match crate::view::begin_clipboard(self.execution.store.data_mut(), target, key) {
            Ok(task) => Ok(task),
            Err(error) => {
                self.fail(error);
                Err(ApplicationError::Terminated)
            }
        }
    }
    /// Complete once between turns. Failed/stale I/O cannot mutate a field and
    /// resulting input events carry no new gesture authority.
    pub fn view_complete_clipboard(
        &mut self,
        token: &varta_view::ClipboardToken,
        result: Result<varta_view::ClipboardData, varta_view::ClipboardError>,
    ) -> Result<bool, ApplicationError> {
        if self.state != InstanceState::Running {
            return Err(ApplicationError::Terminated);
        }
        match crate::view::complete_clipboard(self.execution.store.data_mut(), token, result) {
            Ok(accepted) => Ok(accepted),
            Err(error) => {
                self.fail(error);
                Err(ApplicationError::Terminated)
            }
        }
    }

    /// Apply an experimental host scroll between turns and queue its view events.
    pub fn view_scroll_into_view(
        &mut self,
        target: u32,
        width: f32,
        height: f32,
        font: Option<(&mut varta_view::TextMeasurer, f32)>,
    ) -> Result<usize, ApplicationViewScrollError> {
        if self.state != InstanceState::Running {
            return Err(ApplicationError::Terminated.into());
        }
        let context = self.execution.store.data_mut();
        let count = context.view.scroll_into_view(target, width, height, font)?;
        if let Err(error) = context.view.queue(&mut context.events) {
            self.fail(RuntimeError::View(error));
            return Err(ApplicationError::Terminated.into());
        }
        Ok(count)
    }
    /// The loop time of the instance, in milliseconds since it started.
    fn loop_time(&self) -> f64 {
        self.execution.store.data().scheduler.loop_time()
    }
    fn advance_loop_time(&mut self, to: f64) {
        self.execution.store.data_mut().scheduler.advance(to);
    }
    /// Run every task that is runnable on the virtual clock up to `horizon`
    /// (host tasks by arrival, then due timers, then completions that have
    /// arrived), each as its own turn, until none is. Returns whether the
    /// instance is still running; more than [`varta_view::Script::MAX_STEP_TURNS`]
    /// turns fails it.
    fn settle_step(&mut self, step: usize, horizon: Horizon) -> bool {
        let mut turns = 0;
        self.horizon = Some(horizon);
        loop {
            match self.step(Some(Duration::ZERO)) {
                Step::Turn(_) => {}
                Step::Idle => return true,
                Step::Finished { .. } | Step::Failed => return false,
            }
            turns += 1;
            if turns > varta_view::Script::MAX_STEP_TURNS {
                self.fail(ApplicationError::Unsettled { step }.into());
                return false;
            }
        }
    }

    /// Settle already runnable work normally, then hold virtual loop time
    /// fixed while armed I/O and its reactions finish. There is no implicit
    /// virtual timer advance and no promise of deterministic real I/O order.
    fn wait_network(&mut self, step: usize, timeout: Duration) -> bool {
        let first_turn = self.execution.store.data().turns.current();
        let wait = NetworkWait {
            step,
            until: Instant::now() + timeout,
            timeout,
        };
        self.network_wait = Some(wait);
        if !self.settle_step(step, Horizon::Input) {
            self.network_wait = None;
            return false;
        }
        self.horizon = Some(Horizon::Until(self.loop_time()));
        let running = loop {
            let remaining = wait.until.saturating_duration_since(Instant::now());
            match self.step(Some(remaining)) {
                Step::Turn(_) => {
                    let turns = self
                        .execution
                        .store
                        .data()
                        .turns
                        .current()
                        .saturating_sub(first_turn);
                    if turns > varta_view::Script::MAX_STEP_TURNS {
                        self.fail(ApplicationError::Unsettled { step }.into());
                        break false;
                    }
                }
                Step::Idle => {
                    if self.execution.store.data().net.alive() {
                        self.fail(wait.error().into());
                        break false;
                    }
                    break true;
                }
                Step::Finished { .. } | Step::Failed => break false,
            }
        };
        self.network_wait = None;
        running
    }

    /// The view's tree and accessibility snapshot.
    #[must_use]
    pub fn view_snapshot(&self) -> serde_json::Value {
        self.execution.store.data().view.snapshot()
    }
    /// Platform-neutral accessibility data, without a full diagnostic snapshot.
    /// Observer work refusal does not terminate the application or mutate its view.
    pub fn view_accessibility_tree(
        &self,
    ) -> Result<varta_view::AccessibilityTree, varta_view::AccessibilityError> {
        self.execution.store.data().view.accessibility_tree()
    }
    /// Trusted assistive input between turns. Stale or unsupported actions are
    /// ignored without events; admitted actions retain ordinary event limits.
    pub fn view_accessibility_input(
        &mut self,
        identity: &varta_view::NodeIdentity,
        action: &varta_view::AccessibilityAction,
    ) -> Result<bool, ApplicationError> {
        self.assistive_input(identity, action, None)
    }
    /// Frame-dependent assistive input. Hosts retain the acknowledged model,
    /// viewport, theme and scale guards before calling this between turns.
    pub fn view_accessibility_input_from_frame(
        &mut self,
        identity: &varta_view::NodeIdentity,
        action: &varta_view::AccessibilityAction,
        frame: &varta_view::FrameMetadata,
    ) -> Result<bool, ApplicationError> {
        self.assistive_input(identity, action, Some(frame))
    }
    fn assistive_input(
        &mut self,
        identity: &varta_view::NodeIdentity,
        action: &varta_view::AccessibilityAction,
        frame: Option<&varta_view::FrameMetadata>,
    ) -> Result<bool, ApplicationError> {
        if self.state != InstanceState::Running {
            return Err(ApplicationError::Terminated);
        }
        match crate::view::accessibility_input(
            self.execution.store.data_mut(),
            identity,
            action,
            frame,
        ) {
            Ok(accepted) => Ok(accepted),
            Err(error) => {
                self.fail(error);
                Err(ApplicationError::Terminated)
            }
        }
    }
    /// Bounded diagnostic JSON export; failure leaves the application running.
    pub fn view_snapshot_json(
        &self,
        format: crate::ViewSnapshotFormat,
    ) -> Result<Vec<u8>, crate::ViewSnapshotError> {
        crate::encode_view_snapshot(&self.view_snapshot(), format)
    }
    /// The title committed by the guest view.
    #[must_use]
    pub fn view_title(&self) -> &str {
        self.execution.store.data().view.title()
    }
    /// View batches applied so far.
    #[must_use]
    pub fn view_commits(&self) -> usize {
        self.execution.store.data().view.commits()
    }

    /// Update the viewport conditions used by view styles and platform input.
    pub fn view_set_environment(
        &mut self,
        environment: varta_view::Environment,
    ) -> Result<(), varta_view::LayoutError> {
        self.execution
            .store
            .data_mut()
            .view
            .set_environment(environment)
    }

    /// Update the host geometry used by later layout-dependent commands.
    pub fn view_set_layout_size(
        &mut self,
        width: f32,
        height: f32,
    ) -> Result<(), varta_view::LayoutError> {
        self.execution
            .store
            .data_mut()
            .view
            .set_layout_size(width, height)
    }

    /// Cancel any transient IME preedit without committing field text.
    pub fn view_cancel_preedit(&mut self) {
        self.execution.store.data_mut().view.cancel_preedit();
    }
    /// Update host pointer CSS state without a guest event or activation.
    pub fn view_set_pointer_state(&mut self, hovered: Option<u32>, active: Option<u32>) -> bool {
        self.execution
            .store
            .data_mut()
            .view
            .set_pointer_state(hovered, active)
    }

    /// Render the current view to a bounded CPU image using explicitly supplied fonts.
    /// This is host-side work between guest turns; unsupported view content fails closed.
    pub fn view_rasterize(
        &self,
        width: u32,
        height: u32,
        font: Option<(&mut varta_view::TextMeasurer, f32)>,
    ) -> Result<varta_view::RasterImage, varta_view::PaintError> {
        self.execution
            .store
            .data()
            .view
            .rasterize(width, height, font)
    }

    /// Render into physical pixels using CSS layout and font sizes at the given scale.
    pub fn view_rasterize_scaled(
        &self,
        width: u32,
        height: u32,
        scale: f32,
        font: Option<(&mut varta_view::TextMeasurer, f32)>,
    ) -> Result<varta_view::RasterImage, varta_view::PaintError> {
        self.execution
            .store
            .data()
            .view
            .rasterize_scaled(width, height, scale, font)
    }

    /// Render through an embedding-owned reusable native renderer. Caches do not
    /// alter guest state, event delivery or the stateless raster APIs.
    pub fn view_render(
        &self,
        renderer: &mut varta_view::Renderer,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<varta_view::RasterImage, varta_view::PaintError> {
        self.execution
            .store
            .data()
            .view
            .render(renderer, width, height, scale)
    }

    /// Prepare a host-owned vector scene, preserving the ordinary view limits.
    pub fn view_prepare(
        &self,
        renderer: &mut varta_view::Renderer,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<varta_view::PreparedFrame, varta_view::PaintError> {
        self.execution
            .store
            .data()
            .view
            .prepare(renderer, width, height, scale)
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
            // No task is runnable: the idle period's collection work runs
            // once, under the maintenance budget, before the instance waits.
            if !self.maintained {
                self.maintained = true;
                if let Err(error) = crate::maintenance::idle(
                    &self.instance,
                    &mut self.execution.store,
                    self.network_wait.map(|wait| wait.until),
                ) {
                    self.fail(error);
                    return Step::Failed;
                }
                continue;
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
        if let Some(wait) = self.network_wait
            && Instant::now() >= wait.until
        {
            return Some(wait.error().into());
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

    /// Receive a network completion: stamp it with the next arrival order
    /// and the loop time.
    fn receive(&mut self, event: crate::net::Event) {
        let context = self.execution.store.data_mut();
        let order = context.events.stamp();
        let loop_time = context.scheduler.loop_time();
        self.network.push_back(Arrival {
            event,
            order,
            loop_time,
        });
    }

    /// The earliest ready task by arrival (FIFO across host tasks, due timers
    /// and network completions), if any. On the wall clock a host task's
    /// instant is its arrival, a timer's its due time and a completion's the
    /// moment its transfer reported it; ties go to the host task, then the
    /// timer. On the virtual clock of a scripted session nothing reads the
    /// wall clock: a completion runs before a host task received after it
    /// and before a timer that became due after it arrived (by loop time),
    /// and otherwise host tasks come first, then a timer due by the horizon.
    fn ready(&mut self) -> Option<Ready> {
        self.execution.store.data_mut().net.drive();
        while let Some(event) = self.execution.store.data_mut().net.poll() {
            self.receive(event);
        }
        let context = self.execution.store.data_mut();
        if let Some(horizon) = self.horizon {
            let scheduler = &context.scheduler;
            let due = |due: f64| match horizon {
                Horizon::Input => due <= scheduler.loop_time() || scheduler.immediate_armed(),
                Horizon::Until(end) => due <= end,
            };
            let timer = scheduler.next_due(true).filter(|at| due(*at));
            let network = self
                .network
                .front()
                .map(|arrival| (arrival.order, arrival.loop_time));
            return virtual_next(context.events.next_order(), timer, network);
        }
        let now = Instant::now();
        let host = context.events.next_arrival();
        let timer = context
            .scheduler
            .next_due(true)
            .map(|due| scheduler::target(context, due))
            .filter(|at| *at <= now);
        let network = self.network.front().map(|arrival| arrival.event.arrived());
        wall_next(host, timer, network)
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
            || context.view.attached()
    }

    /// Wait for the next timer or network completion, never past `until`.
    /// Returns false when nothing can arrive before `until` (or at all
    /// without the embedder).
    fn wait(&mut self, until: Option<Instant>) -> Result<bool, RuntimeError> {
        let context = self.execution.store.data_mut();
        // On the virtual clock a timer never becomes due by waiting.
        let timer = match self.horizon {
            Some(_) => None,
            None => context
                .scheduler
                .next_due(true)
                .map(|due| scheduler::target(context, due)),
        };
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
                self.receive(event);
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
        let until = self.network_wait.map(|wait| wait.until);
        let begin = |store: &mut wasmtime::Store<crate::RuntimeContext>,
                     kind|
         -> Result<(), RuntimeError> {
            turns::begin(store, kind)?;
            if let Some(until) = until {
                let context = store.data_mut();
                context.deadline = context.deadline.min(until);
                context.rearm_deadline();
            }
            Ok(())
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
                        TaskKind::View => TurnKind::View,
                    };
                    begin(store, kind)?;
                    let context = store.data_mut();
                    // The task's gesture flag makes a user-gesture turn: the
                    // broker's prompts and the view's `copy_text` read it.
                    context.broker.gesture = task.gesture;
                    if let Payload::View(record) = &task.payload {
                        crate::view::deliver(&self.instance, store, record)?;
                        exceptions::finish(&self.instance, store)?;
                    } else {
                        context.events.deliver(&mut context.scheduler, task)?;
                    }
                }
                Ready::Timer => {
                    begin(store, kind)?;
                    store
                        .data_mut()
                        .scheduler
                        .fire()
                        .map_err(wasmtime::Error::new)?;
                }
                Ready::Network => {
                    begin(store, kind)?;
                    if let Some(Arrival { event, .. }) = self.network.pop_front() {
                        let context = store.data_mut();
                        context.net.deliver(&mut context.scheduler, event)?;
                        if self.horizon.is_none() {
                            scheduler::observed_wall_clock(context);
                        }
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
        self.maintained = false;
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
        let error = match (&error, self.network_wait) {
            (RuntimeError::Budget(budget), Some(wait))
                if matches!(
                    budget.budget,
                    varta_common::budget::BudgetKind::Deadline
                        | varta_common::budget::BudgetKind::MaintenanceDeadline
                ) && Instant::now() >= wait.until
                    && self
                        .execution
                        .store
                        .data()
                        .lifetime
                        .is_none_or(|end| end > wait.until) =>
            {
                wait.error().into()
            }
            _ => error,
        };
        self.state = InstanceState::Failed;
        self.failure = Some(error);
    }

    fn report_into(mut self, report: &mut RunReport) {
        report.failure = self.failure.take();
        report.collect_profile(&mut self.execution.store);
        report.collect(self.execution.store.data_mut());
        if let InstanceState::Finished { exit_code } = self.state {
            report.output.exit_code = exit_code;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On the wall clock tasks run FIFO by arrival: a completion that arrived
    /// before a host task or a due timer runs first, so a sustained stream of
    /// either cannot starve it; ties keep host task, timer, completion.
    #[test]
    fn wall_clock_tasks_run_by_arrival() {
        let start = Instant::now();
        let at = |ms: u64| Some(start + Duration::from_millis(ms));
        assert_eq!(wall_next(at(5), at(3), at(1)), Some(Ready::Network));
        assert_eq!(wall_next(at(1), at(3), at(5)), Some(Ready::Host));
        assert_eq!(wall_next(at(4), at(2), at(3)), Some(Ready::Timer));
        assert_eq!(wall_next(at(1), at(1), at(1)), Some(Ready::Host));
        assert_eq!(wall_next(None, at(1), at(1)), Some(Ready::Timer));
        assert_eq!(wall_next(None, None, None), None);
    }

    /// On the virtual clock the same order is derived without the wall
    /// clock: completions interleave with host tasks by arrival order and
    /// with timers by the loop time they arrived at.
    #[test]
    fn virtual_clock_tasks_interleave_by_arrival() {
        // A completion received after host task 1 and before host task 3.
        assert_eq!(
            virtual_next(Some(1), None, Some((2, 0.0))),
            Some(Ready::Host)
        );
        assert_eq!(
            virtual_next(Some(3), None, Some((2, 0.0))),
            Some(Ready::Network)
        );
        // A timer due at or before the completion's arrival runs first; one
        // that became due after it (a re-armed chain) does not.
        assert_eq!(
            virtual_next(None, Some(10.0), Some((2, 10.0))),
            Some(Ready::Timer)
        );
        assert_eq!(
            virtual_next(None, Some(11.0), Some((2, 10.0))),
            Some(Ready::Network)
        );
        assert_eq!(virtual_next(Some(5), Some(1.0), None), Some(Ready::Host));
        assert_eq!(virtual_next(None, Some(1.0), None), Some(Ready::Timer));
        assert_eq!(
            virtual_next(None, None, Some((1, 0.0))),
            Some(Ready::Network)
        );
        assert_eq!(virtual_next(None, None, None), None);
        // A chain of later host tasks and timers never overtakes it.
        for later in 3..100 {
            assert_eq!(
                virtual_next(Some(later), Some(f64::from(later as u32)), Some((2, 1.0))),
                Some(Ready::Network)
            );
        }
    }
}
