#![cfg_attr(target_arch = "wasm32", no_std)]
//! The browser host's KV value layer (`varta_common::kv::codec`).
//!
//! Built only for `wasm32` by `scripts/build-kv-codec.py`; on native targets
//! the crate is empty. The Worker instantiates this module beside the guest,
//! with its own memory: it never shares the guest's linear memory or stack.
//! Every export runs `varta-kv-codec`, the code the native runtime runs:
//! staging (the value18 walk and the V8 serializer), Deno's key codec, the
//! read and commit descriptors, cursors and versionstamps, the fair fuel,
//! and the decode of an entry into the module's schema, built through the
//! guest's own allocator. Guest memory, the guest helpers, the exception
//! channel, the deadline and the fuel tank are reached only through the
//! `varta_kv_host` imports (`host`).
//!
//! This is the second crate allowed `unsafe`, as `varta-intrinsics-wasm`:
//! only the extern block of host imports, the exported symbols, the arena
//! allocator and the panic handler. All semantics live in the safe
//! `varta-kv-codec` crate.
//!
//! Memory model: one arena, reset when the Worker starts a call
//! (`kv_input`); a call's input and output live in it until the next call.
//! Nothing else survives a call, except the module bounds `kv_configure`
//! sets once.
#[cfg(target_arch = "wasm32")]
extern crate alloc;
#[cfg(target_arch = "wasm32")]
mod arena;
#[cfg(target_arch = "wasm32")]
mod exports;
#[cfg(target_arch = "wasm32")]
mod host;

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}
