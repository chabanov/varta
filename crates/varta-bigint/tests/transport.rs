use varta_bigint::{BinaryOp, Budget, Error, Integer};
use varta_common::{
    bigint::{BigIntView, MAX_PAYLOAD_BYTES, META_BYTES},
    value::ErrorClass,
};
fn budget() -> Budget {
    Budget::new(16_384, 1_000_000)
}
fn parse(text: &str) -> Integer {
    Integer::from_string(text, &mut budget()).unwrap()
}
#[test]
fn binary_transport_roundtrips_exact_digits_across_word_boundaries() {
    for bits in [
        0, 1, 7, 8, 15, 16, 31, 32, 33, 63, 64, 65, 1023, 1024, 4096, 8191,
    ] {
        for sign in [1, -1] {
            let base = Integer::from_i64(sign, &mut budget()).unwrap();
            let value = if bits == 0 {
                parse("0")
            } else {
                base.binary(
                    BinaryOp::LeftShift,
                    &Integer::from_i64(bits - 1, &mut budget()).unwrap(),
                    &mut budget(),
                )
                .unwrap()
            };
            let payload = value.to_payload(&mut budget()).unwrap();
            assert_eq!(
                BigIntView::from_payload(&payload).unwrap().bits(),
                bits as u64
            );
            assert_eq!(
                Integer::from_payload(&payload, &mut budget()).unwrap(),
                value
            );
        }
    }
    let value = parse("-340282366920938463463374607431768211455");
    let bytes = value.to_payload(&mut budget()).unwrap();
    assert_eq!(&bytes[..META_BYTES], &[2, 0, 0, 0, 4, 0, 0, 0]);
    assert!(bytes[META_BYTES..].iter().all(|b| *b == 255));
    let zero = parse("0").to_payload(&mut budget()).unwrap();
    assert_eq!(zero, vec![0; META_BYTES]);
}
#[test]
fn binary_transport_errors_never_become_guest_syntax_errors() {
    for invalid in [vec![], vec![0; 7], vec![1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]] {
        assert_eq!(
            Integer::from_payload(&invalid, &mut budget()),
            Err(Error::InvalidPayload)
        );
    }
    for error in [Error::InvalidPayload, Error::BitCapacity, Error::WorkBudget] {
        assert_eq!(error.recoverable_class(), None);
    }
    assert_eq!(
        Error::Syntax.recoverable_class(),
        Some(ErrorClass::SyntaxError)
    );
    assert_eq!(Error::Type.recoverable_class(), Some(ErrorClass::TypeError));
    for error in [
        Error::DivisionByZero,
        Error::NegativeExponent,
        Error::NonIntegralNumber,
        Error::InvalidRadix,
        Error::InvalidWidth,
    ] {
        assert_eq!(error.recoverable_class(), Some(ErrorClass::RangeError));
    }
}
#[test]
fn binary_transport_budgets_are_checked_before_copying_digits() {
    let payload = parse("256").to_payload(&mut budget()).unwrap();
    assert_eq!(
        Integer::from_payload(&payload, &mut Budget::new(8, 100)),
        Err(Error::BitCapacity)
    );
    assert_eq!(
        Integer::from_payload(&payload, &mut Budget::new(9, 0)),
        Err(Error::WorkBudget)
    );
    assert_eq!(
        parse("256").to_payload(&mut Budget::new(8, 100)),
        Err(Error::BitCapacity)
    );
    assert_eq!(
        parse("256").to_payload(&mut Budget::new(9, 0)),
        Err(Error::WorkBudget)
    );
}
#[test]
fn large_shift_charges_destination_work_without_changing_operands() {
    let one = parse("1");
    let count = parse("8191");
    let mut too_little = Budget::new(8192, 2);
    assert_eq!(
        one.binary(BinaryOp::LeftShift, &count, &mut too_little),
        Err(Error::WorkBudget)
    );
    assert_eq!(too_little.remaining_work(), 0);
    let mut exact = Budget::new(8192, 130);
    let result = one.binary(BinaryOp::LeftShift, &count, &mut exact).unwrap();
    assert_eq!(result.bits(), 8192);
    assert_eq!(exact.spent_work(), 130);
    assert_eq!(one, parse("1"));
    assert_eq!(count, parse("8191"));
    let zero = parse("0");
    assert_eq!(
        zero.binary(BinaryOp::LeftShift, &count, &mut Budget::new(0, 2))
            .unwrap(),
        zero
    );
}
#[test]
fn binary_transport_exact_maximum_and_one_digit_over() {
    let maximum_bits = ((MAX_PAYLOAD_BYTES - META_BYTES) * 8) as u64;
    let mut available = Budget::new(maximum_bits + 32, 1_000_000);
    let count = Integer::from_i64((maximum_bits - 1) as i64, &mut available).unwrap();
    let largest = parse("1")
        .binary(BinaryOp::LeftShift, &count, &mut available)
        .unwrap();
    let bytes = largest.to_payload(&mut available).unwrap();
    assert_eq!(bytes.len(), MAX_PAYLOAD_BYTES);
    assert_eq!(
        Integer::from_payload(&bytes, &mut available).unwrap(),
        largest
    );
    let wider = largest
        .binary(BinaryOp::LeftShift, &parse("32"), &mut available)
        .unwrap();
    assert_eq!(wider.to_payload(&mut available), Err(Error::BitCapacity));
}
#[test]
fn unsigned_digit_import_uses_exact_canonical_bits_and_explicit_work() {
    let value = Integer::from_unsigned_digits(&[0, 1], &mut Budget::new(33, 1)).unwrap();
    assert_eq!(value, parse("4294967296"));
    assert_eq!(
        Integer::from_unsigned_digits(&[], &mut Budget::new(0, 1)).unwrap(),
        parse("0")
    );
    assert_eq!(
        Integer::from_unsigned_digits(&[0], &mut budget()),
        Err(Error::InvalidPayload)
    );
    assert_eq!(
        Integer::from_unsigned_digits(&[1, 0], &mut budget()),
        Err(Error::InvalidPayload)
    );
    assert_eq!(
        Integer::from_unsigned_digits(&[0, 1], &mut Budget::new(32, 1)),
        Err(Error::BitCapacity)
    );
    let mut work = Budget::new(64, 3);
    work.consume_work(3).unwrap();
    assert_eq!(work.remaining_work(), 0);
    assert_eq!(work.spent_work(), 3);
    assert_eq!(work.consume_work(1), Err(Error::WorkBudget));
}
