use super::*;
use crate::{abi, value::*};

#[test]
fn binary64_schema_preserves_rounding_and_zero_bits_without_weakening_legacy_schemas() {
    let schema = JsonSchema {
        version: NUMBER_SCHEMA_VERSION,
        root: 0,
        types: vec![JsonType::Float64],
    };
    for text in [
        "-0",
        "1.25",
        "1e-7",
        "9007199254740993",
        "18446744073709551616",
        "5e-324",
        "1.0000000000000002",
        "0.84551240822557006",
    ] {
        assert_eq!(
            schema.parse_json(text.as_bytes()).unwrap(),
            JsonValue::Float64(text.parse::<f64>().unwrap().to_bits())
        );
    }
    for text in ["1e400", "NaN", "Infinity", "true", "null", "[]"] {
        assert!(schema.parse_json(text.as_bytes()).is_err());
    }
    let mut bad = schema.clone();
    bad.version = SCHEMA_VERSION;
    assert!(bad.to_bytes().is_err());
    let mut bad = schema.clone();
    bad.types = vec![JsonType::I32];
    assert!(bad.to_bytes().is_err());
    let legacy = JsonSchema {
        version: SCHEMA_VERSION,
        root: 0,
        types: vec![JsonType::I32],
    };
    for text in ["1.0", "1e0", "-0", "2147483648"] {
        assert!(legacy.parse_json(text.as_bytes()).is_err(), "{text}");
    }
}

