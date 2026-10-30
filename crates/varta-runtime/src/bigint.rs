//! Pure BigInt ABI27 operations. All native integer work consumes guest fuel.
//!
//! The operations, the value13+ primitive conversions and the primitive
//! comparisons are `varta_intrinsics::bigint` (also linked into v38 guests);
//! these host imports of v28–v37 decode the result and charge the fuel.
use crate::{RuntimeContext, host};
use varta_bigint::{Budget, Error, Integer};
use varta_common::{HostOperation as H, abi, bigint::BigIntView};
use varta_intrinsics::bigint::{self as shared, Limits, MAX_BITS};
use wasmtime::{Caller, Extern, Linker};

/// The frozen string limits of every profile that imports these operations.
const HOST: Limits = Limits {
    string_bytes: abi::MAX_STRING_BYTES,
    result_bytes: abi::MAX_STRING_BYTES,
};

pub(super) fn validate_handle(data: &[u8], handle: i64) -> Result<(), wasmtime::Error> {
    BigIntView::read(data, handle as u64)
        .map(|_| ())
        .map_err(wasmtime::Error::new)
}
pub(super) fn integer(data: &[u8], handle: i64, budget: &mut Budget) -> Result<Integer, Error> {
    shared::integer(data, handle as u64, budget)
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
pub(super) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "varta",
        H::PrimitiveCompare.name(),
        |mut caller: Caller<'_, RuntimeContext>, left: i64, right: i64| {
            numeric(work(&mut caller, |data, budget| {
                shared::primitive_compare(data, left as u64, right as u64, budget, HOST)
            })?)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::PrimitiveEqual.name(),
        |mut caller: Caller<'_, RuntimeContext>,
         strict: i32,
         left: i64,
         right: i64|
         -> Result<i32, wasmtime::Error> {
            let result = work(&mut caller, |data, budget| {
                shared::primitive_equal(data, strict, left as u64, right as u64, budget, HOST)
            })?
            .map_err(wasmtime::Error::new)?;
            Ok(i32::from(result))
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntWidthV2.name(),
        |mut caller: Caller<'_, RuntimeContext>, signed: i32, width: i64, value: i64| {
            let result = work(&mut caller, |data, budget| {
                shared::width_v2(data, signed, width as u64, value as u64, budget, HOST)
            })?;
            output(&mut caller, result)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntToStringV2.name(),
        |mut caller: Caller<'_, RuntimeContext>, receiver: i64, radix: i64| {
            let result = work(&mut caller, |data, budget| {
                shared::to_string_v2(data, receiver as u64, radix as u64, budget, HOST)
            })?;
            output(&mut caller, result)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntWidth.name(),
        |mut caller: Caller<'_, RuntimeContext>, signed: i32, value: i64, width: f64| {
            let result = work(&mut caller, |data, budget| {
                shared::width(data, signed, value as u64, width, budget)
            })?;
            output(&mut caller, result)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntBinary.name(),
        |mut caller: Caller<'_, RuntimeContext>, op: i32, left: i64, right: i64| {
            let result = work(&mut caller, |data, budget| {
                shared::binary(data, op, left as u64, right as u64, budget)
            })?;
            output(&mut caller, result)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntUnary.name(),
        |mut caller: Caller<'_, RuntimeContext>, op: i32, value: i64| {
            let result = work(&mut caller, |data, budget| {
                shared::unary(data, op, value as u64, budget)
            })?;
            output(&mut caller, result)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntFromNumber.name(),
        |mut caller: Caller<'_, RuntimeContext>, value: f64| {
            let result = work(&mut caller, |_, budget| shared::from_number(value, budget))?;
            output(&mut caller, result)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntFromString.name(),
        |mut caller: Caller<'_, RuntimeContext>, pointer: i32, length: i32| {
            let result = work(&mut caller, |data, budget| {
                shared::from_string(data, pointer, length, budget, HOST)
            })?;
            output(&mut caller, result)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntToString.name(),
        |mut caller: Caller<'_, RuntimeContext>, value: i64, radix: f64| {
            let result = work(&mut caller, |data, budget| {
                shared::to_string(data, value as u64, radix, budget, HOST)
            })?;
            output(&mut caller, result)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntToNumber.name(),
        |mut caller: Caller<'_, RuntimeContext>, value: i64| {
            numeric(work(&mut caller, |data, budget| {
                shared::to_number(data, value as u64, budget)
            })?)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntCompareString.name(),
        |mut caller: Caller<'_, RuntimeContext>, left: i64, pointer: i32, length: i32| {
            numeric(work(&mut caller, |data, budget| {
                shared::compare_string(data, left as u64, pointer, length, budget, HOST)
            })?)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntCompare.name(),
        |mut caller: Caller<'_, RuntimeContext>, left: i64, right: i64| {
            numeric(work(&mut caller, |data, budget| {
                shared::compare(data, left as u64, right as u64, budget)
            })?)
        },
    )?;
    linker.func_wrap(
        "varta",
        H::BigIntCompareNumber.name(),
        |mut caller: Caller<'_, RuntimeContext>, left: i64, right: f64| {
            numeric(work(&mut caller, |data, budget| {
                shared::compare_number(data, left as u64, right, budget)
            })?)
        },
    )?;
    // Value13, value15, value16, value17 and value18 cells share these
    // conversions: for primitive and BigInt cells every one of those layouts
    // is the same.
    for (string, number, from_value) in [
        (
            H::PrimitiveStringV2,
            H::PrimitiveNumberV2,
            H::BigIntFromValue,
        ),
        (
            H::PrimitiveStringV3,
            H::PrimitiveNumberV3,
            H::BigIntFromValueV2,
        ),
        (
            H::PrimitiveStringV4,
            H::PrimitiveNumberV4,
            H::BigIntFromValueV3,
        ),
        (
            H::PrimitiveStringV5,
            H::PrimitiveNumberV5,
            H::BigIntFromValueV4,
        ),
        (
            H::PrimitiveStringV6,
            H::PrimitiveNumberV6,
            H::BigIntFromValueV5,
        ),
    ] {
        linker.func_wrap(
            "varta",
            string.name(),
            move |mut caller: Caller<'_, RuntimeContext>,
                  cell: i64|
                  -> Result<i32, wasmtime::Error> {
                let bytes = work(&mut caller, |data, budget| {
                    shared::primitive_to_string(data, cell as u64, budget, HOST)
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
                    shared::primitive_to_number(data, cell as u64, budget, HOST)
                })?)
            },
        )?;
        linker.func_wrap(
            "varta",
            from_value.name(),
            move |mut caller: Caller<'_, RuntimeContext>, cell: i64| {
                let result = work(&mut caller, |data, budget| {
                    shared::from_value(data, cell as u64, budget, HOST)
                })?;
                output(&mut caller, result)
            },
        )?;
    }
    Ok(())
}
