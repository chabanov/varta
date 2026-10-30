//! ABI23 primitive Number conversion host import over the shared
//! `varta-intrinsics` implementation (also linked into v38 guests).
use crate::RuntimeContext;
use varta_common::HostOperation;
#[cfg(test)]
use varta_intrinsics::parse::string_number;
use wasmtime::{Caller, Extern, Linker};
#[cfg(test)]
pub(super) fn whitespace(unit: u16) -> bool {
    varta_common::text::ecmascript_whitespace(unit)
}
pub(super) fn convert(bytes: &[u8], cell: i64) -> Result<f64, wasmtime::Error> {
    varta_intrinsics::primitive::to_number(bytes, cell as u64, varta_common::abi::MAX_STRING_BYTES)
        .map_err(|error| wasmtime::Error::msg(error.to_string()))
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
