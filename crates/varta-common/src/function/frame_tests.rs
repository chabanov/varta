use super::frame::*;
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
fn block(data: &mut [u8], ptr: usize, bytes: usize, flag: i32, mask: u32) {
    let header = ptr - abi::STRING_HEADER_BYTES;
    put(data, header, (abi::STRING_HEADER_BYTES + bytes) as u32);
    put(data, header + 4, value::HEAP_ALLOCATED_FLAG | flag as u32);
    put(data, header + 8, mask);
}
const SEGMENT_BYTES: usize = 8 + 2 * 8;
/// Frame -> frame environment -> one segment with a frame cell and an absent shared cell.
fn fixture() -> (Vec<u8>, usize) {
    let frame = abi::OWNED_STRING_BASE + 16;
    let table = frame + FRAME_BYTES + 16;
    let segment = table + 16 + 16;
    let cell = segment + SEGMENT_BYTES + 16;
    let mut data = vec![0; cell + 16];
    block(&mut data, frame, FRAME_BYTES, FRAME_FLAG, FRAME_MASK as u32);
    put(&mut data, frame, 3);
    put(&mut data, frame + 4, FRAME_FIELDS as u32);
    put64(&mut data, frame + 8, 17 | (2 << 32));
    put64(
        &mut data,
        frame + 16,
        u64::from(STATUS_SUSPENDED) | (9 << 32),
    );
    put64(&mut data, frame + 24, handle(table, 16));
    block(&mut data, table, 16, ENVIRONMENT_FLAG, 1);
    put(&mut data, table, FRAME_ENVIRONMENT_KIND as u32);
    put(&mut data, table + 4, 1);
    put64(&mut data, table + 8, handle(segment, SEGMENT_BYTES));
    block(&mut data, segment, SEGMENT_BYTES, ENVIRONMENT_FLAG, 3);
    put(&mut data, segment, FRAME_SEGMENT_KIND as u32);
    put(&mut data, segment + 4, 2);
    put64(&mut data, segment + 8, handle(cell, 16));
    block(&mut data, cell, 16, BINDING_FLAG, 0);
    put(&mut data, cell, 5);
    put(&mut data, cell + 4, 1);
    put64(&mut data, cell + 8, 42);
    (data, frame)
}

#[test]
fn frame_views_read_exact_envelopes() {
    let (data, frame) = fixture();
    let view = FrameView::read_envelope(&data, frame as u32).unwrap();
    assert_eq!(
        (
            view.layout,
            view.code,
            view.state,
            view.status,
            view.generation
        ),
        (3, 17, 2, STATUS_SUSPENDED, 9)
    );
    assert_eq!(view.bindings.len(), 2);
    assert_eq!(view.bindings[1], 0, "an absent shared cell stays zero");
    assert_eq!(view.completion, None);
    assert_eq!(
        view.token(frame as u32),
        ResumeToken {
            frame: frame as u32,
            generation: 9
        }
    );
    // Mask, kind, field count, status, handler and environment kinds are exact.
    let table = frame + FRAME_BYTES + 16;
    let segment = table + 16 + 16;
    for (at, value) in [
        (
            frame - abi::STRING_HEADER_BYTES + value::HEAP_LINK_OFFSET,
            4,
        ),
        (
            frame - abi::STRING_HEADER_BYTES + value::HEAP_FLAGS_OFFSET,
            1 | ENVIRONMENT_FLAG as u32,
        ),
        (frame + 4, 4),
        (frame + 16, 0),
        (frame + 40, 1),
        (table, FRAME_SEGMENT_KIND as u32),
        (segment, INTERNAL_KIND as u32),
    ] {
        let mut data = data.clone();
        put(&mut data, at, value);
        assert!(
            FrameView::read_envelope(&data, frame as u32).is_err(),
            "{at}"
        );
    }
    assert!(FrameView::read_envelope(&data, (frame + 8) as u32).is_err());
    // A frame without saved bindings has no environment.
    let mut empty = data.clone();
    put64(&mut empty, frame + 24, 0);
    assert!(
        FrameView::read_envelope(&empty, frame as u32)
            .unwrap()
            .bindings
            .is_empty()
    );
}

