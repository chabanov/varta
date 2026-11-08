use super::*;
use crate::RuntimeContext;
use varta_common::{HostOperation, session as contract};
use wasmtime::{Caller, Linker};
fn operation(
    caller: &mut Caller<'_, RuntimeContext>,
    operation: HostOperation,
    secret: Option<String>,
) -> Result<i32, wasmtime::Error> {
    crate::host::reset(caller)?;
    let id = operation
        .capability()
        .ok_or_else(|| wasmtime::Error::msg(INVALID))?;
    let context = caller.data_mut();
    let decision = context.broker.session(&context.policy, id);
    let text = if !matches!(decision, crate::permission::Decision::Granted(())) {
        contract::RESULT_DENIED.into()
    } else if let Some(session) = context.http_session.as_mut() {
        let result = match operation {
            HostOperation::HttpSessionRead => session.get().map(|v| {
                v.map_or_else(
                    || contract::RESULT_ANONYMOUS.into(),
                    |s| format!("{}{s}", contract::RESULT_OK),
                )
            }),
            HostOperation::HttpSessionWrite => session
                .set(secret.unwrap_or_default())
                .map(|()| contract::RESULT_OK.into()),
            HostOperation::HttpSessionClear => session.clear().map(|()| contract::RESULT_OK.into()),
            _ => Err(INVALID),
        };
        // Deliberately no secret, token, supplied header, or URL in errors.
        result.unwrap_or_else(|_| contract::RESULT_INVALID.into())
    } else {
        contract::RESULT_INVALID.into()
    };
    crate::host::result16(caller, text.as_bytes()).map_err(|e| e.into_trap(wasmtime::Error::msg))
}
pub(crate) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    for op in [
        HostOperation::HttpSessionRead,
        HostOperation::HttpSessionClear,
    ] {
        linker.func_wrap(
            varta_common::abi::HOST_MODULE,
            op.name(),
            move |mut caller: Caller<'_, RuntimeContext>| operation(&mut caller, op, None),
        )?;
    }
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::HttpSessionWrite.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| {
            let secret =
                crate::host::input(&mut caller, ptr, len, contract::MAX_SECRET_BYTES * 2, true)
                    .map_err(|e| e.into_trap(wasmtime::Error::msg))?;
            operation(&mut caller, HostOperation::HttpSessionWrite, Some(secret))
        },
    )?;
    // Native clients deliberately have no ambient browser cookie channel.
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::SessionRequestStart.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| {
            crate::host::reset(&mut caller)?;
            let _ = crate::host::input(
                &mut caller,
                ptr,
                len,
                varta_common::net::MAX_REQUEST_BYTES,
                true,
            )
            .map_err(|e| e.into_trap(wasmtime::Error::msg))?;
            caller.data_mut().net.refused_session()
        },
    )?;
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::SessionReset.name(),
        |mut caller: Caller<'_, RuntimeContext>| crate::host::reset(&mut caller),
    )?;
    Ok(())
}
