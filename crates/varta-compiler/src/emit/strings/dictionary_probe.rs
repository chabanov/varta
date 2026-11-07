//! Value18 dictionary tables (v38 `dictionary_hashing`): the probe loops over
//! a table vector built by `lower::dictionary_hashing`, reading and writing
//! its storage in place. A table is a `number[]` whose capacity equals its
//! power-of-two length; a hole is an empty slot, a present `+0` a deleted
//! entry (tombstone) and a present `p + 1` the live entry at position `p` of
//! the key vector, whose key is then present. Probing starts at the key's
//! FNV-1a hash (`value::dictionary_hash`) masked by the capacity and steps
//! linearly, at most once around the table. None of these helpers allocates,
//! so no handle moves or dies while they run.
//!
//! V38 `Map`/`Set` tables (`collection_hashing`) share the encoding and the
//! probe loops: string and BigInt keys use the dictionary helpers, and the
//! [`KeyHash`] helpers hash a number or identity key slot with the murmur3
//! `fmix32` finalizer and compare it by SameValueZero or identity.
use super::*;
use crate::ir::{CollectionProbe, KeyHash};
use varta_common::value::{
    ARRAY_LENGTH_OFFSET, ARRAY_META_BYTES, ARRAY_SLOT_BYTES, DICTIONARY_HASH_OFFSET,
    DICTIONARY_HASH_PRIME, VECTOR_STORAGE_OFFSET,
};

const UNIT: i32 = size_of::<u16>() as i32;
const SLOT_SHIFT: i32 = ARRAY_SLOT_BYTES.trailing_zeros() as i32;
const BIT_SHIFT: i32 = u8::BITS.trailing_zeros() as i32;
const BIT_MASK: i32 = u8::BITS as i32 - 1;
const SLOT: MemArg = MemArg {
    offset: 0,
    align: 3,
    memory_index: MEMORY,
};
const BYTE: MemArg = MemArg {
    offset: 0,
    align: 0,
    memory_index: MEMORY,
};
const NOT_FOUND: f64 = -1.0;
/// The murmur3 `fmix32` finalizer: shift, multiply, shift, multiply, shift.
const MIX_SHIFTS: [i32; 3] = [16, 13, 16];
const MIX_FACTORS: [u32; 2] = [0x85eb_ca6b, 0xc2b2_ae35];
/// The binary64 bits every NaN key slot hashes as.
const CANONICAL_NAN: u64 = 0x7ff8_0000_0000_0000;
const WORD_SHIFT: i64 = u32::BITS as i64;

/// How a lookup compares its keys: code-unit payloads by content through
/// the `StringEquals` helper at this index, other key slots by their
/// [`KeyHash`] class.
#[derive(Clone, Copy)]
enum Equality {
    Content(u32),
    Slot(KeyHash),
}
impl Equality {
    fn keys(self) -> KeyHash {
        match self {
            Self::Content(_) => KeyHash::Units,
            Self::Slot(hash) => hash,
        }
    }
}
/// What a probe does with the slot it finds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// `(keys, table, key)`: the position of the key's entry, or -1.
    Find,
    /// As [`Self::Find`], tombstoning the found slot.
    Unlink,
    /// `(keys, table, key, position)`: the position of the key's entry, or
    /// -1 after referencing `position` from the first empty slot of the
    /// probe. The caller keeps the table below half load, so an empty slot
    /// exists (a probe that finds none traps).
    Insert,
}

/// Locals of the probe helpers (after their parameters): the i64 key, then
/// i32 scratch.
struct Probe {
    key: u32,
    hash: u32,
    ptr: u32,
    len: u32,
    offset: u32,
    slots: u32,
    bits: u32,
    mask: u32,
    at: u32,
}
impl Probe {
    /// The i32 scratch locals from `first` on (`key` is a parameter or an
    /// i64 local).
    const fn from(key: u32, first: u32) -> Self {
        Self {
            key,
            hash: first,
            ptr: first + 1,
            len: first + 2,
            offset: first + 3,
            slots: first + 4,
            bits: first + 5,
            mask: first + 6,
            at: first + 7,
        }
    }
    const SCRATCH: u32 = 8;
}

