#![cfg_attr(target_arch = "wasm32", no_std)]
//! Browser host policy shim, with private memory and no host imports.
//! Unsafe is confined to the wasm ABI buffer and allocator; all validation is
//! safe Rust in varta-common::session. Native builds have no implementation.
#[cfg(target_arch = "wasm32")]
mod wasm;
