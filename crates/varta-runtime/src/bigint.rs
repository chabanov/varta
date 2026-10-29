//! Pure BigInt ABI27 operations. All native integer work consumes guest fuel.
use crate::{RuntimeContext, host};
use varta_bigint::{Budget, Error, Integer};
use varta_common::value::{BIGINT_VALUE_VERSION, ReferenceBox, ValueKind as K};
use varta_common::{
    HostOperation as H, abi,
    bigint::{self, BigIntView, BinaryOp, UnaryOp},
};
use wasmtime::{Caller, Extern, Linker};

mod comparison;

const MAX_BITS: u64 = ((bigint::MAX_PAYLOAD_BYTES - bigint::META_BYTES) * 8) as u64;
pub(super) fn validate_handle(data: &[u8], handle: i64) -> Result<(), wasmtime::Error> {
    BigIntView::read(data, handle as u64)
        .map(|_| ())
        .map_err(wasmtime::Error::new)
}
pub(super) fn integer(data: &[u8], handle: i64, budget: &mut Budget) -> Result<Integer, Error> {
    BigIntView::read(data, handle as u64).map_err(|_| Error::InvalidPayload)?;
    let pointer = handle as u32 as usize;
    let length = ((handle as u64) >> 32) as usize;
    Integer::from_payload(&data[pointer..pointer + length], budget)
}
pub(super) fn work<T>(
    caller: &mut Caller<'_, RuntimeContext>,
    run: impl FnOnce(&[u8], &mut Budget) -> Result<T, Error>,
) -> Result<Result<T, Error>, wasmtime::Error> {
    caller.data_mut().tick()?;
    native_work(caller, run)
}
pub(super) fn native_work<T>(
    caller: &mut Caller<'_, RuntimeContext>,
    run: impl FnOnce(&[u8], &mut Budget) -> Result<T, Error>,
) -> Result<Result<T, Error>, wasmtime::Error> {
    let fuel = caller.get_fuel()?;
    let mut budget = Budget::new(MAX_BITS, fuel);
    let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
        return Err(wasmtime::Error::msg("missing BigInt memory"));
    };
    let result = run(memory.data(&*caller), &mut budget);
    caller.set_fuel(budget.remaining_work())?;
    caller.data().check_budget()?;
    match result {
        Err(error) if error.recoverable_class().is_none() => Err(wasmtime::Error::new(error)),
        other => Ok(other),
    }
}
fn output(
    caller: &mut Caller<'_, RuntimeContext>,
    result: Result<Vec<u8>, Error>,
) -> Result<(i32, i32), wasmtime::Error> {
    match result {
        Ok(bytes) => {
            let pointer = host::result_bytes(caller, &bytes)
                .map_err(|_| wasmtime::Error::msg("BigInt result transfer"))?;
            caller.data().check_budget()?;
            Ok((pointer, 0))
        }
        Err(error) => match error.recoverable_class() {
            Some(class) => Ok((0, class as i32)),
            None => Err(wasmtime::Error::new(error)),
        },
    }
}
fn numeric(result: Result<f64, Error>) -> Result<f64, wasmtime::Error> {
    result.map_err(wasmtime::Error::new)
}
fn string_integer(
    data: &[u8],
    pointer: i32,
    length: i32,
    budget: &mut Budget,
) -> Result<Integer, Error> {
    let pointer = usize::try_from(pointer).map_err(|_| Error::InvalidPayload)?;
    let length = usize::try_from(length).map_err(|_| Error::InvalidPayload)?;
    if length > abi::MAX_STRING_BYTES || !length.is_multiple_of(2) {
        return Err(Error::InvalidPayload);
    }
    let bytes = pointer
        .checked_add(length)
        .and_then(|end| data.get(pointer..end))
        .ok_or(Error::InvalidPayload)?;
    if (length / 2) as u64 > budget.remaining_work() {
        return Err(Error::WorkBudget);
    }
    let units = varta_common::text::units(bytes).map_err(|_| Error::InvalidPayload)?;
    Integer::from_utf16(&units, budget)
}
fn primitive_argument(data: &[u8], cell: i64, budget: &mut Budget) -> Result<ReferenceBox, Error> {
    let value = ReferenceBox::read_with_version(data, cell as u64, BIGINT_VALUE_VERSION)
        .map_err(|_| Error::InvalidPayload)?;
    if !matches!(
        value.kind,
        K::Float64 | K::String | K::Boolean | K::Null | K::Undefined | K::BigInt
    ) {
        return Err(Error::InvalidPayload);
    }
    budget.consume_work(
        1 + if value.kind == K::BigInt {
            (value.payload >> 32) / 4
        } else {
            0
        },
    )?;
    Ok(value)
}
fn implicit_number(
    data: &[u8],
    cell: i64,
    value: ReferenceBox,
    budget: &mut Budget,
) -> Result<f64, Error> {
    if value.kind == K::BigInt {
        return Err(Error::Type);
    }
    if value.kind == K::String {
        budget.consume_work(value.payload >> 32)?;
    }
    crate::primitive_number::convert(data, cell).map_err(|_| Error::InvalidPayload)
}
fn implicit_bigint(
    data: &[u8],
    value: ReferenceBox,
    budget: &mut Budget,
) -> Result<Integer, Error> {
    match value.kind {
        K::BigInt => integer(data, value.payload as i64, budget),
        K::Boolean => Integer::from_bool(value.payload != 0, budget),
        K::String => string_integer(
            data,
            value.payload as u32 as i32,
            (value.payload >> 32) as i32,
            budget,
        ),
        K::Float64 | K::Null | K::Undefined => Err(Error::Type),
        _ => Err(Error::InvalidPayload),
    }
}
fn formatted(value: Integer, radix: f64, budget: &mut Budget) -> Result<Vec<u8>, Error> {
    let radix = if radix.is_nan() { 0.0 } else { radix.trunc() };
    if !(2.0..=36.0).contains(&radix) {
        return Err(Error::InvalidRadix);
    }
    let text = value.to_string_radix(radix as u32, budget)?;
    if text.len() > abi::MAX_STRING_BYTES / 2 {
        return Err(Error::BitCapacity);
    }
    budget.consume_work(text.len() as u64)?;
    Ok(varta_common::text::encode(&text))
}
pub(super) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    comparison::register(linker)?;
    linker.func_wrap(
        "varta",
        H::BigIntWidthV2.name(),
        |mut caller: Caller<'_, RuntimeContext>, signed: i32, width: i64, value: i64| {
            let result = work(&mut caller, |data, budget| {
                if !matches!(signed, 0 | 1) {
                    return Err(Error::InvalidPayload);
                }
                // Validate both transports before allowing a recoverable error.
                // Semantic conversions still occur in ToIndex/ToBigInt order.
                let width_arg = primitive_argument(data, width, budget)?;
                let value = primitive_argument(data, value, budget)?;
                let width =
                    Integer::index_from_number(implicit_number(data, width, width_arg, budget)?)?;
                let value = implicit_bigint(data, value, budget)?;
                let result = if signed == 1 {
                    value.as_int_n(width, budget)?
                } else {
                    value.as_uint_n(width, budget)?
                };
                result.to_payload(budget)
            })?;
            output(&mut caller, result)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntToStringV2.name(),
        |mut caller: Caller<'_, RuntimeContext>, receiver: i64, radix: i64| {
            let result = work(&mut caller, |data, budget| {
                let value = integer(data, receiver, budget)?;
                let arg = primitive_argument(data, radix, budget)?;
                let radix = if arg.kind == K::Undefined {
                    10.0
                } else {
                    implicit_number(data, radix, arg, budget)?
                };
                formatted(value, radix, budget)
            })?;
            output(&mut caller, result)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntWidth.name(),
        |mut caller: Caller<'_, RuntimeContext>, signed: i32, value: i64, width: f64| {
            let result = work(&mut caller, |data, budget| {
                if !matches!(signed, 0 | 1) {
                    return Err(Error::InvalidPayload);
                }
                let width = Integer::index_from_number(width)?;
                let value = integer(data, value, budget)?;
                let result = if signed == 1 {
                    value.as_int_n(width, budget)?
                } else {
                    value.as_uint_n(width, budget)?
                };
                result.to_payload(budget)
            })?;
            output(&mut caller, result)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntBinary.name(),
        |mut caller: Caller<'_, RuntimeContext>, op: i32, left: i64, right: i64| {
            let result = work(&mut caller, |data, budget| {
                let op = BinaryOp::from_tag(op as u32).ok_or(Error::InvalidPayload)?;
                let left = integer(data, left, budget)?;
                let right = integer(data, right, budget)?;
                left.binary(op, &right, budget)?.to_payload(budget)
            })?;
            output(&mut caller, result)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntUnary.name(),
        |mut caller: Caller<'_, RuntimeContext>, op: i32, value: i64| {
            let result = work(&mut caller, |data, budget| {
                let op = UnaryOp::from_tag(op as u32).ok_or(Error::InvalidPayload)?;
                integer(data, value, budget)?
                    .unary(op, budget)?
                    .to_payload(budget)
            })?;
            output(&mut caller, result)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntFromNumber.name(),
        |mut caller: Caller<'_, RuntimeContext>, value: f64| {
            let result = work(&mut caller, |_, budget| {
                Integer::from_number(value, budget)?.to_payload(budget)
            })?;
            output(&mut caller, result)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntFromString.name(),
        |mut caller: Caller<'_, RuntimeContext>, pointer: i32, length: i32| {
            let result = work(&mut caller, |data, budget| {
                string_integer(data, pointer, length, budget)?.to_payload(budget)
            })?;
            output(&mut caller, result)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntToString.name(),
        |mut caller: Caller<'_, RuntimeContext>, value: i64, radix: f64| {
            let result = work(&mut caller, |data, budget| {
                let value = integer(data, value, budget)?;
                let radix = if radix.is_nan() { 0.0 } else { radix.trunc() };
                if !(2.0..=36.0).contains(&radix) {
                    return Err(Error::InvalidRadix);
                }
                let text = value.to_string_radix(radix as u32, budget)?;
                if text.len() > abi::MAX_STRING_BYTES / 2 {
                    return Err(Error::BitCapacity);
                }
                // Formatting and encoding both traverse the output, including zero.
                budget.consume_work(text.len() as u64)?;
                Ok(varta_common::text::encode(&text))
            })?;
            output(&mut caller, result)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntToNumber.name(),
        |mut caller: Caller<'_, RuntimeContext>, value: i64| {
            numeric(work(&mut caller, |data, budget| {
                integer(data, value, budget)?.to_number(budget)
            })?)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntCompareString.name(),
        |mut caller: Caller<'_, RuntimeContext>, left: i64, pointer: i32, length: i32| {
            numeric(work(&mut caller, |data, budget| {
                let left = integer(data, left, budget)?;
                match string_integer(data, pointer, length, budget) {
                    Ok(right) => Ok(left.compare(&right, budget)? as i8 as f64),
                    Err(Error::Syntax) => Ok(f64::NAN),
                    Err(error) => Err(error),
                }
            })?)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntCompare.name(),
        |mut caller: Caller<'_, RuntimeContext>, left: i64, right: i64| {
            numeric(work(&mut caller, |data, budget| {
                let left = integer(data, left, budget)?;
                let right = integer(data, right, budget)?;
                Ok(left.compare(&right, budget)? as i8 as f64)
            })?)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntCompareNumber.name(),
        |mut caller: Caller<'_, RuntimeContext>, left: i64, right: f64| {
            numeric(work(&mut caller, |data, budget| {
                Ok(integer(data, left, budget)?
                    .compare_number(right, budget)?
                    .map_or(f64::NAN, |order| order as i8 as f64))
            })?)
        },
    )?;
    // Value13, value15 and value16 cells share these conversions; each operation reads
    // exactly the layout version its profile stamps.
    for (string, number, from_value, version) in [
        (
            H::PrimitiveStringV2,
            H::PrimitiveNumberV2,
            H::BigIntFromValue,
            BIGINT_VALUE_VERSION,
        ),
        (
            H::PrimitiveStringV3,
            H::PrimitiveNumberV3,
            H::BigIntFromValueV2,
            varta_common::value::DYNAMIC_VALUE_VERSION,
        ),
        (
            H::PrimitiveStringV4,
            H::PrimitiveNumberV4,
            H::BigIntFromValueV3,
            varta_common::value::PROMISE_VALUE_VERSION,
        ),
    ] {
        linker.func_wrap(
            "varta",
            string.name(),
            move |mut caller: Caller<'_, RuntimeContext>,
                  cell: i64|
                  -> Result<i32, wasmtime::Error> {
                let bytes = work(&mut caller, |data, budget| {
                    let value = varta_common::value::ReferenceBox::read_with_version(
                        data,
                        cell as u64,
                        version,
                    )
                    .map_err(|_| Error::InvalidPayload)?;
                    let bytes = if value.kind == varta_common::value::ValueKind::BigInt {
                        let text = integer(data, value.payload as i64, budget)?
                            .to_string_radix(10, budget)?;
                        budget.consume_work(text.len() as u64)?;
                        varta_common::text::encode(&text)
                    } else {
                        let length = (value.payload >> 32) as usize;
                        if value.kind == varta_common::value::ValueKind::String
                            && length as u64 > budget.remaining_work()
                        {
                            return Err(Error::WorkBudget);
                        }
                        let bytes = crate::primitive_string::convert(data, cell)
                            .map_err(|_| Error::InvalidPayload)?;
                        budget.consume_work(bytes.len() as u64)?;
                        bytes
                    };
                    Ok(bytes)
                })?
                .map_err(wasmtime::Error::new)?;
                host::result_bytes(&mut caller, &bytes)
                    .map_err(|_| wasmtime::Error::msg("String result transfer"))
            },
        )?;
        linker.func_wrap(
            "varta",
            number.name(),
            move |mut caller: Caller<'_, RuntimeContext>,
                  cell: i64|
                  -> Result<f64, wasmtime::Error> {
                numeric(work(&mut caller, |data, budget| {
                    let value = varta_common::value::ReferenceBox::read_with_version(
                        data,
                        cell as u64,
                        version,
                    )
                    .map_err(|_| Error::InvalidPayload)?;
                    if value.kind == varta_common::value::ValueKind::BigInt {
                        integer(data, value.payload as i64, budget)?.to_number(budget)
                    } else {
                        if value.kind == varta_common::value::ValueKind::String {
                            budget.consume_work(value.payload >> 32)?;
                        }
                        crate::primitive_number::convert(data, cell)
                            .map_err(|_| Error::InvalidPayload)
                    }
                })?)
            },
        )?;
        linker.func_wrap(
            "varta",
            from_value.name(),
            move |mut caller: Caller<'_, RuntimeContext>, cell: i64| {
                let result = work(&mut caller, |data, budget| {
                    let value = ReferenceBox::read_with_version(data, cell as u64, version)
                        .map_err(|_| Error::InvalidPayload)?;
                    let value = match value.kind {
                        K::BigInt => integer(data, value.payload as i64, budget)?,
                        K::Float64 => Integer::from_number(f64::from_bits(value.payload), budget)?,
                        K::Boolean => Integer::from_bool(value.payload != 0, budget)?,
                        K::String => string_integer(
                            data,
                            value.payload as u32 as i32,
                            (value.payload >> 32) as i32,
                            budget,
                        )?,
                        K::Null | K::Undefined => return Err(Error::Type),
                        _ => return Err(Error::InvalidPayload),
                    };
                    value.to_payload(budget)
                })?;
                output(&mut caller, result)
            },
        )?;
    }
    Ok(())
}
