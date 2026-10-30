//! Versioned pure operations over primitive/reference cells. Shallow validation
//! precedes observation; diagnostic JSON traversals retain all serializer limits.
//!
//! The readers other than logging are `varta_intrinsics::value`, which v38
//! links into the guest for its value18 readers; these host imports of
//! value layouts 8 to 17 call the same code.
use crate::{RuntimeContext, host, number};
use varta_common::{
    HostOperation as Host,
    value::{ReferenceBox, ValueKind as Kind},
};
use varta_intrinsics::value::{self as shared, ValueError};
use wasmtime::{Caller, Extern, Linker};

/// A fatal reader failure, with the host's diagnostics.
fn fatal(error: ValueError) -> wasmtime::Error {
    match error {
        ValueError::Abi(error) => wasmtime::Error::new(error),
        ValueError::BigInt(error) => wasmtime::Error::new(error),
        ValueError::Kind => wasmtime::Error::msg("invalid value kind"),
        ValueError::Class => wasmtime::Error::msg("invalid error class"),
    }
}
fn data<'a>(caller: &'a mut Caller<'_, RuntimeContext>) -> Result<&'a [u8], wasmtime::Error> {
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return Err(wasmtime::Error::msg("missing value memory"));
    };
    Ok(memory.data(&*caller))
}
fn read(
    caller: &mut Caller<'_, RuntimeContext>,
    value: i64,
    version: u32,
) -> Result<ReferenceBox, wasmtime::Error> {
    let module = caller.data().module;
    ReferenceBox::read_in(data(caller)?, value as u64, version, module)
        .map_err(wasmtime::Error::new)
}
fn text(bytes: &[u8], value: ReferenceBox) -> &[u8] {
    // The checked cell guarantees the complete even-length range.
    &bytes[value.payload as u32 as usize
        ..value.payload as u32 as usize + (value.payload >> i32::BITS) as usize]
}
fn register_log(
    linker: &mut Linker<RuntimeContext>,
    version: u32,
    log: Host,
) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "varta",
        log.name(),
        move |mut caller: Caller<'_, RuntimeContext>, value: i64| {
            caller.data_mut().tick()?;
            let value = read(&mut caller, value, version)?;
            let display = match value.kind {
                Kind::BigInt => crate::bigint::native_work(&mut caller, |data, budget| {
                    let text = crate::bigint::integer(data, value.payload as i64, budget)?
                        .to_string_radix(10, budget)?;
                    budget.consume_work(text.len() as u64)?;
                    Ok(text)
                })?
                .map_err(wasmtime::Error::new)?,
                Kind::Null => "null".into(),
                Kind::Undefined => "undefined".into(),
                Kind::Boolean => value.payload.to_string(),
                Kind::Float64 => {
                    let value = f64::from_bits(value.payload);
                    if matches!(
                        log,
                        Host::ValueLogV7
                            | Host::ValueLogV8
                            | Host::ValueLogV9
                            | Host::ValueLogV10
                            | Host::ValueLogV11
                            | Host::ValueLogV12
                    ) {
                        number::canonical_display(value)
                    } else {
                        number::display(value)
                    }
                }
                Kind::String => String::from_utf16_lossy(
                    &varta_common::text::units(text(data(&mut caller)?, value))
                        .map_err(wasmtime::Error::new)?,
                ),
                Kind::Error => {
                    let object = varta_common::value::ErrorObject::read_with_version(
                        data(&mut caller)?,
                        value.payload,
                        version,
                    )
                    .map_err(wasmtime::Error::new)?;
                    let message = String::from_utf16_lossy(
                        &varta_common::text::units(text(
                            data(&mut caller)?,
                            ReferenceBox {
                                kind: Kind::String,
                                payload: object.message,
                            },
                        ))
                        .map_err(wasmtime::Error::new)?,
                    );
                    if message.is_empty() {
                        object.class.name().into()
                    } else {
                        format!("{}: {message}", object.class.name())
                    }
                }
                // Value17: a function shows as Node shows an anonymous one.
                Kind::Function => crate::json::FUNCTION_DISPLAY.into(),
                Kind::Record | Kind::Array | Kind::Dictionary | Kind::Promise => {
                    if matches!(
                        version,
                        varta_common::value::BIGINT_VALUE_VERSION
                            | varta_common::value::DYNAMIC_VALUE_VERSION
                            | varta_common::value::PROMISE_VALUE_VERSION
                            | varta_common::value::FUNCTION_FIELD_VALUE_VERSION
                            | varta_common::value::HASHED_DICTIONARY_VALUE_VERSION
                    ) {
                        caller.data().check_budget()?;
                        let fuel = caller.get_fuel()?;
                        let mut budget = varta_bigint::Budget::new(
                            ((varta_common::bigint::MAX_PAYLOAD_BYTES
                                - varta_common::bigint::META_BYTES)
                                * 8) as u64,
                            fuel,
                        );
                        let module = caller.data().module;
                        let result = crate::json::render_diagnostic(
                            data(&mut caller)?,
                            value.kind as u32,
                            value.payload,
                            version,
                            module,
                            &mut budget,
                        );
                        caller.set_fuel(budget.remaining_work())?;
                        caller.data().check_budget()?;
                        String::from_utf8(result?).map_err(wasmtime::Error::new)?
                    } else {
                        let module = caller.data().module;
                        String::from_utf8(crate::json::render_reference_in(
                            data(&mut caller)?,
                            value.kind as u32,
                            value.payload,
                            version,
                            module,
                        )?)
                        .map_err(wasmtime::Error::new)?
                    }
                }
                _ => return Err(wasmtime::Error::msg("invalid value kind")),
            };
            caller.data_mut().emit(format!("varta log: {display}"))
        },
    )?;
    Ok(())
}
pub(super) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    for (version, log, type_of, truthy, is_nullish, is_array, strict_equal, nullish_equal) in [
        (
            varta_common::value::REFERENCE_VALUE_VERSION,
            Host::ValueLog,
            Host::ValueTypeOf,
            Host::ValueTruthy,
            Host::ValueNullish,
            Host::ValueIsArray,
            Host::ValueStrictEqual,
            Host::ValueNullishEqual,
        ),
        (
            varta_common::value::VECTOR_VALUE_VERSION,
            Host::ValueLogV3,
            Host::ValueTypeOfV3,
            Host::ValueTruthyV3,
            Host::ValueNullishV3,
            Host::ValueIsArrayV3,
            Host::ValueStrictEqualV3,
            Host::ValueNullishEqualV3,
        ),
        (
            varta_common::value::ERROR_VALUE_VERSION,
            Host::ValueLogV4,
            Host::ValueTypeOfV4,
            Host::ValueTruthyV4,
            Host::ValueNullishV4,
            Host::ValueIsArrayV4,
            Host::ValueStrictEqualV4,
            Host::ValueNullishEqualV4,
        ),
        (
            varta_common::value::ERROR_FAMILY_VALUE_VERSION,
            Host::ValueLogV5,
            Host::ValueTypeOfV5,
            Host::ValueTruthyV5,
            Host::ValueNullishV5,
            Host::ValueIsArrayV5,
            Host::ValueStrictEqualV5,
            Host::ValueNullishEqualV5,
        ),
        (
            varta_common::value::SPARSE_VALUE_VERSION,
            Host::ValueLogV6,
            Host::ValueTypeOfV6,
            Host::ValueTruthyV6,
            Host::ValueNullishV6,
            Host::ValueIsArrayV6,
            Host::ValueStrictEqualV6,
            Host::ValueNullishEqualV6,
        ),
        (
            varta_common::value::BIGINT_VALUE_VERSION,
            Host::ValueLogV8,
            Host::ValueTypeOfV7,
            Host::ValueTruthyV7,
            Host::ValueNullishV7,
            Host::ValueIsArrayV7,
            Host::ValueStrictEqualV7,
            Host::ValueNullishEqualV7,
        ),
        (
            varta_common::value::DYNAMIC_VALUE_VERSION,
            Host::ValueLogV9,
            Host::ValueTypeOfV8,
            Host::ValueTruthyV8,
            Host::ValueNullishV8,
            Host::ValueIsArrayV8,
            Host::ValueStrictEqualV8,
            Host::ValueNullishEqualV8,
        ),
        (
            varta_common::value::PROMISE_VALUE_VERSION,
            Host::ValueLogV10,
            Host::ValueTypeOfV9,
            Host::ValueTruthyV9,
            Host::ValueNullishV9,
            Host::ValueIsArrayV9,
            Host::ValueStrictEqualV9,
            Host::ValueNullishEqualV9,
        ),
        (
            varta_common::value::FUNCTION_FIELD_VALUE_VERSION,
            Host::ValueLogV11,
            Host::ValueTypeOfV10,
            Host::ValueTruthyV10,
            Host::ValueNullishV10,
            Host::ValueIsArrayV10,
            Host::ValueStrictEqualV10,
            Host::ValueNullishEqualV10,
        ),
        (
            varta_common::value::HASHED_DICTIONARY_VALUE_VERSION,
            Host::ValueLogV12,
            Host::ValueTypeOfV11,
            Host::ValueTruthyV11,
            Host::ValueNullishV11,
            Host::ValueIsArrayV11,
            Host::ValueStrictEqualV11,
            Host::ValueNullishEqualV11,
        ),
    ] {
        register_log(linker, version, log)?;
        linker.func_wrap(
            "varta",
            type_of.name(),
            move |mut caller: Caller<'_, RuntimeContext>, value: i64| {
                caller.data_mut().tick()?;
                let value = read(&mut caller, value, version)?;
                let name = shared::type_of(value).map_err(fatal)?;
                host::result16(&mut caller, name.as_bytes())
                    .map_err(|_| wasmtime::Error::msg("value typeof result transfer"))
            },
        )?;
        for operation in [truthy, is_nullish, is_array] {
            linker.func_wrap(
                "varta",
                operation.name(),
                move |mut caller: Caller<'_, RuntimeContext>, value: i64| {
                    caller.data_mut().tick()?;
                    let value = read(&mut caller, value, version)?;
                    let result = if operation == is_array {
                        shared::is_array(value)
                    } else if operation == is_nullish {
                        shared::nullish(value)
                    } else {
                        shared::truthy(data(&mut caller)?, value).map_err(fatal)?
                    };
                    Ok(i32::from(result))
                },
            )?;
        }
        for operation in [strict_equal, nullish_equal] {
            linker.func_wrap(
                "varta",
                operation.name(),
                move |mut caller: Caller<'_, RuntimeContext>, a: i64, b: i64| {
                    caller.data_mut().tick()?;
                    let a = read(&mut caller, a, version)?;
                    let b = read(&mut caller, b, version)?;
                    let equal = if operation == nullish_equal {
                        shared::nullish_equal(a, b)
                    } else if a.kind == Kind::BigInt && b.kind == Kind::BigInt {
                        // BigInt comparison work is charged to the fuel.
                        crate::bigint::native_work(&mut caller, |data, budget| {
                            shared::strict_equal(data, a, b, budget).map_err(|error| match error {
                                ValueError::BigInt(error) => error,
                                _ => varta_bigint::Error::InvalidPayload,
                            })
                        })?
                        .map_err(wasmtime::Error::new)?
                    } else {
                        // No other kind does BigInt work.
                        let mut budget = varta_bigint::Budget::new(0, 0);
                        shared::strict_equal(data(&mut caller)?, a, b, &mut budget)
                            .map_err(fatal)?
                    };
                    Ok(i32::from(equal))
                },
            )?;
        }
    }
    linker.func_wrap(
        "varta",
        Host::ValueIsError.name(),
        |mut caller: Caller<'_, RuntimeContext>, value: i64| {
            caller.data_mut().tick()?;
            let value = read(&mut caller, value, varta_common::value::ERROR_VALUE_VERSION)?;
            Ok::<i32, wasmtime::Error>(i32::from(value.kind == Kind::Error))
        },
    )?;
    for (operation, version) in [
        (
            Host::ValueIsErrorClass,
            varta_common::value::ERROR_FAMILY_VALUE_VERSION,
        ),
        (
            Host::ValueIsErrorClassV2,
            varta_common::value::SPARSE_VALUE_VERSION,
        ),
        (
            Host::ValueIsErrorClassV3,
            varta_common::value::BIGINT_VALUE_VERSION,
        ),
        (
            Host::ValueIsErrorClassV4,
            varta_common::value::DYNAMIC_VALUE_VERSION,
        ),
        (
            Host::ValueIsErrorClassV5,
            varta_common::value::PROMISE_VALUE_VERSION,
        ),
        (
            Host::ValueIsErrorClassV6,
            varta_common::value::FUNCTION_FIELD_VALUE_VERSION,
        ),
        (
            Host::ValueIsErrorClassV7,
            varta_common::value::HASHED_DICTIONARY_VALUE_VERSION,
        ),
    ] {
        linker.func_wrap(
            "varta",
            operation.name(),
            move |mut caller: Caller<'_, RuntimeContext>, value: i64, class: i32| {
                caller.data_mut().tick()?;
                if varta_common::value::ErrorClass::from_tag(class as u32).is_none() {
                    return Err(fatal(ValueError::Class));
                }
                let value = read(&mut caller, value, version)?;
                let module = caller.data().module;
                let matched = shared::is_error_class(
                    data(&mut caller)?,
                    value,
                    class as u32,
                    version,
                    module,
                )
                .map_err(fatal)?;
                Ok::<i32, wasmtime::Error>(i32::from(matched))
            },
        )?;
    }
    register_log(
        linker,
        varta_common::value::SPARSE_VALUE_VERSION,
        Host::ValueLogV7,
    )?;
    Ok(())
}
