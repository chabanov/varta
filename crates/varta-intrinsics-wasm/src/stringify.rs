//! Linked serialization (v38): `json_stringify_owned` and the diagnostic
//! text of a logged value, `value_format_v12`, through the shared value18
//! encoder (`varta_intrinsics::json`).
//!
//! Both run in a [`scratch`] entry: the encoder's frames, path set and field
//! lists live in guest heap blocks rooted for the call, and the serialized
//! value is rooted too, because the result string is allocated between the
//! measuring and the writing pass ([`text::produce`]) and the writing pass
//! may allocate scratch. Nothing else is allocated in guest memory: the
//! result is exactly as long as the host's, so the memory budget admits the
//! same values. A fatal failure the host reports with a message ends the
//! execution with its linked code (`EncodeError::linked_code`), which the
//! runtime reports as the host's error; a noncanonical layout traps.
use crate::{
    arena::scratch,
    guest::{self, Guest, bounds},
    text,
};
use varta_bigint::Budget;
use varta_common::console::RenderMode;
use varta_common::value::{ErrorClass, ValueKind, string_range};
use varta_intrinsics::{
    bigint::MAX_BITS,
    inspect,
    json::{self, EncodeError, Sink},
};

/// The first scratch block; later blocks double.
const SCRATCH_BYTES: usize = 1024;
/// The sign bit of the exported i64: set when the low word is the error
/// class of a recoverable failure instead of a handle (handles are below
/// 2^62: a string's length is at most 2^30 bytes).
const ERROR_FLAG: u64 = 1 << 63;

/// End the execution on a fatal serialization failure.
fn fatal(error: EncodeError) -> ! {
    match error.linked_code() {
        Some((code, requested)) => guest::fail(code, requested),
        None => guest::trap(),
    }
}

/// Root a serialized guest value: every kind whose payload is a handle.
fn root(kind: u32, payload: u64) {
    if ValueKind::from_tag(kind).is_some_and(ValueKind::is_reference) {
        guest::retain_root(payload);
    }
}

/// `json_stringify_owned (kind, lo, hi) -> (handle, error)`, exported as one
/// i64 that the linker's adapter unpacks: the retained owned string (0 for
/// `undefined`), or [`ERROR_FLAG`] with the `TypeError` class when the value
/// holds a BigInt.
#[unsafe(no_mangle)]
pub extern "C" fn json_stringify_owned(kind: i32, lo: i32, hi: i32) -> i64 {
    let (kind, payload) = (
        kind as u32,
        (u64::from(hi as u32) << u32::BITS) | u64::from(lo as u32),
    );
    let bounds = bounds();
    let outcome = scratch(SCRATCH_BYTES, || {
        root(kind, payload);
        text::produce(|sink| json::stringify(&Guest, kind, payload, bounds, sink).map(drop))
    });
    match outcome {
        Ok(handle) => {
            // Retained, as the host import retains its result.
            guest::retain_root(handle);
            handle as i64
        }
        Err(EncodeError::BigInt) => (ERROR_FLAG | ErrorClass::TypeError as u64) as i64,
        Err(error) => fatal(error),
    }
}

/// `value_format_v12 (cell) -> ptr`: the text v38 logs through
/// `log_string16` for the `unknown[]` cell of a `console.log` argument list
/// (`node_console`): Node's `util.formatWithOptions` of the arguments
/// ([`inspect::format_arguments`]), as an owned string ([`text`]).
#[unsafe(no_mangle)]
pub extern "C" fn value_format_v12(cell: i64) -> i32 {
    let bounds = bounds();
    let handle = scratch(SCRATCH_BYTES, || {
        guest::retain_root(cell as u64);
        let units = inspect::format_arguments(
            &Guest,
            cell as u64,
            bounds,
            &mut Budget::new(MAX_BITS, u64::MAX),
        )?;
        text::produce(|sink| {
            sink.units(&units);
            Ok::<(), EncodeError>(())
        })
    })
    .unwrap_or_else(|error| fatal(error));
    let (ptr, len) = string_range(handle);
    guest::set_result_len(len as i32);
    ptr as i32
}

/// `console_render (mode, cell) -> ptr`: the text of a v38 console render
/// request (`console_methods`: `console.dir` or `console.table`) held by
/// the `unknown[]` cell `cell` ([`inspect::render`]), as an owned string
/// like [`value_format_v12`]. An unknown mode traps.
#[unsafe(no_mangle)]
pub extern "C" fn console_render(mode: i32, cell: i64) -> i32 {
    let Some(mode) = RenderMode::from_word(mode) else {
        guest::trap();
    };
    let bounds = bounds();
    let handle = scratch(SCRATCH_BYTES, || {
        guest::retain_root(cell as u64);
        let units = inspect::render(
            &Guest,
            mode,
            cell as u64,
            bounds,
            &mut Budget::new(MAX_BITS, u64::MAX),
        )?;
        text::produce(|sink| {
            sink.units(&units);
            Ok::<(), EncodeError>(())
        })
    })
    .unwrap_or_else(|error| fatal(error));
    let (ptr, len) = string_range(handle);
    guest::set_result_len(len as i32);
    ptr as i32
}
