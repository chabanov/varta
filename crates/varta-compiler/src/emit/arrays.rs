use super::*;
use varta_common::value as values;

impl Emitter<'_> {
    pub(super) fn array(&mut self, ty: Type, elements: &[Option<Expr>]) -> Result<()> {
        let Type::Array(id) = ty else {
            return Err(CompilerError::Internal("missing array type".into()));
        };
        let element = self.arrays[id as usize];
        let kind = element
            .profile_kind(self.profile)
            .ok_or_else(|| CompilerError::Internal("missing array element kind".into()))?;
        for value in elements.iter().flatten() {
            self.expr(value)?;
        }
        let capacity = if self.codegen_v2() && self.profile.resizable_arrays() {
            // V38: room for the first pushes, as `push` grows a backing.
            values::array_capacity(elements.len()).max(strings::MIN_APPEND_CAPACITY)
        } else if self.profile.resizable_arrays() {
            values::array_capacity(elements.len())
        } else {
            elements.len()
        };
        let bitmap = values::ARRAY_META_BYTES + capacity * values::ARRAY_SLOT_BYTES;
        let size = bitmap
            + if self.profile.sparse_arrays() {
                varta_common::limits::DataLimits::sparse_bitmap_bytes(capacity)
            } else {
                0
            };
        self.code.push(I::I32Const(size as i32));
        self.string_helper(strings::Helper::Allocate)?;
        self.code.push(I::LocalSet(self.temporary));
        // V38 (`codegen_v2`): small backings store their absent slots and
        // presence words directly, since `memory.fill` is charged per byte.
        let stored = self.codegen_v2() && size <= STORED_BACKING_BYTES;
        if self.profile.resizable_arrays() && !stored {
            self.code.extend([
                I::LocalGet(self.temporary),
                I::I32Const(0),
                I::I32Const(size as i32),
                I::MemoryFill(strings::MEMORY),
            ]);
        }
        self.initialize_value(
            strings::ALLOCATED
                | values::HEAP_ARRAY_FLAG
                | if self.profile.sparse_arrays() {
                    values::HEAP_SPARSE_FLAG
                } else {
                    0
                },
            i32::from(element.needs_root()),
            kind as i32,
            elements.len() as i32,
        );
        for (index, value) in elements.iter().enumerate().rev() {
            let Some(value) = value else {
                continue;
            };
            self.pack_slot(value.ty);
            self.code.extend([
                I::LocalSet(self.string_temporary),
                I::LocalGet(self.temporary),
                I::LocalGet(self.string_temporary),
                I::I64Store(slot(index as u32)),
            ]);
        }
        if self.profile.resizable_arrays() && stored {
            self.store_backing(elements, capacity, bitmap, size);
        } else if self.profile.sparse_arrays() {
            for (index, value) in elements.iter().enumerate() {
                if value.is_some() {
                    let at = wasm_encoder::MemArg {
                        offset: (bitmap + index / u8::BITS as usize) as u64,
                        align: 0,
                        memory_index: strings::MEMORY,
                    };
                    self.code.extend([
                        I::LocalGet(self.temporary),
                        I::LocalGet(self.temporary),
                        I::I32Load8U(at),
                        I::I32Const(1 << (index % u8::BITS as usize)),
                        I::I32Or,
                        I::I32Store8(at),
                    ]);
                }
            }
        }
        self.pack_value(size);
        if self.profile.resizable_arrays() {
            self.string_helper(strings::Helper::RootPush)?;
            self.code.extend([
                I::LocalSet(self.string_temporary),
                I::I32Const(values::VECTOR_BYTES as i32),
            ]);
            self.string_helper(strings::Helper::Allocate)?;
            self.code.push(I::LocalSet(self.temporary));
            self.initialize_value(
                strings::ALLOCATED | values::HEAP_VECTOR_FLAG,
                1,
                kind as i32,
                elements.len() as i32,
            );
            self.code.extend([
                I::LocalGet(self.temporary),
                I::LocalGet(self.string_temporary),
                I::I64Store(slot(0)),
            ]);
            self.pack_value(values::VECTOR_BYTES);
        }
        Ok(())
    }
}

