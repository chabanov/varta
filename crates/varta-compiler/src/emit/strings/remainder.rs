//! V38 (`codegen_v2`) Number `%`: `ToNumber` remainder with the sign of the
//! dividend. Integral operands below 2^53 in magnitude (and a nonzero divisor)
//! are exact as i64 remainders; a zero result keeps the dividend's sign
//! (`-4 % 2` is `-0`). Every other operand pair (fractions, NaN, infinities,
//! a zero divisor, larger magnitudes) calls the host remainder, so results
//! are identical to earlier profiles.
use super::*;

/// 2^53: integers below it in magnitude are exact in binary64 and i64.
const SAFE_LIMIT: f64 = 9_007_199_254_740_992.0;

pub(super) fn function(host: u32) -> Function {
    const DIVIDEND: u32 = 0;
    const DIVISOR: u32 = 1;
    let mut f = Function::new([]);
    let integral = |value: u32| {
        [
            I::LocalGet(value),
            I::LocalGet(value),
            I::F64Trunc,
            I::F64Eq,
            I::LocalGet(value),
            I::F64Abs,
            I::F64Const(SAFE_LIMIT.into()),
            I::F64Lt,
            I::I32And,
        ]
    };
    instructions(&mut f, integral(DIVIDEND));
    instructions(&mut f, integral(DIVISOR));
    instructions(
        &mut f,
        [
            I::I32And,
            I::LocalGet(DIVISOR),
            I::F64Const(0.0.into()),
            I::F64Ne,
            I::I32And,
            I::If(BlockType::Result(ValType::F64)),
            I::LocalGet(DIVIDEND),
            I::I64TruncF64S,
            I::LocalGet(DIVISOR),
            I::I64TruncF64S,
            I::I64RemS,
            I::F64ConvertI64S,
            // A zero remainder takes the dividend's sign.
            I::LocalGet(DIVIDEND),
            I::F64Copysign,
            I::Else,
            I::LocalGet(DIVIDEND),
            I::LocalGet(DIVISOR),
            I::Call(host),
            I::End,
            I::End,
        ],
    );
    f
}
