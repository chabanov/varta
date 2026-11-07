//! Class subtyping in containers (v38 `type_level`). TypeScript's arrays
//! are covariant: a `B[]` is assignable to `A[]` when `B` extends `A`, and
//! a `(B | C)[]` when both extend `A`. As in JavaScript, the converted value
//! is the same array: the conversion is a `Reinterpret` that `covariance`
//! records, and `covariance::resolve` gives the array types that share an
//! array one element representation and tests the reads through the
//! narrower names. Only element types that retype without a value
//! conversion qualify: a class or a union of classes (with `null` or
//! `undefined`) to an ancestor class or a union of ancestors.
use super::*;

/// `value` (an array) converted to the array type `expected` when its
/// elements are class instances of descendants of `expected`'s elements.
pub(in crate::lower) fn covariant_array(
    value: Expr,
    expected: Option<Type>,
    types: &ir::ValueTypes,
) -> Expr {
    let value = overloads::overloaded_value(value, expected, types);
    // A callable object viewed with fewer properties, or its call signature
    // (step 4): the same function.
    let value = match expected {
        Some(wanted) if callables::converts(value.ty, wanted, types) => Expr {
            span: value.span,
            ty: wanted,
            kind: E::Reinterpret(Box::new(value)),
        },
        _ => value,
    };
    // A value of one generic construct type used as another (step 4).
    let value = match expected {
        Some(wanted) if wanted != value.ty => generic_constructors::convert(value, wanted, types),
        _ => value,
    };
    // A function literal lowered against a generic call signature's erased
    // type (`const id: <T>(x: T) => T = (x) => x`) takes that marked type.
    if let (Some(Type::Function(wanted)), Type::Function(given)) = (expected, value.ty)
        && types.profile.type_level()
        && wanted != given
    {
        let (wanted_signature, given_signature) = (
            &types.functions[wanted as usize],
            &types.functions[given as usize],
        );
        if wanted_signature.generic.is_some()
            && given_signature.generic.is_none()
            && wanted_signature.receiver == given_signature.receiver
            && wanted_signature.params == given_signature.params
            && wanted_signature.shape == given_signature.shape
            && wanted_signature.result == given_signature.result
            && !given_signature.construct
        {
            return Expr {
                span: value.span,
                ty: Type::Function(wanted),
                kind: E::Reinterpret(Box::new(value)),
            };
        }
    }
    let (Some(Type::Array(wanted)), Type::Array(given)) = (expected, value.ty) else {
        return value;
    };
    if !types.profile.type_level() || wanted == given {
        return value;
    }
    let (wanted_element, given_element) =
        (types.arrays[wanted as usize], types.arrays[given as usize]);
    if !covariance::shares(given_element, wanted_element, types) {
        return value;
    }
    covariance::note(given, wanted, types);
    Expr {
        span: value.span,
        ty: Type::Array(wanted),
        kind: E::Reinterpret(Box::new(value)),
    }
}
