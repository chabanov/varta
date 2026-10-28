use std::cmp::Ordering;
use varta_bigint::{BinaryOp as B, Budget, Error, Integer, UnaryOp as U};

fn budget() -> Budget {
    Budget::new(16384, 10_000_000)
}
fn parse(s: &str) -> Integer {
    Integer::from_string(s, &mut budget()).unwrap()
}
fn text(n: &Integer) -> String {
    n.to_string_radix(10, &mut budget()).unwrap()
}

#[test]
fn string_grammar_and_canonical_transport() {
    for (input, expected) in [
        ("", "0"),
        ("\u{feff} -00012\u{2029}", "-12"),
        ("+0", "0"),
        ("0xff", "255"),
        ("0O17", "15"),
        ("0b101", "5"),
    ] {
        assert_eq!(text(&parse(input)), expected);
        assert_eq!(
            Integer::from_utf16(&input.encode_utf16().collect::<Vec<_>>(), &mut budget()).unwrap(),
            parse(input)
        );
    }
    for s in [
        "+", "-", "-0x1", "+0b1", "1n", "1_000", "1.0", "1e2", "0b2", "0x", "\u{85}1", "１２",
    ] {
        assert_eq!(
            Integer::from_string(s, &mut budget()),
            Err(Error::Syntax),
            "{s}"
        );
    }
    for s in ["", "-0", "00", "-01", "+1", " 1", "0xff"] {
        assert_eq!(
            Integer::from_canonical(s, &mut budget()),
            Err(Error::Syntax)
        );
    }
    assert_eq!(
        Integer::from_utf16(&[0xd800], &mut budget()),
        Err(Error::Syntax)
    );
    for s in ["0", "1", "-123456789012345678901234567890"] {
        assert_eq!(text(&Integer::from_canonical(s, &mut budget()).unwrap()), s);
    }
}

#[test]
fn arithmetic_matches_independent_small_integer_results() {
    for a in -32i128..=32 {
        for b in -16i128..=16 {
            let left = parse(&a.to_string());
            let right = parse(&b.to_string());
            for (op, expected) in [
                (B::Add, a + b),
                (B::Subtract, a - b),
                (B::Multiply, a * b),
                (B::And, a & b),
                (B::Or, a | b),
                (B::Xor, a ^ b),
            ] {
                assert_eq!(
                    text(&left.binary(op, &right, &mut budget()).unwrap()),
                    expected.to_string()
                );
            }
            if b != 0 {
                for (op, expected) in [(B::Divide, a / b), (B::Remainder, a % b)] {
                    assert_eq!(
                        text(&left.binary(op, &right, &mut budget()).unwrap()),
                        expected.to_string()
                    );
                }
            }
        }
        let n = parse(&a.to_string());
        assert_eq!(
            text(&n.unary(U::Not, &mut budget()).unwrap()),
            (!a).to_string()
        );
        assert_eq!(
            text(&n.unary(U::Negate, &mut budget()).unwrap()),
            (-a).to_string()
        );
    }
}

#[test]
fn powers_shifts_and_recoverable_errors() {
    assert_eq!(
        text(
            &parse("-3")
                .binary(B::Power, &parse("5"), &mut budget())
                .unwrap()
        ),
        "-243"
    );
    assert_eq!(
        text(
            &parse("0")
                .binary(B::Power, &parse("0"), &mut budget())
                .unwrap()
        ),
        "1"
    );
    let huge = parse("999999999999999999999999999999999999999");
    assert_eq!(
        text(&parse("-1").binary(B::Power, &huge, &mut budget()).unwrap()),
        "-1"
    );
    assert_eq!(
        text(
            &parse("-3")
                .binary(B::RightShift, &huge, &mut budget())
                .unwrap()
        ),
        "-1"
    );
    assert_eq!(
        text(
            &parse("3")
                .binary(B::LeftShift, &parse("-1"), &mut budget())
                .unwrap()
        ),
        "1"
    );
    assert_eq!(
        text(
            &parse("-3")
                .binary(B::RightShift, &parse("1"), &mut budget())
                .unwrap()
        ),
        "-2"
    );
    for (op, rhs, error) in [
        (B::Divide, "0", Error::DivisionByZero),
        (B::Remainder, "0", Error::DivisionByZero),
        (B::Power, "-1", Error::NegativeExponent),
        (B::UnsignedRightShift, "1", Error::Type),
    ] {
        assert_eq!(
            parse("2").binary(op, &parse(rhs), &mut budget()),
            Err(error)
        );
        assert!(!error.is_resource());
    }
    assert_eq!(parse("1").unary(U::Plus, &mut budget()), Err(Error::Type));
    assert_eq!(
        parse("1").binary(B::LeftShift, &huge, &mut budget()),
        Err(Error::BitCapacity)
    );
}

