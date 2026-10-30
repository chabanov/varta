//! V38 (`codegen_v2`): append a static number of evaluated slots without an
//! argument array (`xs.push(a, b)` with a fixed argument list).
//!
//! The caller keeps the target cell and every reference slot rooted, or proves
//! them reachable from unchanged bindings (`stable_operands`), so a growth
//! allocation that collects loses nothing. Growth is geometric from
//! [`MIN_CAPACITY`] slots; the new backing zeroes only its spare slots and
//! presence bitmap (the live slots and presence bytes are copied over), and a
//! fixed 128-byte bitmap is zeroed word by word, since `memory.fill` and
//! `memory.copy` are charged per byte. The backing layout is unchanged: power
//! of two capacities, zero spare slots and no presence bit at or beyond the
//! length.
use super::*;
use varta_common::value as values;

/// The largest argument list appended without an argument array.
pub(in crate::emit) const MAX_VALUES: usize = 4;
/// Capacity of a backing that grows from fewer slots, and of v38 literal
/// backings.
pub(in crate::emit) const MIN_CAPACITY: usize = 4;
const AGAIN: u32 = 0;
const DONE: u32 = 1;
const COUNT: MemArg = MemArg {
    offset: values::ARRAY_LENGTH_OFFSET,
    ..WORD
};
const BACKING: MemArg = MemArg {
    offset: values::VECTOR_STORAGE_OFFSET,
    ..ROOT
};
const SLOT_SHIFT: i32 = values::ARRAY_SLOT_BYTES.trailing_zeros() as i32;
const BIT_SHIFT: i32 = u8::BITS.trailing_zeros() as i32;

/// Locals after the target handle and the `count` slot parameters.
struct Locals {
    cell: u32,
    length: u32,
    total: u32,
    storage: u32,
    capacity: u32,
    dest: u32,
    bytes: u32,
    bitmap: u32,
    index: u32,
    handle: u32,
}
const TARGET: u32 = 0;
const I32_LOCALS: u32 = 9;

