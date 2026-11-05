#![forbid(unsafe_code)]
#![no_std]
//! The value layer of v38 KV storage (`kv`), shared by every host.
//!
//! The native runtime (`varta-runtime`, over `denokv_sqlite`) and the
//! official browser host (this crate compiled to wasm32 by
//! `scripts/build-kv-codec.py`, `varta-kv-codec-wasm`) run this one
//! implementation of everything a KV import does to values, so the two hosts
//! encode, decode, charge and allocate identically:
//!
//! - [`stage`]: the walk of a value18 guest value into the V8 value it stores
//!   as, through [`varta_common::memory::Memory`];
//! - [`v8`]: the strict V8 `ValueSerializer` subset Deno KV stores;
//! - [`key`]: Deno KV's key codec (the FoundationDB tuple subset);
//! - [`protocol`]: staging, the read and commit descriptors, cursors and
//!   versionstamp text, with Deno's limits;
//! - [`decode`]: the classification of a stored value or key into the
//!   module's checked schema;
//! - [`build`] and [`guest`]: the checked construction of the result in guest
//!   memory through the guest's own allocator, and the recoverable
//!   `TypeError` of a mismatch;
//! - [`charge`]: the v38 fair fuel of KV work.
//!
//! The storage engine stays with each host: Deno KV's SQLite files natively,
//! the Varta KV container in OPFS in the browser. A host supplies guest
//! memory, the guest allocator, the exception channel and the turn deadline
//! through the traits here; this crate performs no I/O and reads no clock.
extern crate alloc;
#[cfg(test)]
extern crate std;

pub mod build;
pub mod charge;
pub mod decode;
pub mod guest;
pub mod key;
pub mod protocol;
pub mod stage;
pub mod transport;
pub mod v8;

/// The wire facts of v38 KV storage: `varta_common::kv`, compiled here from
/// its one source file because `varta-common` builds it only with `std`.
#[path = "../../varta-common/src/kv.rs"]
pub mod wire;
// `wire`'s own tests name the ABI as `crate::abi`.
#[cfg(test)]
use varta_common::abi;

/// Whether `number` is integral (`f64::fract` is zero), without `std`.
#[must_use]
pub fn integral(number: f64) -> bool {
    number - libm::trunc(number) == 0.0
}

/// A host's turn deadline: polled between bounded steps of host work.
pub trait Deadline {
    /// Whether the deadline has passed.
    fn passed(&self) -> bool;
}

/// Work items between two deadline polls of a walk.
pub const DEADLINE_STRIDE: usize = 1024;
