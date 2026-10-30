#![forbid(unsafe_code)]
//! Capability-checked Wasmtime execution. Only portable Wasm enters this API.
mod application;
#[cfg(test)]
mod array_tests;
mod bigint;
#[cfg(test)]
mod bigint_abi_tests;
#[cfg(test)]
mod bigint_memory_tests;
#[cfg(test)]
mod bigint_tests;
#[cfg(test)]
mod bigint_values_abi_tests;
#[cfg(test)]
mod bigint_values_tests;
#[cfg(test)]
mod contract_tests;
mod contracts;
pub use application::{
    Application, ApplicationError, ApplicationOptions, InstanceState, Step, ViewPlatform,
    ViewReport,
};
mod events;
pub use events::{Delivery, HostEvent};
mod exceptions;
mod execution;
mod fuel_profile;
#[cfg(test)]
mod fuel_profile_tests;
mod turns;
pub use permission::PermissionPrompter;
pub use turns::{BUDGET_REPORT_PREFIX, BudgetExhausted, BudgetReport, TurnKind, TurnUsage};
mod prepared;
#[cfg(test)]
mod prepared_tests;
pub use prepared::PreparedModule;
mod host;
mod http;
#[cfg(test)]
mod http_abi_tests;
mod http_contract;
mod references;
mod resources;
mod server;
#[cfg(test)]
mod server_tests;
mod unions;
mod utf16;
pub use server::{HttpShutdown, RuntimeEvent, RuntimeObserver};
#[cfg(test)]
mod array_inference_tests;
#[cfg(test)]
mod async_generator_tests;
#[cfg(test)]
mod async_method_tests;
#[cfg(test)]
mod async_tests;
#[cfg(test)]
mod bitwise_compound_tests;
#[cfg(test)]
mod bitwise_tests;
#[cfg(test)]
mod call_flow_tests;
#[cfg(test)]
mod data_limit_tests;
#[cfg(test)]
mod default_export_tests;
#[cfg(test)]
mod exit_flow_tests;
#[cfg(test)]
mod frame_admission_tests;
#[cfg(test)]
mod frame_limit_tests;
#[cfg(test)]
mod frame_liveness_tests;
mod frames;
#[cfg(test)]
mod generator_tests;
#[cfg(test)]
mod guest_truth_tests;
#[cfg(test)]
mod interface_extends_tests;
#[cfg(test)]
mod invocation_tests;
mod invoke;
mod json;
#[cfg(test)]
mod json_limit_tests;
mod json_parse;
#[cfg(test)]
mod json_parse_abi_tests;
#[cfg(test)]
mod json_parse_tests;
mod language;
#[cfg(test)]
mod language_tests;
#[cfg(test)]
mod literal_type_tests;
#[cfg(test)]
mod logical_assignment_tests;
#[cfg(test)]
mod math_f16_tests;
#[cfg(test)]
mod math_hypot_tests;
#[cfg(test)]
mod math_tests;
#[cfg(test)]
mod module_constant_tests;
#[cfg(test)]
mod mutable_flow_tests;
#[cfg(test)]
mod named_type_tests;
mod native_math;
#[cfg(test)]
mod native_math_tests;
mod net;
#[cfg(test)]
mod net_loopback;
#[cfg(test)]
mod net_tests;
mod number;
mod number_format;
#[cfg(test)]
mod number_format_tests;
mod number_parse;
#[cfg(test)]
mod number_parse_tests;
#[cfg(test)]
mod number_predicate_tests;
mod number_radix;
#[cfg(test)]
mod number_radix_tests;
#[cfg(test)]
mod number_tests;
#[cfg(test)]
mod number_text_tests;
#[cfg(test)]
mod number_value_tests;
#[cfg(test)]
mod numeric_constant_tests;
#[cfg(test)]
mod parameter_flow_tests;
mod permission;
mod policy;
#[cfg(test)]
mod post_write_tests;
#[cfg(test)]
mod power_tests;
#[cfg(test)]
mod predicate_search_tests;
#[cfg(test)]
mod primitive_builtin_tests;
#[cfg(test)]
mod primitive_comparison_tests;
#[cfg(test)]
mod primitive_compound_tests;
mod primitive_number;
#[cfg(test)]
mod primitive_number_tests;
#[cfg(test)]
mod primitive_operator_tests;
mod primitive_string;
#[cfg(test)]
mod primitive_string_tests;
#[cfg(test)]
mod promise_tests;
#[cfg(test)]
mod promise_value_tests;
#[cfg(test)]
mod property_deletion_tests;
mod random;
#[cfg(test)]
mod random_tests;
#[cfg(test)]
mod record_compound_tests;
#[cfg(test)]
mod record_tests;
#[cfg(test)]
mod reduce_right_tests;
mod scheduler;
#[cfg(test)]
mod timer_tests;
#[cfg(test)]
mod truth_guard_tests;
#[cfg(test)]
mod turn_budget_tests;
#[cfg(test)]
mod undefined_tests;
#[cfg(test)]
mod update_coercion_tests;
#[cfg(test)]
mod update_tests;
mod validate;
mod view;
#[cfg(test)]
mod view_tests;
pub use varta_view::{Script as ViewScript, ScriptError as ViewScriptError, ViewError};