/// V38 (`codegen_v2`): the arguments of `xs.push(...)` when the list is
/// static, has no hole and is short enough to append without an argument
/// array (`Helper::ArrayAppend`); `None` keeps the argument-array path.
pub(super) fn appended(
    profile: varta_common::language::LanguageProfile,
    items: &Expr,
) -> Option<Vec<&Expr>> {
    let E::Array(values) = &items.kind else {
        return None;
    };
    (crate::codegen_v2(profile)
        && profile.resizable_arrays()
        && (1..=strings::MAX_APPENDED).contains(&values.len()))
    .then(|| values.iter().flatten().collect::<Vec<_>>())
    .filter(|present| present.len() == values.len())
}

/// V38: the largest literal backing initialized by word stores (a fixed
/// bitmap and at most 16 slots) instead of `memory.fill`.
const STORED_BACKING_BYTES: usize =
    values::ARRAY_META_BYTES + 16 * values::ARRAY_SLOT_BYTES + values::SPARSE_BITMAP_BYTES;

impl Emitter<'_> {
    /// V38: zero the absent and spare slots of the literal backing in
    /// `temporary` and store its presence bitmap word by word (the present
    /// elements' bits, then zero words). Present slots were stored already.
    fn store_backing(
        &mut self,
        elements: &[Option<Expr>],
        capacity: usize,
        bitmap: usize,
        size: usize,
    ) {
        const WORD_BITS: usize = u64::BITS as usize;
        for index in 0..capacity {
            if elements.get(index).is_none_or(Option::is_none) {
                self.code.extend([
                    I::LocalGet(self.temporary),
                    I::I64Const(0),
                    I::I64Store(slot(index as u32)),
                ]);
            }
        }
        if !self.profile.sparse_arrays() {
            return;
        }
        for (word, offset) in (bitmap..size).step_by(size_of::<u64>()).enumerate() {
            let bits = elements
                .iter()
                .enumerate()
                .skip(word * WORD_BITS)
                .take(WORD_BITS)
                .filter(|(_, value)| value.is_some())
                .fold(0u64, |bits, (index, _)| bits | 1 << (index % WORD_BITS));
            self.code.extend([
                I::LocalGet(self.temporary),
                I::I64Const(bits as i64),
                I::I64Store(wasm_encoder::MemArg {
                    offset: offset as u64,
                    ..strings::ROOT
                }),
            ]);
        }
    }
}

pub(super) fn slot(index: u32) -> wasm_encoder::MemArg {
    wasm_encoder::MemArg {
        offset: (values::ARRAY_META_BYTES + index as usize * values::ARRAY_SLOT_BYTES) as u64,
        ..strings::ROOT
    }
}

