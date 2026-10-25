#![forbid(unsafe_code)]
//! Capability-checked Wasmtime execution. Only portable Wasm enters this API.
mod host;
mod http;
mod policy;
mod validate;

use std::time::{Duration, Instant};
use varta_common::{
    CapabilityGrant,
    abi::{self, HostStatus},
};
use wasmtime::{Config, Engine, Module, Store, StoreLimits, StoreLimitsBuilder};

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
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
        varta_common::validate_grants(capabilities)?;
        if let Some(ceiling) = ceiling {
            varta_common::validate_grants(ceiling)?;
        }
        let module = self.validate_module(bytes)?;
        let mut store = Store::new(
            &self.engine,
            RuntimeContext {
                policy: match ceiling {
                    Some(ceiling) => policy::Policy::with_ceiling(capabilities, Some(ceiling)),
                    None => policy::Policy::new(capabilities),
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
        let _deadline = Deadline::new(self.engine.clone())?;
        let mut linker = wasmtime::Linker::new(&self.engine);
        host::register(&mut linker)?;
        let result = (|| {
            let instance = linker.instantiate(&mut store, &module)?;
            if let Some(start) = instance.get_func(&mut store, "_start") {
                start.call(&mut store, &[], &mut [])?;
            }
            Ok::<(), wasmtime::Error>(())
        })();
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

struct Deadline {
    stop: std::sync::mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Deadline {
    fn new(engine: Engine) -> Result<Self, wasmtime::Error> {
        let (stop, wait) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("varta-deadline".into())
            .spawn(move || {
                if matches!(
                    wait.recv_timeout(Duration::from_secs(abi::DEADLINE_SECONDS)),
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

#[cfg(test)]
mod result_tests;
#[cfg(test)]
mod string_operation_tests;
#[cfg(test)]
mod string_tests;
#[cfg(test)]
mod tests;
