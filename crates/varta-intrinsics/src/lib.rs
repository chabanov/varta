#![no_std]
#![forbid(unsafe_code)]
//! Pure, capability-free Varta intrinsics with one implementation.
//!
//! The native runtime calls these functions for the host imports of earlier
//! profiles. Profile `checked-f64-utf16-v38` (`linked_intrinsics`) compiles the
//! same code to `wasm32` (`varta-intrinsics-wasm`) and the compiler links the
//! functions a program uses into its guest module, so every host computes the
//! same bits and a third-party host implements only effects.
//!
//! Nothing here performs I/O, reads ambient state or holds authority. Guest
//! memory is read only through [`Memory`], which bounds every access. The
//! value layout (cells, tags, heap header and limits) has one owner,
//! `varta-common`, whose `no_std` core this crate builds against.
extern crate alloc;

pub mod bigint;
pub mod error;
pub mod format;
pub mod json;
pub mod math;
pub mod number;
pub mod parse;
pub mod primitive;
pub mod radix;
pub mod string;
pub mod value;

pub use error::IntrinsicError;
pub use varta_common::memory::Memory;
