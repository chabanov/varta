//! The v38 fair fuel of KV work (`varta_common::fuel`), beyond the call
//! charge and the descriptor and result bytes every host import pays.
use varta_common::{fuel, json::JsonSchema};

/// Staging one key or value: its encoded key and value bytes.
pub(super) fn staged(bytes: usize) -> u64 {
    fuel::work(bytes, fuel::KV_BYTE)
}

/// A read's entries: each entry and its key and value bytes.
pub(super) fn returned(entries: impl Iterator<Item = usize>) -> u64 {
    entries.fold(0, |total: u64, bytes| {
        total
            .saturating_add(fuel::KV_ENTRY)
            .saturating_add(fuel::work(bytes, fuel::KV_BYTE))
    })
}

/// A commit: the operation and the key and value bytes it writes (the keys
/// of its checks and its mutations, and its values).
pub(super) fn committed(bytes: usize) -> u64 {
    fuel::KV_OPERATION.saturating_add(fuel::work(bytes, fuel::KV_BYTE))
}

/// Decoding an entry's key or value into `schema`: the schema's types and
/// fields and the values decoded, at the checked JSON rates (its bytes were
/// charged when the read returned them).
pub(super) fn decoded(schema: &JsonSchema, values: usize) -> u64 {
    fuel::work(schema.types.len(), fuel::JSON_SCHEMA_TYPE)
        .saturating_add(fuel::work(schema.fields(), fuel::JSON_SCHEMA_FIELD))
        .saturating_add(fuel::work(values, fuel::JSON_PARSE_VALUE))
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
}
