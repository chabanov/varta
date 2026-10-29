use super::*;
use crate::{abi, value::*};
use serde::Deserialize;
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use std::{cell::Cell, fmt};

impl JsonSchema {
    pub fn parse_json(&self, bytes: &[u8]) -> Result<JsonValue, CheckedJsonError> {
        self.decode_with_limits(bytes, &Cell::new(None))
    }
    pub(super) fn decode_with_limits(
        &self,
        bytes: &[u8],
        limit: &Cell<Option<JsonInputLimit>>,
    ) -> Result<JsonValue, CheckedJsonError> {
        self.validate()?;
        if bytes.len() > abi::MAX_INPUT_BYTES {
            limit.set(Some(JsonInputLimit::Bytes));
            return Err(CheckedJsonError::Input("byte limit".into()));
        }
        if self.version >= UTF16_SCHEMA_VERSION && std::str::from_utf8(bytes).is_err() {
            return Err(CheckedJsonError::Input(
                "invalid UTF-8 JSON transport".into(),
            ));
        }
        let mut deserializer = serde_json::Deserializer::from_slice(bytes);
        let mut nodes = 0;
        let value = Seed {
            limit,
            schema: self,
            ty: self.root,
            nodes: &mut nodes,
            depth: 0,
        }
        .deserialize(&mut deserializer)
        .map_err(|e| CheckedJsonError::Input(e.to_string()))?;
        deserializer
            .end()
            .map_err(|e| CheckedJsonError::Input(e.to_string()))?;
        Ok(value)
    }
}
struct Seed<'a> {
    limit: &'a Cell<Option<JsonInputLimit>>,
    schema: &'a JsonSchema,
    ty: u32,
    nodes: &'a mut usize,
    depth: usize,
}
impl Seed<'_> {
    fn expected<E: de::Error>(&self, kind: ValueKind) -> Result<(), E> {
        if self.schema.types[self.ty as usize].kind() == kind {
            Ok(())
        } else {
            Err(E::custom("value type does not match schema"))
        }
    }
}
impl<'de> DeserializeSeed<'de> for Seed<'_> {
    type Value = JsonValue;
    fn deserialize<D: de::Deserializer<'de>>(self, de: D) -> Result<Self::Value, D::Error> {
        if let JsonType::PrimitiveUnion { members } | JsonType::ReferenceUnion { members } =
            &self.schema.types[self.ty as usize]
        {
            // Capture only this bounded token, then decode its unique JSON-kind
            // member. No speculative shape matching or lost duplicate keys.
            let raw = <&serde_json::value::RawValue>::deserialize(de)?;
            let kind = match raw.get().as_bytes().first() {
                Some(b'n') => ValueKind::Null,
                Some(b't' | b'f') => ValueKind::Boolean,
                Some(b'"') => ValueKind::String,
                Some(b'{') => ValueKind::Record,
                Some(b'[') => ValueKind::Array,
                _ => ValueKind::Float64,
            };
            let member = members
                .iter()
                .find(|m| {
                    let member = self.schema.types[**m as usize].kind();
                    member == kind || kind == ValueKind::Record && member == ValueKind::Dictionary
                })
                .ok_or_else(|| de::Error::custom("value type does not match union schema"))?;
            let mut decoder = serde_json::Deserializer::from_str(raw.get());
            let value = Seed {
                limit: self.limit,
                schema: self.schema,
                ty: *member,
                nodes: self.nodes,
                depth: self.depth,
            }
            .deserialize(&mut decoder)
            .map_err(de::Error::custom)?;
            decoder.end().map_err(de::Error::custom)?;
            return Ok(JsonValue::Union {
                ty: self.ty,
                value: Box::new(value),
            });
        }
        *self.nodes += 1;
        if self.depth > MAX_JSON_DEPTH || *self.nodes > MAX_JSON_NODES {
            self.limit.set(Some(if self.depth > MAX_JSON_DEPTH {
                JsonInputLimit::Depth
            } else {
                JsonInputLimit::Nodes
            }));
            return Err(de::Error::custom("depth or node limit"));
        }
        if self.schema.version >= UTF16_SCHEMA_VERSION
            && matches!(self.schema.types[self.ty as usize], JsonType::String)
        {
            de.deserialize_bytes(self)
        } else {
            de.deserialize_any(self)
        }
    }
}
impl Seed<'_> {
    /// Decode a dynamic-key object: distinct keys (after escape decoding),
    /// bounded entries, own-key order with array indices first.
    fn dictionary<'de, M: MapAccess<'de>>(
        self,
        mut map: M,
        value: u32,
    ) -> Result<JsonValue, M::Error> {
        let mut entries: Vec<(Vec<u16>, JsonValue)> = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        while let Some(key) = map.next_key::<KeySeed>()? {
            let KeySeed(units) = key;
            if units.len() > abi::MAX_STRING_BYTES / 2 {
                self.limit.set(Some(JsonInputLimit::StringBytes));
                return Err(de::Error::custom("string byte limit"));
            }
            if entries.len() == MAX_DICTIONARY_ENTRIES {
                self.limit.set(Some(JsonInputLimit::DictionaryEntries));
                return Err(de::Error::custom("dictionary entry limit"));
            }
            if !seen.insert(units.clone()) {
                return Err(de::Error::custom("duplicate object field"));
            }
            let entry = map.next_value_seed(Seed {
                limit: self.limit,
                schema: self.schema,
                ty: value,
                nodes: self.nodes,
                depth: self.depth + 1,
            })?;
            entries.push((units, entry));
        }
        // Stable: array-index keys ascend, other keys keep insertion order.
        entries.sort_by_key(|(key, _)| match array_index(key) {
            Some(index) => (false, index),
            None => (true, 0),
        });
        Ok(JsonValue::Dictionary {
            ty: self.ty,
            entries,
        })
    }
}
/// A JSON object key as exact UTF-16 units, including escaped lone surrogates.
struct KeySeed(Vec<u16>);
impl<'de> Deserialize<'de> for KeySeed {
    fn deserialize<D: de::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct Units;
        impl Visitor<'_> for Units {
            type Value = KeySeed;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an object key")
            }
            fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<KeySeed, E> {
                crate::text::from_wtf8(bytes)
                    .map(KeySeed)
                    .map_err(E::custom)
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<KeySeed, E> {
                Ok(KeySeed(value.encode_utf16().collect()))
            }
        }
        de.deserialize_bytes(Units)
    }
}
impl<'de> Visitor<'de> for Seed<'_> {
    type Value = JsonValue;
    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a value matching the checked JSON schema")
    }
    fn visit_unit<E: de::Error>(self) -> Result<JsonValue, E> {
        self.expected(ValueKind::Null)?;
        Ok(JsonValue::Null)
    }
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<JsonValue, E> {
        self.expected(ValueKind::Boolean)?;
        Ok(JsonValue::Boolean(value))
    }
    fn visit_i64<E: de::Error>(self, value: i64) -> Result<JsonValue, E> {
        if self.schema.types[self.ty as usize].kind() == ValueKind::Float64 {
            return Ok(JsonValue::Float64((value as f64).to_bits()));
        }
        self.expected(ValueKind::I32)?;
        i32::try_from(value)
            .map(JsonValue::I32)
            .map_err(|_| E::custom("i32 range"))
    }
    fn visit_u64<E: de::Error>(self, value: u64) -> Result<JsonValue, E> {
        if self.schema.types[self.ty as usize].kind() == ValueKind::Float64 {
            return Ok(JsonValue::Float64((value as f64).to_bits()));
        }
        self.expected(ValueKind::I32)?;
        i32::try_from(value)
            .map(JsonValue::I32)
            .map_err(|_| E::custom("i32 range"))
    }
    fn visit_f64<E: de::Error>(self, value: f64) -> Result<JsonValue, E> {
        if self.schema.types[self.ty as usize].kind() == ValueKind::Float64 && value.is_finite() {
            return Ok(JsonValue::Float64(value.to_bits()));
        }
        Err(E::custom(
            "numbers require plain signed i32 integer notation",
        ))
    }
    fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<JsonValue, E> {
        self.expected(ValueKind::String)?;
        let units = crate::text::from_wtf8(bytes).map_err(E::custom)?;
        if units.len() > abi::MAX_STRING_BYTES / 2 {
            self.limit.set(Some(JsonInputLimit::StringBytes));
            return Err(E::custom("string byte limit"));
        }
        Ok(JsonValue::String16(units))
    }
    fn visit_str<E: de::Error>(self, value: &str) -> Result<JsonValue, E> {
        self.expected(ValueKind::String)?;
        if value.len() > abi::MAX_STRING_BYTES {
            self.limit.set(Some(JsonInputLimit::StringBytes));
            return Err(E::custom("string byte limit"));
        }
        Ok(JsonValue::String(value.into()))
    }
    fn visit_string<E: de::Error>(self, value: String) -> Result<JsonValue, E> {
        self.expected(ValueKind::String)?;
        if value.len() > abi::MAX_STRING_BYTES {
            self.limit.set(Some(JsonInputLimit::StringBytes));
            return Err(E::custom("string byte limit"));
        }
        Ok(JsonValue::String(value))
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<JsonValue, M::Error> {
        if let JsonType::Dictionary { value, .. } = self.schema.types[self.ty as usize] {
            return self.dictionary(map, value);
        }
        let JsonType::Record { fields, .. } = &self.schema.types[self.ty as usize] else {
            return Err(de::Error::custom("object does not match schema"));
        };
        let mut values: Vec<Option<JsonValue>> = (0..fields.len()).map(|_| None).collect();
        while let Some(name) = map.next_key::<String>()? {
            let index = fields
                .binary_search_by(|f| f.name.cmp(&name))
                .map_err(|_| de::Error::custom("unknown object field"))?;
            if values[index].is_some() {
                return Err(de::Error::custom("duplicate object field"));
            }
            values[index] = Some(map.next_value_seed(Seed {
                limit: self.limit,
                schema: self.schema,
                ty: fields[index].ty,
                nodes: self.nodes,
                depth: self.depth + 1,
            })?);
        }
        let fields = values
            .into_iter()
            .enumerate()
            .map(|(slot, v)| match v {
                Some(value) => Ok(value),
                None if fields[slot].optional => {
                    *self.nodes += 1;
                    if *self.nodes > MAX_JSON_NODES {
                        self.limit.set(Some(JsonInputLimit::Nodes));
                        return Err(de::Error::custom("node limit"));
                    }
                    let ty = fields[slot].ty;
                    Ok(match self.schema.types[ty as usize] {
                        JsonType::Undefined => JsonValue::Undefined,
                        _ => JsonValue::Union {
                            ty,
                            value: Box::new(JsonValue::Undefined),
                        },
                    })
                }
                None => Err(de::Error::custom("missing object field")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(JsonValue::Record {
            ty: self.ty,
            fields,
        })
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<JsonValue, S::Error> {
        let JsonType::Array { element } = self.schema.types[self.ty as usize] else {
            return Err(de::Error::custom("array does not match schema"));
        };
        let mut values = Vec::new();
        while let Some(value) = seq.next_element_seed(Seed {
            limit: self.limit,
            schema: self.schema,
            ty: element,
            nodes: self.nodes,
            depth: self.depth + 1,
        })? {
            if values.len() == MAX_ARRAY_ELEMENTS {
                self.limit.set(Some(JsonInputLimit::ArrayElements));
                return Err(de::Error::custom("array element limit"));
            }
            values.push(value);
        }
        Ok(JsonValue::Array {
            ty: self.ty,
            values,
        })
    }
}
