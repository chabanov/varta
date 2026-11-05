//! `input type=number` stepping by ArrowUp/ArrowDown, as Chrome 156 does it
//! for the attributes L0 admits. L0 has no `step`, `min` or `max` attribute
//! on inputs, so the step is Chrome's default 1 with step base 0 and no
//! bounds (Blink's `NumberInputType` default step range).
//!
//! The value stays the string L0 keeps. Stepping reads it with Chrome's
//! number sanitization (an HTML floating-point number whose value is a finite
//! double; anything else, including the empty string, is invalid and counts
//! as 0; [`is_valid`]) and writes the
//! stepped number back as a plain integer string:
//!
//! * a value within Blink's acceptable error (2⁻²⁴ of the step) of a step
//!   multiple is aligned: the result is that multiple ± 1;
//! * any other value is a step mismatch: Up rounds it to the next multiple
//!   above (`ceil`), Down to the next one below (`floor`).
//!
//! Chrome computes this in Blink's 18-digit `Decimal`, which truncates long
//! inputs and prints large results in exponent form. Native stepping is exact
//! decimal arithmetic and admits only the range where both agree, measured
//! in Chrome: values whose first 18 typed digits are all their nonzero
//! digits; integers below 10¹⁸ in magnitude (Blink's 18-digit coefficient
//! holds them and their ±1 results), other values below 10¹⁵ with at most 18
//! fraction digits; and results below 10¹⁸ (Blink prints 10¹⁸ as `1e+18`).
//! Other valid numbers are refused ([`StepRefusal`]): the key leaves the
//! value unchanged, and the view counts the refusal for the host
//! (`stepsRefused` in the snapshot).

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
        "number value outside the exact stepping range (more than 18 significant typed digits, an integer of magnitude 10^18 or more, another value of magnitude 10^15 or more, more than 18 fraction digits, or a result of magnitude 10^18 or more)"
    )]
    OutsideExactRange,
}

/// Blink's `Decimal` precision: typed digits beyond it are truncated.
const DECIMAL_PRECISION: usize = 18;
/// Admitted fraction digits; values are exact integers at this scale.
const FRACTION_DIGITS: i64 = 18;
/// 10^[`FRACTION_DIGITS`]: one step at the fixed-point scale.
const SCALE: i128 = 1_000_000_000_000_000_000;
/// Non-integral values are admitted below 10^this: Chrome steps larger ones
/// with precision loss (`12345678901234567.5` Up gives `…569`).
const FRACTION_MAGNITUDE_DIGITS: i64 = 15;
/// Integral values are admitted below 10^this: Blink's 18-digit coefficient
/// holds them exactly.
const INTEGER_MAGNITUDE_DIGITS: i64 = 18;
/// Results are admitted below this magnitude, in steps (10^18): Chrome
/// prints 10^18 in exponent form (`1e+18`).
const RESULT_LIMIT: i128 = 1_000_000_000_000_000_000;
/// Blink's acceptable step error is the step divided by 2^`FLT_MANT_DIG`.
const ACCEPTABLE_ERROR_BITS: u32 = 24;
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

/// Whether `value` is valid under Chrome's number sanitization: the HTML
/// floating-point grammar (an optional `-`, digits with an optional fraction
/// or a fraction alone, an optional exponent, no trailing `.`) and a value
/// that rounds to a finite double (above `f64::MAX` it is invalid).
pub(crate) fn is_valid(value: &str) -> bool {
    parse(value).is_some()
}

