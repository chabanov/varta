//! The assignability relation of conditional types, constraints and type
//! predicates (TypeScript's `isTypeAssignableTo` over closed types): unions
//! on the source side distribute, literal sets are subsets of their base,
//! tuples and arrays are covariant, object types are structural with width
//! subtyping and optional properties, function types are contravariant in
//! their parameters and covariant in their result.
use super::*;

/// Whether `source` is assignable to `target`.
pub(in crate::lower) fn extends(source: Type, target: Type, types: &ir::ValueTypes) -> bool {
    extends_depth(source, target, types, 0)
}

fn extends_depth(source: Type, target: Type, types: &ir::ValueTypes, depth: usize) -> bool {
    if source == target || source == Type::Never || target == Type::Unknown {
        return true;
    }
    if depth > MAX_ANALYSIS_DEPTH || target == Type::Never {
        return false;
    }
    let depth = depth + 1;
    let sources = unions::members(source, types);
    if sources.len() > 1 {
        return sources
            .iter()
            .all(|member| extends_depth(*member, target, types, depth));
    }
    if let Type::Literal(base, id) = source
        && types.literals.values(id).len() > 1
    {
        // Every value of a literal set must be assignable on its own.
        return match target {
            Type::Literal(..) => literals::assignable(target, source, types),
            _ => {
                unions::members(target, types).contains(&base.erased())
                    || literals::member_of(source, &unions::members(target, types), types)
            }
        };
    }
    let targets = unions::members(target, types);
    if targets.len() > 1 {
        return targets
            .iter()
            .any(|member| extends_depth(source, *member, types, depth))
            || arguments::accepts(target, source, types);
    }
    match (source, target) {
        (Type::Literal(..), _) => literals::assignable(target, source, types),
        (Type::Number | Type::Float64, Type::Number | Type::Float64) => true,
        (Type::Tuple(a), Type::Tuple(b)) => tuples(a, b, types, depth),
        (Type::Tuple(a), Type::Array(element)) => {
            let tuple = &types.tuples[a as usize];
            let element = types.arrays[element as usize];
            tuple
                .elements
                .iter()
                .chain(tuple.rest.iter())
                .chain(tuple.trailing.iter())
                .all(|member| extends_depth(*member, element, types, depth))
        }
        (Type::Array(a), Type::Array(b)) => extends_depth(
            types.arrays[a as usize],
            types.arrays[b as usize],
            types,
            depth,
        ),
        (Type::Record(a), Type::Record(b)) => records(a, b, types, depth),
        (Type::Class(class), Type::Record(b)) => {
            arguments::accepts(target, source, types) || class_record(class, b, types, depth)
        }
        (Type::Function(a), Type::Function(b)) => functions(a, b, types, depth),
        (Type::Promise(a), Type::Promise(b)) => extends_depth(
            types.promises[a as usize].value,
            types.promises[b as usize].value,
            types,
            depth,
        ),
        (Type::Record(a), Type::Dictionary(b)) if types.records[a as usize].class.is_none() => {
            let value = types.dictionaries[b as usize].value;
            types.records[a as usize]
                .fields
                .iter()
                .all(|field| extends_depth(field.ty, value, types, depth))
        }
        (Type::Dictionary(a), Type::Dictionary(b)) => extends_depth(
            types.dictionaries[a as usize].value,
            types.dictionaries[b as usize].value,
            types,
            depth,
        ),
        (_, Type::Object(nullable)) => {
            object_kind(source) || nullable && matches!(source, Type::Null)
        }
        // `{}` admits every value except null and undefined.
        (_, Type::Record(b))
            if types.records[b as usize].fields.is_empty()
                && types.records[b as usize].class.is_none() =>
        {
            !matches!(source, Type::Null | Type::Undefined | Type::Void)
        }
        (Type::Never, _) => true,
        _ => arguments::accepts(target, source, types),
    }
}

