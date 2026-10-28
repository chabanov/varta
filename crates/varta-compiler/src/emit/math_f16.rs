//! Exact binary64 -> binary16 ties-to-even, without a binary32 intermediate.
use super::*;

const FRACTION_BITS: i64 = 52;
const EXPONENT_BIAS: i32 = 1023;
const EXPONENT_MASK: i32 = 0x7ff;
const HALF_FRACTION_BITS: i32 = 10;
const HALF_MIN_NORMAL: f64 = 0.00006103515625; // 2^-14
const SUBNORMAL_SCALE: f64 = 16777216.0; // 2^24
const OVERFLOW_MIDPOINT: f64 = 65520.0;

impl Emitter<'_> {
    pub(super) fn float16_round(&mut self) {
        // All argument effects have finished before these scratch slots are used.
        self.code.extend([
            I::LocalTee(self.float_temporary),
            I::F64Abs,
            I::F64Const(OVERFLOW_MIDPOINT.into()),
            I::F64Lt,
            I::If(BlockType::Result(ValType::F64)),
            I::LocalGet(self.float_temporary),
            I::F64Abs,
            I::F64Const(HALF_MIN_NORMAL.into()),
            I::F64Lt,
            I::If(BlockType::Result(ValType::F64)),
            I::LocalGet(self.float_temporary),
            I::F64Const(SUBNORMAL_SCALE.into()),
            I::F64Mul,
            I::F64Nearest,
            I::F64Const((1.0 / SUBNORMAL_SCALE).into()),
            I::F64Mul,
            I::Else,
            I::LocalGet(self.float_temporary),
            I::I64ReinterpretF64,
            I::I64Const(FRACTION_BITS),
            I::I64ShrU,
            I::I32WrapI64,
            I::I32Const(EXPONENT_MASK),
            I::I32And,
            I::LocalSet(self.temporary),
            I::LocalGet(self.float_temporary),
            // x * 2^(10-e) is an exact rescaling to an 11-bit significand.
            I::I32Const(2 * EXPONENT_BIAS + HALF_FRACTION_BITS),
            I::LocalGet(self.temporary),
            I::I32Sub,
            I::I64ExtendI32U,
            I::I64Const(FRACTION_BITS),
            I::I64Shl,
            I::F64ReinterpretI64,
            I::F64Mul,
            I::F64Nearest,
            // Rescale the rounded integer by the exact half-precision step.
            I::LocalGet(self.temporary),
            I::I32Const(HALF_FRACTION_BITS),
            I::I32Sub,
            I::I64ExtendI32U,
            I::I64Const(FRACTION_BITS),
            I::I64Shl,
            I::F64ReinterpretI64,
            I::F64Mul,
            I::End,
            I::Else,
            // NaN propagates, infinities retain sign, and finite overflow maps
            // to signed infinity. Halfway overflow selects infinity (even).
            I::LocalGet(self.float_temporary),
            I::F64Const(f64::INFINITY.into()),
            I::F64Mul,
            I::End,
        ]);
    }
}
