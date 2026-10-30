//! Numeric text: the strict ECMAScript `StringToNumber` grammar and the
//! `parseInt`/`parseFloat` prefix parsers, over UTF-16LE bytes.
use alloc::vec::Vec;
use num_bigint::BigUint;
use varta_common::text::ecmascript_whitespace as whitespace;

fn decimal(bytes: &[u8]) -> f64 {
    let unsigned = if matches!(bytes.first(), Some(b'+' | b'-')) {
        &bytes[1..]
    } else {
        bytes
    };
    if unsigned == b"Infinity" {
        return if bytes.first() == Some(&b'-') {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let mut at = 0;
    while unsigned.get(at).is_some_and(u8::is_ascii_digit) {
        at += 1;
    }
    let mut digits = at;
    if unsigned.get(at) == Some(&b'.') {
        at += 1;
        let start = at;
        while unsigned.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        digits += at - start;
    }
    if digits == 0 {
        return f64::NAN;
    }
    if matches!(unsigned.get(at), Some(b'e' | b'E')) {
        at += 1;
        if matches!(unsigned.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        let start = at;
        while unsigned.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        if at == start {
            return f64::NAN;
        }
    }
    if at != unsigned.len() {
        return f64::NAN;
    }
    // Grammar is checked first: Rust's additional inf/NaN spellings are not admitted.
    core::str::from_utf8(bytes)
        .ok()
        .and_then(|text| text.parse::<f64>().ok())
        .unwrap_or(f64::NAN)
}
fn prefixed(bytes: &[u8], width: u32) -> f64 {
    if bytes.is_empty() {
        return f64::NAN;
    }
    let mut bits = 0_u32;
    let mut leading = 0_u64;
    let mut guard = false;
    let mut sticky = false;
    for &ch in bytes {
        let digit = match ch {
            b'0'..=b'9' => u32::from(ch - b'0'),
            b'a'..=b'f' => u32::from(ch - b'a') + 10,
            b'A'..=b'F' => u32::from(ch - b'A') + 10,
            _ => return f64::NAN,
        };
        if digit >= 1 << width {
            return f64::NAN;
        }
        for shift in (0..width).rev() {
            let bit = (digit >> shift) & 1;
            if bits == 0 && bit == 0 {
                continue;
            }
            bits += 1;
            if bits <= 53 {
                leading = (leading << 1) | u64::from(bit);
            } else if bits == 54 {
                guard = bit != 0;
            } else {
                sticky |= bit != 0;
            }
        }
    }
    if bits <= 53 {
        return leading as f64;
    }
    if guard && (sticky || leading & 1 != 0) {
        leading += 1;
    }
    if leading == 1 << 53 {
        leading >>= 1;
        bits += 1;
    }
    let exponent = bits - 1;
    if exponent > 1023 {
        return f64::INFINITY;
    }
    f64::from_bits((u64::from(exponent + 1023) << 52) | (leading & 0x000f_ffff_ffff_ffff))
}
/// ECMAScript `StringToNumber` of UTF-16LE bytes (`Number("…")`).
#[must_use]
pub fn string_number(bytes: &[u8]) -> f64 {
    let unit = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let mut start = 0;
    let mut end = bytes.len() & !1;
    while start < end && whitespace(unit(start)) {
        start += 2;
    }
    while end > start && whitespace(unit(end - 2)) {
        end -= 2;
    }
    if start == end {
        return 0.0;
    }
    let mut ascii = Vec::with_capacity((end - start) / 2);
    for at in (start..end).step_by(2) {
        let ch = unit(at);
        if ch > 127 {
            return f64::NAN;
        }
        ascii.push(ch as u8);
    }
    if ascii.first() == Some(&b'0') && ascii.len() >= 2 {
        let width = match ascii[1] {
            b'x' | b'X' => Some(4),
            b'o' | b'O' => Some(3),
            b'b' | b'B' => Some(1),
            _ => None,
        };
        if let Some(width) = width {
            return prefixed(&ascii[2..], width);
        }
    }
    decimal(&ascii)
}

fn unit(bytes: &[u8], at: usize) -> Option<u16> {
    bytes
        .get(at * 2..at * 2 + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
}
fn trim_start(bytes: &[u8]) -> usize {
    let mut at = 0;
    while unit(bytes, at).is_some_and(whitespace) {
        at += 1;
    }
    at
}
fn digit(unit: u16) -> Option<u32> {
    Some(match unit {
        0x30..=0x39 => u32::from(unit - 0x30),
        0x41..=0x5a => u32::from(unit - 0x41) + 10,
        0x61..=0x7a => u32::from(unit - 0x61) + 10,
        _ => return None,
    })
}
/// ECMAScript `ToInt32` of a Number.
#[must_use]
pub fn int32(value: f64) -> i32 {
    let bits = value.to_bits();
    let exponent = ((bits >> 52) & 0x7ff) as i32 - 1023;
    if !(0..84).contains(&exponent) {
        return 0;
    }
    let significand = (bits & 0xf_ffff_ffff_ffff) | (1 << 52);
    let magnitude = if exponent < 52 {
        (significand >> (52 - exponent)) as u32
    } else {
        (significand as u32).wrapping_shl((exponent - 52) as u32)
    };
    if value.is_sign_negative() {
        magnitude.wrapping_neg() as i32
    } else {
        magnitude as i32
    }
}
fn rounded(value: &BigUint) -> f64 {
    let mut bits = value.bits();
    if bits <= 53 {
        return value.to_u64_digits().first().copied().unwrap_or(0) as f64;
    }
    if bits > 1024 {
        return f64::INFINITY;
    }
    let shift = (bits - 53) as usize;
    let mut leading = (value >> shift)
        .to_u64_digits()
        .first()
        .copied()
        .unwrap_or(0);
    let remainder = value - (BigUint::from(leading) << shift);
    let half = BigUint::from(1_u8) << (shift - 1);
    if remainder > half || (remainder == half && leading & 1 != 0) {
        leading += 1;
    }
    if leading == 1 << 53 {
        leading >>= 1;
        bits += 1;
    }
    if bits > 1024 {
        return f64::INFINITY;
    }
    f64::from_bits(((bits + 1022) << 52) | (leading & 0xf_ffff_ffff_ffff))
}
/// `parseInt(text, radix)` over UTF-16LE bytes, with `radix` already `ToInt32`.
#[must_use]
pub fn integer(bytes: &[u8], radix: i32) -> f64 {
    let mut at = trim_start(bytes);
    let negative = unit(bytes, at) == Some(0x2d);
    if matches!(unit(bytes, at), Some(0x2b | 0x2d)) {
        at += 1;
    }
    if radix != 0 && !(2..=36).contains(&radix) {
        return f64::NAN;
    }
    let strip = radix == 0 || radix == 16;
    let mut radix = if radix == 0 { 10 } else { radix as u32 };
    if strip && unit(bytes, at) == Some(0x30) && matches!(unit(bytes, at + 1), Some(0x78 | 0x58)) {
        radix = 16;
        at += 2;
    }
    let first = at;
    let mut value = BigUint::from(0_u8);
    while let Some(digit) = unit(bytes, at).and_then(digit).filter(|&n| n < radix) {
        value *= radix;
        value += digit;
        at += 1;
        // Every extension is >= this overflowing prefix; its suffix cannot change the result.
        if value.bits() > 1024 {
            return if negative {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            };
        }
    }
    if at == first {
        return f64::NAN;
    }
    let result = rounded(&value);
    if negative { -result } else { result }
}
/// `parseFloat(text)` over UTF-16LE bytes.
#[must_use]
pub fn float(bytes: &[u8]) -> f64 {
    let start = trim_start(bytes);
    let mut at = start;
    let negative = unit(bytes, at) == Some(0x2d);
    if matches!(unit(bytes, at), Some(0x2b | 0x2d)) {
        at += 1;
    }
    if b"Infinity"
        .iter()
        .enumerate()
        .all(|(offset, &ch)| unit(bytes, at + offset) == Some(u16::from(ch)))
    {
        return if negative {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }
    let digits_start = at;
    while unit(bytes, at).is_some_and(|ch| (0x30..=0x39).contains(&ch)) {
        at += 1;
    }
    let mut digits = at - digits_start;
    if unit(bytes, at) == Some(0x2e) {
        at += 1;
        let fraction = at;
        while unit(bytes, at).is_some_and(|ch| (0x30..=0x39).contains(&ch)) {
            at += 1;
        }
        digits += at - fraction;
    }
    if digits == 0 {
        return f64::NAN;
    }
    let mut end = at;
    if matches!(unit(bytes, at), Some(0x65 | 0x45)) {
        at += 1;
        if matches!(unit(bytes, at), Some(0x2b | 0x2d)) {
            at += 1;
        }
        let exponent = at;
        while unit(bytes, at).is_some_and(|ch| (0x30..=0x39).contains(&ch)) {
            at += 1;
        }
        if at > exponent {
            end = at;
        }
    }
    // Every unit in start..end was matched as ASCII above.
    let ascii = (start..end)
        .filter_map(|i| unit(bytes, i))
        .map(|unit| unit as u8)
        .collect::<Vec<_>>();
    core::str::from_utf8(&ascii)
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(f64::NAN)
}
