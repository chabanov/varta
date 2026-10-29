use super::*;
use varta_common::value as abi;

pub(super) type LayoutKey = (u32, Vec<u32>);
pub(super) fn layout_key(id: u32, fields: &[(u32, Expr)], shapes: &[ir::RecordShape]) -> LayoutKey {
    let mut order: Vec<_> = fields
        .iter()
        .enumerate()
        .map(|(position, (index, _))| {
            let name = &shapes[id as usize].fields[*index as usize].name;
            (enumeration_key(name, position), *index)
        })
        .collect();
    order.sort_by_key(|(key, _)| *key);
    (id, order.into_iter().map(|(_, index)| index).collect())
}

fn enumeration_key(name: &str, position: usize) -> (bool, u32) {
    let integer = name
        .parse::<u32>()
        .ok()
        .filter(|n| *n < u32::MAX && n.to_string() == name);
    (integer.is_none(), integer.unwrap_or(position as u32))
}
pub(super) fn canonical_layout(id: u32, shapes: &[ir::RecordShape]) -> LayoutKey {
    let fields = &shapes[id as usize].fields;
    let mut order: Vec<_> = (0..fields.len() as u32).collect();
    order.sort_by_key(|slot| enumeration_key(&fields[*slot as usize].name, *slot as usize));
    (id, order)
}

pub(super) fn descriptor(
    key: &LayoutKey,
    shapes: &[ir::RecordShape],
    strings: &BTreeMap<Vec<u8>, u32>,
    version: u32,
) -> Result<Vec<u8>> {
    let shape = &shapes[key.0 as usize];
    let mut bytes = Vec::new();
    for value in [version, key.1.len() as u32] {
        bytes.extend(value.to_le_bytes());
    }
    for index in &key.1 {
        let field = &shape.fields[*index as usize];
        let kind = field
            .ty
            .layout_kind(version)
            .ok_or_else(|| CompilerError::Internal("missing object field kind".into()))?;
        for value in [
            strings[field.name.as_bytes()],
            field.name.len() as u32,
            *index
                | if matches!(
                    version,
                    abi::MISSING_VALUE_VERSION
                        | abi::REFERENCE_VALUE_VERSION
                        | abi::VECTOR_VALUE_VERSION
                        | abi::ERROR_VALUE_VERSION
                        | abi::ERROR_FAMILY_VALUE_VERSION
                        | abi::SPARSE_VALUE_VERSION
                        | abi::BIGINT_VALUE_VERSION
                        | abi::DYNAMIC_VALUE_VERSION
                        | abi::PROMISE_VALUE_VERSION
                ) && field.optional
                {
                    abi::OPTIONAL_FIELD_FLAG
                } else {
                    0
                },
            kind as u32,
        ] {
            bytes.extend(value.to_le_bytes());
        }
    }
    // Value15 wide records: the collector reads this slot-indexed edge bitmap
    // instead of a 32-bit link-word mask.
    let count = shape.fields.len();
    if abi::is_wide_record(count) {
        let edges = edge_set(shape);
        for word in 0..abi::presence_words(count) {
            bytes.extend(((edges >> (word * abi::PRESENCE_WORD_BITS)) as u32).to_le_bytes());
        }
    }
    Ok(bytes)
}
/// Slots holding managed references, indexed by canonical slot.
fn edge_set(shape: &ir::RecordShape) -> u128 {
    shape
        .fields
        .iter()
        .enumerate()
        .fold(0u128, |mask, (slot, field)| {
            if field.ty.needs_root() {
                mask | 1 << slot
            } else {
                mask
            }
        })
}

