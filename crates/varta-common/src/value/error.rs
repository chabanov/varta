//! First Error representation: immutable default name and optional own message.
use super::*;
use crate::abi;
#[derive(Debug, Clone, Copy)]
pub struct ErrorObject {
    pub class: ErrorClass,
    pub message: u64,
    pub has_message: bool,
}
impl ErrorObject {
    pub fn read(data: &[u8], handle: u64) -> Result<Self, ValueAbiError> {
        Self::read_with_version(data, handle, ERROR_VALUE_VERSION)
    }
    pub fn read_with_version(
        data: &[u8],
        handle: u64,
        version: u32,
    ) -> Result<Self, ValueAbiError> {
        let invalid = || ValueAbiError("noncanonical Error object".into());
        let ptr = handle as u32 as usize;
        if (handle >> i32::BITS) as usize != ERROR_BYTES
            || ptr < abi::OWNED_STRING_BASE + abi::STRING_HEADER_BYTES
            || !ptr.is_multiple_of(HEAP_ALIGNMENT)
        {
            return Err(invalid());
        }
        let header = ptr - abi::STRING_HEADER_BYTES;
        let word = |at: usize| {
            data.get(at..at.checked_add(4)?)
                .and_then(|s| s.try_into().ok())
                .map(u32::from_le_bytes)
        };
        let size = word(header).ok_or_else(invalid)? as usize;
        let flags = word(ptr).ok_or_else(invalid)?;
        let class = ErrorClass::from_tag(flags >> ERROR_CLASS_SHIFT).ok_or_else(invalid)?;
        if !matches!(
            version,
            ERROR_VALUE_VERSION
                | ERROR_FAMILY_VALUE_VERSION
                | SPARSE_VALUE_VERSION
                | BIGINT_VALUE_VERSION
                | FUNCTION_VALUE_VERSION
        ) || (version == ERROR_VALUE_VERSION && class != ErrorClass::Error)
        {
            return Err(invalid());
        }
        let own = flags & ERROR_OWN_MESSAGE;
        if size < abi::STRING_HEADER_BYTES + ERROR_BYTES
            || !size.is_multiple_of(HEAP_ALIGNMENT)
            || header.checked_add(size).is_none_or(|end| end > data.len())
            || word(header + HEAP_FLAGS_OFFSET)
                != Some(HEAP_ALLOCATED_FLAG | HEAP_ERROR_FLAG as u32)
            || word(header + HEAP_LINK_OFFSET) != Some(1)
            || word(ptr + 4) != Some(1)
        {
            return Err(invalid());
        }
        let message = data
            .get(ptr + RECORD_META_BYTES..ptr + ERROR_BYTES)
            .and_then(|s| s.try_into().ok())
            .map(u64::from_le_bytes)
            .ok_or_else(invalid)?;
        let start = message as u32 as usize;
        let len = (message >> i32::BITS) as usize;
        if len > abi::MAX_STRING_BYTES
            || !len.is_multiple_of(2)
            || start.checked_add(len).is_none_or(|end| end > data.len())
            || (own == 0 && message != 0)
        {
            return Err(invalid());
        }
        Ok(Self {
            class,
            message,
            has_message: own != 0,
        })
    }
}

/// Closed built-in identities; arbitrary subclasses remain a separate feature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ErrorClass {
    Error = 0,
    TypeError = 1,
    RangeError = 2,
    SyntaxError = 3,
}
impl ErrorClass {
    pub const ALL: &[Self] = &[
        Self::Error,
        Self::TypeError,
        Self::RangeError,
        Self::SyntaxError,
    ];
    #[must_use]
    pub fn from_tag(tag: u32) -> Option<Self> {
        Self::ALL.iter().copied().find(|class| *class as u32 == tag)
    }
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|class| class.name() == name)
    }
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Error => "Error",
            Self::TypeError => "TypeError",
            Self::RangeError => "RangeError",
            Self::SyntaxError => "SyntaxError",
        }
    }
}
