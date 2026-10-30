//! `Number.prototype.toString(radix)` host import over the shared
//! `varta-intrinsics` implementation (also linked into v38 guests).
use crate::{RuntimeContext, host};
use varta_common::HostOperation;
use wasmtime::{Caller, Linker};
fn format(value: f64, radix: f64) -> Result<String, wasmtime::Error> {
    varta_intrinsics::radix::format(value, radix)
        .map_err(|error| wasmtime::Error::msg(error.to_string()))
}
pub(super) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "varta",
        HostOperation::NumberToString.name(),
        |mut caller: Caller<'_, RuntimeContext>,
         value: f64,
         radix: f64|
         -> Result<i32, wasmtime::Error> {
            caller.data_mut().tick()?;
            let text = format(value, radix)?;
            let pointer = host::result16(&mut caller, text.as_bytes())
                .map_err(|_| wasmtime::Error::msg("numeric radix result transfer"))?;
            caller.data().check_budget()?;
            Ok(pointer)
        },
    )?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use num_bigint::BigUint;
    use varta_intrinsics::radix::MAX_TEXT_BYTES;
    #[test]
    fn number_radix_known_integer_fraction_and_extreme_values() {
        for (x, r, text) in [
            (255.0, 16.0, "ff"),
            (-255.0, 16.0, "-ff"),
            (0.5, 2.0, "0.1"),
            (0.1, 16.0, "0.1999999999999a"),
            (35.0, 36.0, "z"),
            (36.0, 36.0, "10"),
            (1.5, 3.0, "1.111111111111111111111111111111111"),
            (1e21, 10.0, "1e+21"),
            (-0.0, 2.0, "0"),
        ] {
            assert_eq!(format(x, r).unwrap(), text, "{x}/{r}");
        }
        assert_eq!(
            format(f64::from_bits(1), 2.0).unwrap(),
            format!("0.{}1", "0".repeat(1073))
        );
        assert_eq!(
            format(f64::MAX, 2.0).unwrap(),
            format!("{}{}", "1".repeat(53), "0".repeat(971))
        );
    }
    fn rounded(n: BigUint, d: BigUint) -> BigUint {
        let mut q = &n / &d;
        let twice = (&n % &d) << 1_usize;
        if twice > d || (twice == d && q.bit(0)) {
            q += 1_u8;
        }
        q
    }
    fn ratio_bits(n: BigUint, d: BigUint) -> u64 {
        if n == BigUint::from(0_u8) {
            return 0;
        }
        let mut exponent = n.bits() as i32 - d.bits() as i32;
        if (exponent >= 0 && n < (&d << exponent as usize))
            || (exponent < 0 && (&n << (-exponent) as usize) < d)
        {
            exponent -= 1;
        }
        if exponent < -1022 {
            return rounded(n << 1074_usize, d)
                .to_u64_digits()
                .first()
                .copied()
                .unwrap_or(0);
        }
        let shift = 52 - exponent;
        let q = if shift >= 0 {
            rounded(n << shift as usize, d)
        } else {
            rounded(n, d << (-shift) as usize)
        };
        let mut significand = q.to_u64_digits()[0];
        if significand == 1 << 53 {
            significand >>= 1;
            exponent += 1;
        }
        if exponent > 1023 {
            return f64::INFINITY.to_bits();
        }
        (((exponent + 1023) as u64) << 52) | (significand & 0x000f_ffff_ffff_ffff)
    }
    fn representation_bits(significand: BigUint, exponent: i32, radix: u32) -> u64 {
        if exponent >= 0 {
            ratio_bits(
                significand * BigUint::from(radix).pow(exponent as u32),
                BigUint::from(1_u8),
            )
        } else {
            ratio_bits(significand, BigUint::from(radix).pow((-exponent) as u32))
        }
    }
    fn verify(x: f64, radix: u32) {
        let text = format(x, f64::from(radix)).unwrap();
        assert!(text.len() <= MAX_TEXT_BYTES);
        if !x.is_finite() {
            assert_eq!(text, crate::number::canonical_display(x));
            return;
        }
        if x == 0.0 {
            assert_eq!(text, "0");
            return;
        }
        if radix == 10 {
            assert_eq!(text.parse::<f64>().unwrap().to_bits(), x.to_bits());
            return;
        }
        assert_eq!(text.starts_with('-'), x.is_sign_negative());
        let unsigned = text.strip_prefix('-').unwrap_or(&text);
        assert!(!unsigned.starts_with('0') || unsigned.starts_with("0."));
        assert!(!unsigned.contains('.') || !unsigned.ends_with('0'));
        assert!(unsigned.bytes().all(|ch| ch == b'.'
            || (ch.is_ascii_digit() && u32::from(ch - b'0') < radix)
            || (ch.is_ascii_lowercase() && u32::from(ch - b'a') + 10 < radix)));
        let fraction = unsigned.split_once('.').map_or(0, |(_, f)| f.len());
        let digits = unsigned.replace('.', "");
        let digits = digits.trim_start_matches('0');
        let significant = digits.trim_end_matches('0');
        let exponent = -(fraction as i32) + (digits.len() - significant.len()) as i32;
        let number = BigUint::parse_bytes(significant.as_bytes(), radix).unwrap();
        let bits = representation_bits(number.clone(), exponent, radix);
        assert_eq!(bits, x.abs().to_bits(), "{x}/{radix}: {text}");
        if significant.len() > 1 {
            let shorter = number / radix;
            for candidate in [shorter.clone(), shorter + 1_u8] {
                assert_ne!(
                    representation_bits(candidate, exponent + 1, radix),
                    x.abs().to_bits(),
                    "nonminimal {x}/{radix}: {text}"
                );
            }
        }
    }
    #[test]
    fn number_radix_all_bases_round_trip_minimally_against_independent_rational_parsing() {
        for radix in 2..=36 {
            for exponent in (0..=2046_u64).step_by(31).chain([1, 1022, 1023, 2046]) {
                for fraction in [0, 1, 0xf_ffff_ffff_ffff] {
                    let x = f64::from_bits((exponent << 52) | fraction);
                    verify(x, radix);
                    verify(-x, radix);
                }
            }
            for x in [
                0.0,
                -0.0,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NAN,
                0.1,
                0.5,
                1.5,
                f64::from_bits(1),
                f64::MAX,
            ] {
                verify(x, radix);
            }
        }
    }
    #[test]
    fn number_radix_every_exponent_in_representative_even_and_odd_bases_is_bounded() {
        for exponent in 0..=2046_u64 {
            let x = f64::from_bits((exponent << 52) | 0x7_ffff_ffff_ffff);
            for radix in [2, 3, 8, 16, 36] {
                verify(x, radix);
            }
        }
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1.0, 37.0, 2.5] {
            assert!(format(1.0, bad).is_err());
        }
    }
}
