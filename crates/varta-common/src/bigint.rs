//! Canonical binary transport for opaque guest BigInt scalars.
//!
//! V28/ABI27 select scalar transport; v29/value13 admits it in value graphs.
//! A managed value is an immutable leaf: no digit word is a child reference.
use crate::{abi, value::ValueAbiError};

pub const TRANSPORT_VERSION: u32 = 1;
pub const HEAP_FLAG: u32 = 2048;
pub const META_BYTES: usize = 8;
pub const DIGIT_BYTES: usize = 4;
const BLOCK_RESERVED_OFFSET: usize = 12;
pub const MAX_PAYLOAD_BYTES: usize = abi::MAX_STRING_BYTES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum Sign {
    Zero = 0,
    Positive = 1,
    Negative = 2,
}

/// A validated, borrowed sign and little-endian base-2^32 magnitude.
#[derive(Debug, Clone, Copy)]
pub struct BigIntView<'a> {
    sign: Sign,
    magnitude: &'a [u8],
}
fn invalid() -> ValueAbiError {
    ValueAbiError("noncanonical BigInt transport".into())
}
fn word(data: &[u8], offset: usize) -> Result<u32, ValueAbiError> {
    offset
        .checked_add(DIGIT_BYTES)
        .and_then(|end| data.get(offset..end))
        .and_then(|bytes| bytes.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(invalid)
}
impl<'a> BigIntView<'a> {
    /// Validate host result bytes before a managed guest copy is published.
    pub fn from_payload(payload: &'a [u8]) -> Result<Self, ValueAbiError> {
        if !(META_BYTES..=MAX_PAYLOAD_BYTES).contains(&payload.len()) {
            return Err(invalid());
        }
        let sign = match word(payload, 0)? {
            0 => Sign::Zero,
            1 => Sign::Positive,
            2 => Sign::Negative,
            _ => return Err(invalid()),
        };
        let digits = word(payload, DIGIT_BYTES)? as usize;
        let length = digits
            .checked_mul(DIGIT_BYTES)
            .and_then(|n| n.checked_add(META_BYTES))
            .ok_or_else(invalid)?;
        if length != payload.len()
            || (sign == Sign::Zero) != (digits == 0)
            || (digits != 0 && word(payload, length - DIGIT_BYTES)? == 0)
        {
            return Err(invalid());
        }
        Ok(Self {
            sign,
            magnitude: &payload[META_BYTES..],
        })
    }
    /// Validate the complete managed block, exact payload and zero child mask.
    /// Structured readers admit this leaf only under value ABI13.
    pub fn read(data: &'a [u8], handle: u64) -> Result<Self, ValueAbiError> {
        let pointer = handle as u32 as usize;
        let length = (handle >> u32::BITS) as usize;
        if pointer < abi::OWNED_STRING_BASE + abi::STRING_HEADER_BYTES
            || !pointer.is_multiple_of(crate::value::HEAP_ALIGNMENT)
            || !(META_BYTES..=MAX_PAYLOAD_BYTES).contains(&length)
        {
            return Err(invalid());
        }
        let header = pointer - abi::STRING_HEADER_BYTES;
        let size = word(data, header)? as usize;
        if size < abi::STRING_HEADER_BYTES
            || !size.is_multiple_of(crate::value::HEAP_ALIGNMENT)
            || length > size - abi::STRING_HEADER_BYTES
            || header.checked_add(size).is_none_or(|end| end > data.len())
            || word(data, header + crate::value::HEAP_FLAGS_OFFSET)?
                != crate::value::HEAP_ALLOCATED_FLAG | HEAP_FLAG
            || word(data, header + crate::value::HEAP_LINK_OFFSET)? != 0
            || word(data, header + BLOCK_RESERVED_OFFSET)? != 0
        {
            return Err(invalid());
        }
        let payload = pointer
            .checked_add(length)
            .and_then(|end| data.get(pointer..end))
            .ok_or_else(invalid)?;
        Self::from_payload(payload)
    }
    #[must_use]
    pub fn sign(self) -> Sign {
        self.sign
    }
    #[must_use]
    pub fn is_zero(self) -> bool {
        self.sign == Sign::Zero
    }
    #[must_use]
    pub fn magnitude(self) -> &'a [u8] {
        self.magnitude
    }
    #[must_use]
    pub fn bits(self) -> u64 {
        if self.is_zero() {
            return 0;
        }
        // Validation proved the last word exists and is nonzero.
        let top = word(self.magnitude, self.magnitude.len() - DIGIT_BYTES)
            .expect("validated BigInt high digit");
        (self.magnitude.len() as u64 * u64::from(u8::BITS)) - u64::from(top.leading_zeros())
    }
}

