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

/// V38 linked decoder (`json_parse_v19` as linked code): the code a fatal
/// input limit stores in the module's `limits::RESOURCE_FAILURE` global
/// before it traps, above every `limits::Resource` code, so the runtime
/// reports exactly the host import's [`JsonDecodeError::Limit`].
pub const LINKED_LIMIT_BASE: i32 = 0x100;
impl JsonInputLimit {
    const ALL: [Self; 7] = [
        Self::Bytes,
        Self::Depth,
        Self::Nodes,
        Self::ArrayElements,
        Self::StringBytes,
        Self::DictionaryEntries,
        Self::Deadline,
    ];
    /// This limit's linked failure code.
    #[must_use]
    pub fn linked_code(self) -> i32 {
        let index = Self::ALL
            .iter()
            .position(|limit| *limit == self)
            .unwrap_or_default();
        LINKED_LIMIT_BASE + index as i32
    }
    /// The limit a linked failure code names.
    #[must_use]
    pub fn from_linked_code(code: i32) -> Option<Self> {
        let index = usize::try_from(code.checked_sub(LINKED_LIMIT_BASE)?).ok()?;
        Self::ALL.get(index).copied()
    }
}
