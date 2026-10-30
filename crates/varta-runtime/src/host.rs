use crate::{GuestExit, HostFailure, RuntimeContext};
use varta_common::HostOperation;
use varta_common::abi::{self, HostStatus};
use wasmtime::{Caller, Extern, Linker, Val};

fn global(
    caller: &mut Caller<'_, RuntimeContext>,
    name: &str,
    value: i32,
) -> Result<(), wasmtime::Error> {
    if let Some(Extern::Global(global)) = caller.get_export(name) {
        global.set(caller, Val::I32(value))?;
    }
    Ok(())
}

pub(super) fn reset(caller: &mut Caller<'_, RuntimeContext>) -> Result<(), wasmtime::Error> {
    caller.data_mut().tick()?;
    global(caller, abi::RESULT_LEN, 0)?;
    global(caller, abi::RESULT_STATUS, HostStatus::Ok as i32)
}

pub(crate) fn input(
    caller: &mut Caller<'_, RuntimeContext>,
    ptr: i32,
    len: i32,
    limit: usize,
    utf16: bool,
) -> Result<String, HostFailure> {
    if ptr < 0 || len < 0 || len as usize > limit {
        return Err(HostFailure::invalid());
    }
    // V38 fair fuel: the argument's bytes, before any effect.
    if crate::fair_fuel::bytes(caller, len as usize).is_err() {
        return Err(HostFailure::exhausted());
    }
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return Err(HostFailure::invalid());
    };
    let bytes = memory
        .data(&*caller)
        .get(ptr as usize..(ptr as usize).saturating_add(len as usize))
        .ok_or_else(HostFailure::invalid)?;
    if utf16 {
        let text = varta_common::text::resource(bytes).map_err(|_| HostFailure::invalid())?;
        if text.len() > limit {
            return Err(HostFailure::invalid());
        }
        return Ok(text);
    }
    std::str::from_utf8(bytes)
        .map(ToOwned::to_owned)
        .map_err(|_| HostFailure::invalid())
}

pub(super) fn failure(
    caller: &mut Caller<'_, RuntimeContext>,
    error: HostFailure,
) -> Result<(), wasmtime::Error> {
    if error.exhausted {
        return Err(wasmtime::Error::new(wasmtime::Trap::OutOfFuel));
    }
    global(caller, abi::RESULT_LEN, 0)?;
    global(caller, abi::RESULT_STATUS, error.status as i32)?;
    caller
        .data_mut()
        .emit(format!("varta log: {}", error.message))
}

pub(super) fn result(
    caller: &mut Caller<'_, RuntimeContext>,
    bytes: &[u8],
) -> Result<i32, HostFailure> {
    if bytes.len() > abi::MAX_RESULT_BYTES {
        return Err(HostFailure::too_large());
    }
    // Host results are UTF-8 strings in this ABI, including files and HTTP bodies.
    std::str::from_utf8(bytes).map_err(|_| HostFailure::invalid())?;
    result_bytes(caller, bytes)
}
/// Convert well-formed host UTF-8 exactly once; the shared buffer holds UTF-16LE bytes.
pub(super) fn result16(
    caller: &mut Caller<'_, RuntimeContext>,
    bytes: &[u8],
) -> Result<i32, HostFailure> {
    if bytes.len() > abi::MAX_RESULT_BYTES {
        return Err(HostFailure::too_large());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| HostFailure::invalid())?;
    result_bytes(caller, &varta_common::text::encode(text))
}
pub(super) fn result_bytes(
    caller: &mut Caller<'_, RuntimeContext>,
    bytes: &[u8],
) -> Result<i32, HostFailure> {
    if bytes.len() > abi::MAX_RESULT_BYTES {
        return Err(HostFailure::too_large());
    }
    // V38 fair fuel: the result's bytes, before the guest sees them.
    if crate::fair_fuel::bytes(caller, bytes.len()).is_err() {
        return Err(HostFailure::exhausted());
    }
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return Err(HostFailure::invalid());
    };
    let Some(Extern::Global(length)) = caller.get_export(abi::RESULT_LEN) else {
        return Err(HostFailure::invalid());
    };
    let desired = abi::RESULT_BASE + bytes.len();
    if desired > memory.data_size(&*caller) {
        let pages = (desired - memory.data_size(&*caller)).div_ceil(65_536);
        memory
            .grow(&mut *caller, pages as u64)
            .map_err(HostFailure::io)?;
    }
    memory
        .write(&mut *caller, abi::RESULT_BASE, bytes)
        .map_err(HostFailure::io)?;
    length
        .set(caller, Val::I32(bytes.len() as i32))
        .map_err(HostFailure::io)?;
    Ok(abi::RESULT_BASE as i32)
}

