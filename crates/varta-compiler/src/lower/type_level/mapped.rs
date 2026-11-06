//! Mapped types `{ [K in C as N]: V }` with `+/-readonly` and `+/-?`
//! (TypeScript Handbook, Mapped Types). The key set is resolved, so the
//! result is a closed record (or, for a `string` key set, a dictionary).
//!
//! A homomorphic mapped type (`K in keyof T`) keeps the optionality of
//! `T`'s properties, maps a tuple or array element-wise and distributes
//! over a union bound to a naked `T`. `readonly` is binding-level intent in
//! Varta (the closed type is the same), so its modifiers are accepted and
//! erased like `Readonly<T>`.
use super::*;
use ir::{LiteralBase, RecordField};

pub(super) fn evaluate(mapped: &ast::TsMappedType, types: &mut ir::ValueTypes) -> Result<Type> {
    let constraint = mapped
        .type_param
        .constraint
        .as_deref()
        .ok_or_else(|| invalid("mapped type without a key constraint"))?;
    let value = mapped.type_ann.as_deref().ok_or_else(|| {
        unsupported("mapped type without a value type (its properties would be any)")
    })?;
    let key = mapped.type_param.name.sym.to_string();
    if let ast::TsType::TsTypeOperator(operator) = constraint
        && operator.op == ast::TsTypeOperatorOp::KeyOf
    {
        let source = keyword_type(&operator.type_ann, types)?;
        if let Some(name) = naked_parameter(&operator.type_ann, types) {
            let members = unions::members(source, types);
            if members.len() > 1 {
                let mut results = Vec::new();
                for member in members {
                    let mut frame = generics::visible(types);
                    frame.insert(name.clone(), member);
                    results.push(generics::with_frame(types, frame, |t| {
                        homomorphic(mapped, &key, value, member, t)
                    })?);
                }
                return union_of(results, types);
            }
            if source == Type::Never {
                return Ok(Type::Never);
            }
        }
        return homomorphic(mapped, &key, value, source, types);
    }
    let keys = keyword_type(constraint, types)?;
    map_keys(mapped, &key, value, keys, None, types)
}

