//! Nonmoving immutable strings with exact compiler roots and a bounded reusable heap.
mod allocate;
mod collect;
mod compare;
mod concat;
mod roots;
use std::collections::BTreeMap;
use varta_common::abi;
use wasm_encoder::{BlockType, Function, Instruction as I, MemArg, ValType};

pub(super) const LENGTH_SHIFT: u32 = i32::BITS;
pub(super) const MEMORY: u32 = 0;
const ALIGN: u32 = 8;
const HEADER: u32 = abi::STRING_HEADER_BYTES as u32;
const FLAGS: u64 = 4;
const NEXT: u64 = 8;
const ALLOCATED: i32 = 1;
const MARKED: i32 = 2;
const WORD: MemArg = MemArg {
    offset: 0,
    align: 2,
    memory_index: MEMORY,
};
const FLAG: MemArg = MemArg {
    offset: FLAGS,
    ..WORD
};
const LINK: MemArg = MemArg {
    offset: NEXT,
    ..WORD
};
pub(super) const ROOT: MemArg = MemArg {
    offset: 0,
    align: 3,
    memory_index: MEMORY,
};
#[derive(Clone, Copy)]
pub(super) struct Globals {
    pub heap: u32,
    pub free: u32,
    pub roots: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Helper {
    Snapshot,
    Concat,
    Compare,
    Allocate,
    Collect,
    RootPush,
}
impl Helper {
    pub(super) fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        match self {
            Self::Snapshot => (vec![ValType::I32; 2], vec![ValType::I64]),
            Self::Concat => (vec![ValType::I64; 2], vec![ValType::I64]),
            Self::Compare => (vec![ValType::I64; 2], vec![ValType::I32]),
            Self::Allocate => (vec![ValType::I32], vec![ValType::I32]),
            Self::Collect => (vec![], vec![]),
            Self::RootPush => (vec![ValType::I64], vec![ValType::I64]),
        }
    }
    pub(super) fn needs_arena(self) -> bool {
        self != Self::Compare
    }
    pub(super) fn body(self, globals: Globals, helpers: &BTreeMap<Self, u32>) -> Function {
        // Emission adds the complete allocator dependency set before indexing.
        match self {
            Self::Snapshot => snapshot(helpers[&Self::Allocate]),
            Self::Concat => concat::function(helpers[&Self::Allocate]),
            Self::Compare => compare::function(),
            Self::Allocate => allocate::function(globals, helpers[&Self::Collect]),
            Self::Collect => collect::function(globals),
            Self::RootPush => roots::function(globals.roots),
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
fn pack(function: &mut Function, len: u32, dest: u32) {
    instructions(
        function,
        [
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
fn snapshot(allocate: u32) -> Function {
    const INPUT_PTR: u32 = 0;
    const INPUT_LEN: u32 = 1;
    const DEST: u32 = 2;
    let mut function = Function::new([(1, ValType::I32)]);
    instructions(
        &mut function,
        [
            I::LocalGet(INPUT_LEN),
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::I64Const(0),
            I::Return,
            I::End,
            I::LocalGet(INPUT_LEN),
            I::Call(allocate),
            I::LocalTee(DEST),
            I::LocalGet(INPUT_PTR),
            I::LocalGet(INPUT_LEN),
            I::MemoryCopy {
                src_mem: MEMORY,
                dst_mem: MEMORY,
            },
        ],
    );
    pack(&mut function, INPUT_LEN, DEST);
    function.instruction(&I::End);
    function
}
