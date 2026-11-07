//! Integrity-level header bits (v38 `integrity_levels`, K9 D2): the
//! [`ir::Intrinsic::Integrity`] operations on an object block's header flags
//! (`varta_common::value::HEAP_INTEGRITY_FLAGS`). The bits live in the block
//! the handle's low word addresses (a record, class instance, vector cell or
//! array, dictionary, error or promise); a union or `unknown` cell forwards
//! to its object payload, and a primitive payload has none.
use super::*;
use varta_common::value::{self as values, ValueKind};

/// The cell kinds whose payload is an object block that carries the bits.
const OBJECT_KINDS: i32 = 1 << ValueKind::Record as i32
    | 1 << ValueKind::Array as i32
    | 1 << ValueKind::Error as i32
    | 1 << ValueKind::Dictionary as i32
    | 1 << ValueKind::Promise as i32;
/// A cell's payload word.
const PAYLOAD: wasm_encoder::MemArg = wasm_encoder::MemArg {
    offset: values::UNION_META_BYTES as u64,
    align: 3,
    memory_index: strings::MEMORY,
};
const HEADER: i32 = varta_common::abi::STRING_HEADER_BYTES as i32;

impl Emitter<'_> {
    /// `[i64] -> [i64]` for [`ir::IntegrityOp::Set`], `[i64] -> [i32]` for
    /// [`ir::IntegrityOp::Test`]; `cell` when the operand is a union or
    /// `unknown` cell.
    pub(super) fn integrity(&mut self, op: ir::IntegrityOp, cell: bool) -> Result<()> {
        let set = match op {
            ir::IntegrityOp::Set(bits) => Ok(bits),
            ir::IntegrityOp::Test(mask) => Err(mask),
            ir::IntegrityOp::Level { .. } => {
                return Err(CompilerError::Internal(
                    "integrity level test reached emission unexpanded".into(),
                ));
            }
        };
        if !cell {
            match set {
                Ok(bits) => {
                    self.code
                        .extend([I::LocalTee(self.string_temporary), I::I32WrapI64]);
                    self.set_integrity(bits);
                    self.code.push(I::LocalGet(self.string_temporary));
                }
                Err(mask) => {
                    self.code.push(I::I32WrapI64);
                    self.test_integrity(mask);
                }
            }
            return Ok(());
        }
        // The header of the payload object, or no operation for a primitive
        // payload.
        self.code.extend([
            I::LocalTee(self.string_temporary),
            I::I32WrapI64,
            I::LocalSet(self.temporary),
            I::I32Const(1),
            I::LocalGet(self.temporary),
            I::I32Load(strings::WORD),
            I::I32Shl,
            I::I32Const(OBJECT_KINDS),
            I::I32And,
        ]);
        let block = match set {
            Ok(_) => BlockType::Empty,
            Err(_) => BlockType::Result(ValType::I32),
        };
        self.code.extend([
            I::If(block),
            I::LocalGet(self.temporary),
            I::I64Load(PAYLOAD),
            I::I32WrapI64,
        ]);
        match set {
            Ok(bits) => {
                self.set_integrity(bits);
                self.code
                    .extend([I::End, I::LocalGet(self.string_temporary)]);
            }
            Err(mask) => {
                self.test_integrity(mask);
                self.code.extend([I::Else, I::I32Const(0), I::End]);
            }
        }
        Ok(())
    }
    /// `[i32 payload] -> []`: add `bits` to the block's header flags.
    fn set_integrity(&mut self, bits: u32) {
        self.code.extend([
            I::I32Const(HEADER),
            I::I32Sub,
            I::LocalTee(self.temporary),
            I::LocalGet(self.temporary),
            I::I32Load(strings::FLAG),
            I::I32Const(bits as i32),
            I::I32Or,
            I::I32Store(strings::FLAG),
        ]);
    }
    /// `[i32 payload] -> [i32]`: whether the block's header flags have any of
    /// `mask`.
    fn test_integrity(&mut self, mask: u32) {
        self.code.extend([
            I::I32Const(HEADER),
            I::I32Sub,
            I::I32Load(strings::FLAG),
            I::I32Const(mask as i32),
            I::I32And,
            I::I32Const(0),
            I::I32Ne,
        ]);
    }
}