impl Emitter<'_> {
    pub(super) fn array_pop(&mut self, base: &Expr, result: Type) -> Result<()> {
        let Type::Array(id) = base.ty else {
            return Err(CompilerError::Internal("missing pop array type".into()));
        };
        let element = self.arrays[id as usize];
        self.expr(base)?;
        self.code.extend([
            I::LocalTee(self.string_temporary),
            I::I32WrapI64,
            I::I32Load(wasm_encoder::MemArg {
                offset: values::ARRAY_LENGTH_OFFSET,
                ..strings::WORD
            }),
            I::I32Eqz,
            I::If(BlockType::Result(wasm_types(result)[0])),
            I::I32Const(0),
        ]);
        if result != Type::Undefined {
            self.box_union(Type::Undefined)?;
        }
        self.code.extend([
            I::Else,
            I::LocalGet(self.string_temporary),
            I::I32WrapI64,
            I::I32Load(wasm_encoder::MemArg {
                offset: values::ARRAY_LENGTH_OFFSET,
                ..strings::WORD
            }),
            I::I32Const(1),
            I::I32Sub,
            I::F64ConvertI32U,
            I::LocalSet(self.float_temporary),
            I::LocalGet(self.string_temporary),
            I::I32WrapI64,
            I::I64Load(slot(0)),
            I::I32WrapI64,
            I::LocalSet(self.temporary),
        ]);
        if self.profile.sparse_arrays() {
            self.code.extend([
                I::LocalGet(self.string_temporary),
                I::LocalGet(self.float_temporary),
                I::I32TruncF64U,
            ]);
            self.string_helper(strings::Helper::ArrayPresent)?;
        }
        // V38 (`incremental_collection`): the removed element is shaded.
        if element.needs_root()
            && let Some(barrier) = self.barrier()
        {
            self.code
                .extend([I::GlobalGet(barrier.marking), I::If(BlockType::Empty)]);
            self.pop_slot_address();
            self.code
                .extend([I::I64Load(strings::ROOT), I::Call(barrier.shade), I::End]);
        }
        // Preserve the removed payload on the Wasm stack; no allocation occurs
        // until boxing roots its actual reference, after clearing the slot.
        self.pop_slot_address();
        self.code.push(I::I64Load(strings::ROOT));
        self.pop_slot_address();
        self.code
            .extend([I::I64Const(0), I::I64Store(strings::ROOT)]);
        if self.profile.sparse_arrays() {
            self.code.extend([
                I::LocalGet(self.string_temporary),
                I::LocalGet(self.float_temporary),
                I::I32TruncF64U,
                I::I32Const(0),
            ]);
            self.string_helper(strings::Helper::ArraySetPresent)?;
        }
        for cell in [true, false] {
            self.code.push(if cell {
                I::LocalGet(self.string_temporary)
            } else {
                I::LocalGet(self.temporary)
            });
            if cell {
                self.code.push(I::I32WrapI64);
            }
            self.code.extend([
                I::LocalGet(self.float_temporary),
                I::I32TruncF64U,
                I::I32Store(wasm_encoder::MemArg {
                    offset: values::ARRAY_LENGTH_OFFSET,
                    ..strings::WORD
                }),
            ]);
        }
        if self.profile.sparse_arrays() {
            self.code.extend([
                I::LocalSet(self.string_temporary),
                I::If(BlockType::Result(wasm_types(result)[0])),
                I::LocalGet(self.string_temporary),
            ]);
        }
        if !element.is_cell() {
            self.unpack_slot(element);
            if result != Type::Undefined {
                self.box_union(element)?;
            }
        }
        if self.profile.sparse_arrays() {
            self.code.extend([I::Else, I::I32Const(0)]);
            if result != Type::Undefined {
                self.box_union(Type::Undefined)?;
            }
            self.code.push(I::End);
        }
        self.code.push(I::End);
        Ok(())
    }
    fn pop_slot_address(&mut self) {
        self.code.extend([
            I::LocalGet(self.temporary),
            I::LocalGet(self.float_temporary),
            I::I32TruncF64U,
            I::I32Const(values::ARRAY_SLOT_BYTES as i32),
            I::I32Mul,
            I::I32Add,
            I::I32Const(values::ARRAY_META_BYTES as i32),
            I::I32Add,
        ]);
    }
}

impl Emitter<'_> {
    /// The caller has checked that every union member is Number or Undefined.
    /// Leave earlier receiver/bound operands on the Wasm stack across effects.
    pub(super) fn array_bound(&mut self, value: &Expr, default: f64) -> Result<()> {
        self.expr(value)?;
        if value.ty == Type::Undefined {
            self.code.extend([I::Drop, I::F64Const(default.into())]);
        } else if value.ty.is_cell() {
            self.code.extend([
                I::I32WrapI64,
                I::LocalTee(self.temporary),
                I::I32Load(strings::WORD),
                I::I32Const(values::ValueKind::Undefined as i32),
                I::I32Eq,
                I::If(BlockType::Result(ValType::F64)),
                I::F64Const(default.into()),
                I::Else,
                I::LocalGet(self.temporary),
                I::I64Load(slot(0)),
                I::F64ReinterpretI64,
                I::End,
            ]);
        }
        Ok(())
    }
}

