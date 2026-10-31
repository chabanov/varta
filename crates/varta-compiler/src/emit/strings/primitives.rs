//! V38 (`codegen_v2`, `linked_intrinsics`) primitive conversions and
//! comparisons of primitive cells: an inline fast path for the common cases
//! in front of the linked general path (`primitive_to_string`,
//! `primitive_to_number` and, for BigInts, `primitive_to_string_v6`,
//! `primitive_to_number_v6`, `primitive_compare`, `primitive_equal`).
//!
//! The fast paths read the guest's own cells (tag at the payload's first
//! word, value at [`UNION_META_BYTES`]) and answer exactly what the linked
//! code answers for them:
//!
//! - `String(cell)`: a string cell's own string (strings are immutable, so
//!   no copy), the static spellings of booleans, `null` and `undefined`, and
//!   the decimal digits of an integral Number below 2^53 in magnitude (`-0`
//!   is `"0"`; every such integer is its shortest round-trip text), two at a
//!   time from a static digit-pair table. Every other Number is the
//!   BigInt-free linked conversion, a BigInt the value18 one.
//! - `Number(cell)`: Numbers, booleans, `null` (0), `undefined` (NaN) and
//!   strings of 1 to 15 ASCII decimal digits; other strings are the linked
//!   parser, BigInts the value18 conversion.
//! - `<`, `<=`, `>`, `>=` of two cells: two strings by UTF-16 code units
//!   (the `Compare` helper); a BigInt with a string by the linked comparison
//!   (`StringToBigInt`); every other pair numerically, with `ToNumber` of a
//!   string by the digit reader or the linked parser and a BigInt below 2^32
//!   in magnitude exactly; a larger BigInt is the linked comparison.
//! - `==`/`===`: same kinds by value (strings by content through
//!   `StringEquals`), BigInts below 2^32 numerically; different kinds are
//!   unequal under `===`, and under `==` `null` and `undefined` equal each
//!   other and nothing else, a BigInt with a string is the linked
//!   comparison and every other pair compares numerically as above.
//!
//! Helpers follow the kinds their cells may hold ([`Cells`], the `bigint`
//! flag of the conversions): a path for a kind the cells cannot hold is not
//! emitted, so its linked code is not linked.
use super::*;
use varta_common::value::{HEAP_UNION_FLAG, UNION_BYTES, UNION_META_BYTES, ValueKind};

/// 2^53: integers below it in magnitude print their exact decimal digits.
const SAFE_LIMIT: f64 = 9_007_199_254_740_992.0;
const UNIT_BYTES: i32 = size_of::<u16>() as i32;
const DECIMAL: i64 = 10;
/// Digits of the largest magnitude the decimal writer takes (below 2^53).
const MAX_DIGITS: i32 = 16;
/// The base of the two-digit steps.
const PAIR: i64 = DECIMAL * DECIMAL;
/// Digits of the longest digit string read without the linked parser: 15
/// decimal digits are an exact integer below 2^53.
const MAX_STRING_DIGITS: i32 = 15;
const ZERO: i64 = b'0' as i64;
const MINUS: i32 = b'-' as i32;
const UNIT: MemArg = MemArg {
    offset: 0,
    align: 1,
    memory_index: MEMORY,
};
const PAYLOAD: MemArg = MemArg {
    offset: UNION_META_BYTES as u64,
    align: 3,
    memory_index: MEMORY,
};

/// The static spellings of `String` and `typeof` results, in one UTF-16
/// string of static data: each spelling is a slice of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Spelling {
    False,
    True,
    Null,
    Undefined,
    Object,
    String,
    Number,
    Boolean,
    BigInt,
    Function,
}
impl Spelling {
    const ALL: [Self; 10] = [
        Self::False,
        Self::True,
        Self::Null,
        Self::Undefined,
        Self::Object,
        Self::String,
        Self::Number,
        Self::Boolean,
        Self::BigInt,
        Self::Function,
    ];
    fn text(self) -> &'static str {
        match self {
            Self::False => "false",
            Self::True => "true",
            Self::Null => "null",
            Self::Undefined => "undefined",
            Self::Object => "object",
            Self::String => "string",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::BigInt => "bigint",
            Self::Function => "function",
        }
    }
}
/// The spellings followed by the digit pairs `00` to `99`.
pub(in crate::emit) fn spellings() -> Vec<u8> {
    let mut text: String = Spelling::ALL.iter().map(|s| s.text()).collect();
    for pair in 0..PAIR {
        text.push_str(&format!("{pair:02}"));
    }
    varta_common::text::encode(&text)
}