/// Whether values of `ty` are objects (`object` admits them).
fn object_kind(ty: Type) -> bool {
    matches!(
        ty,
        Type::Record(_)
            | Type::Dictionary(_)
            | Type::Class(_)
            | Type::Array(_)
            | Type::Tuple(_)
            | Type::Function(_)
            | Type::Promise(_)
            | Type::Collection(..)
            | Type::Error
            | Type::NativeError(_)
            | Type::Object(false)
    )
}

fn tuples(a: u32, b: u32, types: &ir::ValueTypes, depth: usize) -> bool {
    let source = &types.tuples[a as usize];
    let target = &types.tuples[b as usize];
    // Tuples with trailing elements relate position by position only when
    // both have the same leading and trailing arity.
    if !source.trailing.is_empty() || !target.trailing.is_empty() {
        return source.elements.len() == target.elements.len()
            && source.required == target.required
            && source.trailing.len() == target.trailing.len()
            && source.rest.is_some() == target.rest.is_some()
            && source
                .elements
                .iter()
                .chain(source.rest.iter())
                .chain(source.trailing.iter())
                .zip(
                    target
                        .elements
                        .iter()
                        .chain(target.rest.iter())
                        .chain(target.trailing.iter()),
                )
                .all(|(own, wanted)| extends_depth(*own, *wanted, types, depth));
    }
    // Every length the source admits must be one the target admits.
    if source.required < target.required
        || source.rest.is_some() && target.rest.is_none()
        || target.rest.is_none() && source.elements.len() > target.elements.len()
    {
        return false;
    }
    let target_at = |index: usize| target.elements.get(index).copied().or(target.rest);
    source.elements.iter().enumerate().all(|(index, element)| {
        target_at(index).is_some_and(|wanted| extends_depth(*element, wanted, types, depth))
    }) && match (source.rest, target.rest) {
        (Some(rest), Some(wanted)) => extends_depth(rest, wanted, types, depth),
        _ => true,
    }
}

fn records(a: u32, b: u32, types: &ir::ValueTypes, depth: usize) -> bool {
    let source = &types.records[a as usize];
    let target = &types.records[b as usize];
    if target.class.is_some() {
        return false;
    }
    target.fields.iter().all(|wanted| {
        match source.fields.iter().find(|field| field.name == wanted.name) {
            Some(field) => {
                (wanted.optional || !field.optional)
                    && extends_depth(field.ty, wanted.ty, types, depth)
            }
            None => wanted.optional,
        }
    })
}

/// A class instance against an object type: its public fields and methods.
fn class_record(class: u32, b: u32, types: &ir::ValueTypes, depth: usize) -> bool {
    let target = &types.records[b as usize];
    target
        .fields
        .iter()
        .all(|wanted| match types.classes.field(class, &wanted.name) {
            Some((_, field)) => extends_depth(field.ty, wanted.ty, types, depth),
            None => wanted.optional,
        })
}

fn functions(a: u32, b: u32, types: &ir::ValueTypes, depth: usize) -> bool {
    let source = &types.functions[a as usize];
    let target = &types.functions[b as usize];
    if source.construct != target.construct {
        return false;
    }
    // The source may ignore trailing parameters, but must not require more.
    if source.shape.minimum() > target.params.len() && !target.shape.rest {
        return false;
    }
    let params_ok = source
        .params
        .iter()
        .zip(target.params.iter())
        .all(|(own, wanted)| extends_depth(*wanted, *own, types, depth));
    let result_ok = target.result == Type::Void
        || target.result == Type::Undefined && source.result == Type::Void
        || extends_depth(source.result, target.result, types, depth);
    let receiver_ok = match (source.receiver, target.receiver) {
        (Some(own), Some(wanted)) => extends_depth(wanted, own, types, depth),
        (Some(_), None) => false,
        _ => true,
    };
    params_ok && result_ok && receiver_ok
}
