//! Data-level v14 transport; source admission and native imports stay version gated.
use varta_common::{abi, function::*, value::*};
fn word(data: &mut [u8], at: usize, n: u32) {
    data[at..at + 4].copy_from_slice(&n.to_le_bytes());
}
fn wide(data: &mut [u8], at: usize, n: u64) {
    data[at..at + 8].copy_from_slice(&n.to_le_bytes());
}
fn handle(ptr: usize, len: usize) -> u64 {
    ((len as u64) << 32) | ptr as u64
}
fn block(data: &mut [u8], ptr: usize, len: usize, flags: u32, mask: u32) {
    word(data, ptr - 16, (len + 16).next_multiple_of(8) as u32);
    word(data, ptr - 12, 1 | flags);
    word(data, ptr - 8, mask);
}
struct Fixture {
    data: Vec<u8>,
    function: u64,
    record: u64,
    array: u64,
    cell: u64,
    catalog: FunctionCatalog,
}
fn fixture(cycle: bool) -> Fixture {
    let f = abi::OWNED_STRING_BASE + 16;
    let r = f + 40;
    let a = r + 40;
    let storage = a + 32;
    let cell = storage + 160;
    let env = cell + 32;
    let binding = env + 32;
    let mut data = vec![0; binding + 32];
    block(
        &mut data,
        f,
        CLOSURE_BYTES,
        HEAP_FLAG as u32,
        if cycle { 2 } else { 0 },
    );
    word(&mut data, f, 7);
    word(&mut data, f + 4, 2);
    word(&mut data, f + 8, 17);
    data[16] = b'f';
    word(&mut data, 32, FUNCTION_VALUE_VERSION);
    word(&mut data, 36, 1);
    for (at, n) in [(40, 16), (44, 1), (48, 0), (52, ValueKind::Function as u32)] {
        word(&mut data, at, n);
    }
    block(
        &mut data,
        r,
        record_bytes(FUNCTION_VALUE_VERSION, 1),
        HEAP_RECORD_FLAG as u32,
        1,
    );
    word(&mut data, r, 32);
    word(&mut data, r + 4, 1);
    wide(&mut data, r + 8, handle(f, 24));
    word(&mut data, r + 16, 1);
    block(&mut data, a, 16, HEAP_VECTOR_FLAG as u32, 1);
    word(&mut data, a, 12);
    word(&mut data, a + 4, 1);
    wide(&mut data, a + 8, handle(storage, 144));
    block(
        &mut data,
        storage,
        144,
        (HEAP_ARRAY_FLAG | HEAP_SPARSE_FLAG) as u32,
        1,
    );
    word(&mut data, storage, 12);
    word(&mut data, storage + 4, 1);
    wide(&mut data, storage + 8, handle(f, 24));
    data[storage + 16] = 1;
    block(&mut data, cell, 16, HEAP_UNION_FLAG as u32, 1);
    word(&mut data, cell, 12);
    word(&mut data, cell + 4, 1);
    wide(&mut data, cell + 8, handle(f, 24));
    let mut catalog = FunctionCatalog {
        version: CATALOG_VERSION,
        value_version: FUNCTION_VALUE_VERSION,
        types: vec![],
        bindings: vec![],
        functions: vec![CaptureFunction {
            code: 17,
            signature: 7,
            captures: vec![],
        }],
        frames: vec![],
    };
    if cycle {
        wide(&mut data, f + 16, handle(env, 16));
        block(&mut data, env, 16, ENVIRONMENT_FLAG as u32, 1);
        word(&mut data, env + 4, 1);
        wide(&mut data, env + 8, handle(binding, 16));
        block(&mut data, binding, 16, BINDING_FLAG as u32, 1);
        word(&mut data, binding + 4, 1);
        wide(&mut data, binding + 8, handle(r, 24));
        catalog.types = vec![
            CaptureType::Function { signature: 7 },
            CaptureType::Record {
                fields: vec![CaptureField {
                    name: "f".into(),
                    ty: 0,
                    optional: false,
                }],
            },
        ];
        catalog.bindings = vec![CaptureBinding {
            ty: 1,
            result: false,
        }];
        catalog.functions[0].captures = vec![0];
    }
    Fixture {
        data,
        function: handle(f, 24),
        record: handle(r, 24),
        array: handle(a, 16),
        cell: handle(cell, 16),
        catalog,
    }
}
fn encode(f: &Fixture, kind: ValueKind, payload: u64) -> Result<Option<Vec<u8>>, wasmtime::Error> {
    let mut work = abi::FUEL;
    crate::encode_value_graph_with_functions(&f.data, kind, payload, &f.catalog, &mut work)
        .map_err(|error| wasmtime::Error::msg(error.to_string()))
}
#[test]
fn function_value14_boxes_require_catalog_and_old_versions_reject_the_tag() {
    let f = fixture(false);
    assert_eq!(
        ReferenceBox::read_with_catalog(&f.data, f.cell, &f.catalog, abi::FUEL)
            .unwrap()
            .kind,
        ValueKind::Function
    );
    assert!(ReferenceBox::read_with_version(&f.data, f.cell, FUNCTION_VALUE_VERSION).is_err());
    for version in REFERENCE_VALUE_VERSION..=BIGINT_VALUE_VERSION {
        assert!(!ValueKind::Function.supported_by(version));
        assert!(ReferenceBox::read_with_version(&f.data, f.cell, version).is_err());
    }
    let mut legacy = f.catalog.clone();
    legacy.value_version = BIGINT_VALUE_VERSION;
    assert!(ReferenceBox::read_with_catalog(&f.data, f.cell, &legacy, abi::FUEL).is_err());
}
#[test]
fn function_value14_record_and_sparse_vector_keep_exact_reference_masks() {
    let f = fixture(false);
    validate_aggregate(&f.data, ValueKind::Record, f.record, FUNCTION_VALUE_VERSION).unwrap();
    let vector = Vector::read_with_version(&f.data, f.array, FUNCTION_VALUE_VERSION).unwrap();
    assert_eq!(vector.kind, ValueKind::Function);
    assert!(vector.is_present(&f.data, 0).unwrap());
    assert!(
        validate_aggregate(&f.data, ValueKind::Record, f.record, BIGINT_VALUE_VERSION).is_err()
    );
    assert!(Vector::read_with_version(&f.data, f.array, BIGINT_VALUE_VERSION).is_err());
    let mut bad = f.data.clone();
    word(&mut bad, f.record as u32 as usize - 8, 0);
    assert!(validate_aggregate(&bad, ValueKind::Record, f.record, FUNCTION_VALUE_VERSION).is_err());
}
#[test]
fn function_value14_json_omits_properties_writes_array_null_and_omits_top_level() {
    let f = fixture(false);
    assert_eq!(encode(&f, ValueKind::Function, f.function).unwrap(), None);
    assert_eq!(encode(&f, ValueKind::ReferenceUnion, f.cell).unwrap(), None);
    assert_eq!(
        encode(&f, ValueKind::Record, f.record).unwrap(),
        Some(b"{}".to_vec())
    );
    assert_eq!(
        encode(&f, ValueKind::Array, f.array).unwrap(),
        Some(b"[null]".to_vec())
    );
}
#[test]
fn function_value14_private_capture_cycles_do_not_become_json_object_cycles() {
    for capture_array in [false, true] {
        let mut f = fixture(true);
        if capture_array {
            let function = ClosureView::read_envelope(&f.data, f.function).unwrap();
            let binding = function.environment.unwrap().bindings().next().unwrap() as u32 as usize;
            wide(&mut f.data, binding + 8, f.array);
            f.catalog.types[1] = CaptureType::Array { element: 0 };
        }
        f.catalog
            .validate_closure(&f.data, f.function, abi::FUEL)
            .unwrap();
        assert_eq!(
            encode(&f, ValueKind::Record, f.record).unwrap(),
            Some(b"{}".to_vec())
        );
        assert_eq!(
            encode(&f, ValueKind::Array, f.array).unwrap(),
            Some(b"[null]".to_vec())
        );
    }
}

