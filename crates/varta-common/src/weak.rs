//! Weak references (v38 `weak_references`, K9 D5): the ephemeron tables of
//! `WeakMap`, `WeakSet`, `WeakRef` and `FinalizationRegistry` in the managed
//! heap, and the checkpoint export through which the host ends a job.
//!
//! An ephemeron table is a heap block with [`HEAP_WEAK_FLAG`] and no edge
//! flag, so ordinary tracing marks it but never traces its
//! slots. Its header link word chains every live table of the instance into
//! the collector's weak list. The payload starts with four words
//! ([`CAPACITY_WORD`], [`KIND_WORD`], [`USED_WORD`], [`CLEARED_WORD`]) and
//! holds `capacity` pairs of two value slots from [`PAIRS_OFFSET`]: a key
//! slot holding the identity address of the key's heap block (or
//! [`KEY_EMPTY`]/[`KEY_VACATED`]) and a value slot.
//!
//! When marking has traced every strong edge, the collector iterates the
//! marked tables to a fixed point: the value of a pair whose key is marked
//! is marked and traced (an ephemeron, ECMA-262 §9.10.3), and every key of a
//! [`KIND_STRONG`] table is (the kept objects of the current job). Then it
//! vacates every pair whose key stayed unmarked, counting it in the
//! table's [`CLEARED_WORD`] and raising the module's pending flag for a
//! [`KIND_NOTIFY`] table (a `FinalizationRegistry`'s targets), and unlinks
//! every unmarked table, before any block is swept.
//!
//! The host calls [`CHECKPOINT_EXPORT`] whenever a microtask checkpoint has
//! drained its queue: the guest clears the kept objects (ClearKeptObjects)
//! and, when a collection vacated a registered target since the last call,
//! queues the cleanup job of the live registries as a new job of the same
//! checkpoint, which the host then drains.

/// Value18 (v38 `weak_references`): the heap flag of an ephemeron table, an
/// additive flag of value layout 18 next to the other heap flags of
/// [`crate::value`] (kept here, outside the intrinsics' fingerprinted core,
/// since no linked or browser reader ever meets one). Tables have no edge
/// flag, so ordinary tracing marks one without tracing its slots; no value
/// tag refers to one and no host value reader sees one.
pub const HEAP_WEAK_FLAG: i32 = 1 << 14;
/// The export the host calls at the end of every microtask checkpoint of a
/// module that uses weak references: `() -> ()`.
pub const CHECKPOINT_EXPORT: &str = "varta.weak.checkpoint";
/// Payload word: the table's capacity in pairs.
pub const CAPACITY_WORD: u32 = 0;
/// Payload word: the table's kind bits ([`KIND_VALUES`], [`KIND_STRONG`],
/// [`KIND_NOTIFY`]).
pub const KIND_WORD: u32 = 4;
/// Payload word: pairs whose key slot is not [`KEY_EMPTY`] (occupied or
/// vacated), maintained by the guest's probes.
pub const USED_WORD: u32 = 8;
/// Payload word: pairs the collector vacated since the table was created.
pub const CLEARED_WORD: u32 = 12;
/// Byte offset of the first pair in the payload.
pub const PAIRS_OFFSET: u32 = 16;
/// Bytes of one pair: the key slot and the value slot.
pub const PAIR_BYTES: u32 = 16;
/// Byte offset of a pair's value slot.
pub const VALUE_OFFSET: u32 = 8;
/// A key slot that never held a key.
pub const KEY_EMPTY: u64 = 0;
/// A key slot whose pair was deleted or cleared by the collector.
pub const KEY_VACATED: u64 = 1;
/// Kind bit: value slots are references, ephemeron edges of their keys.
pub const KIND_VALUES: u32 = 1;
/// Kind bit: keys are strong references (the kept objects).
pub const KIND_STRONG: u32 = 2;
/// Kind bit: a vacated key raises the module's pending-finalization flag.
pub const KIND_NOTIFY: u32 = 4;
/// The fewest pairs of a table.
pub const MIN_PAIRS: u32 = 8;
