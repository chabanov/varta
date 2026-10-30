//! V38 (`codegen_v2`, `linked_intrinsics`) value18 readers of reference
//! cells: an inline answer from the cell's tag in front of the linked reader
//! (`value_typeof_v11`, `value_truthy_v11`, `value_nullish_v11`,
//! `value_is_array_v11`, `value_strict_equal_v11`, `value_nullish_equal_v11`,
//! `value_is_error_class_v7`).
//!
//! The guest builds its cells itself, so a cell of a value18 kind answers
//! from its tag and payload exactly as the linked reader answers for it
//! after validating it (`typeof` is a static spelling, so it allocates
//! nothing). A tag that is no value18 reference-cell kind, a BigInt whose
//! value is needed (truthiness, `===`) and an unknown error class selector go
//! to the linked reader, which validates the cell (and its children, the
//! dictionary hash table included) and traps on a noncanonical one.
use super::primitives::{FastPaths, Spelling, among, is, read};
use super::*;
use varta_common::value::{ERROR_CLASS_SHIFT, ErrorClass, ValueKind as K};

/// Value18 reference-cell kinds (`ReferenceBox::read_in` of value layout
/// 18): every kind but `I32` and the union cell kinds.
const KINDS: [K; 12] = [
    K::Null,
    K::Boolean,
    K::String,
    K::Record,
    K::Array,
    K::Float64,
    K::Undefined,
    K::Error,
    K::BigInt,
    K::Function,
    K::Dictionary,
    K::Promise,
];
/// The kinds whose `typeof` is `"object"`.
const OBJECTS: [K; 6] = [
    K::Null,
    K::Error,
    K::Record,
    K::Array,
    K::Dictionary,
    K::Promise,
];
/// The always truthy kinds.
const OBJECT_VALUES: [K; 6] = [
    K::Error,
    K::Record,
    K::Array,
    K::Dictionary,
    K::Function,
    K::Promise,
];
/// Leave with the linked reader's answer unless `tag` is a value18 kind.
fn checked(function: &mut Function, tag: u32, fallback: &[I<'static>]) {
    instructions(function, among(tag, &KINDS));
    instructions(function, [I::I32Eqz, I::If(BlockType::Empty)]);
    instructions(function, fallback.iter().cloned());
    instructions(function, [I::Return, I::End]);
}

/// `(cell: i64) -> string`: `typeof`.
pub(super) fn type_of(snapshot: u32, linked: FastPaths) -> Function {
    const CELL: u32 = 0;
    const TAG: u32 = 1;
    const PAYLOAD: u32 = 2;
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I64)]);
    let host = linked.type_of.unwrap_or(0);
    read(&mut f, CELL, TAG, PAYLOAD);
    checked(
        &mut f,
        TAG,
        &[
            I::LocalGet(CELL),
            I::Call(host),
            I::GlobalGet(linked.result_len),
            I::Call(snapshot),
        ],
    );
    instructions(&mut f, among(TAG, &OBJECTS));
    instructions(
        &mut f,
        [
            I::If(BlockType::Empty),
            I::I64Const(linked.spelling(Spelling::Object)),
            I::Return,
            I::End,
        ],
    );
    for (kind, spelling) in [
        (K::Undefined, Spelling::Undefined),
        (K::String, Spelling::String),
        (K::Float64, Spelling::Number),
        (K::Boolean, Spelling::Boolean),
        (K::BigInt, Spelling::BigInt),
    ] {
        instructions(&mut f, is(TAG, kind));
        instructions(
            &mut f,
            [
                I::If(BlockType::Empty),
                I::I64Const(linked.spelling(spelling)),
                I::Return,
                I::End,
            ],
        );
    }
    // The last kind left is a function.
    instructions(
        &mut f,
        [I::I64Const(linked.spelling(Spelling::Function)), I::End],
    );
    f
}

