#![forbid(unsafe_code)]
//! Capability-checked Wasmtime execution. Only portable Wasm enters this API.
#[cfg(test)]
mod contract_tests;
mod contracts;
mod execution;
mod host;
mod http;
#[cfg(test)]
mod invocation_tests;
mod invoke;
mod policy;
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

    pub fn run_wasm_bytes(
        &self,
        bytes: &[u8],
        capabilities: &[CapabilityGrant],
    ) -> Result<RunOutput, RuntimeError> {
        self.execute(bytes, capabilities, None)
    }

    /// Run with the conjunction of package requests and a trusted host ceiling.
    /// An empty ceiling denies every privileged operation; it never means fallback.
    pub fn run_wasm_bytes_with_policy(
        &self,
        bytes: &[u8],
        requested: &[CapabilityGrant],
        ceiling: &[CapabilityGrant],
    ) -> Result<RunOutput, RuntimeError> {
        self.execute(bytes, requested, Some(ceiling))
    }

    fn execute(
        &self,
        bytes: &[u8],
        capabilities: &[CapabilityGrant],
        ceiling: Option<&[CapabilityGrant]>,
    ) -> Result<RunOutput, RuntimeError> {
        let module = self.validate_module(bytes)?;
        let mut execution = execution::Execution::new(&self.engine, capabilities, ceiling)?;
        let result = (|| {
            let instance = execution.instantiate(&module)?;
            let store = &mut execution.store;
            if let Some(start) = instance.get_func(&mut *store, "_start") {
                start.call(&mut *store, &[], &mut [])?;
            }
            Ok::<(), wasmtime::Error>(())
        })();
        let store = &mut execution.store;
        if let Err(error) = result {
            if let Some(exit) = error.downcast_ref::<GuestExit>() {
                store.data_mut().output.exit_code = exit.0;
            } else {
                return Err(error.into());
            }
        }
        Ok(std::mem::take(&mut store.data_mut().output))
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
