//! Pure UTF-16 intrinsics. Resource conversion lives in the privileged host path.
use crate::{RuntimeContext, host};
use varta_common::{HostOperation as Host, abi, text};
use wasmtime::{Caller, Extern, Linker};

fn read(
    caller: &mut Caller<'_, RuntimeContext>,
    ptr: i32,
    len: i32,
) -> Result<Vec<u16>, wasmtime::Error> {
    if ptr < 0 || len < 0 || len as usize > abi::MAX_STRING_BYTES {
        return Err(wasmtime::Error::msg("invalid UTF-16 range"));
    }
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return Err(wasmtime::Error::msg("missing memory"));
    };
    let data = memory
        .data(&*caller)
        .get(ptr as usize..(ptr as usize).saturating_add(len as usize))
        .ok_or_else(|| wasmtime::Error::msg("UTF-16 range outside memory"))?;
    text::units(data).map_err(wasmtime::Error::new)
}
fn index(value: f64, length: usize) -> Option<usize> {
    let value = if value.is_nan() { 0.0 } else { value.trunc() };
    if value < 0.0 || value >= length as f64 {
        None
    } else {
        Some(value as usize)
    }
}
pub(super) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "varta",
        Host::LogString16.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| {
            caller.data_mut().tick()?;
            let units = read(&mut caller, ptr, len)?;
            // Diagnostic output is Unicode text, never a round-trippable resource identifier.
            caller
                .data_mut()
                .emit(format!("varta log: {}", String::from_utf16_lossy(&units)))
        },
    )?;
    linker.func_wrap(
        "varta",
        Host::StringCharAt16.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32, position: f64| {
            caller.data_mut().tick()?;
            let units = read(&mut caller, ptr, len)?;
            let bytes = index(position, units.len())
                .map(|i| units[i].to_le_bytes().to_vec())
                .unwrap_or_default();
            host::result_bytes(&mut caller, &bytes)
                .map_err(|_| wasmtime::Error::msg("UTF-16 result transfer failed"))
        },
    )?;
    linker.func_wrap(
        "varta",
        Host::StringCharCodeAt16.name(),
        |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32, position: f64| {
            caller.data_mut().tick()?;
            let units = read(&mut caller, ptr, len)?;
            Ok(index(position, units.len()).map_or(f64::NAN, |i| f64::from(units[i])))
        },
    )?;
    for (operation, upper) in [
        (Host::StringToUpperCase16, true),
        (Host::StringToLowerCase16, false),
    ] {
        linker.func_wrap(
            "varta",
            operation.name(),
            move |mut caller: Caller<'_, RuntimeContext>, ptr: i32, len: i32| {
                caller.data_mut().tick()?;
                let units = read(&mut caller, ptr, len)?;
                let bytes: Vec<u8> = convert_case(&units, upper)
                    .into_iter()
                    .flat_map(u16::to_le_bytes)
                    .collect();
                if bytes.len() > abi::MAX_STRING_BYTES {
                    return Err(wasmtime::Error::msg(
                        "case conversion exceeds the string limit",
                    ));
                }
                host::result_bytes(&mut caller, &bytes)
                    .map_err(|_| wasmtime::Error::msg("UTF-16 result transfer failed"))
            },
        )?;
    }
    Ok(())
}

/// Default Unicode case conversion (`String.prototype.toUpperCase` and
/// `toLowerCase`, no locale): every run of well-formed UTF-16 is mapped as a
/// whole, so context rules such as the final sigma see the run; lone
/// surrogates are copied unchanged, as in ECMAScript.
fn convert_case(units: &[u16], upper: bool) -> Vec<u16> {
    let mut output = Vec::with_capacity(units.len());
    let mut run = String::new();
    let flush = |run: &mut String, output: &mut Vec<u16>| {
        let mapped = if upper {
            run.to_uppercase()
        } else {
            run.to_lowercase()
        };
        output.extend(mapped.encode_utf16());
        run.clear();
    };
    for decoded in char::decode_utf16(units.iter().copied()) {
        match decoded {
            Ok(character) => run.push(character),
            Err(lone) => {
                flush(&mut run, &mut output);
                output.push(lone.unpaired_surrogate());
            }
        }
    }
    flush(&mut run, &mut output);
    output
}

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
