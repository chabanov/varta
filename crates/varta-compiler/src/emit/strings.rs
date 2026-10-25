//! Immutable UTF-8 slices, bounded arena allocation and pure string operations.
mod compare;
mod concat;
use varta_common::abi;
use wasm_encoder::{BlockType, Function, Instruction as I, ValType};

pub(super) const LENGTH_SHIFT: u32 = i32::BITS;
const MEMORY: u32 = 0;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Helper {
    Snapshot,
    Concat,
    Compare,
}
impl Helper {
    pub(super) fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        match self {
            Self::Snapshot => (vec![ValType::I32; 2], vec![ValType::I64]),
            Self::Concat => (vec![ValType::I64; 2], vec![ValType::I64]),
            Self::Compare => (vec![ValType::I64; 2], vec![ValType::I32]),
        }
    }
    pub(super) fn needs_arena(self) -> bool {
        self != Self::Compare
    }
    pub(super) fn body(self, arena: u32) -> Function {
        match self {
            Self::Snapshot => snapshot(arena),
            Self::Concat => concat::function(arena),
            Self::Compare => compare::function(),
        }
    }
}

fn instructions(function: &mut Function, code: impl IntoIterator<Item = I<'static>>) {
    for instruction in code {
        function.instruction(&instruction);
    }
}
fn length(function: &mut Function, input: u32, output: u32) {
    instructions(
        function,
        [
            I::LocalGet(input),
            I::I64Const(LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I32WrapI64,
            I::LocalSet(output),
        ],
    );
}

/// Reserve space without publishing the cursor; callers copy before commit.
fn reserve(function: &mut Function, arena: u32, len: u32, dest: u32, end: u32) {
    instructions(
        function,
        [
            I::LocalGet(len),
            I::I32Const(abi::MAX_STRING_BYTES as i32),
            I::I32GtU,
            I::If(BlockType::Empty),
            I::Unreachable,
            I::End,
            I::GlobalGet(arena),
            I::LocalTee(dest),
            I::LocalGet(len),
            I::I32Add,
            I::LocalSet(end),
            // The private cursor is at most 8 MiB and a value at most 1 MiB,
            // so this addition cannot wrap. Check the arena before growing memory.
            I::LocalGet(end),
            I::I32Const(abi::MAX_MEMORY_BYTES as i32),
            I::I32GtU,
            I::If(BlockType::Empty),
            I::Unreachable,
            I::End,
            I::LocalGet(end),
            I::MemorySize(MEMORY),
            I::I32Const(abi::WASM_PAGE_BYTES as i32),
            I::I32Mul,
            I::I32GtU,
            I::If(BlockType::Empty),
            I::LocalGet(end),
            I::I32Const((abi::WASM_PAGE_BYTES - 1) as i32),
            I::I32Add,
            I::I32Const(abi::WASM_PAGE_BYTES as i32),
            I::I32DivU,
            I::MemorySize(MEMORY),
            I::I32Sub,
            I::MemoryGrow(MEMORY),
            I::I32Const(-1),
            I::I32Eq,
            I::If(BlockType::Empty),
            I::Unreachable,
            I::End,
            I::End,
        ],
    );
}
fn commit(function: &mut Function, arena: u32, len: u32, dest: u32, end: u32) {
    instructions(
        function,
        [
            I::LocalGet(end),
            I::GlobalSet(arena),
            I::LocalGet(dest),
            I::I64ExtendI32U,
            I::LocalGet(len),
            I::I64ExtendI32U,
            I::I64Const(LENGTH_SHIFT as i64),
            I::I64Shl,
            I::I64Or,
        ],
    );
}

fn snapshot(arena: u32) -> Function {
    const INPUT_PTR: u32 = 0;
    const INPUT_LEN: u32 = 1;
    const DEST: u32 = 2;
    const END: u32 = 3;
    let mut function = Function::new([(END - DEST + 1, ValType::I32)]);
    instructions(
        &mut function,
        [
            I::LocalGet(INPUT_LEN),
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::I64Const(0),
            I::Return,
            I::End,
        ],
    );
    reserve(&mut function, arena, INPUT_LEN, DEST, END);
    instructions(
        &mut function,
        [
            I::LocalGet(DEST),
            I::LocalGet(INPUT_PTR),
            I::LocalGet(INPUT_LEN),
            I::MemoryCopy {
                src_mem: MEMORY,
                dst_mem: MEMORY,
            },
        ],
    );
    commit(&mut function, arena, INPUT_LEN, DEST, END);
    function.instruction(&I::End);
    function
}
