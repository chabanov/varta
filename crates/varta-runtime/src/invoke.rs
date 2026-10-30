//! Validated JSON dispatch. Every call owns and drops a fresh bounded instance.
use crate::{GuestExit, RuntimeContext, RuntimeError, VartaRuntime, execution::Execution};
use varta_common::{
    CapabilityGrant, ToolInvocationError as Error, ToolInvocationOutput, ToolResult, ToolType,
    ToolValue, abi,
};
use wasmtime::{Instance, Memory, Store, Val};

impl VartaRuntime {
    pub fn invoke_tool(
        &self,
        bytes: &[u8],
        name: &str,
        arguments: &[u8],
        requested: &[CapabilityGrant],
    ) -> Result<ToolInvocationOutput, Error> {
        self.invoke(bytes, name, arguments, requested, None)
    }
    /// Both package requests and the explicit host ceiling constrain every host effect.
    pub fn invoke_tool_with_policy(
        &self,
        bytes: &[u8],
        name: &str,
        arguments: &[u8],
        requested: &[CapabilityGrant],
        ceiling: &[CapabilityGrant],
    ) -> Result<ToolInvocationOutput, Error> {
        self.invoke(bytes, name, arguments, requested, Some(ceiling))
    }
    fn invoke(
        &self,
        bytes: &[u8],
        name: &str,
        arguments: &[u8],
        requested: &[CapabilityGrant],
        ceiling: Option<&[CapabilityGrant]>,
    ) -> Result<ToolInvocationOutput, Error> {
        if !varta_common::valid_tool_name(name) {
            return Err(Error::InvalidArguments("invalid tool name".into()));
        }
        if arguments.len() > abi::MAX_INPUT_BYTES {
            return Err(Error::InvalidArguments(
                "JSON exceeds input byte limit".into(),
            ));
        }
        varta_common::validate_grants(requested)
            .map_err(|e| Error::InvalidPolicy(e.to_string()))?;
        if let Some(ceiling) = ceiling {
            varta_common::validate_grants(ceiling)
                .map_err(|e| Error::InvalidPolicy(e.to_string()))?;
        }
        let (module, catalog) = self
            .prepare_module_and_tools(bytes)
            .map_err(|e| Error::InvalidArtifact(e.to_string()))?;
        let tool = catalog
            .as_ref()
            .and_then(|catalog| catalog.tools.iter().find(|tool| tool.name == name))
            .ok_or_else(|| Error::UnknownTool(name.into()))?;
        let arguments = tool.parse_arguments(arguments)?;
        // No instantiation or guest allocation occurs until every argument is valid.
        let mut execution =
            Execution::new(&self.engine, requested, ceiling).map_err(runtime_error)?;
        let instance = execution
            .instantiate_prepared(&module)
            .map_err(execution_error)?;
        let mut params = Vec::with_capacity(arguments.len());
        for argument in arguments {
            execution.check_deadline().map_err(execution_error)?;
            params.push(match argument {
                ToolValue::Integer(value) => Val::I32(value),
                ToolValue::Boolean(value) => Val::I32(i32::from(value)),
                ToolValue::String(value) => Val::I64(transfer(&mut execution, &instance, &value)?),
                ToolValue::Void => return Err(Error::InvalidArguments("void argument".into())),
            });
        }
        let function = instance
            .get_func(&mut execution.store, &tool.export)
            .ok_or_else(|| Error::InvalidArtifact("missing validated handler".into()))?;
        let mut results = match tool.result {
            ToolType::Void => vec![],
            ToolType::String => vec![Val::I64(0)],
            _ => vec![Val::I32(0)],
        };
        execution.check_deadline().map_err(execution_error)?;
        function
            .call(&mut execution.store, &params, &mut results)
            .map_err(execution_error)?;
        execution.check_deadline().map_err(execution_error)?;
        let value = match (tool.result, results.first()) {
            (ToolType::Void, None) => ToolValue::Void,
            (ToolType::Integer, Some(Val::I32(value))) => ToolValue::Integer(*value),
            (ToolType::Boolean, Some(Val::I32(0))) => ToolValue::Boolean(false),
            (ToolType::Boolean, Some(Val::I32(1))) => ToolValue::Boolean(true),
            (ToolType::String, Some(Val::I64(handle))) => {
                let memory = memory(&mut execution, &instance)?;
                ToolValue::String(string(memory, &execution.store, *handle)?.to_owned())
            }
            _ => return Err(Error::InvalidResult("noncanonical result value".into())),
        };
        let result = ToolResult { value };
        let mut budget = JsonBudget(tool.limits.output_bytes);
        serde_json::to_writer(&mut budget, &result).map_err(|_| {
            Error::InvalidResult("serialized result exceeds output byte limit".into())
        })?;
        execution.check_deadline().map_err(execution_error)?;
        Ok(ToolInvocationOutput {
            result,
            logs: std::mem::take(&mut execution.store.data_mut().output.lines),
        })
    }
}

