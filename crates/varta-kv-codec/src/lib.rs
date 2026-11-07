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

/// The deepest a KV value nests: the root is at level 0 and every element,
/// property or entry one level below its aggregate. The stage walk
/// ([`stage::Stager`]), the V8 encoder and decoder ([`v8::encode`],
/// [`v8::decode`]) and the classifier ([`decode::Classifier`]) refuse a
/// value with anything below this level with the recoverable `TypeError`
/// [`too_deep`], on every host; the construction ([`build`]) is iterative.
/// An object reached more than once (written once, then back-referenced)
/// counts at its deepest reach, as the classifier, which expands it, sees
/// it: the depth limit never refuses on a `get` what a `set` stored.
///
/// Derivation. Each of those walks recurses once per level, and in a
/// browser a wasm call frame lives on the engine's native stack, which a
/// module cannot size (the codec's linear-memory stack holds only its
/// address-taken locals). Measured on 2026-11-05 with a shallow guest and a
/// chain of records: Chrome 156 trapped the guest ("Maximum call stack size
/// exceeded") from about 750 levels (the decode of a `get`), WebKit 26.5
/// from about 170 (the stage walk of a `set`), and an unoptimized native
/// build overflowed its 8 MiB main thread at 1500 (the process aborted).
/// The limit is the legacy checked-JSON depth
/// ([`varta_common::value::MAX_JSON_DEPTH`]), under half of the smallest
/// measured bound, so at least half of the engine stack stays the guest's.
pub const MAX_VALUE_DEPTH: usize = varta_common::value::MAX_JSON_DEPTH;

/// The recoverable `TypeError` text of a value nesting deeper than
/// [`MAX_VALUE_DEPTH`], staged, encoded or decoded.
#[must_use]
pub fn too_deep() -> alloc::string::String {
    alloc::format!("a KV value cannot nest deeper than {MAX_VALUE_DEPTH} levels")
}

/// The most values a KV value expands to ([`wire::MAX_VALUE_NODES`]).
pub const MAX_VALUE_NODES: usize = wire::MAX_VALUE_NODES;
/// The most code units of text a KV value expands to
/// ([`wire::MAX_VALUE_UNITS`]).
pub const MAX_VALUE_UNITS: usize = wire::MAX_VALUE_UNITS;

/// The recoverable `TypeError` text of a value whose [`Extent`] exceeds
/// [`MAX_VALUE_NODES`] or [`MAX_VALUE_UNITS`], encoded or decoded.
#[must_use]
pub fn too_expanded() -> alloc::string::String {
    alloc::format!(
        "a KV value cannot expand to more than {MAX_VALUE_NODES} values or {MAX_VALUE_UNITS} code units of text"
    )
}

/// What a KV value expands to when a read builds it: its stored values and
/// its code units of text, a part shared within the value (stored once,
/// then back-referenced) counted again at every reach.
///
/// Sharing makes a small stored value expand exponentially (`x = { a: x,
/// b: x }` repeated), and a read builds every reach. The bounds stop only
/// that: a value without shared parts that fits [`wire::MAX_VALUE_BYTES`]
/// is always within them (their derivation in `varta_common::kv`), so
/// sharing never makes a read build more than the largest unshared value.
///
/// The bounds are kept by the V8 codec, the one walk every stored value
/// passes on every host:
/// - on `set`, the encoder ([`v8::encode`]) counts the whole value after
///   Deno's encoded-size check, whose message wins for a value both refuse
///   (it never expands a shared part, so it need not stop early), and the
///   staged value fails with the recoverable `TypeError` [`too_expanded`]
///   when it is used as a stored value; a key is never counted (Deno's own
///   key-size limits name it);
/// - on `get`, the decoder ([`v8::decode`]) refuses a stored value (by Deno
///   or an earlier build) by name as soon as it passes the bounds, before
///   the classifier builds anything.
///
/// Each aggregate's extent below it is recorded with its height and added
/// again at every later reach, in constant time.
///
/// What counts, alike on both walks: every value (an element, a property's
/// value, a `Map` key or value, a `Set` value; not a hole), every code unit
/// of a string and of a property key of a plain object (an array-index key
/// as its decimal digits). The `undefined` a read builds for an absent
/// optional field is no stored value and does not count: it costs what it
/// costs for an unshared value, its fair fuel (the classifier pays it), so
/// the result of a `get` depends on the stored value only, never on the
/// type it is read at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Extent {
    /// Values, each reach of a shared part counted.
    pub values: usize,
    /// Code units of text, each reach of a shared part counted.
    pub units: usize,
}

impl Extent {
    /// Add `values` and `units`.
    pub fn add(&mut self, values: usize, units: usize) {
        self.values = self.values.saturating_add(values);
        self.units = self.units.saturating_add(units);
    }

    /// Whether the value is within both bounds.
    #[must_use]
    pub fn within(self) -> bool {
        self.values <= MAX_VALUE_NODES && self.units <= MAX_VALUE_UNITS
    }

    /// Add `values` and `units`: false once the value is beyond either
    /// bound.
    #[must_use]
    pub fn grow(&mut self, values: usize, units: usize) -> bool {
        self.add(values, units);
        self.within()
    }

    /// The extent added since `start` (an earlier state of the same walk,
    /// which only grows).
    #[must_use]
    pub fn since(self, start: Self) -> Self {
        Self {
            values: self.values - start.values,
            units: self.units - start.units,
        }
    }
}

/// The code units of an array-index property key as decimal text.
#[must_use]
pub fn index_units(index: u32) -> usize {
    index
        .checked_ilog10()
        .map_or(1, |digits| digits as usize + 1)
}

#[cfg(test)]
mod build_tests;
