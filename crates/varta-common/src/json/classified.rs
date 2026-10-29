//! Grammar and whole-input budgets precede checked schema decoding. Classification
//! uses typed limit state, never matching or parsing a decoder's error message.
use super::*;
use crate::{abi, value::*};
use serde::{
    Deserialize,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};
use serde_json::value::RawValue;
use std::{cell::Cell, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum JsonInputLimit {
    #[error("input byte")]
    Bytes,
    #[error("depth")]
    Depth,
    #[error("node")]
    Nodes,
    #[error("array element")]
    ArrayElements,
    #[error("string byte")]
    StringBytes,
    /// Schema 15 dictionaries: more than `value::MAX_DICTIONARY_ENTRIES` keys.
    #[error("dictionary entry")]
    DictionaryEntries,
}
#[derive(Debug, thiserror::Error)]
pub enum JsonDecodeError {
    #[error(transparent)]
    Schema(#[from] CheckedJsonError),
    #[error("invalid JSON syntax at line {line} column {column}")]
    Syntax { line: usize, column: usize },
    #[error("JSON value does not match the checked schema")]
    Type,
    #[error("checked JSON {0} limit")]
    Limit(JsonInputLimit),
}
impl JsonSchema {
    /// Recoverable syntax/type classification with fatal, schema-independent budgets.
    /// Syntax locations are serde_json's one-based lines/byte columns in UTF-8 input.
    pub fn parse_json_classified(&self, bytes: &[u8]) -> Result<JsonValue, JsonDecodeError> {
        self.validate()?;
        if bytes.len() > abi::MAX_INPUT_BYTES {
            return Err(JsonDecodeError::Limit(JsonInputLimit::Bytes));
        }
        if std::str::from_utf8(bytes).is_err() {
            return Err(JsonDecodeError::Syntax { line: 1, column: 1 });
        }
        let mut decoder = serde_json::Deserializer::from_slice(bytes);
        // RawValue's iterative grammar scanner does not convert numbers or lose
        // escaped lone surrogates. Its work/storage are bounded by input bytes.
        let raw = <&RawValue>::deserialize(&mut decoder).map_err(syntax)?;
        decoder.end().map_err(syntax)?;
        let limit = Cell::new(None);
        let mut nodes = 0;
        if let Err(error) = walk(raw, 0, &mut nodes, &limit) {
            return Err(if let Some(limit) = limit.get() {
                JsonDecodeError::Limit(limit)
            } else {
                syntax(error)
            });
        }
        self.decode_with_limits(bytes, &limit).map_err(|_| {
            if let Some(limit) = limit.get() {
                JsonDecodeError::Limit(limit)
            } else {
                JsonDecodeError::Type
            }
        })
    }
}
fn syntax(error: serde_json::Error) -> JsonDecodeError {
    JsonDecodeError::Syntax {
        line: error.line(),
        column: error.column(),
    }
}
fn exhausted<E: de::Error>(state: &Cell<Option<JsonInputLimit>>, limit: JsonInputLimit) -> E {
    state.set(Some(limit));
    E::custom("checked JSON input limit")
}
fn walk(
    raw: &RawValue,
    depth: usize,
    nodes: &mut usize,
    limit: &Cell<Option<JsonInputLimit>>,
) -> Result<(), serde_json::Error> {
    if depth > MAX_JSON_DEPTH {
        return Err(exhausted(limit, JsonInputLimit::Depth));
    }
    *nodes += 1;
    if *nodes > MAX_JSON_NODES {
        return Err(exhausted(limit, JsonInputLimit::Nodes));
    }
    let mut decoder = serde_json::Deserializer::from_str(raw.get());
    let state = Budget {
        depth,
        nodes,
        limit,
    };
    match raw.get().as_bytes().first() {
        Some(b'{') => de::Deserializer::deserialize_map(&mut decoder, state),
        Some(b'[') => de::Deserializer::deserialize_seq(&mut decoder, state),
        Some(b'"') => TextBudget(limit).deserialize(&mut decoder),
        _ => Ok(()),
    }
}
struct Budget<'a> {
    depth: usize,
    nodes: &'a mut usize,
    limit: &'a Cell<Option<JsonInputLimit>>,
}
impl<'de> DeserializeSeed<'de> for Budget<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<(), D::Error> {
        let raw = <&RawValue>::deserialize(decoder)?;
        walk(raw, self.depth, self.nodes, self.limit).map_err(de::Error::custom)
    }
}
impl<'de> Visitor<'de> for Budget<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded JSON input")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<(), M::Error> {
        while map.next_key_seed(TextBudget(self.limit))?.is_some() {
            map.next_value_seed(Budget {
                depth: self.depth + 1,
                nodes: self.nodes,
                limit: self.limit,
            })?;
        }
        Ok(())
    }
    fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> Result<(), S::Error> {
        let mut count = 0;
        while seq
            .next_element_seed(Budget {
                depth: self.depth + 1,
                nodes: self.nodes,
                limit: self.limit,
            })?
            .is_some()
        {
            count += 1;
            if count > MAX_ARRAY_ELEMENTS {
                return Err(exhausted(self.limit, JsonInputLimit::ArrayElements));
            }
        }
        Ok(())
    }
}
struct TextBudget<'a>(&'a Cell<Option<JsonInputLimit>>);
impl<'de> DeserializeSeed<'de> for TextBudget<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<(), D::Error> {
        decoder.deserialize_bytes(self)
    }
}
impl<'de> Visitor<'de> for TextBudget<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded JSON string")
    }
    fn visit_bytes<E: de::Error>(self, bytes: &[u8]) -> Result<(), E> {
        let units = crate::text::from_wtf8(bytes).map_err(E::custom)?;
        if units.len() > abi::MAX_STRING_BYTES / 2 {
            return Err(exhausted(self.0, JsonInputLimit::StringBytes));
        }
        Ok(())
    }
}
