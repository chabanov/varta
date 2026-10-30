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
pub(super) fn function(utf16: bool, chunked: bool) -> Function {
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
    if chunked {
        // V38: skip the common prefix eight bytes at a time. Equal bytes are
        // equal code units in both encodings, so the unit loop below resumes
        // at a unit boundary of the first differing chunk.
        skip_equal_chunks(
            &mut function,
            [LEFT_PTR, RIGHT_PTR, LEFT_LEN, RIGHT_LEN],
            INDEX,
        );
    }
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

/// Advance `index` past whole equal eight-byte chunks of both inputs, staying
/// within the shorter one.
fn skip_equal_chunks(
    function: &mut Function,
    [left, right, left_len, right_len]: [u32; 4],
    index: u32,
) {
    const CHUNK: i32 = size_of::<u64>() as i32;
    const QUAD: MemArg = MemArg {
        offset: 0,
        align: 0,
        memory_index: MEMORY,
    };
    instructions(
        function,
        [
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(index),
            I::I32Const(CHUNK),
            I::I32Add,
            I::LocalGet(left_len),
            I::I32GtU,
            I::LocalGet(index),
            I::I32Const(CHUNK),
            I::I32Add,
            I::LocalGet(right_len),
            I::I32GtU,
            I::I32Or,
            I::BrIf(1),
            I::LocalGet(left),
            I::LocalGet(index),
            I::I32Add,
            I::I64Load(QUAD),
            I::LocalGet(right),
            I::LocalGet(index),
            I::I32Add,
            I::I64Load(QUAD),
            I::I64Ne,
            I::BrIf(1),
            I::LocalGet(index),
            I::I32Const(CHUNK),
            I::I32Add,
            I::LocalSet(index),
            I::Br(0),
            I::End,
            I::End,
        ],
    );
}

/// V38 content equality: identical handles, then lengths, then eight-byte
/// chunks with an overlapping final chunk; shorter inputs compare one
/// overlapping pair of four-, two- or one-byte words. Every load stays inside
/// both slices. Equal bytes are exactly equal code units in either encoding.
pub(super) fn equals(utf16: bool) -> Function {
    const LEFT: u32 = 0;
    const RIGHT: u32 = 1;
    const LEFT_PTR: u32 = 2;
    const RIGHT_PTR: u32 = 3;
    const LEN: u32 = 4;
    const RIGHT_LEN: u32 = 5;
    const INDEX: u32 = 6;
    const CHUNK: i32 = size_of::<u64>() as i32;
    const WORD_BYTES: i32 = size_of::<u32>() as i32;
    const HALF_BYTES: i32 = size_of::<u16>() as i32;
    let unaligned = |align| MemArg {
        offset: 0,
        align,
        memory_index: MEMORY,
    };
    let mut function = Function::new([(INDEX - LEFT_PTR + 1, ValType::I32)]);
    instructions(
        &mut function,
        [
            I::LocalGet(LEFT),
            I::LocalGet(RIGHT),
            I::I64Eq,
            I::If(BlockType::Empty),
            I::I32Const(1),
            I::Return,
            I::End,
        ],
    );
    length(&mut function, LEFT, LEN);
    length(&mut function, RIGHT, RIGHT_LEN);
    instructions(
        &mut function,
        [
            I::LocalGet(LEN),
            I::LocalGet(RIGHT_LEN),
            I::I32Ne,
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
    // Compare the words at byte `offset` of both inputs; return 0 on a
    // difference.
    let pair = |function: &mut Function,
                offset: u32,
                load: fn(MemArg) -> I<'static>,
                align: u32,
                wide: bool| {
        instructions(
            function,
            [
                I::LocalGet(LEFT_PTR),
                I::LocalGet(offset),
                I::I32Add,
                load(unaligned(align)),
                I::LocalGet(RIGHT_PTR),
                I::LocalGet(offset),
                I::I32Add,
                load(unaligned(align)),
                if wide { I::I64Ne } else { I::I32Ne },
                I::If(BlockType::Empty),
                I::I32Const(0),
                I::Return,
                I::End,
            ],
        );
    };
    // len >= 8: whole chunks, then the overlapping last chunk.
    instructions(
        &mut function,
        [
            I::LocalGet(LEN),
            I::I32Const(CHUNK),
            I::I32GeU,
            I::If(BlockType::Empty),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(INDEX),
            I::I32Const(CHUNK),
            I::I32Add,
            I::LocalGet(LEN),
            I::I32GtU,
            I::BrIf(1),
        ],
    );
    pair(&mut function, INDEX, I::I64Load, 0, true);
    instructions(
        &mut function,
        [
            I::LocalGet(INDEX),
            I::I32Const(CHUNK),
            I::I32Add,
            I::LocalSet(INDEX),
            I::Br(0),
            I::End,
            I::End,
            I::LocalGet(LEN),
            I::I32Const(CHUNK),
            I::I32Sub,
            I::LocalSet(INDEX),
        ],
    );
    pair(&mut function, INDEX, I::I64Load, 0, true);
    instructions(&mut function, [I::I32Const(1), I::Return, I::End]);
    // Shorter inputs: one overlapping pair of the widest word that fits.
    // UTF-16 lengths are even; the byte pair only serves UTF-8 inputs.
    type Load = fn(MemArg) -> I<'static>;
    let widths: [(i32, Load); 3] = [
        (WORD_BYTES, I::I32Load),
        (HALF_BYTES, I::I32Load16U),
        (1, I::I32Load8U),
    ];
    for (width, load) in widths.into_iter().take(if utf16 { 2 } else { 3 }) {
        instructions(
            &mut function,
            [
                I::LocalGet(LEN),
                I::I32Const(width),
                I::I32GeU,
                I::If(BlockType::Empty),
            ],
        );
        pair(&mut function, INDEX, load, 0, false);
        instructions(
            &mut function,
            [
                I::LocalGet(LEN),
                I::I32Const(width),
                I::I32Sub,
                I::LocalSet(INDEX),
            ],
        );
        pair(&mut function, INDEX, load, 0, false);
        instructions(&mut function, [I::I32Const(1), I::Return, I::End]);
    }
    // Both inputs are empty (or single bytes compared above).
    instructions(&mut function, [I::I32Const(1), I::End]);
    function
}
