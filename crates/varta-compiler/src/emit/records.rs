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
            .value_kind()
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
    Ok(bytes)
}

impl Emitter<'_> {
    pub(super) fn record(
        &mut self,
        ty: Type,
        fields: &[(u32, Expr)],
        present: Option<u32>,
    ) -> Result<()> {
        let Type::Record(id) = ty else {
            return Err(CompilerError::Internal("missing object shape".into()));
        };
        for (_, value) in fields {
            self.expr(value)?;
        }
        let size = abi::record_bytes(self.profile.value_abi(), fields.len());
        self.code.push(I::I32Const(size as i32));
        self.string_helper(strings::Helper::Allocate)?;
        self.code.push(I::LocalSet(self.temporary));
        let mask = self.records[id as usize]
            .fields
            .iter()
            .enumerate()
            .fold(0u32, |mask, (i, field)| {
                mask | if field.ty.needs_root() { 1 << i } else { 0 }
            });
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
            let present = present.unwrap_or_else(|| abi::field_mask(fields.len()));
            let offset = abi::presence_offset(fields.len());
            self.code.extend([
                I::LocalGet(self.temporary),
                I::I32Const(present as i32),
                I::I32Store(wasm_encoder::MemArg {
                    offset: offset as u64,
                    ..strings::WORD
                }),
            ]);
            let order = layout_key(id, fields, self.records)
                .1
                .into_iter()
                .filter(|slot| present & (1u32 << slot) != 0)
                .chain(std::iter::repeat(0))
                .take(fields.len());
            for (index, slot) in order.enumerate() {
                self.code.extend([
                    I::LocalGet(self.temporary),
                    I::I32Const(slot as i32),
                    I::I32Store(wasm_encoder::MemArg {
                        offset: (offset + 4 + index * 4) as u64,
                        ..strings::WORD
                    }),
                ]);
            }
        }
        self.pack_value(size);
        Ok(())
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
    pub(super) fn json(&mut self, value: &Expr, result: Type) -> Result<()> {
        let kind = value
            .ty
            .value_kind()
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
        if result.is_union() {
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
