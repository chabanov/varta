//! The built-in utility types (TypeScript Handbook, Utility Types), each
//! evaluated over resolved types exactly as its `lib.es5.d.ts` definition
//! would be: `Partial`, `Required`, `Readonly`, `Pick`, `Omit`, `Record`,
//! `Exclude`, `Extract`, `NonNullable`, `ReturnType`, `Parameters`,
//! `ConstructorParameters`, `InstanceType`, `ThisParameterType`,
//! `OmitThisParameter`, `Awaited`, `NoInfer` and the intrinsic string
//! types. A module-local declaration of the same name shadows a built-in.
use super::*;
use ir::{LiteralBase, RecordField};
use template_types::Intrinsic;

/// A utility type reference; `None` for any other name.
pub(super) fn reference(
    reference: &ast::TsTypeRef,
    types: &mut ir::ValueTypes,
) -> Result<Option<Type>> {
    let ast::TsEntityName::Ident(id) = &reference.type_name else {
        return Ok(None);
    };
    // `ReturnType<typeof setTimeout>` keeps its timer handle type (v36).
    if let Some(handle) = promises::timers::handle_type(reference, types) {
        return Ok(Some(handle));
    }
    let name = id.sym.as_ref();
    let Some(params) = reference.type_params.as_deref() else {
        return Ok(None);
    };
    if named::declared(types, name)
        || generics::parameter(&ast::TsType::TsTypeRef(reference.clone()), types).is_some()
    {
        return Ok(None);
    }
    let arity = match name {
        "Partial"
        | "Required"
        | "Readonly"
        | "NonNullable"
        | "ReturnType"
        | "Parameters"
        | "ConstructorParameters"
        | "InstanceType"
        | "ThisParameterType"
        | "OmitThisParameter"
        | "Awaited"
        | "NoInfer"
        | "Uppercase"
        | "Lowercase"
        | "Capitalize"
        | "Uncapitalize" => 1,
        "Pick" | "Omit" | "Record" | "Exclude" | "Extract" => 2,
        _ => return Ok(None),
    };
    if params.params.len() != arity {
        return Err(type_error(format!(
            "{name} expects {arity} type argument(s), found {}",
            params.params.len()
        )));
    }
    let first = keyword_type(&params.params[0], types)?;
    let second = match arity {
        2 => Some(keyword_type(&params.params[1], types)?),
        _ => None,
    };
    let second = || second.ok_or_else(|| invalid("missing second type argument"));
    Ok(Some(match name {
        "Partial" => distribute(first, types, partial)?,
        "Required" => distribute(first, types, required)?,
        // Readonly is binding-level intent in Varta (see `type_operators`).
        "Readonly" | "NoInfer" => first,
        "Pick" => pick(first, second()?, types)?,
        "Omit" => omit(first, second()?, types)?,
        "Record" => record(first, second()?, types)?,
        "Exclude" => filter(first, second()?, false, types)?,
        "Extract" => filter(first, second()?, true, types)?,
        "NonNullable" => {
            let members = split(first, types)
                .into_iter()
                .filter(|m| !matches!(m, Type::Null | Type::Undefined | Type::Void))
                .collect();
            union_of(members, types)?
        }
        "ReturnType" => distribute(first, types, |ty, t| {
            signature_part(ty, "ReturnType", false, t, |s, _| Ok(s.result))
        })?,
        "Parameters" => distribute(first, types, |ty, t| {
            signature_part(ty, "Parameters", false, t, |s, t| parameters_tuple(s, 0, t))
        })?,
        "ConstructorParameters" => distribute(first, types, |ty, t| {
            signature_part(ty, "ConstructorParameters", true, t, |s, t| {
                parameters_tuple(s, 0, t)
            })
        })?,
        "InstanceType" => distribute(first, types, |ty, t| {
            signature_part(ty, "InstanceType", true, t, |s, _| Ok(s.result))
        })?,
        "ThisParameterType" => match first {
            Type::Function(id) => types.functions[id as usize]
                .receiver
                .unwrap_or(Type::Unknown),
            _ => Type::Unknown,
        },
        "OmitThisParameter" => match first {
            Type::Function(id) if types.functions[id as usize].receiver.is_some() => {
                let signature = types.functions[id as usize].clone();
                functions::intern_full(
                    None,
                    signature.params,
                    signature.shape,
                    signature.result,
                    types,
                )?
            }
            other => other,
        },
        "Awaited" => awaited(first, types)?,
        intrinsic => match Intrinsic::from_name(intrinsic) {
            Some(intrinsic) => intrinsic.evaluate(first, types)?,
            None => return Ok(None),
        },
    }))
}

