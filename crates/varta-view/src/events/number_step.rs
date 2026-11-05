//! `input type=number` stepping by ArrowUp/ArrowDown, as Chrome 156 does it
//! for the attributes L0 admits. L0 has no `step`, `min` or `max` attribute
//! on inputs, so the step is Chrome's default 1 with step base 0 and no
//! bounds (Blink's `NumberInputType` default step range).
//!
//! The value stays the string L0 keeps. Stepping reads it with Chrome's
//! number sanitization (an HTML floating-point number; anything else,
//! including the empty string, is invalid and counts as 0) and writes the
//! stepped number back as a plain integer string:
//!
//! * a value within Blink's acceptable error (2⁻²⁴ of the step) of a step
//!   multiple is aligned: the result is that multiple ± 1;
//! * any other value is a step mismatch: Up rounds it to the next multiple
//!   above (`ceil`), Down to the next one below (`floor`).
//!
//! Chrome computes this in Blink's 18-digit `Decimal`, which truncates long
//! inputs and prints large results in exponent form. Native stepping is exact
//! decimal arithmetic and admits only the range where both agree: values
//! whose first 18 typed digits are all their nonzero digits, with magnitude
//! below 10¹⁵ and at most 18 fraction digits, and results up to 10¹⁵. Other
//! valid numbers are refused ([`StepRefusal`]): the key changes nothing.

/// Which arrow key stepped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Up,
    Down,
}

/// Why a number field did not step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum StepRefusal {
    #[error(
        "number value outside the exact stepping range (more than 18 significant typed digits, magnitude 10^15 or more, or more than 18 fraction digits)"
    )]
    OutsideExactRange,
}

/// Blink's `Decimal` precision: typed digits beyond it are truncated.
const DECIMAL_PRECISION: usize = 18;
/// Admitted fraction digits; values are exact integers at this scale.
const FRACTION_DIGITS: i64 = 18;
/// 10^[`FRACTION_DIGITS`]: one step at the fixed-point scale.
const SCALE: i128 = 1_000_000_000_000_000_000;
/// Values below 10^this, so results print as plain integers in Chrome.
const MAGNITUDE_DIGITS: i64 = 15;
/// The largest admitted magnitude of a result, in steps (10^15).
const MAX_STEPS: i128 = 1_000_000_000_000_000;
/// Blink's acceptable step error is the step divided by 2^`FLT_MANT_DIG`.
const ACCEPTABLE_ERROR_BITS: u32 = 24;
/// A decimal exponent from which a number exceeds `f64::MAX`
/// (1.797…·10³⁰⁸): Chrome's sanitization makes such a value invalid.
const OVERFLOW_EXPONENT: i64 = 309;
/// Exponent digits beyond this magnitude saturate (the value is then far
/// outside every admitted range either way).
const EXPONENT_LIMIT: i64 = 100_000;
const RADIX: u32 = 10;

/// A sanitized number value: `digits × 10^exponent`, `digits` without
/// leading or trailing zeros (empty for zero), and Blink's typed digits.
struct Number {
    negative: bool,
    digits: Vec<u8>,
    exponent: i64,
    /// Whether every typed digit past Blink's precision is zero.
    exact_in_blink: bool,
}

