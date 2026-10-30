//! `Number.prototype.toString(radix)`: exact binary64 rounding intervals
//! choose the shortest fixed text in bases 2..36.
use crate::{IntrinsicError, number::canonical_display};
use alloc::string::String;
use num_bigint::BigUint;

const DENOMINATOR_POWER: usize = 1075;
/// Longest radix text (the smallest subnormal in base 2).
pub const MAX_TEXT_BYTES: usize = 1077;
// 54 radix refinements make even an open rounding interval wider than one unit.
const MAX_SIGNIFICAND_DIGITS: usize = 55;

/// `|value| * 2^1075` as an exact integer, from the binary64 bits.
#[must_use]
pub fn scaled(bits: u64) -> BigUint {
    let exponent = (bits >> 52) as usize;
    let fraction = bits & 0x000f_ffff_ffff_ffff;
    if exponent == 0 {
        BigUint::from(fraction) << 1_usize
    } else {
        BigUint::from(fraction | (1 << 52)) << exponent
    }
}
fn fixed(value: f64, radix: u32) -> Result<String, IntrinsicError> {
    let bits = value.abs().to_bits();
    let mut center = scaled(bits);
    let previous = scaled(bits - 1);
    let next = if bits == f64::MAX.to_bits() {
        BigUint::from(1_u8) << 2099_usize
    } else {
        scaled(bits + 1)
    };
    let mut lower = (&center + previous) >> 1_usize;
    let mut upper = (&center + next) >> 1_usize;
    let closed = bits & 1 == 0;
    let mut denominator = BigUint::from(1_u8) << DENOMINATOR_POWER;
    let mut exponent = 0_i32;
    while center < denominator {
        center *= radix;
        lower *= radix;
        upper *= radix;
        exponent -= 1;
    }
    while center >= &denominator * radix {
        denominator *= radix;
        exponent += 1;
    }
    for _ in 0..MAX_SIGNIFICAND_DIGITS {
        let mut first = &lower / &denominator;
        if !closed || &lower % &denominator != BigUint::from(0_u8) {
            first += 1_u8;
        }
        let mut last = &upper / &denominator;
        if !closed && &upper % &denominator == BigUint::from(0_u8) {
            last -= 1_u8;
        }
        if first <= last {
            let mut selected = &center / &denominator;
            let remainder = &center % &denominator;
            let double = remainder << 1_usize;
            if double > denominator || (double == denominator && selected.bit(0)) {
                selected += 1_u8;
            }
            selected = selected.max(first).min(last);
            let mut digits = selected.to_str_radix(radix);
            while digits.ends_with('0') {
                digits.pop();
                exponent += 1;
            }
            let point = digits.len() as i32 + exponent;
            let mut text = String::new();
            if value.is_sign_negative() {
                text.push('-');
            }
            if point <= 0 {
                text.push_str("0.");
                text.extend(core::iter::repeat_n('0', (-point) as usize));
                text.push_str(&digits);
            } else if point as usize >= digits.len() {
                text.push_str(&digits);
                text.extend(core::iter::repeat_n('0', point as usize - digits.len()));
            } else {
                let point = point as usize;
                text.push_str(&digits[..point]);
                text.push('.');
                text.push_str(&digits[point..]);
            }
            if text.len() > MAX_TEXT_BYTES {
                return Err(IntrinsicError::RadixTextBound);
            }
            return Ok(text);
        }
        center *= radix;
        lower *= radix;
        upper *= radix;
        exponent -= 1;
    }
    Err(IntrinsicError::RadixPrecisionBound)
}
/// `Number.prototype.toString(radix)` for a guest-checked integral radix.
pub fn format(value: f64, radix: f64) -> Result<String, IntrinsicError> {
    if !(2.0..=36.0).contains(&radix) || libm::trunc(radix) != radix {
        return Err(IntrinsicError::NoncanonicalRadix);
    }
    if radix == 10.0 || !value.is_finite() || value == 0.0 {
        return Ok(canonical_display(value));
    }
    fixed(value, radix as u32)
}
