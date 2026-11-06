//! `keyof T`, indexed access `T[K]` and intersections `A & B` over every
//! closed type (TypeScript Handbook, Keyof Type Operator, Indexed Access
//! Types, Intersection Types).
use super::*;
use ir::{LiteralBase, RecordField};

/// `keyof T`: the key set of an object type as a string literal set,
/// `string | number` for a string index signature, the common keys of a
/// union, `never` for an empty object or `unknown`.
pub(super) fn key_of(target: &ast::TsType, types: &mut ir::ValueTypes) -> Result<Type> {
    let ty = keyword_type(target, types)?;
    keys(ty, types)
}

pub(super) fn keys(ty: Type, types: &mut ir::ValueTypes) -> Result<Type> {
    let members = unions::members(ty, types);
    if members.len() > 1
        && members
            .iter()
            .all(|m| matches!(m, Type::Record(_) | Type::Class(_)))
    {
        // keyof (A | B) is the keys every member has.
        let mut common: Option<Vec<String>> = None;
        for member in members {
            let names = names(member, types)?;
            common = Some(match common {
                None => names,
                Some(previous) => previous.into_iter().filter(|n| names.contains(n)).collect(),
            });
        }
        return string_keys(common.unwrap_or_default(), types);
    }
    match ty {
        Type::Record(_) => {
            let names = names(ty, types)?;
            string_keys(names, types)
        }
        Type::Dictionary(_) => union_of(vec![Type::String, types.number()], types),
        Type::Unknown => Ok(Type::Never),
        Type::Class(_) => Err(unsupported(
            "keyof a class type (methods and inherited members are keys)",
        )),
        Type::Array(_) | Type::Tuple(_) => Err(unsupported(
            "keyof an array or tuple type (every array method is a key)",
        )),
        Type::Never => Err(unsupported(
            "keyof never (string | number | symbol needs symbol keys)",
        )),
        other => Err(unsupported(&format!(
            "keyof {} (its prototype methods are keys)",
            subtyping::describe(other, types)
        ))),
    }
}

fn names(ty: Type, types: &ir::ValueTypes) -> Result<Vec<String>> {
    match ty {
        Type::Record(id) => Ok(types.records[id as usize]
            .fields
            .iter()
            .map(|field| field.name.clone())
            .collect()),
        _ => Err(unsupported(
            "keyof a class type (methods and inherited members are keys)",
        )),
    }
}

/// The string literal set of `names`; `never` when there is none.
pub(super) fn string_keys(names: Vec<String>, types: &mut ir::ValueTypes) -> Result<Type> {
    if names.is_empty() {
        return Ok(Type::Never);
    }
    literals::string_set(names.iter().map(String::as_str), types)
}

/// One key of an indexed access, as distributed from the index type.
enum Key {
    Name(String),
    Position(usize),
    Number,
    String,
}

fn index_keys(index: Type, types: &mut ir::ValueTypes) -> Result<Vec<Key>> {
    let mut keys = Vec::new();
    for member in split(index, types) {
        keys.push(match member {
            Type::Literal(LiteralBase::String, _) => {
                let names = literals::string_members(member, types)
                    .ok_or_else(|| unsupported("index type with a non-key string"))?;
                Key::Name(names.into_iter().next().unwrap_or_default())
            }
            Type::Literal(LiteralBase::Number, id) => {
                let value = types.literals.values(id).first().cloned();
                match value {
                    Some(literals::LiteralValue::Number(bits)) => {
                        let value = f64::from_bits(bits);
                        if value >= 0.0
                            && value.fract() == 0.0
                            && value < tuples::MAX_TUPLE_ELEMENTS as f64
                        {
                            Key::Position(value as usize)
                        } else {
                            Key::Name(template_types::number_text(value))
                        }
                    }
                    _ => return Err(invalid("malformed number literal type")),
                }
            }
            Type::Number | Type::Float64 => Key::Number,
            Type::String => Key::String,
            Type::Never => continue,
            other => {
                return Err(type_error(format!(
                    "type {} cannot be used as an index type",
                    subtyping::describe(other, types)
                )));
            }
        });
    }
    Ok(keys)
}

/// `T[K]`: distributes over the members of `T` and of `K`.
pub(super) fn indexed(
    access: &ast::TsIndexedAccessType,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let object = keyword_type(&access.obj_type, types)?;
    let index = keyword_type(&access.index_type, types)?;
    index_type(object, index, types)
}

pub(super) fn index_type(object: Type, index: Type, types: &mut ir::ValueTypes) -> Result<Type> {
    let keys = index_keys(index, types)?;
    let mut results = Vec::new();
    for member in unions::members(object, types) {
        for key in &keys {
            results.push(access(member, key, types)?);
        }
    }
    union_of(results, types)
}

