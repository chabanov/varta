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

fn write_error(f: &mut std::fmt::Formatter<'_>, name: &str, message: &[u16]) -> std::fmt::Result {
    f.write_str(name)?;
    if !message.is_empty() {
        f.write_str(": ")?;
        write_units(f, message)?;
    }
    Ok(())
}
