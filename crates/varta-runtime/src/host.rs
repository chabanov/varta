use crate::{GuestExit, HostFailure, RuntimeContext};
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

fn reset(caller: &mut Caller<'_, RuntimeContext>) -> Result<(), wasmtime::Error> {
    caller.data_mut().tick()?;
    global(caller, abi::RESULT_LEN, 0)?;
    global(caller, abi::RESULT_STATUS, HostStatus::Ok as i32)
}

fn input(
    caller: &mut Caller<'_, RuntimeContext>,
    ptr: i32,
    len: i32,
    limit: usize,
) -> Result<String, HostFailure> {
    if ptr < 0 || len < 0 || len as usize > limit {
        return Err(HostFailure::invalid());
    }
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return Err(HostFailure::invalid());
    };
    let bytes = memory
        .data(&*caller)
        .get(ptr as usize..(ptr as usize).saturating_add(len as usize))
        .ok_or_else(HostFailure::invalid)?;
    std::str::from_utf8(bytes)
        .map(ToOwned::to_owned)
        .map_err(|_| HostFailure::invalid())
}

fn failure(
    caller: &mut Caller<'_, RuntimeContext>,
    error: HostFailure,
) -> Result<(), wasmtime::Error> {
    global(caller, abi::RESULT_LEN, 0)?;
    global(caller, abi::RESULT_STATUS, error.status as i32)?;
    caller
        .data_mut()
        .emit(format!("varta log: {}", error.message))
}

fn result(caller: &mut Caller<'_, RuntimeContext>, bytes: &[u8]) -> Result<i32, HostFailure> {
    if bytes.len() > abi::MAX_RESULT_BYTES {
        return Err(HostFailure::too_large());
    }
    // Host results are UTF-8 strings in this ABI, including files and HTTP bodies.
    std::str::from_utf8(bytes).map_err(|_| HostFailure::invalid())?;
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

pub(crate) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
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
            match input(&mut caller, ptr, len, abi::MAX_RESULT_BYTES) {
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
            if !caller.data().policy.enabled("time.now") {
                failure(&mut caller, HostFailure::denied("time.now", "clock"))?;
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
    for (name, id) in [("env_has", "env.has"), ("env_get", "env.get")] {
        linker.func_wrap(
            "varta",
            name,
            move |mut caller: Caller<'_, RuntimeContext>,
                  ptr: i32,
                  len: i32|
                  -> Result<i32, wasmtime::Error> {
                reset(&mut caller)?;
                let value = (|| {
                    let name = input(&mut caller, ptr, len, abi::MAX_INPUT_BYTES)?;
                    if !caller.data().policy.env(id, &name) {
                        return Err(HostFailure::denied(id, &name));
                    }
                    if id == "env.has" {
                        return Ok(std::env::var_os(&name).is_some() as i32);
                    }
                    let value = std::env::var(&name).map_err(HostFailure::io)?;
                    result(&mut caller, value.as_bytes())
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
        "env_print",
        |mut caller: Caller<'_, RuntimeContext>,
         ptr: i32,
         len: i32|
         -> Result<(), wasmtime::Error> {
            reset(&mut caller)?;
            let value = (|| {
                let name = input(&mut caller, ptr, len, abi::MAX_INPUT_BYTES)?;
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
    for (name, id) in [
        ("fs_read", "fs.read"),
        ("net_get", "net.get"),
        ("net_fetch", "net.fetch"),
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
                    let target = input(&mut caller, ptr, len, abi::MAX_INPUT_BYTES)?;
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
                    result(&mut caller, &bytes)
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
    linker.func_wrap(
        "varta",
        "fs_print",
        |mut caller: Caller<'_, RuntimeContext>,
         ptr: i32,
         len: i32|
         -> Result<(), wasmtime::Error> {
            reset(&mut caller)?;
            let value = (|| {
                let path = input(&mut caller, ptr, len, abi::MAX_INPUT_BYTES)?;
                let bytes = caller.data().policy.read("fs.print", &path)?;
                String::from_utf8(bytes).map_err(|_| HostFailure::invalid())
            })();
            match value {
                Ok(text) => caller.data_mut().emit(text),
                Err(error) => failure(&mut caller, error),
            }
        },
    )?;
    Ok(())
}