/// What the fast-path helpers call: the linked operations they fall back
/// to, the result-length global the string results are read with, and the
/// static spellings.
#[derive(Clone, Copy, Debug, Default)]
pub(in crate::emit) struct FastPaths {
    pub spellings: Option<u32>,
    pub result_len: u32,
    /// `primitive_to_string_v6` (BigInt cells) and `primitive_to_string`
    /// (every other cell, without BigInt code).
    pub text: Option<u32>,
    pub number_text: Option<u32>,
    /// `primitive_to_number_v6` (BigInt cells) and `primitive_to_number`.
    pub number: Option<u32>,
    pub string_number: Option<u32>,
    pub compare: Option<u32>,
    pub equal: Option<u32>,
    pub type_of: Option<u32>,
    pub truthy: Option<u32>,
    pub nullish: Option<u32>,
    pub is_array: Option<u32>,
    pub strict_equal: Option<u32>,
    pub nullish_equal: Option<u32>,
    pub is_error_class: Option<u32>,
}
impl FastPaths {
    /// Address of the digit pairs, after the spellings.
    fn pairs(self) -> u32 {
        let units: usize = Spelling::ALL.iter().map(|s| s.text().len()).sum();
        self.spellings.unwrap_or(0) + (units * UNIT_BYTES as usize) as u32
    }
    /// The packed static handle of `spelling` in the spellings table.
    pub(super) fn spelling(self, spelling: Spelling) -> i64 {
        let index = Spelling::ALL
            .iter()
            .position(|s| *s == spelling)
            .unwrap_or_default();
        let start: usize = Spelling::ALL[..index].iter().map(|s| s.text().len()).sum();
        let ptr = i64::from(self.spellings.unwrap_or(0)) + (start as i64) * i64::from(UNIT_BYTES);
        let len = (spelling.text().len() as i64) * i64::from(UNIT_BYTES);
        ptr | (len << LENGTH_SHIFT)
    }
}

/// Load a cell's tag into `tag` and its payload into `payload`.
pub(super) fn read(function: &mut Function, cell: u32, tag: u32, payload: u32) {
    instructions(
        function,
        [
            I::LocalGet(cell),
            I::I32WrapI64,
            I::I32Load(WORD),
            I::LocalSet(tag),
            I::LocalGet(cell),
            I::I32WrapI64,
            I::I64Load(PAYLOAD),
            I::LocalSet(payload),
        ],
    );
}
/// `tag == kind`.
pub(super) fn is(tag: u32, kind: ValueKind) -> [I<'static>; 3] {
    [I::LocalGet(tag), I::I32Const(kind as i32), I::I32Eq]
}
fn mask(kinds: &[ValueKind]) -> i32 {
    kinds
        .iter()
        .fold(0u32, |mask, kind| mask | 1 << *kind as u32) as i32
}
/// Whether `tag` is one of `kinds` (tags are below 32; a larger one is in no
/// set).
pub(super) fn among(tag: u32, kinds: &[ValueKind]) -> [I<'static>; 9] {
    [
        I::LocalGet(tag),
        I::I32Const(u32::BITS as i32),
        I::I32LtU,
        I::I32Const(mask(kinds)),
        I::LocalGet(tag),
        I::I32ShrU,
        I::I32Const(1),
        I::I32And,
        I::I32And,
    ]
}
/// The Number-like kinds (Number, boolean, null, undefined), whose
/// `ToNumber` needs no parsing.
const NUMERIC: [ValueKind; 4] = [
    ValueKind::Float64,
    ValueKind::Boolean,
    ValueKind::Null,
    ValueKind::Undefined,
];
/// Whether `tag` is a Number-like kind.
fn numeric(function: &mut Function, tag: u32) {
    instructions(function, among(tag, &NUMERIC));
}
/// `ToNumber` of a Number-like cell already read into `tag`/`payload`.
fn to_number(function: &mut Function, tag: u32, payload: u32) {
    instructions(function, is(tag, ValueKind::Float64));
    instructions(
        function,
        [
            I::If(BlockType::Result(ValType::F64)),
            I::LocalGet(payload),
            I::F64ReinterpretI64,
            I::Else,
        ],
    );
    instructions(function, is(tag, ValueKind::Boolean));
    instructions(
        function,
        [
            I::If(BlockType::Result(ValType::F64)),
            I::LocalGet(payload),
            I::F64ConvertI64U,
            I::Else,
        ],
    );
    instructions(function, is(tag, ValueKind::Null));
    instructions(
        function,
        [
            I::If(BlockType::Result(ValType::F64)),
            I::F64Const(0.0.into()),
            I::Else,
            I::F64Const(f64::NAN.into()),
            I::End,
            I::End,
            I::End,
        ],
    );
}

