//! These opaque objects do not yet participate in JSON/aggregate value tags.
pub const CELL_BYTES: usize = 16;
pub const HEAP_FLAG: i32 = 128;
pub const SIGNATURE_OFFSET: u64 = 0;
pub const CODE_OFFSET: u64 = 8;
pub const RESERVED_OFFSETS: [u64; 2] = [4, 12];
pub const MAX_TYPES: usize = 128;

/// V12 function bodies retain code at offset 8 and add an environment edge.
pub const CLOSURE_BYTES: usize = 24;
pub const ENVIRONMENT_OFFSET: u64 = 16;
pub const FUNCTION_FIELDS: i32 = 2;
pub const ENVIRONMENT_MASK: i32 = 2;
pub const ENVIRONMENT_FLAG: i32 = 256;
pub const BINDING_FLAG: i32 = 512;
pub const MAX_CAPTURES: usize = 32;
pub const INTERNAL_KIND: i32 = 0;

/// Compiler budgets for one parameter list or lexical declaration's patterns.
pub const MAX_BINDING_PATTERN_DEPTH: usize = 64;
pub const MAX_BINDING_PATTERN_NODES: usize = 1024;
/// Parameter lists retain their existing limits.
pub const MAX_PARAMETER_PATTERN_DEPTH: usize = MAX_BINDING_PATTERN_DEPTH;
pub const MAX_PARAMETER_PATTERN_NODES: usize = MAX_BINDING_PATTERN_NODES;

mod view;
pub use view::{BindingView, ClosureView, EnvironmentView};
mod catalog;
#[cfg(test)]
mod view_tests;
pub use catalog::{
    CATALOG_SECTION, CATALOG_VERSION, CaptureBinding, CaptureField, CaptureFunction, CaptureType,
    FrameLayout, FrameSlot, FrameState, FunctionCatalog, MAX_CAPTURE_TYPES,
};
pub mod frame;
#[cfg(test)]
mod frame_tests;
mod validate;
pub use validate::{CaptureValidation, VerifiedFunctionCatalog};
#[cfg(test)]
mod catalog_tests;
