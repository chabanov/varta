use crate::RuntimeError;
use varta_common::abi;
use wasmtime::{Engine, Module};

pub(crate) fn module(engine: &Engine, bytes: &[u8]) -> Result<Module, RuntimeError> {
    preflight(bytes)?;
    if bytes.len() > abi::MAX_WASM_BYTES || !bytes.starts_with(b"\0asm\x01\0\0\0") {
        return Err(wasmtime::Error::msg("expected a bounded portable Wasm module").into());
    }
    let module = Module::new(engine, bytes)?;
    for import in module.imports() {
        if import.module() != "varta" || varta_common::host_capability(import.name()).is_none() {
            return Err(wasmtime::Error::msg(format!(
                "unknown host import: {}.{}",
                import.module(),
                import.name()
            ))
            .into());
        }
        let wasmtime::ExternType::Func(ty) = import.ty() else {
            return Err(wasmtime::Error::msg("only function imports are supported").into());
        };
        let (params, results) = varta_common::HostOperation::from_name(import.name())
            .ok_or_else(|| error("unknown host operation"))?
            .signature();
        if ty.params().len() != params
            || ty.results().len() != results
            || ty
                .params()
                .chain(ty.results())
                .any(|ty| !matches!(ty, wasmtime::ValType::I32))
        {
            return Err(
                wasmtime::Error::msg(format!("invalid host signature: {}", import.name())).into(),
            );
        }
    }
    if let Some(export) = module.get_export("_start") {
        let wasmtime::ExternType::Func(ty) = export else {
            return Err(wasmtime::Error::msg("_start must be a function").into());
        };
        if ty.params().len() != 0 || ty.results().len() != 0 {
            return Err(wasmtime::Error::msg("_start must have signature () -> ()").into());
        }
    }
    check_exports(&module)?;
    Ok(module)
}

fn error(message: &str) -> RuntimeError {
    wasmtime::Error::msg(message.to_owned()).into()
}

fn preflight(bytes: &[u8]) -> Result<(), RuntimeError> {
    use wasmparser::{Parser, Payload};
    if bytes.len() > abi::MAX_WASM_BYTES {
        return Err(error("Wasm exceeds size limit"));
    }
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|e| error(&e.to_string()))? {
            Payload::TypeSection(reader) => {
                for (count, ty) in reader.into_iter_err_on_gc_types().enumerate() {
                    let ty = ty.map_err(|e| error(&e.to_string()))?;
                    if count >= abi::MAX_WASM_SIGNATURES
                        || ty.params().len() > abi::MAX_WASM_PARAMETERS
                        || ty.results().len() > 2
                    {
                        return Err(error("Wasm signature complexity exceeds limits"));
                    }
                }
            }
            Payload::ImportSection(reader) if reader.count() > 64 => {
                return Err(error("too many imports"));
            }
            Payload::FunctionSection(reader)
                if reader.count() as usize
                    > abi::MAX_USER_FUNCTIONS + abi::MAX_INTERNAL_FUNCTIONS =>
            {
                return Err(error("too many functions"));
            }
            Payload::MemorySection(reader) => {
                if reader.count() > 1 {
                    return Err(error("only one memory is supported"));
                }
                for memory in reader {
                    let memory = memory.map_err(|e| error(&e.to_string()))?;
                    if memory.memory64
                        || memory.shared
                        || memory.initial > (abi::MAX_MEMORY_BYTES / abi::RESULT_BASE) as u64
                    {
                        return Err(error("unsupported memory type or initial size"));
                    }
                }
            }
            Payload::TableSection(reader) => {
                if reader.count() > 1 {
                    return Err(error("only one table is supported"));
                }
                for table in reader {
                    let table = table.map_err(|e| error(&e.to_string()))?;
                    if table.ty.initial > 1024 {
                        return Err(error("table exceeds element budget"));
                    }
                }
            }
            Payload::GlobalSection(reader) if reader.count() > 128 => {
                return Err(error("too many globals"));
            }
            Payload::CodeSectionEntry(body) => {
                if body.range().len() > 65_536 {
                    return Err(error("function body exceeds compilation budget"));
                }
            }
            Payload::CustomSection(section)
                if section.name() == "varta.abi"
                    && section.data() != abi::VERSION.to_le_bytes() =>
            {
                return Err(error("unsupported Wasm ABI version"));
            }
            _ => {}
        }
    }
    Ok(())
}

fn check_exports(module: &Module) -> Result<(), RuntimeError> {
    use wasmtime::{ExternType, Mutability, ValType};
    let mut needs_memory = false;
    let mut needs_result = false;
    for import in module.imports() {
        needs_memory |= !matches!(import.name(), "log_i32" | "process_exit" | "time_now");
        needs_result |= matches!(import.name(), "env_get" | "fs_read" | "net_get");
    }
    if needs_memory {
        match module.get_export("memory") {
            Some(ExternType::Memory(memory)) if !memory.is_64() && !memory.is_shared() => {}
            _ => {
                return Err(error(
                    "host pointer imports require an exported 32-bit memory",
                ));
            }
        }
    }
    for name in [abi::RESULT_LEN, abi::RESULT_STATUS] {
        match module.get_export(name) {
            Some(ExternType::Global(global))
                if matches!(global.content(), ValType::I32)
                    && global.mutability() == Mutability::Var => {}
            None if !needs_result => {}
            _ => {
                return Err(error(
                    "ABI result globals must be exported mutable i32 values",
                ));
            }
        }
    }
    Ok(())
}
