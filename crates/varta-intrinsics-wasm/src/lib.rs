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
//! Memory model: this code runs on its own private linear memory (merged as
//! the guest module's second memory, never exported). It reaches guest
//! memory 0 only through the `varta_link` accessors, which bound every copy,
//! and it keeps no state between calls: the allocator is an arena reset on
//! every entry, and a nested entry traps.
#[cfg(target_arch = "wasm32")]
mod arena;
#[cfg(target_arch = "wasm32")]
mod exports;
#[cfg(target_arch = "wasm32")]
mod guest;