impl Emitter<'_> {
    pub(super) fn record(
        &mut self,
        ty: Type,
        fields: &[(u32, Expr)],
        present: Option<u128>,
    ) -> Result<()> {
        let id = match ty {
            Type::Record(id) => id,
            Type::Dictionary(id) => self.dictionaries[id as usize].storage,
            _ => return Err(CompilerError::Internal("missing object shape".into())),
        };
        for (_, value) in fields {
            self.expr(value)?;
        }
        let size = abi::record_bytes(self.profile.value_layout(), fields.len());
        self.code.push(I::I32Const(size as i32));
        self.string_helper(strings::Helper::Allocate)?;
        self.code.push(I::LocalSet(self.temporary));
        // A wide record's link word stays zero: its edges trail the descriptor.
        let mask = if abi::is_wide_record(fields.len()) {
            0
        } else {
            edge_set(&self.records[id as usize]) as u32
        };
        self.initialize_value(
            strings::ALLOCATED | abi::HEAP_RECORD_FLAG,
            mask as i32,
            self.layouts[&layout_key(id, fields, self.records)] as i32,
            fields.len() as i32,
        );
        for (index, value) in fields.iter().rev() {
            self.pack_slot(value.ty);
            self.code.extend([
                I::LocalSet(self.string_temporary),
                I::LocalGet(self.temporary),
                I::LocalGet(self.string_temporary),
                I::I64Store(field(*index)),
            ]);
        }
        if self.profile.missing() {
            let count = fields.len();
            let present = present.unwrap_or_else(|| abi::wide_field_mask(count));
            let offset = abi::presence_offset(count);
            for word in 0..abi::presence_words(count) {
                self.code.extend([
                    I::LocalGet(self.temporary),
                    I::I32Const((present >> (word * abi::PRESENCE_WORD_BITS)) as u32 as i32),
                    I::I32Store(wasm_encoder::MemArg {
                        offset: (offset + word * size_of::<u32>()) as u64,
                        ..strings::WORD
                    }),
                ]);
            }
            let order_offset = abi::order_offset(count);
            let order = layout_key(id, fields, self.records)
                .1
                .into_iter()
                .filter(|slot| present & (1u128 << slot) != 0)
                .chain(std::iter::repeat(0))
                .take(count);
            for (index, slot) in order.enumerate() {
                self.code.extend([
                    I::LocalGet(self.temporary),
                    I::I32Const(slot as i32),
                    I::I32Store(wasm_encoder::MemArg {
                        offset: (order_offset + index * 4) as u64,
                        ..strings::WORD
                    }),
                ]);
            }
        }
        self.pack_value(size);
        Ok(())
    }
    /// Push the number of present slots of a `count`-field record whose
    /// payload pointer `load` pushes (one popcount per presence word).
    pub(super) fn present_count(&mut self, load: &[I<'static>], count: usize) {
        let base = abi::presence_offset(count);
        for word in 0..abi::presence_words(count) {
            self.code.extend(load.iter().cloned());
            self.code.extend([
                I::I32Load(wasm_encoder::MemArg {
                    offset: (base + word * size_of::<u32>()) as u64,
                    ..strings::WORD
                }),
                I::I32Popcnt,
            ]);
            if word > 0 {
                self.code.push(I::I32Add);
            }
        }
    }
    pub(super) fn initialize_value(&mut self, flags: i32, link: i32, meta: i32, count: i32) {
        self.code.extend([
            I::LocalGet(self.temporary),
            I::I32Const(varta_common::abi::STRING_HEADER_BYTES as i32),
            I::I32Sub,
            I::I32Const(flags),
            I::I32Store(strings::FLAG),
            I::LocalGet(self.temporary),
            I::I32Const(varta_common::abi::STRING_HEADER_BYTES as i32),
            I::I32Sub,
            I::I32Const(link),
            I::I32Store(strings::LINK),
            I::LocalGet(self.temporary),
            I::I32Const(meta),
            I::I32Store(strings::WORD),
            I::LocalGet(self.temporary),
            I::I32Const(count),
            I::I32Store(wasm_encoder::MemArg {
                offset: 4,
                ..strings::WORD
            }),
        ]);
    }
    pub(super) fn pack_value(&mut self, size: usize) {
        self.code.extend([
            I::LocalGet(self.temporary),
            I::I64ExtendI32U,
            I::I64Const((size as i64) << strings::LENGTH_SHIFT),
            I::I64Or,
        ]);
    }
    pub(super) fn json(&mut self, value: &Expr, result: Type, status: Option<u32>) -> Result<()> {
        let kind = value
            .ty
            .profile_kind(self.profile)
            .ok_or_else(|| CompilerError::Internal("missing JSON value kind".into()))?;
        self.code.push(I::I32Const(kind as i32));
        self.expr(value)?;
        self.pack_slot(value.ty);
        self.code.extend([
            I::LocalTee(self.string_temporary),
            I::I32WrapI64,
            I::LocalGet(self.string_temporary),
            I::I64Const(strings::LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I32WrapI64,
            I::Call(self.hosts[&self.profile.json_stringify()]),
        ]);
        if let Some(status) = status {
            self.code.extend([
                I::LocalSet(self.local_slots[status as usize]),
                I::LocalSet(self.temporary),
                I::LocalGet(self.local_slots[status as usize]),
                I::I32Eqz,
                I::If(BlockType::Result(if result == Type::Undefined {
                    ValType::I32
                } else {
                    ValType::I64
                })),
                I::LocalGet(self.temporary),
            ]);
            self.json_result(result)?;
            self.code.push(I::Else);
            self.code.push(if result == Type::Undefined {
                I::I32Const(0)
            } else {
                I::I64Const(0)
            });
            self.code.push(I::End);
            Ok(())
        } else {
            self.json_result(result)
        }
    }
    fn json_result(&mut self, result: Type) -> Result<()> {
        if result.is_cell() {
            self.code.extend([
                I::LocalSet(self.temporary),
                I::GlobalGet(self.result_len),
                I::I32Eqz,
                I::If(BlockType::Result(ValType::I64)),
                I::I32Const(0),
            ]);
            self.box_union(Type::Undefined)?;
            self.code.extend([
                I::Else,
                I::LocalGet(self.temporary),
                I::GlobalGet(self.result_len),
            ]);
            self.string_helper(strings::Helper::Snapshot)?;
            self.box_union(Type::String)?;
            self.code.push(I::End);
            return Ok(());
        }
        if result == Type::Undefined {
            return Ok(());
        }
        self.code.push(I::GlobalGet(self.result_len));
        self.string_helper(strings::Helper::Snapshot)
    }
}
pub(super) fn field(index: u32) -> wasm_encoder::MemArg {
    wasm_encoder::MemArg {
        offset: (abi::RECORD_META_BYTES + index as usize * abi::RECORD_SLOT_BYTES) as u64,
        ..strings::ROOT
    }
}
