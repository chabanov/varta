//! Primitive abstract equality and ordering (`primitive_compare`,
//! `primitive_equal`); transported values remain canonical.
use super::*;

fn units<M: Memory + ?Sized>(memory: &M, value: PrimitiveBox) -> Result<Vec<u16>, Error> {
    let (start, length) = string_range(value.payload);
    // The checked cell proved the complete even-length range.
    let bytes = memory.borrow(start, length).ok_or(Error::InvalidPayload)?;
    text::units(&bytes).map_err(|_| Error::InvalidPayload)
}
fn text_order<M: Memory + ?Sized>(
    memory: &M,
    a: PrimitiveBox,
    b: PrimitiveBox,
    budget: &mut Budget,
) -> Result<f64, Error> {
    budget.consume_work((a.payload >> 32) + (b.payload >> 32))?;
    Ok(order(units(memory, a)?.cmp(&units(memory, b)?)))
}
fn number<M: Memory + ?Sized>(
    memory: &M,
    cell: u64,
    v: PrimitiveBox,
    budget: &mut Budget,
    limits: Limits,
) -> Result<f64, Error> {
    match v.kind {
        K::Float64 => Ok(f64::from_bits(v.payload)),
        K::Boolean => Ok(v.payload as f64),
        K::Null => Ok(0.0),
        K::Undefined => Ok(f64::NAN),
        K::String => implicit_number(memory, cell, v, budget, limits),
        _ => Err(Error::InvalidPayload),
    }
}
fn string_bigint<M: Memory + ?Sized>(
    memory: &M,
    v: PrimitiveBox,
    budget: &mut Budget,
    limits: Limits,
) -> Result<Option<Integer>, Error> {
    let (pointer, length) = string_range(v.payload);
    match string_integer(memory, pointer as i32, length as i32, budget, limits) {
        Ok(v) => Ok(Some(v)),
        Err(Error::Syntax) => Ok(None),
        Err(e) => Err(e),
    }
}
fn numeric_order<M: Memory + ?Sized>(
    memory: &M,
    (left, a): (u64, PrimitiveBox),
    (right, b): (u64, PrimitiveBox),
    budget: &mut Budget,
    limits: Limits,
) -> Result<f64, Error> {
    Ok(match (a.kind, b.kind) {
        (K::BigInt, K::BigInt) => order(
            integer(memory, a.payload, budget)?
                .compare(&integer(memory, b.payload, budget)?, budget)?,
        ),
        (K::BigInt, _) => integer(memory, a.payload, budget)?
            .compare_number(number(memory, right, b, budget, limits)?, budget)?
            .map_or(f64::NAN, order),
        (_, K::BigInt) => -integer(memory, b.payload, budget)?
            .compare_number(number(memory, left, a, budget, limits)?, budget)?
            .map_or(f64::NAN, order),
        _ => number(memory, left, a, budget, limits)?
            .partial_cmp(&number(memory, right, b, budget, limits)?)
            .map_or(f64::NAN, order),
    })
}
fn ordered<M: Memory + ?Sized>(
    memory: &M,
    (left, a): (u64, PrimitiveBox),
    (right, b): (u64, PrimitiveBox),
    budget: &mut Budget,
    limits: Limits,
) -> Result<f64, Error> {
    match (a.kind, b.kind) {
        (K::String, K::String) => text_order(memory, a, b, budget),
        (K::BigInt, K::String) => {
            let big = integer(memory, a.payload, budget)?;
            Ok(match string_bigint(memory, b, budget, limits)? {
                Some(s) => order(big.compare(&s, budget)?),
                None => f64::NAN,
            })
        }
        (K::String, K::BigInt) => {
            let string = string_bigint(memory, a, budget, limits)?;
            let big = integer(memory, b.payload, budget)?;
            Ok(match string {
                Some(s) => order(s.compare(&big, budget)?),
                None => f64::NAN,
            })
        }
        _ => numeric_order(memory, (left, a), (right, b), budget, limits),
    }
}
fn equality<M: Memory + ?Sized>(
    memory: &M,
    strict: bool,
    (left, mut a): (u64, PrimitiveBox),
    (right, mut b): (u64, PrimitiveBox),
    budget: &mut Budget,
    limits: Limits,
) -> Result<bool, Error> {
    if a.kind == b.kind {
        return match a.kind {
            K::Null | K::Undefined | K::Boolean => Ok(a.payload == b.payload),
            K::Float64 => Ok(f64::from_bits(a.payload) == f64::from_bits(b.payload)),
            K::String => Ok(text_order(memory, a, b, budget)? == 0.0),
            K::BigInt => Ok(numeric_order(memory, (left, a), (right, b), budget, limits)? == 0.0),
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
        return Ok(ordered(memory, (left, a), (right, b), budget, limits)? == 0.0);
    }
    Ok(numeric_order(memory, (left, a), (right, b), budget, limits)? == 0.0)
}
/// `a < b`-style abstract relational comparison of two primitive or BigInt
/// cells: -1, 0, 1, or NaN when unordered (`primitive_compare`).
pub fn compare<M: Memory + ?Sized>(
    memory: &M,
    left: u64,
    right: u64,
    budget: &mut Budget,
    limits: Limits,
) -> Result<f64, Error> {
    let a = primitive_argument(memory, left, budget, limits)?;
    let b = primitive_argument(memory, right, budget, limits)?;
    ordered(memory, (left, a), (right, b), budget, limits)
}
/// `==` (`strict` 0) or `===` (`strict` 1) of two primitive or BigInt cells
/// (`primitive_equal`).
pub fn equal<M: Memory + ?Sized>(
    memory: &M,
    strict: i32,
    left: u64,
    right: u64,
    budget: &mut Budget,
    limits: Limits,
) -> Result<bool, Error> {
    if !matches!(strict, 0 | 1) {
        return Err(Error::InvalidPayload);
    }
    let a = primitive_argument(memory, left, budget, limits)?;
    let b = primitive_argument(memory, right, budget, limits)?;
    equality(memory, strict == 1, (left, a), (right, b), budget, limits)
}
