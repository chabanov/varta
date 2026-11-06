//! Exact ECMAScript string and number semantics that Tailwind's TypeScript
//! relies on implicitly (Varta's own helper for the port of Tailwind CSS
//! v4.3.5, Tailwind Labs, MIT): `Number(string)`, `parseFloat`,
//! `Number.prototype.toString()`, `Number.isInteger`, `String.prototype.trim`
//! and relational string comparison by UTF-16 code units.
//!
//! References are to ECMAScript 2024 (ECMA-262 15th edition).

use std::cmp::Ordering;

/// Bits of one binary (`0b`) digit.
const BINARY_DIGIT_BITS: u32 = 1;
/// Bits of one octal (`0o`) digit.
const OCTAL_DIGIT_BITS: u32 = 3;
/// Bits of one hexadecimal (`0x`) digit.
const HEX_DIGIT_BITS: u32 = 4;
/// Significand precision of an IEEE 754 binary64 value, hidden bit included.
const F64_PRECISION_BITS: usize = f64::MANTISSA_DIGITS as usize;
/// Exponent bias of an IEEE 754 binary64 value.
const F64_EXPONENT_BIAS: usize = 1023;
/// Bit offset of the exponent field in an IEEE 754 binary64 value.
const F64_EXPONENT_SHIFT: u32 = 52;
/// The literal spelling of positive infinity in `StrUnsignedDecimalLiteral`.
const INFINITY_LITERAL: &str = "Infinity";

/// ECMAScript `WhiteSpace` or `LineTerminator` (§12.2, §12.3): the set that
/// `String.prototype.trim`, `StringToNumber` and RegExp `\s` share.
#[must_use]
pub fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{0009}' | '\u{000B}' | '\u{000C}' | '\u{FEFF}'
            // Zs (Space_Separator)
            | '\u{0020}' | '\u{00A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
    ) || is_line_terminator(c)
}

/// ECMAScript `LineTerminator` (§12.3): the characters RegExp `.` excludes.
#[must_use]
pub fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `String.prototype.trim` (§22.1.3.32).
#[must_use]
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_whitespace)
}

/// `String.prototype.trimStart` (§22.1.3.34).
#[must_use]
pub fn js_trim_start(s: &str) -> &str {
    s.trim_start_matches(is_js_whitespace)
}

/// `String.prototype.trimEnd` (§22.1.3.33).
#[must_use]
pub fn js_trim_end(s: &str) -> &str {
    s.trim_end_matches(is_js_whitespace)
}

/// The relational comparison of two strings (§7.2.13 IsLessThan): code unit
/// by code unit in UTF-16, a proper prefix ordering first.
#[must_use]
pub fn compare_utf16(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// `Number(string)`: StringToNumber (§7.1.4.1.1).
///
/// Surrounding white space and line terminators are ignored, an empty or
/// blank string is `0`, `Infinity` may be signed, `0x`/`0o`/`0b` integers are
/// unsigned, and decimal literals take an optional exponent. Numeric
/// separators and anything else give `NaN`.
#[must_use]
pub fn to_number(s: &str) -> f64 {
    let text = js_trim(s);
    if text.is_empty() {
        return 0.0;
    }
    if let Some(value) = non_decimal_integer(text) {
        return value;
    }
    match scan_decimal_literal(text) {
        Some(len) if len == text.len() => decimal_value(text),
        _ => f64::NAN,
    }
}

/// `parseFloat(string)` (§19.2.4): the longest `StrDecimalLiteral` prefix
/// after leading white space, or `NaN` when there is none.
#[must_use]
pub fn parse_float(s: &str) -> f64 {
    let text = js_trim_start(s);
    match scan_decimal_literal(text) {
        Some(len) => decimal_value(&text[..len]),
        None => f64::NAN,
    }
}

/// `Number::toString(n, 10)` (§6.1.6.1.20), as `String(n)` prints it:
/// shortest round-trip digits, `-0` as `0`, exponents from `1e21` and below
/// `1e-6`.
#[must_use]
pub fn number_to_string(n: f64) -> String {
    ryu_js::Buffer::new().format(n).to_owned()
}

/// `Number.isInteger(n)` (§21.1.2.3).
#[must_use]
pub fn is_integer(n: f64) -> bool {
    n.is_finite() && n.trunc() == n
}

/// The value of a text already validated as a `StrDecimalLiteral`.
fn decimal_value(literal: &str) -> f64 {
    let (negative, unsigned) = match literal.as_bytes().first() {
        Some(b'-') => (true, &literal[1..]),
        Some(b'+') => (false, &literal[1..]),
        _ => (false, literal),
    };
    let magnitude = if unsigned == INFINITY_LITERAL {
        f64::INFINITY
    } else {
        // The literal was validated against the ECMAScript grammar, which is a
        // subset of what Rust's correctly rounded parser accepts.
        unsigned.parse::<f64>().unwrap_or(f64::NAN)
    };
    if negative { -magnitude } else { magnitude }
}

/// The byte length of the longest prefix of `text` that is a
/// `StrDecimalLiteral` (§7.1.4.1), if any.
fn scan_decimal_literal(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut pos = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    if text[pos..].starts_with(INFINITY_LITERAL) {
        return Some(pos + INFINITY_LITERAL.len());
    }
    let integer_digits = count_digits(&bytes[pos..]);
    pos += integer_digits;
    let mut fraction_digits = 0;
    if bytes.get(pos) == Some(&b'.') {
        fraction_digits = count_digits(&bytes[pos + 1..]);
        if integer_digits > 0 || fraction_digits > 0 {
            pos += 1 + fraction_digits;
        }
    }
    if integer_digits == 0 && fraction_digits == 0 {
        return None;
    }
    if matches!(bytes.get(pos), Some(b'e' | b'E')) {
        let mut exponent = pos + 1;
        if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
            exponent += 1;
        }
        let exponent_digits = count_digits(&bytes[exponent..]);
        if exponent_digits > 0 {
            pos = exponent + exponent_digits;
        }
    }
    Some(pos)
}

