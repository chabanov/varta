//! V38 (`weak_references`, K9 D5): the `varta.weak.*` intrinsics of the
//! weak prelude (`lower::weak`) over ephemeron tables
//! (`strings::weak`).
use super::*;
use ir::WeakOp;
use varta_common::value::ValueKind;
use varta_common::weak as w;

/// The value tags of a weak key in a cell: objects and symbols.
const KEY_TAGS: [ValueKind; 8] = [
    ValueKind::Record,
    ValueKind::Array,
    ValueKind::Error,
    ValueKind::Function,
    ValueKind::Dictionary,
    ValueKind::Promise,
    ValueKind::Bytes,
    ValueKind::Symbol,
];

/// The guest helper an operation calls, if any.
pub(super) fn helper(op: WeakOp) -> Option<strings::Helper> {
    use strings::Helper as H;
    Some(match op {
        WeakOp::Table(_) | WeakOp::Like => H::WeakTable,
        WeakOp::State | WeakOp::SlotHash | WeakOp::Holds | WeakOp::Value => H::WeakPair,
        WeakOp::Put => H::WeakPut,
        WeakOp::Remove => H::WeakRemove,
        WeakOp::Move => H::WeakMove,
        WeakOp::Clear => H::WeakClear,
        WeakOp::Collect => H::WeakGc,
        WeakOp::Word(_) | WeakOp::Hash | WeakOp::Pending | WeakOp::Same => return None,
    })
}

impl Emitter<'_> {
    /// One `varta.weak.*` operation; `ty` is the result type.
    pub(super) fn weak(&mut self, op: WeakOp, args: &[Expr], ty: Type) -> Result<()> {
        match op {
            WeakOp::Table(kind) => {
                self.code.push(I::I32Const(kind as i32));
                self.expr(&args[0])?;
                self.string_helper(strings::Helper::WeakTable)?;
            }
            WeakOp::Like => {
                self.expr(&args[0])?;
                self.code
                    .extend([I::I32WrapI64, I::I32Load(strings::weak::word(w::KIND_WORD))]);
                self.expr(&args[1])?;
                self.string_helper(strings::Helper::WeakTable)?;
            }
            WeakOp::Word(offset) => {
                self.expr(&args[0])?;
                self.code.extend([
                    I::I32WrapI64,
                    I::I32Load(strings::weak::word(offset)),
                    I::F64ConvertI32U,
                ]);
            }
            WeakOp::State => {
                self.pair(args)?;
                // Empty 0, vacated 1, occupied 2.
                self.code.extend([
                    I::I64Load(strings::weak::KEY),
                    I::LocalTee(self.string_temporary),
                    I::I64Const(w::KEY_VACATED as i64 + 1),
                    I::I64LtU,
                    I::If(BlockType::Result(ValType::F64)),
                    I::LocalGet(self.string_temporary),
                    I::F64ConvertI64U,
                    I::Else,
                    I::F64Const((w::KEY_VACATED as f64 + 1.0).into()),
                    I::End,
                ]);
            }
            WeakOp::Hash => {
                self.identity(&args[0])?;
                self.code.extend(strings::weak::hash(self.temporary));
            }
            WeakOp::SlotHash => {
                self.pair(args)?;
                self.code.push(I::I32Load(strings::WORD));
                self.code.extend(strings::weak::hash(self.temporary));
            }
            WeakOp::Holds => {
                self.pair(args)?;
                self.code.push(I::I32Load(strings::WORD));
                self.identity(&args[2])?;
                self.code.push(I::I32Eq);
            }
            WeakOp::Value => {
                self.pair(args)?;
                self.code.push(I::I64Load(strings::weak::VALUE));
                self.unpack_slot(ty);
            }
            WeakOp::Put => {
                self.expr(&args[0])?;
                self.expr(&args[1])?;
                self.identity(&args[2])?;
                self.expr(&args[3])?;
                let value = args[3].ty.erased();
                self.pack_slot(value);
                self.code.push(I::I32Const(i32::from(value.is_handle())));
                self.string_helper(strings::Helper::WeakPut)?;
            }
            WeakOp::Remove | WeakOp::Move | WeakOp::Clear => {
                for arg in args {
                    self.expr(arg)?;
                }
                self.string_helper(helper(op).expect("a table helper"))?;
            }
            WeakOp::Pending => {
                let pending = self
                    .memory
                    .and_then(|memory| memory.weak)
                    .ok_or_else(|| CompilerError::Internal("missing weak globals".into()))?
                    .pending;
                self.code
                    .extend([I::GlobalGet(pending), I::I32Const(0), I::GlobalSet(pending)]);
            }
            WeakOp::Same => {
                self.identity(&args[0])?;
                self.identity(&args[1])?;
                self.code.push(I::I32Eq);
            }
            WeakOp::Collect => self.string_helper(strings::Helper::WeakGc)?,
        }
        Ok(())
    }

    /// `[] -> [i32]`: the pair address of `(table, position)` operands.
    fn pair(&mut self, args: &[Expr]) -> Result<()> {
        self.expr(&args[0])?;
        self.expr(&args[1])?;
        self.string_helper(strings::Helper::WeakPair)
    }

    /// `[] -> [i32]`: whether the tag of the cell whose payload address is
    /// in the temporary names an object or symbol.
    fn key_tag(&mut self) {
        for (at, kind) in KEY_TAGS.into_iter().enumerate() {
            self.code.extend([
                I::LocalGet(self.temporary),
                I::I32Load(strings::WORD),
                I::I32Const(kind as i32),
                I::I32Eq,
            ]);
            if at > 0 {
                self.code.push(I::I32Or);
            }
        }
    }

    /// `[] -> [i32]`: the identity address of a weak key: the handle of an
    /// object or symbol type, or the member of a cell holding one; 0 for any
    /// other value (no identity address is 0, so it holds no pair and is
    /// never the same as a key; the prelude checks CanBeHeldWeakly before it
    /// occupies a pair).
    fn identity(&mut self, value: &Expr) -> Result<()> {
        let ty = value.ty.erased();
        self.expr(value)?;
        if !ty.is_handle() || matches!(ty, Type::String | Type::BigInt) {
            for _ in 0..ty.width() {
                self.code.push(I::Drop);
            }
            self.code.push(I::I32Const(0));
            return Ok(());
        }
        if !ty.is_cell() {
            self.code.push(I::I32WrapI64);
            return Ok(());
        }
        self.code
            .extend([I::I32WrapI64, I::LocalSet(self.temporary)]);
        self.key_tag();
        self.code.extend([
            I::If(BlockType::Result(ValType::I32)),
            I::LocalGet(self.temporary),
            I::I64Load(records::field(0)),
            I::I32WrapI64,
            I::Else,
            I::I32Const(0),
            I::End,
        ]);
        Ok(())
    }
}
