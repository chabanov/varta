//! Strict decoding of a stored V8 value or a key into the checked schema the
//! module names (`kv_value`, `kv_key`): the same closed types, unions and
//! limits as checked JSON parsing, so `get<T>` proves the whole stored graph.
//!
//! An object fills a record (every property a declared field, every missing
//! field optional) or a dictionary; a number fills a number (`-0`, NaN and the
//! infinities included, which JSON cannot carry); `undefined` fills an
//! optional or `undefined`-admitting position; arrays must have no holes. A
//! union selects its unique member of the value's kind, exactly as checked
//! JSON does. A `Date`, `Map`, `Set`, `Uint8Array` or BigInt has no checked
//! schema yet and fails, like every mismatch, with a recoverable `TypeError`.
use super::v8::{Key, Object, Value};
use crate::Deadline;
use alloc::{
    boxed::Box,
    format,
    rc::Rc,
    string::{String, ToString},
    vec::Vec,
};
use varta_common::{
    json::{JsonSchema, JsonType, JsonValue},
    limits::DataLimits,
    value::{ValueKind, array_index},
};

#[derive(Debug)]
pub enum DecodeError {
    /// The stored value does not match the type: a recoverable `TypeError`.
    Mismatch(String),
    /// The turn deadline passed while classifying.
    Deadline,
}

fn mismatch(message: impl Into<String>) -> DecodeError {
    DecodeError::Mismatch(message.into())
}

/// The kind of a V8 value, as checked JSON names the kind of a JSON token.
fn kind(value: &Value) -> Result<ValueKind, DecodeError> {
    Ok(match value {
        Value::Undefined => ValueKind::Undefined,
        Value::Null => ValueKind::Null,
        Value::Boolean(_) => ValueKind::Boolean,
        Value::Number(_) => ValueKind::Float64,
        Value::String(_) => ValueKind::String,
        Value::BigInt(_) => ValueKind::BigInt,
        Value::Object(object) => match **object {
            Object::Plain(_) => ValueKind::Record,
            Object::Dense(_) | Object::Sparse { .. } => ValueKind::Array,
            _ => return Err(unreadable(object.kind())),
        },
    })
}

fn unreadable(kind: &str) -> DecodeError {
    mismatch(format!(
        "a stored {kind} has no checked type yet and cannot be read"
    ))
}

pub struct Classifier<'a, D: Deadline + ?Sized> {
    schema: &'a JsonSchema,
    limits: DataLimits,
    deadline: &'a D,
    nodes: usize,
}

impl<'a, D: Deadline + ?Sized> Classifier<'a, D> {
    pub fn new(schema: &'a JsonSchema, limits: DataLimits, deadline: &'a D) -> Self {
        Self {
            schema,
            limits,
            deadline,
            nodes: 0,
        }
    }

    fn visit(&mut self, depth: usize) -> Result<(), DecodeError> {
        self.nodes += 1;
        if self.nodes.is_multiple_of(crate::DEADLINE_STRIDE) && self.deadline.passed() {
            return Err(DecodeError::Deadline);
        }
        if depth > self.limits.json_depth || self.nodes > self.limits.json_nodes {
            return Err(mismatch("the stored value exceeds the depth or node limit"));
        }
        Ok(())
    }

    /// The values visited so far (the decoded values of a finished root).
    pub fn values(&self) -> usize {
        self.nodes
    }

    /// The root value at the schema's root type.
    pub fn root(&mut self, value: &Value) -> Result<JsonValue, DecodeError> {
        self.classify(value, self.schema.root, 0)
    }

