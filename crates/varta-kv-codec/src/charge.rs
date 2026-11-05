//! The v38 fair fuel of KV work (`varta_common::fuel`), beyond the call
//! charge and the descriptor and result bytes every host import pays.
use varta_common::{fuel, json::JsonSchema};

/// Staging one key or value: its encoded key and value bytes.
pub fn staged(bytes: usize) -> u64 {
    fuel::work(bytes, fuel::KV_BYTE)
}

/// A read's entries: each entry and its key and value bytes.
pub fn returned(entries: impl Iterator<Item = usize>) -> u64 {
    entries.fold(0, |total: u64, bytes| {
        total
            .saturating_add(fuel::KV_ENTRY)
            .saturating_add(fuel::work(bytes, fuel::KV_BYTE))
    })
}

/// A commit: the operation and the key and value bytes it writes (the keys
/// of its checks and its mutations, and its values).
pub fn committed(bytes: usize) -> u64 {
    fuel::KV_OPERATION.saturating_add(fuel::work(bytes, fuel::KV_BYTE))
}

/// Decoding an entry's key or value into `schema`: the schema's types and
/// fields and the values decoded, at the checked JSON rates (its bytes were
/// charged when the read returned them). A decode pays [`schema`] before it
/// classifies and [`values`] as it classifies.
pub fn decoded(schema: &JsonSchema, values: usize) -> u64 {
    self::schema(schema).saturating_add(self::values(values))
}

/// The schema's types and fields a decode reads and checks.
pub fn schema(schema: &JsonSchema) -> u64 {
    fuel::work(schema.types.len(), fuel::JSON_SCHEMA_TYPE)
        .saturating_add(fuel::work(schema.fields(), fuel::JSON_SCHEMA_FIELD))
}

/// `values` decoded values.
pub fn values(values: usize) -> u64 {
    fuel::work(values, fuel::JSON_PARSE_VALUE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kv_charges_follow_the_published_rates() {
        assert_eq!(staged(0), 0);
        assert_eq!(staged(10), 10 * fuel::KV_BYTE);
        assert_eq!(returned([].into_iter()), 0);
        assert_eq!(
            returned([3, 7].into_iter()),
            2 * fuel::KV_ENTRY + 10 * fuel::KV_BYTE
        );
        assert_eq!(committed(0), fuel::KV_OPERATION);
        assert_eq!(committed(5), fuel::KV_OPERATION + 5 * fuel::KV_BYTE);
        assert_eq!(staged(usize::MAX), u64::MAX, "saturating");
        assert_eq!(returned([usize::MAX, 1].into_iter()), u64::MAX);
    }

    /// A decode pays its schema, then its values as it goes: together the
    /// published decode charge.
    #[test]
    fn a_decode_pays_its_schema_and_values() {
        let null = JsonSchema {
            version: varta_common::json::HASHED_DICTIONARY_SCHEMA_VERSION,
            root: 0,
            types: alloc::vec![varta_common::json::JsonType::Null],
        };
        assert_eq!(schema(&null), fuel::JSON_SCHEMA_TYPE);
        assert_eq!(values(3), 3 * fuel::JSON_PARSE_VALUE);
        assert_eq!(decoded(&null, 3), schema(&null) + values(3));
    }
}
