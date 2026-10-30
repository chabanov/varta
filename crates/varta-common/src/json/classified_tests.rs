use super::*;
use crate::{abi, value::*};
fn number() -> JsonSchema {
    JsonSchema {
        version: ERROR_FAMILY_SCHEMA_VERSION,
        root: 0,
        types: vec![JsonType::Float64],
    }
}
#[test]
fn classified_json_separates_complete_grammar_type_mismatches_and_finite_domain() {
    let schema = number();
    for input in [
        "{",
        "[1,]",
        "\"x\" trailing",
        "NaN",
        "Infinity",
        "01",
        "1e",
        "{\"v\":]}",
        "true false",
    ] {
        assert!(
            matches!(
                schema.parse_json_classified(input.as_bytes()),
                Err(JsonDecodeError::Syntax { .. })
            ),
            "{input}"
        );
    }
    for input in ["null", "true", "\"x\"", "{}", "[]", "1e400"] {
        assert!(
            matches!(
                schema.parse_json_classified(input.as_bytes()),
                Err(JsonDecodeError::Type)
            ),
            "{input}"
        );
    }
    assert_eq!(
        schema.parse_json_classified(b"-0").unwrap(),
        JsonValue::Float64((-0f64).to_bits())
    );
}
#[test]
fn classified_json_enforces_whole_input_limits_before_schema_mismatch() {
    let schema = number();
    let row = format!("[{}]", vec!["0"; MAX_ARRAY_ELEMENTS].join(","));
    for (input, limit) in [
        (" ".repeat(abi::MAX_INPUT_BYTES + 1), JsonInputLimit::Bytes),
        (
            format!("[{}]", vec!["0"; MAX_ARRAY_ELEMENTS + 1].join(",")),
            JsonInputLimit::ArrayElements,
        ),
        (format!("[{row},{row},{row},{row}]"), JsonInputLimit::Nodes),
        (
            format!(
                "{}0{}",
                "[".repeat(MAX_JSON_DEPTH + 1),
                "]".repeat(MAX_JSON_DEPTH + 1)
            ),
            JsonInputLimit::Depth,
        ),
    ] {
        assert!(
            matches!(schema.parse_json_classified(input.as_bytes()),Err(JsonDecodeError::Limit(actual)) if actual==limit),
            "{limit}: {input}"
        );
    }
}
#[test]
fn classified_json_preserves_utf16_and_rejects_duplicate_unknown_missing_and_unsupported_keys_as_types()
 {
    let schema = JsonSchema {
        version: ERROR_FAMILY_SCHEMA_VERSION,
        root: 1,
        types: vec![
            JsonType::String,
            JsonType::Record {
                descriptor: 0,
                fields: vec![JsonField {
                    name: "v".into(),
                    ty: 0,
                    optional: false,
                }],
            },
        ],
    };
    for input in [
        r#"{}"#,
        r#"{"v":1}"#,
        r#"{"v":"a","v":"b"}"#,
        r#"{"v":"a","extra":0}"#,
        r#"{"\ud800":"a"}"#,
    ] {
        assert!(
            matches!(
                schema.parse_json_classified(input.as_bytes()),
                Err(JsonDecodeError::Type)
            ),
            "{input}"
        );
    }
    assert_eq!(
        schema
            .parse_json_classified(r#"{"v":"\ud800😀"}"#.as_bytes())
            .unwrap(),
        JsonValue::Record {
            ty: 1,
            fields: vec![JsonValue::String16(vec![0xd800, 0xd83d, 0xde00])]
        }
    );
    let mut invalid = schema;
    invalid.version = u32::MAX;
    assert!(matches!(
        invalid.parse_json_classified(b"{"),
        Err(JsonDecodeError::Schema(_))
    ));
}
#[test]
fn classified_json_counts_synthetic_optional_values_as_fatal_nodes() {
    let schema = JsonSchema {
        version: ERROR_FAMILY_SCHEMA_VERSION,
        root: 4,
        types: vec![
            JsonType::Undefined,
            JsonType::Float64,
            JsonType::PrimitiveUnion {
                members: vec![0, 1],
            },
            JsonType::Record {
                descriptor: 0,
                fields: (0..MAX_RECORD_FIELDS)
                    .map(|i| JsonField {
                        name: format!("f{i:02}"),
                        ty: 2,
                        optional: true,
                    })
                    .collect(),
            },
            JsonType::Array { element: 3 },
        ],
    };
    let input = format!("[{}]", vec!["{}"; 128].join(","));
    assert!(matches!(
        schema.parse_json_classified(input.as_bytes()),
        Err(JsonDecodeError::Limit(JsonInputLimit::Nodes))
    ));
    assert!(
        schema
            .parse_json(input.as_bytes())
            .unwrap_err()
            .to_string()
            .contains("node limit")
    );
}

#[test]
fn v38_limits_lift_the_input_ceilings_and_bound_host_work_by_the_deadline() {
    use crate::limits::DataLimits;
    let numbers = JsonSchema {
        version: FUNCTION_FIELD_SCHEMA_VERSION,
        root: 1,
        types: vec![JsonType::Float64, JsonType::Array { element: 0 }],
    };
    let wide = format!("[{}0]", "1,".repeat(MAX_ARRAY_ELEMENTS * 40));
    assert!(wide.len() > abi::MAX_INPUT_BYTES);
    // Earlier profiles keep their input byte ceiling.
    assert!(matches!(
        numbers.parse_json_classified(wide.as_bytes()),
        Err(JsonDecodeError::Limit(JsonInputLimit::Bytes))
    ));
    let JsonValue::Array { values, .. } = numbers
        .parse_json_classified_limited(wide.as_bytes(), DataLimits::V38)
        .unwrap()
    else {
        panic!("expected an array");
    };
    assert_eq!(values.len(), MAX_ARRAY_ELEMENTS * 40 + 1);
    // A passed deadline stops the host work at its next check.
    let past = std::time::Instant::now();
    assert!(matches!(
        numbers.parse_json_classified_until(wide.as_bytes(), DataLimits::V38, past),
        Err(JsonDecodeError::Limit(JsonInputLimit::Deadline))
    ));
    assert!(
        numbers
            .parse_json_until(wide.as_bytes(), DataLimits::V38, past)
            .is_err()
    );
}
