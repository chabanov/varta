//! Pure UTF-16 intrinsics over the shared `varta-intrinsics` implementation
//! (also linked into v38 guests). Resource conversion lives in the
//! privileged host path; logging stays a host effect.
use crate::{RuntimeContext, host};
use varta_common::{HostOperation as Host, abi};
use varta_intrinsics::string;
use wasmtime::{Caller, Extern, Linker};

fn fatal(error: varta_intrinsics::IntrinsicError) -> wasmtime::Error {
    wasmtime::Error::msg(error.to_string())
}
fn read(
    caller: &mut Caller<'_, RuntimeContext>,
    ptr: i32,
    len: i32,
) -> Result<Vec<u8>, wasmtime::Error> {
    // V38 fair fuel: the argument's bytes (a negative length is rejected
    // below).
    crate::fair_fuel::bytes(caller, usize::try_from(len).unwrap_or(0))?;
    let Some(Extern::Memory(memory)) = caller.get_export(varta_common::abi::MEMORY_EXPORT) else {
        return Err(wasmtime::Error::msg("missing memory"));
    };
    // The host copies: the result transfer needs the caller mutably.
    string::read(memory.data(&*caller), ptr, len, abi::MAX_STRING_BYTES)
        .map(std::borrow::Cow::into_owned)
        .map_err(fatal)
}
pub(super) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        Host::LogString16.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| {
            caller.data_mut().tick()?;
            let units = read(&mut caller, ptr, len)?;
            // Diagnostic output is Unicode text, never a round-trippable resource identifier.
            caller.data_mut().emit(format!(
                "varta log: {}",
                String::from_utf16_lossy(&string::units(&units).collect::<Vec<_>>())
            ))
        },
    )?;
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        Host::StringCharAt16.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32, position: f64| {
            caller.data_mut().tick()?;
            let units = read(&mut caller, ptr, len)?;
            host::result_bytes(&mut caller, &string::char_at(&units, position))
                .map_err(|_| wasmtime::Error::msg("UTF-16 result transfer failed"))
        },
    )?;
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        Host::StringCharCodeAt16.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32, position: f64| {
            caller.data_mut().tick()?;
            let units = read(&mut caller, ptr, len)?;
            Ok(string::char_code_at(&units, position))
        },
    )?;
    for (operation, upper) in [
        (Host::StringToUpperCase16, true),
        (Host::StringToLowerCase16, false),
    ] {
        linker.func_wrap(
            varta_common::abi::HOST_MODULE,
            operation.name(),
            move |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| {
                caller.data_mut().tick()?;
                let units = read(&mut caller, ptr, len)?;
                let bytes =
                    string::convert_case16(&units, upper, abi::MAX_STRING_BYTES).map_err(fatal)?;
                host::result_bytes(&mut caller, &bytes)
                    .map_err(|_| wasmtime::Error::msg("UTF-16 result transfer failed"))
            },
        )?;
    }
    Ok(())
}

#[cfg(test)]
use string::convert_case;
#[cfg(test)]
mod tests {
    use super::convert_case;
    fn convert(text: &str, upper: bool) -> String {
        String::from_utf16(&convert_case(
            &text.encode_utf16().collect::<Vec<_>>(),
            upper,
        ))
        .unwrap()
    }
    #[test]
    fn case_conversion_follows_default_unicode_mappings() {
        // Pinned against Node 24: "straße ǆ ﬁ".toUpperCase(), "ΑΣ ΑΣΑ".toLowerCase().
        assert_eq!(convert("straße ǆ ﬁ", true), "STRASSE Ǆ FI");
        assert_eq!(convert("ΑΣ ΑΣΑ", false), "ας ασα");
        assert_eq!(convert("İ", false), "i\u{307}");
        assert_eq!(convert("abc", true), "ABC");
    }
    #[test]
    fn case_conversion_keeps_lone_surrogates() {
        let units = [0x61, 0xD800, 0x62, 0xDC00];
        assert_eq!(convert_case(&units, true), [0x41, 0xD800, 0x42, 0xDC00]);
    }
}
