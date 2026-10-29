//! Continuation frames: the K4.4 suspension foundation (runtime-internal).
//!
//! No source profile admits suspension yet. A frame is its own managed kind,
//! distinct from closures and environments; it stores saved bindings through
//! the existing binding-cell and environment retention mechanisms. Validators
//! here read immutable guest-memory snapshots, allocate nothing beyond their
//! result and never invoke guest code.
use super::*;
use crate::{abi, value};
use value::ValueAbiError;

/// Heap flag of a continuation frame; never combined with closure flags.
pub const FRAME_FLAG: i32 = 4096;
/// Physical fields after the `[layout-id:u32, field-count:u32]` header.
pub const FRAME_FIELDS: usize = 5;
pub const FRAME_BYTES: usize = value::RECORD_META_BYTES + FRAME_FIELDS * value::RECORD_SLOT_BYTES;
/// `resume-code:u32 | state:u32`; never a reference.
pub const CONTROL_FIELD: u32 = 0;
/// `status:u32 | generation:u32`; never a reference.
pub const STATUS_FIELD: u32 = 1;
/// Frame environment (kind [`FRAME_ENVIRONMENT_KIND`]) holding binding segments.
pub const ENVIRONMENT_FIELD: u32 = 2;
/// Binding cell with the yielded, returned or thrown value; zero when none.
pub const COMPLETION_FIELD: u32 = 3;
/// Reserved zero: in this block the handler context is static per state.
pub const HANDLER_FIELD: u32 = 4;
/// Exact child-reference mask: environment, completion and handler slots.
pub const FRAME_MASK: i32 =
    (1 << ENVIRONMENT_FIELD) | (1 << COMPLETION_FIELD) | (1 << HANDLER_FIELD);
/// Environment kinds private to frames; closure environments keep [`INTERNAL_KIND`].
pub const FRAME_ENVIRONMENT_KIND: i32 = 1;
pub const FRAME_SEGMENT_KIND: i32 = 2;
/// One segment's width is bounded by the 32-bit environment edge mask.
pub const SEGMENT_BINDINGS: usize = u32::BITS as usize;
pub const MAX_FRAME_SEGMENTS: usize = 8;
/// The frame's own budget, deliberately independent of [`MAX_CAPTURES`].
pub const MAX_FRAME_BINDINGS: usize = SEGMENT_BINDINGS * MAX_FRAME_SEGMENTS;
/// Suspension points per suspendable function, excluding the initial state.
pub const MAX_SUSPENSION_STATES: usize = 256;
/// Permanent root-table slots retaining live (suspended or running) frames.
pub const MAX_LIVE_FRAMES: usize = 64;
/// State zero means "created, body not entered yet".
pub const INITIAL_STATE: u32 = 0;
pub const STATUS_SUSPENDED: u32 = 1;
pub const STATUS_RUNNING: u32 = 2;
pub const STATUS_COMPLETED: u32 = 3;
/// The closed exported dispatcher: `(token:i64, completion:i32, value:i64) -> i32`.
pub const RESUME_EXPORT: &str = "varta_resume";
/// Frame catalog rows exist only in catalog version 2.
pub const FRAME_CATALOG_VERSION: u32 = 2;

/// How the scheduler resumes a suspended frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Completion {
    Normal = 0,
    Throw = 1,
    Return = 2,
}
/// What `varta_resume` reports after the guest returns control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Outcome {
    Suspended = 0,
    Returned = 1,
    Threw = 2,
}
impl Outcome {
    pub fn from_code(code: i32) -> Option<Self> {
        [Self::Suspended, Self::Returned, Self::Threw]
            .into_iter()
            .find(|outcome| *outcome as i32 == code)
    }
}
/// Admissible scalar resume inputs. Host-supplied references need a validated
/// transport and are not admitted in this block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FrameInput {
    /// Any binary64 bit pattern.
    Float64,
    /// Canonical 0 or 1.
    Boolean,
    /// Canonical 0.
    Undefined,
}
impl FrameInput {
    pub fn admits(self, value: u64) -> bool {
        match self {
            Self::Float64 => true,
            Self::Boolean => value <= 1,
            Self::Undefined => value == 0,
        }
    }
}

/// A resume token names one live frame and one suspension generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResumeToken {
    pub frame: u32,
    pub generation: u32,
}
impl ResumeToken {
    pub fn to_bits(self) -> i64 {
        (u64::from(self.generation) << u32::BITS | u64::from(self.frame)) as i64
    }
    pub fn from_bits(bits: i64) -> Self {
        let bits = bits as u64;
        Self {
            frame: bits as u32,
            generation: (bits >> u32::BITS) as u32,
        }
    }
}

