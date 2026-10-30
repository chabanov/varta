//! The reference classification of recoverable checked parses over UTF-8:
//! grammar and whole-input budgets precede checked schema decoding.
//! Classification uses typed limit state, never matching or parsing a
//! decoder's error message. The host imports run the shared UTF-16 decoder
//! ([`super::scan`]), whose differential tests compare it with this one.
use super::*;
use crate::limits::DataLimits;
use serde::{
    Deserialize,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
};
use serde_json::value::RawValue;
use std::{cell::Cell, fmt, time::Instant};

impl JsonSchema {
    /// Recoverable syntax/type classification with fatal, schema-independent budgets.
    /// Syntax locations are serde_json's one-based lines/byte columns in UTF-8 input.
    pub fn parse_json_classified(&self, bytes: &[u8]) -> Result<JsonValue, JsonDecodeError> {
        self.classify(bytes, (DataLimits::LEGACY, &crate::limits::FROZEN), None)
    }
    /// [`Self::parse_json_classified_limited`] whose host work stops with
    /// [`JsonInputLimit::Deadline`] once `deadline` passes.
    pub fn parse_json_classified_until(
        &self,
        bytes: &[u8],
        limits: DataLimits,
        deadline: Instant,
    ) -> Result<JsonValue, JsonDecodeError> {
        self.classify(bytes, (limits, &crate::limits::MAXIMUM), Some(deadline))
    }
    /// [`Self::parse_json_classified`] under a profile's data limits (v38
    /// `data_limits`): input bytes, depth, visited values, array and
    /// dictionary entries and string bytes.
    pub fn parse_json_classified_limited(
        &self,
        bytes: &[u8],
        limits: DataLimits,
    ) -> Result<JsonValue, JsonDecodeError> {
        self.classify(bytes, (limits, &crate::limits::MAXIMUM), None)
    }
    fn classify(
        &self,
        bytes: &[u8],
        (limits, structural): (DataLimits, &crate::limits::ProgramLimits),
        deadline: Option<Instant>,
    ) -> Result<JsonValue, JsonDecodeError> {
        // As in `decode_with_limits`.
        self.validate_within(structural)?;
        if bytes.len() > limits.json_input_bytes {
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
        if let Err(error) = walk(raw, 0, &mut nodes, &limit, (limits, deadline)) {
            return Err(if let Some(limit) = limit.get() {
                JsonDecodeError::Limit(limit)
            } else {
                syntax(error)
            });
        }
        self.decode_with_limits(bytes, &limit, (limits, structural), deadline)
            .map_err(|_| {
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
    (limits, deadline): (DataLimits, Option<Instant>),
) -> Result<(), serde_json::Error> {
    if depth > limits.json_depth {
        return Err(exhausted(limit, JsonInputLimit::Depth));
    }
    *nodes += 1;
    if super::decode::overdue(*nodes, deadline) {
        return Err(exhausted(limit, JsonInputLimit::Deadline));
    }
    if *nodes > limits.json_nodes {
        return Err(exhausted(limit, JsonInputLimit::Nodes));
    }
    let mut decoder = serde_json::Deserializer::from_str(raw.get());
    let state = Budget {
        depth,
        nodes,
        limit,
        limits,
        deadline,
    };
    match raw.get().as_bytes().first() {
        Some(b'{') => de::Deserializer::deserialize_map(&mut decoder, state),
        Some(b'[') => de::Deserializer::deserialize_seq(&mut decoder, state),
        Some(b'"') => TextBudget(limit, limits).deserialize(&mut decoder),
        _ => Ok(()),
    }
}
struct Budget<'a> {
    depth: usize,
    nodes: &'a mut usize,
    limit: &'a Cell<Option<JsonInputLimit>>,
    limits: DataLimits,
    deadline: Option<Instant>,
}
impl<'de> DeserializeSeed<'de> for Budget<'_> {
    type Value = ();
    fn deserialize<D: de::Deserializer<'de>>(self, decoder: D) -> Result<(), D::Error> {
        let raw = <&RawValue>::deserialize(decoder)?;
        walk(
            raw,
            self.depth,
            self.nodes,
            self.limit,
            (self.limits, self.deadline),
        )
        .map_err(de::Error::custom)
    }
}
impl<'de> Visitor<'de> for Budget<'_> {
    type Value = ();
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded JSON input")
    }
    fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<(), M::Error> {
        while map
            .next_key_seed(TextBudget(self.limit, self.limits))?
            .is_some()
        {
            map.next_value_seed(Budget {
                depth: self.depth + 1,
                nodes: self.nodes,
                limit: self.limit,
                limits: self.limits,
                deadline: self.deadline,
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
                limits: self.limits,
                deadline: self.deadline,
            })?
            .is_some()
        {
            count += 1;
            if count > self.limits.array_elements {
                return Err(exhausted(self.limit, JsonInputLimit::ArrayElements));
            }
        }
        Ok(())
    }
}
struct TextBudget<'a>(&'a Cell<Option<JsonInputLimit>>, DataLimits);
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
        if units.len() > self.1.string_bytes / 2 {
            return Err(exhausted(self.0, JsonInputLimit::StringBytes));
        }
        Ok(())
    }
}
