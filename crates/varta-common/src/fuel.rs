//! Fair host fuel (profile v38, `LanguageProfile::fair_fuel`): the fixed,
//! published rates every host charges a v38 module's fuel for the work of
//! its host imports, so a budget means the same on every host and a host
//! import is no longer free work for the guest.
//!
//! A host import of a v38 module costs [`CALL`] when it is called (charged
//! before it runs), plus [`BYTE`] per byte of every string argument it reads
//! (charged before its effect) and of every string result it writes, in the
//! result buffer or as an owned string (charged before the result is
//! published to the guest). Checked JSON adds its decoding and encoding
//! work: [`JSON_PARSE_UNIT`] per input code unit, [`JSON_PARSE_VALUE`] per
//! decoded value, [`JSON_SCHEMA_TYPE`] per schema type and
//! [`JSON_SCHEMA_FIELD`] per record field or union member checked (charged
//! before the first guest allocation), [`JSON_STRINGIFY_VALUE`] per value
//! visited and [`JSON_STRINGIFY_UNIT`] per output code unit (charged before
//! the result is published). Fuel the import's guest callbacks consume (the
//! value allocator, the retention helper) is metered as guest fuel. A charge
//! the remaining fuel cannot cover sets the fuel to zero and ends the turn
//! with the ordinary typed fuel-budget error. Profiles before v38 charge
//! nothing for host work.
//!
//! Calibration (`docs/development-plan.md`, fair host fuel): [`CALL`] is the
//! guest fuel of the cheapest linked intrinsic call (an argument check, the
//! call and its return); [`BYTE`] is Wasmtime's `memory.copy` rate, the
//! guest cost of moving a byte; the JSON rates are fitted to the fuel the
//! linked decoder and encoder spend beyond the host import on the measured
//! workloads (input units, values, schema types) and rounded up, so a host
//! import never costs less than its linked equivalent for the same work.

/// One host import call.
pub const CALL: u64 = 50;
/// One byte of a string argument read or a string result written.
pub const BYTE: u64 = 1;
/// One UTF-16 code unit of checked JSON input decoded.
pub const JSON_PARSE_UNIT: u64 = 30;
/// One checked JSON value decoded and built.
pub const JSON_PARSE_VALUE: u64 = 1500;
/// One schema type read and checked (with its static descriptor).
pub const JSON_SCHEMA_TYPE: u64 = 3000;
/// One record field or union member of the schema read and checked (with
/// its descriptor entry).
pub const JSON_SCHEMA_FIELD: u64 = 3000;
/// One value visited by checked JSON serialization or by the rendering of a
/// logged value: the largest per-value fuel of the linked encoder beyond
/// this charge on the measured shapes (a logged promise, about 5,000),
/// rounded up, so the host import never costs less than v38's linked
/// serialization.
pub const JSON_STRINGIFY_VALUE: u64 = 5500;
/// One UTF-16 code unit of serialized JSON output.
pub const JSON_STRINGIFY_UNIT: u64 = 30;
/// One KV storage operation (v38 `kv`): opening a database, one snapshot
/// read or one atomic commit (its SQLite statements and transaction; the
/// durable sync of a commit is bounded by the turn deadline, not by fuel).
pub const KV_OPERATION: u64 = 100_000;
/// One byte of a KV key or value: encoded and staged, written by a commit or
/// returned by a read.
pub const KV_BYTE: u64 = 10;
/// One entry a KV read returns (its decode pays the checked JSON rates).
pub const KV_ENTRY: u64 = 2_000;

/// The fair fuel of `units` of work at `rate`, saturating: a charge that
/// cannot be represented cannot be paid either.
#[must_use]
pub const fn work(units: usize, rate: u64) -> u64 {
    (units as u64).saturating_mul(rate)
}
