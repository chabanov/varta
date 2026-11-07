#![cfg_attr(target_arch = "wasm32", no_std)]
//! The browser host's view style engine (`varta_style::session`).
//!
//! Built only for `wasm32` by `scripts/build-view-engine.py`; on native
//! targets the crate is empty. The browser view model and DOM applier
//! instantiate this module with its own memory; each exported call
//! marshals its arguments and runs `varta-style`, the code the native
//! renderer runs, so both hosts accept, refuse and compute every style
//! input identically.
//!
//! This crate may use `unsafe` only for its heap (`heap`), the exported
//! symbols with their input and output buffers, the single session cell,
//! the one import (the bridge's font measurements, `fonts`) and its answer
//! cell, and the panic handler, as the KV codec shim does. All semantics live in the
//! safe `varta-style` crate.
//!
//! Calling convention: `view_engine_input(len)` returns the address of a
//! fresh `len`-byte input buffer that the host fills with UTF-8 text before
//! a call that reads it (`class`, `style`, `sheet`); a call that produces
//! bytes (`computed`) leaves them at `view_engine_output()` for
//! `view_engine_output_length()` bytes until the next call that produces
//! output.
#[cfg(target_arch = "wasm32")]
extern crate alloc;
#[cfg(target_arch = "wasm32")]
mod exports;
#[cfg(target_arch = "wasm32")]
mod fonts;
#[cfg(target_arch = "wasm32")]
mod heap;

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}