/// The number of leading ASCII decimal digits.
fn count_digits(bytes: &[u8]) -> usize {
    bytes.iter().take_while(|b| b.is_ascii_digit()).count()
}

/// A `NonDecimalIntegerLiteral` without separators (§12.9.3), rounded to the
/// nearest binary64 value, ties to even.
fn non_decimal_integer(text: &str) -> Option<f64> {
    let bytes = text.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'0' {
        return None;
    }
    let bits_per_digit = match bytes[1] {
        b'b' | b'B' => BINARY_DIGIT_BITS,
        b'o' | b'O' => OCTAL_DIGIT_BITS,
        b'x' | b'X' => HEX_DIGIT_BITS,
        _ => return None,
    };
    let digits = &text[2..];
    if digits.is_empty() {
        // `0x` alone is not a literal; the decimal scan rejects it as well.
        return Some(f64::NAN);
    }
    let radix = 1_u32 << bits_per_digit;
    let mut bits: Vec<bool> = Vec::with_capacity(digits.len() * bits_per_digit as usize);
    for c in digits.chars() {
        let Some(digit) = c.to_digit(radix) else {
            return Some(f64::NAN);
        };
        for shift in (0..bits_per_digit).rev() {
            bits.push((digit >> shift) & 1 == 1);
        }
    }
    Some(round_bits(&bits))
}

/// The binary64 value nearest to an unsigned integer given as big-endian
/// bits, ties to even.
fn round_bits(bits: &[bool]) -> f64 {
    let Some(first_one) = bits.iter().position(|bit| *bit) else {
        return 0.0;
    };
    let significant = &bits[first_one..];
    let fold = |acc: u64, bit: &bool| (acc << 1) | u64::from(*bit);
    if significant.len() <= F64_PRECISION_BITS {
        // At most 53 bits: exactly representable.
        return significant.iter().fold(0, fold) as f64;
    }
    let mut mantissa = significant[..F64_PRECISION_BITS].iter().fold(0, fold);
    let round_bit = significant[F64_PRECISION_BITS];
    let sticky = significant[F64_PRECISION_BITS + 1..].iter().any(|bit| *bit);
    let mut exponent = significant.len() - F64_PRECISION_BITS;
    if round_bit && (sticky || mantissa & 1 == 1) {
        mantissa += 1;
        if mantissa == 1 << F64_PRECISION_BITS {
            mantissa >>= 1;
            exponent += 1;
        }
    }
    // The mantissa has at most 53 bits, so the conversion and the product
    // with a power of two are exact or overflow to infinity.
    mantissa as f64 * power_of_two(exponent)
}

