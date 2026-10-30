//! Dense v9 array search with actual tagged-value equality. No allocation,
//! callbacks or host operations occur after arguments have been evaluated.
use super::*;
use varta_common::value as values;
const ARRAY: u32 = 0;
const KIND: u32 = 1;
const NEEDLE: u32 = 2;
const FROM: u32 = 3;
const SAME_VALUE_ZERO: u32 = 4;
const SOURCE: u32 = 5;
const LENGTH: u32 = 6;
const INDEX: u32 = 7;
const STORAGE: u32 = 8;
const ELEMENT: u32 = 9;
const CURRENT_KIND: u32 = 10;
const VALUE: u32 = 11;
const BITMAP: u32 = 12;
const AGAIN: u32 = 0;
const DONE: u32 = 1;
const NOT_FOUND: i32 = -1;
pub(super) fn function(
    compare: u32,
    sparse: bool,
    last: bool,
    bigint: bool,
    sizes: sparse::Sizes,
) -> Function {
    let mut f = Function::new([
        (6, ValType::I32),
        (1, ValType::I64),
        (if sparse { 1 } else { 0 }, ValType::I32),
    ]);
    instructions(
        &mut f,
        [
            I::LocalGet(ARRAY),
            I::I32WrapI64,
            I::LocalTee(SOURCE),
            I::I32Load(MemArg {
                offset: values::ARRAY_LENGTH_OFFSET,
                ..WORD
            }),
            I::LocalTee(LENGTH),
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::I32Const(NOT_FOUND),
            I::Return,
            I::End,
        ],
    );
    start_index(&mut f, last);
    instructions(
        &mut f,
        [
            I::LocalGet(SOURCE),
            I::I32Load(WORD),
            I::LocalSet(ELEMENT),
            I::LocalGet(SOURCE),
            I::I64Load(MemArg {
                offset: values::VECTOR_STORAGE_OFFSET,
                ..ROOT
            }),
            I::I32WrapI64,
            I::LocalSet(STORAGE),
        ],
    );
    if sparse {
        sparse::bitmap_of(&mut f, ARRAY, sizes);
        f.instruction(&I::LocalSet(BITMAP));
    }
    resolve(&mut f, KIND, NEEDLE);
    instructions(
        &mut f,
        [I::Block(BlockType::Empty), I::Loop(BlockType::Empty)],
    );
    if last {
        instructions(
            &mut f,
            [I::LocalGet(INDEX), I::I32Const(0), I::I32LtS, I::BrIf(DONE)],
        );
    } else {
        instructions(
            &mut f,
            [
                I::LocalGet(INDEX),
                I::LocalGet(LENGTH),
                I::I32GeU,
                I::BrIf(DONE),
            ],
        );
    }
    if sparse {
        sparse::get(&mut f, BITMAP, INDEX);
        f.instruction(&I::If(BlockType::Empty));
    }
    instructions(
        &mut f,
        [
            I::LocalGet(ELEMENT),
            I::LocalSet(CURRENT_KIND),
            I::LocalGet(STORAGE),
            I::I32Const(values::ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(INDEX),
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
            I::I64Load(ROOT),
            I::LocalSet(VALUE),
        ],
    );
    resolve(&mut f, CURRENT_KIND, VALUE);
    instructions(
        &mut f,
        [
            I::LocalGet(CURRENT_KIND),
            I::LocalGet(KIND),
            I::I32Eq,
            I::If(BlockType::Empty),
        ],
    );
    equal(&mut f, compare, bigint);
    instructions(
        &mut f,
        [
            I::If(BlockType::Empty),
            I::LocalGet(INDEX),
            I::Return,
            I::End,
            I::End,
        ],
    );
    if sparse {
        instructions(
            &mut f,
            [
                I::Else,
                I::LocalGet(SAME_VALUE_ZERO),
                I::LocalGet(KIND),
                I::I32Const(values::ValueKind::Undefined as i32),
                I::I32Eq,
                I::I32And,
                I::If(BlockType::Empty),
                I::LocalGet(INDEX),
                I::Return,
                I::End,
                I::End,
            ],
        );
    }
    instructions(
        &mut f,
        [
            I::LocalGet(INDEX),
            I::I32Const(1),
            if last { I::I32Sub } else { I::I32Add },
            I::LocalSet(INDEX),
            I::Br(AGAIN),
            I::End,
            I::End,
            I::I32Const(NOT_FOUND),
            I::End,
        ],
    );
    f
}
/// Normalize in floating point and reject/clamp before integer conversion.
/// Reverse search keeps negative relative indices negative until the miss check;
/// forward search retains its existing zero-clamping and upper-bound rejection.
fn start_index(f: &mut Function, last: bool) {
    instructions(
        f,
        [
            I::LocalGet(FROM),
            I::LocalGet(FROM),
            I::F64Ne,
            I::If(BlockType::Result(ValType::F64)),
            I::F64Const(0.0.into()),
            I::Else,
            I::LocalGet(FROM),
            I::F64Trunc,
            I::End,
            I::LocalSet(FROM),
        ],
    );
    if last {
        instructions(
            f,
            [
                I::LocalGet(FROM),
                I::F64Const(0.0.into()),
                I::F64Lt,
                I::If(BlockType::Result(ValType::F64)),
                I::LocalGet(FROM),
                I::LocalGet(LENGTH),
                I::F64ConvertI32U,
                I::F64Add,
                I::Else,
                I::LocalGet(FROM),
                I::LocalGet(LENGTH),
                I::I32Const(1),
                I::I32Sub,
                I::F64ConvertI32U,
                I::F64Min,
                I::End,
                I::LocalSet(FROM),
                I::LocalGet(FROM),
                I::F64Const(0.0.into()),
                I::F64Lt,
                I::If(BlockType::Empty),
                I::I32Const(NOT_FOUND),
                I::Return,
                I::End,
                I::LocalGet(FROM),
                I::I32TruncF64U,
                I::LocalSet(INDEX),
            ],
        );
    } else {
        instructions(
            f,
            [
                I::LocalGet(FROM),
                I::LocalGet(LENGTH),
                I::F64ConvertI32U,
                I::F64Ge,
                I::If(BlockType::Empty),
                I::I32Const(NOT_FOUND),
                I::Return,
                I::End,
                I::LocalGet(FROM),
                I::F64Const(0.0.into()),
                I::F64Lt,
                I::If(BlockType::Result(ValType::F64)),
                I::LocalGet(FROM),
                I::LocalGet(LENGTH),
                I::F64ConvertI32U,
                I::F64Add,
                I::F64Const(0.0.into()),
                I::F64Max,
                I::Else,
                I::LocalGet(FROM),
                I::End,
                I::I32TruncF64U,
                I::LocalSet(INDEX),
            ],
        );
    }
}
fn resolve(f: &mut Function, kind: u32, payload: u32) {
    instructions(
        f,
        [
            I::LocalGet(kind),
            I::I32Const(values::ValueKind::PrimitiveUnion as i32),
            I::I32Eq,
            I::LocalGet(kind),
            I::I32Const(values::ValueKind::ReferenceUnion as i32),
            I::I32Eq,
            I::I32Or,
            I::If(BlockType::Empty),
            I::LocalGet(payload),
            I::I32WrapI64,
            I::I32Load(WORD),
            I::LocalSet(kind),
            I::LocalGet(payload),
            I::I32WrapI64,
            I::I64Load(MemArg {
                offset: values::UNION_META_BYTES as u64,
                ..ROOT
            }),
            I::LocalSet(payload),
            I::End,
        ],
    );
}
fn equal(f: &mut Function, compare: u32, bigint: bool) {
    instructions(
        f,
        [
            I::LocalGet(KIND),
            I::I32Const(values::ValueKind::Float64 as i32),
            I::I32Eq,
            I::If(BlockType::Result(ValType::I32)),
            I::LocalGet(VALUE),
            I::F64ReinterpretI64,
            I::LocalGet(NEEDLE),
            I::F64ReinterpretI64,
            I::F64Eq,
            I::LocalGet(SAME_VALUE_ZERO),
            I::LocalGet(VALUE),
            I::F64ReinterpretI64,
            I::LocalGet(VALUE),
            I::F64ReinterpretI64,
            I::F64Ne,
            I::LocalGet(NEEDLE),
            I::F64ReinterpretI64,
            I::LocalGet(NEEDLE),
            I::F64ReinterpretI64,
            I::F64Ne,
            I::I32And,
            I::I32And,
            I::I32Or,
            I::Else,
            I::LocalGet(KIND),
            I::I32Const(values::ValueKind::String as i32),
            I::I32Eq,
        ],
    );
    if bigint {
        instructions(
            f,
            [
                I::LocalGet(KIND),
                I::I32Const(values::ValueKind::BigInt as i32),
                I::I32Eq,
                I::I32Or,
            ],
        );
    }
    instructions(
        f,
        [
            I::If(BlockType::Result(ValType::I32)),
            I::LocalGet(VALUE),
            I::LocalGet(NEEDLE),
            I::Call(compare),
            I::I32Eqz,
            I::Else,
            I::LocalGet(VALUE),
            I::LocalGet(NEEDLE),
            I::I64Eq,
            I::End,
            I::End,
        ],
    );
}
