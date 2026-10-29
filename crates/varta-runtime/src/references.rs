//! Versioned pure operations over primitive/reference cells. Shallow validation
//! precedes observation; diagnostic JSON traversals retain all serializer limits.
use crate::{RuntimeContext, host, number};
use varta_common::{
    HostOperation as Host,
    value::{ReferenceBox, ValueKind as Kind},
};
use wasmtime::{Caller, Extern, Linker};
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
    ReferenceBox::read_with_version(data(caller)?, value as u64, version)
        .map_err(wasmtime::Error::new)
}
fn nullish(value: ReferenceBox) -> bool {
    matches!(value.kind, Kind::Null | Kind::Undefined)
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
                    if matches!(log, Host::ValueLogV7 | Host::ValueLogV8 | Host::ValueLogV9) {
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
                Kind::Record | Kind::Array | Kind::Dictionary => {
                    if matches!(
                        version,
                        varta_common::value::BIGINT_VALUE_VERSION
                            | varta_common::value::DYNAMIC_VALUE_VERSION
                    ) {
                        caller.data().check_budget()?;
                        let fuel = caller.get_fuel()?;
                        let mut budget = varta_bigint::Budget::new(
                            ((varta_common::bigint::MAX_PAYLOAD_BYTES
                                - varta_common::bigint::META_BYTES)
                                * 8) as u64,
                            fuel,
                        );
                        let result = crate::json::render_diagnostic(
                            data(&mut caller)?,
                            value.kind as u32,
                            value.payload,
                            version,
                            &mut budget,
                        );
                        caller.set_fuel(budget.remaining_work())?;
                        caller.data().check_budget()?;
                        String::from_utf8(result?).map_err(wasmtime::Error::new)?
                    } else {
                        String::from_utf8(crate::json::render_reference(
                            data(&mut caller)?,
                            value.kind as u32,
                            value.payload,
                            version,
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
    ] {
        register_log(linker, version, log)?;
        linker.func_wrap(
            "varta",
            type_of.name(),
            move |mut caller: Caller<'_, RuntimeContext>, value: i64| {
                caller.data_mut().tick()?;
                let value = read(&mut caller, value, version)?;
                let name = match value.kind {
                    Kind::Error | Kind::Null | Kind::Record | Kind::Array | Kind::Dictionary => {
                        "object"
                    }
                    Kind::Undefined => "undefined",
                    Kind::String => "string",
                    Kind::Float64 => "number",
                    Kind::Boolean => "boolean",
                    Kind::BigInt => "bigint",
                    _ => return Err(wasmtime::Error::msg("invalid value kind")),
                };
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
                        value.kind == Kind::Array
                    } else if operation == is_nullish {
                        nullish(value)
                    } else {
                        match value.kind {
                            Kind::Null | Kind::Undefined => false,
                            Kind::Error | Kind::Record | Kind::Array | Kind::Dictionary => true,
                            Kind::BigInt => !varta_common::bigint::BigIntView::read(
                                data(&mut caller)?,
                                value.payload,
                            )
                            .map_err(wasmtime::Error::new)?
                            .is_zero(),
                            Kind::Boolean => value.payload != 0,
                            Kind::String => value.payload >> i32::BITS != 0,
                            Kind::Float64 => {
                                let n = f64::from_bits(value.payload);
                                n != 0.0 && !n.is_nan()
                            }
                            _ => false,
                        }
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
                        nullish(a) && nullish(b)
                    } else if a.kind != b.kind {
                        false
                    } else {
                        match a.kind {
                            Kind::BigInt => {
                                crate::bigint::native_work(&mut caller, |data, budget| {
                                    let left =
                                        crate::bigint::integer(data, a.payload as i64, budget)?;
                                    let right =
                                        crate::bigint::integer(data, b.payload as i64, budget)?;
                                    Ok(left.compare(&right, budget)? == std::cmp::Ordering::Equal)
                                })?
                                .map_err(wasmtime::Error::new)?
                            }
                            Kind::Float64 => f64::from_bits(a.payload) == f64::from_bits(b.payload),
                            Kind::String => {
                                let bytes = data(&mut caller)?;
                                text(bytes, a) == text(bytes, b)
                            }
                            _ => a.payload == b.payload,
                        }
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
    ] {
        linker.func_wrap(
            "varta",
            operation.name(),
            move |mut caller: Caller<'_, RuntimeContext>, value: i64, class: i32| {
                caller.data_mut().tick()?;
                let class = varta_common::value::ErrorClass::from_tag(class as u32)
                    .ok_or_else(|| wasmtime::Error::msg("invalid error class"))?;
                let value = read(&mut caller, value, version)?;
                let matched = if value.kind == Kind::Error {
                    let object = varta_common::value::ErrorObject::read_with_version(
                        data(&mut caller)?,
                        value.payload,
                        version,
                    )
                    .map_err(wasmtime::Error::new)?;
                    class == varta_common::value::ErrorClass::Error || object.class == class
                } else {
                    false
                };
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