fn invalid() -> ValueAbiError {
    ValueAbiError("noncanonical continuation frame".into())
}
fn word(data: &[u8], at: usize) -> Result<u32, ValueAbiError> {
    data.get(at..at.checked_add(4).ok_or_else(invalid)?)
        .and_then(|v| v.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(invalid)
}
fn wide(data: &[u8], at: usize) -> Result<u64, ValueAbiError> {
    data.get(at..at.checked_add(8).ok_or_else(invalid)?)
        .and_then(|v| v.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or_else(invalid)
}
/// Check an aligned allocated block with an exact flag word and edge mask.
fn block(data: &[u8], ptr: usize, bytes: usize, flag: i32, mask: u32) -> Result<(), ValueAbiError> {
    if ptr < abi::OWNED_STRING_BASE + abi::STRING_HEADER_BYTES
        || !ptr.is_multiple_of(value::HEAP_ALIGNMENT)
    {
        return Err(invalid());
    }
    let header = ptr - abi::STRING_HEADER_BYTES;
    let allocated = word(data, header)? as usize;
    if allocated < abi::STRING_HEADER_BYTES + bytes
        || !allocated.is_multiple_of(value::HEAP_ALIGNMENT)
        || header
            .checked_add(allocated)
            .is_none_or(|end| end > data.len())
        || word(data, header + value::HEAP_FLAGS_OFFSET)?
            != value::HEAP_ALLOCATED_FLAG | flag as u32
        || word(data, header + value::HEAP_LINK_OFFSET)? != mask
    {
        return Err(invalid());
    }
    Ok(())
}
fn field(ptr: usize, index: u32) -> usize {
    ptr + value::RECORD_META_BYTES + index as usize * value::RECORD_SLOT_BYTES
}
/// A managed handle carries its exact byte length in the high word.
fn handle_ptr(handle: u64, bytes: usize) -> Result<usize, ValueAbiError> {
    if (handle >> u32::BITS) as usize != bytes {
        return Err(invalid());
    }
    Ok(handle as u32 as usize)
}

/// The physical frame envelope plus its segment table. Saved payloads stay
/// opaque; a typed consumer checks them against the module's frame layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameView {
    pub layout: u32,
    pub code: u32,
    pub state: u32,
    pub status: u32,
    pub generation: u32,
    /// Saved binding handles in frame order; zero for an uninitialized shared cell.
    pub bindings: Vec<u64>,
    pub completion: Option<BindingView>,
}
impl FrameView {
    pub fn read_envelope(data: &[u8], frame: u32) -> Result<Self, ValueAbiError> {
        let ptr = frame as usize;
        block(data, ptr, FRAME_BYTES, FRAME_FLAG, FRAME_MASK as u32)?;
        if word(data, ptr + 4)? as usize != FRAME_FIELDS
            || wide(data, field(ptr, HANDLER_FIELD))? != 0
        {
            return Err(invalid());
        }
        let control = wide(data, field(ptr, CONTROL_FIELD))?;
        let status = wide(data, field(ptr, STATUS_FIELD))?;
        let status_word = status as u32;
        if !matches!(
            status_word,
            STATUS_SUSPENDED | STATUS_RUNNING | STATUS_COMPLETED
        ) {
            return Err(invalid());
        }
        let completion = match wide(data, field(ptr, COMPLETION_FIELD))? {
            0 => None,
            handle => Some(BindingView::read_envelope(data, handle)?),
        };
        Ok(Self {
            layout: word(data, ptr)?,
            code: control as u32,
            state: (control >> u32::BITS) as u32,
            status: status_word,
            generation: (status >> u32::BITS) as u32,
            bindings: match wide(data, field(ptr, ENVIRONMENT_FIELD))? {
                0 => Vec::new(),
                handle => segments(data, handle)?,
            },
            completion,
        })
    }
    pub fn token(&self, frame: u32) -> ResumeToken {
        ResumeToken {
            frame,
            generation: self.generation,
        }
    }
}
fn environment(data: &[u8], handle: u64, kind: i32) -> Result<Vec<u64>, ValueAbiError> {
    let bytes = (handle >> u32::BITS) as usize;
    let count = bytes
        .checked_sub(value::RECORD_META_BYTES)
        .filter(|n| n.is_multiple_of(value::RECORD_SLOT_BYTES))
        .map(|n| n / value::RECORD_SLOT_BYTES)
        .filter(|n| (1..=SEGMENT_BINDINGS).contains(n))
        .ok_or_else(invalid)?;
    let ptr = handle_ptr(handle, bytes)?;
    block(data, ptr, bytes, ENVIRONMENT_FLAG, value::field_mask(count))?;
    if word(data, ptr)? != kind as u32 || word(data, ptr + 4)? as usize != count {
        return Err(invalid());
    }
    (0..count as u32)
        .map(|i| wide(data, field(ptr, i)))
        .collect()
}
fn segments(data: &[u8], handle: u64) -> Result<Vec<u64>, ValueAbiError> {
    let table = environment(data, handle, FRAME_ENVIRONMENT_KIND)?;
    if table.len() > MAX_FRAME_SEGMENTS {
        return Err(invalid());
    }
    let mut bindings = Vec::new();
    for segment in table {
        for binding in environment(data, segment, FRAME_SEGMENT_KIND)? {
            if binding != 0 {
                BindingView::read_envelope(data, binding)?;
            }
            bindings.push(binding);
        }
    }
    Ok(bindings)
}
