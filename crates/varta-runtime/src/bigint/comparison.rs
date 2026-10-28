//! Primitive abstract equality and ordering; transported values remain canonical.
use super::*;
use std::cmp::Ordering;
fn order(value: Ordering) -> f64 {
    match value {
        Ordering::Less => -1.0,
        Ordering::Equal => 0.0,
        Ordering::Greater => 1.0,
    }
}
fn text_order(
    data: &[u8],
    a: ReferenceBox,
    b: ReferenceBox,
    budget: &mut Budget,
) -> Result<f64, Error> {
    budget.consume_work((a.payload >> 32) + (b.payload >> 32))?;
    let units = |v: ReferenceBox| {
        let start = v.payload as u32 as usize;
        data[start..start + (v.payload >> 32) as usize]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|s| u16::from_le_bytes(*s))
    };
    Ok(order(units(a).cmp(units(b))))
}
fn number(data: &[u8], cell: i64, v: ReferenceBox, budget: &mut Budget) -> Result<f64, Error> {
    match v.kind {
        K::Float64 => Ok(f64::from_bits(v.payload)),
        K::Boolean => Ok(v.payload as f64),
        K::Null => Ok(0.0),
        K::Undefined => Ok(f64::NAN),
        K::String => implicit_number(data, cell, v, budget),
        _ => Err(Error::InvalidPayload),
    }
}
fn string_bigint(
    data: &[u8],
    v: ReferenceBox,
    budget: &mut Budget,
) -> Result<Option<Integer>, Error> {
    match string_integer(
        data,
        v.payload as u32 as i32,
        (v.payload >> 32) as i32,
        budget,
    ) {
        Ok(v) => Ok(Some(v)),
        Err(Error::Syntax) => Ok(None),
        Err(e) => Err(e),
    }
}
fn numeric_order(
    data: &[u8],
    left: i64,
    a: ReferenceBox,
    right: i64,
    b: ReferenceBox,
    budget: &mut Budget,
) -> Result<f64, Error> {
    Ok(match (a.kind, b.kind) {
        (K::BigInt, K::BigInt) => integer(data, a.payload as i64, budget)?
            .compare(&integer(data, b.payload as i64, budget)?, budget)?
            as i8 as f64,
        (K::BigInt, _) => integer(data, a.payload as i64, budget)?
            .compare_number(number(data, right, b, budget)?, budget)?
            .map_or(f64::NAN, |o| o as i8 as f64),
        (_, K::BigInt) => -integer(data, b.payload as i64, budget)?
            .compare_number(number(data, left, a, budget)?, budget)?
            .map_or(f64::NAN, |o| o as i8 as f64),
        _ => number(data, left, a, budget)?
            .partial_cmp(&number(data, right, b, budget)?)
            .map_or(f64::NAN, order),
    })
}
fn compare(
    data: &[u8],
    left: i64,
    a: ReferenceBox,
    right: i64,
    b: ReferenceBox,
    budget: &mut Budget,
) -> Result<f64, Error> {
    match (a.kind, b.kind) {
        (K::String, K::String) => text_order(data, a, b, budget),
        (K::BigInt, K::String) => {
            let big = integer(data, a.payload as i64, budget)?;
            Ok(match string_bigint(data, b, budget)? {
                Some(s) => big.compare(&s, budget)? as i8 as f64,
                None => f64::NAN,
            })
        }
        (K::String, K::BigInt) => {
            let string = string_bigint(data, a, budget)?;
            let big = integer(data, b.payload as i64, budget)?;
            Ok(match string {
                Some(s) => s.compare(&big, budget)? as i8 as f64,
                None => f64::NAN,
            })
        }
        _ => numeric_order(data, left, a, right, b, budget),
    }
}
fn equal(
    data: &[u8],
    strict: bool,
    left: i64,
    mut a: ReferenceBox,
    right: i64,
    mut b: ReferenceBox,
    budget: &mut Budget,
) -> Result<bool, Error> {
    if a.kind == b.kind {
        return match a.kind {
            K::Null | K::Undefined | K::Boolean => Ok(a.payload == b.payload),
            K::Float64 => Ok(f64::from_bits(a.payload) == f64::from_bits(b.payload)),
            K::String => Ok(text_order(data, a, b, budget)? == 0.0),
            K::BigInt => Ok(numeric_order(data, left, a, right, b, budget)? == 0.0),
            _ => Err(Error::InvalidPayload),
        };
    }
    if strict {
        return Ok(false);
    }
    if matches!(a.kind, K::Null | K::Undefined) || matches!(b.kind, K::Null | K::Undefined) {
        return Ok(
            matches!(a.kind, K::Null | K::Undefined) && matches!(b.kind, K::Null | K::Undefined)
        );
    }
    for v in [&mut a, &mut b] {
        if v.kind == K::Boolean {
            v.kind = K::Float64;
            v.payload = (v.payload as f64).to_bits();
        }
    }
    if matches!(
        (a.kind, b.kind),
        (K::BigInt, K::String) | (K::String, K::BigInt)
    ) {
        return Ok(compare(data, left, a, right, b, budget)? == 0.0);
    }
    Ok(numeric_order(data, left, a, right, b, budget)? == 0.0)
}
pub(super) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "varta",
        H::PrimitiveCompare.name(),
        |mut caller: Caller<'_, RuntimeContext>, left: i64, right: i64| {
            numeric(work(&mut caller, |data, budget| {
                let a = primitive_argument(data, left, budget)?;
                let b = primitive_argument(data, right, budget)?;
                compare(data, left, a, right, b, budget)
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
                if !matches!(strict, 0 | 1) {
                    return Err(Error::InvalidPayload);
                }
                let a = primitive_argument(data, left, budget)?;
                let b = primitive_argument(data, right, budget)?;
                equal(data, strict == 1, left, a, right, b, budget)
            })?
            .map_err(wasmtime::Error::new)?;
            Ok(i32::from(result))
        },
    )?;
    Ok(())
}