/// `2^exponent` as a binary64 value, infinity when it overflows.
fn power_of_two(exponent: usize) -> f64 {
    if exponent > F64_EXPONENT_BIAS {
        return f64::INFINITY;
    }
    f64::from_bits(((exponent + F64_EXPONENT_BIAS) as u64) << F64_EXPONENT_SHIFT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn same(a: f64, b: f64) -> bool {
        (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
    }

    #[test]
    fn string_to_number() {
        let cases: &[(&str, f64)] = &[
            ("", 0.0),
            ("   ", 0.0),
            (" 12 ", 12.0),
            ("\n\t12\u{2028}", 12.0),
            ("\u{FEFF}7\u{00A0}", 7.0),
            ("12", 12.0),
            ("012", 12.0),
            ("-0", -0.0),
            ("+1.5", 1.5),
            (".5", 0.5),
            ("5.", 5.0),
            ("-.5e-3", -0.0005),
            ("1e21", 1e21),
            ("1E+2", 100.0),
            ("Infinity", f64::INFINITY),
            ("+Infinity", f64::INFINITY),
            ("-Infinity", f64::NEG_INFINITY),
            ("0x1F", 31.0),
            ("0X1f", 31.0),
            ("0o17", 15.0),
            ("0b101", 5.0),
            ("0x", f64::NAN),
            ("-0x10", f64::NAN),
            ("0x1g", f64::NAN),
            ("1_0", f64::NAN),
            ("1e", f64::NAN),
            (".", f64::NAN),
            ("-", f64::NAN),
            ("infinity", f64::NAN),
            ("inf", f64::NAN),
            ("NaN", f64::NAN),
            ("1px", f64::NAN),
            ("1 2", f64::NAN),
            ("\u{0085}1", f64::NAN),
        ];
        for (input, expected) in cases {
            let actual = to_number(input);
            assert!(same(actual, *expected), "{input:?}: {actual} != {expected}");
        }
    }

    #[test]
    fn non_decimal_literals_round_to_nearest_even() {
        // 2^53 + 1 rounds down to 2^53 (tie to even), 2^53 + 3 rounds up.
        assert_eq!(to_number("0x20000000000001"), 9_007_199_254_740_992.0);
        assert_eq!(to_number("0x20000000000003"), 9_007_199_254_740_996.0);
        // Above the tie with a sticky bit.
        assert_eq!(to_number("0x200000000000011"), 144_115_188_075_855_904.0);
        assert_eq!(to_number(&format!("0x{}", "f".repeat(300))), f64::INFINITY);
        assert_eq!(to_number("0x0000"), 0.0);
    }

    #[test]
    fn parse_float_takes_the_longest_prefix() {
        let cases: &[(&str, f64)] = &[
            ("1.5rem", 1.5),
            ("  -2px", -2.0),
            ("1e3x", 1000.0),
            ("1e", 1.0),
            ("1e+", 1.0),
            (".5.5", 0.5),
            ("5.", 5.0),
            ("Infinityx", f64::INFINITY),
            ("-Infinity", f64::NEG_INFINITY),
            ("0x10", 0.0),
            ("calc(1px)", f64::NAN),
            ("", f64::NAN),
            ("-", f64::NAN),
            ("-0", -0.0),
        ];
        for (input, expected) in cases {
            let actual = parse_float(input);
            assert!(same(actual, *expected), "{input:?}: {actual} != {expected}");
        }
    }

    #[test]
    fn number_to_string_matches_javascript() {
        assert_eq!(number_to_string(0.07 * 100.0), "7.000000000000001");
        assert_eq!(number_to_string(1e21), "1e+21");
        assert_eq!(number_to_string(1e20), "100000000000000000000");
        assert_eq!(number_to_string(-0.0), "0");
        assert_eq!(number_to_string(0.5), "0.5");
        assert_eq!(number_to_string(1e-7), "1e-7");
        assert_eq!(number_to_string(0.000001), "0.000001");
        assert_eq!(number_to_string(25.0), "25");
        assert_eq!(number_to_string(f64::NAN), "NaN");
        assert_eq!(number_to_string(f64::INFINITY), "Infinity");
        assert_eq!(number_to_string(f64::NEG_INFINITY), "-Infinity");
    }

    #[test]
    fn integers() {
        assert!(is_integer(1.0));
        assert!(is_integer(-0.0));
        assert!(is_integer(1e300));
        assert!(!is_integer(1.5));
        assert!(!is_integer(f64::NAN));
        assert!(!is_integer(f64::INFINITY));
    }

    #[test]
    fn trimming_and_comparison() {
        assert_eq!(js_trim("\u{FEFF} a \u{3000}"), "a");
        assert_eq!(js_trim("\u{0085}a"), "\u{0085}a");
        assert_eq!(js_trim_end("a \n"), "a");
        assert_eq!(js_trim_start(" \ra "), "a ");
        // U+FF5E is above the surrogate range, U+1F600 is encoded below it.
        assert_eq!(compare_utf16("\u{1F600}", "\u{FF5E}"), Ordering::Less);
        assert_eq!("\u{1F600}".cmp("\u{FF5E}"), Ordering::Greater);
        assert_eq!(compare_utf16("ab", "abc"), Ordering::Less);
    }
}
