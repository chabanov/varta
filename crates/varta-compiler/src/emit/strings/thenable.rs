//! Allocation-free own-property inspection for unsupported Promise resolution.
//! Inputs are compiler-owned canonical unknown cells, not native pointers. The
//! caller separately checks the closed class method table (methods aren't slots).
use super::*;
use varta_common::value as v;

const INPUT: u32 = 0;
const PTR: u32 = 1;
const KIND: u32 = 2;
const DESCRIPTOR: u32 = 3;
const COUNT: u32 = 4;
const INDEX: u32 = 5;
const SLOT: u32 = 6;
const VALUES: u32 = 7;
const KEYS: u32 = 8;
const PAYLOAD: u32 = 9;
const WORD_BYTES: i32 = size_of::<u32>() as i32;
const THEN_UTF8: i32 = i32::from_le_bytes(*b"then");
const THEN_UTF16: i64 = i64::from_le_bytes(*b"t\0h\0e\0n\0");
const THEN_UNITS_BYTES: i64 = 4 * size_of::<u16>() as i64;

/// Return whether the payload in PAYLOAD of physical KIND is callable. Union
/// cells are flattened by lowering; primitive cells can never hold functions.
fn callable(f: &mut Function) {
    instructions(
        f,
        [
            I::LocalGet(KIND),
            I::I32Const(v::ValueKind::Function as i32),
            I::I32Eq,
            I::If(BlockType::Empty),
            I::I32Const(1),
            I::Return,
            I::End,
            I::LocalGet(KIND),
            I::I32Const(v::ValueKind::ReferenceUnion as i32),
            I::I32Eq,
            I::If(BlockType::Empty),
            I::LocalGet(PAYLOAD),
            I::I32WrapI64,
            I::I32Load(WORD),
            I::I32Const(v::ValueKind::Function as i32),
            I::I32Eq,
            I::Return,
            I::End,
            I::I32Const(0),
            I::Return,
        ],
    );
}

