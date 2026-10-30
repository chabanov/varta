use super::*;
use crate::{abi, value};
fn fixture() -> (Vec<u8>, u64, usize, usize, FunctionCatalog) {
    let (data, root, env, binding) = super::view_tests::graph(1);
    let catalog = FunctionCatalog {
        version: CATALOG_VERSION,
        value_version: value::BIGINT_VALUE_VERSION,
        types: vec![CaptureType::String],
        bindings: (0..20)
            .map(|_| CaptureBinding {
                ty: 0,
                result: true,
            })
            .collect(),
        functions: vec![CaptureFunction {
            code: 17,
            signature: 7,
            captures: vec![19],
        }],
        frames: vec![],
    };
    (data, root, env, binding, catalog)
}
fn put(data: &mut [u8], at: usize, n: u32) {
    data[at..at + 4].copy_from_slice(&n.to_le_bytes());
}
fn put64(data: &mut [u8], at: usize, n: u64) {
    data[at..at + 8].copy_from_slice(&n.to_le_bytes());
}
#[test]
fn capture_catalog_roundtrip_checks_typed_result_captures_and_work_bounds() {
    let (data, root, _, _, catalog) = fixture();
    let bytes = catalog.to_bytes().unwrap();
    let catalog = FunctionCatalog::from_bytes(&bytes).unwrap();
    let proof = catalog.validate_closure(&data, root, abi::FUEL).unwrap();
    assert!(proof.nodes >= 2 && proof.work > 0 && proof.work <= abi::FUEL);
    assert!(catalog.validate_closure(&data, root, 0).is_err());
    assert!(catalog.validate_closure(&data, root, 1).is_err());
}
#[test]
fn typed_capture_validation_rejects_kind_masks_width_ids_status_and_payload_forgery() {
    let (data, root, _, binding, catalog) = fixture();
    for (offset, n) in [(binding, 0), (binding - 8, 0), (binding + 4, 1)] {
        let mut bad = data.clone();
        put(&mut bad, offset, n);
        assert!(catalog.validate_closure(&bad, root, abi::FUEL).is_err());
    }
    for (offset, n) in [
        (binding + 8, 1u64 << 32),
        (binding + 8, (2u64 << 32) | u32::MAX as u64),
        (binding + 16, 5),
        (binding + 16, 1u64 << 32),
    ] {
        let mut bad = data.clone();
        put64(&mut bad, offset, n);
        assert!(catalog.validate_closure(&bad, root, abi::FUEL).is_err());
    }
    let mut boolean = catalog.clone();
    boolean.types[0] = CaptureType::Boolean;
    let mut bad = data;
    put(&mut bad, binding - 8, 0);
    put64(&mut bad, binding + 8, 2);
    // A structurally valid scalar cell with a noncanonical Boolean is still invalid.
    assert!(BindingView::read_envelope(&bad, ((24u64) << 32) | binding as u64).is_ok());
    assert!(boolean.validate_closure(&bad, root, abi::FUEL).is_err());
}
#[test]
fn typed_capture_validation_checks_catalog_code_and_signature_before_alias_memoization() {
    let (data, root, _, _, catalog) = fixture();
    let ptr = root as u32 as usize;
    for (offset, n) in [(ptr, 8), (ptr + 8, 18)] {
        let mut bad = data.clone();
        put(&mut bad, offset, n);
        assert!(ClosureView::read_envelope(&bad, root).is_ok());
        assert!(catalog.validate_closure(&bad, root, abi::FUEL).is_err());
    }
}
#[test]
fn typed_function_capture_cycles_are_bounded_and_retain_expected_signatures() {
    let (mut data, root, env, binding, mut catalog) = fixture();
    put(&mut data, binding - 16, 32);
    put(&mut data, binding + 4, 1);
    put64(&mut data, env + 8, (16u64 << 32) | binding as u64);
    put64(&mut data, binding + 8, root);
    catalog.types[0] = CaptureType::Function { signature: 7 };
    for row in &mut catalog.bindings {
        row.result = false;
    }
    let proof = catalog.validate_closure(&data, root, abi::FUEL).unwrap();
    assert!(proof.nodes < 10);
    catalog.types[0] = CaptureType::Function { signature: 8 };
    assert!(catalog.validate_closure(&data, root, abi::FUEL).is_err());
}
#[test]
fn capture_catalog_rejects_invalid_metadata_and_deep_bottom_up_type_dags() {
    let (_, _, _, _, catalog) = fixture();
    let mut bad = catalog.clone();
    bad.functions.push(bad.functions[0].clone());
    assert!(bad.validate().is_err());
    let mut bad = catalog.clone();
    bad.bindings[19].ty = u32::MAX;
    assert!(bad.validate().is_err());
    let mut bad = catalog.clone();
    bad.functions[0].captures = vec![0; MAX_CAPTURES + 1];
    assert!(bad.validate().is_err());
    let mut bad = catalog.clone();
    bad.value_version = value::FUNCTION_FIELD_VALUE_VERSION + 1;
    assert!(bad.validate().is_err());
    let mut bad = catalog.clone();
    bad.types = vec![CaptureType::Array { element: 0 }];
    for binding in &mut bad.bindings {
        binding.result = false;
    }
    assert!(bad.validate().is_err());
    let mut chain = FunctionCatalog {
        version: CATALOG_VERSION,
        value_version: value::BIGINT_VALUE_VERSION,
        types: vec![CaptureType::Float64],
        bindings: vec![],
        functions: vec![],
        frames: vec![],
    };
    for _ in 0..value::MAX_JSON_DEPTH {
        chain.types.push(CaptureType::Array {
            element: chain.types.len() as u32 - 1,
        });
    }
    chain.validate().unwrap();
    chain.types.push(CaptureType::Array {
        element: chain.types.len() as u32 - 1,
    });
    assert!(chain.validate().is_err());
    let mut json = serde_json::to_value(catalog).unwrap();
    json["unknown"] = serde_json::json!(true);
    assert!(FunctionCatalog::from_bytes(&serde_json::to_vec(&json).unwrap()).is_err());
    assert!(FunctionCatalog::from_bytes(b"{}").is_err());
}
#[test]
fn capture_catalog_class_leaf_is_record_tagged_and_shares_the_union_record_slot() {
    let catalog = |types| FunctionCatalog {
        version: CATALOG_VERSION,
        value_version: value::BIGINT_VALUE_VERSION,
        types,
        bindings: vec![CaptureBinding {
            ty: 0,
            result: false,
        }],
        functions: vec![],
        frames: vec![],
    };
    let class = catalog(vec![CaptureType::Class]);
    let bytes = class.to_bytes().unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("\"kind\":\"class\""));
    let class = FunctionCatalog::from_bytes(&bytes).unwrap();
    assert_eq!(class.types[0].value_kind(), Some(value::ValueKind::Record));
    assert!(class.types[0].reference());
    catalog(vec![
        CaptureType::Undefined,
        CaptureType::Class,
        CaptureType::ReferenceUnion {
            members: vec![0, 1],
        },
    ])
    .validate()
    .unwrap();
    for members in [
        CaptureType::PrimitiveUnion {
            members: vec![0, 1],
        },
        CaptureType::ReferenceUnion {
            members: vec![1, 2],
        },
    ] {
        let bad = catalog(vec![
            CaptureType::Undefined,
            CaptureType::Class,
            CaptureType::Record { fields: vec![] },
            members,
        ]);
        assert!(bad.validate().is_err());
    }
}

#[test]
fn verified_capture_catalog_keeps_metadata_and_graph_work_checks() {
    let (data, root, _, binding, mut catalog) = fixture();
    let raw = catalog.validate_closure(&data, root, abi::FUEL).unwrap();
    let verified = catalog.clone().into_verified().unwrap();
    let decoded = VerifiedFunctionCatalog::from_bytes(&catalog.to_bytes().unwrap()).unwrap();
    // Mutating the former owner cannot alter either sealed catalog.
    catalog.functions.clear();
    assert!(catalog.validate_closure(&data, root, abi::FUEL).is_err());
    for owner in [&verified, &decoded] {
        let proof = owner.validate_closure(&data, root, abi::FUEL).unwrap();
        assert_eq!((proof.nodes, proof.work), (raw.nodes, raw.work));
        assert!(owner.validate_closure(&data, root, raw.work - 1).is_err());
        let mut corrupt = data.clone();
        put64(&mut corrupt, binding + 16, 5);
        assert!(owner.validate_closure(&corrupt, root, abi::FUEL).is_err());
    }
    catalog.value_version = 0;
    assert!(catalog.into_verified().is_err());
}
