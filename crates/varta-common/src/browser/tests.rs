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