/// `{ [K in keyof T]: V }` for one resolved `T`.
fn homomorphic(
    mapped: &ast::TsMappedType,
    key: &str,
    value: &ast::TsType,
    source: Type,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    match source {
        Type::Record(id) if types.records[id as usize].class.is_none() => {
            let keys = objects::keys(source, types)?;
            map_keys(mapped, key, value, keys, Some(id), types)
        }
        Type::Tuple(id) if mapped.name_type.is_none() => tuple(mapped, key, value, id, types),
        Type::Array(_) if mapped.name_type.is_none() => {
            let element = bound(key, types.number(), value, types)?;
            arrays::intern(element, types)
        }
        Type::Dictionary(_) => {
            let keys = objects::keys(source, types)?;
            map_keys(mapped, key, value, keys, None, types)
        }
        Type::Class(_) => Err(unsupported(
            "mapped type over a class instance type (its methods and inherited members are keys)",
        )),
        other if is_primitive(other) => Ok(other),
        other => Err(unsupported(&format!(
            "mapped type over {}",
            subtyping::describe(other, types)
        ))),
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

/// The value type with the key parameter bound to `key_type`.
fn bound(
    key: &str,
    key_type: Type,
    value: &ast::TsType,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let mut frame = generics::visible(types);
    frame.insert(key.to_owned(), key_type);
    generics::with_frame(types, frame, |t| keyword_type(value, t))
}

/// Apply the `?` modifier to one property: `(optional, type)`.
fn modified(
    modifier: Option<ast::TruePlusMinus>,
    inherited: bool,
    ty: Type,
    types: &mut ir::ValueTypes,
) -> Result<(bool, Type)> {
    Ok(match modifier {
        Some(ast::TruePlusMinus::True | ast::TruePlusMinus::Plus) => {
            (true, tuple_types::union_with_undefined(ty, types)?)
        }
        Some(ast::TruePlusMinus::Minus) => (false, tuple_types::strip_undefined(ty, types)?),
        None if inherited => (true, tuple_types::union_with_undefined(ty, types)?),
        None => (false, ty),
    })
}

fn map_keys(
    mapped: &ast::TsMappedType,
    key: &str,
    value: &ast::TsType,
    keys: Type,
    source: Option<u32>,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let mut fields: Vec<RecordField> = Vec::new();
    let mut dictionary: Option<Type> = None;
    for key_type in split(keys, types) {
        let (name, literal) = match key_type {
            Type::Literal(LiteralBase::String, _) => (
                literals::string_members(key_type, types)
                    .and_then(|names| names.into_iter().next())
                    .ok_or_else(|| unsupported("mapped type key that is not a property name"))?,
                true,
            ),
            Type::Literal(LiteralBase::Number, id) => match types.literals.values(id).first() {
                Some(literals::LiteralValue::Number(bits)) => {
                    (template_types::number_text(f64::from_bits(*bits)), true)
                }
                _ => return Err(invalid("malformed number literal type")),
            },
            Type::String if mapped.name_type.is_none() => (String::new(), false),
            Type::Never => continue,
            other => {
                return Err(unsupported(&format!(
                    "mapped type over the key type {} (only string and number literal keys and string have a closed layout)",
                    subtyping::describe(other, types)
                )));
            }
        };
        if !literal {
            // `{ [K in string]: V }` is the string index signature.
            dictionary = Some(bound(key, Type::String, value, types)?);
            continue;
        }
        let inherited = source.is_some_and(|id| {
            types.records[id as usize]
                .fields
                .iter()
                .any(|field| field.name == name && field.optional)
        });
        let names = match &mapped.name_type {
            Some(remap) => {
                let renamed = bound(key, key_type, remap, types)?;
                let mut names = Vec::new();
                for member in split(renamed, types) {
                    match member {
                        Type::Never => {}
                        Type::Literal(LiteralBase::String, _) => names
                            .extend(literals::string_members(member, types).unwrap_or_default()),
                        Type::Literal(LiteralBase::Number, id) => {
                            if let Some(literals::LiteralValue::Number(bits)) =
                                types.literals.values(id).first()
                            {
                                names.push(template_types::number_text(f64::from_bits(*bits)));
                            }
                        }
                        other => {
                            return Err(unsupported(&format!(
                                "mapped type key remapped to {} (only literal keys have a closed layout)",
                                subtyping::describe(other, types)
                            )));
                        }
                    }
                }
                names
            }
            None => vec![name],
        };
        if names.is_empty() {
            continue;
        }
        let ty = bound(key, key_type, value, types)?;
        let (optional, ty) = modified(mapped.optional, inherited, ty, types)?;
        for name in names {
            if let Some(existing) = fields.iter_mut().find(|field| field.name == name) {
                // Two keys remapped to one name: TypeScript unions the values.
                existing.ty = union_of(vec![existing.ty, ty], types)?;
                existing.optional &= optional;
            } else {
                fields.push(RecordField {
                    name: records::key(name)?,
                    ty,
                    optional,
                });
            }
        }
    }
    if let Some(value) = dictionary {
        if !fields.is_empty() {
            return Err(unsupported(
                "mapped type over string and literal keys together (a mixed index record)",
            ));
        }
        return dictionaries::intern(value, types);
    }
    records::intern(fields, types)
}

/// A homomorphic mapped type over a tuple maps each element; `K` is the
/// element's index as a string literal (`"0"`, `"1"`, ...).
fn tuple(
    mapped: &ast::TsMappedType,
    key: &str,
    value: &ast::TsType,
    id: u32,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let tuple = types.tuples[id as usize].clone();
    let mut elements = Vec::new();
    let mut required = 0;
    for (index, _) in tuple.elements.iter().enumerate() {
        let key_type = literals::string_set([index.to_string().as_str()], types)?;
        let ty = bound(key, key_type, value, types)?;
        let inherited = index >= tuple.required;
        let (optional, ty) = modified(mapped.optional, inherited, ty, types)?;
        if !optional && elements.len() > required {
            return Err(unsupported(
                "mapped tuple type with a required element after an optional one",
            ));
        }
        if !optional {
            required += 1;
        }
        elements.push(ty);
    }
    let rest = match tuple.rest {
        Some(_) => Some(bound(key, types.number(), value, types)?),
        None => None,
    };
    let readonly = match mapped.readonly {
        Some(ast::TruePlusMinus::Minus) => false,
        Some(_) => true,
        None => tuple.readonly,
    };
    tuples::intern_rest(elements, required, rest, readonly, types)
}
