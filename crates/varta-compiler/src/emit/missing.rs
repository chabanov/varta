use super::*;
use varta_common::value as values;
/// Bytes of one insertion-order word of an optional-field record.
const ORDER_WORD_BYTES: i32 = size_of::<u32>() as i32;
impl Emitter<'_> {
    pub(super) fn record_delete(
        &mut self,
        object: &Expr,
        absent: &Expr,
        slot: u32,
        scan: u32,
        write: u32,
        count: u32,
    ) -> Result<()> {
        let Type::Record(shape) = object.ty else {
            return Err(CompilerError::Internal(
                "missing delete record shape".into(),
            ));
        };
        let fields = self.records[shape as usize].fields.len();
        let (word, bit) = values::presence_bit(fields, slot as usize);
        let presence = wasm_encoder::MemArg {
            offset: word as u64,
            ..strings::WORD
        };
        let order = values::order_offset(fields) as u64;
        let scan = self.local_slots[scan as usize];
        let write = self.local_slots[write as usize];
        let count = self.local_slots[count as usize];
        self.expr(object)?;
        self.expr(absent)?;
        self.pack_slot(absent.ty);
        self.code.extend([
            I::LocalSet(self.string_temporary),
            I::I32WrapI64,
            I::LocalSet(self.temporary),
            I::LocalGet(self.temporary),
            I::LocalGet(self.string_temporary),
            I::I64Store(records::field(slot)),
        ]);
        self.present_count(&[I::LocalGet(self.temporary)], fields);
        self.code.extend([
            I::LocalSet(count),
            I::I32Const(0),
            I::LocalSet(scan),
            I::I32Const(0),
            I::LocalSet(write),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(scan),
            I::LocalGet(count),
            I::I32GeU,
            I::BrIf(1),
            I::LocalGet(self.temporary),
            I::LocalGet(scan),
            I::I32Const(ORDER_WORD_BYTES),
            I::I32Mul,
            I::I32Add,
            I::I32Load(wasm_encoder::MemArg {
                offset: order,
                ..strings::WORD
            }),
            I::I32Const(slot as i32),
            I::I32Ne,
            I::If(BlockType::Empty),
            I::LocalGet(self.temporary),
            I::LocalGet(write),
            I::I32Const(ORDER_WORD_BYTES),
            I::I32Mul,
            I::I32Add,
            I::LocalGet(self.temporary),
            I::LocalGet(scan),
            I::I32Const(ORDER_WORD_BYTES),
            I::I32Mul,
            I::I32Add,
            I::I32Load(wasm_encoder::MemArg {
                offset: order,
                ..strings::WORD
            }),
            I::I32Store(wasm_encoder::MemArg {
                offset: order,
                ..strings::WORD
            }),
            I::LocalGet(write),
            I::I32Const(1),
            I::I32Add,
            I::LocalSet(write),
            I::End,
            I::LocalGet(scan),
            I::I32Const(1),
            I::I32Add,
            I::LocalSet(scan),
            I::Br(0),
            I::End,
            I::End,
            I::LocalGet(self.temporary),
            I::LocalGet(write),
            I::I32Const(ORDER_WORD_BYTES),
            I::I32Mul,
            I::I32Add,
            I::I32Const(0),
            I::I32Store(wasm_encoder::MemArg {
                offset: order,
                ..strings::WORD
            }),
            I::LocalGet(self.temporary),
            I::LocalGet(self.temporary),
            I::I32Load(presence),
            I::I32Const(!bit as i32),
            I::I32And,
            I::I32Store(presence),
            I::I32Const(1),
        ]);
        Ok(())
    }
    pub(super) fn has_own(&mut self, value: &Expr, slot: Option<u32>) -> Result<()> {
        self.expr(value)?;
        if let Some(slot) = slot {
            let Type::Record(id) = value.ty else {
                return Err(CompilerError::Internal("missing record shape".into()));
            };
            let (offset, bit) =
                values::presence_bit(self.records[id as usize].fields.len(), slot as usize);
            self.code.extend([
                I::I32WrapI64,
                I::I32Load(wasm_encoder::MemArg {
                    offset: offset as u64,
                    ..strings::WORD
                }),
                I::I32Const(bit as i32),
                I::I32And,
                I::I32Eqz,
                I::I32Eqz,
            ]);
        } else {
            self.code.extend([I::Drop, I::I32Const(0)]);
        }
        Ok(())
    }
    /// The setter has completed; temporary still contains the object's pointer.
    pub(super) fn mark_present(&mut self, ty: Type, slot: u32) {
        let Type::Record(id) = ty else {
            return;
        };
        let count = self.records[id as usize].fields.len();
        let (word, bit) = values::presence_bit(count, slot as usize);
        let presence = wasm_encoder::MemArg {
            offset: word as u64,
            ..strings::WORD
        };
        let bit = bit as i32;
        self.code.extend([
            I::LocalGet(self.temporary),
            I::I32Load(presence),
            I::I32Const(bit),
            I::I32And,
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::LocalGet(self.temporary),
            I::I32Const(values::order_offset(count) as i32),
            I::I32Add,
        ]);
        self.present_count(&[I::LocalGet(self.temporary)], count);
        self.code.extend([
            I::I32Const(ORDER_WORD_BYTES),
            I::I32Mul,
            I::I32Add,
            I::I32Const(slot as i32),
            I::I32Store(strings::WORD),
            I::LocalGet(self.temporary),
            I::LocalGet(self.temporary),
            I::I32Load(presence),
            I::I32Const(bit),
            I::I32Or,
            I::I32Store(presence),
            I::End,
        ]);
    }
    pub(super) fn maybe_index(
        &mut self,
        base: &Expr,
        index: &Expr,
        result: Type,
        relative: bool,
    ) -> Result<()> {
        // Keep the first operand on the stack until the second finishes: its
        // nested calls may use every scratch local and trigger collection.
        self.expr(base)?;
        if relative {
            self.array_bound(index, 0.0)?;
        } else {
            self.expr(index)?;
        }
        self.code.extend([
            I::LocalSet(self.float_temporary),
            I::LocalTee(self.string_temporary),
        ]);
        let element = if base.ty == Type::String {
            self.code.extend([
                I::I64Const(strings::LENGTH_SHIFT as i64),
                I::I64ShrU,
                I::I32WrapI64,
                I::I32Const(1),
                I::I32ShrU,
            ]);
            Type::String
        } else {
            let Type::Array(id) = base.ty else {
                return Err(CompilerError::Internal("missing indexed type".into()));
            };
            self.code.extend([
                I::I32WrapI64,
                I::I32Load(wasm_encoder::MemArg {
                    offset: values::ARRAY_LENGTH_OFFSET,
                    ..strings::WORD
                }),
            ]);
            self.arrays[id as usize]
        };
        self.code.push(I::LocalSet(self.temporary));
        if relative {
            self.code.extend([
                I::LocalGet(self.float_temporary),
                I::LocalGet(self.float_temporary),
                I::F64Ne,
                I::If(BlockType::Result(ValType::F64)),
                I::F64Const(0.0.into()),
                I::Else,
                I::LocalGet(self.float_temporary),
                I::F64Trunc,
                I::End,
                I::LocalSet(self.float_temporary),
                I::LocalGet(self.float_temporary),
                I::F64Const(0.0.into()),
                I::F64Lt,
                I::If(BlockType::Result(ValType::F64)),
                I::LocalGet(self.float_temporary),
                I::LocalGet(self.temporary),
                I::F64ConvertI32U,
                I::F64Add,
                I::Else,
                I::LocalGet(self.float_temporary),
                I::End,
                I::LocalSet(self.float_temporary),
            ]);
        }
        self.code.extend([
            I::LocalGet(self.float_temporary),
            I::F64Trunc,
            I::LocalGet(self.float_temporary),
            I::F64Ne,
            I::LocalGet(self.float_temporary),
            I::F64Const(0.0.into()),
            I::F64Lt,
            I::I32Or,
            I::LocalGet(self.float_temporary),
            I::LocalGet(self.temporary),
            I::F64ConvertI32U,
            I::F64Ge,
            I::I32Or,
        ]);
        if self.profile.sparse_arrays() && matches!(base.ty, Type::Array(_)) {
            self.code.extend([
                I::If(BlockType::Result(ValType::I32)),
                I::I32Const(1),
                I::Else,
                I::LocalGet(self.string_temporary),
                I::LocalGet(self.float_temporary),
                I::I32TruncF64U,
            ]);
            self.string_helper(strings::Helper::ArrayPresent)?;
            self.code.extend([I::I32Eqz, I::End]);
        }
        self.code.extend([
            I::If(BlockType::Result(wasm_types(result)[0])),
            I::I32Const(0),
        ]);
        if result != Type::Undefined {
            self.box_union(Type::Undefined)?;
        }
        self.code.push(I::Else);
        if base.ty == Type::String && self.codegen_v2() {
            // V38: the one `charAt` path (ASCII fast path, then the linked
            // intrinsic).
            self.code.extend([
                I::LocalGet(self.string_temporary),
                I::LocalGet(self.float_temporary),
            ]);
            self.string_helper(strings::Helper::CharAt)?;
            self.box_union(Type::String)?;
        } else if base.ty == Type::String {
            self.code.extend([
                I::LocalGet(self.string_temporary),
                I::I32WrapI64,
                I::LocalGet(self.string_temporary),
                I::I64Const(strings::LENGTH_SHIFT as i64),
                I::I64ShrU,
                I::I32WrapI64,
                I::LocalGet(self.float_temporary),
                I::Call(self.hosts[&Host::StringCharAt16]),
                I::GlobalGet(self.result_len),
            ]);
            self.string_helper(strings::Helper::Snapshot)?;
            self.box_union(Type::String)?;
        } else {
            self.code
                .extend([I::LocalGet(self.string_temporary), I::I32WrapI64]);
            if self.profile.resizable_arrays() {
                self.code
                    .extend([I::I64Load(arrays::slot(0)), I::I32WrapI64]);
            }
            self.code.extend([
                I::I32Const(values::ARRAY_META_BYTES as i32),
                I::I32Add,
                I::LocalGet(self.float_temporary),
                I::I32TruncF64U,
                I::I32Const(values::ARRAY_SLOT_BYTES as i32),
                I::I32Mul,
                I::I32Add,
                I::I64Load(strings::ROOT),
            ]);
            if !element.is_cell() {
                self.unpack_slot(element);
                if result != Type::Undefined {
                    self.box_union(element)?;
                }
            }
        }
        self.code.push(I::End);
        Ok(())
    }
}

impl Emitter<'_> {
    pub(super) fn array_has_own(&mut self, base: &Expr, index: &Expr) -> Result<()> {
        self.expr(base)?;
        self.expr(index)?;
        self.code.extend([
            I::LocalSet(self.float_temporary),
            I::LocalSet(self.string_temporary),
            I::LocalGet(self.float_temporary),
            I::F64Trunc,
            I::LocalGet(self.float_temporary),
            I::F64Ne,
            I::LocalGet(self.float_temporary),
            I::F64Const(0.0.into()),
            I::F64Lt,
            I::I32Or,
            I::LocalGet(self.float_temporary),
            I::F64Const((self.profile.data().array_elements as f64).into()),
            I::F64Ge,
            I::I32Or,
            I::If(BlockType::Result(ValType::I32)),
            I::I32Const(0),
            I::Else,
            I::LocalGet(self.string_temporary),
            I::LocalGet(self.float_temporary),
            I::I32TruncF64U,
        ]);
        self.string_helper(strings::Helper::ArrayPresent)?;
        self.code.push(I::End);
        Ok(())
    }
}