pub(super) fn function(
    count: u8,
    allocate: u32,
    sparse: bool,
    limits: varta_common::limits::DataLimits,
) -> Function {
    let values_count = u32::from(count);
    let first = values_count + 1;
    let l = Locals {
        cell: first,
        length: first + 1,
        total: first + 2,
        storage: first + 3,
        capacity: first + 4,
        dest: first + 5,
        bytes: first + 6,
        bitmap: first + 7,
        index: first + 8,
        handle: first + I32_LOCALS,
    };
    let sizes = sparse::Sizes::new(limits);
    let mut f = Function::new([(I32_LOCALS, ValType::I32), (1, ValType::I64)]);
    instructions(
        &mut f,
        [
            I::LocalGet(TARGET),
            I::I32WrapI64,
            I::LocalTee(l.cell),
            I::I32Load(COUNT),
            I::LocalTee(l.length),
            I::I32Const(i32::from(count)),
            I::I32Add,
            I::LocalTee(l.total),
            I::I32Const(limits.array_elements as i32),
            I::I32GtU,
            I::If(BlockType::Empty),
            I::Unreachable,
            I::End,
            I::LocalGet(l.cell),
            I::I64Load(BACKING),
            I::LocalTee(l.handle),
            I::I32WrapI64,
            I::LocalSet(l.storage),
            I::LocalGet(l.handle),
            I::I64Const(LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I32WrapI64,
            I::LocalSet(l.bytes),
        ],
    );
    if sparse {
        instructions(&mut f, sizes.capacity(&[I::LocalGet(l.bytes)]));
    } else {
        f.instruction(&I::LocalGet(l.bytes));
        instructions(
            &mut f,
            [
                I::I32Const(values::ARRAY_META_BYTES as i32),
                I::I32Sub,
                I::I32Const(SLOT_SHIFT),
                I::I32ShrU,
            ],
        );
    }
    instructions(
        &mut f,
        [
            I::LocalSet(l.capacity),
            I::LocalGet(l.total),
            I::LocalGet(l.capacity),
            I::I32GtU,
            I::If(BlockType::Empty),
            // Double, at least to the minimum capacity, until the total fits.
            I::LocalGet(l.capacity),
            I::I32Const(1),
            I::I32Shl,
            I::LocalTee(l.capacity),
            I::I32Const(MIN_CAPACITY as i32),
            I::LocalGet(l.capacity),
            I::I32Const(MIN_CAPACITY as i32),
            I::I32GtU,
            I::Select,
            I::LocalSet(l.capacity),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(l.capacity),
            I::LocalGet(l.total),
            I::I32GeU,
            I::BrIf(DONE),
            I::LocalGet(l.capacity),
            I::I32Const(1),
            I::I32Shl,
            I::LocalSet(l.capacity),
            I::Br(AGAIN),
            I::End,
            I::End,
        ],
    );
    if sparse {
        // The old bitmap, while `bytes` still holds the old backing size.
        backing_bitmap(&mut f, &l, sizes);
        f.instruction(&I::LocalSet(l.index));
        instructions(&mut f, sizes.backing_bytes(&[I::LocalGet(l.capacity)]));
    } else {
        instructions(
            &mut f,
            [
                I::LocalGet(l.capacity),
                I::I32Const(SLOT_SHIFT),
                I::I32Shl,
                I::I32Const(values::ARRAY_META_BYTES as i32),
                I::I32Add,
            ],
        );
    }
    instructions(
        &mut f,
        [
            I::LocalTee(l.bytes),
            I::Call(allocate),
            I::LocalSet(l.dest),
            // Spare slots after the appended ones, up to the bitmap.
            I::LocalGet(l.dest),
            I::I32Const(values::ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(l.total),
            I::I32Const(SLOT_SHIFT),
            I::I32Shl,
            I::I32Add,
            I::I32Const(0),
            I::LocalGet(l.capacity),
            I::LocalGet(l.total),
            I::I32Sub,
            I::I32Const(SLOT_SHIFT),
            I::I32Shl,
            I::MemoryFill(MEMORY),
        ],
    );
    if sparse {
        new_bitmap(&mut f, &l, sizes);
        f.instruction(&I::LocalSet(l.bitmap));
        zero_bitmap(&mut f, &l, sizes);
    }
    instructions(
        &mut f,
        [
            I::LocalGet(l.dest),
            I::LocalGet(l.cell),
            I::I32Load(WORD),
            I::I32Store(WORD),
            I::LocalGet(l.dest),
            I::LocalGet(l.length),
            I::I32Store(COUNT),
            I::LocalGet(l.dest),
            I::I32Const(HEADER as i32),
            I::I32Sub,
            I::I32Const(
                ALLOCATED
                    | values::HEAP_ARRAY_FLAG
                    | if sparse { values::HEAP_SPARSE_FLAG } else { 0 },
            ),
            I::I32Store(FLAG),
            I::LocalGet(l.dest),
            I::I32Const(HEADER as i32),
            I::I32Sub,
            I::LocalGet(l.storage),
            I::I32Const(HEADER as i32),
            I::I32Sub,
            I::I32Load(LINK),
            I::I32Store(LINK),
            I::LocalGet(l.dest),
            I::I32Const(values::ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(l.storage),
            I::I32Const(values::ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(l.length),
            I::I32Const(SLOT_SHIFT),
            I::I32Shl,
            I::MemoryCopy {
                src_mem: MEMORY,
                dst_mem: MEMORY,
            },
        ],
    );
    if sparse {
        // No presence bit is set at or beyond the length, so the bytes that
        // hold the first `length` bits carry the whole bitmap.
        instructions(&mut f, [I::LocalGet(l.bitmap), I::LocalGet(l.index)]);
        instructions(
            &mut f,
            [
                I::LocalGet(l.length),
                I::I32Const((u8::BITS - 1) as i32),
                I::I32Add,
                I::I32Const(BIT_SHIFT),
                I::I32ShrU,
                I::MemoryCopy {
                    src_mem: MEMORY,
                    dst_mem: MEMORY,
                },
            ],
        );
    }
    f.instruction(&I::LocalGet(l.cell));
    pack(&mut f, l.bytes, l.dest);
    instructions(
        &mut f,
        [
            I::I64Store(BACKING),
            I::LocalGet(l.dest),
            I::LocalSet(l.storage),
            I::End,
            // From here `storage` and `bytes` describe the current backing.
            // The appended slots, then their presence bits.
            I::LocalGet(l.storage),
            I::LocalGet(l.length),
            I::I32Const(SLOT_SHIFT),
            I::I32Shl,
            I::I32Add,
            I::LocalSet(l.dest),
        ],
    );
    for value in 0..values_count {
        instructions(
            &mut f,
            [
                I::LocalGet(l.dest),
                I::LocalGet(1 + value),
                I::I64Store(MemArg {
                    offset: (values::ARRAY_META_BYTES + value as usize * values::ARRAY_SLOT_BYTES)
                        as u64,
                    ..ROOT
                }),
            ],
        );
    }
    if sparse {
        backing_bitmap(&mut f, &l, sizes);
        f.instruction(&I::LocalSet(l.bitmap));
        for value in 0..values_count {
            instructions(
                &mut f,
                [
                    I::LocalGet(l.length),
                    I::I32Const(value as i32),
                    I::I32Add,
                    I::LocalSet(l.index),
                ],
            );
            sparse::set(&mut f, l.bitmap, l.index);
        }
    }
    instructions(
        &mut f,
        [
            I::LocalGet(l.storage),
            I::LocalGet(l.total),
            I::I32Store(COUNT),
            I::LocalGet(l.cell),
            I::LocalGet(l.total),
            I::I32Store(COUNT),
            I::LocalGet(l.total),
            I::End,
        ],
    );
    f
}

/// `[] -> [address]` of the presence bitmap of the backing at `storage` with
/// `bytes` packed bytes.
fn backing_bitmap(f: &mut Function, l: &Locals, sizes: sparse::Sizes) {
    instructions(f, [I::LocalGet(l.storage), I::LocalGet(l.bytes), I::I32Add]);
    instructions(f, sizes.bitmap_bytes(&[I::LocalGet(l.bytes)]));
    f.instruction(&I::I32Sub);
}

/// `[] -> [address]` of the presence bitmap of the new backing.
fn new_bitmap(f: &mut Function, l: &Locals, sizes: sparse::Sizes) {
    instructions(f, [I::LocalGet(l.dest), I::LocalGet(l.bytes), I::I32Add]);
    instructions(f, sizes.bitmap_bytes(&[I::LocalGet(l.bytes)]));
    f.instruction(&I::I32Sub);
}

/// Zero the new backing's bitmap: word stores for the fixed 128 bytes, and
/// `memory.fill` for the one-bit-per-slot bitmap of a large backing.
fn zero_bitmap(f: &mut Function, l: &Locals, sizes: sparse::Sizes) {
    let words = |f: &mut Function| {
        for word in 0..values::SPARSE_BITMAP_BYTES / size_of::<u64>() {
            instructions(
                f,
                [
                    I::LocalGet(l.bitmap),
                    I::I64Const(0),
                    I::I64Store(MemArg {
                        offset: (word * size_of::<u64>()) as u64,
                        align: 0,
                        memory_index: MEMORY,
                    }),
                ],
            );
        }
    };
    if !sizes.large {
        words(f);
        return;
    }
    instructions(
        f,
        [
            I::LocalGet(l.capacity),
            I::I32Const(values::MAX_ARRAY_ELEMENTS as i32),
            I::I32GtU,
            I::If(BlockType::Empty),
            I::LocalGet(l.bitmap),
            I::I32Const(0),
        ],
    );
    instructions(f, sizes.bitmap_bytes(&[I::LocalGet(l.bytes)]));
    instructions(f, [I::MemoryFill(MEMORY), I::Else]);
    words(f);
    f.instruction(&I::End);
}
