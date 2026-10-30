//! Decimal formatting (`toFixed`, `toExponential`, `toPrecision`): fixed uses
//! pinned Ryū; exact ratios round significant digits.
use crate::{IntrinsicError, number::canonical_display, radix::scaled};
use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use num_bigint::BigUint;

/// Longest decimal text (`toFixed(100)` of the largest finite values).
pub const MAX_TEXT_BYTES: usize = 123;

/// The three checked decimal formats (`varta_common::number_format::NumberFormat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Fixed,
    Exponential,
    Precision,
}

/// `precision` significant decimal digits of `|value|` and its decimal exponent.
#[must_use]
pub fn significant(value: f64, precision: u32) -> (String, i32) {
    if value == 0.0 {
        return ("0".repeat(precision as usize), 0);
    }
    let mut numerator = scaled(value.abs().to_bits());
    let mut denominator = BigUint::from(1_u8) << 1075_usize;
    let mut exponent = 0;
    while numerator < denominator {
        numerator *= 10_u8;
        exponent -= 1;
    }
    while numerator >= &denominator * 10_u8 {
        denominator *= 10_u8;
        exponent += 1;
    }
    numerator *= BigUint::from(10_u8).pow(precision - 1);
    let mut rounded = &numerator / &denominator;
    if (&numerator % &denominator) << 1_usize >= denominator {
        rounded += 1_u8;
    }
    if rounded >= BigUint::from(10_u8).pow(precision) {
        rounded /= 10_u8;
        exponent += 1;
    }
    (rounded.to_str_radix(10), exponent)
}
fn exponential(digits: &str, exponent: i32, negative: bool) -> String {
    let mut text = String::new();
    if negative {
        text.push('-');
    }
    text.push_str(&digits[..1]);
    if digits.len() > 1 {
        text.push('.');
        text.push_str(&digits[1..]);
    }
    text.push('e');
    if exponent >= 0 {
        text.push('+');
    }
    text.push_str(&exponent.to_string());
    text
}
fn shortest_exponential(value: f64) -> String {
    if value == 0.0 {
        return "0e+0".into();
    }
    let text = canonical_display(value.abs());
    let (mantissa, exponent) = match text.split_once('e') {
        // Ryū writes a decimal exponent; anything else cannot occur.
        Some((m, e)) => (m, e.parse::<i32>().unwrap_or(0)),
        None => (text.as_str(), 0),
    };
    let point = mantissa.find('.').unwrap_or(mantissa.len());
    let all: String = mantissa.chars().filter(|&c| c != '.').collect();
    let digits = all.trim_start_matches('0');
    let exponent = exponent + point as i32 - 1 - (all.len() - digits.len()) as i32;
    exponential(digits.trim_end_matches('0'), exponent, value < 0.0)
}
fn precision_text(value: f64, precision: u32) -> String {
    let (digits, exponent) = significant(value, precision);
    if exponent < -6 || exponent >= precision as i32 {
        return exponential(&digits, exponent, value < 0.0);
    }
    let mut text = String::new();
    if value < 0.0 {
        text.push('-');
    }
    let point = exponent + 1;
    if point <= 0 {
        text.push_str("0.");
        text.extend(core::iter::repeat_n('0', (-point) as usize));
        text.push_str(&digits);
    } else {
        let point = point as usize;
        text.push_str(&digits[..point]);
        if point < digits.len() {
            text.push('.');
            text.push_str(&digits[point..]);
        }
    }
    text
}
fn integral(value: f64) -> bool {
    libm::trunc(value) == value
}
/// `Number.prototype.<mode>(digits)`; `digits == -1` is the undefined
/// argument of `toExponential`/`toPrecision`. Digits were range-checked by
/// the guest, so an out-of-range value is a fatal noncanonical call.
pub fn format(mode: Mode, value: f64, digits: f64) -> Result<String, IntrinsicError> {
    if digits.is_nan() || (digits.is_finite() && !integral(digits)) {
        return Err(IntrinsicError::NoncanonicalPrecision);
    }
    let text = if mode == Mode::Fixed {
        if !(0.0..=100.0).contains(&digits) {
            return Err(IntrinsicError::FixedPrecision);
        }
        let mut buffer = ryu_js::Buffer::new();
        buffer.format_to_fixed(value, digits as u8).into()
    } else if !value.is_finite() {
        canonical_display(value)
    } else if digits == -1.0 {
        if mode == Mode::Precision {
            canonical_display(value)
        } else {
            shortest_exponential(value)
        }
    } else {
        let minimum = if mode == Mode::Precision { 1.0 } else { 0.0 };
        if !(minimum..=100.0).contains(&digits) {
            return Err(IntrinsicError::DecimalPrecision);
        }
        if mode == Mode::Precision {
            precision_text(value, digits as u32)
        } else {
            let (text, exponent) = significant(value, digits as u32 + 1);
            exponential(&text, exponent, value < 0.0)
        }
    };
    if text.len() > MAX_TEXT_BYTES {
        return Err(IntrinsicError::DecimalTextBound);
    }
    Ok(text)
}

/// [`format`] as UTF-16LE result bytes.
pub fn format16(mode: Mode, value: f64, digits: f64) -> Result<Vec<u8>, IntrinsicError> {
    format(mode, value, digits).map(|text| crate::text::encode(&text))
}
