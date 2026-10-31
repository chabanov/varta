//! Owned text results (v38): a linked operation that returns text returns a
//! fresh guest string from the guest allocator, never the 1 MiB result
//! buffer, so a large result within the memory budget does not trap and an
//! exhausted budget is the allocator's typed resource failure.
//!
//! The contract with the compiler's call sites is a host string result's,
//! except for where the text lives: the export returns the string's pointer
//! (0 for the empty string) and sets `env_result_len` to its byte length,
//! and the call site packs the two into the string handle without a
//! snapshot copy. Like a snapshot's result, the block is unrooted: the call
//! site consumes it before any later allocation.
use crate::guest;
use varta_common::{
    abi::{MAX_RESULT_BYTES, RESULT_BASE},
    limits::{Resource, V38_MAX_STRING_BYTES},
};
use varta_intrinsics::{IntrinsicError, json::Sink};

/// UTF-16 code units are two bytes.
const UNIT_BYTES: usize = size_of::<u16>();

/// End the execution with the typed one-value limit when a result of `len`
/// bytes exceeds the v38 string limit.
fn check_size(len: usize) {
    if len > V38_MAX_STRING_BYTES {
        guest::fail(
            Resource::ValueSize as i32,
            u32::try_from(len).unwrap_or(u32::MAX),
        );
    }
}

/// The owned guest string of the UTF-16LE `bytes`, computed in the per-call
/// arena of an [`crate::arena::entry`]: its pointer, with `env_result_len`
/// set. When the arena continued above the guest heap (where the allocator
/// is about to place blocks), the bytes are staged in the result buffer
/// first; the results this path serves fit it.
pub fn owned(bytes: &[u8]) -> i32 {
    let len = bytes.len();
    if len == 0 {
        guest::set_result_len(0);
        return 0;
    }
    check_size(len);
    let ptr = if crate::arena::spilled() {
        if len > MAX_RESULT_BYTES {
            guest::fatal(IntrinsicError::ResultTooLarge);
        }
        guest::result(bytes);
        let ptr = guest::allocate_block(len);
        // SAFETY: the result buffer holds `len` bytes (just written) and
        // `ptr` is a fresh guest block of at least `len` bytes; both lie in
        // memory 0 and are disjoint.
        unsafe {
            core::ptr::copy_nonoverlapping(RESULT_BASE as *const u8, ptr as *mut u8, len);
        }
        ptr
    } else {
        let ptr = guest::allocate_block(len);
        // SAFETY: `bytes` lives in the reserved arena of the intrinsic
        // region, which the guest allocator never touches, and `ptr` is a
        // fresh guest block of at least `len` bytes.
        unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, len) };
        ptr
    };
    guest::set_result_len(len as i32);
    ptr as i32
}

/// A fresh guest string of `units` code units, filled by `write`, which
/// receives a sink writing into it: its packed handle. The block is rooted
/// while `write` runs (it may allocate scratch, and so collect) and the
/// caller's root reset drops that root.
pub fn filled(units: usize, write: impl FnOnce(&mut Fill)) -> u64 {
    let len = units.saturating_mul(UNIT_BYTES);
    if len == 0 {
        return 0;
    }
    check_size(len);
    let ptr = guest::allocate_block(len);
    let handle = ((len as u64) << u32::BITS) | ptr as u64;
    guest::retain_root(handle);
    let mut fill = Fill {
        at: ptr,
        end: ptr + len,
    };
    write(&mut fill);
    if fill.at != fill.end {
        guest::trap();
    }
    handle
}

/// Code units of output the first pass of [`produce`] keeps in scratch: a
/// result up to this length costs one pass and one copy, a longer one a
/// second, writing pass, so scratch stays bounded and the result string is
/// allocated once at its exact length.
const BUFFERED_UNITS: usize = 16 * 1024;

/// Units of the first pass kept inline, before any scratch is allocated:
/// most results (a Number, a short string, a small record) need none.
const INLINE_UNITS: usize = 32;