/// Locals of the decimal writer ([`decimal`]), after a helper's own.
#[derive(Clone, Copy)]
struct Decimal {
    /// The Number (f64).
    number: u32,
    /// Its magnitude (i64), then the remaining digits.
    value: u32,
    /// The next magnitude (i64).
    quotient: u32,
    /// Digits plus the sign, then the byte length (i32).
    length: u32,
    /// The text's address (i32).
    dest: u32,
    /// The write position (i32).
    at: u32,
}
/// If `number` is integral and below 2^53 in magnitude, return its decimal
/// text (exactly its shortest round-trip text, `-0` as `"0"`); otherwise
/// fall through. Digits are written two at a time from the digit pairs of
/// the static spellings.
fn decimal(f: &mut Function, allocate: u32, linked: FastPaths, locals: Decimal) {
    let Decimal {
        number,
        value,
        quotient,
        length,
        dest,
        at,
    } = locals;
    let pairs = linked.pairs();
    instructions(
        f,
        [
            I::LocalGet(number),
            I::LocalGet(number),
            I::F64Trunc,
            I::F64Eq,
            I::LocalGet(number),
            I::F64Abs,
            I::F64Const(SAFE_LIMIT.into()),
            I::F64Lt,
            I::I32And,
            I::If(BlockType::Empty),
            // `value` = |n|; `length` = the sign, then plus the digits.
            I::LocalGet(number),
            I::F64Abs,
            I::I64TruncF64U,
            I::LocalSet(value),
            I::LocalGet(number),
            I::F64Const(0.0.into()),
            I::F64Lt,
            I::Block(BlockType::Result(ValType::I32)),
        ],
    );
    // The digit count: the first power of ten above the magnitude.
    let mut power: i64 = DECIMAL;
    for digits in 1..MAX_DIGITS {
        instructions(
            f,
            [
                I::I32Const(digits),
                I::LocalGet(value),
                I::I64Const(power),
                I::I64LtU,
                I::BrIf(0),
                I::Drop,
            ],
        );
        power *= DECIMAL;
    }
    instructions(
        f,
        [
            I::I32Const(MAX_DIGITS),
            I::End,
            I::I32Add,
            I::I32Const(UNIT_BYTES),
            I::I32Mul,
            I::LocalTee(length),
            I::Call(allocate),
            I::LocalTee(dest),
            I::LocalGet(length),
            I::I32Add,
            I::LocalSet(at),
            // Two digits at a time from the last.
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(value),
            I::I64Const(PAIR),
            I::I64LtU,
            I::BrIf(1),
            I::LocalGet(at),
            I::I32Const(2 * UNIT_BYTES),
            I::I32Sub,
            I::LocalTee(at),
            I::LocalGet(value),
            I::LocalGet(value),
            I::I64Const(PAIR),
            I::I64DivU,
            I::LocalTee(quotient),
            I::I64Const(PAIR),
            I::I64Mul,
            I::I64Sub,
            I::I32WrapI64,
            I::I32Const(2 * UNIT_BYTES),
            I::I32Mul,
            I::I32Load(MemArg {
                offset: u64::from(pairs),
                ..UNIT
            }),
            I::I32Store(UNIT),
            I::LocalGet(quotient),
            I::LocalSet(value),
            I::Br(0),
            I::End,
            I::End,
            // One or two digits remain.
            I::LocalGet(value),
            I::I64Const(DECIMAL),
            I::I64GeU,
            I::If(BlockType::Empty),
            I::LocalGet(at),
            I::I32Const(2 * UNIT_BYTES),
            I::I32Sub,
            I::LocalTee(at),
            I::LocalGet(value),
            I::I32WrapI64,
            I::I32Const(2 * UNIT_BYTES),
            I::I32Mul,
            I::I32Load(MemArg {
                offset: u64::from(pairs),
                ..UNIT
            }),
            I::I32Store(UNIT),
            I::Else,
            I::LocalGet(at),
            I::I32Const(UNIT_BYTES),
            I::I32Sub,
            I::LocalTee(at),
            I::LocalGet(value),
            I::I64Const(ZERO),
            I::I64Add,
            I::I64Store16(UNIT),
            I::End,
            // A remaining unit is the sign.
            I::LocalGet(at),
            I::LocalGet(dest),
            I::I32Ne,
            I::If(BlockType::Empty),
            I::LocalGet(dest),
            I::I32Const(MINUS),
            I::I32Store16(UNIT),
            I::End,
        ],
    );
    pack(f, length, dest);
    instructions(f, [I::Return, I::End]);
}

