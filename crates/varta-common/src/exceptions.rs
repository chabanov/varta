//! Primitive guest exceptions. Fatal execution failures use a separate path.
use crate::{abi, text};
pub const PREFIX: &str = "varta.exception.";
pub const PENDING: &str = "varta.exception.pending";
pub const VERSION: &str = "varta.exception.version";
pub const ERROR_CHANNEL_VERSION: i32 = 2;
pub const FAMILY_VERSION: &str = "varta.exception.version_v3";
pub const FAMILY_CHANNEL_VERSION: i32 = 3;
pub const BIGINT_VERSION: &str = "varta.exception.version_v4";
pub const BIGINT_CHANNEL_VERSION: i32 = 4;
/// v32 `error_subclasses()`: the channel may also carry a class-instance record
/// whose descriptor declares String `name` and `message` fields.
pub const CLASS_ERROR_VERSION: &str = "varta.exception.version_v5";
pub const CLASS_ERROR_CHANNEL_VERSION: i32 = 5;
/// v34 value layout 15: channel v5 contents, every thrown cell, Error object
/// and class-instance record read as value15.
pub const DYNAMIC_VERSION: &str = "varta.exception.version_v6";
pub const DYNAMIC_CHANNEL_VERSION: i32 = 6;
/// v35 value layout 16: channel v6 contents, every thrown cell, Error object
/// and class-instance record read as value16. Promises are never thrown.
pub const PROMISE_VERSION: &str = "varta.exception.version_v7";
pub const PROMISE_CHANNEL_VERSION: i32 = 7;
/// v36 value layout 17: channel v7 contents, every thrown cell, Error object
/// and class-instance record read as value17 (whose records may hold function
/// fields). Function values themselves are never thrown.
pub const FUNCTION_FIELD_VERSION: &str = "varta.exception.version_v8";
pub const FUNCTION_FIELD_CHANNEL_VERSION: i32 = 8;
/// Channel v9 (v38 `dictionary_hashing`): the channel v8 contents in value18
/// layouts.
pub const HASHED_DICTIONARY_VERSION: &str = "varta.exception.version_v9";
pub const HASHED_DICTIONARY_CHANNEL_VERSION: i32 = 9;
pub const VALUE: &str = "varta.exception.value";
pub const ROOT: usize = abi::STRING_ROOT_BASE;
pub const RESERVED_ROOT_BYTES: usize = abi::STRING_ROOT_SLOT_BYTES;
pub const MAX_DIAGNOSTIC_UNITS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThrownValue {
    /// Owned canonical binary payload; diagnostics are bounded hexadecimal.
    BigInt(Vec<u8>),
    /// Raw binary64 bits preserve NaN and signed zero without a heap reference.
    Number(u64),
    Boolean(bool),
    String(Vec<u16>),
    Null,
    Undefined,
    NativeError {
        class: crate::value::ErrorClass,
        message: Vec<u16>,
        has_message: bool,
    },
    Error {
        message: Vec<u16>,
        has_message: bool,
    },
    /// Instance of a source class extending a native error (channel v5). The
    /// name and message are the instance's field values when it was thrown.
    ErrorInstance {
        name: Vec<u16>,
        message: Vec<u16>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("uncaught guest exception: {value}")]
pub struct UncaughtException {
    pub value: ThrownValue,
}
impl std::fmt::Display for ThrownValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BigInt(payload) => crate::bigint::write_diagnostic(f, payload),
            Self::Number(bits) => {
                let n = f64::from_bits(*bits);
                if n == f64::INFINITY {
                    f.write_str("Infinity")
                } else if n == f64::NEG_INFINITY {
                    f.write_str("-Infinity")
                } else {
                    write!(f, "{n}")
                }
            }
            Self::Boolean(b) => write!(f, "{b}"),
            Self::Null => f.write_str("null"),
            Self::Undefined => f.write_str("undefined"),
            Self::NativeError { class, message, .. } => write_error(f, class.name(), message),
            Self::Error { message, .. } => {
                f.write_str("Error")?;
                if !message.is_empty() {
                    f.write_str(": ")?;
                    write_units(f, message)?;
                }
                Ok(())
            }
            Self::ErrorInstance { name, message } => {
                // Error.prototype.toString: an empty part drops the separator.
                if name.is_empty() {
                    return write_units(f, message);
                }
                write_bare(f, name)?;
                if !message.is_empty() {
                    f.write_str(": ")?;
                    write_units(f, message)?;
                }
                Ok(())
            }
            Self::String(units) => write_units(f, units),
        }
    }
}
fn write_units(f: &mut std::fmt::Formatter<'_>, units: &[u16]) -> std::fmt::Result {
    let mut json = Vec::new();
    text::write_json(&mut json, &units[..units.len().min(MAX_DIAGNOSTIC_UNITS)])
        .map_err(|_| std::fmt::Error)?;
    f.write_str(std::str::from_utf8(&json).map_err(|_| std::fmt::Error)?)?;
    if units.len() > MAX_DIAGNOSTIC_UNITS {
        f.write_str("…")?;
    }
    Ok(())
}

/// Escaped, bounded guest text without the surrounding JSON quotes.
fn write_bare(f: &mut std::fmt::Formatter<'_>, units: &[u16]) -> std::fmt::Result {
    let mut json = Vec::new();
    text::write_json(&mut json, &units[..units.len().min(MAX_DIAGNOSTIC_UNITS)])
        .map_err(|_| std::fmt::Error)?;
    let text = std::str::from_utf8(&json).map_err(|_| std::fmt::Error)?;
    let text = text.strip_prefix('"').unwrap_or(text);
    f.write_str(text.strip_suffix('"').unwrap_or(text))?;
    if units.len() > MAX_DIAGNOSTIC_UNITS {
        f.write_str("…")?;
    }
    Ok(())
}

fn write_error(f: &mut std::fmt::Formatter<'_>, name: &str, message: &[u16]) -> std::fmt::Result {
    f.write_str(name)?;
    if !message.is_empty() {
        f.write_str(": ")?;
        write_units(f, message)?;
    }
    Ok(())
}