/// Shared operation selectors; unknown selectors are malformed host input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum UnaryOp {
    Negate = 0,
    Not = 1,
    Plus = 2,
}
impl UnaryOp {
    pub fn from_tag(tag: u32) -> Option<Self> {
        Some(match tag {
            0 => Self::Negate,
            1 => Self::Not,
            2 => Self::Plus,
            _ => return None,
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum BinaryOp {
    Add = 0,
    Subtract = 1,
    Multiply = 2,
    Divide = 3,
    Remainder = 4,
    Power = 5,
    And = 6,
    Or = 7,
    Xor = 8,
    LeftShift = 9,
    RightShift = 10,
    UnsignedRightShift = 11,
}
impl BinaryOp {
    pub fn from_tag(tag: u32) -> Option<Self> {
        Some(match tag {
            0 => Self::Add,
            1 => Self::Subtract,
            2 => Self::Multiply,
            3 => Self::Divide,
            4 => Self::Remainder,
            5 => Self::Power,
            6 => Self::And,
            7 => Self::Or,
            8 => Self::Xor,
            9 => Self::LeftShift,
            10 => Self::RightShift,
            11 => Self::UnsignedRightShift,
            _ => return None,
        })
    }
}

/// Render owned BigInt exceptions without an unbounded decimal conversion.
pub fn write_diagnostic(f: &mut std::fmt::Formatter<'_>, payload: &[u8]) -> std::fmt::Result {
    let Ok(value) = BigIntView::from_payload(payload) else {
        return f.write_str("<invalid BigInt>");
    };
    if value.is_zero() {
        return f.write_str("0n");
    }
    if value.sign() == Sign::Negative {
        f.write_str("-")?;
    }
    f.write_str("0x")?;
    let digits = value.magnitude().iter().rev().skip_while(|&&b| b == 0);
    for (count, byte) in digits.enumerate() {
        if count == crate::exceptions::MAX_DIAGNOSTIC_UNITS / 2 {
            f.write_str("…")?;
            break;
        }
        if count == 0 {
            write!(f, "{byte:x}")?;
        } else {
            write!(f, "{byte:02x}")?;
        }
    }
    f.write_str("n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value;
    fn payload(sign: Sign, digits: &[u32]) -> Vec<u8> {
        let mut bytes = (sign as u32).to_le_bytes().to_vec();
        bytes.extend_from_slice(&(digits.len() as u32).to_le_bytes());
        bytes.extend(digits.iter().flat_map(|digit| digit.to_le_bytes()));
        bytes
    }
    fn managed(payload: &[u8]) -> (Vec<u8>, u64) {
        let start = abi::OWNED_STRING_BASE;
        let pointer = start + abi::STRING_HEADER_BYTES;
        let size =
            (abi::STRING_HEADER_BYTES + payload.len()).next_multiple_of(value::HEAP_ALIGNMENT);
        let mut memory = vec![0; start + size];
        memory[start..start + 4].copy_from_slice(&(size as u32).to_le_bytes());
        memory[start + value::HEAP_FLAGS_OFFSET..start + value::HEAP_FLAGS_OFFSET + 4]
            .copy_from_slice(&(value::HEAP_ALLOCATED_FLAG | HEAP_FLAG).to_le_bytes());
        memory[pointer..pointer + payload.len()].copy_from_slice(payload);
        (
            memory,
            ((payload.len() as u64) << u32::BITS) | pointer as u64,
        )
    }
    #[test]
    fn bigint_owned_exception_diagnostics_are_bounded_and_do_not_truncate_payloads() {
        assert_eq!(
            crate::exceptions::ThrownValue::BigInt(vec![]).to_string(),
            "<invalid BigInt>"
        );
        let bytes = payload(Sign::Negative, &vec![u32::MAX; 1024]);
        let value = crate::exceptions::ThrownValue::BigInt(bytes.clone());
        let text = value.to_string();
        assert!(text.starts_with("-0x"));
        assert!(text.ends_with("…n"));
        assert!(text.len() <= crate::exceptions::MAX_DIAGNOSTIC_UNITS + 16);
        assert_eq!(value, crate::exceptions::ThrownValue::BigInt(bytes));
        assert_eq!(
            crate::exceptions::ThrownValue::BigInt(payload(Sign::Zero, &[])).to_string(),
            "0n"
        );
    }
    #[test]
    fn bigint_transport_bits_sign_and_canonical_zero() {
        for (sign, digits, bits) in [
            (Sign::Zero, vec![], 0),
            (Sign::Positive, vec![1], 1),
            (Sign::Negative, vec![u32::MAX], 32),
            (Sign::Positive, vec![0, 1], 33),
            (Sign::Negative, vec![u32::MAX, u32::MAX], 64),
        ] {
            let bytes = payload(sign, &digits);
            let view = BigIntView::from_payload(&bytes).unwrap();
            assert_eq!(view.sign(), sign);
            assert_eq!(view.is_zero(), sign == Sign::Zero);
            assert_eq!(view.bits(), bits);
            assert_eq!(view.magnitude(), &bytes[META_BYTES..]);
            let (memory, handle) = managed(&bytes);
            assert_eq!(BigIntView::read(&memory, handle).unwrap().bits(), bits);
        }
    }
    #[test]
    fn bigint_transport_rejects_noncanonical_payloads_and_counts() {
        for bytes in [
            vec![],
            vec![0; META_BYTES - 1],
            payload(Sign::Zero, &[1]),
            payload(Sign::Positive, &[]),
            payload(Sign::Negative, &[]),
            payload(Sign::Negative, &[0]),
            payload(Sign::Positive, &[1, 0]),
            [3u32, 1, 7]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
            [1u32, u32::MAX, 7]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        ] {
            assert!(BigIntView::from_payload(&bytes).is_err(), "{bytes:?}");
        }
        let valid = payload(Sign::Positive, &[1, 2]);
        for end in 0..valid.len() {
            assert!(BigIntView::from_payload(&valid[..end]).is_err());
        }
        let mut trailing = valid;
        trailing.extend_from_slice(&[0; DIGIT_BYTES]);
        assert!(BigIntView::from_payload(&trailing).is_err());
    }
    #[test]
    fn bigint_transport_requires_bounded_allocated_leaf_and_exact_handle() {
        let (memory, handle) = managed(&payload(Sign::Negative, &[1, 2]));
        let header = abi::OWNED_STRING_BASE;
        for (offset, value) in [
            (header, 16u32),
            (header, 31),
            (header, u32::MAX),
            (header + value::HEAP_FLAGS_OFFSET, 0),
            (
                header + value::HEAP_FLAGS_OFFSET,
                value::HEAP_ALLOCATED_FLAG,
            ),
            (
                header + value::HEAP_FLAGS_OFFSET,
                value::HEAP_ALLOCATED_FLAG | value::HEAP_ARRAY_FLAG as u32,
            ),
            (
                header + value::HEAP_FLAGS_OFFSET,
                value::HEAP_ALLOCATED_FLAG | HEAP_FLAG | 2,
            ),
            (header + value::HEAP_LINK_OFFSET, 1),
            (header + BLOCK_RESERVED_OFFSET, 1),
        ] {
            let mut bad = memory.clone();
            bad[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(
                BigIntView::read(&bad, handle).is_err(),
                "offset={offset} value={value}"
            );
        }
        for bad in [
            0,
            handle + 1,
            handle - 8,
            handle ^ (1u64 << 32),
            handle + (4u64 << 32),
            u64::MAX,
        ] {
            assert!(BigIntView::read(&memory, bad).is_err());
        }
        assert!(BigIntView::read(&memory[..memory.len() - 1], handle).is_err());
    }
    #[test]
    fn bigint_transport_maximum_payload_has_no_unbounded_count_allocation() {
        let digits = (MAX_PAYLOAD_BYTES - META_BYTES) / DIGIT_BYTES;
        let mut bytes = vec![0; MAX_PAYLOAD_BYTES];
        bytes[..4].copy_from_slice(&(Sign::Positive as u32).to_le_bytes());
        bytes[4..META_BYTES].copy_from_slice(&(digits as u32).to_le_bytes());
        bytes[MAX_PAYLOAD_BYTES - DIGIT_BYTES..].copy_from_slice(&u32::MAX.to_le_bytes());
        let view = BigIntView::from_payload(&bytes).unwrap();
        assert_eq!(view.bits(), (digits as u64) * u64::from(u32::BITS));
        bytes.push(0);
        assert!(BigIntView::from_payload(&bytes).is_err());
    }
    #[test]
    fn bigint_transport_keeps_old_tags_frozen_and_versions_structured_admission() {
        assert_eq!(
            value::ValueKind::from_tag(11),
            Some(value::ValueKind::BigInt)
        );
        for version in 1..=value::SPARSE_VALUE_VERSION {
            assert!(!value::ValueKind::BigInt.supported_by(version));
        }
        assert!(value::ValueKind::BigInt.supported_by(value::BIGINT_VALUE_VERSION));
        assert!(value::supported_version(13));
        assert!(abi::supported(27));
        assert_eq!(crate::language::SUPPORTED_PROFILES.len(), 33);
    }
}