/// Parse `value` with Chrome's number sanitization grammar: an optional `-`,
/// digits with an optional fraction (or a fraction alone), an optional
/// exponent, and no trailing `.`. `None` is an invalid value.
fn parse(value: &str) -> Option<Number> {
    let bytes = value.as_bytes();
    let mut at = 0;
    let negative = bytes.first() == Some(&b'-');
    if negative {
        at += 1;
    }
    let mut typed = Vec::new();
    let mut leading = true;
    let mut fraction_digits = 0i64;
    let mut mantissa_digits = 0usize;
    while let Some(digit) = bytes.get(at).filter(|byte| byte.is_ascii_digit()) {
        mantissa_digits += 1;
        if !(leading && *digit == b'0') {
            leading = false;
            typed.push(digit - b'0');
        }
        at += 1;
    }
    if bytes.get(at) == Some(&b'.') {
        at += 1;
        while let Some(digit) = bytes.get(at).filter(|byte| byte.is_ascii_digit()) {
            mantissa_digits += 1;
            fraction_digits += 1;
            typed.push(digit - b'0');
            at += 1;
        }
    }
    if mantissa_digits == 0 || value.ends_with('.') {
        return None;
    }
    let mut exponent = 0i64;
    if matches!(bytes.get(at), Some(b'e' | b'E')) {
        at += 1;
        let sign = match bytes.get(at) {
            Some(b'-') => {
                at += 1;
                -1
            }
            Some(b'+') => {
                at += 1;
                1
            }
            _ => 1,
        };
        let start = at;
        while let Some(digit) = bytes.get(at).filter(|byte| byte.is_ascii_digit()) {
            exponent = (exponent * i64::from(RADIX) + i64::from(digit - b'0')).min(EXPONENT_LIMIT);
            at += 1;
        }
        if at == start {
            return None;
        }
        exponent *= sign;
    }
    if at != bytes.len() {
        return None;
    }
    let exact_in_blink = typed
        .iter()
        .skip(DECIMAL_PRECISION)
        .all(|digit| *digit == 0);
    // Normalize: drop leading zeros (fraction zeros typed after the point)
    // and trailing zeros into the exponent.
    let first = typed.iter().position(|digit| *digit != 0);
    let mut digits = first.map_or_else(Vec::new, |first| typed[first..].to_vec());
    let mut exponent = exponent - fraction_digits;
    while digits.last() == Some(&0) {
        digits.pop();
        exponent += 1;
    }
    Some(Number {
        negative,
        digits,
        exponent,
        exact_in_blink,
    })
}

/// The value at the fixed-point scale (`value × 10^18`), or `None` for an
/// invalid value; a valid value outside the exact range is refused.
fn scaled(value: &str) -> Result<Option<i128>, StepRefusal> {
    let Some(number) = parse(value) else {
        return Ok(None);
    };
    if number.digits.is_empty() {
        return Ok(Some(0));
    }
    let length = i64::try_from(number.digits.len()).unwrap_or(i64::MAX);
    let leading = number.exponent.saturating_add(length - 1);
    // Above `f64::MAX` Chrome's sanitization rejects the value.
    if leading >= OVERFLOW_EXPONENT {
        return Ok(None);
    }
    if !number.exact_in_blink || leading >= MAGNITUDE_DIGITS || number.exponent < -FRACTION_DIGITS {
        return Err(StepRefusal::OutsideExactRange);
    }
    // At most 33 digits: below 10^15 with at most 18 fraction digits.
    let mut steps = number.digits.iter().fold(0i128, |total, digit| {
        total * i128::from(RADIX) + i128::from(*digit)
    });
    for _ in 0..number.exponent + FRACTION_DIGITS {
        steps *= i128::from(RADIX);
    }
    Ok(Some(if number.negative { -steps } else { steps }))
}

