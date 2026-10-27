//! Compare valid UTF-8 slices in lexicographic UTF-16 code-unit order.
use super::*;
use wasm_encoder::MemArg;

// At the first differing UTF-8 byte, normal unsigned byte order matches UTF-16
// except BMP U+E000..FFFF (EE/EF leads), which sort AFTER supplementary scalars.
// Move those leads above F0..F4. Continuation bytes are <= BF and unchanged.
const HIGH_BMP_LEAD: i32 = 0xee;
const HIGH_BMP_LEAD_RANGE: i32 = 1;
const ORDER_BIAS: i32 = 0x10;
fn order_byte(function: &mut Function, byte: u32) {
    instructions(
        function,
        [
            I::LocalGet(byte),
            I::I32Const(HIGH_BMP_LEAD),
            I::I32Sub,
            I::I32Const(HIGH_BMP_LEAD_RANGE),
            I::I32LeU,
            I::If(BlockType::Empty),
            I::LocalGet(byte),
            I::I32Const(ORDER_BIAS),
            I::I32Add,
            I::LocalSet(byte),
            I::End,
        ],
    );
}
pub(super) fn function(utf16: bool) -> Function {
    const LEFT: u32 = 0;
    const RIGHT: u32 = 1;
    const LEFT_PTR: u32 = 2;
    const RIGHT_PTR: u32 = 3;
    const LEFT_LEN: u32 = 4;
    const RIGHT_LEN: u32 = 5;
    const INDEX: u32 = 6;
    const LEFT_BYTE: u32 = 7;
    const RIGHT_BYTE: u32 = 8;
    const REPEAT_DEPTH: u32 = 0;
    const DONE_DEPTH: u32 = 1;
    const BYTE: MemArg = MemArg {
        offset: 0,
        align: 0,
        memory_index: MEMORY,
    };
    let mut function = Function::new([(RIGHT_BYTE - LEFT_PTR + 1, ValType::I32)]);
    instructions(
        &mut function,
        [
            I::LocalGet(LEFT),
            I::LocalGet(RIGHT),
            I::I64Eq,
            I::If(BlockType::Empty),
            I::I32Const(0),
            I::Return,
            I::End,
            I::LocalGet(LEFT),
            I::I32WrapI64,
            I::LocalSet(LEFT_PTR),
            I::LocalGet(RIGHT),
            I::I32WrapI64,
            I::LocalSet(RIGHT_PTR),
        ],
    );
    length(&mut function, LEFT, LEFT_LEN);
    length(&mut function, RIGHT, RIGHT_LEN);
    instructions(
        &mut function,
        [
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(INDEX),
            I::LocalGet(LEFT_LEN),
            I::I32GeU,
            I::LocalGet(INDEX),
            I::LocalGet(RIGHT_LEN),
            I::I32GeU,
            I::I32Or,
            I::BrIf(DONE_DEPTH),
            I::LocalGet(LEFT_PTR),
            I::LocalGet(INDEX),
            I::I32Add,
            if utf16 {
                I::I32Load16U(BYTE)
            } else {
                I::I32Load8U(BYTE)
            },
            I::LocalSet(LEFT_BYTE),
            I::LocalGet(RIGHT_PTR),
            I::LocalGet(INDEX),
            I::I32Add,
            if utf16 {
                I::I32Load16U(BYTE)
            } else {
                I::I32Load8U(BYTE)
            },
            I::LocalSet(RIGHT_BYTE),
        ],
    );
    if !utf16 {
        order_byte(&mut function, LEFT_BYTE);
        order_byte(&mut function, RIGHT_BYTE);
    }
    instructions(
        &mut function,
        [
            I::LocalGet(LEFT_BYTE),
            I::LocalGet(RIGHT_BYTE),
            I::I32LtU,
            I::If(BlockType::Empty),
            I::I32Const(-1),
            I::Return,
            I::End,
            I::LocalGet(LEFT_BYTE),
            I::LocalGet(RIGHT_BYTE),
            I::I32GtU,
            I::If(BlockType::Empty),
            I::I32Const(1),
            I::Return,
            I::End,
            I::LocalGet(INDEX),
            I::I32Const(if utf16 { 2 } else { 1 }),
            I::I32Add,
            I::LocalSet(INDEX),
            I::Br(REPEAT_DEPTH),
            I::End,
            I::End,
            I::LocalGet(LEFT_LEN),
            I::LocalGet(RIGHT_LEN),
            I::I32LtU,
            I::If(BlockType::Result(ValType::I32)),
            I::I32Const(-1),
            I::Else,
            I::LocalGet(LEFT_LEN),
            I::LocalGet(RIGHT_LEN),
            I::I32GtU,
            I::End,
            I::End,
        ],
    );
    function
}