/// The `process.env` snapshot of v38 (`env_snapshot`): a JSON object of the
/// set variables whose names the `env.get` grants list exactly, in the
/// package request and in the host ceiling (each name also passes the check
/// `env_get` makes), in name order. A wildcard pattern lists nothing: the
/// environment is never enumerated, only granted names are read. Values that
/// are not Unicode are skipped.
fn env_snapshot(policy: &crate::policy::Policy) -> String {
    const ENV_GET: &str = "env.get";
    let mut variables = serde_json::Map::new();
    for name in policy.exact_names(ENV_GET) {
        if !policy.env(ENV_GET, &name) {
            continue;
        }
        if let Some(value) = std::env::var_os(&name).and_then(|value| value.into_string().ok()) {
            variables.insert(name, serde_json::Value::String(value));
        }
    }
    serde_json::Value::Object(variables).to_string()
}

pub(crate) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    crate::number::register(linker)?;
    crate::bigint::register(linker)?;
    crate::native_math::register(linker)?;
    crate::random::register(linker)?;
    crate::scheduler::register(linker)?;
    crate::net::register(linker)?;
    crate::kv::register(linker)?;
    crate::events::register(linker)?;
    crate::view::register(linker)?;
    crate::primitive_string::register(linker)?;
    crate::primitive_number::register(linker)?;
    crate::number_radix::register(linker)?;
    crate::number_format::register(linker)?;
    crate::number_parse::register(linker)?;
    crate::utf16::register(linker)?;
    crate::unions::register(linker)?;
    crate::references::register(linker)?;
    // Internal feature negotiation: binding is authorized by the service runner.
    linker.func_wrap(
        "varta",
        varta_common::HostOperation::HttpService.name(),
        |mut caller: Caller<'_, RuntimeContext>| -> Result<(), wasmtime::Error> {
            caller.data_mut().tick()
        },
    )?;
    linker.func_wrap(
        "varta",
        varta_common::HostOperation::HttpStop.name(),
        |mut caller: Caller<'_, RuntimeContext>| -> Result<(), wasmtime::Error> {
            reset(&mut caller)?;
            let Some((endpoint, control)) = caller.data().http_control.clone() else {
                return failure(&mut caller, HostFailure::invalid());
            };
            if !caller.data().policy.listen(&endpoint) {
                return failure(&mut caller, HostFailure::denied("net.listen", &endpoint));
            }
            control.shutdown();
            Ok(())
        },
    )?;
    crate::json::register(linker)?;
    crate::json_parse::register(linker)?;
    linker.func_wrap(
        "varta",
        "log_i32",
        |mut caller: Caller<'_, RuntimeContext>, value: i32| {
            caller.data_mut().tick()?;
            caller.data_mut().emit(format!("varta log: {value}"))
        },
    )?;
    linker.func_wrap(
        "varta",
        "log_string",
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| {
            caller.data_mut().tick()?;
            match input(&mut caller, ptr, len, abi::MAX_RESULT_BYTES, false) {
                Ok(text) => caller.data_mut().emit(format!("varta log: {text}")),
                Err(error) => failure(&mut caller, error),
            }
        },
    )?;
    linker.func_wrap(
        "varta",
        "time_now",
        |mut caller: Caller<'_, RuntimeContext>| -> Result<i32, wasmtime::Error> {
            reset(&mut caller)?;
            // Decided by the permission broker before the clock is read; a
            // denial is the recoverable failed status of every profile.
            let context = caller.data_mut();
            if let crate::permission::Decision::Denied = context.broker.clock(&context.policy) {
                let capability = crate::permission::CLOCK_CAPABILITY;
                failure(&mut caller, HostFailure::denied(capability, "clock"))?;
                return Ok(0);
            }
            let seconds = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(wasmtime::Error::new)?
                .as_secs();
            match i32::try_from(seconds) {
                Ok(seconds) => Ok(seconds),
                Err(_) => {
                    failure(&mut caller, HostFailure::invalid())?;
                    Ok(0)
                }
            }
        },
    )?;
    linker.func_wrap(
        "varta",
        "process_exit",
        |mut caller: Caller<'_, RuntimeContext>, code: i32| -> Result<(), wasmtime::Error> {
            reset(&mut caller)?;
            if !caller.data().policy.enabled("process.exit") {
                return failure(&mut caller, HostFailure::denied("process.exit", "process"));
            }
            Err(wasmtime::Error::new(GuestExit(code)))
        },
    )?;
    for (name, id, utf16) in [
        ("env_has", "env.has", false),
        ("env_get", "env.get", false),
        ("env_has16", "env.has", true),
        ("env_get16", "env.get", true),
    ] {
        linker.func_wrap(
            "varta",
            name,
            move |mut caller: Caller<'_, RuntimeContext>,
                  ptr: i32,
                  len: i32|
                  -> Result<i32, wasmtime::Error> {
                reset(&mut caller)?;
                let value = (|| {
                    let name = input(&mut caller, ptr, len, abi::MAX_INPUT_BYTES, utf16)?;
                    if !caller.data().policy.env(id, &name) {
                        return Err(HostFailure::denied(id, &name));
                    }
                    if id == "env.has" {
                        return Ok(std::env::var_os(&name).is_some() as i32);
                    }
                    let value = std::env::var(&name).map_err(HostFailure::io)?;
                    if utf16 {
                        result16(&mut caller, value.as_bytes())
                    } else {
                        result(&mut caller, value.as_bytes())
                    }
                })();
                match value {
                    Ok(value) => Ok(value),
                    Err(error) => {
                        failure(&mut caller, error)?;
                        Ok(0)
                    }
                }
            },
        )?;
    }
    linker.func_wrap(
        "varta",
        HostOperation::EnvSnapshot.name(),
        |mut caller: Caller<'_, RuntimeContext>,
         ptr: i32,
         len: i32|
         -> Result<i32, wasmtime::Error> {
            reset(&mut caller)?;
            const ENV_GET: &str = "env.get";
            let text = (|| {
                let name = input(&mut caller, ptr, len, abi::MAX_INPUT_BYTES, true)?;
                if name.is_empty() {
                    return Ok(env_snapshot(&caller.data().policy));
                }
                // One named variable, checked exactly as `env_get` checks it;
                // an unset variable is absent without a failure.
                if !caller.data().policy.env(ENV_GET, &name) {
                    return Err(HostFailure::denied(ENV_GET, &name));
                }
                let mut variables = serde_json::Map::new();
                if let Some(value) = std::env::var_os(&name).and_then(|v| v.into_string().ok()) {
                    variables.insert(name, serde_json::Value::String(value));
                }
                Ok(serde_json::Value::Object(variables).to_string())
            })();
            let text = match text {
                Ok(text) => text,
                Err(error) => {
                    failure(&mut caller, error)?;
                    return Ok(0);
                }
            };
            match result16(&mut caller, text.as_bytes()) {
                Ok(value) => Ok(value),
                Err(error) => {
                    failure(&mut caller, error)?;
                    Ok(0)
                }
            }
        },
    )?;
    for (name, utf16) in [("env_print", false), ("env_print16", true)] {
        linker.func_wrap(
            "varta",
            name,
            move |mut caller: Caller<'_, RuntimeContext>,
                  ptr: i32,
                  len: i32|
                  -> Result<(), wasmtime::Error> {
                reset(&mut caller)?;
                let value = (|| {
                    let name = input(&mut caller, ptr, len, abi::MAX_INPUT_BYTES, utf16)?;
                    if !caller.data().policy.env("env.print", &name) {
                        return Err(HostFailure::denied("env.print", &name));
                    }
                    let value = std::env::var(&name).map_err(HostFailure::io)?;
                    if value.len() > abi::MAX_RESULT_BYTES {
                        return Err(HostFailure::too_large());
                    }
                    Ok(format!("varta env: {name}={value}"))
                })();
                match value {
                    Ok(line) => caller.data_mut().emit(line),
                    Err(error) => failure(&mut caller, error),
                }
            },
        )?;
    }
    for (name, id, utf16) in [
        ("fs_read", "fs.read", false),
        ("net_get", "net.get", false),
        ("net_fetch", "net.fetch", false),
        ("fs_read16", "fs.read", true),
        ("net_get16", "net.get", true),
        ("net_fetch16", "net.fetch", true),
    ] {
        linker.func_wrap(
            "varta",
            name,
            move |mut caller: Caller<'_, RuntimeContext>,
                  ptr: i32,
                  len: i32|
                  -> Result<i32, wasmtime::Error> {
                reset(&mut caller)?;
                let value = (|| {
                    let target = input(&mut caller, ptr, len, abi::MAX_INPUT_BYTES, utf16)?;
                    let bytes = if id == "fs.read" {
                        caller.data().policy.read(id, &target)?
                    } else {
                        let url = caller
                            .data()
                            .policy
                            .network(id, &target)
                            .ok_or_else(|| HostFailure::denied(id, &target))?;
                        let remaining = caller
                            .data()
                            .deadline
                            .saturating_duration_since(std::time::Instant::now());
                        let (status, body) = crate::http::fetch(&url, remaining, id == "net.get")?;
                        if id == "net.fetch" {
                            return Ok(status);
                        }
                        body
                    };
                    if utf16 {
                        result16(&mut caller, &bytes)
                    } else {
                        result(&mut caller, &bytes)
                    }
                })();
                match value {
                    Ok(value) => Ok(value),
                    Err(error) => {
                        failure(&mut caller, error)?;
                        Ok(if id == "net.fetch" { -1 } else { 0 })
                    }
                }
            },
        )?;
    }
    for (name, utf16) in [("fs_print", false), ("fs_print16", true)] {
        linker.func_wrap(
            "varta",
            name,
            move |mut caller: Caller<'_, RuntimeContext>,
                  ptr: i32,
                  len: i32|
                  -> Result<(), wasmtime::Error> {
                reset(&mut caller)?;
                let value = (|| {
                    let path = input(&mut caller, ptr, len, abi::MAX_INPUT_BYTES, utf16)?;
                    let bytes = caller.data().policy.read("fs.print", &path)?;
                    String::from_utf8(bytes).map_err(|_| HostFailure::invalid())
                })();
                match value {
                    Ok(text) => caller.data_mut().emit(text),
                    Err(error) => failure(&mut caller, error),
                }
            },
        )?;
    }
    Ok(())
}
