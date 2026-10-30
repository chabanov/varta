use super::*;
use alloc::vec::Vec;
use num_bigint::Sign;
impl Integer {
    /// ECMAScript StringToBigInt grammar, including its exact whitespace set.
    pub fn from_string(text: &str, budget: &mut Budget) -> Result<Self, Error> {
        budget.charge(
            u64::try_from(text.len())
                .map_err(|_| Error::WorkBudget)?
                .max(1),
        )?;
        let text = text.trim_matches(|c: char| {
            u16::try_from(u32::from(c)).is_ok_and(varta_common::text::ecmascript_whitespace)
        });
        Self::parse_ascii(text.as_bytes(), budget)
    }
    /// Lone surrogates and non-ASCII numeric content are Syntax errors.
    pub fn from_utf16(units: &[u16], budget: &mut Budget) -> Result<Self, Error> {
        budget.charge(
            u64::try_from(units.len())
                .map_err(|_| Error::WorkBudget)?
                .max(1),
        )?;
        let start = units
            .iter()
            .position(|u| !varta_common::text::ecmascript_whitespace(*u))
            .unwrap_or(units.len());
        let end = units
            .iter()
            .rposition(|u| !varta_common::text::ecmascript_whitespace(*u))
            .map_or(start, |i| i + 1);
        let bytes = units[start..end]
            .iter()
            .map(|u| {
                u8::try_from(*u)
                    .ok()
                    .filter(u8::is_ascii)
                    .ok_or(Error::Syntax)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::parse_ascii(&bytes, budget)
    }
    /// Heap transport uses decimal without whitespace, prefixes, + or negative zero.
    pub fn from_canonical(text: &str, budget: &mut Budget) -> Result<Self, Error> {
        budget.charge(
            u64::try_from(text.len())
                .map_err(|_| Error::WorkBudget)?
                .max(1),
        )?;
        let digits = text
            .as_bytes()
            .strip_prefix(b"-")
            .unwrap_or(text.as_bytes());
        if digits.is_empty()
            || digits.iter().any(|c| !c.is_ascii_digit())
            || digits.len() > 1 && digits[0] == b'0'
            || text == "-0"
        {
            return Err(Error::Syntax);
        }
        Self::parse_ascii(text.as_bytes(), budget)
    }
    fn parse_ascii(mut text: &[u8], budget: &mut Budget) -> Result<Self, Error> {
        if text.is_empty() {
            return budget.finish(BigInt::zero());
        }
        let signed = matches!(text.first(), Some(b'+' | b'-'));
        let negative = text.first() == Some(&b'-');
        if signed {
            text = &text[1..];
        }
        let (radix, digits) = if text.starts_with(b"0x") || text.starts_with(b"0X") {
            (16, &text[2..])
        } else if text.starts_with(b"0o") || text.starts_with(b"0O") {
            (8, &text[2..])
        } else if text.starts_with(b"0b") || text.starts_with(b"0B") {
            (2, &text[2..])
        } else {
            (10, text)
        };
        if signed && radix != 10 || digits.is_empty() {
            return Err(Error::Syntax);
        }
        for c in digits {
            let digit = match c {
                b'0'..=b'9' => u32::from(c - b'0'),
                b'a'..=b'f' => u32::from(c - b'a') + 10,
                b'A'..=b'F' => u32::from(c - b'A') + 10,
                _ => return Err(Error::Syntax),
            };
            if digit >= radix {
                return Err(Error::Syntax);
            }
        }
        let significant = digits
            .iter()
            .position(|d| *d != b'0')
            .map_or(&digits[digits.len()..], |i| &digits[i..]);
        if significant.is_empty() {
            return budget.finish(BigInt::zero());
        }
        // A lower bit bound rejects enormous inputs before parsing, without
        // rejecting boundary values that fit the caller's exact capacity.
        let lower = u64::try_from(significant.len() - 1)
            .map_err(|_| Error::BitCapacity)?
            .saturating_mul(u64::from(radix.ilog2()))
            .saturating_add(1);
        budget.capacity(lower)?;
        let work = u64::try_from(significant.len())
            .map_err(|_| Error::WorkBudget)?
            .div_ceil(8)
            .max(1);
        budget.charge(work.checked_mul(work).ok_or(Error::WorkBudget)?)?;
        let mut value = BigInt::parse_bytes(significant, radix).ok_or(Error::Syntax)?;
        if negative {
            value = -value;
        }
        debug_assert!(value.sign() != Sign::NoSign);
        budget.finish(value)
    }
}