/// Apply `map` to each union member (a homomorphic mapped type distributes).
fn distribute(
    ty: Type,
    types: &mut ir::ValueTypes,
    mut map: impl FnMut(Type, &mut ir::ValueTypes) -> Result<Type>,
) -> Result<Type> {
    let members = unions::members(ty, types);
    if ty == Type::Never {
        return Ok(Type::Never);
    }
    let mut results = Vec::new();
    for member in members {
        results.push(map(member, types)?);
    }
    union_of(results, types)
}

fn object_fields(ty: Type, what: &str, types: &mut ir::ValueTypes) -> Result<Vec<RecordField>> {
    match ty {
        Type::Record(id) if types.records[id as usize].class.is_none() => {
            Ok(types.records[id as usize].fields.clone())
        }
        Type::Class(class) => objects::class_members(class, types),
        other => Err(unsupported(&format!(
            "{what} of {}",
            subtyping::describe(other, types)
        ))),
    }
}

fn partial(ty: Type, types: &mut ir::ValueTypes) -> Result<Type> {
    match ty {
        Type::Tuple(id) => {
            let tuple = types.tuples[id as usize].clone();
            let mut elements = Vec::new();
            for element in tuple.elements {
                elements.push(tuple_types::union_with_undefined(element, types)?);
            }
            let rest = match tuple.rest {
                Some(rest) => Some(tuple_types::union_with_undefined(rest, types)?),
                None => None,
            };
            let mut trailing = Vec::new();
            for element in tuple.trailing {
                trailing.push(tuple_types::union_with_undefined(element, types)?);
            }
            tuples::intern_tuple(elements, 0, (rest, trailing), tuple.readonly, types)
        }
        Type::Array(id) => {
            let element = tuple_types::union_with_undefined(types.arrays[id as usize], types)?;
            arrays::intern(element, types)
        }
        other if is_primitive(other) => Ok(other),
        _ => {
            let mut fields = object_fields(ty, "Partial", types)?;
            for field in &mut fields {
                field.ty = tuple_types::union_with_undefined(field.ty, types)?;
                field.optional = true;
            }
            records::intern(fields, types)
        }
    }
}

fn required(ty: Type, types: &mut ir::ValueTypes) -> Result<Type> {
    match ty {
        Type::Tuple(id) => {
            let tuple = types.tuples[id as usize].clone();
            let mut elements = Vec::new();
            for (index, element) in tuple.elements.iter().enumerate() {
                elements.push(if index >= tuple.required {
                    tuple_types::strip_undefined(*element, types)?
                } else {
                    *element
                });
            }
            let count = elements.len();
            tuples::intern_tuple(
                elements,
                count,
                (tuple.rest, tuple.trailing),
                tuple.readonly,
                types,
            )
        }
        Type::Array(_) => Ok(ty),
        other if is_primitive(other) => Ok(other),
        _ => {
            let mut fields = object_fields(ty, "Required", types)?;
            for field in &mut fields {
                if field.optional {
                    field.ty = tuple_types::strip_undefined(field.ty, types)?;
                    field.optional = false;
                }
            }
            records::intern(fields, types)
        }
    }
}

fn is_primitive(ty: Type) -> bool {
    matches!(
        ty,
        Type::String
            | Type::Number
            | Type::Float64
            | Type::Boolean
            | Type::BigInt
            | Type::Null
            | Type::Undefined
            | Type::Literal(..)
    )
}

/// The property names of a key type argument.
fn key_names(keys: Type, what: &str, types: &mut ir::ValueTypes) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for member in split(keys, types) {
        match member {
            Type::Literal(LiteralBase::String, _) => {
                names.extend(literals::string_members(member, types).unwrap_or_default());
            }
            Type::Literal(LiteralBase::Number, id) => {
                if let Some(literals::LiteralValue::Number(bits)) =
                    types.literals.values(id).first()
                {
                    names.push(template_types::number_text(f64::from_bits(*bits)));
                }
            }
            Type::Never => {}
            other => {
                return Err(unsupported(&format!(
                    "{what} with the key type {} (only literal keys select properties statically)",
                    subtyping::describe(other, types)
                )));
            }
        }
    }
    Ok(names)
}

/// `Pick<T, K>`: the properties `K` of `T`, keeping their modifiers; over a
/// union, each property's union of member types.
fn pick(ty: Type, keys: Type, types: &mut ir::ValueTypes) -> Result<Type> {
    let names = key_names(keys, "Pick", types)?;
    let members = unions::members(ty, types);
    let mut fields = Vec::new();
    for name in names {
        let mut member_types = Vec::new();
        let mut optional = false;
        for member in &members {
            let field = object_fields(*member, "Pick", types)?
                .into_iter()
                .find(|field| field.name == name)
                .ok_or_else(|| {
                    type_error(format!(
                        "type '{name}' does not satisfy the constraint keyof {} of Pick",
                        subtyping::describe(ty, types)
                    ))
                })?;
            optional |= field.optional;
            member_types.push(field.ty);
        }
        fields.push(RecordField {
            name,
            ty: union_of(member_types, types)?,
            optional,
        });
    }
    records::intern(fields, types)
}