/// Step `value` once in `direction`, returning the new value string.
pub(crate) fn step(value: &str, direction: Direction) -> Result<String, StepRefusal> {
    // An invalid or empty value counts as 0, which is aligned.
    let current = scaled(value)?.unwrap_or(0);
    let magnitude = current.abs();
    // Blink's `Decimal::Round`: half away from zero.
    let rounded_magnitude = magnitude / SCALE + i128::from(2 * (magnitude % SCALE) >= SCALE);
    let rounded = if current < 0 {
        -rounded_magnitude
    } else {
        rounded_magnitude
    };
    let remainder = (current - rounded * SCALE).abs();
    let mismatch = remainder << ACCEPTABLE_ERROR_BITS > SCALE;
    let result = match (mismatch, direction) {
        (true, Direction::Up) => {
            current.div_euclid(SCALE) + i128::from(current.rem_euclid(SCALE) != 0)
        }
        (true, Direction::Down) => current.div_euclid(SCALE),
        (false, Direction::Up) => rounded + 1,
        (false, Direction::Down) => rounded - 1,
    };
    if result.abs() > MAX_STEPS {
        return Err(StepRefusal::OutsideExactRange);
    }
    Ok(result.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use Direction::{Down, Up};

    /// Values and results measured in headless Chrome 156 (macOS) with
    /// `input.value = value` and one key press.
    #[test]
    fn steps_match_chrome_rounding_alignment_and_invalid_values() {
        for (value, direction, expected) in [
            ("", Up, "1"),
            ("", Down, "-1"),
            ("1.5", Up, "2"),
            ("1.5", Down, "1"),
            ("-1.5", Up, "-1"),
            ("-0.5", Up, "0"),
            ("0.5", Down, "0"),
            ("1", Down, "0"),
            ("-5", Down, "-6"),
            ("2.0", Up, "3"),
            (".5", Up, "1"),
            ("-.5", Up, "0"),
            ("00.5", Up, "1"),
            ("1.50", Up, "2"),
            ("1e3", Up, "1001"),
            ("1E3", Down, "999"),
            ("1.e5", Up, "100001"),
            ("1.E5", Down, "99999"),
            ("-1.e1", Down, "-11"),
            ("1e+3", Down, "999"),
            ("1e-0", Down, "0"),
            ("1e0005", Down, "99999"),
            ("5e+2", Up, "501"),
            ("5E-1", Up, "1"),
            ("1.5e1", Up, "16"),
            ("0.1e1", Up, "2"),
            ("0e5", Up, "1"),
            ("-0", Up, "1"),
            ("-0", Down, "-1"),
            ("-0.0", Down, "-1"),
            ("0.0", Down, "-1"),
            ("0.1", Up, "1"),
            ("1e-7", Up, "1"),
            ("1e-7", Down, "0"),
            // Within 2^-24 of a multiple: aligned, so ±1 from the multiple.
            ("6e-8", Down, "0"),
            ("5e-8", Up, "1"),
            ("5e-8", Down, "-1"),
            ("-5e-8", Up, "1"),
            ("3e-8", Down, "-1"),
            ("1e-8", Down, "-1"),
            ("1e-10", Down, "-1"),
            ("1e-15", Down, "-1"),
            ("1e-16", Down, "-1"),
            ("1e-18", Down, "-1"),
            ("0.000000000000001", Down, "-1"),
            ("-0.000000000000001", Up, "1"),
            ("1.00000001", Up, "2"),
            ("1.00000001", Down, "0"),
            ("1.0000001", Up, "2"),
            ("0.99999999", Up, "2"),
            ("0.99999999", Down, "0"),
            ("1.000000000000001", Down, "0"),
            ("0.30000000000000004", Down, "0"),
            ("123456.7", Up, "123457"),
            ("123456789.123456789", Up, "123456790"),
            ("123456789012345", Up, "123456789012346"),
            ("-123456789012344.9", Down, "-123456789012345"),
            ("999999999999998.5", Up, "999999999999999"),
            ("999999999999999.0", Up, "1000000000000000"),
            ("99999999999999.0", Up, "100000000000000"),
            ("5e14", Up, "500000000000001"),
            ("00012", Up, "13"),
            // Invalid values count as 0.
            ("12e", Up, "1"),
            ("12e", Down, "-1"),
            ("5.", Up, "1"),
            ("+5", Up, "1"),
            (" 5", Up, "1"),
            ("5 ", Up, "1"),
            ("1,000", Up, "1"),
            ("-", Up, "1"),
            (".", Up, "1"),
            (".e5", Up, "1"),
            ("1e", Up, "1"),
            ("1e+", Up, "1"),
            ("--5", Up, "1"),
            ("0x10", Up, "1"),
            ("Infinity", Up, "1"),
            ("NaN", Up, "1"),
            ("1e309", Up, "1"),
            ("1e400", Up, "1"),
            ("-1e400", Up, "1"),
            ("1e1023", Up, "1"),
        ] {
            assert_eq!(
                step(value, direction).as_deref(),
                Ok(expected),
                "{value} {direction:?}"
            );
        }
    }

    #[test]
    fn values_chrome_computes_with_decimal_precision_loss_are_refused() {
        // Chrome: 1e+18, 9.99999999999999999e+19, 123456789012345679,
        // 1.5e+300, 9007199254740995, -1 (1e-400 and 1e-19 are aligned),
        // 1000000000000001, 1.7976931348623157e308 (unchanged) and -1.
        for (value, direction) in [
            ("999999999999999999", Up),
            ("99999999999999999999", Up),
            ("123456789012345678.5", Up),
            ("1.5e300", Up),
            ("9007199254740993.5", Up),
            ("1e-400", Down),
            ("1e-19", Down),
            ("1000000000000000", Up),
            ("1.7976931348623157e308", Up),
            ("0.0000000000000000000005e20", Down),
        ] {
            assert_eq!(
                step(value, direction),
                Err(StepRefusal::OutsideExactRange),
                "{value}"
            );
        }
        // Trailing typed zeros past 18 digits are exact in Blink too.
        assert_eq!(step("1.50000000000000000000000", Up).as_deref(), Ok("2"));
    }
}
