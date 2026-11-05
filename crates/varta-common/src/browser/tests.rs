use super::*;
use std::collections::BTreeSet;

#[test]
fn checked_in_browser_module_matches_registries() {
    assert_eq!(
        javascript().unwrap(),
        include_str!("../../browser/registry.mjs")
    );
}

#[test]
fn inventory_round_trips_every_operation_without_duplicates() {
    let mut names = BTreeSet::new();
    for &op in HostOperation::ALL {
        assert!(names.insert(op.name()));
        assert_eq!(HostOperation::from_name(op.name()), Some(op));
    }
    assert!(!names.is_empty());
    assert_eq!(
        HostOperation::from_name("__unregistered_browser_effect"),
        None
    );
    let registry = registry();
    let operations = registry["operations"].as_array().unwrap();
    assert_eq!(operations.len(), names.len());
    assert!(
        operations
            .windows(2)
            .all(|ops| ops[0]["name"].as_str() < ops[1]["name"].as_str())
    );
    for operation in operations {
        let op = HostOperation::from_name(operation["name"].as_str().unwrap()).unwrap();
        let (params, results) = op.scalar_signature();
        assert_eq!(
            operation["params"],
            json!(params.iter().map(scalar).collect::<Vec<_>>())
        );
        assert_eq!(
            operation["results"],
            json!(results.iter().map(scalar).collect::<Vec<_>>())
        );
        assert_eq!(operation["capability"], json!(op.capability()));
    }
}

#[test]
fn every_profile_admits_exactly_its_registered_wire_variants() {
    let data = registry();
    for &p in language::SUPPORTED_PROFILES {
        let profile = data["profiles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == json!(p))
            .unwrap();
        let mut count = 0;
        for &op in HostOperation::ALL {
            if admits(p, op) {
                count += 1;
                assert!(op.minimum_abi() <= p.host_abi());
                assert_eq!(p.host_operation(op), op);
                assert!(!p.links(op));
            }
        }
        assert_eq!(profile["admittedImports"], json!(count));
        assert_eq!(profile["fairFuel"], json!(p.fair_fuel()));
        assert_eq!(profile["toolEntries"], json!(!p.binary64()));
        assert_eq!(profile["httpVersion"], json!(http::version_for(p)));
    }
    let p = LanguageProfile::CheckedF64Utf16V38;
    assert!(admits(p, HostOperation::HttpService));
    assert!(!admits(
        LanguageProfile::CheckedF64Utf16V37,
        HostOperation::HttpService
    ));
    assert!(!admits(p, HostOperation::JsonParse));
    assert!(!admits(p, p.json_parse())); // v38 links this helper instead.
}

#[test]
fn fuel_rates_keep_exact_integer_values_and_generation_is_repeatable() {
    assert_eq!(registry()["fuel"]["call"], json!(fuel::CALL));
    assert_eq!(registry()["fuel"]["kvOperation"], json!(fuel::KV_OPERATION));
    assert_eq!(javascript().unwrap(), javascript().unwrap());
}

#[test]
fn scheduled_profile_layouts_and_limits_match_the_native_readers() {
    let data = registry();
    for &profile in language::SUPPORTED_PROFILES
        .iter()
        .filter(|profile| profile.promises())
    {
        for count in 0..=value::max_record_fields(profile.value_abi()) {
            assert_eq!(
                data["values"]["recordShapes"][count]["bytes"],
                json!(value::record_bytes(profile.value_abi(), count))
            );
        }
        let row = data["profiles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == json!(profile))
            .unwrap();
        assert_eq!(
            row["schedulerLimits"]["held"],
            json!(profile.limits().data.pending_continuations)
        );
    }
    for tag in 0..64 {
        if let Some(kind) = value::ValueKind::from_tag(tag) {
            assert_eq!(data["values"]["tags"][format!("{kind:?}")], json!(tag));
        }
    }
}

#[test]
fn pilot_effect_facts_name_registered_operations_capabilities_and_collector_constants() {
    use crate::{budget, collector as c};
    let data = registry();
    let effects = &data["effects"];
    for (key, operation) in [
        ("clock", HostOperation::DateNow),
        ("random", HostOperation::MathRandom),
        ("environment", HostOperation::EnvSnapshot),
        ("exit", HostOperation::ExitFloat),
    ] {
        assert_eq!(effects[key]["operation"], json!(operation.name()));
        if key != "random" {
            assert_eq!(effects[key]["capability"], json!(operation.capability()));
        }
        assert!(admits(LanguageProfile::CheckedF64Utf16V38, operation));
    }
    // Entropy is ungated (accepted decision 9); the clock and the exit are
    // boolean capabilities the generated ceiling validation accepts.
    assert_eq!(HostOperation::MathRandom.capability(), None);
    let booleans = data["booleanCapabilities"].as_array().unwrap();
    assert!(booleans.contains(&effects["clock"]["capability"]));
    assert!(booleans.contains(&effects["exit"]["capability"]));
    assert_eq!(effects["random"]["sampleBits"], json!(f64::MANTISSA_DIGITS));
    assert_eq!(effects["exit"]["minimum"], json!(i32::MIN));
    let collector = &effects["collector"];
    assert_eq!(collector["idleExport"], json!(c::IDLE_EXPORT));
    assert_eq!(collector["stepUnits"], json!(c::IDLE_STEP_UNITS));
    assert_eq!(
        [&collector["none"], &collector["more"], &collector["done"]],
        [
            &json!(c::IDLE_NONE),
            &json!(c::IDLE_MORE),
            &json!(c::IDLE_DONE)
        ]
    );
    assert_eq!(collector["stepFuel"], json!(budget::MAINTENANCE_STEP_FUEL));
    // A maintenance allowance always fits the browser tank's turn ceiling.
    assert_eq!(data["maxTurnFuel"], json!(budget::MAX_FUEL_PER_TURN));
    const { assert!(budget::MAX_MAINTENANCE_FUEL <= budget::MAX_FUEL_PER_TURN) };
    for template in ["fuelExhausted", "deadlineExhausted"] {
        let text = collector[template].as_str().unwrap();
        assert!(text.contains(collector["turnPlaceholder"].as_str().unwrap()));
        assert!(text.contains(collector["limitPlaceholder"].as_str().unwrap()));
    }
}