/// `(cell: i64) -> i32`: `ToBoolean`.
pub(super) fn truthy(linked: FastPaths) -> Function {
    const CELL: u32 = 0;
    const TAG: u32 = 1;
    const PAYLOAD: u32 = 2;
    const NUMBER: u32 = 3;
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I64), (1, ValType::F64)]);
    let fallback = [I::LocalGet(CELL), I::Call(linked.truthy.unwrap_or(0))];
    read(&mut f, CELL, TAG, PAYLOAD);
    checked(&mut f, TAG, &fallback);
    // A BigInt's value is in its leaf: the linked reader reads it.
    instructions(&mut f, is(TAG, K::BigInt));
    instructions(&mut f, [I::If(BlockType::Empty)]);
    instructions(&mut f, fallback);
    instructions(&mut f, [I::Return, I::End]);
    instructions(&mut f, among(TAG, &OBJECT_VALUES));
    instructions(
        &mut f,
        [I::If(BlockType::Empty), I::I32Const(1), I::Return, I::End],
    );
    instructions(&mut f, is(TAG, K::Boolean));
    instructions(
        &mut f,
        [
            I::If(BlockType::Empty),
            I::LocalGet(PAYLOAD),
            I::I64Const(0),
            I::I64Ne,
            I::Return,
            I::End,
        ],
    );
    instructions(&mut f, is(TAG, K::String));
    instructions(
        &mut f,
        [
            I::If(BlockType::Empty),
            I::LocalGet(PAYLOAD),
            I::I64Const(LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I64Const(0),
            I::I64Ne,
            I::Return,
            I::End,
        ],
    );
    // A Number is truthy unless zero or NaN; null and undefined are falsy.
    instructions(&mut f, is(TAG, K::Float64));
    instructions(
        &mut f,
        [
            I::If(BlockType::Empty),
            I::LocalGet(PAYLOAD),
            I::F64ReinterpretI64,
            I::LocalTee(NUMBER),
            I::LocalGet(NUMBER),
            I::F64Eq,
            I::LocalGet(NUMBER),
            I::F64Const(0.0.into()),
            I::F64Ne,
            I::I32And,
            I::Return,
            I::End,
            I::I32Const(0),
            I::End,
        ],
    );
    f
}

/// `(cell: i64) -> i32`: `null` or `undefined` (`nullish`), or an array.
pub(super) fn kind_test(linked: FastPaths, nullish: bool) -> Function {
    const CELL: u32 = 0;
    const TAG: u32 = 1;
    const PAYLOAD: u32 = 2;
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I64)]);
    let host = if nullish {
        linked.nullish
    } else {
        linked.is_array
    };
    read(&mut f, CELL, TAG, PAYLOAD);
    checked(
        &mut f,
        TAG,
        &[I::LocalGet(CELL), I::Call(host.unwrap_or(0))],
    );
    if nullish {
        instructions(&mut f, among(TAG, &[K::Null, K::Undefined]));
    } else {
        instructions(&mut f, is(TAG, K::Array));
    }
    f.instruction(&I::End);
    f
}