use std::collections::BTreeMap;
use std::time::Instant;
#[cfg(test)]
use varta_common::abi;
use varta_common::budget::{BudgetKind, BudgetRequest, Budgets};
use varta_common::permission::{
    AuditEntry, Consent, DecisionFile, PermissionError, StoredDecision,
};
use varta_common::{CapabilityGrant, Manifest, abi::HostStatus};
use wasmtime::{Config, Engine, Module, StoreLimits};

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Uncaught(#[from] varta_common::exceptions::UncaughtException),
    #[error(transparent)]
    Language(#[from] varta_common::language::LanguageError),
    #[error(transparent)]
    Http(#[from] varta_common::http::HttpError),
    #[error(transparent)]
    ValueAbi(#[from] varta_common::value::ValueAbiError),
    #[error(transparent)]
    Frames(#[from] varta_common::function::admission::FrameAdmissionError),
    #[error(transparent)]
    ToolContract(#[from] varta_common::ToolContractError),
    #[error(transparent)]
    Policy(#[from] varta_common::VartaError),
    #[error(transparent)]
    Scheduler(#[from] varta_common::scheduler::SchedulerError),
    #[error(transparent)]
    Net(#[from] varta_common::net::NetError),
    /// A v38 (`data_limits`) guest exhausted a real resource.
    #[error(transparent)]
    Resource(#[from] varta_common::limits::ResourceExhausted),
    /// A v38 turn exhausted a per-turn or per-instance budget.
    #[error(transparent)]
    Budget(#[from] BudgetExhausted),
    #[error(transparent)]
    Permission(#[from] PermissionError),
    #[error(transparent)]
    Event(#[from] varta_common::events::EventError),
    #[error(transparent)]
    Application(#[from] ApplicationError),
    /// V38: an invalid view batch, a second commit in one turn or an
    /// overflowing view event queue; fatal for the instance.
    #[error(transparent)]
    View(ViewError),
    /// V38: a headless script input that names no attached element.
    #[error(transparent)]
    ViewInput(varta_view::InputError),
    #[error("{0:#}")]
    Execution(#[from] wasmtime::Error),
}

#[derive(Debug, Default)]
pub struct RunOutput {
    pub lines: Vec<String>,
    pub exit_code: i32,
}

pub(crate) struct RuntimeContext {
    policy: policy::Policy,
    limits: StoreLimits,
    output: RunOutput,
    output_bytes: usize,
    /// Host calls of the current turn (of the whole execution before v38).
    host_calls: u32,
    /// The end of the current turn (of the whole execution before v38).
    deadline: Instant,
    /// When the instance was created; loop time is measured from here.
    started: Instant,
    /// The end of the instance: the execution deadline of a bounded run,
    /// the embedder's optional lifetime bound of an application instance.
    lifetime: Option<Instant>,
    budgets: varta_common::budget::Budgets,
    turns: turns::Turns,
    /// The permission broker (v37), with broker v1 states in v38.
    broker: permission::Broker,
    /// V38 host event sources and the host task queue.
    events: events::HostEvents,
    /// Rearms the store's epoch thread for the next turn's deadline.
    rearm: Option<std::sync::mpsc::Sender<Option<Instant>>>,
    http_control: Option<(String, HttpShutdown)>,
    random: random::RandomSource,
    scheduler: scheduler::Scheduler,
    /// V37 network requests: ids, completions and transfer tasks.
    net: net::Net,
    function_catalog: Option<std::sync::Arc<varta_common::function::VerifiedFunctionCatalog>>,
    /// The instantiated module's bounds (v38 `program_limits`): where its
    /// record descriptors, keys and JSON schemas may lie, and its profile's
    /// structural limits.
    module: varta_common::value::ModuleBounds,
    /// Present only in a fuel-profiling execution (`fuel_profile`).
    profiler: Option<Box<fuel_profile::Profiler>>,
    /// V38 view protocol: the renderer and its commit-per-turn rule; its
    /// event records are host tasks of the instance (`events`).
    view: view::Session,
}

/// A fuel-profiled execution ([`VartaRuntime::run_fuel_profile`]). A trap or
/// an exhausted budget keeps the output and the profile up to that point.
#[derive(Debug)]
pub struct FuelProfileRun {
    pub output: RunOutput,
    pub profile: varta_common::profile::FuelProfile,
    /// Why the execution stopped early; a guest exit is not an error.
    pub error: Option<RuntimeError>,
}

impl RuntimeContext {
    fn tick(&mut self) -> Result<(), wasmtime::Error> {
        self.host_calls += 1;
        self.check_budget()
    }

    fn check_budget(&self) -> Result<(), wasmtime::Error> {
        if self.host_calls > self.budgets.host_calls_per_turn {
            return Err(self.turns.exhausted(
                BudgetKind::HostCalls,
                &self.budgets,
                turns::HOST_BUDGET_EXHAUSTED,
            ));
        }
        self.check_deadline()
    }

    /// The deadline of host bookkeeping: typed in a v38 turn, the frozen
    /// legacy text before.
    pub(crate) fn check_deadline(&self) -> Result<(), wasmtime::Error> {
        if Instant::now() >= self.deadline {
            return Err(self.turns.exhausted(
                BudgetKind::Deadline,
                &self.budgets,
                turns::HOST_BUDGET_EXHAUSTED,
            ));
        }
        Ok(())
    }

    /// The error of a deadline reached while the event loop waited.
    pub(crate) fn deadline_error(&self) -> wasmtime::Error {
        self.turns.exhausted(
            BudgetKind::Deadline,
            &self.budgets,
            turns::DEADLINE_EXCEEDED,
        )
    }

    pub(crate) fn rearm_deadline(&self) {
        if let Some(rearm) = &self.rearm {
            // A stopped epoch thread only means the store is being dropped.
            let _ = rearm.send(Some(self.deadline));
        }
    }

    fn emit(&mut self, line: String) -> Result<(), wasmtime::Error> {
        let exhausted = |context: &Self| {
            context.turns.exhausted(
                BudgetKind::Output,
                &context.budgets,
                turns::OUTPUT_BUDGET_EXHAUSTED,
            )
        };
        let Some(bytes) = self.output_bytes.checked_add(line.len() + 1) else {
            return Err(exhausted(self));
        };
        self.output_bytes = bytes;
        if self.output_bytes > self.budgets.output_bytes {
            return Err(exhausted(self));
        }
        self.output.lines.push(line);
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct HostFailure {
    status: HostStatus,
    message: String,
}
impl HostFailure {
    fn denied(id: &str, target: &str) -> Self {
        // Keep the existing human-readable denial while ABI status remains structured.
        let label = if id.starts_with("env.") { "env" } else { id };
        Self {
            status: HostStatus::Denied,
            message: format!("<capability denied: {label}> ({target})"),
        }
    }
    fn io(error: impl std::fmt::Display) -> Self {
        Self {
            status: HostStatus::Io,
            message: format!("<host I/O error: {error}>"),
        }
    }
    fn too_large() -> Self {
        Self {
            status: HostStatus::TooLarge,
            message: "<result too large>".into(),
        }
    }
    fn invalid() -> Self {
        Self {
            status: HostStatus::InvalidInput,
            message: "<invalid host input>".into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("guest exit {0}")]
struct GuestExit(i32);

pub struct VartaRuntime {
    engine: Engine,
}

impl Default for VartaRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl VartaRuntime {
    #[must_use]
    pub fn new() -> Self {
        let mut config = Config::new();
        config.consume_fuel(true).epoch_interruption(true);
        // This configuration only enables supported built-in engine settings.
        let engine = Engine::new(&config).expect("valid built-in Wasmtime configuration");
        Self { engine }
    }

    /// Compile and validate portable bytecode; native machine code is never accepted.
    pub fn validate_module(&self, bytes: &[u8]) -> Result<Module, RuntimeError> {
        self.validate_module_and_tools(bytes)
            .map(|(module, _)| module)
    }

    /// Validate code and public contracts without instantiating or running guest code.
    pub fn validate_module_and_tools(
        &self,
        bytes: &[u8],
    ) -> Result<(Module, Option<varta_common::ToolCatalog>), RuntimeError> {
        self.prepare_module_and_tools(bytes)
            .map(|(prepared, tools)| (prepared.module, tools))
    }

    /// Validate portable code and retain its immutable private capture metadata.
    pub fn prepare_module(&self, bytes: &[u8]) -> Result<PreparedModule, RuntimeError> {
        self.prepare_module_and_tools(bytes)
            .map(|(module, _)| module)
    }

    /// Prepare code and its public contracts without executing guest code.
    pub fn prepare_module_and_tools(
        &self,
        bytes: &[u8],
    ) -> Result<(PreparedModule, Option<varta_common::ToolCatalog>), RuntimeError> {
        validate::module(&self.engine, bytes)
    }

    /// Inspect validated portable code without instantiation, filesystem loading
    /// or exposing backend types to packaging and CLI consumers.
    pub fn inspect_module(
        &self,
        bytes: &[u8],
    ) -> Result<varta_common::WasmInspection, RuntimeError> {
        let (module, tool_catalog) = self.validate_module_and_tools(bytes)?;
        Ok(varta_common::WasmInspection {
            language: language::inspect(bytes)?,
            value_abi_version: validate::value_version(bytes)?,
            imports: module
                .imports()
                .map(|import| varta_common::WasmImport {
                    module: import.module().into(),
                    name: import.name().into(),
                })
                .collect(),
            exports: module
                .exports()
                .map(|export| export.name().into())
                .collect(),
            memories: module
                .exports()
                .filter(|export| matches!(export.ty(), wasmtime::ExternType::Memory(_)))
                .count(),
            tool_catalog,
            http_service: http_contract::read(bytes)?,
        })
    }

    pub fn run_wasm_bytes(
        &self,
        bytes: &[u8],
        capabilities: &[CapabilityGrant],
    ) -> Result<RunOutput, RuntimeError> {
        self.execute(bytes, capabilities, None, None)
    }

    /// Run with the conjunction of package requests and a trusted host ceiling.
    /// An empty ceiling denies every privileged operation; it never means fallback.
    pub fn run_wasm_bytes_with_policy(
        &self,
        bytes: &[u8],
        requested: &[CapabilityGrant],
        ceiling: &[CapabilityGrant],
    ) -> Result<RunOutput, RuntimeError> {
        self.execute(bytes, requested, Some(ceiling), None)
    }

    /// Stream server readiness/diagnostics/output without retaining a lifetime log.
    pub fn run_observed(
        &self,
        bytes: &[u8],
        requested: &[CapabilityGrant],
        ceiling: Option<&[CapabilityGrant]>,
        observer: RuntimeObserver,
    ) -> Result<RunOutput, RuntimeError> {
        self.execute(bytes, requested, ceiling, Some(observer))
    }

    /// Run a fuel-profile build (`varta dev --profile-fuel`) with the same
    /// grants, ceiling and budgets as [`Self::run_wasm_bytes_with_policy`] and
    /// report the fuel of every guest function: [`Self::run_with`] with
    /// [`RunOptions::fuel_profile`], which also carries manifest budgets,
    /// decisions and the per-turn budget report. The probes refund their own
    /// cost, so the run consumes the fuel of the uninstrumented program. An
    /// HTTP service's startup is profiled; its server is not started.
    pub fn run_fuel_profile(
        &self,
        bytes: &[u8],
        requested: &[CapabilityGrant],
        ceiling: Option<&[CapabilityGrant]>,
    ) -> Result<FuelProfileRun, RuntimeError> {
        let report = self.run_with(
            bytes,
            RunOptions {
                requested: requested.to_vec(),
                ceiling: ceiling.map(<[CapabilityGrant]>::to_vec),
                fuel_profile: true,
                ..RunOptions::default()
            },
        );
        match report.fuel_profile {
            Some(profile) => Ok(FuelProfileRun {
                output: report.output,
                profile,
                error: report.failure,
            }),
            None => Err(report
                .failure
                .unwrap_or_else(|| wasmtime::Error::msg("missing fuel profiler").into())),
        }
    }

    fn execute(
        &self,
        bytes: &[u8],
        capabilities: &[CapabilityGrant],
        ceiling: Option<&[CapabilityGrant]>,
        observer: Option<RuntimeObserver>,
    ) -> Result<RunOutput, RuntimeError> {
        let options = RunOptions {
            requested: capabilities.to_vec(),
            ceiling: ceiling.map(<[CapabilityGrant]>::to_vec),
            observer,
            ..RunOptions::default()
        };
        self.run_with(bytes, options).into_result()
    }

    /// Run one bounded execution and report everything it produced: the
    /// output captured before a failure, the failure, the budget report and
    /// the permission audit log. The earlier entry points are this run with
    /// default options, minus the partial output of a failed run.
    #[must_use]
    pub fn run_with(&self, bytes: &[u8], options: RunOptions) -> RunReport {
        let mut report = RunReport::new(artifact_identity(bytes));
        let prepared = match self.prepare_instance(bytes, &options, execution::Mode::Bounded, None)
        {
            Ok(prepared) => prepared,
            Err(error) => {
                report.failure = Some(error);
                return report;
            }
        };
        let Prepared {
            module,
            mut execution,
            ..
        } = prepared;
        let mut instance = None;
        let result = (|| {
            let started = execution.instantiate_prepared(&module)?;
            let store = &mut execution.store;
            instance = Some(started);
            if let Some(start) = started.get_func(&mut *store, "_start") {
                start.call(&mut *store, &[], &mut [])?;
            }
            exceptions::finish(&started, store)?;
            scheduler::checkpoint(&started, store)?;
            Ok::<(), RuntimeError>(())
        })();
        let store = &mut execution.store;
        let result = result.map_err(scheduler::typed).map_err(|error| {
            let error = match &instance {
                Some(instance) => resources::classify(instance, store, error),
                None => error,
            };
            turns::classify(error, store.data())
        });
        turns::close(instance.as_ref(), store);
        let exited = result.is_err();
        if let Err(error) = result {
            if let RuntimeError::Execution(source) = &error
                && let Some(exit) = source.downcast_ref::<GuestExit>()
            {
                store.data_mut().output.exit_code = exit.0;
            } else {
                report.failure = Some(error);
            }
        }
        report.collect_profile(store);
        report.collect(store.data_mut());
        drop(execution);
        // A profiled HTTP service's startup is profiled; its server is not
        // started.
        if exited || options.fuel_profile {
            return report;
        }
        match http_contract::read(bytes) {
            Ok(Some(contract)) => {
                let output = std::mem::take(&mut report.output);
                match server::run(server::Start {
                    engine: self.engine.clone(),
                    module,
                    contract,
                    requested: &options.requested,
                    ceiling: options.ceiling.as_deref(),
                    output,
                    observer: options.observer.clone(),
                }) {
                    Ok(output) => report.output = output,
                    Err(error) => report.failure = Some(error),
                }
            }
            Ok(None) => {}
            Err(error) => report.failure = Some(error),
        }
        report
    }

    /// Validate the module and create its instance setup: the profile's turn
    /// semantics, the budgets resolved against the ceiling and the broker.
    fn prepare_instance(
        &self,
        bytes: &[u8],
        options: &RunOptions,
        mode: execution::Mode,
        prompter: Option<Box<dyn PermissionPrompter>>,
    ) -> Result<Prepared, RuntimeError> {
        let (module, profiled) = if options.fuel_profile {
            let names = fuel_profile::names(bytes)?;
            (
                validate::module_with(&self.engine, bytes, true)?.0,
                Some(names),
            )
        } else {
            (self.prepare_module(bytes)?, None)
        };
        let profile = language::inspect(bytes)?.profile;
        let v38 = profile.is_some_and(varta_common::language::LanguageProfile::turn_budgets);
        if let Some(ceiling) = &options.budget_ceiling {
            ceiling.validate()?;
        }
        options.budgets.validate()?;
        // Earlier profiles keep the production budgets and their meaning.
        let resolved = if v38 {
            Budgets::resolve(&options.budgets, options.budget_ceiling.as_ref())
        } else {
            Budgets::resolve(&BudgetRequest::default(), None)
        };
        let artifact = artifact_identity(bytes);
        let stored = match (&options.decisions, v38) {
            (Some(decisions), true) => decisions.decisions_for(&artifact),
            _ => BTreeMap::new(),
        };
        let consents = if v38 {
            let mut consents = options.consents.clone();
            for grant in &options.requested {
                consents
                    .entry(grant.id.clone())
                    .or_insert_with(Consent::required);
            }
            consents
        } else {
            BTreeMap::new()
        };
        let setup = execution::Setup {
            requested: &options.requested,
            ceiling: options.ceiling.as_deref(),
            budgets: resolved.budgets,
            capped: resolved.capped,
            renew: v38,
            mode,
            broker: permission::BrokerSetup {
                v1: v38,
                artifact,
                consents,
                stored,
                prompter,
            },
        };
        let mut execution = execution::Execution::with_setup(&self.engine, setup)?;
        if let Some(names) = profiled {
            let budget = execution.store.get_fuel()?;
            execution.store.data_mut().profiler =
                Some(Box::new(fuel_profile::Profiler::new(budget, names)));
        }
        Ok(Prepared { module, execution })
    }
}

/// A validated module with its instance setup, before instantiation.
struct Prepared {
    module: PreparedModule,
    execution: execution::Execution,
}

/// The application identity before C3 provenance: the SHA-256 digest of the
/// portable Wasm bytes (`sha256:` and lowercase hexadecimal).
#[must_use]
pub fn artifact_identity(bytes: &[u8]) -> String {
    use sha2::Digest;
    use std::fmt::Write;
    let digest = sha2::Sha256::digest(bytes);
    let mut text = String::from(varta_common::permission::ARTIFACT_PREFIX);
    for byte in digest {
        // Writing to a String cannot fail.
        let _ = write!(text, "{byte:02x}");
    }
    text
}

/// The inputs of one run besides the module.
#[derive(Default, Clone)]
pub struct RunOptions {
    pub requested: Vec<CapabilityGrant>,
    /// The trusted host ceiling; `None` means the package's requests alone.
    pub ceiling: Option<Vec<CapabilityGrant>>,
    /// Budgets the package requests (v38; ignored by earlier profiles).
    pub budgets: BudgetRequest,
    /// Budgets the host allows; without it the production defaults cap the
    /// request.
    pub budget_ceiling: Option<BudgetRequest>,
    /// Required/optional and purpose of each requested grant (v38).
    pub consents: BTreeMap<String, Consent>,
    /// Stored decisions keyed by application identity (the headless
    /// policy file); decisions for other identities are ignored.
    pub decisions: Option<DecisionFile>,
    pub observer: Option<RuntimeObserver>,
    /// Development only: `bytes` is a fuel-profile build
    /// ([`varta_common::profile`]); link its probes and report the fuel of
    /// every guest function across all turns in [`RunReport::fuel_profile`].
    pub fuel_profile: bool,
}
impl RunOptions {
    /// The options of a package manifest and an optional host ceiling
    /// manifest, both validated; a ceiling never carries consent metadata.
    pub fn from_manifests(
        package: &Manifest,
        host: Option<&Manifest>,
    ) -> Result<Self, RuntimeError> {
        if let Some(host) = host {
            host.validate_ceiling()?;
        }
        Ok(Self {
            requested: package.grants()?,
            ceiling: host.map(Manifest::grants).transpose()?,
            budgets: package.budget_request(),
            budget_ceiling: host.and_then(|host| host.budgets),
            consents: package.consents(),
            decisions: None,
            observer: None,
            fuel_profile: false,
        })
    }
}

/// Everything one run produced. Lines emitted before a failure are kept.
#[derive(Default)]
pub struct RunReport {
    pub output: RunOutput,
    pub failure: Option<RuntimeError>,
    /// `None` when the run failed before an instance existed.
    pub budget: Option<BudgetReport>,
    pub audit: Vec<AuditEntry>,
    /// Audit entries dropped beyond the bounded log.
    pub audit_dropped: u64,
    /// Stored decisions after the run, including prompt answers (v38).
    pub decisions: BTreeMap<String, StoredDecision>,
    /// The application identity decisions are keyed by.
    pub artifact: String,
    /// The per-function fuel of a fuel-profiling run
    /// ([`RunOptions::fuel_profile`]), up to a failure.
    pub fuel_profile: Option<varta_common::profile::FuelProfile>,
}
impl RunReport {
    fn new(artifact: String) -> Self {
        Self {
            artifact,
            ..Self::default()
        }
    }
    /// Close a fuel-profiling run's profile; a malformed probe sequence is
    /// the run's failure unless it already failed.
    fn collect_profile(&mut self, store: &mut wasmtime::Store<RuntimeContext>) {
        match fuel_profile::finish(store) {
            Some(Ok(profile)) => self.fuel_profile = Some(profile),
            Some(Err(error)) => {
                self.failure.get_or_insert(error);
            }
            None => {}
        }
    }
    fn collect(&mut self, context: &mut RuntimeContext) {
        self.output = std::mem::take(&mut context.output);
        self.budget = Some(context.turns.report(context.budgets, context.output_bytes));
        let (audit, dropped) = context.broker.audit_log();
        self.audit = audit;
        self.audit_dropped = dropped;
        self.decisions = context.broker.decisions();
    }
    /// The earlier entry points' result: the output, or only the failure.
    pub fn into_result(self) -> Result<RunOutput, RuntimeError> {
        match self.failure {
            Some(error) => Err(error),
            None => Ok(self.output),
        }
    }
}

#[cfg(test)]
mod linked_footprint_tests;
#[cfg(test)]
mod linked_intrinsic_tests;
#[cfg(test)]
mod reclamation_tests;
#[cfg(test)]
mod result_tests;
#[cfg(test)]
mod string_operation_tests;
#[cfg(test)]
mod string_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod utf16_tests;

#[cfg(test)]
mod union_tests;

#[cfg(test)]
mod missing_tests;

#[cfg(test)]
mod reference_tests;

#[cfg(test)]
mod checked_json_tests;

#[cfg(test)]
mod vector_tests;

#[cfg(test)]
mod array_methods_tests;

#[cfg(test)]
mod array_search_tests;

#[cfg(test)]
mod exception_tests;

#[cfg(test)]
mod error_tests;

#[cfg(test)]
mod error_family_tests;

#[cfg(test)]
mod recoverable_json_tests;

#[cfg(test)]
mod string_repeat_tests;
#[cfg(test)]
mod string_trim_tests;
#[cfg(test)]
mod template_literal_tests;

#[cfg(test)]
mod function_envelope_tests;
#[cfg(test)]
mod function_field_tests;
#[cfg(test)]
mod function_value14_tests;
#[cfg(test)]
mod function_value_tests;

#[cfg(test)]
mod closure_tests;

#[cfg(test)]
mod array_callback_tests;

#[cfg(test)]
mod sparse_array_tests;

#[cfg(test)]
mod array_mutation_tests;
#[cfg(test)]
mod array_reduce_tests;
#[cfg(test)]
mod assignment_pattern_tests;
#[cfg(test)]
mod assignment_value_tests;
#[cfg(test)]
mod callback_inference_tests;
#[cfg(test)]
mod local_binding_tests;
#[cfg(test)]
mod member_target_tests;
#[cfg(test)]
mod named_function_tests;
#[cfg(test)]
mod parameter_tests;
#[cfg(test)]
mod pattern_tests;
#[cfg(test)]
mod spread_tests;
#[cfg(test)]
mod this_tests;

#[cfg(test)]
mod for_of_tests;

#[cfg(test)]
mod do_while_tests;

#[cfg(test)]
mod label_tests;

#[cfg(test)]
mod object_keys_tests;

#[cfg(test)]
mod object_assign_tests;
#[cfg(test)]
mod object_entries_tests;
#[cfg(test)]
mod object_is_tests;
#[cfg(test)]
mod object_values_tests;

#[cfg(test)]
mod array_enumeration_tests;

#[cfg(test)]
mod abort_signal_tests;
#[cfg(test)]
mod catch_pattern_tests;
#[cfg(test)]
mod class_generic_tests;
#[cfg(test)]
mod class_tests;
#[cfg(test)]
mod collection_tests;
#[cfg(test)]
mod declared_field_tests;
#[cfg(test)]
mod dictionary_hashing_tests;
#[cfg(test)]
mod dictionary_tests;
#[cfg(test)]
mod error_subclass_tests;
#[cfg(test)]
mod frozen_profile_tests;
#[cfg(test)]
mod generic_tests;
#[cfg(test)]
mod language_core_tests;
#[cfg(test)]
mod mixed_record_tests;
#[cfg(test)]
mod program_limits_tests;
#[cfg(test)]
mod regexp_tests;
#[cfg(test)]
mod static_region_tests;
#[cfg(test)]
mod strict_member_tests;
#[cfg(test)]
mod subtyping_tests;
#[cfg(test)]
mod unknown_json_tests;
#[cfg(test)]
mod unknown_tests;
#[cfg(test)]
mod v32_cross_stream_tests;
#[cfg(test)]
mod v32_wave2_tests;
#[cfg(test)]
mod v34_cross_stream_tests;
#[cfg(test)]
mod v35_cross_stream_tests;
#[cfg(test)]
mod v36_cross_stream_tests;
#[cfg(test)]
mod v37_integration_tests;
#[cfg(test)]
mod value15_tests;
#[cfg(test)]
mod value16_tests;
#[cfg(test)]
mod value17_tests;
#[cfg(test)]
mod value18_tests;
#[cfg(test)]
mod web_headers_tests;
#[cfg(test)]
mod wide_record_tests;

#[cfg(test)]
mod string_range_tests;

#[cfg(test)]
mod string_search_tests;

#[cfg(test)]
mod compound_assignment_tests;
#[cfg(test)]
mod own_key_tests;
#[cfg(test)]
mod record_assignment_tests;
#[cfg(test)]
mod record_read_tests;
#[cfg(test)]
mod record_write_tests;
#[cfg(test)]
mod simple_assignment_tests;

/// Encode a value14 graph without invoking guest functions or `toJSON` hooks.
/// The owner catalog must come from immutable module metadata. `function_work`
/// is decremented by bounded capture validation; malformed graphs remain errors.
pub fn encode_value_graph_with_functions(
    data: &[u8],
    kind: varta_common::value::ValueKind,
    payload: u64,
    catalog: &varta_common::function::FunctionCatalog,
    function_work: &mut u64,
) -> Result<Option<Vec<u8>>, RuntimeError> {
    if catalog.value_version != varta_common::value::FUNCTION_VALUE_VERSION {
        return Err(wasmtime::Error::msg("function graph codec requires value14").into());
    }
    catalog.validate().map_err(wasmtime::Error::new)?;
    Ok(json::render_with_functions(
        data,
        kind as u32,
        payload,
        catalog,
        function_work,
    )?)
}
#[cfg(test)]
mod codegen_hotspot_tests;
#[cfg(test)]
mod codegen_v2_tests;