#[test]
fn number_conversion_rounding_and_exact_comparison() {
    for exponent in 0..1024u64 {
        for mantissa in [0, 1, (1u64 << 51) - 1, (1u64 << 52) - 1] {
            let n = f64::from_bits(((exponent + 1023) << 52) | mantissa);
            if !n.is_finite() || n.fract() != 0.0 {
                continue;
            }
            for v in [n, -n] {
                let integer = Integer::from_number(v, &mut budget()).unwrap();
                assert_eq!(integer.to_number(&mut budget()).unwrap(), v);
                assert_eq!(
                    integer.compare_number(v, &mut budget()).unwrap(),
                    Some(Ordering::Equal)
                );
            }
        }
    }
    for (s, expected) in [
        ("9007199254740993", 9007199254740992.0),
        ("9007199254740995", 9007199254740996.0),
        ("-9007199254740995", -9007199254740996.0),
    ] {
        assert_eq!(parse(s).to_number(&mut budget()).unwrap(), expected);
    }
    assert_eq!(
        parse("9007199254740993")
            .compare_number(9007199254740992.0, &mut budget())
            .unwrap(),
        Some(Ordering::Greater)
    );
    assert_eq!(
        parse("-1").compare_number(-1.5, &mut budget()).unwrap(),
        Some(Ordering::Greater)
    );
    assert_eq!(
        parse("0")
            .compare_number(f64::from_bits(1), &mut budget())
            .unwrap(),
        Some(Ordering::Less)
    );
    assert_eq!(
        parse("1").compare_number(f64::NAN, &mut budget()).unwrap(),
        None
    );
    for v in [0.5, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            Integer::from_number(v, &mut budget()),
            Err(Error::NonIntegralNumber)
        );
    }
}

#[test]
fn signed_and_unsigned_widths() {
    for width in 0..=12u64 {
        let modulus = 1i64 << width;
        for value in -260i64..=260 {
            let n = Integer::from_i64(value, &mut budget()).unwrap();
            let unsigned = value.rem_euclid(modulus);
            let signed = if width > 0 && unsigned >= modulus / 2 {
                unsigned - modulus
            } else {
                unsigned
            };
            assert_eq!(
                text(&n.as_uint_n(width, &mut budget()).unwrap()),
                unsigned.to_string()
            );
            assert_eq!(
                text(&n.as_int_n(width, &mut budget()).unwrap()),
                signed.to_string()
            );
        }
    }
    assert_eq!(
        text(&parse("-1").as_int_n((1 << 53) - 1, &mut budget()).unwrap()),
        "-1"
    );
    assert_eq!(
        parse("-1").as_uint_n((1 << 53) - 1, &mut budget()),
        Err(Error::BitCapacity)
    );
    assert_eq!(
        parse("1").as_int_n(1 << 53, &mut budget()),
        Err(Error::InvalidWidth)
    );
    assert_eq!(Integer::index_from_number(-0.5), Ok(0));
    assert_eq!(Integer::index_from_number(f64::NAN), Ok(0));
    assert_eq!(Integer::index_from_number(-1.0), Err(Error::InvalidWidth));
}

#[test]
fn radix_and_resource_limits_are_explicit() {
    for radix in 2..=36 {
        assert_eq!(
            parse("123456789012345678901234567890")
                .to_string_radix(radix, &mut budget())
                .unwrap(),
            num_bigint::BigInt::parse_bytes(b"123456789012345678901234567890", 10)
                .unwrap()
                .to_str_radix(radix)
        );
    }
    assert_eq!(
        parse("1").to_string_radix(1, &mut budget()),
        Err(Error::InvalidRadix)
    );
    assert_eq!(
        Integer::from_string("256", &mut Budget::new(8, 100)),
        Err(Error::BitCapacity)
    );
    assert_eq!(
        Integer::from_string("255", &mut Budget::new(8, 100))
            .unwrap()
            .bits(),
        8
    );
    let mut exhausted = Budget::new(1024, 0);
    assert_eq!(
        parse("1").binary(B::Add, &parse("2"), &mut exhausted),
        Err(Error::WorkBudget)
    );
    assert_eq!(exhausted.remaining_work(), 0);
    assert!(Error::WorkBudget.is_resource());
    assert!(Error::BitCapacity.is_resource());
    let mut accounting = Budget::new(1024, 100);
    Integer::from_bool(true, &mut accounting).unwrap();
    assert_eq!(accounting.spent_work(), 1);
    assert_eq!(accounting.remaining_work(), 99);
}