/// `(left: i64, right: i64) -> i32`: `===` (`strict`) or both nullish.
pub(super) fn equal(string_equals: Option<u32>, linked: FastPaths, strict: bool) -> Function {
    const LEFT: u32 = 0;
    const RIGHT: u32 = 1;
    const LEFT_TAG: u32 = 2;
    const RIGHT_TAG: u32 = 3;
    const LEFT_PAYLOAD: u32 = 4;
    const RIGHT_PAYLOAD: u32 = 5;
    let mut f = Function::new([(2, ValType::I32), (2, ValType::I64)]);
    let host = if strict {
        linked.strict_equal
    } else {
        linked.nullish_equal
    };
    let fallback = [
        I::LocalGet(LEFT),
        I::LocalGet(RIGHT),
        I::Call(host.unwrap_or(0)),
    ];
    read(&mut f, LEFT, LEFT_TAG, LEFT_PAYLOAD);
    read(&mut f, RIGHT, RIGHT_TAG, RIGHT_PAYLOAD);
    checked(&mut f, LEFT_TAG, &fallback);
    checked(&mut f, RIGHT_TAG, &fallback);
    if !strict {
        instructions(&mut f, among(LEFT_TAG, &[K::Null, K::Undefined]));
        instructions(&mut f, among(RIGHT_TAG, &[K::Null, K::Undefined]));
        instructions(&mut f, [I::I32And, I::End]);
        return f;
    }
    instructions(
        &mut f,
        [
            I::LocalGet(LEFT_TAG),
            I::LocalGet(RIGHT_TAG),
            I::I32Ne,
            I::If(BlockType::Empty),
            I::I32Const(0),
            I::Return,
            I::End,
        ],
    );
    // Two BigInts compare by value: the linked reader reads their leaves.
    instructions(&mut f, is(LEFT_TAG, K::BigInt));
    instructions(&mut f, [I::If(BlockType::Empty)]);
    instructions(&mut f, fallback);
    instructions(&mut f, [I::Return, I::End]);
    instructions(&mut f, is(LEFT_TAG, K::String));
    instructions(
        &mut f,
        [
            I::If(BlockType::Empty),
            I::LocalGet(LEFT_PAYLOAD),
            I::LocalGet(RIGHT_PAYLOAD),
            I::Call(string_equals.unwrap_or(0)),
            I::Return,
            I::End,
        ],
    );
    instructions(&mut f, is(LEFT_TAG, K::Float64));
    instructions(
        &mut f,
        [
            I::If(BlockType::Empty),
            I::LocalGet(LEFT_PAYLOAD),
            I::F64ReinterpretI64,
            I::LocalGet(RIGHT_PAYLOAD),
            I::F64ReinterpretI64,
            I::F64Eq,
            I::Return,
            I::End,
            // Every other kind by its payload: identity of a reference,
            // the value of a boolean, null or undefined.
            I::LocalGet(LEFT_PAYLOAD),
            I::LocalGet(RIGHT_PAYLOAD),
            I::I64Eq,
            I::End,
        ],
    );
    f
}

/// `(cell: i64, class: i32) -> i32`: `instanceof` a built-in error class.
pub(super) fn is_error_class(linked: FastPaths) -> Function {
    const CELL: u32 = 0;
    const CLASS: u32 = 1;
    const TAG: u32 = 2;
    const PAYLOAD: u32 = 3;
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I64)]);
    let fallback = [
        I::LocalGet(CELL),
        I::LocalGet(CLASS),
        I::Call(linked.is_error_class.unwrap_or(0)),
    ];
    let classes = ErrorClass::ALL.len() as i32;
    read(&mut f, CELL, TAG, PAYLOAD);
    checked(&mut f, TAG, &fallback);
    // An unknown class selector is the linked reader's fatal error.
    instructions(
        &mut f,
        [
            I::LocalGet(CLASS),
            I::I32Const(classes),
            I::I32GeU,
            I::If(BlockType::Empty),
        ],
    );
    instructions(&mut f, fallback);
    instructions(&mut f, [I::Return, I::End]);
    instructions(&mut f, is(TAG, K::Error));
    instructions(
        &mut f,
        [
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::I32Const(0),
            I::Return,
            I::End,
            // Every Error object is an `Error`; otherwise its own class.
            I::LocalGet(CLASS),
            I::I32Const(ErrorClass::Error as i32),
            I::I32Eq,
            I::LocalGet(PAYLOAD),
            I::I32WrapI64,
            I::I32Load(WORD),
            I::I32Const(ERROR_CLASS_SHIFT as i32),
            I::I32ShrU,
            I::LocalGet(CLASS),
            I::I32Eq,
            I::I32Or,
            I::End,
        ],
    );
    f
}
