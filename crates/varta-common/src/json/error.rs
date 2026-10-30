//! Typed outcomes of checked JSON decoding, shared by the host imports and
//! the linked v38 decoder.
use super::CheckedJsonError;

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
    /// V38 (`data_limits`): the execution deadline passed while the host was
    /// still decoding.
    #[error("deadline")]
    Deadline,
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