/// `(number: f64) -> string`: `String(n)` of a Number boxed nowhere. A
/// Number the decimal writer does not answer is boxed in a fresh cell for
/// the linked conversion (the BigInt-free `primitive_to_string`).
pub(super) fn number_text(allocate: u32, linked: FastPaths) -> Function {
    const NUMBER: u32 = 0;
    const LENGTH: u32 = 1;
    const DEST: u32 = 2;
    const AT: u32 = 3;
    const VALUE: u32 = 4;
    const QUOTIENT: u32 = 5;
    let mut f = Function::new([(AT - LENGTH + 1, ValType::I32), (2, ValType::I64)]);
    decimal(
        &mut f,
        allocate,
        linked,
        Decimal {
            number: NUMBER,
            value: VALUE,
            quotient: QUOTIENT,
            length: LENGTH,
            dest: DEST,
            at: AT,
        },
    );
    // The cell `box_union` builds for a Number.
    instructions(
        &mut f,
        [
            I::I32Const(UNION_BYTES as i32),
            I::Call(allocate),
            I::LocalSet(DEST),
            I::LocalGet(DEST),
            I::I32Const(HEADER as i32),
            I::I32Sub,
            I::I32Const(ALLOCATED | HEAP_UNION_FLAG),
            I::I32Store(FLAG),
            I::LocalGet(DEST),
            I::I32Const(HEADER as i32),
            I::I32Sub,
            I::I32Const(0),
            I::I32Store(LINK),
            I::LocalGet(DEST),
            I::I32Const(ValueKind::Float64 as i32),
            I::I32Store(WORD),
            I::LocalGet(DEST),
            I::I32Const(1),
            I::I32Store(MemArg {
                offset: size_of::<u32>() as u64,
                ..WORD
            }),
            I::LocalGet(DEST),
            I::LocalGet(NUMBER),
            I::F64Store(MemArg {
                offset: UNION_META_BYTES as u64,
                align: 3,
                memory_index: MEMORY,
            }),
            I::LocalGet(DEST),
            I::I64ExtendI32U,
            I::I64Const((UNION_BYTES as i64) << LENGTH_SHIFT),
            I::I64Or,
            I::Call(linked.number_text.unwrap_or(0)),
        ],
    );
    // The linked conversion returns an owned string.
    instructions(&mut f, super::owned_text(linked.result_len));
    f.instruction(&I::End);
    f
}

