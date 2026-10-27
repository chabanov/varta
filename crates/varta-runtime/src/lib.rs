#![forbid(unsafe_code)]
//! Capability-checked Wasmtime execution. Only portable Wasm enters this API.
#[cfg(test)]
mod array_tests;
#[cfg(test)]
mod contract_tests;
mod contracts;
mod exceptions;
mod execution;
mod host;
mod http;
#[cfg(test)]
mod http_abi_tests;
mod http_contract;
mod references;
mod server;
#[cfg(test)]
mod server_tests;
mod unions;
mod utf16;
pub use server::{HttpShutdown, RuntimeEvent, RuntimeObserver};
#[cfg(test)]
mod invocation_tests;
mod invoke;
mod json;
mod json_parse;
#[cfg(test)]
mod json_parse_abi_tests;
#[cfg(test)]
mod json_parse_tests;
mod language;
#[cfg(test)]
mod language_tests;
mod number;
#[cfg(test)]
mod number_tests;
#[cfg(test)]
mod number_value_tests;
mod policy;
#[cfg(test)]
mod record_tests;
#[cfg(test)]
mod undefined_tests;
mod validate;

use std::time::Instant;
use varta_common::{
    CapabilityGrant,
    abi::{self, HostStatus},
};
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
    ToolContract(#[from] varta_common::ToolContractError),
    #[error(transparent)]
    Policy(#[from] varta_common::VartaError),
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
    host_calls: u32,
    deadline: Instant,
    http_control: Option<(String, HttpShutdown)>,
}

impl RuntimeContext {
    fn tick(&mut self) -> Result<(), wasmtime::Error> {
        self.host_calls += 1;
        if self.host_calls > abi::MAX_HOST_CALLS || Instant::now() >= self.deadline {
            return Err(wasmtime::Error::msg("host operation budget exhausted"));
        }
        Ok(())
    }

    fn emit(&mut self, line: String) -> Result<(), wasmtime::Error> {
        self.output_bytes = self
            .output_bytes
            .checked_add(line.len() + 1)
            .ok_or_else(|| wasmtime::Error::msg("output budget exhausted"))?;
        if self.output_bytes > abi::MAX_OUTPUT_BYTES {
            return Err(wasmtime::Error::msg("output budget exhausted"));
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

    fn execute(
        &self,
        bytes: &[u8],
        capabilities: &[CapabilityGrant],
        ceiling: Option<&[CapabilityGrant]>,
        observer: Option<RuntimeObserver>,
    ) -> Result<RunOutput, RuntimeError> {
        let module = self.validate_module(bytes)?;
        let mut execution = execution::Execution::new(&self.engine, capabilities, ceiling)?;
        let result = (|| {
            let instance = execution.instantiate(&module)?;
            let store = &mut execution.store;
            if let Some(start) = instance.get_func(&mut *store, "_start") {
                start.call(&mut *store, &[], &mut [])?;
            }
            exceptions::finish(&instance, store)?;
            Ok::<(), RuntimeError>(())
        })();
        let store = &mut execution.store;
        let exited = result.is_err();
        if let Err(error) = result {
            if let RuntimeError::Execution(source) = &error
                && let Some(exit) = source.downcast_ref::<GuestExit>()
            {
                store.data_mut().output.exit_code = exit.0;
            } else {
                return Err(error);
            }
        }
        let output = std::mem::take(&mut store.data_mut().output);
        drop(execution);
        if !exited && let Some(contract) = http_contract::read(bytes)? {
            return server::run(server::Start {
                engine: self.engine.clone(),
                module,
                contract,
                requested: capabilities,
                ceiling,
                output,
                observer,
            });
        }
        Ok(output)
    }
}

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
mod object_values_tests;

#[cfg(test)]
mod array_enumeration_tests;

#[cfg(test)]
mod catch_pattern_tests;

#[cfg(test)]
mod string_range_tests;

#[cfg(test)]
mod string_search_tests;
