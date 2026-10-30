//! Linked checked JSON (`json_parse_v19`): the host import's exact checks
//! and outcomes, through the shared `varta_common::json` decoder
//! ([`scan`](varta_common::json::scan)) and top-down builder
//! ([`build`](varta_common::json::build)).
//!
//! The whole input is checked into a tape before the guest allocator runs.
//! The decoder's scratch (tape, frames, key sets) lives in ordinary guest
//! heap blocks rooted for the call ([`crate::arena::scratch`]); the input
//! string is rooted too, so the collector the builder's allocations may run
//! keeps both. The result is built top-down: every block is published into
//! its reachable parent (the root retained) before the next allocation.
//! Syntax and type mismatches are thrown as `SyntaxError`/`TypeError`
//! through the exception channel, as the host does; input limits end the
//! execution with the host's typed limit error, and every other failure
//! (a malformed schema, descriptor or channel) is a trap.
use crate::{
    arena::scratch,
    guest::{self, Guest, GuestHeap},
};
use varta_common::{
    abi, exceptions as channel,
    json::{
        HASHED_DICTIONARY_SCHEMA_VERSION, JsonDecodeError, JsonInputLimit, JsonSchema,
        SchemaBounds,
        scan::{self, JsonLimits, Tape, Unbounded},
    },
    memory::Memory,
    value::*,
};

/// Messages of the thrown errors, as the host import throws them.
const SYNTAX_MESSAGE: &str = "Invalid JSON syntax";
const TYPE_MESSAGE: &str = "JSON value does not match the checked schema";
/// The first scratch block; later blocks double (the tape grows in place
/// while it is the newest allocation).
const SCRATCH_BYTES: usize = 4096;

/// What a checked parse produced.
enum Outcome {
    Value(u64),
    Throw(ErrorClass),
}

/// `json_parse_v19 (ptr, len, schema_ptr, schema_len) -> payload`.
#[unsafe(no_mangle)]
pub extern "C" fn json_parse_v19(ptr: i32, len: i32, schema_ptr: i32, schema_len: i32) -> i64 {
    let limits = JsonLimits::V38;
    if len >= 0 && len as usize > limits.input_bytes {
        limit(JsonInputLimit::Bytes);
    }
    if ptr < 0 || len < 0 || schema_ptr < 0 || schema_len < 0 {
        guest::trap();
    }
    ready();
    let (ptr, len) = (ptr as usize, len as usize);
    let outcome = scratch(SCRATCH_BYTES, || {
        let region = guest::static_region().unwrap_or_else(|| guest::trap());
        let schema = schema(region, schema_ptr as usize, schema_len as usize);
        let Some(text) = Guest.borrow(ptr, len) else {
            guest::trap();
        };
        if !len.is_multiple_of(size_of::<u16>()) {
            guest::trap();
        }
        // The input string stays alive while the builder allocates.
        guest::retain_root(((len as u64) << u32::BITS) | ptr as u64);
        let mut tape = Tape::default();
        match scan::scan(&text, &schema, limits, &mut tape, &mut Unbounded) {
            Ok(()) => {
                let Ok(value) =
                    varta_common::json::build::build(&tape, &text, &schema, &mut GuestHeap);
                Outcome::Value(value)
            }
            Err(JsonDecodeError::Syntax { .. }) => Outcome::Throw(ErrorClass::SyntaxError),
            Err(JsonDecodeError::Type) => Outcome::Throw(ErrorClass::TypeError),
            Err(JsonDecodeError::Limit(input)) => limit(input),
            Err(JsonDecodeError::Schema(_)) => guest::trap(),
        }
    });
    // The scratch and the input are unrooted; root the result alone, as
    // the host's retention does.
    match outcome {
        Outcome::Value(value) => {
            guest::retain_root(value);
            guest::set_result_len(0);
            value as i64
        }
        Outcome::Throw(class) => {
            throw(class);
            0
        }
    }
}

