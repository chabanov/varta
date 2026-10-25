//! Internal string values pack an immutable UTF-8 slice into i64: len:ptr.
//! Host results are copied before another host operation can reuse its buffer.
use varta_common::abi;
use wasm_encoder::{BlockType, Function, Instruction as I, ValType};

pub(super) const LENGTH_SHIFT: u32 = i32::BITS;
const MEMORY: u32 = 0;
const INPUT_PTR: u32 = 0;
const INPUT_LEN: u32 = 1;
const DEST: u32 = 2;
const END: u32 = 3;

pub(super) fn snapshot(arena: u32) -> Function {
    let mut function = Function::new([(2, ValType::I32)]);
    let instructions = [
        // Empty/failed results need neither allocation nor access to pointer 0.
        I::LocalGet(INPUT_LEN),
        I::I32Eqz,
        I::If(BlockType::Empty),
        I::I64Const(0),
        I::Return,
        I::End,
        I::LocalGet(INPUT_LEN),
        I::I32Const(abi::MAX_RESULT_BYTES as i32),
        I::I32GtU,
        I::If(BlockType::Empty),
        I::Unreachable,
        I::End,
        I::GlobalGet(arena),
        I::LocalTee(DEST),
        I::LocalGet(INPUT_LEN),
        I::I32Add,
        I::LocalSet(END),
        // The cursor starts below 8 MiB and each result is at most 1 MiB;
        // addition cannot wrap. Exhaustion traps before publishing a new value.
        I::LocalGet(END),
        I::I32Const(abi::MAX_MEMORY_BYTES as i32),
        I::I32GtU,
        I::If(BlockType::Empty),
        I::Unreachable,
        I::End,
        I::LocalGet(END),
        I::MemorySize(MEMORY),
        I::I32Const(abi::WASM_PAGE_BYTES as i32),
        I::I32Mul,
        I::I32GtU,
        I::If(BlockType::Empty),
        I::LocalGet(END),
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
        I::LocalGet(DEST),
        I::LocalGet(INPUT_PTR),
        I::LocalGet(INPUT_LEN),
        I::MemoryCopy {
            src_mem: MEMORY,
            dst_mem: MEMORY,
        },
        I::LocalGet(END),
        I::GlobalSet(arena),
        I::LocalGet(DEST),
        I::I64ExtendI32U,
        I::LocalGet(INPUT_LEN),
        I::I64ExtendI32U,
        I::I64Const(LENGTH_SHIFT as i64),
        I::I64Shl,
        I::I64Or,
        I::End,
    ];
    for instruction in &instructions {
        function.instruction(instruction);
    }
    function
}
