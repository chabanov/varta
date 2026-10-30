use super::*;
use crate::{abi, value};
fn put(data: &mut [u8], at: usize, n: u32) {
    data[at..at + 4].copy_from_slice(&n.to_le_bytes());
}
fn put64(data: &mut [u8], at: usize, n: u64) {
    data[at..at + 8].copy_from_slice(&n.to_le_bytes());
}
fn handle(ptr: usize, bytes: usize) -> u64 {
    ((bytes as u64) << 32) | ptr as u64
}
fn block(data: &mut [u8], header: usize, bytes: usize, flag: i32, mask: u32) {
    put(data, header, (abi::STRING_HEADER_BYTES + bytes) as u32);
    put(data, header + 4, value::HEAP_ALLOCATED_FLAG | flag as u32);
    put(data, header + 8, mask);
}
pub(super) fn graph(count: usize) -> (Vec<u8>, u64, usize, usize) {
    let function = abi::OWNED_STRING_BASE + 16;
    let environment = function + CLOSURE_BYTES + 16;
    let environment_bytes = 8 + count * 8;
    let binding = environment + environment_bytes + 16;
    let mut data = vec![0; binding + 24];
    block(
        &mut data,
        function - 16,
        CLOSURE_BYTES,
        HEAP_FLAG,
        if count == 0 {
            0
        } else {
            ENVIRONMENT_MASK as u32
        },
    );
    put(&mut data, function, 7);
    put(&mut data, function + 4, FUNCTION_FIELDS as u32);
    put(&mut data, function + 8, 17);
    if count > 0 {
        put64(
            &mut data,
            function + 16,
            handle(environment, environment_bytes),
        );
        block(
            &mut data,
            environment - 16,
            environment_bytes,
            ENVIRONMENT_FLAG,
            value::field_mask(count),
        );
        put(&mut data, environment, INTERNAL_KIND as u32);
        put(&mut data, environment + 4, count as u32);
        block(&mut data, binding - 16, 24, BINDING_FLAG, 1);
        put(&mut data, binding, 19);
        put(&mut data, binding + 4, 2);
        put64(&mut data, binding + 8, 0);
        put64(&mut data, binding + 16, 1);
        for i in 0..count {
            put64(&mut data, environment + 8 + i * 8, handle(binding, 24));
        }
    }
    (data, handle(function, CLOSURE_BYTES), environment, binding)
}
#[test]
fn closure_envelope_reads_empty_and_maximum_shared_capture_environments() {
    let (data, root, _, _) = graph(0);
    let function = ClosureView::read_envelope(&data, root).unwrap();
    assert_eq!((function.signature, function.code), (7, 17));
    assert!(function.environment.is_none());
    let (data, root, _, binding) = graph(MAX_CAPTURES);
    let function = ClosureView::read_envelope(&data, root).unwrap();
    let environment = function.environment.unwrap();
    assert_eq!(environment.bindings().len(), MAX_CAPTURES);
    for cell in environment.bindings() {
        assert_eq!(cell, handle(binding, 24));
        let view = BindingView::read_envelope(&data, cell).unwrap();
        assert!(view.matches_shape(19, 2, true));
        assert!(!view.matches_shape(20, 2, true));
        assert!(!view.matches_shape(19, 1, true));
        assert!(!view.matches_shape(19, 2, false));
        assert_eq!(view.status, Some(1));
    }
}
#[test]
fn closure_envelope_rejects_wrong_sizes_flags_ids_reserved_words_and_trace_masks() {
    let (data, root, _, _) = graph(1);
    let ptr = root as u32 as usize;
    for (at, n) in [
        (ptr - 16, 32),
        (ptr - 16, 41),
        (ptr - 12, 0),
        (ptr - 12, 131),
        (ptr - 8, 0),
        (ptr, MAX_TYPES as u32),
        (ptr + 4, 0),
        (ptr + 8, abi::MAX_USER_FUNCTIONS as u32),
        (ptr + 12, 1),
    ] {
        let mut bad = data.clone();
        put(&mut bad, at, n);
        assert!(
            ClosureView::read_envelope(&bad, root).is_err(),
            "offset {at}, value {n}"
        );
    }
    for bad in [
        0,
        u64::MAX,
        handle(ptr, CELL_BYTES),
        handle(ptr + 1, CLOSURE_BYTES),
        handle(8, CLOSURE_BYTES),
    ] {
        assert!(ClosureView::read_envelope(&data, bad).is_err());
    }
    for end in [0, ptr, ptr + 8, ptr + CLOSURE_BYTES - 1] {
        assert!(ClosureView::read_envelope(&data[..end], root).is_err());
    }
}
#[test]
fn closure_envelope_checks_environment_and_binding_envelopes_without_interpreting_payloads() {
    let (data, root, environment, binding) = graph(1);
    for (at, n) in [
        (environment, 1),
        (environment + 4, 0),
        (environment - 8, 0),
        (environment - 12, 0),
        (binding - 12, 0),
        (binding - 8, 2),
        (binding + 4, 1),
    ] {
        let mut bad = data.clone();
        put(&mut bad, at, n);
        assert!(
            ClosureView::read_envelope(&bad, root).is_err(),
            "offset {at}"
        );
    }
    let mut bad = data.clone();
    put64(&mut bad, environment + 8, handle(environment, 16));
    assert!(ClosureView::read_envelope(&bad, root).is_err());
    let mut bad = data.clone();
    put64(&mut bad, root as u32 as usize + 16, 0);
    assert!(ClosureView::read_envelope(&bad, root).is_err());
    // Recursive function captures are legal. Envelope validation never recursively
    // follows opaque payload bits or mistakes them for public value tags.
    let mut cycle = data;
    put64(&mut cycle, binding + 8, root);
    assert!(ClosureView::read_envelope(&cycle, root).is_ok());
}
#[test]
fn binding_envelope_distinguishes_scalar_bits_from_traced_handles() {
    let (mut data, _, _, binding) = graph(1);
    block(&mut data, binding - 16, 16, BINDING_FLAG, 0);
    put(&mut data, binding + 4, 1);
    put64(&mut data, binding + 8, u64::MAX);
    let view = BindingView::read_envelope(&data, handle(binding, 16)).unwrap();
    assert!(!view.reference);
    assert_eq!(view.value, u64::MAX);
    assert_eq!(view.status, None);
    assert!(view.matches_shape(19, 1, false));
    for bytes in [0, 8, 32, usize::MAX] {
        assert!(BindingView::read_envelope(&data, handle(binding, bytes)).is_err());
    }
}
#[test]
fn closure_envelope_preserves_legacy_cells_and_old_version_tag_admission() {
    let (mut data, root, _, _) = graph(0);
    let ptr = root as u32 as usize;
    block(&mut data, ptr - 16, CELL_BYTES, HEAP_FLAG, 0);
    put(&mut data, ptr + 4, 0);
    assert!(ClosureView::read_envelope(&data, handle(ptr, CELL_BYTES)).is_err());
    assert_eq!(
        value::ValueKind::from_tag(12),
        Some(value::ValueKind::Function)
    );
    assert!(!value::ValueKind::Function.supported_by(value::BIGINT_VALUE_VERSION));
    assert!(value::ValueKind::Function.supported_by(value::FUNCTION_VALUE_VERSION));
    assert!(value::supported_version(value::BIGINT_VALUE_VERSION));
    assert!(!value::supported_version(
        value::HASHED_DICTIONARY_VALUE_VERSION + 1
    ));
}