/// Parse `value` with Chrome's number sanitization ([`is_valid`]). `None`
/// is an invalid value.
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
    // The grammar is a subset of Rust's float syntax, which rounds to the
    // nearest double as Chrome's parser does.
    if at != bytes.len() || !value.parse::<f64>().is_ok_and(f64::is_finite) {
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
    let magnitude_digits = if number.exponent >= 0 {
        INTEGER_MAGNITUDE_DIGITS
    } else {
        FRACTION_MAGNITUDE_DIGITS
    };
    if !number.exact_in_blink || leading >= magnitude_digits || number.exponent < -FRACTION_DIGITS {
        return Err(StepRefusal::OutsideExactRange);
    }
    // At most 36 digits: below 10^18 with at most 18 fraction digits.
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
    if result.abs() >= RESULT_LIMIT {
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
            ("999999999999999.01", Down, "999999999999999"),
            ("999999999999999.01", Up, "1000000000000000"),
            // Integers below 10^18 step exactly, up to results below 10^18.
            ("1000000000000000", Up, "1000000000000001"),
            ("1000000000000000", Down, "999999999999999"),
            ("9007199254740991", Up, "9007199254740992"),
            ("12345678901234567", Up, "12345678901234568"),
            ("99999999999999999", Up, "100000000000000000"),
            ("-99999999999999999", Down, "-100000000000000000"),
            ("1e16", Up, "10000000000000001"),
            ("1e17", Up, "100000000000000001"),
            ("1e17", Down, "99999999999999999"),
            ("5e16", Down, "49999999999999999"),
            ("0.5e17", Up, "50000000000000001"),
            ("1.0e17", Up, "100000000000000001"),
            ("123456789012345678", Up, "123456789012345679"),
            ("123456789012345678.0", Up, "123456789012345679"),
            ("-123456789012345678", Down, "-123456789012345679"),
            ("999999999999999998", Up, "999999999999999999"),
            ("-999999999999999998", Down, "-999999999999999999"),
            ("999999999999999999", Down, "999999999999999998"),
            ("-999999999999999999", Up, "-999999999999999998"),
            ("9.99999999999999998e17", Up, "999999999999999999"),
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
            ("1e99999", Up, "1"),
            // Above `f64::MAX` once rounded to a double.
            ("1.7976931348623159e308", Up, "1"),
            ("-1.7976931348623159e308", Up, "1"),
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
        // Chrome: 1e+18, -1e+18, 1e+18, 9.99999999999999999e+19,
        // 123456789012345679, 1.5e+300, 9007199254740995,
        // 12345678901234569, 10000000000000001, -1 (1e-400 and 1e-19 are
        // aligned), 1.7976931348623157e308 (unchanged), 1 and -1. Chrome
        // steps `1000000000000000.5` exactly (1000000000000001), but
        // non-integral values from 10^15 are refused with the rest.
        for (value, direction) in [
            ("999999999999999999", Up),
            ("-999999999999999999", Down),
            ("1e18", Down),
            ("99999999999999999999", Up),
            ("123456789012345678.5", Up),
            ("1.5e300", Up),
            ("9007199254740993.5", Up),
            ("12345678901234567.5", Up),
            ("9999999999999999.5", Up),
            ("1e-400", Down),
            ("1e-19", Down),
            ("1.7976931348623157e308", Up),
            ("1.7976931348623158e308", Up),
            ("0.0000000000000000000005e20", Down),
            ("1000000000000000.5", Up),
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

    /// Chrome 156 keeps a valid assigned value and empties an invalid one.
    #[test]
    fn validity_follows_chrome_sanitization() {
        for value in [
            "0",
            "-0",
            "1.5",
            ".5",
            "-.5",
            "1.e5",
            "1E3",
            "1e+3",
            "5e-1",
            "0e99999",
            "1e-400",
            "1e308",
            "1.7976931348623157e308",
            "1.7976931348623158e308",
        ] {
            assert!(is_valid(value), "{value}");
        }
        for value in [
            "",
            "1.",
            "5.",
            "+5",
            " 5",
            "5 ",
            "1,000",
            "12e",
            "1e+",
            ".e5",
            "-",
            ".",
            "--5",
            "0x10",
            "Infinity",
            "NaN",
            "1e309",
            "1e99999",
            "1.7976931348623159e308",
            "-1.7976931348623159e308",
        ] {
            assert!(!is_valid(value), "{value}");
        }
    }
}
