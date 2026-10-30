#![cfg_attr(target_arch = "wasm32", no_std)]
//! The v38 guest intrinsic library (`linked_intrinsics`).
//!
//! Built only for `wasm32` by `scripts/build-intrinsics.py`; on native targets
//! the crate is empty. Each export carries the exact name and signature of
//! the host operation it replaces, and the compiler links only the exports a
//! program imports into its module. This is the one crate allowed `unsafe`:
//! the extern block of linker-provided accessors, the exported symbols, the
//! allocator and the panic handler. All semantics live in the safe
//! `varta-intrinsics` crate.
//!
//! Memory model: a linked module has one memory. The compiler's linker
//! relocates this library's shadow stack and static data into the module's
//! intrinsic region of memory 0 (`abi::StaticRegion::intrinsic_base`) and
//! traps when the stack pointer would leave it; the per-call arena lives in
//! the free memory above the guest heap. Guest values are reached only
//! through the `varta_link` accessors, which bound every copy, and nothing
//! survives a call: the arena is reset on every entry, and a nested entry
//! traps. The linked JSON decoder (`json`, linked only under the compiler's
//! test switch) instead takes its scratch from guest heap blocks rooted for
//! the call and builds its result through the guest's allocator.
#[cfg(target_arch = "wasm32")]
extern crate alloc;
#[cfg(target_arch = "wasm32")]
mod arena;
#[cfg(target_arch = "wasm32")]
mod bigint;
#[cfg(target_arch = "wasm32")]
mod exports;
#[cfg(target_arch = "wasm32")]
mod guest;
#[cfg(target_arch = "wasm32")]
mod json;