pub(super) fn function(profile: varta_common::language::LanguageProfile) -> Function {
    let mut f = Function::new([(KEYS, ValType::I32), (1, ValType::I64)]);
    instructions(
        &mut f,
        [
            I::LocalGet(INPUT),
            I::I32WrapI64,
            I::LocalTee(PTR),
            I::I32Load(WORD),
            I::LocalSet(KIND),
            I::LocalGet(PTR),
            I::I64Load(MemArg {
                offset: v::UNION_META_BYTES as u64,
                ..ROOT
            }),
            I::I32WrapI64,
            I::LocalSet(PTR),
            I::LocalGet(KIND),
            I::I32Const(v::ValueKind::Record as i32),
            I::I32Eq,
            I::If(BlockType::Empty),
            I::LocalGet(PTR),
            I::I32Load(WORD),
            I::LocalSet(DESCRIPTOR),
            I::LocalGet(PTR),
            I::I32Load(MemArg {
                offset: size_of::<u32>() as u64,
                ..WORD
            }),
            I::LocalSet(COUNT),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(INDEX),
            I::LocalGet(COUNT),
            I::I32GeU,
            I::BrIf(1),
            I::LocalGet(DESCRIPTOR),
            I::I32Const(v::RECORD_DESCRIPTOR_HEADER as i32),
            I::I32Add,
            I::LocalGet(INDEX),
            I::I32Const(v::RECORD_DESCRIPTOR_ENTRY as i32),
            I::I32Mul,
            I::I32Add,
            I::LocalTee(SLOT),
            I::I32Load(MemArg {
                offset: size_of::<u32>() as u64,
                ..WORD
            }),
            I::I32Const(size_of::<u32>() as i32),
            I::I32Eq,
            I::If(BlockType::Empty),
            I::LocalGet(SLOT),
            I::I32Load(WORD),
            I::I32Load(WORD),
            I::I32Const(THEN_UTF8),
            I::I32Eq,
            I::If(BlockType::Empty),
            I::LocalGet(SLOT),
            I::I32Load(MemArg {
                offset: 3 * size_of::<u32>() as u64,
                ..WORD
            }),
            I::LocalSet(KIND),
            I::LocalGet(SLOT),
            I::I32Load(MemArg {
                offset: 2 * size_of::<u32>() as u64,
                ..WORD
            }),
            I::I32Const(!(v::OPTIONAL_FIELD_FLAG as i32)),
            I::I32And,
            I::LocalSet(SLOT),
            // Presence bitmap follows the values, for both narrow and wide records.
            I::LocalGet(PTR),
            I::I32Const(v::RECORD_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(COUNT),
            I::I32Const(v::RECORD_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
            I::LocalGet(SLOT),
            I::I32Const(v::PRESENCE_WORD_BITS as i32),
            I::I32DivU,
            I::I32Const(WORD_BYTES),
            I::I32Mul,
            I::I32Add,
            I::I32Load(WORD),
            I::I32Const(1),
            I::LocalGet(SLOT),
            I::I32Shl,
            I::I32And,
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::I32Const(0),
            I::Return,
            I::End,
            I::LocalGet(PTR),
            I::I32Const(v::RECORD_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(SLOT),
            I::I32Const(v::RECORD_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
            I::I64Load(ROOT),
            I::LocalSet(PAYLOAD),
        ],
    );
    callable(&mut f);
    instructions(
        &mut f,
        [
            I::End,
            I::End,
            I::LocalGet(INDEX),
            I::I32Const(1),
            I::I32Add,
            I::LocalSet(INDEX),
            I::Br(0),
            I::End,
            I::End,
            I::I32Const(0),
            I::Return,
            I::End,
        ],
    );
    let (keys, values) = if profile.dictionary_hashing() {
        (
            v::HASHED_DICTIONARY_KEYS_SLOT,
            v::HASHED_DICTIONARY_VALUES_SLOT,
        )
    } else {
        (0, 1)
    };
    instructions(
        &mut f,
        [
            I::LocalGet(KIND),
            I::I32Const(v::ValueKind::Dictionary as i32),
            I::I32Eq,
            I::If(BlockType::Empty),
            I::LocalGet(PTR),
            I::I64Load(MemArg {
                offset: (v::RECORD_META_BYTES + keys * v::RECORD_SLOT_BYTES) as u64,
                ..ROOT
            }),
            I::I32WrapI64,
            I::LocalSet(DESCRIPTOR),
            I::LocalGet(PTR),
            I::I64Load(MemArg {
                offset: (v::RECORD_META_BYTES + values * v::RECORD_SLOT_BYTES) as u64,
                ..ROOT
            }),
            I::I32WrapI64,
            I::LocalTee(VALUES),
            I::I32Load(WORD),
            I::LocalSet(KIND),
            I::LocalGet(VALUES),
            I::I32Load(MemArg {
                offset: v::VECTOR_STORAGE_OFFSET,
                ..WORD
            }),
            I::LocalSet(VALUES),
            I::LocalGet(DESCRIPTOR),
            I::I32Load(MemArg {
                offset: size_of::<u32>() as u64,
                ..WORD
            }),
            I::LocalSet(COUNT),
            I::LocalGet(DESCRIPTOR),
            I::I32Load(MemArg {
                offset: v::VECTOR_STORAGE_OFFSET,
                ..WORD
            }),
            I::LocalSet(KEYS),
            I::I32Const(0),
            I::LocalSet(INDEX),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(INDEX),
            I::LocalGet(COUNT),
            I::I32GeU,
            I::BrIf(1),
            I::LocalGet(KEYS),
            I::I32Const(v::ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(INDEX),
            I::I32Const(v::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
            I::I64Load(ROOT),
            I::LocalTee(PAYLOAD),
            I::I64Const(LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I64Const(THEN_UNITS_BYTES),
            I::I64Eq,
            I::If(BlockType::Empty),
            I::LocalGet(PAYLOAD),
            I::I32WrapI64,
            I::I64Load(ROOT),
            I::I64Const(THEN_UTF16),
            I::I64Eq,
            I::If(BlockType::Empty),
            I::LocalGet(VALUES),
            I::I32Const(v::ARRAY_META_BYTES as i32),
            I::I32Add,
            I::LocalGet(INDEX),
            I::I32Const(v::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
            I::I64Load(ROOT),
            I::LocalSet(PAYLOAD),
        ],
    );
    callable(&mut f);
    instructions(
        &mut f,
        [
            I::End,
            I::End,
            I::LocalGet(INDEX),
            I::I32Const(1),
            I::I32Add,
            I::LocalSet(INDEX),
            I::Br(0),
            I::End,
            I::End,
            I::End,
            I::I32Const(0),
            I::End,
        ],
    );
    f
}