#[test]
fn function_value14_json_validates_discarded_function_values_and_budget() {
    let mut f = fixture(false);
    let ptr = f.function as u32 as usize;
    word(&mut f.data, ptr + 8, 18);
    for (kind, payload) in [
        (ValueKind::Function, f.function),
        (ValueKind::Record, f.record),
        (ValueKind::Array, f.array),
        (ValueKind::ReferenceUnion, f.cell),
    ] {
        assert!(encode(&f, kind, payload).is_err());
    }
    let f = fixture(false);
    let mut work = 0;
    assert!(
        super::json::render_with_functions(
            &f.data,
            ValueKind::Function as u32,
            f.function,
            &f.catalog,
            &mut work
        )
        .is_err()
    );
    assert!(
        super::json::render_reference(
            &f.data,
            ValueKind::Function as u32,
            f.function,
            FUNCTION_VALUE_VERSION
        )
        .is_err()
    );
}

#[test]
fn function_value14_json_tojson_hook_requires_explicit_guest_dispatch() {
    let mut f = fixture(false);
    f.data[16..22].copy_from_slice(b"toJSON");
    word(&mut f.data, 44, 6);
    let failure = encode(&f, ValueKind::Record, f.record).unwrap_err();
    assert!(
        failure
            .to_string()
            .contains("toJSON hooks require guest call dispatch")
    );
}
#[test]
fn function_value14_public_codec_accepts_real_emitted_scalar_closure_snapshots() {
    let (data, mut catalog, functions) = super::function_envelope_tests::catalog_fixture(
        "export function main():void{let n=5;const f=():number=>n;console.log(f());}",
    );
    // The physical scalar-capture form is unchanged; an owner can explicitly
    // transcode its detached snapshot. This does not admit value14 into v30 VM calls.
    let mut work = abi::FUEL;
    assert!(
        crate::encode_value_graph_with_functions(
            &data,
            ValueKind::Function,
            functions[0],
            &catalog,
            &mut work
        )
        .is_err()
    );
    catalog.value_version = FUNCTION_VALUE_VERSION;
    for handle in functions {
        assert_eq!(
            crate::encode_value_graph_with_functions(
                &data,
                ValueKind::Function,
                handle,
                &catalog,
                &mut work
            )
            .unwrap(),
            None
        );
    }
    assert!(work < abi::FUEL);
}
