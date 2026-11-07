//! Class subtyping in containers (v38 `type_level`). TypeScript's arrays
//! are covariant: a `B[]` is assignable to `A[]` when `B` extends `A`.
//! Varta's array of class handles has the same physical elements for both,
//! but an alias typed `A[]` could store an `A` into the `B[]` another alias
//! still reads as `B`. The conversion therefore copies the array (a shallow
//! `slice()`), unless the value is a fresh array literal: elements keep
//! their identity, the array does not (an intentional deviation from Node,
//! where both names alias one array). Only element types that retype
//! without a value conversion qualify: a class to an ancestor class, or a
//! union of class instances (and `null`/`undefined`) to the union of their
//! ancestors.
use super::*;

/// `value` (an array) converted to the array type `expected` when its
/// elements are class instances of descendants of `expected`'s elements.
pub(in crate::lower) fn covariant_array(
    value: Expr,
    expected: Option<Type>,
    types: &ir::ValueTypes,
) -> Expr {
    let value = overloads::overloaded_value(value, expected, types);
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
    if !retypes(given_element, wanted_element, types) {
        return value;
    }
    let span = value.span;
    let copied = if matches!(value.kind, E::Array(_)) {
        value
    } else {
        Expr {
            span,
            ty: value.ty,
            kind: E::ArraySlice(
                Box::new(value),
                Box::new(Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Float64(0.0),
                }),
                Box::new(Expr {
                    span,
                    ty: Type::Undefined,
                    kind: E::Integer(0),
                }),
            ),
        }
    };
    Expr {
        span,
        ty: Type::Array(wanted),
        kind: E::Reinterpret(Box::new(copied)),
    }
}

/// Whether a value of `given` is a value of `wanted` with the same bits.
fn retypes(given: Type, wanted: Type, types: &ir::ValueTypes) -> bool {
    if classes::upcast(types, given, wanted) {
        return true;
    }
    if !given.is_union() || !wanted.is_union() {
        return false;
    }
    let members = unions::members(given, types);
    let targets = unions::members(wanted, types);
    members.iter().all(|member| {
        matches!(member, Type::Class(_) | Type::Null | Type::Undefined)
            && (targets.contains(member)
                || targets
                    .iter()
                    .any(|target| classes::upcast(types, *member, *target)))
    })
}

impl Lower<'_> {
    /// An array whose elements are a union of class instances (`(B | C)[]`)
    /// as an array of their common ancestor (`A[]`): a new array of the
    /// unboxed elements, since a union cell and a class handle differ.
    pub(in crate::lower) fn class_union_array(
        &mut self,
        value: Expr,
        expected: Option<Type>,
    ) -> Expr {
        let (Some(Type::Array(wanted)), Type::Array(given)) = (expected, value.ty) else {
            return value;
        };
        if !self.types.profile.type_level() || wanted == given {
            return value;
        }
        let (target, source) = (
            self.types.arrays[wanted as usize],
            self.types.arrays[given as usize],
        );
        let Type::Class(_) = target else {
            return value;
        };
        let members = unions::members(source, self.types);
        if !source.is_union()
            || !members
                .iter()
                .all(|member| classes::upcast(self.types, *member, target) || *member == target)
        {
            return value;
        }
        let span = value.span;
        let local = |slot: u32, ty: Type| Expr {
            span,
            ty,
            kind: E::Local(slot),
        };
        let number = |value: f64| Expr {
            span,
            ty: Type::Float64,
            kind: E::Float64(value),
        };
        let array_type = Type::Array(wanted);
        let given_type = value.ty;
        let from = self.hidden(given_type);
        let into = self.hidden(array_type);
        let index = self.hidden(Type::Float64);
        let limit = self.hidden(Type::Float64);
        let output = self.hidden(array_type);
        let marker = self.hidden(Type::Number);
        let element = Expr {
            span,
            ty: target,
            kind: E::UnboxUnion(Box::new(Expr {
                span,
                ty: source,
                kind: E::Index(
                    Box::new(local(from, given_type)),
                    Box::new(local(index, Type::Float64)),
                ),
            })),
        };
        let push = S::Expr(Expr {
            span,
            ty: Type::Float64,
            kind: E::ArrayPush(
                Box::new(local(into, array_type)),
                Box::new(Expr {
                    span,
                    ty: array_type,
                    kind: E::Array(vec![Some(element)]),
                }),
            ),
        });
        let empty = Expr {
            span,
            ty: array_type,
            kind: E::Array(Vec::new()),
        };
        let count = Expr {
            span,
            ty: Type::Float64,
            kind: E::Length(Box::new(local(from, given_type))),
        };
        let test = Expr {
            span,
            ty: Type::Boolean,
            kind: E::Binary(
                ast::BinaryOp::Lt,
                Box::new(local(index, Type::Float64)),
                Box::new(local(limit, Type::Float64)),
            ),
        };
        let step = Expr {
            span,
            ty: Type::Float64,
            kind: E::Binary(
                ast::BinaryOp::Add,
                Box::new(local(index, Type::Float64)),
                Box::new(number(1.0)),
            ),
        };
        Expr {
            span,
            ty: array_type,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![from, into],
                body: vec![
                    S::Init(from, value),
                    S::Init(into, empty),
                    S::Init(limit, count),
                    S::For(
                        vec![S::Init(index, number(0.0))],
                        Some(test),
                        vec![S::Set(index, step)],
                        vec![push],
                    ),
                    S::Init(output, local(into, array_type)),
                ],
            },
        }
    }
}