/// Text that `produce` writes into a fresh guest string of exactly its
/// length ([`filled`]): the first pass counts it, keeping up to
/// [`BUFFERED_UNITS`] (the first [`INLINE_UNITS`] in place, the rest in
/// scratch) to copy; a longer text is written by a second pass. The caller
/// runs it in a [`crate::arena::scratch`] entry, since the guest allocator
/// runs between the passes, and roots every guest value `produce` reads.
pub fn produce<E>(mut produce: impl FnMut(&mut Pass<'_>) -> Result<(), E>) -> Result<u64, E> {
    // Built in place and passed by reference: moving it would copy it
    // (`memory.copy` is charged per byte).
    let mut buffer = Buffered {
        inline: [0; INLINE_UNITS],
        spill: alloc::vec::Vec::new(),
        count: 0,
    };
    produce(&mut Pass::Buffer(&mut buffer))?;
    if buffer.count <= BUFFERED_UNITS {
        let inline = buffer.count.min(INLINE_UNITS);
        return Ok(filled(buffer.count, |fill| {
            fill.units(&buffer.inline[..inline]);
            fill.units(&buffer.spill);
        }));
    }
    let units = buffer.count;
    drop(buffer);
    let mut outcome = Ok(());
    let handle = filled(units, |fill| outcome = produce(&mut Pass::Fill(fill)));
    outcome.map(|()| handle)
}

/// The first pass's units: [`INLINE_UNITS`] in place, the rest of up to
/// [`BUFFERED_UNITS`] in scratch, and the count of all of them.
pub struct Buffered {
    inline: [u16; INLINE_UNITS],
    spill: alloc::vec::Vec<u16>,
    count: usize,
}
impl Buffered {
    fn push(&mut self, units: &[u16]) {
        let start = self.count;
        self.count += units.len();
        if self.count > BUFFERED_UNITS {
            if self.spill.capacity() != 0 {
                self.spill = alloc::vec::Vec::new();
            }
            return;
        }
        let inline = units.len().min(INLINE_UNITS.saturating_sub(start));
        if inline != 0 {
            self.inline[start..start + inline].copy_from_slice(&units[..inline]);
        }
        self.spill.extend_from_slice(&units[inline..]);
    }
}

/// One pass of [`produce`]: counting (with the buffered prefix), then
/// filling. (A sink enum, not a trait object: linked code has no function
/// table.)
pub enum Pass<'a> {
    Buffer(&'a mut Buffered),
    Fill(&'a mut Fill),
}
impl Sink for Pass<'_> {
    fn units(&mut self, units: &[u16]) {
        match self {
            Self::Buffer(buffer) => buffer.push(units),
            Self::Fill(fill) => fill.units(units),
        }
    }
    fn ascii(&mut self, bytes: &[u8]) {
        match self {
            Self::Buffer(buffer) => {
                let mut units = [0u16; INLINE_UNITS];
                for chunk in bytes.chunks(INLINE_UNITS) {
                    for (unit, byte) in units.iter_mut().zip(chunk) {
                        *unit = u16::from(*byte);
                    }
                    buffer.push(&units[..chunk.len()]);
                }
            }
            Self::Fill(fill) => {
                for byte in bytes {
                    fill.unit(u16::from(*byte));
                }
            }
        }
    }
}

/// A sink writing code units into a fresh guest string; writing past its
/// end traps.
pub struct Fill {
    at: usize,
    end: usize,
}
impl Fill {
    /// Write one code unit.
    pub fn unit(&mut self, unit: u16) {
        if self.end - self.at < UNIT_BYTES {
            guest::trap();
        }
        // SAFETY: `[at, at + 2)` lies inside the fresh block `filled`
        // allocated (checked above), which nothing else writes.
        unsafe { (self.at as *mut u16).write_unaligned(unit) };
        self.at += UNIT_BYTES;
    }
}
impl Sink for Fill {
    fn units(&mut self, units: &[u16]) {
        let len = units.len() * UNIT_BYTES;
        if self.end - self.at < len {
            guest::trap();
        }
        // SAFETY: `[at, at + len)` lies inside the fresh block `filled`
        // allocated (checked above), disjoint from `units` (library memory
        // or guest scratch), and nothing else writes it.
        unsafe {
            core::ptr::copy_nonoverlapping(units.as_ptr().cast::<u8>(), self.at as *mut u8, len);
        }
        self.at += len;
    }
}