    fn classify(&mut self, value: &Value, ty: u32, depth: usize) -> Result<JsonValue, DecodeError> {
        // The shared KV depth counts the stored values, as the stage walk
        // and the V8 codec do; the `undefined` of a missing optional field
        // stores nothing and is not counted here.
        if depth > crate::MAX_VALUE_DEPTH {
            return Err(mismatch(crate::too_deep()));
        }
        let schema = self.schema;
        let expected = &schema.types[ty as usize];
        if let JsonType::PrimitiveUnion { members } | JsonType::ReferenceUnion { members } =
            expected
        {
            let kind = kind(value)?;
            let member = members
                .iter()
                .copied()
                .find(|member| {
                    let member = schema.types[*member as usize].kind();
                    member == kind
                        || kind == ValueKind::Record && member == ValueKind::Dictionary
                        || kind == ValueKind::Float64 && member == ValueKind::I32
                })
                .ok_or_else(|| match value {
                    Value::BigInt(_) => unreadable("BigInt"),
                    _ => mismatch("the stored value matches no member of the union"),
                })?;
            return Ok(JsonValue::Union {
                ty,
                value: Box::new(self.classify(value, member, depth)?),
            });
        }
        self.visit(depth)?;
        Ok(match (expected, value) {
            (JsonType::Undefined, Value::Undefined) => JsonValue::Undefined,
            (JsonType::Null, Value::Null) => JsonValue::Null,
            (JsonType::Boolean, Value::Boolean(value)) => JsonValue::Boolean(*value),
            (JsonType::Float64, Value::Number(number)) => JsonValue::Float64(number.to_bits()),
            (JsonType::I32, Value::Number(number))
                if crate::integral(*number)
                    && *number >= f64::from(i32::MIN)
                    && *number <= f64::from(i32::MAX)
                    && !(*number == 0.0 && number.is_sign_negative()) =>
            {
                JsonValue::I32(*number as i32)
            }
            (JsonType::String, Value::String(units)) => {
                if units.len() > self.limits.string_bytes / 2 {
                    return Err(mismatch("a stored string exceeds the string limit"));
                }
                JsonValue::String16(units.clone())
            }
            (JsonType::Record { fields, .. }, Value::Object(object)) => {
                let Object::Plain(properties) = &**object else {
                    return Err(self.kind_mismatch(value, ty));
                };
                self.record(properties, fields, ty, depth)?
            }
            (JsonType::Dictionary { value: element, .. }, Value::Object(object)) => {
                let Object::Plain(properties) = &**object else {
                    return Err(self.kind_mismatch(value, ty));
                };
                self.dictionary(properties, *element, ty, depth)?
            }
            (JsonType::Array { element }, Value::Object(object)) => {
                let values = match &**object {
                    Object::Dense(elements) => elements
                        .iter()
                        .map(|element| {
                            element
                                .as_ref()
                                .ok_or_else(|| mismatch("a stored array has holes"))
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    Object::Sparse { length, elements } if elements.len() == *length as usize => {
                        elements.iter().map(|(_, value)| value).collect()
                    }
                    Object::Sparse { .. } => return Err(mismatch("a stored array has holes")),
                    _ => return Err(self.kind_mismatch(value, ty)),
                };
                if values.len() > self.limits.array_elements {
                    return Err(mismatch("a stored array exceeds the element limit"));
                }
                let values = values
                    .into_iter()
                    .map(|value| self.classify(value, *element, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?;
                JsonValue::Array { ty, values }
            }
            _ => return Err(self.kind_mismatch(value, ty)),
        })
    }

    fn kind_mismatch(&self, value: &Value, ty: u32) -> DecodeError {
        match value {
            Value::Object(object) if !matches!(**object, Object::Plain(_)) => {
                if let Object::Dense(_) | Object::Sparse { .. } = **object {
                    mismatch(format!(
                        "a stored array does not match {:?}",
                        self.schema.types[ty as usize].kind()
                    ))
                } else {
                    unreadable(object.kind())
                }
            }
            Value::BigInt(_) => unreadable("BigInt"),
            _ => mismatch(format!(
                "a stored {:?} does not match {:?}",
                kind(value).unwrap_or(ValueKind::Record),
                self.schema.types[ty as usize].kind()
            )),
        }
    }

    fn record(
        &mut self,
        properties: &[(Key, Value)],
        fields: &[varta_common::json::JsonField],
        ty: u32,
        depth: usize,
    ) -> Result<JsonValue, DecodeError> {
        let mut values: Vec<Option<JsonValue>> = (0..fields.len()).map(|_| None).collect();
        for (key, value) in properties {
            let name = key_text(key)?;
            let slot = fields
                .binary_search_by(|field| field.name.as_str().cmp(&name))
                .map_err(|_| mismatch(format!("the stored object has the unknown field {name}")))?;
            values[slot] = Some(self.classify(value, fields[slot].ty, depth + 1)?);
        }
        let fields = values
            .into_iter()
            .enumerate()
            .map(|(slot, value)| match value {
                Some(value) => Ok(value),
                None if fields[slot].optional => {
                    self.visit(depth + 1)?;
                    let ty = fields[slot].ty;
                    Ok(match self.schema.types[ty as usize] {
                        JsonType::Undefined => JsonValue::Undefined,
                        _ => JsonValue::Union {
                            ty,
                            value: Box::new(JsonValue::Undefined),
                        },
                    })
                }
                None => Err(mismatch(format!(
                    "the stored object lacks the field {}",
                    fields[slot].name
                ))),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(JsonValue::Record { ty, fields })
    }

    fn dictionary(
        &mut self,
        properties: &[(Key, Value)],
        element: u32,
        ty: u32,
        depth: usize,
    ) -> Result<JsonValue, DecodeError> {
        if properties.len()
            > varta_common::value::dictionary_entry_limit(self.schema.value_version(), self.limits)
        {
            return Err(mismatch("a stored object exceeds the entry limit"));
        }
        let mut entries = Vec::with_capacity(properties.len());
        for (key, value) in properties {
            let units: Vec<u16> = key_text(key)?.encode_utf16().collect();
            entries.push((units, self.classify(value, element, depth + 1)?));
        }
        // Own-key order: array indices ascending, then insertion order.
        entries.sort_by_key(|(key, _)| match array_index(key) {
            Some(index) => (false, index),
            None => (true, 0),
        });
        Ok(JsonValue::Dictionary { ty, entries })
    }
}

/// A property key as field text; a key with a lone surrogate cannot name
/// a declared field or a dictionary key of the subset.
fn key_text(key: &Key) -> Result<String, DecodeError> {
    match key {
        Key::Index(index) => Ok(index.to_string()),
        Key::Name(units) => String::from_utf16(units)
            .map_err(|_| mismatch("a stored property key is not valid UTF-16")),
    }
}

/// A stored key as the V8 array value it is in JavaScript.
pub fn key_value(bytes: &[u8]) -> Result<Value, DecodeError> {
    use crate::key::Part;
    let parts = crate::key::decode(bytes).map_err(|_| mismatch("the stored key is malformed"))?;
    let parts = parts
        .into_iter()
        .map(|part| match part {
            Part::String(text) => Ok(Value::String(text.encode_utf16().collect())),
            Part::Float(number) => Ok(Value::Number(number)),
            Part::Int => Err(unreadable("BigInt key part")),
            Part::Bytes => Err(unreadable("Uint8Array key part")),
            Part::Boolean(_) => Err(mismatch(
                "a stored boolean key part is outside the string and number key parts",
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::Object(Rc::new(Object::Dense(
        parts.into_iter().map(Some).collect(),
    ))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use varta_common::json::HASHED_DICTIONARY_SCHEMA_VERSION;

    struct Never;
    impl Deadline for Never {
        fn passed(&self) -> bool {
            false
        }
    }

    /// A chain of `depth` single-element arrays around `null`.
    fn chain(depth: usize) -> Value {
        (0..depth).fold(Value::Null, |inner, _| {
            Value::Object(Rc::new(Object::Dense(vec![Some(inner)])))
        })
    }

    /// The classifier keeps the shared KV depth whatever the module's JSON
    /// depth (v38: far deeper), with the named recoverable text.
    #[test]
    fn classification_keeps_the_shared_depth_limit() {
        // `type Nested = Nested[] | null`: a union of an array of itself and null.
        let schema = JsonSchema {
            version: HASHED_DICTIONARY_SCHEMA_VERSION,
            root: 0,
            types: vec![
                JsonType::ReferenceUnion {
                    members: vec![1, 2],
                },
                JsonType::Array { element: 0 },
                JsonType::Null,
            ],
        };
        let limits = DataLimits::V38;
        assert!(limits.json_depth > crate::MAX_VALUE_DEPTH);
        let classify = |depth| Classifier::new(&schema, limits, &Never).root(&chain(depth));
        for depth in [crate::MAX_VALUE_DEPTH - 1, crate::MAX_VALUE_DEPTH] {
            assert!(classify(depth).is_ok(), "{depth}");
        }
        match classify(crate::MAX_VALUE_DEPTH + 1) {
            Err(DecodeError::Mismatch(message)) => assert_eq!(message, crate::too_deep()),
            other => panic!("{other:?}"),
        }
    }
}