/// The schema at `[at, at + len)` of the static region, read, validated
/// within the v38 limits and checked against its descriptors.
fn schema(region: abi::StaticRegion, at: usize, len: usize) -> JsonSchema {
    if !region.contains(at, len) {
        guest::trap();
    }
    let Some(bytes) = Guest.borrow(at, len) else {
        guest::trap();
    };
    let schema = JsonSchema::read(&bytes, SchemaBounds::WASM, abi::MAX_MEMORY_BYTES)
        .unwrap_or_else(|_| guest::trap());
    if schema.version != HASHED_DICTIONARY_SCHEMA_VERSION
        || schema.check_descriptors(&Guest, region).is_err()
    {
        guest::trap();
    }
    schema
}

/// The exception channel must be empty and canonical before a recoverable
/// parse, as the host checks.
fn ready() {
    let (version, pending, value) = guest::exception_state();
    let mut root = [0u8; channel::RESERVED_ROOT_BYTES];
    if version != channel::HASHED_DICTIONARY_CHANNEL_VERSION
        || pending != 0
        || value != 0
        || !Guest.copy_to(channel::ROOT, &mut root)
        || root.iter().any(|byte| *byte != 0)
    {
        guest::trap();
    }
}

/// End the execution with the host's typed input-limit error.
fn limit(limit: JsonInputLimit) -> ! {
    guest::fail(limit.linked_code(), 0)
}

/// A stamped, retained block of `len` bytes from the guest allocator.
fn owned(len: usize, flag: i32, link: u32, write: impl FnOnce(usize)) -> u64 {
    let ptr = guest::allocate_block(len);
    write(ptr);
    let header = ptr - abi::STRING_HEADER_BYTES;
    // SAFETY: `ptr` is a fresh block of at least `len` payload bytes whose
    // header precedes it.
    unsafe {
        ((header + HEAP_FLAGS_OFFSET) as *mut u32)
            .write_unaligned(HEAP_ALLOCATED_FLAG | flag as u32);
        ((header + HEAP_LINK_OFFSET) as *mut u32).write_unaligned(link);
    }
    let handle = ((len as u64) << u32::BITS) | ptr as u64;
    guest::retain_root(handle);
    handle
}

/// Throw a native error of `class` with the host's message: its message
/// string, error object and union cell, each retained, then the channel.
fn throw(class: ErrorClass) {
    let message = match class {
        ErrorClass::SyntaxError => SYNTAX_MESSAGE,
        _ => TYPE_MESSAGE,
    };
    let units = message.encode_utf16().count();
    let message = owned(units * size_of::<u16>(), 0, 0, |ptr| {
        for (index, unit) in message.encode_utf16().enumerate() {
            // SAFETY: `ptr` holds `units` units.
            unsafe { ((ptr + index * size_of::<u16>()) as *mut u16).write_unaligned(unit) };
        }
    });
    let object = owned(ERROR_BYTES, HEAP_ERROR_FLAG, 1, |ptr| {
        // SAFETY: `ptr` holds an error object.
        unsafe {
            (ptr as *mut u32)
                .write_unaligned(((class as u32) << ERROR_CLASS_SHIFT) | ERROR_OWN_MESSAGE);
            ((ptr + size_of::<u32>()) as *mut u32).write_unaligned(1);
            ((ptr + 2 * size_of::<u32>()) as *mut u64).write_unaligned(message);
        }
    });
    let cell = owned(UNION_BYTES, HEAP_UNION_FLAG, 1, |ptr| {
        // SAFETY: `ptr` holds a union cell.
        unsafe {
            (ptr as *mut u32).write_unaligned(ValueKind::Error as u32);
            ((ptr + size_of::<u32>()) as *mut u32).write_unaligned(1);
            ((ptr + UNION_META_BYTES) as *mut u64).write_unaligned(object);
        }
    });
    guest::set_result_len(0);
    // SAFETY: the channel's reserved root slot is guest memory below the
    // heap, outside the intrinsic region.
    unsafe { (channel::ROOT as *mut u64).write_unaligned(cell) };
    guest::throw(cell);
}
