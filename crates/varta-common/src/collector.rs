//! Collection between and within turns (v38 `incremental_collection`, wave 3
//! (f)).
//!
//! A v38 module with a managed heap exports [`IDLE_EXPORT`], `(i32 units) ->
//! i32 status`, which the runtime calls only between the turns of a
//! long-lived application instance, when no task is runnable, under the
//! instance's separate maintenance budget (`budget::MAINTENANCE_FUEL`,
//! never a turn's fuel). The guest decides from its own heap state alone
//! whether a collection is due: a cycle starts when the bytes allocated since
//! the last completed cycle reach [`IDLE_MIN_BYTES`] and the live bytes it
//! left shifted right by [`IDLE_LIVE_SHIFT`]; a cycle in progress continues.
//! One call performs at most `units` work units ([`IDLE_STEP_UNITS`] from the
//! reference runtime) and returns [`IDLE_NONE`] (nothing to do),
//! [`IDLE_MORE`] (work done, the cycle continues) or [`IDLE_DONE`] (work done,
//! the cycle completed). The export calls no host import, allocates nothing
//! through the host and changes no host status; its effect is reclaimed heap
//! memory only, so the same allocation history always gives the same answers.
//!
//! Inside turns the collector is incremental: a cycle marks from a snapshot
//! of the exact roots with a deletion (snapshot-at-the-beginning) write
//! barrier on every reference store and sweeps lazily, each allocation
//! performing work proportional to its size ([`STEP_MIN_UNITS`] plus the
//! request's bytes shifted right by [`STEP_BYTE_SHIFT`]); a large object is
//! traced [`TRACE_CHUNK_SLOTS`] slots at a time. A request that the memory
//! budget cannot satisfy still runs one complete collection first, so an
//! allocation fails exactly where it did before.

/// The idle-collection export of a v38 module with a managed heap.
pub const IDLE_EXPORT: &str = "varta.collector.idle";
/// [`IDLE_EXPORT`] status: no collection work was due.
pub const IDLE_NONE: i32 = 0;
/// [`IDLE_EXPORT`] status: work was done and the cycle continues.
pub const IDLE_MORE: i32 = 1;
/// [`IDLE_EXPORT`] status: work was done and the cycle completed.
pub const IDLE_DONE: i32 = 2;
/// Work units the reference runtime grants one [`IDLE_EXPORT`] call.
pub const IDLE_STEP_UNITS: u32 = 2048;
/// Bytes allocated since the last completed cycle below which an idle
/// period never starts one.
pub const IDLE_MIN_BYTES: u32 = 64 * 1024;
/// An idle period starts a cycle once the bytes allocated since the last
/// completed cycle reach the live bytes it left, shifted right by this.
pub const IDLE_LIVE_SHIFT: u32 = 2;
/// Work units every allocation performs while a cycle is in progress.
pub const STEP_MIN_UNITS: u32 = 8;
/// An allocation of `n` bytes performs `n >> STEP_BYTE_SHIFT` further units.
pub const STEP_BYTE_SHIFT: u32 = 4;
/// An in-turn cycle starts only once the managed heap holds at least this
/// many bytes; a smaller heap collects completely at once, as before (its
/// pause is short, and a cycle's floating garbage would grow it).
pub const INCREMENTAL_MIN_HEAP_BYTES: u32 = 64 * 1024;
/// An in-turn cycle starts only while the heap end stays below the memory
/// maximum divided by this; above it a request past the threshold collects
/// completely at once, as before, so a heap near the budget is compacted at
/// its end and never fragmented by a cycle's metadata and floating garbage.
pub const INCREMENTAL_HEAP_DIVISOR: u32 = 2;
/// A request of at least the managed heap's bytes shifted right by this
/// that crosses the threshold collects completely instead of starting a
/// cycle: its own initialization costs work in proportion, and its block then
/// goes where the garbage was instead of past a cycle's floating garbage.
pub const LARGE_REQUEST_SHIFT: u32 = 3;
/// Reference slots of one object traced per work quantum; a larger array is
/// traced in chunks.
pub const TRACE_CHUNK_SLOTS: u32 = 256;
/// Entries of the smallest incremental mark stack (block header addresses);
/// a full stack falls back to exact rescans and never loses a mark.
pub const MIN_MARK_STACK_ENTRIES: u32 = 1024;
/// Heap bytes per mark-stack entry when a cycle sizes its stack.
pub const MARK_STACK_BYTES_PER_ENTRY: u32 = 512;