fn schema() -> JsonSchema {
    JsonSchema {
        version: SCHEMA_VERSION,
        root: 5,
        types: vec![
            JsonType::I32,
            JsonType::String,
            JsonType::Boolean,
            JsonType::Null,
            JsonType::Array { element: 1 },
            JsonType::Record {
                descriptor: 0,
                fields: vec![
                    JsonField {
                        optional: false,
                        name: "count".into(),
                        ty: 0,
                    },
                    JsonField {
                        optional: false,
                        name: "empty".into(),
                        ty: 3,
                    },
                    JsonField {
                        optional: false,
                        name: "enabled".into(),
                        ty: 2,
                    },
                    JsonField {
                        optional: false,
                        name: "labels".into(),
                        ty: 4,
                    },
                ],
            },
        ],
    }
}
#[test]
fn checked_decoding_reorders_closed_fields_without_coercion_and_preserves_unicode() {
    let schema = JsonSchema::from_bytes(&schema().to_bytes().unwrap()).unwrap();
    let result = schema
        .parse_json(
            r#"{"labels":["Привіт🦀\u0000",""],"enabled":false,"empty":null,"count":-2147483648}"#
                .as_bytes(),
        )
        .unwrap();
    assert_eq!(
        result,
        JsonValue::Record {
            ty: 5,
            fields: vec![
                JsonValue::I32(i32::MIN),
                JsonValue::Null,
                JsonValue::Boolean(false),
                JsonValue::Array {
                    ty: 4,
                    values: vec![
                        JsonValue::String("Привіт🦀\0".into()),
                        JsonValue::String("".into())
                    ]
                },
            ]
        }
    );
}
#[test]
fn duplicate_unknown_missing_wrong_typed_and_invalid_json_are_rejected() {
    for json in [
        "null",
        "[]",
        "{}",
        "{",
        "{}{}",
        "{\"count\":1,\"extra\":0}",
        r#"{"count":1,"count":2}"#,
        r#"{"count":1,"coun\u0074":2}"#,
        r#"{"count":1.0}"#,
        r#"{"count":1e0}"#,
        r#"{"count":-0}"#,
        r#"{"count":2147483648}"#,
        r#"{"count":-2147483649}"#,
        r#"{"count":1e999}"#,
        r#"{"count":"1"}"#,
        r#"{"count":null}"#,
        r#"{"count":true}"#,
        r#"{"count":1,"empty":null,"enabled":0,"labels":[]}"#,
        r#"{"count":1,"empty":null,"enabled":true,"labels":[1]}"#,
        r#"{"count":1,"empty":null,"enabled":true,"labels":["\uD800"]}"#,
    ] {
        assert!(schema().parse_json(json.as_bytes()).is_err(), "{json}");
    }
    assert!(schema().parse_json(b"{\"count\":\"\xff\"}").is_err());
    for token in [
        "1.0",
        "1e0",
        "-0",
        "2147483648",
        "-2147483649",
        "1e999",
        "\"1\"",
        "null",
        "true",
    ] {
        let input = format!("{{\"count\":{token},\"empty\":null,\"enabled\":true,\"labels\":[]}}");
        assert!(schema().parse_json(input.as_bytes()).is_err(), "{token}");
    }
    for input in [
        r#"{"count":1,"empty":null,"enabled":true,"labels":[],"extra":0}"#,
        r#"{"count":1,"empty":null,"enabled":true,"labels":[],"count":2}"#,
        r#"{"count":1,"empty":null,"enabled":true,"labels":[],"coun\u0074":2}"#,
    ] {
        assert!(schema().parse_json(input.as_bytes()).is_err(), "{input}");
    }
    let padded = format!(
        "{}{{\"count\":1,\"empty\":null,\"enabled\":true,\"labels\":[]}}",
        " ".repeat(abi::MAX_INPUT_BYTES)
    );
    assert!(
        schema()
            .parse_json(padded.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("byte limit")
    );
    let string = JsonSchema {
        version: SCHEMA_VERSION,
        root: 0,
        types: vec![JsonType::String],
    };
    assert!(string.parse_json(b"\"\xff\"").is_err());
}
#[test]
fn schema_versions_references_fields_and_metadata_bytes_are_bounded() {
    for bytes in [
        b"{}".as_slice(),
        b"{\"version\":1,\"version\":1}",
        &vec![b' '; MAX_SCHEMA_BYTES + 1],
    ] {
        assert!(JsonSchema::from_bytes(bytes).is_err());
    }
    let mut padded = schema().to_bytes().unwrap();
    padded.resize(MAX_SCHEMA_BYTES + 1, b' ');
    assert!(
        JsonSchema::from_bytes(&padded)
            .unwrap_err()
            .to_string()
            .contains("byte limit")
    );
    let mut cases = Vec::new();
    let mut bad = schema();
    bad.version += 1;
    cases.push(bad);
    let mut bad = schema();
    bad.root = u32::MAX;
    cases.push(bad);
    let mut bad = schema();
    bad.types[0] = JsonType::Array { element: 0 };
    cases.push(bad);
    let mut bad = schema();
    bad.types[4] = JsonType::Array { element: u32::MAX };
    cases.push(bad);
    let mut bad = schema();
    bad.types = vec![JsonType::Null; MAX_SCHEMA_TYPES + 1];
    cases.push(bad);
    for key in ["__proto__".to_owned(), "x".repeat(MAX_RECORD_KEY_BYTES + 1)] {
        let mut bad = schema();
        bad.types[5] = JsonType::Record {
            descriptor: 0,
            fields: vec![JsonField {
                optional: false,
                name: key,
                ty: 0,
            }],
        };
        cases.push(bad);
    }
    let mut bad = schema();
    bad.types[5] = JsonType::Record {
        descriptor: 0,
        fields: vec![
            JsonField {
                optional: false,
                name: "x".into(),
                ty: 0
            };
            MAX_RECORD_FIELDS + 1
        ],
    };
    cases.push(bad);
    let mut bad = schema();
    bad.types[5] = JsonType::Record {
        descriptor: 0,
        fields: vec![
            JsonField {
                optional: false,
                name: "b".into(),
                ty: 0,
            },
            JsonField {
                optional: false,
                name: "a".into(),
                ty: 0,
            },
        ],
    };
    cases.push(bad);
    for bad in cases {
        assert!(bad.validate().is_err(), "{bad:?}");
    }
}
#[test]
fn input_array_node_and_schema_depth_limits_are_independent() {
    let arrays = JsonSchema {
        version: SCHEMA_VERSION,
        root: 2,
        types: vec![
            JsonType::I32,
            JsonType::Array { element: 0 },
            JsonType::Array { element: 1 },
        ],
    };
    let values = vec!["1"; MAX_ARRAY_ELEMENTS].join(",");
    assert!(
        arrays
            .parse_json(format!("[[{values}]]").as_bytes())
            .is_ok()
    );
    assert!(
        arrays
            .parse_json(format!("[[{values},1]]").as_bytes())
            .is_err()
    );
    assert!(
        arrays
            .parse_json(format!("[[{values}],[{values}],[{values}],[{values}]]").as_bytes())
            .unwrap_err()
            .to_string()
            .contains("node limit")
    );
    let mut types = vec![JsonType::I32];
    for _ in 0..MAX_JSON_DEPTH {
        types.push(JsonType::Array {
            element: types.len() as u32 - 1,
        });
    }
    let deep = JsonSchema {
        version: SCHEMA_VERSION,
        root: types.len() as u32 - 1,
        types,
    };
    deep.validate().unwrap();
    assert!(
        deep.parse_json(
            format!(
                "{}1{}",
                "[".repeat(MAX_JSON_DEPTH),
                "]".repeat(MAX_JSON_DEPTH)
            )
            .as_bytes()
        )
        .is_ok()
    );
    let mut too_deep = deep;
    too_deep.types.push(JsonType::Array {
        element: too_deep.root,
    });
    too_deep.root += 1;
    assert!(too_deep.validate().is_err());
}

fn checked_union_schema() -> JsonSchema {
    JsonSchema {
        version: CHECKED_SCHEMA_VERSION,
        root: 5,
        types: vec![
            JsonType::Float64,
            JsonType::Undefined,
            JsonType::String,
            JsonType::PrimitiveUnion {
                members: vec![0, 1, 2],
            },
            JsonType::Array { element: 3 },
            JsonType::Record {
                descriptor: 0,
                fields: vec![
                    JsonField {
                        name: "a".into(),
                        ty: 3,
                        optional: true,
                    },
                    JsonField {
                        name: "b".into(),
                        ty: 4,
                        optional: false,
                    },
                ],
            },
        ],
    }
}
#[test]
fn checked_union_schema_decodes_missing_members_and_lone_units_without_coercion() {
    let schema = checked_union_schema();
    let parsed = schema.parse_json(br#"{"b":[1,"\uD800"]}"#).unwrap();
    assert_eq!(
        parsed,
        JsonValue::Record {
            ty: 5,
            fields: vec![
                JsonValue::Union {
                    ty: 3,
                    value: Box::new(JsonValue::Undefined)
                },
                JsonValue::Array {
                    ty: 4,
                    values: vec![
                        JsonValue::Union {
                            ty: 3,
                            value: Box::new(JsonValue::Float64(1f64.to_bits()))
                        },
                        JsonValue::Union {
                            ty: 3,
                            value: Box::new(JsonValue::String16(vec![0xd800]))
                        },
                    ]
                },
            ]
        }
    );
    for input in [
        br#"{"b":[true]}"#.as_slice(),
        br#"{"a":null,"b":[]}"#,
        br#"{"a":1,"\u0061":2,"b":[]}"#,
        br#"{"b":[],"b":[]}"#,
        br#"{"b":[1e400]}"#,
        br#"{"b":[],"x":0}"#,
        br#"{}"#,
        br#"{"b":[""],"a":""}{}"#,
        b"{\"b\":[\"\xff\"]}",
    ] {
        assert!(schema.parse_json(input).is_err(), "{input:?}");
    }
}
#[test]
fn checked_union_schema_rejects_ambiguous_recursive_and_unversioned_metadata() {
    for members in [
        vec![],
        vec![0],
        vec![0, 0],
        vec![0, 3],
        vec![0, 99],
        vec![1, 2, 2],
    ] {
        let mut schema = checked_union_schema();
        schema.types[3] = JsonType::PrimitiveUnion { members };
        assert!(schema.validate().is_err());
    }
    for version in SCHEMA_VERSION..CHECKED_SCHEMA_VERSION {
        let mut schema = checked_union_schema();
        schema.version = version;
        assert!(schema.validate().is_err());
    }
    let mut schema = checked_union_schema();
    schema.types[3] = JsonType::ReferenceUnion {
        members: vec![0, 1, 2],
    };
    assert!(schema.validate().is_err());
    let mut schema = checked_union_schema();
    if let JsonType::Record { fields, .. } = &mut schema.types[5] {
        fields[0].ty = 0;
    }
    assert!(schema.validate().is_err());
    let mut schema = checked_union_schema();
    if let JsonType::Record { fields, .. } = &mut schema.types[5] {
        fields[0].ty = 99;
    }
    assert!(schema.validate().is_err());
    let mut schema = checked_union_schema();
    schema.types.push(JsonType::Record {
        descriptor: 0,
        fields: vec![],
    });
    schema.types.push(JsonType::Record {
        descriptor: 0,
        fields: vec![],
    });
    schema.types.push(JsonType::ReferenceUnion {
        members: vec![6, 7],
    });
    assert!(schema.validate().is_err());
}
#[test]
fn checked_union_schema_retains_array_node_depth_and_input_budgets() {
    let schema = checked_union_schema();
    let values = vec!["1"; MAX_ARRAY_ELEMENTS].join(",");
    assert!(
        schema
            .parse_json(format!("{{\"b\":[{values}]}}").as_bytes())
            .is_ok()
    );
    assert!(
        schema
            .parse_json(format!("{{\"b\":[{values},1]}}").as_bytes())
            .is_err()
    );
    let mut schema = checked_union_schema();
    schema.types.push(JsonType::Array { element: 5 });
    schema.root = 6;
    let row = format!("{{\"b\":[{values}]}}");
    assert!(
        schema
            .parse_json(format!("[{row},{row},{row},{row}]").as_bytes())
            .unwrap_err()
            .to_string()
            .contains("node limit")
    );
    let mut padded = b" ".repeat(abi::MAX_INPUT_BYTES + 1);
    padded.extend(b"[]");
    assert!(schema.parse_json(&padded).is_err());
    let mut schema = checked_union_schema();
    for _ in 0..MAX_JSON_DEPTH {
        schema.types.push(JsonType::Array {
            element: schema.root,
        });
        schema.root += 1;
    }
    assert!(schema.validate().is_err());
}
#[test]
fn bigint_schema_cannot_create_integers_from_json_tokens_or_reinterpret_old_schemas() {
    let schema = JsonSchema {
        version: BIGINT_SCHEMA_VERSION,
        root: 0,
        types: vec![JsonType::BigInt],
    };
    schema.validate().unwrap();
    assert_eq!(schema.value_version(), BIGINT_VALUE_VERSION);
    for input in ["1", "1.5", "\"1\"", "true", "null", "[]", "{}"] {
        assert!(
            matches!(
                schema.parse_json_classified(input.as_bytes()),
                Err(JsonDecodeError::Type)
            ),
            "{input}"
        );
    }
    assert!(matches!(
        schema.parse_json_classified(b"1n"),
        Err(JsonDecodeError::Syntax { .. })
    ));
    for version in SCHEMA_VERSION..=SPARSE_SCHEMA_VERSION {
        assert!(
            JsonSchema {
                version,
                ..schema.clone()
            }
            .validate()
            .is_err()
        );
    }
    let union = JsonSchema {
        version: BIGINT_SCHEMA_VERSION,
        root: 2,
        types: vec![
            JsonType::BigInt,
            JsonType::Float64,
            JsonType::ReferenceUnion {
                members: vec![0, 1],
            },
        ],
    };
    union.validate().unwrap();
    assert_eq!(
        union.parse_json_classified(b"12").unwrap(),
        JsonValue::Union {
            ty: 2,
            value: Box::new(JsonValue::Float64(12.0f64.to_bits()))
        }
    );
    let wrong_cell = JsonSchema {
        types: vec![
            JsonType::BigInt,
            JsonType::Float64,
            JsonType::PrimitiveUnion {
                members: vec![0, 1],
            },
        ],
        ..union
    };
    assert!(wrong_cell.validate().is_err());
}
#[test]
fn function_schema14_maps_existing_json_types_without_inventing_callable_values() {
    let schema = JsonSchema {
        version: FUNCTION_SCHEMA_VERSION,
        root: 0,
        types: vec![JsonType::Float64],
    };
    schema.validate().unwrap();
    assert_eq!(schema.value_version(), FUNCTION_VALUE_VERSION);
    assert!(schema.parse_json_classified(b"7").is_ok());
    for text in ["function(){}", "()=>1"] {
        assert!(matches!(
            schema.parse_json_classified(text.as_bytes()),
            Err(JsonDecodeError::Syntax { .. })
        ));
    }
    assert!(
        JsonSchema::from_bytes(
            br#"{"version":14,"root":0,"types":[{"kind":"function","signature":0}]}"#
        )
        .is_err()
    );
    let old = JsonSchema {
        version: BIGINT_SCHEMA_VERSION,
        ..schema
    };
    assert_eq!(old.value_version(), BIGINT_VALUE_VERSION);
}