fn memory(execution: &mut Execution, instance: &Instance) -> Result<Memory, Error> {
    instance
        .get_memory(&mut execution.store, "memory")
        .ok_or_else(|| Error::InvalidResult("missing ABI memory".into()))
}
fn transfer(execution: &mut Execution, instance: &Instance, input: &str) -> Result<i64, Error> {
    let memory = memory(execution, instance)?;
    memory
        .write(&mut execution.store, abi::RESULT_BASE, input.as_bytes())
        .map_err(|_| Error::InvalidResult("invalid ABI input buffer".into()))?;
    let copy = instance
        .get_typed_func::<(i32, i32), i64>(&mut execution.store, varta_common::TOOL_COPY_STRING)
        .map_err(execution_error)?;
    let retain = instance
        .get_typed_func::<i64, i64>(&mut execution.store, varta_common::TOOL_RETAIN_STRING)
        .map_err(execution_error)?;
    let handle = copy
        .call(
            &mut execution.store,
            (abi::RESULT_BASE as i32, input.len() as i32),
        )
        .map_err(execution_error)?;
    if string(memory, &execution.store, handle)? != input {
        return Err(Error::InvalidResult(
            "string transfer changed the input".into(),
        ));
    }
    if retain
        .call(&mut execution.store, handle)
        .map_err(execution_error)?
        != handle
    {
        return Err(Error::InvalidResult(
            "string retention changed the handle".into(),
        ));
    }
    Ok(handle)
}
fn string(memory: Memory, store: &Store<RuntimeContext>, handle: i64) -> Result<&str, Error> {
    let packed = handle as u64;
    let ptr = packed as u32 as usize;
    let len = (packed >> i32::BITS) as usize;
    if len > abi::MAX_STRING_BYTES {
        return Err(Error::InvalidResult("string exceeds byte limit".into()));
    }
    let end = ptr
        .checked_add(len)
        .ok_or_else(|| Error::InvalidResult("string range overflow".into()))?;
    let bytes = memory
        .data(store)
        .get(ptr..end)
        .ok_or_else(|| Error::InvalidResult("string range outside guest memory".into()))?;
    std::str::from_utf8(bytes).map_err(|_| Error::InvalidResult("string is not valid UTF-8".into()))
}
fn execution_error(error: wasmtime::Error) -> Error {
    if let Some(exit) = error.downcast_ref::<GuestExit>() {
        Error::GuestExit(exit.0)
    } else {
        Error::ExecutionFailed(format!("{error:#}"))
    }
}
fn runtime_error(error: RuntimeError) -> Error {
    match error {
        RuntimeError::Uncaught(error) => Error::ExecutionFailed(error.to_string()),
        RuntimeError::Scheduler(error) => Error::ExecutionFailed(error.to_string()),
        RuntimeError::Net(error) => Error::ExecutionFailed(error.to_string()),
        RuntimeError::Resource(error) => Error::ExecutionFailed(error.to_string()),
        RuntimeError::Policy(error) => Error::InvalidPolicy(error.to_string()),
        RuntimeError::Execution(error) => execution_error(error),
        RuntimeError::ToolContract(error) => Error::InvalidArtifact(error.to_string()),
        RuntimeError::Http(error) => Error::InvalidArtifact(error.to_string()),
        RuntimeError::ValueAbi(error) => Error::InvalidArtifact(error.to_string()),
        RuntimeError::Frames(error) => Error::InvalidArtifact(error.to_string()),
        RuntimeError::Language(error) => Error::InvalidArtifact(error.to_string()),
    }
}

// Count encoded bytes without allocating an oversized escaped JSON string.
struct JsonBudget(usize);
impl std::io::Write for JsonBudget {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_sub(bytes.len())
            .ok_or_else(|| std::io::Error::other("JSON output limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