/// `Omit<T, K>` = `Pick<T, Exclude<keyof T, K>>`.
fn omit(ty: Type, keys: Type, types: &mut ir::ValueTypes) -> Result<Type> {
    let omitted = key_names_lenient(keys, types)?;
    let all = objects::keys(ty, types)?;
    let kept: Vec<String> = literals::string_members(all, types)
        .unwrap_or_default()
        .into_iter()
        .filter(|name| omitted.as_ref().is_some_and(|names| !names.contains(name)))
        .collect();
    let kept = objects::string_keys(kept, types)?;
    pick(ty, kept, types)
}

/// The names of an `Omit` key argument; `string` omits every key.
fn key_names_lenient(keys: Type, types: &mut ir::ValueTypes) -> Result<Option<Vec<String>>> {
    if split(keys, types).contains(&Type::String) {
        return Ok(None);
    }
    key_names(keys, "Omit", types).map(Some)
}

/// `Record<K, V>`: one required `V` property per literal key, or the string
/// index signature for `string`.
fn record(keys: Type, value: Type, types: &mut ir::ValueTypes) -> Result<Type> {
    if keys == Type::String {
        return dictionaries::intern(value, types);
    }
    let names = key_names(keys, "Record", types)?;
    let fields = names
        .into_iter()
        .map(|name| {
            Ok(RecordField {
                name: records::key(name)?,
                ty: value,
                optional: false,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    records::intern(fields, types)
}

/// `Exclude<T, U>` (`keep` false) and `Extract<T, U>` (`keep` true).
fn filter(ty: Type, by: Type, keep: bool, types: &mut ir::ValueTypes) -> Result<Type> {
    let members = split(ty, types)
        .into_iter()
        .filter(|member| extends(*member, by, types) == keep)
        .collect();
    union_of(members, types)
}

/// A part of a function (or, `construct`, constructor) type.
fn signature_part(
    ty: Type,
    what: &str,
    construct: bool,
    types: &mut ir::ValueTypes,
    part: impl FnOnce(&ir::FunctionType, &mut ir::ValueTypes) -> Result<Type>,
) -> Result<Type> {
    match ty {
        Type::Function(id) if types.functions[id as usize].construct == construct => {
            let signature = types.functions[id as usize].clone();
            part(&signature, types)
        }
        // A class object (v38) through its constructor type.
        Type::ClassValue(class) if construct => {
            let constructor = constructors::class_constructor(class, types)?;
            signature_part(constructor, what, construct, types, part)
        }
        Type::Never => Ok(Type::Never),
        other => Err(type_error(format!(
            "type argument {} of {what} is not a {} type",
            subtyping::describe(other, types),
            if construct { "constructor" } else { "function" }
        ))),
    }
}

/// The parameter list of `signature` from `from` on as a tuple type:
/// optional parameters are optional elements and a rest parameter is the
/// tuple's rest element.
pub(super) fn parameters_tuple(
    signature: &ir::FunctionType,
    from: usize,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let fixed = signature.shape.fixed_count();
    let mut elements = Vec::new();
    let mut required = 0;
    for index in from..fixed {
        let optional = signature.shape.optional[index];
        elements.push(signature.params[index]);
        if !optional {
            required = elements.len();
        }
    }
    let rest = if signature.shape.rest && from <= fixed {
        match signature.params.get(fixed) {
            Some(Type::Array(id)) => Some(types.arrays[*id as usize]),
            Some(Type::Tuple(id)) => {
                // A rest parameter typed by a tuple splices its elements.
                let tuple = types.tuples[*id as usize].clone();
                if elements.len() != required {
                    return Err(unsupported(
                        "tuple-typed rest parameter after optional parameters",
                    ));
                }
                let base = elements.len();
                elements.extend(tuple.elements.iter().copied());
                required = base + tuple.required;
                tuple.rest
            }
            _ => None,
        }
    } else {
        None
    };
    tuples::intern_rest(elements, required, rest, false, types)
}

/// `Awaited<T>`: the settled value of nested promises, per union member.
fn awaited(ty: Type, types: &mut ir::ValueTypes) -> Result<Type> {
    distribute(ty, types, |member, t| match member {
        Type::Promise(id) => {
            let value = t.promises[id as usize].value;
            awaited(value, t)
        }
        other => Ok(other),
    })
}