#[test]
fn tokens_inputs_and_outcomes_have_fixed_encodings() {
    let token = ResumeToken {
        frame: 0x0004_0010,
        generation: 7,
    };
    assert_eq!(ResumeToken::from_bits(token.to_bits()), token);
    assert_eq!(token.to_bits() as u64 >> 32, 7);
    assert!(FrameInput::Float64.admits(f64::NAN.to_bits()));
    assert!(FrameInput::Boolean.admits(1) && !FrameInput::Boolean.admits(2));
    assert!(FrameInput::Undefined.admits(0) && !FrameInput::Undefined.admits(1));
    for outcome in [Outcome::Suspended, Outcome::Returned, Outcome::Threw] {
        assert_eq!(Outcome::from_code(outcome as i32), Some(outcome));
    }
    assert_eq!(Outcome::from_code(3), None);
    assert_ne!(
        MAX_FRAME_BINDINGS, MAX_CAPTURES,
        "frames have their own budget"
    );
}

fn catalog(version: u32, frames: Vec<FrameLayout>) -> FunctionCatalog {
    FunctionCatalog {
        version,
        value_version: value::BIGINT_VALUE_VERSION,
        types: vec![CaptureType::Float64],
        bindings: vec![CaptureBinding {
            ty: 0,
            result: false,
        }],
        functions: vec![],
        frames,
    }
}
fn layout(code: u32) -> FrameLayout {
    FrameLayout {
        code,
        slots: vec![FrameSlot {
            binding: 0,
            shared: false,
        }],
        states: vec![FrameState {
            input: FrameInput::Boolean,
            yielded: 0,
        }],
        result: Some(0),
        return_input: Some(FrameInput::Float64),
        exception: 0,
    }
}

#[test]
fn frame_catalogs_are_versioned_and_older_catalogs_stay_readable() {
    let v1 = catalog(CATALOG_VERSION, vec![]).to_bytes().unwrap();
    assert!(!String::from_utf8_lossy(&v1).contains("frames"));
    FunctionCatalog::from_bytes(&v1).unwrap();
    FunctionCatalog::from_frame_bytes(&v1).unwrap();
    let v2 = catalog(FRAME_CATALOG_VERSION, vec![layout(4)])
        .to_bytes()
        .unwrap();
    assert!(FunctionCatalog::from_bytes(&v2).is_err());
    assert_eq!(
        FunctionCatalog::from_frame_bytes(&v2).unwrap().frames,
        [layout(4)]
    );
    let mut invalid = vec![
        catalog(FRAME_CATALOG_VERSION, vec![]),
        catalog(CATALOG_VERSION, vec![layout(4)]),
        catalog(FRAME_CATALOG_VERSION, vec![layout(4), layout(4)]),
        catalog(
            FRAME_CATALOG_VERSION,
            vec![layout(abi::MAX_USER_FUNCTIONS as u32)],
        ),
        catalog(FRAME_CATALOG_VERSION + 1, vec![layout(4)]),
    ];
    for change in [
        |l: &mut FrameLayout| l.exception = 1,
        |l: &mut FrameLayout| l.slots[0].binding = 1,
        |l: &mut FrameLayout| l.states[0].yielded = 1,
        |l: &mut FrameLayout| l.result = Some(1),
        |l: &mut FrameLayout| l.states = vec![l.states[0].clone(); MAX_SUSPENSION_STATES + 1],
        |l: &mut FrameLayout| l.slots = vec![l.slots[0].clone(); MAX_FRAME_BINDINGS + 1],
    ] {
        let mut row = layout(4);
        change(&mut row);
        invalid.push(catalog(FRAME_CATALOG_VERSION, vec![row]));
    }
    for catalog in invalid {
        assert!(catalog.to_bytes().is_err(), "{catalog:?}");
    }
}