/// `(cell: i64) -> string`.
pub(super) fn text(allocate: u32, linked: FastPaths, bigint: bool) -> Function {
    const CELL: u32 = 0;
    const TAG: u32 = 1;
    const LENGTH: u32 = 2;
    const DEST: u32 = 3;
    const AT: u32 = 4;
    const PAYLOAD_LOCAL: u32 = 5;
    const VALUE: u32 = 6;
    const QUOTIENT: u32 = 7;
    const NUMBER: u32 = 8;
    let mut f = Function::new([
        (AT - TAG + 1, ValType::I32),
        (QUOTIENT - PAYLOAD_LOCAL + 1, ValType::I64),
        (1, ValType::F64),
    ]);
    read(&mut f, CELL, TAG, PAYLOAD_LOCAL);
    // A Number first: the common case.
    instructions(&mut f, is(TAG, ValueKind::Float64));
    instructions(
        &mut f,
        [
            I::If(BlockType::Empty),
            I::LocalGet(PAYLOAD_LOCAL),
            I::F64ReinterpretI64,
            I::LocalSet(NUMBER),
        ],
    );
    decimal(
        &mut f,
        allocate,
        linked,
        Decimal {
            number: NUMBER,
            value: VALUE,
            quotient: QUOTIENT,
            length: LENGTH,
            dest: DEST,
            at: AT,
        },
    );
    f.instruction(&I::End);
    // A string cell's own string.
    instructions(&mut f, is(TAG, ValueKind::String));
    instructions(
        &mut f,
        [
            I::If(BlockType::Empty),
            I::LocalGet(PAYLOAD_LOCAL),
            I::Return,
            I::End,
        ],
    );
    instructions(&mut f, is(TAG, ValueKind::Boolean));
    instructions(
        &mut f,
        [
            I::If(BlockType::Empty),
            I::I64Const(linked.spelling(Spelling::True)),
            I::I64Const(linked.spelling(Spelling::False)),
            I::LocalGet(PAYLOAD_LOCAL),
            I::I64Const(0),
            I::I64Ne,
            I::Select,
            I::Return,
            I::End,
        ],
    );
    for (kind, spelling) in [
        (ValueKind::Null, Spelling::Null),
        (ValueKind::Undefined, Spelling::Undefined),
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
    // Every other cell: the linked conversion into an owned string, the
    // BigInt one only for a BigInt.
    if bigint {
        instructions(&mut f, is(TAG, ValueKind::BigInt));
        instructions(
            &mut f,
            [
                I::If(BlockType::Result(ValType::I32)),
                I::LocalGet(CELL),
                I::Call(linked.text.unwrap_or(0)),
                I::Else,
                I::LocalGet(CELL),
                I::Call(linked.number_text.unwrap_or(0)),
                I::End,
            ],
        );
    } else {
        instructions(
            &mut f,
            [I::LocalGet(CELL), I::Call(linked.number_text.unwrap_or(0))],
        );
    }
    instructions(&mut f, super::owned_text(linked.result_len));
    f.instruction(&I::End);
    f
}

/// Locals of the digit-string reader ([`digit_string`]).
#[derive(Clone, Copy)]
struct Digits {
    /// The next unit's address (i32).
    at: u32,
    /// The current digit (i32).
    unit: u32,
    /// The end address (i32).
    end: u32,
    /// The accumulated value (i64).
    value: u32,
}
/// Leave 1 and set `out` to `ToNumber` of the string `payload` when it is 1
/// to [`MAX_STRING_DIGITS`] ASCII decimal digits (an exact integer below
/// 2^53, leading zeros included); leave 0 for any other string, which the
/// linked parser reads.
fn digit_string(f: &mut Function, payload: u32, out: u32, locals: Digits) {
    let Digits {
        at,
        unit,
        end,
        value,
    } = locals;
    instructions(
        f,
        [
            I::Block(BlockType::Result(ValType::I32)),
            I::I32Const(0),
            I::LocalGet(payload),
            I::I64Const(LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I32WrapI64,
            I::LocalTee(end),
            I::I32Eqz,
            I::LocalGet(end),
            I::I32Const(MAX_STRING_DIGITS * UNIT_BYTES),
            I::I32GtU,
            I::I32Or,
            I::BrIf(0),
            I::Drop,
            I::LocalGet(payload),
            I::I32WrapI64,
            I::LocalTee(at),
            I::LocalGet(end),
            I::I32Add,
            I::LocalSet(end),
            I::I64Const(0),
            I::LocalSet(value),
            I::Loop(BlockType::Empty),
            I::LocalGet(at),
            I::I32Load16U(UNIT),
            I::I32Const(ZERO as i32),
            I::I32Sub,
            I::LocalTee(unit),
            I::I32Const(9),
            I::I32GtU,
            I::If(BlockType::Empty),
            I::I32Const(0),
            I::Br(2),
            I::End,
            I::LocalGet(value),
            I::I64Const(DECIMAL),
            I::I64Mul,
            I::LocalGet(unit),
            I::I64ExtendI32U,
            I::I64Add,
            I::LocalSet(value),
            I::LocalGet(at),
            I::I32Const(UNIT_BYTES),
            I::I32Add,
            I::LocalTee(at),
            I::LocalGet(end),
            I::I32LtU,
            I::BrIf(0),
            I::End,
            I::LocalGet(value),
            I::F64ConvertI64U,
            I::LocalSet(out),
            I::I32Const(1),
            I::End,
        ],
    );
}
/// One operand of a comparison: its cell and the locals it was read into.
#[derive(Clone, Copy)]
struct Operand {
    cell: u32,
    tag: u32,
    payload: u32,
    /// Receives its `ToNumber`.
    number: u32,
}
/// Leave 1 and set the operand's `number` to the Number its cell compares
/// as, or leave 0 for a BigInt of more than one digit word: Number-like
/// kinds inline, digit strings by the digit reader, every other string by
/// the linked parser (`primitive_to_number`, which the linked comparison
/// calls for it too) and a BigInt below 2^32 in magnitude exactly (so a
/// comparison with a Number or another such BigInt is numeric). Callers send
/// a BigInt compared with a string to the linked comparison first: that
/// string converts with `StringToBigInt`, not `ToNumber`.
fn side(f: &mut Function, operand: Operand, digits: Digits, linked: FastPaths, cells: Cells) {
    let Operand {
        cell,
        tag,
        payload,
        number,
    } = operand;
    // What remains after the Number-like kinds: a BigInt, if any.
    let bigint = |f: &mut Function| {
        if cells.bigint {
            small_bigint(f, payload, number);
        } else {
            f.instruction(&I::I32Const(0));
        }
    };
    numeric(f, tag);
    f.instruction(&I::If(BlockType::Result(ValType::I32)));
    to_number(f, tag, payload);
    instructions(f, [I::LocalSet(number), I::I32Const(1), I::Else]);
    if cells.string {
        instructions(f, is(tag, ValueKind::String));
        f.instruction(&I::If(BlockType::Result(ValType::I32)));
        digit_string(f, payload, number, digits);
        instructions(
            f,
            [
                I::I32Eqz,
                I::If(BlockType::Empty),
                I::LocalGet(cell),
                I::Call(linked.string_number.unwrap_or(0)),
                I::LocalSet(number),
                I::End,
                I::I32Const(1),
                I::Else,
            ],
        );
        bigint(f);
        f.instruction(&I::End);
    } else {
        bigint(f);
    }
    f.instruction(&I::End);
}
/// Leave 1 and set `out` to the BigInt leaf `payload` as an exact Number
/// when it has at most one digit word; leave 0 otherwise.
fn small_bigint(f: &mut Function, payload: u32, out: u32) {
    const SIGN: MemArg = MemArg {
        offset: 0,
        align: 2,
        memory_index: MEMORY,
    };
    const DIGITS: MemArg = MemArg {
        offset: varta_common::bigint::DIGIT_BYTES as u64,
        ..SIGN
    };
    const FIRST: MemArg = MemArg {
        offset: varta_common::bigint::META_BYTES as u64,
        ..SIGN
    };
    instructions(
        f,
        [
            I::LocalGet(payload),
            I::I32WrapI64,
            I::I32Load(DIGITS),
            I::I32Const(1),
            I::I32GtU,
            I::If(BlockType::Result(ValType::I32)),
            I::I32Const(0),
            I::Else,
            // Zero has no digit word (and nothing follows its count).
            I::LocalGet(payload),
            I::I32WrapI64,
            I::I32Load(DIGITS),
            I::If(BlockType::Result(ValType::F64)),
            I::LocalGet(payload),
            I::I32WrapI64,
            I::I32Load(FIRST),
            I::F64ConvertI32U,
            I::Else,
            I::F64Const(0.0.into()),
            I::End,
            I::LocalSet(out),
            I::LocalGet(payload),
            I::I32WrapI64,
            I::I32Load(SIGN),
            I::I32Const(varta_common::bigint::Sign::Negative as i32),
            I::I32Eq,
            I::If(BlockType::Empty),
            I::LocalGet(out),
            I::F64Neg,
            I::LocalSet(out),
            I::End,
            I::I32Const(1),
            I::End,
        ],
    );
}
/// Leave 1 when one operand is a BigInt and the other a string.
fn bigint_and_string(f: &mut Function, left: u32, right: u32) {
    instructions(f, is(left, ValueKind::BigInt));
    instructions(f, is(right, ValueKind::String));
    f.instruction(&I::I32And);
    instructions(f, is(left, ValueKind::String));
    instructions(f, is(right, ValueKind::BigInt));
    instructions(f, [I::I32And, I::I32Or]);
}

/// `(text: string) -> f64`: `Number(s)` of a string boxed nowhere. A string
/// the digit reader does not answer is boxed in a fresh cell (rooted while
/// the cell is allocated) for the linked parser (`primitive_to_number`).
pub(super) fn string_number(
    allocate: u32,
    root_push: u32,
    roots: u32,
    linked: FastPaths,
) -> Function {
    const TEXT: u32 = 0;
    const AT: u32 = 1;
    const UNIT_LOCAL: u32 = 2;
    const END: u32 = 3;
    const SAVED: u32 = 4;
    const DEST: u32 = 5;
    const VALUE: u32 = 6;
    const NUMBER: u32 = 7;
    let mut f = Function::new([
        (DEST - AT + 1, ValType::I32),
        (1, ValType::I64),
        (1, ValType::F64),
    ]);
    digit_string(
        &mut f,
        TEXT,
        NUMBER,
        Digits {
            at: AT,
            unit: UNIT_LOCAL,
            end: END,
            value: VALUE,
        },
    );
    instructions(
        &mut f,
        [
            I::If(BlockType::Empty),
            I::LocalGet(NUMBER),
            I::Return,
            I::End,
            // The cell `box_union` builds for a string, with the string
            // rooted while it is allocated.
            I::GlobalGet(roots),
            I::LocalSet(SAVED),
            I::LocalGet(TEXT),
            I::Call(root_push),
            I::Drop,
            I::I32Const(UNION_BYTES as i32),
            I::Call(allocate),
            I::LocalSet(DEST),
            I::LocalGet(DEST),
            I::I32Const(HEADER as i32),
            I::I32Sub,
            I::I32Const(ALLOCATED | HEAP_UNION_FLAG),
            I::I32Store(FLAG),
            I::LocalGet(DEST),
            I::I32Const(HEADER as i32),
            I::I32Sub,
            I::I32Const(1),
            I::I32Store(LINK),
            I::LocalGet(DEST),
            I::I32Const(ValueKind::String as i32),
            I::I32Store(WORD),
            I::LocalGet(DEST),
            I::I32Const(1),
            I::I32Store(MemArg {
                offset: size_of::<u32>() as u64,
                ..WORD
            }),
            I::LocalGet(DEST),
            I::LocalGet(TEXT),
            I::I64Store(PAYLOAD),
            // The linked parser allocates nothing in the guest heap.
            I::LocalGet(SAVED),
            I::GlobalSet(roots),
            I::LocalGet(DEST),
            I::I64ExtendI32U,
            I::I64Const((UNION_BYTES as i64) << LENGTH_SHIFT),
            I::I64Or,
            I::Call(linked.string_number.unwrap_or(0)),
            I::End,
        ],
    );
    f
}

/// `(cell: i64) -> f64`.
pub(super) fn number(linked: FastPaths, bigint: bool) -> Function {
    const CELL: u32 = 0;
    const TAG: u32 = 1;
    const AT: u32 = 2;
    const UNIT_LOCAL: u32 = 3;
    const END: u32 = 4;
    const PAYLOAD_LOCAL: u32 = 5;
    const VALUE: u32 = 6;
    const NUMBER: u32 = 7;
    let mut f = Function::new([
        (END - TAG + 1, ValType::I32),
        (VALUE - PAYLOAD_LOCAL + 1, ValType::I64),
        (1, ValType::F64),
    ]);
    let digits = Digits {
        at: AT,
        unit: UNIT_LOCAL,
        end: END,
        value: VALUE,
    };
    read(&mut f, CELL, TAG, PAYLOAD_LOCAL);
    // A string first: `Number(s)` mostly converts text.
    instructions(&mut f, is(TAG, ValueKind::String));
    f.instruction(&I::If(BlockType::Result(ValType::I32)));
    digit_string(&mut f, PAYLOAD_LOCAL, NUMBER, digits);
    f.instruction(&I::Else);
    numeric(&mut f, TAG);
    f.instruction(&I::If(BlockType::Result(ValType::I32)));
    to_number(&mut f, TAG, PAYLOAD_LOCAL);
    instructions(
        &mut f,
        [
            I::LocalSet(NUMBER),
            I::I32Const(1),
            I::Else,
            I::I32Const(0),
            I::End,
            I::End,
            I::If(BlockType::Result(ValType::F64)),
            I::LocalGet(NUMBER),
            I::Else,
        ],
    );
    if bigint {
        instructions(&mut f, is(TAG, ValueKind::BigInt));
        instructions(
            &mut f,
            [
                I::If(BlockType::Result(ValType::F64)),
                I::LocalGet(CELL),
                I::Call(linked.number.unwrap_or(0)),
                I::Else,
                I::LocalGet(CELL),
                I::Call(linked.string_number.unwrap_or(0)),
                I::End,
            ],
        );
    } else {
        instructions(
            &mut f,
            [
                I::LocalGet(CELL),
                I::Call(linked.string_number.unwrap_or(0)),
            ],
        );
    }
    instructions(&mut f, [I::End, I::End]);
    f
}

/// `(left: i64, right: i64) -> f64`: -1, 0, 1 or NaN when unordered.
pub(super) fn order(compare: u32, linked: FastPaths, cells: Cells) -> Function {
    const LEFT: u32 = 0;
    const RIGHT: u32 = 1;
    const LEFT_TAG: u32 = 2;
    const RIGHT_TAG: u32 = 3;
    const AT: u32 = 4;
    const UNIT_LOCAL: u32 = 5;
    const END: u32 = 6;
    const LEFT_PAYLOAD: u32 = 7;
    const RIGHT_PAYLOAD: u32 = 8;
    const VALUE: u32 = 9;
    const X: u32 = 10;
    const Y: u32 = 11;
    let mut f = Function::new([(5, ValType::I32), (3, ValType::I64), (2, ValType::F64)]);
    let digits = Digits {
        at: AT,
        unit: UNIT_LOCAL,
        end: END,
        value: VALUE,
    };
    read(&mut f, LEFT, LEFT_TAG, LEFT_PAYLOAD);
    read(&mut f, RIGHT, RIGHT_TAG, RIGHT_PAYLOAD);
    if cells.string {
        instructions(&mut f, is(LEFT_TAG, ValueKind::String));
        instructions(&mut f, is(RIGHT_TAG, ValueKind::String));
        instructions(
            &mut f,
            [
                I::I32And,
                I::If(BlockType::Empty),
                I::LocalGet(LEFT_PAYLOAD),
                I::LocalGet(RIGHT_PAYLOAD),
                I::Call(compare),
                I::F64ConvertI32S,
                I::Return,
                I::End,
            ],
        );
    }
    // A BigInt and a string: `StringToBigInt`, the linked comparison.
    if cells.bigint && cells.string {
        bigint_and_string(&mut f, LEFT_TAG, RIGHT_TAG);
        instructions(
            &mut f,
            [
                I::If(BlockType::Empty),
                I::LocalGet(LEFT),
                I::LocalGet(RIGHT),
                I::Call(linked.compare.unwrap_or(0)),
                I::Return,
                I::End,
            ],
        );
    }
    side(
        &mut f,
        Operand {
            cell: LEFT,
            tag: LEFT_TAG,
            payload: LEFT_PAYLOAD,
            number: X,
        },
        digits,
        linked,
        cells,
    );
    side(
        &mut f,
        Operand {
            cell: RIGHT,
            tag: RIGHT_TAG,
            payload: RIGHT_PAYLOAD,
            number: Y,
        },
        digits,
        linked,
        cells,
    );
    instructions(
        &mut f,
        [
            I::I32And,
            I::If(BlockType::Empty),
            I::F64Const((-1.0).into()),
            I::F64Const(1.0.into()),
            I::F64Const(0.0.into()),
            I::F64Const(f64::NAN.into()),
            I::LocalGet(X),
            I::LocalGet(Y),
            I::F64Eq,
            I::Select,
            I::LocalGet(X),
            I::LocalGet(Y),
            I::F64Gt,
            I::Select,
            I::LocalGet(X),
            I::LocalGet(Y),
            I::F64Lt,
            I::Select,
            I::Return,
            I::End,
        ],
    );
    // A BigInt of more than one digit word; without BigInt cells every
    // operand was answered.
    if cells.bigint {
        instructions(
            &mut f,
            [
                I::LocalGet(LEFT),
                I::LocalGet(RIGHT),
                I::Call(linked.compare.unwrap_or(0)),
            ],
        );
    } else {
        f.instruction(&I::Unreachable);
    }
    f.instruction(&I::End);
    f
}

/// `(strict: i32, left: i64, right: i64) -> i32`: 1 when equal.
pub(super) fn equal(string_equals: u32, linked: FastPaths, cells: Cells) -> Function {
    const STRICT: u32 = 0;
    const LEFT: u32 = 1;
    const RIGHT: u32 = 2;
    const LEFT_TAG: u32 = 3;
    const RIGHT_TAG: u32 = 4;
    const AT: u32 = 5;
    const UNIT_LOCAL: u32 = 6;
    const END: u32 = 7;
    const LEFT_PAYLOAD: u32 = 8;
    const RIGHT_PAYLOAD: u32 = 9;
    const VALUE: u32 = 10;
    const X: u32 = 11;
    const Y: u32 = 12;
    let mut f = Function::new([(5, ValType::I32), (3, ValType::I64), (2, ValType::F64)]);
    let digits = Digits {
        at: AT,
        unit: UNIT_LOCAL,
        end: END,
        value: VALUE,
    };
    read(&mut f, LEFT, LEFT_TAG, LEFT_PAYLOAD);
    read(&mut f, RIGHT, RIGHT_TAG, RIGHT_PAYLOAD);
    let nullish = |tag: u32| {
        let mut code = Vec::from(is(tag, ValueKind::Null));
        code.extend(is(tag, ValueKind::Undefined));
        code.push(I::I32Or);
        code
    };
    // Same kinds other than BigInt: by value.
    instructions(
        &mut f,
        [I::LocalGet(LEFT_TAG), I::LocalGet(RIGHT_TAG), I::I32Eq],
    );
    instructions(&mut f, is(LEFT_TAG, ValueKind::BigInt));
    instructions(&mut f, [I::I32Eqz, I::I32And, I::If(BlockType::Empty)]);
    if cells.string {
        instructions(&mut f, is(LEFT_TAG, ValueKind::String));
        instructions(
            &mut f,
            [
                I::If(BlockType::Empty),
                I::LocalGet(LEFT_PAYLOAD),
                I::LocalGet(RIGHT_PAYLOAD),
                I::Call(string_equals),
                I::Return,
                I::End,
            ],
        );
    }
    instructions(&mut f, is(LEFT_TAG, ValueKind::Float64));
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
            // Booleans, null and undefined: by payload.
            I::LocalGet(LEFT_PAYLOAD),
            I::LocalGet(RIGHT_PAYLOAD),
            I::I64Eq,
            I::Return,
            I::End,
        ],
    );
    // Different kinds: unequal under `===`.
    instructions(
        &mut f,
        [
            I::LocalGet(LEFT_TAG),
            I::LocalGet(RIGHT_TAG),
            I::I32Ne,
            I::LocalGet(STRICT),
            I::I32And,
            I::If(BlockType::Empty),
            I::I32Const(0),
            I::Return,
            I::End,
        ],
    );
    // `==` with a nullish side: both nullish.
    instructions(&mut f, nullish(LEFT_TAG));
    instructions(&mut f, nullish(RIGHT_TAG));
    instructions(&mut f, [I::I32Or, I::If(BlockType::Empty)]);
    instructions(&mut f, nullish(LEFT_TAG));
    instructions(&mut f, nullish(RIGHT_TAG));
    instructions(&mut f, [I::I32And, I::Return, I::End]);
    // A BigInt and a string: `StringToBigInt`, the linked comparison.
    if cells.bigint && cells.string {
        bigint_and_string(&mut f, LEFT_TAG, RIGHT_TAG);
        instructions(
            &mut f,
            [
                I::If(BlockType::Empty),
                I::LocalGet(STRICT),
                I::LocalGet(LEFT),
                I::LocalGet(RIGHT),
                I::Call(linked.equal.unwrap_or(0)),
                I::Return,
                I::End,
            ],
        );
    }
    // `==` of other kinds, and of BigInts below 2^32: numerically.
    side(
        &mut f,
        Operand {
            cell: LEFT,
            tag: LEFT_TAG,
            payload: LEFT_PAYLOAD,
            number: X,
        },
        digits,
        linked,
        cells,
    );
    side(
        &mut f,
        Operand {
            cell: RIGHT,
            tag: RIGHT_TAG,
            payload: RIGHT_PAYLOAD,
            number: Y,
        },
        digits,
        linked,
        cells,
    );
    instructions(
        &mut f,
        [
            I::I32And,
            I::If(BlockType::Empty),
            I::LocalGet(X),
            I::LocalGet(Y),
            I::F64Eq,
            I::Return,
            I::End,
        ],
    );
    // A BigInt of more than one digit word; without BigInt cells every
    // operand was answered.
    if cells.bigint {
        instructions(
            &mut f,
            [
                I::LocalGet(STRICT),
                I::LocalGet(LEFT),
                I::LocalGet(RIGHT),
                I::Call(linked.equal.unwrap_or(0)),
            ],
        );
    } else {
        f.instruction(&I::Unreachable);
    }
    f.instruction(&I::End);
    f
}