/// `SLOTS`, `BITS` and `MASK` of the table vector in local `table`: the
/// capacity is the length, so the presence bitmap follows the slots.
fn open(f: &mut Function, table: u32, p: &Probe) {
    instructions(
        f,
        [
            I::LocalGet(table),
            I::I32WrapI64,
            I::LocalTee(p.ptr),
            I::I32Load(MemArg {
                offset: ARRAY_LENGTH_OFFSET,
                ..WORD
            }),
            I::I32Const(1),
            I::I32Sub,
            I::LocalSet(p.mask),
            I::LocalGet(p.ptr),
            I::I32Load(MemArg {
                offset: VECTOR_STORAGE_OFFSET,
                ..WORD
            }),
            I::I32Const(ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalTee(p.slots),
            I::LocalGet(p.mask),
            I::I32Const(1),
            I::I32Add,
            I::I32Const(SLOT_SHIFT),
            I::I32Shl,
            I::I32Add,
            I::LocalSet(p.bits),
        ],
    );
}
/// `AT = hash(KEY) & MASK`, the key's home slot.
fn home(f: &mut Function, p: &Probe, keys: KeyHash) {
    if keys == KeyHash::Units {
        units_home(f, p);
        return;
    }
    if keys == KeyHash::Number {
        // KEY becomes the canonical slot of its SameValueZero class: `+0`
        // for `-0` (x + 0) and one NaN for every NaN.
        instructions(
            f,
            [
                I::LocalGet(p.key),
                I::F64ReinterpretI64,
                I::LocalGet(p.key),
                I::F64ReinterpretI64,
                I::F64Ne,
                I::If(BlockType::Result(ValType::I64)),
                I::I64Const(CANONICAL_NAN as i64),
                I::Else,
                I::LocalGet(p.key),
                I::F64ReinterpretI64,
                I::F64Const(0.0.into()),
                I::F64Add,
                I::I64ReinterpretF64,
                I::End,
                I::LocalTee(p.key),
                I::I32WrapI64,
                I::LocalGet(p.key),
                I::I64Const(WORD_SHIFT),
                I::I64ShrU,
                I::I32WrapI64,
                I::I32Xor,
            ],
        );
    } else {
        instructions(f, [I::LocalGet(p.key), I::I32WrapI64]);
    }
    f.instruction(&I::LocalSet(p.hash));
    for (step, shift) in MIX_SHIFTS.iter().enumerate() {
        instructions(
            f,
            [
                I::LocalGet(p.hash),
                I::LocalGet(p.hash),
                I::I32Const(*shift),
                I::I32ShrU,
                I::I32Xor,
            ],
        );
        if let Some(factor) = MIX_FACTORS.get(step) {
            instructions(f, [I::I32Const(*factor as i32), I::I32Mul]);
        }
        f.instruction(&I::LocalSet(p.hash));
    }
    instructions(
        f,
        [
            I::LocalGet(p.hash),
            I::LocalGet(p.mask),
            I::I32And,
            I::LocalSet(p.at),
        ],
    );
}
/// `AT = fnv1a(KEY) & MASK` over the code units of a payload key.
fn units_home(f: &mut Function, p: &Probe) {
    instructions(
        f,
        [
            I::LocalGet(p.key),
            I::I32WrapI64,
            I::LocalSet(p.ptr),
            I::LocalGet(p.key),
            I::I64Const(LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I32WrapI64,
            I::LocalSet(p.len),
            I::I32Const(0),
            I::LocalSet(p.offset),
            I::I32Const(DICTIONARY_HASH_OFFSET as i32),
            I::LocalSet(p.hash),
            I::LocalGet(p.len),
            I::If(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(p.hash),
            I::LocalGet(p.ptr),
            I::LocalGet(p.offset),
            I::I32Add,
            I::I32Load16U(MemArg { align: 1, ..WORD }),
            I::I32Xor,
            I::I32Const(DICTIONARY_HASH_PRIME as i32),
            I::I32Mul,
            I::LocalSet(p.hash),
            I::LocalGet(p.offset),
            I::I32Const(UNIT),
            I::I32Add,
            I::LocalTee(p.offset),
            I::LocalGet(p.len),
            I::I32LtU,
            I::BrIf(0),
            I::End,
            I::End,
            I::LocalGet(p.hash),
            I::LocalGet(p.mask),
            I::I32And,
            I::LocalSet(p.at),
        ],
    );
}
/// Push whether slot `AT` is present (bit 0 of the shifted bitmap byte).
fn present(p: &Probe) -> [I<'static>; 12] {
    [
        I::LocalGet(p.bits),
        I::LocalGet(p.at),
        I::I32Const(BIT_SHIFT),
        I::I32ShrU,
        I::I32Add,
        I::I32Load8U(BYTE),
        I::LocalGet(p.at),
        I::I32Const(BIT_MASK),
        I::I32And,
        I::I32ShrU,
        I::I32Const(1),
        I::I32And,
    ]
}
/// Push the address of slot `AT`.
fn slot(p: &Probe) -> [I<'static>; 5] {
    [
        I::LocalGet(p.slots),
        I::LocalGet(p.at),
        I::I32Const(SLOT_SHIFT),
        I::I32Shl,
        I::I32Add,
    ]
}
/// `AT = (AT + 1) & MASK`.
fn step(p: &Probe) -> [I<'static>; 6] {
    [
        I::LocalGet(p.at),
        I::I32Const(1),
        I::I32Add,
        I::LocalGet(p.mask),
        I::I32And,
        I::LocalSet(p.at),
    ]
}
/// Advance `AT` to the first empty slot of the probe from `AT`, store the
/// reference `position + 1` (pushed by `reference`) there and mark it
/// present. The load bound keeps an empty slot.
fn store(f: &mut Function, p: &Probe, reference: &[I<'static>]) {
    instructions(f, [I::Block(BlockType::Empty), I::Loop(BlockType::Empty)]);
    instructions(f, present(p));
    instructions(f, [I::I32Eqz, I::BrIf(1)]);
    instructions(f, step(p));
    instructions(f, [I::Br(0), I::End, I::End]);
    instructions(f, slot(p));
    instructions(f, reference.iter().cloned());
    instructions(
        f,
        [
            I::F64Store(SLOT),
            // bits[AT >> 3] |= 1 << (AT & 7)
            I::LocalGet(p.bits),
            I::LocalGet(p.at),
            I::I32Const(BIT_SHIFT),
            I::I32ShrU,
            I::I32Add,
            I::LocalTee(p.ptr),
            I::LocalGet(p.ptr),
            I::I32Load8U(BYTE),
            I::I32Const(1),
            I::LocalGet(p.at),
            I::I32Const(BIT_MASK),
            I::I32And,
            I::I32Shl,
            I::I32Or,
            I::I32Store8(BYTE),
        ],
    );
}

/// `(keys, table, key) -> f64`: the position of `key`'s live entry, or -1.
/// With `unlink`, the entry's slot also becomes a tombstone.
pub(super) fn find(equals: u32, unlink: bool) -> Function {
    probe(
        Equality::Content(equals),
        if unlink { Mode::Unlink } else { Mode::Find },
    )
}
fn probe(equality: Equality, mode: Mode) -> Function {
    const KEYS: u32 = 0;
    const TABLE: u32 = 1;
    const POSITION: u32 = 3;
    let parameters = if mode == Mode::Insert { 4 } else { 3 };
    let p = Probe::from(2, parameters);
    let entries = parameters + Probe::SCRATCH;
    let steps = entries + 1;
    let reference = steps + 1;
    // A number key compares the entry key twice (SameValueZero).
    let entry = reference + 1;
    let floats = if equality.keys() == KeyHash::Number {
        2
    } else {
        1
    };
    let mut f = Function::new([(Probe::SCRATCH + 2, ValType::I32), (floats, ValType::F64)]);
    open(&mut f, TABLE, &p);
    home(&mut f, &p, equality.keys());
    instructions(
        &mut f,
        [
            I::LocalGet(KEYS),
            I::I32WrapI64,
            I::I32Load(MemArg {
                offset: VECTOR_STORAGE_OFFSET,
                ..WORD
            }),
            I::I32Const(ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalSet(entries),
            I::LocalGet(p.mask),
            I::I32Const(1),
            I::I32Add,
            I::LocalSet(steps),
            I::Loop(BlockType::Empty),
        ],
    );
    // An empty slot ends the probe; an insertion references its position
    // there.
    instructions(&mut f, present(&p));
    instructions(&mut f, [I::I32Eqz, I::If(BlockType::Empty)]);
    if mode == Mode::Insert {
        store(
            &mut f,
            &p,
            &[I::LocalGet(POSITION), I::F64Const(1.0.into()), I::F64Add],
        );
    }
    instructions(&mut f, [I::F64Const(NOT_FOUND.into()), I::Return, I::End]);
    instructions(&mut f, slot(&p));
    instructions(
        &mut f,
        [
            I::F64Load(SLOT),
            I::LocalTee(reference),
            I::F64Const(0.0.into()),
            I::F64Gt,
            I::If(BlockType::Empty),
            // A live reference names a present key.
            I::LocalGet(entries),
            I::LocalGet(reference),
            I::I32TruncF64U,
            I::I32Const(1),
            I::I32Sub,
            I::I32Const(SLOT_SHIFT),
            I::I32Shl,
            I::I32Add,
            I::I64Load(SLOT),
        ],
    );
    equal(&mut f, equality, p.key, entry);
    f.instruction(&I::If(BlockType::Empty));
    if mode == Mode::Unlink {
        instructions(&mut f, slot(&p));
        instructions(&mut f, [I::F64Const(0.0.into()), I::F64Store(SLOT)]);
    }
    instructions(
        &mut f,
        [
            I::LocalGet(reference),
            I::F64Const(1.0.into()),
            I::F64Sub,
            I::Return,
            I::End,
            I::End,
        ],
    );
    instructions(&mut f, step(&p));
    instructions(
        &mut f,
        [
            I::LocalGet(steps),
            I::I32Const(1),
            I::I32Sub,
            I::LocalTee(steps),
            I::BrIf(0),
            I::End,
        ],
    );
    // A full circle without an empty slot: a lookup misses, while an
    // insertion has broken the load bound.
    f.instruction(&if mode == Mode::Insert {
        I::Unreachable
    } else {
        I::F64Const(NOT_FOUND.into())
    });
    f.instruction(&I::End);
    f
}

/// Consume the entry key slot pushed by the probe; push whether it holds
/// the same key as local `key` (an f64 local `entry` is the scratch of a
/// number comparison).
fn equal(f: &mut Function, equality: Equality, key: u32, entry: u32) {
    if let Equality::Slot(KeyHash::Cell) = equality {
        instructions(f, cell_payload());
    }
    match equality {
        Equality::Content(equals) => {
            instructions(f, [I::LocalGet(key), I::Call(equals)]);
        }
        Equality::Slot(KeyHash::Identity | KeyHash::Units | KeyHash::Cell) => {
            instructions(f, [I::LocalGet(key), I::I64Eq]);
        }
        // SameValueZero: entry == key (+0 equals -0), or both are NaN, whose
        // bits may differ.
        Equality::Slot(KeyHash::Number) => instructions(
            f,
            [
                I::F64ReinterpretI64,
                I::LocalTee(entry),
                I::LocalGet(key),
                I::F64ReinterpretI64,
                I::F64Eq,
                I::LocalGet(entry),
                I::LocalGet(entry),
                I::F64Ne,
                I::LocalGet(key),
                I::F64ReinterpretI64,
                I::LocalGet(key),
                I::F64ReinterpretI64,
                I::F64Ne,
                I::I32And,
                I::I32Or,
            ],
        ),
    }
}

/// Replace the key vector entry on the stack, a union cell, by its payload:
/// the reference the cell holds ([`KeyHash::Cell`]).
fn cell_payload() -> [I<'static>; 2] {
    [I::I32WrapI64, I::I64Load(crate::emit::records::field(0))]
}

/// The `Map`/`Set` probe `probe` for keys hashed as `hash`
/// (`collection_hashing`); `equals` is the `StringEquals` helper, which
/// code-unit keys compare with.
pub(super) fn collection(probe: CollectionProbe, hash: KeyHash, equals: u32) -> Function {
    let equality = if hash == KeyHash::Units {
        Equality::Content(equals)
    } else {
        Equality::Slot(hash)
    };
    match probe {
        CollectionProbe::Find => self::probe(equality, Mode::Find),
        CollectionProbe::Unlink => self::probe(equality, Mode::Unlink),
        CollectionProbe::Insert => self::probe(equality, Mode::Insert),
        CollectionProbe::Index => indexed(hash),
    }
}

/// `(table, key, position) -> f64`: reference entry `position` from the
/// first empty slot of `key`'s probe and mark it present; returns 0.
pub(super) fn place() -> Function {
    const TABLE: u32 = 0;
    const POSITION: u32 = 2;
    let p = Probe::from(1, 3);
    let mut f = Function::new([(Probe::SCRATCH, ValType::I32)]);
    open(&mut f, TABLE, &p);
    home(&mut f, &p, KeyHash::Units);
    store(
        &mut f,
        &p,
        &[I::LocalGet(POSITION), I::F64Const(1.0.into()), I::F64Add],
    );
    instructions(&mut f, [I::F64Const(0.0.into()), I::End]);
    f
}

/// `(keys, table) -> f64`: reference every entry of the dense key vector
/// from a fresh table of empty slots; returns 0. Callers index only
/// dictionaries without holes (unindexed or just compacted).
pub(super) fn index() -> Function {
    indexed(KeyHash::Units)
}
fn indexed(keys: KeyHash) -> Function {
    const KEYS: u32 = 0;
    const TABLE: u32 = 1;
    const KEY: u32 = 2;
    let p = Probe::from(KEY, 3);
    let entries = 3 + Probe::SCRATCH;
    let count = entries + 1;
    let position = count + 1;
    // Locals: the key (2), then the probe scratch, entries, count, position.
    let mut f = Function::new([(1, ValType::I64), (Probe::SCRATCH + 3, ValType::I32)]);
    open(&mut f, TABLE, &p);
    instructions(
        &mut f,
        [
            I::LocalGet(KEYS),
            I::I32WrapI64,
            I::LocalTee(p.ptr),
            I::I32Load(MemArg {
                offset: ARRAY_LENGTH_OFFSET,
                ..WORD
            }),
            I::LocalSet(count),
            I::LocalGet(p.ptr),
            I::I32Load(MemArg {
                offset: VECTOR_STORAGE_OFFSET,
                ..WORD
            }),
            I::I32Const(ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalSet(entries),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(position),
            I::LocalGet(count),
            I::I32GeU,
            I::BrIf(1),
            I::LocalGet(entries),
            I::LocalGet(position),
            I::I32Const(SLOT_SHIFT),
            I::I32Shl,
            I::I32Add,
            I::I64Load(SLOT),
        ],
    );
    if keys == KeyHash::Cell {
        instructions(&mut f, cell_payload());
    }
    f.instruction(&I::LocalSet(KEY));
    home(&mut f, &p, keys);
    store(
        &mut f,
        &p,
        &[
            I::LocalGet(position),
            I::I32Const(1),
            I::I32Add,
            I::F64ConvertI32U,
        ],
    );
    instructions(
        &mut f,
        [
            I::LocalGet(position),
            I::I32Const(1),
            I::I32Add,
            I::LocalSet(position),
            I::Br(0),
            I::End,
            I::End,
            I::F64Const(0.0.into()),
            I::End,
        ],
    );
    f
}

/// `(keys, values, table, position) -> f64`: the ordered insertion of an
/// array-index key below the last key. The dense vectors were just extended
/// by the new entry; it moves to `position` and the entries from there shift
/// up by one (both bitmaps are already full over the new length), and every
/// table reference to a shifted entry follows it. The caller then places the
/// new entry. Returns 0.
pub(super) fn insert_at(barrier: Option<incremental::Barrier>) -> Function {
    const KEYS: u32 = 0;
    const VALUES: u32 = 1;
    const TABLE: u32 = 2;
    const POSITION: u32 = 3;
    const AT: u32 = 4;
    const SLOTS: u32 = 5;
    const LENGTH: u32 = 6;
    const END: u32 = 7;
    const LAST: u32 = 8;
    const REFERENCE: u32 = 9;
    let mut f = Function::new([(4, ValType::I32), (1, ValType::I64), (1, ValType::F64)]);
    // V38 (`incremental_collection`): the entries move within both vectors,
    // which are first traced whole.
    for vector in [KEYS, VALUES] {
        instructions(&mut f, incremental::backing_code(barrier, vector));
    }
    instructions(
        &mut f,
        [I::LocalGet(POSITION), I::I32TruncF64U, I::LocalSet(AT)],
    );
    for vector in [KEYS, VALUES] {
        instructions(
            &mut f,
            [
                I::LocalGet(vector),
                I::I32WrapI64,
                I::LocalTee(SLOTS),
                I::I32Load(MemArg {
                    offset: ARRAY_LENGTH_OFFSET,
                    ..WORD
                }),
                I::LocalSet(LENGTH),
                I::LocalGet(SLOTS),
                I::I32Load(MemArg {
                    offset: VECTOR_STORAGE_OFFSET,
                    ..WORD
                }),
                I::I32Const(ARRAY_META_BYTES as i32),
                I::I32Add,
                I::LocalTee(SLOTS),
                I::LocalGet(LENGTH),
                I::I32Const(1),
                I::I32Sub,
                I::I32Const(SLOT_SHIFT),
                I::I32Shl,
                I::I32Add,
                I::I64Load(SLOT),
                I::LocalSet(LAST),
                // memmove(slots + (at + 1) * 8, slots + at * 8, (length - 1 - at) * 8)
                I::LocalGet(SLOTS),
                I::LocalGet(AT),
                I::I32Const(1),
                I::I32Add,
                I::I32Const(SLOT_SHIFT),
                I::I32Shl,
                I::I32Add,
                I::LocalGet(SLOTS),
                I::LocalGet(AT),
                I::I32Const(SLOT_SHIFT),
                I::I32Shl,
                I::I32Add,
                I::LocalGet(LENGTH),
                I::I32Const(1),
                I::I32Sub,
                I::LocalGet(AT),
                I::I32Sub,
                I::I32Const(SLOT_SHIFT),
                I::I32Shl,
                I::MemoryCopy {
                    src_mem: MEMORY,
                    dst_mem: MEMORY,
                },
                I::LocalGet(SLOTS),
                I::LocalGet(AT),
                I::I32Const(SLOT_SHIFT),
                I::I32Shl,
                I::I32Add,
                I::LocalGet(LAST),
                I::I64Store(SLOT),
            ],
        );
    }
    // References p + 1 > position follow their entry; holes and tombstones
    // hold +0 and stay.
    instructions(
        &mut f,
        [
            I::LocalGet(TABLE),
            I::I64Eqz,
            I::If(BlockType::Empty),
            I::F64Const(0.0.into()),
            I::Return,
            I::End,
            I::LocalGet(TABLE),
            I::I32WrapI64,
            I::LocalTee(SLOTS),
            I::I32Load(MemArg {
                offset: ARRAY_LENGTH_OFFSET,
                ..WORD
            }),
            I::I32Const(SLOT_SHIFT),
            I::I32Shl,
            I::LocalGet(SLOTS),
            I::I32Load(MemArg {
                offset: VECTOR_STORAGE_OFFSET,
                ..WORD
            }),
            I::I32Const(ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalTee(SLOTS),
            I::I32Add,
            I::LocalSet(END),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(SLOTS),
            I::LocalGet(END),
            I::I32GeU,
            I::BrIf(1),
            I::LocalGet(SLOTS),
            I::F64Load(SLOT),
            I::LocalTee(REFERENCE),
            I::LocalGet(POSITION),
            I::F64Gt,
            I::If(BlockType::Empty),
            I::LocalGet(SLOTS),
            I::LocalGet(REFERENCE),
            I::F64Const(1.0.into()),
            I::F64Add,
            I::F64Store(SLOT),
            I::End,
            I::LocalGet(SLOTS),
            I::I32Const(ARRAY_SLOT_BYTES as i32),
            I::I32Add,
            I::LocalSet(SLOTS),
            I::Br(0),
            I::End,
            I::End,
            I::F64Const(0.0.into()),
            I::End,
        ],
    );
    f
}
