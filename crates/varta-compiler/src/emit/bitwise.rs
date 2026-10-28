//! Inline ToUint32 uses binary64 bits, never a trapping float-to-integer cast.
use super::*;

const FRACTION_BITS: i32 = 52;
const EXPONENT_BIAS: i32 = 1023;
const EXPONENT_MASK: i64 = 0x7ff;
const FRACTION_MASK: i64 = (1_i64 << FRACTION_BITS) - 1;
const HIDDEN_BIT: i64 = 1_i64 << FRACTION_BITS;
const INTEGER_BITS: i32 = i32::BITS as i32;
const INTEGER_EXPONENT: i32 = EXPONENT_BIAS + FRACTION_BITS;
const FIRST_ZERO_EXPONENT: i32 = INTEGER_EXPONENT + INTEGER_BITS;

impl Emitter<'_> {
    pub(super) fn float_bitwise(&mut self, op: BinaryOp) -> Result<()> {
        // Both user operands are already evaluated. No calls/allocations run
        // while reusing these scratch slots, including in nested expressions.
        self.code.push(I::LocalSet(self.float_temporary));
        self.number_to_uint32();
        self.code.push(I::LocalGet(self.float_temporary));
        self.number_to_uint32();
        self.code.push(match op {
            BinaryOp::BitAnd => I::I32And,
            BinaryOp::BitOr => I::I32Or,
            BinaryOp::BitXor => I::I32Xor,
            BinaryOp::LShift => I::I32Shl,
            BinaryOp::RShift => I::I32ShrS,
            BinaryOp::ZeroFillRShift => I::I32ShrU,
            _ => {
                return Err(CompilerError::Internal(
                    "unlowered Number bitwise operator".into(),
                ));
            }
        });
        // Wasm masks shift counts to five bits. Only >>> returns an unsigned
        // Number; every result still occupies the ordinary binary64 slot.
        self.code.push(if op == BinaryOp::ZeroFillRShift {
            I::F64ConvertI32U
        } else {
            I::F64ConvertI32S
        });
        Ok(())
    }
    fn significand(&mut self) {
        self.code.extend([
            I::LocalGet(self.string_temporary),
            I::I64Const(FRACTION_MASK),
            I::I64And,
            I::I64Const(HIDDEN_BIT),
            I::I64Or,
        ]);
    }
    pub(super) fn number_to_uint32(&mut self) {
        self.code.extend([
            I::I64ReinterpretF64,
            I::LocalTee(self.string_temporary),
            I::I64Const(i64::from(FRACTION_BITS)),
            I::I64ShrU,
            I::I64Const(EXPONENT_MASK),
            I::I64And,
            I::I32WrapI64,
            I::LocalSet(self.temporary),
            I::LocalGet(self.temporary),
            I::I32Const(EXPONENT_BIAS),
            I::I32GeU,
            I::LocalGet(self.temporary),
            I::I32Const(FIRST_ZERO_EXPONENT),
            I::I32LtU,
            I::I32And,
            I::If(BlockType::Result(ValType::I32)),
            I::LocalGet(self.temporary),
            I::I32Const(INTEGER_EXPONENT),
            I::I32GeU,
            I::If(BlockType::Result(ValType::I64)),
        ]);
        self.significand();
        self.code.extend([
            I::LocalGet(self.temporary),
            I::I32Const(INTEGER_EXPONENT),
            I::I32Sub,
            I::I64ExtendI32U,
            I::I64Shl,
            I::Else,
        ]);
        self.significand();
        self.code.extend([
            I::I32Const(INTEGER_EXPONENT),
            I::LocalGet(self.temporary),
            I::I32Sub,
            I::I64ExtendI32U,
            I::I64ShrU,
            I::End,
            I::I32WrapI64,
            I::LocalGet(self.string_temporary),
            I::I64Const(0),
            I::I64LtS,
            I::If(BlockType::Result(ValType::I32)),
            I::I32Const(-1),
            I::Else,
            I::I32Const(1),
            I::End,
            I::I32Mul,
            I::Else,
            I::I32Const(0),
            I::End,
        ]);
        // Encoded exponents below the bias truncate to zero. At/above the
        // upper bound every finite value is a multiple of 2^32; NaN/Infinity
        // also take this zero branch. The remaining shifts are bounded.
    }
}