fn access(object: Type, key: &Key, types: &mut ir::ValueTypes) -> Result<Type> {
    let missing = |name: &str, types: &ir::ValueTypes| {
        type_error(format!(
            "property {name} does not exist on type {}",
            subtyping::describe(object, types)
        ))
    };
    match (object, key) {
        (Type::Record(id), Key::Name(name)) => types.records[id as usize]
            .fields
            .iter()
            .find(|field| field.name == *name)
            .map(|field| field.ty)
            .ok_or_else(|| missing(name, types)),
        (Type::Record(id), Key::Position(at)) => {
            let name = at.to_string();
            types.records[id as usize]
                .fields
                .iter()
                .find(|field| field.name == name)
                .map(|field| field.ty)
                .ok_or_else(|| missing(&name, types))
        }
        (Type::Class(class), Key::Name(name)) => types
            .classes
            .field(class, name)
            .map(|(_, field)| field.ty)
            .ok_or_else(|| {
                unsupported(&format!(
                    "indexed access of class member {name} (only data fields have a static type here)"
                ))
            }),
        (Type::Tuple(id), _) => tuple_access(id, key, types),
        (Type::Array(id), Key::Number) => Ok(types.arrays[id as usize]),
        (Type::Array(id), Key::Position(_)) => Ok(types.arrays[id as usize]),
        (Type::Array(_), Key::Name(name)) if name == "length" => Ok(types.number()),
        (Type::Dictionary(id), Key::String | Key::Name(_) | Key::Number | Key::Position(_)) => {
            Ok(types.dictionaries[id as usize].value)
        }
        (Type::String, Key::Name(name)) if name == "length" => Ok(types.number()),
        (Type::String, Key::Number | Key::Position(_)) => Ok(Type::String),
        (Type::Never, _) => Ok(Type::Never),
        (_, Key::Name(name)) => Err(missing(name, types)),
        _ => Err(unsupported(&format!(
            "indexed access of {} by a non-literal key",
            subtyping::describe(object, types)
        ))),
    }
}

fn tuple_access(id: u32, key: &Key, types: &mut ir::ValueTypes) -> Result<Type> {
    let tuple = types.tuples[id as usize].clone();
    let position = match key {
        Key::Position(at) => Some(*at),
        Key::Name(name) => name
            .parse::<usize>()
            .ok()
            .filter(|at| at.to_string() == *name),
        _ => None,
    };
    match (key, position) {
        (_, Some(at)) => tuple
            .elements
            .get(at)
            .copied()
            .or(tuple.rest)
            .ok_or_else(|| {
                type_error(format!(
                    "tuple type of length {} has no element at index {at}",
                    tuple.elements.len()
                ))
            }),
        (Key::Name(name), None) if name == "length" => {
            if tuple.rest.is_some() {
                return Ok(types.number());
            }
            // `[A, B?]['length']` is `1 | 2`.
            let values = (tuple.required..=tuple.elements.len())
                .filter_map(|length| literals::number_value(length as f64))
                .collect();
            literals::of_values(LiteralBase::Number, values, types)
        }
        (Key::Number, None) => {
            let members = tuple
                .elements
                .iter()
                .chain(tuple.rest.iter())
                .copied()
                .collect();
            union_of(members, types)
        }
        (Key::Name(name), None) => Err(type_error(format!(
            "property {name} does not exist on a tuple type"
        ))),
        _ => Err(unsupported("indexed access of a tuple by a string key")),
    }
}

/// `A & B`: object types merge their properties (a property in both
/// intersects its types), literal sets intersect their base, `unknown` is
/// the identity and a union distributes. Two function types would make an
/// overloaded value, which has no single static signature.
pub(super) fn intersection(
    intersection: &ast::TsIntersectionType,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let mut result = Type::Unknown;
    for member in &intersection.types {
        let member = keyword_type(member, types)?;
        result = intersect(result, member, types)?;
    }
    Ok(result)
}

