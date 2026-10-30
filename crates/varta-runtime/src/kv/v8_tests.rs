//! The V8 codec against bytes Deno wrote: the Deno 2.9.4 fixture rows
//! (`kv_cases/deno-2.9.4`) and the Deno 2.9.7 probe (`kv_cases/deno-2.9.7-probe`).
use super::*;

const ROWS: &str = include_str!("../kv_cases/deno-2.9.4/rows.json");
const PROBE: &str = include_str!("../kv_cases/deno-2.9.7-probe/rows.txt");
/// The v_encoding of V8 values (`denokv_proto::VALUE_ENCODING_V8`).
const V8_ENCODING: i64 = 1;

const LIMITS: Limits = Limits {
    depth: 64,
    nodes: 100_000,
    elements: 100_000,
    string_units: 1 << 20,
};

fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// `(key bytes, value bytes)` of every V8 row of the 2.9.4 fixture.
fn fixture_rows() -> Vec<(Vec<u8>, Vec<u8>)> {
    let rows: serde_json::Value = serde_json::from_str(ROWS).unwrap();
    rows["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["v_encoding"].as_i64() == Some(V8_ENCODING))
        .map(|row| {
            (
                hex(row["k"].as_str().unwrap()),
                hex(row["v"].as_str().unwrap()),
            )
        })
        .collect()
}

/// `(name, encoding, bytes)` of every probe row.
fn probe_rows() -> Vec<(String, i64, Vec<u8>)> {
    PROBE
        .lines()
        .map(|line| {
            let mut parts = line.split(' ');
            let name = parts.next().unwrap().to_owned();
            let encoding = parts.next().unwrap().parse().unwrap();
            (name, encoding, hex(parts.next().unwrap()))
        })
        .collect()
}

fn probe(name: &str) -> Vec<u8> {
    probe_rows()
        .into_iter()
        .find(|(row, _, _)| row == name)
        .unwrap()
        .2
}

fn fixture(key_suffix: &[&str]) -> Value {
    let mut key = Vec::new();
    for part in std::iter::once("fixture").chain(key_suffix.iter().copied()) {
        key.push(0x02);
        key.extend_from_slice(part.as_bytes());
        key.push(0x00);
    }
    let (_, bytes) = fixture_rows()
        .into_iter()
        .find(|(row, _)| *row == key)
        .unwrap_or_else(|| panic!("no row {key_suffix:?}"));
    decode(&bytes, LIMITS).unwrap()
}

fn object(value: &Value) -> &Object {
    match value {
        Value::Object(object) => object,
        other => panic!("not an object: {other:?}"),
    }
}

#[test]
fn every_deno_2_9_4_fixture_value_reencodes_byte_identically() {
    let rows = fixture_rows();
    assert_eq!(rows.len(), 45);
    for (key, bytes) in rows {
        let value =
            decode(&bytes, LIMITS).unwrap_or_else(|error| panic!("row {key:02x?}: {error}"));
        assert_eq!(encode(&value, usize::MAX).unwrap(), bytes, "row {key:02x?}");
    }
}

#[test]
fn fixture_values_decode_to_the_written_javascript_values() {
    assert_eq!(
        fixture(&["str", "latin1"]),
        Value::String(units("café ñ ü"))
    );
    assert_eq!(
        fixture(&["str", "utf16"]),
        Value::String(units("Привіт 🌍"))
    );
    assert_eq!(fixture(&["str", "empty"]), Value::String(Vec::new()));
    assert_eq!(fixture(&["num", "small"]), Value::Number(42.0));
    assert_eq!(fixture(&["num", "neg"]), Value::Number(-7.0));
    assert_eq!(
        fixture(&["num", "smi_max"]),
        Value::Number(f64::from((1 << 30) - 1))
    );
    assert_eq!(
        fixture(&["num", "beyond_smi"]),
        Value::Number(2f64.powi(31))
    );
    assert_eq!(
        fixture(&["num", "int53"]),
        Value::Number(9_007_199_254_740_991.0)
    );
    let Value::Number(zero) = fixture(&["num", "neg_zero"]) else {
        panic!()
    };
    assert!(zero == 0.0 && zero.is_sign_negative());
    assert!(matches!(fixture(&["num", "nan"]), Value::Number(n) if n.is_nan()));
    assert_eq!(
        fixture(&["num", "neg_inf"]),
        Value::Number(f64::NEG_INFINITY)
    );
    assert_eq!(
        fixture(&["bigint"]),
        Value::BigInt("12345678901234567890".parse().unwrap())
    );
    assert_eq!(fixture(&["bool", "true"]), Value::Boolean(true));
    assert_eq!(fixture(&["null"]), Value::Null);
    assert_eq!(fixture(&["undefined"]), Value::Undefined);
    let flat = fixture(&["obj", "flat"]);
    let Object::Plain(properties) = object(&flat) else {
        panic!()
    };
    let keys: Vec<Key> = properties.iter().map(|(key, _)| key.clone()).collect();
    assert_eq!(
        keys,
        ["a", "b", "c", "d"].map(|name| Key::Name(units(name)))
    );
    // Integer keys are numbers, ahead of names, ascending.
    let numeric = fixture(&["obj", "numeric_keys"]);
    let Object::Plain(properties) = object(&numeric) else {
        panic!()
    };
    let keys: Vec<Key> = properties.iter().map(|(key, _)| key.clone()).collect();
    assert_eq!(keys, [Key::Index(1), Key::Index(10), Key::Name(units("x"))]);
    let undefined_field = fixture(&["obj", "undefined_field"]);
    assert_eq!(
        object(&undefined_field),
        &Object::Plain(vec![
            (Key::Name(units("a")), Value::Number(1.0)),
            (Key::Name(units("b")), Value::Undefined),
        ])
    );
    let holes = fixture(&["arr", "holes"]);
    assert_eq!(
        object(&holes),
        &Object::Sparse {
            length: 3,
            elements: vec![(0, Value::Number(1.0)), (2, Value::Number(3.0))],
        }
    );
    let bytes = fixture(&["bytes_in_obj"]);
    let Object::Plain(properties) = object(&bytes) else {
        panic!()
    };
    assert_eq!(
        object(&properties[0].1),
        &Object::Bytes {
            buffer: vec![9, 8, 7],
            offset: 0,
            length: 3,
        }
    );
    // 2026-10-30T12:00:00.123Z
    assert_eq!(
        object(&fixture(&["date"])),
        &Object::Date(1_793_361_600_123.0)
    );
    assert_eq!(
        object(&fixture(&["map"])),
        &Object::Map(vec![
            (Value::String(units("a")), Value::Number(1.0)),
            (Value::Number(2.0), Value::String(units("b"))),
        ])
    );
    assert_eq!(
        object(&fixture(&["set"])),
        &Object::Set(vec![Value::String(units("a")), Value::Number(1.0)])
    );
}

#[test]
fn deno_2_9_7_probe_values_reencode_byte_identically() {
    // Heap numbers holding small integers are not canonical: V8 wrote them
    // as doubles because of how they were computed.
    const NONCANONICAL: [&str; 1] = ["datediff"];
    let mut checked = 0;
    for (name, encoding, bytes) in probe_rows() {
        if encoding != V8_ENCODING || NONCANONICAL.contains(&name.as_str()) || name == "arrprops" {
            continue;
        }
        let value = decode(&bytes, LIMITS).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(encode(&value, usize::MAX).unwrap(), bytes, "{name}");
        checked += 1;
    }
    assert_eq!(checked, 25);
    // Arrays with named properties are outside the subset.
    assert_eq!(
        decode(&probe("arrprops"), LIMITS),
        Err(V8Error::Unsupported("array with named properties"))
    );
    let heap = probe("datediff");
    assert_eq!(decode(&heap, LIMITS).unwrap(), Value::Number(1500.0));
    assert_eq!(
        encode(&Value::Number(1500.0), usize::MAX).unwrap(),
        hex("ff1049b817")
    );
}

#[test]
fn numbers_use_int32_exactly_in_the_smi_range() {
    let bytes = |number: f64| encode(&Value::Number(number), usize::MAX).unwrap();
    for name in ["smi31m1", "negsmi31", "smi30", "negsmi30m1", "computed"] {
        let value = decode(&probe(name), LIMITS).unwrap();
        let Value::Number(number) = value else {
            panic!()
        };
        assert_eq!(bytes(number), probe(name), "{name}");
        assert_eq!(bytes(number)[2], b'I', "{name}");
    }
    assert_eq!(bytes(2f64.powi(31))[2], b'N');
    assert_eq!(bytes(-(2f64.powi(31)) - 1.0)[2], b'N');
    assert_eq!(bytes(-0.0)[2], b'N');
    assert_eq!(bytes(0.5)[2], b'N');
    // Every NaN is written as V8's canonical quiet NaN.
    assert_eq!(
        bytes(f64::from_bits(0xFFF8_0000_0000_0001)),
        hex("ff104e000000000000f87f")
    );
}

#[test]
fn shared_objects_are_written_once_and_referenced() {
    let value = decode(&probe("shared"), LIMITS).unwrap();
    let Object::Plain(properties) = object(&value) else {
        panic!()
    };
    let (Value::Object(x), Value::Object(y)) = (&properties[0].1, &properties[1].1) else {
        panic!()
    };
    assert!(Rc::ptr_eq(x, y));
    // Equal but distinct objects are written twice.
    let copy = Value::Object(Rc::new(Object::Plain(vec![
        (Key::Name(units("x")), Value::Object(Rc::new((**x).clone()))),
        (Key::Name(units("y")), Value::Object(Rc::new((**x).clone()))),
    ])));
    assert_ne!(encode(&copy, usize::MAX).unwrap(), probe("shared"));
}

#[test]
fn two_byte_strings_are_aligned_with_padding() {
    for name in ["pad1", "pad2", "pad3", "padkey", "lone"] {
        let bytes = probe(name);
        assert_eq!(
            encode(&decode(&bytes, LIMITS).unwrap(), usize::MAX).unwrap(),
            bytes
        );
    }
    // `{ab: "Привіт"}` needs one padding byte before `c`.
    assert_eq!(probe("pad2")[7], tag::PADDING);
    let Value::String(lone) = decode(&probe("lone"), LIMITS).unwrap() else {
        panic!()
    };
    assert_eq!(lone, [0x61, 0xD800, 0x62]);
}

#[test]
fn malformed_values_fail_with_typed_errors() {
    let cases: [(&str, V8Error); 18] = [
        ("", V8Error::Truncated),
        ("ff", V8Error::Truncated),
        ("0f10", V8Error::Header),
        ("ff0c5f", V8Error::Version(12)),
        ("ff115f", V8Error::Version(17)),
        ("ff105f5f", V8Error::Trailing),
        ("ff1049", V8Error::Truncated),
        ("ff104980808080808001", V8Error::Malformed("varint")),
        ("ff104980", V8Error::Truncated),
        ("ff10228000", V8Error::Malformed("varint")),
        ("ff1063036100", V8Error::Malformed("two-byte string")),
        ("ff10ee", V8Error::UnknownTag(0xEE)),
        ("ff1052", V8Error::Unsupported("RegExp")),
        ("ff10725f", V8Error::Unsupported("Error")),
        ("ff105a080000000000000000", V8Error::Malformed("BigInt")),
        ("ff105a01", V8Error::Malformed("BigInt")),
        ("ff106f22016149022201614904", V8Error::DuplicateKey),
        (
            "ff104101490222016b49022401017b",
            V8Error::Unsupported("array with named properties"),
        ),
    ];
    for (input, error) in cases {
        assert_eq!(decode(&hex(input), LIMITS), Err(error), "{input}");
    }
    let more: [(&str, V8Error); 9] = [
        // A reference to the object still being read is a cycle.
        ("ff106f2201615e007b01", V8Error::Cycle),
        (
            "ff106f2201615e057b01",
            V8Error::Malformed("object reference"),
        ),
        // The property count disagrees.
        ("ff106f2201614902", V8Error::Truncated),
        ("ff106f22016149027b02", V8Error::Malformed("property count")),
        // A sparse element outside the length.
        ("ff106101490449024001", V8Error::Malformed("array index")),
        // A buffer without a view, a view of another type, and a view
        // beyond its buffer.
        ("ff1042020102", V8Error::Truncated),
        (
            "ff104202010256620002",
            V8Error::Unsupported("typed array other than Uint8Array"),
        ),
        (
            "ff10420201025642010200",
            V8Error::Malformed("array buffer view range"),
        ),
        (
            "ff10420201025642000201",
            V8Error::Unsupported("length-tracking or resizable view"),
        ),
    ];
    for (input, error) in more {
        assert_eq!(decode(&hex(input), LIMITS), Err(error), "{input}");
    }
    // A length beyond the input fails before any allocation.
    assert_eq!(
        decode(&hex("ff1041ffffff0f"), LIMITS),
        Err(V8Error::Limit("array"))
    );
    assert_eq!(decode(&hex("ff104164"), LIMITS), Err(V8Error::Truncated));
}

#[test]
fn reads_are_bounded() {
    let nested = |depth: usize| {
        let mut bytes = hex("ff10");
        bytes.extend(std::iter::repeat_n(b'A', depth).flat_map(|tag| [tag, 1]));
        bytes.push(b'_');
        for _ in 0..depth {
            bytes.extend([b'$', 0, 1]);
        }
        bytes
    };
    let limits = Limits { depth: 8, ..LIMITS };
    assert!(decode(&nested(8), limits).is_ok());
    assert_eq!(decode(&nested(9), limits), Err(V8Error::Limit("depth")));
    let few = Limits { nodes: 3, ..LIMITS };
    assert_eq!(
        decode(&hex("ff104103490249044906240003"), few),
        Err(V8Error::Limit("node"))
    );
    let short = Limits {
        string_units: 4,
        ..LIMITS
    };
    assert_eq!(
        decode(&hex("ff10220568656c6c6f"), short),
        Err(V8Error::Limit("string"))
    );
    assert_eq!(
        encode(&Value::String(vec![0x78; 100]), 64),
        Err(V8Error::Limit("value size"))
    );
}

#[test]
fn values_built_for_writing_match_deno() {
    let name = |text: &str| Key::Name(units(text));
    let record = Value::Object(Rc::new(Object::Plain(vec![
        (name("a"), Value::Number(1.0)),
        (name("b"), Value::String(units("two"))),
        (name("c"), Value::Boolean(true)),
        (name("d"), Value::Null),
    ])));
    assert_eq!(
        encode(&record, usize::MAX).unwrap(),
        hex("ff106f2201614902220162220374776f22016354220164307b04")
    );
    let bytes = Value::Object(Rc::new(Object::Plain(vec![(
        name("data"),
        Value::Object(Rc::new(Object::Bytes {
            buffer: vec![9, 8, 7],
            offset: 0,
            length: 3,
        })),
    )])));
    assert_eq!(
        encode(&bytes, usize::MAX).unwrap(),
        hex("ff106f220464617461420309080756420003007b01")
    );
    let holes = Value::Object(Rc::new(Object::Sparse {
        length: 3,
        elements: vec![(0, Value::Number(1.0)), (2, Value::Number(3.0))],
    }));
    assert_eq!(
        encode(&holes, usize::MAX).unwrap(),
        hex("ff1061034900490249044906400203")
    );
    for (value, wire) in [
        (BigInt::from(-5), "ff105a110500000000000000"),
        (BigInt::from(0), "ff105a00"),
        (
            BigInt::from(1u128 << 64),
            "ff105a2000000000000000000100000000000000",
        ),
    ] {
        assert_eq!(
            encode(&Value::BigInt(value), usize::MAX).unwrap(),
            hex(wire)
        );
    }
}