impl Emitter<'_> {
    /// Allocate the final-length sparse map output once. All payloads/bits are
    /// initially absent; successful callbacks publish one existing slot at a time.
    pub(super) fn array_reserve(
        &mut self,
        ty: Type,
        length: &Expr,
        count: u32,
        bytes: u32,
    ) -> Result<()> {
        let Type::Array(id) = ty else {
            return Err(CompilerError::Internal(
                "missing reserved array type".into(),
            ));
        };
        let element = self.arrays[id as usize];
        let kind = element
            .profile_kind(self.profile)
            .ok_or_else(|| CompilerError::Internal("missing reserved element kind".into()))?;
        let count = self.local_slots[count as usize];
        let bytes = self.local_slots[bytes as usize];
        self.expr(length)?;
        self.code.extend([
            I::LocalTee(self.float_temporary),
            I::F64Trunc,
            I::LocalGet(self.float_temporary),
            I::F64Ne,
            I::LocalGet(self.float_temporary),
            I::F64Const(0.0.into()),
            I::F64Lt,
            I::I32Or,
            I::LocalGet(self.float_temporary),
            I::F64Const((self.profile.data().array_elements as f64).into()),
            I::F64Gt,
            I::I32Or,
        ]);
        self.trap_if();
        self.code.extend([
            I::LocalGet(self.float_temporary),
            I::I32TruncF64U,
            I::LocalSet(count),
        ]);
        self.code.extend([
            I::LocalGet(count),
            I::I32Eqz,
            I::If(BlockType::Result(ValType::I32)),
            I::I32Const(0),
            I::Else,
            I::I32Const(1),
            I::I32Const(i32::BITS as i32),
            I::LocalGet(count),
            I::I32Const(1),
            I::I32Sub,
            I::I32Clz,
            I::I32Sub,
            I::I32Shl,
            I::End,
        ]);
        let sizes = strings::Sizes::new(self.profile.data());
        if sizes.large {
            // The capacity, kept in `bytes` until its backing size replaces it.
            self.code.push(I::LocalSet(bytes));
            self.code.extend(sizes.backing_bytes(&[I::LocalGet(bytes)]));
        } else {
            self.code.extend([
                I::I32Const(values::ARRAY_SLOT_BYTES as i32),
                I::I32Mul,
                I::I32Const((values::ARRAY_META_BYTES + values::SPARSE_BITMAP_BYTES) as i32),
                I::I32Add,
            ]);
        }
        self.code.push(I::LocalTee(bytes));
        self.string_helper(strings::Helper::Allocate)?;
        self.code.extend([
            I::LocalSet(self.temporary),
            I::LocalGet(self.temporary),
            I::I32Const(0),
            I::LocalGet(bytes),
            I::MemoryFill(strings::MEMORY),
        ]);
        self.initialize_value(
            strings::ALLOCATED | values::HEAP_ARRAY_FLAG | values::HEAP_SPARSE_FLAG,
            i32::from(element.needs_root()),
            kind as i32,
            0,
        );
        self.code.extend([
            I::LocalGet(self.temporary),
            I::LocalGet(count),
            I::I32Store(wasm_encoder::MemArg {
                offset: values::ARRAY_LENGTH_OFFSET,
                ..strings::WORD
            }),
            I::LocalGet(self.temporary),
            I::I64ExtendI32U,
            I::LocalGet(bytes),
            I::I64ExtendI32U,
            I::I64Const(strings::LENGTH_SHIFT as i64),
            I::I64Shl,
            I::I64Or,
        ]);
        self.string_helper(strings::Helper::RootPush)?;
        self.code.extend([
            I::LocalSet(self.string_temporary),
            I::I32Const(values::VECTOR_BYTES as i32),
        ]);
        self.string_helper(strings::Helper::Allocate)?;
        self.code.push(I::LocalSet(self.temporary));
        self.initialize_value(
            strings::ALLOCATED | values::HEAP_VECTOR_FLAG,
            1,
            kind as i32,
            0,
        );
        self.code.extend([
            I::LocalGet(self.temporary),
            I::LocalGet(count),
            I::I32Store(wasm_encoder::MemArg {
                offset: values::ARRAY_LENGTH_OFFSET,
                ..strings::WORD
            }),
            I::LocalGet(self.temporary),
            I::LocalGet(self.string_temporary),
            I::I64Store(slot(0)),
        ]);
        self.pack_value(values::VECTOR_BYTES);
        Ok(())
    }
}