pub(super) fn intersect(a: Type, b: Type, types: &mut ir::ValueTypes) -> Result<Type> {
    if a == b || b == Type::Unknown {
        return Ok(a);
    }
    if a == Type::Unknown {
        return Ok(b);
    }
    if a == Type::Never || b == Type::Never {
        return Ok(Type::Never);
    }
    let left = split_members(a, types);
    let right = split_members(b, types);
    if left.len() > 1 || right.len() > 1 {
        let mut results = Vec::new();
        for x in &left {
            for y in &right {
                results.push(intersect(*x, *y, types)?);
            }
        }
        return union_of(results, types);
    }
    match (a, b) {
        (Type::Record(x), Type::Record(y))
            if types.records[x as usize].class.is_none()
                && types.records[y as usize].class.is_none() =>
        {
            merge_records(x, y, types)
        }
        (Type::Function(_), Type::Function(_)) => Err(unsupported(
            "intersection of function types (an overloaded function value has no single static signature)",
        )),
        _ if extends(a, b, types) => Ok(a),
        _ if extends(b, a, types) => Ok(b),
        (
            Type::Literal(..)
            | Type::String
            | Type::Number
            | Type::Float64
            | Type::Boolean
            | Type::Null
            | Type::Undefined
            | Type::BigInt,
            Type::Literal(..)
            | Type::String
            | Type::Number
            | Type::Float64
            | Type::Boolean
            | Type::Null
            | Type::Undefined
            | Type::BigInt,
        ) => {
            // Disjoint primitives.
            Ok(Type::Never)
        }
        (
            Type::String
            | Type::Number
            | Type::Float64
            | Type::Boolean
            | Type::BigInt
            | Type::Literal(..),
            Type::Record(_),
        )
        | (
            Type::Record(_),
            Type::String
            | Type::Number
            | Type::Float64
            | Type::Boolean
            | Type::BigInt
            | Type::Literal(..),
        ) => Err(unsupported(
            "intersection of a primitive with an object type (a branded primitive has no value with that property)",
        )),
        _ => Err(unsupported(&format!(
            "intersection of {} and {} (no closed type has the members of both)",
            subtyping::describe(a, types),
            subtyping::describe(b, types)
        ))),
    }
}

/// Distribution members of an intersection operand: union members and the
/// values of a literal set of more than one value.
fn split_members(ty: Type, types: &mut ir::ValueTypes) -> Vec<Type> {
    let members = unions::members(ty, types);
    if members.len() > 1 {
        return members;
    }
    match ty {
        Type::Literal(..) => split(ty, types),
        _ => vec![ty],
    }
}

fn merge_records(x: u32, y: u32, types: &mut ir::ValueTypes) -> Result<Type> {
    let mut fields: Vec<RecordField> = types.records[x as usize].fields.clone();
    for field in types.records[y as usize].fields.clone() {
        match fields.iter().position(|f| f.name == field.name) {
            None => fields.push(field),
            Some(at) => {
                let existing = fields[at].clone();
                let optional = existing.optional && field.optional;
                let (a, b) = if optional {
                    (existing.ty, field.ty)
                } else {
                    (
                        required_type(&existing, types)?,
                        required_type(&field, types)?,
                    )
                };
                let ty = intersect(a, b, types)?;
                if ty == Type::Never {
                    return Err(unsupported(&format!(
                        "intersection whose property {} has no common type (the object type would be never)",
                        field.name
                    )));
                }
                fields[at] = RecordField {
                    name: field.name,
                    ty,
                    optional,
                };
            }
        }
    }
    records::intern(fields, types)
}

/// A field's type as a required property (an optional one sheds `undefined`).
fn required_type(field: &RecordField, types: &mut ir::ValueTypes) -> Result<Type> {
    if field.optional {
        tuple_types::strip_undefined(field.ty, types)
    } else {
        Ok(field.ty)
    }
}

impl Lower<'_> {
    /// `object[key]` for a record and a computed key. A key whose static
    /// type is a string literal set of the record's fields reads with the
    /// precise indexed access type `T[K]` (the union of those fields'
    /// types); any other string key is the dynamic read, which may miss.
    pub(in crate::lower) fn keyed_record_read(
        &mut self,
        span: swc_common::Span,
        value: Expr,
        key: Expr,
        shape: u32,
    ) -> Result<Expr> {
        let precise = literals::string_members(key.ty, self.types).and_then(|names| {
            let fields = &self.types.records[shape as usize].fields;
            names
                .iter()
                .map(|name| {
                    fields
                        .iter()
                        .find(|field| field.name == *name)
                        .map(|f| f.ty)
                })
                .collect::<Option<Vec<Type>>>()
        });
        let key = literals::erase(key, self.types);
        if key.ty != Type::String {
            return Err(self.source.locate(
                type_error(
                    "computed record key must be a narrowed string; coercion is unsupported",
                ),
                key.span,
            ));
        }
        let read = self.dynamic_record_read(span, value, key, shape)?;
        let Some(members) = precise else {
            return Ok(read);
        };
        let narrow = union_of(members, self.types).map_err(|e| self.source.locate(e, span))?;
        if narrow == read.ty {
            return Ok(read);
        }
        self.checked_narrow(read, narrow, "record key outside its literal key type")
    }
}
