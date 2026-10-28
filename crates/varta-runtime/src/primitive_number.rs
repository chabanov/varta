//! ABI23 primitive Number conversion, with strict ECMAScript numeric-string grammar.
use crate::RuntimeContext;
use varta_common::{
    HostOperation,
    value::{PrimitiveBox, ValueKind},
};
use wasmtime::{Caller, Extern, Linker};
pub(super) fn whitespace(unit: u16) -> bool {
    matches!(unit,0x0009..=0x000d|0x0020|0x00a0|0x1680|0x2000..=0x200a|0x2028|0x2029|0x202f|0x205f|0x3000|0xfeff)
}
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
    std::str::from_utf8(bytes)
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
fn string_number(bytes: &[u8]) -> f64 {
    let unit = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let mut start = 0;
    let mut end = bytes.len();
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
pub(super) fn convert(bytes: &[u8], cell: i64) -> Result<f64, wasmtime::Error> {
    let value = PrimitiveBox::read(bytes, cell as u64).map_err(wasmtime::Error::new)?;
    Ok(match value.kind {
        ValueKind::Float64 => f64::from_bits(value.payload),
        ValueKind::Boolean => value.payload as f64,
        ValueKind::Null => 0.0,
        ValueKind::Undefined => f64::NAN,
        ValueKind::String => {
            let start = value.payload as u32 as usize;
            let length = (value.payload >> 32) as usize;
            string_number(&bytes[start..start + length])
        }
        _ => {
            return Err(wasmtime::Error::msg(
                "Number conversion requires a primitive cell",
            ));
        }
    })
}
pub(super) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "varta",
        HostOperation::PrimitiveNumber.name(),
        |mut caller: Caller<'_, RuntimeContext>, cell: i64| -> Result<f64, wasmtime::Error> {
            caller.data_mut().tick()?;
            let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
                return Err(wasmtime::Error::msg("missing value memory"));
            };
            let result = convert(memory.data(&caller), cell)?;
            caller.data().check_budget()?;
            Ok(result)
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(text: &str) -> f64 {
        string_number(&varta_common::text::encode(text))
    }
    fn equal(actual: f64, expected: f64, label: &str) {
        if expected.is_nan() {
            assert!(actual.is_nan(), "{label}: {actual}");
        } else {
            assert_eq!(actual.to_bits(), expected.to_bits(), "{label}");
        }
    }
    #[test]
    fn primitive_number_string_grammar_and_exact_whitespace_match_ecmascript() {
        for (text, value) in [
            ("", 0.0),
            (" ", 0.0),
            ("-0", -0.0),
            ("+.5", 0.5),
            ("1.", 1.0),
            ("1.e+2", 100.0),
            ("00077", 77.0),
            ("-12.5e-1", -1.25),
            ("Infinity", f64::INFINITY),
            ("+Infinity", f64::INFINITY),
            ("-Infinity", f64::NEG_INFINITY),
            ("0xFF", 255.0),
            ("0X10e2", 4322.0),
            ("0o77", 63.0),
            ("0O10", 8.0),
            ("0b11", 3.0),
            ("0B10", 2.0),
            ("9007199254740993", 9007199254740992.0),
            ("9007199254740995", 9007199254740996.0),
            ("2.4703282292062327e-324", 0.0),
            ("2.4703282292062328e-324", f64::from_bits(1)),
            ("1.7976931348623158e308", f64::MAX),
            ("1.7976931348623159e308", f64::INFINITY),
            ("-1e-999999999999999999", -0.0),
            ("0e999999999999999999999", 0.0),
            (
                "1.00000000000000011102230246251565404236316680908203125",
                1.0,
            ),
            (
                "1.00000000000000011102230246251565404236316680908203126",
                f64::from_bits(1.0_f64.to_bits() + 1),
            ),
            (
                "1.00000000000000033306690738754696212708950042724609375",
                f64::from_bits(1.0_f64.to_bits() + 2),
            ),
        ] {
            equal(parse(text), value, text);
        }
        for text in [
            "+",
            "-",
            ".",
            "inf",
            "INF",
            "infinity",
            "+inf",
            "NaN",
            "nan",
            "12junk",
            "1_0",
            "0x",
            "0b",
            "0o",
            "0b2",
            "0o8",
            "0x1p2",
            "0x1e+2",
            "+0x1",
            "-0x1",
            "+0b1",
            "-0o1",
            "0b1.0",
            "0o7n",
            "1n",
            "1,0",
            "1\0",
            "1e",
            "1e+",
            "1e-",
            "1 2",
            "\u{feff}+\u{feff}1",
        ] {
            assert!(parse(text).is_nan(), "{text}");
        }
        let expected = [
            0x0009, 0x000a, 0x000b, 0x000c, 0x000d, 0x0020, 0x00a0, 0x1680, 0x2000, 0x2001, 0x2002,
            0x2003, 0x2004, 0x2005, 0x2006, 0x2007, 0x2008, 0x2009, 0x200a, 0x2028, 0x2029, 0x202f,
            0x205f, 0x3000, 0xfeff,
        ];
        for unit in 0..=u16::MAX {
            assert_eq!(whitespace(unit), expected.contains(&unit), "{unit:x}");
            let bytes = [unit.to_le_bytes(), 49_u16.to_le_bytes(), unit.to_le_bytes()].concat();
            assert_eq!(
                string_number(&bytes) == 1.0,
                expected.contains(&unit),
                "{unit:x}"
            );
        }
    }
    #[test]
    fn primitive_number_prefixed_integers_round_against_independent_u128_casts() {
        for value in 0..=u16::MAX {
            let value = u128::from(value);
            for text in [
                format!("0b{value:b}"),
                format!("0o{value:o}"),
                format!("0x{value:x}"),
            ] {
                equal(parse(&text), value as f64, &text);
            }
        }
        let mut state = 0x9e3779b97f4a7c15_fedcba9876543210_u128;
        for _ in 0..4096 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 43;
            for text in [
                format!("0b{state:b}"),
                format!("0o{state:o}"),
                format!("0x{state:x}"),
            ] {
                equal(parse(&text), state as f64, &text);
            }
        }
        for exponent in 53..128 {
            let center = 1_u128 << exponent;
            let half = 1_u128 << (exponent - 53);
            for value in [
                center - 1,
                center,
                center + half - 1,
                center + half,
                center + half + 1,
                center + 3 * half - 1,
                center + 3 * half,
                center + 3 * half + 1,
            ] {
                let text = format!("0x{value:x}");
                equal(parse(&text), value as f64, &text);
            }
        }
    }
    #[test]
    fn primitive_number_binary64_text_round_trips_all_exponents_and_threshold_neighbors() {
        for exponent in 0..=0x7ff_u64 {
            for fraction in [0, 1, 0x7_ffff_ffff_ffff, 0xf_ffff_ffff_ffff] {
                let x = f64::from_bits((exponent << 52) | fraction);
                for x in [x, -x] {
                    let text = crate::number::canonical_display(x);
                    equal(parse(&text), if x == 0.0 { 0.0 } else { x }, &text);
                }
            }
        }
        for exponent in -323..=308 {
            let x = format!("1e{exponent}").parse::<f64>().unwrap();
            for offset in -2_i64..=2 {
                let x = f64::from_bits(x.to_bits().saturating_add_signed(offset));
                for x in [x, -x] {
                    let text = crate::number::canonical_display(x);
                    equal(parse(&text), if x == 0.0 { 0.0 } else { x }, &text);
                }
            }
        }
    }
    #[test]
    fn primitive_number_large_prefix_goldens_and_maximum_strings_keep_rounding_and_grammar() {
        // Generated independently using CPython arbitrary-precision integer conversion.
        for (text, bits) in [
            (
                "0x1ffffffffffffffffffffffffffffffff",
                0x4800000000000000_u64,
            ),
            (
                "0x200000000000000000000000000000000",
                0x4800000000000000_u64,
            ),
            (
                "0x20000000000000fffffffffffffffffff",
                0x4800000000000000_u64,
            ),
            (
                "0x200000000000010000000000000000000",
                0x4800000000000000_u64,
            ),
            (
                "0x200000000000010000000000000000001",
                0x4800000000000001_u64,
            ),
            (
                "0x200000000000030000000000000000000",
                0x4800000000000002_u64,
            ),
            (
                "0x200000000000030000000000000000001",
                0x4800000000000002_u64,
            ),
            (
                "0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                0x4fe0000000000000_u64,
            ),
            (
                "0x8000000000000000000000000000000000000000000000000000000000000000",
                0x4fe0000000000000_u64,
            ),
            (
                "0x80000000000003ffffffffffffffffffffffffffffffffffffffffffffffffff",
                0x4fe0000000000000_u64,
            ),
            (
                "0x8000000000000400000000000000000000000000000000000000000000000000",
                0x4fe0000000000000_u64,
            ),
            (
                "0x8000000000000400000000000000000000000000000000000000000000000001",
                0x4fe0000000000001_u64,
            ),
            (
                "0x8000000000000c00000000000000000000000000000000000000000000000000",
                0x4fe0000000000002_u64,
            ),
            (
                "0x8000000000000c00000000000000000000000000000000000000000000000001",
                0x4fe0000000000002_u64,
            ),
            (
                "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                0x5ff0000000000000_u64,
            ),
            (
                "0x100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
                0x5ff0000000000000_u64,
            ),
            (
                "0x100000000000007ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                0x5ff0000000000000_u64,
            ),
            (
                "0x100000000000008000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
                0x5ff0000000000000_u64,
            ),
            (
                "0x100000000000008000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001",
                0x5ff0000000000001_u64,
            ),
            (
                "0x100000000000018000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
                0x5ff0000000000002_u64,
            ),
            (
                "0x100000000000018000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001",
                0x5ff0000000000002_u64,
            ),
            (
                "0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                0x7fe0000000000000_u64,
            ),
            (
                "0x8000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
                0x7fe0000000000000_u64,
            ),
            (
                "0x80000000000003ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                0x7fe0000000000000_u64,
            ),
            (
                "0x8000000000000400000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
                0x7fe0000000000000_u64,
            ),
            (
                "0x8000000000000400000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001",
                0x7fe0000000000001_u64,
            ),
            (
                "0x8000000000000c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
                0x7fe0000000000002_u64,
            ),
            (
                "0x8000000000000c00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001",
                0x7fe0000000000002_u64,
            ),
            (
                "0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                0x7ff0000000000000_u64,
            ),
            (
                "0x10000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
                0x7ff0000000000000_u64,
            ),
            (
                "0x100000000000007ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                0x7ff0000000000000_u64,
            ),
            (
                "0x10000000000000800000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
                0x7ff0000000000000_u64,
            ),
            (
                "0x10000000000000800000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001",
                0x7ff0000000000000_u64,
            ),
            (
                "0x10000000000001800000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
                0x7ff0000000000000_u64,
            ),
            (
                "0x10000000000001800000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001",
                0x7ff0000000000000_u64,
            ),
            (
                "0xfffffffffffffbffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
                0x7fefffffffffffff_u64,
            ),
            (
                "0xfffffffffffffc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
                0x7ff0000000000000_u64,
            ),
            (
                "0xfffffffffffffc00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001",
                0x7ff0000000000000_u64,
            ),
        ] {
            assert_eq!(parse(text).to_bits(), bits, "{text}");
        }
        let maximum = varta_common::abi::MAX_STRING_BYTES / 2;
        equal(parse(&" ".repeat(maximum)), 0.0, "maximum whitespace");
        equal(
            parse(&format!("0x{}1", "0".repeat(maximum - 3))),
            1.0,
            "maximum prefixed leading zeros",
        );
        assert!(parse(&format!("0x{}z", "f".repeat(maximum - 3))).is_nan());
        assert!(parse(&format!("1{}x", "0".repeat(maximum - 2))).is_nan());
        equal(
            parse(&format!("1{}", "0".repeat(maximum - 1))),
            f64::INFINITY,
            "maximum decimal overflow",
        );
    }
}
