//! Managed lexical bindings and traced environments; no guest stack address escapes.
use super::*;
use varta_common::{abi, function as layout, value};
impl Emitter<'_> {
    pub(super) fn read_binding(&mut self, slot: u32, ty: Type) {
        if let Some(cell) = self.cell_slots.get(&slot).copied() {
            self.code.extend([
                I::LocalGet(cell),
                I::I32WrapI64,
                I::I64Load(records::field(0)),
            ]);
            self.unpack_slot(ty.result_value().unwrap_or(ty));
            if ty.width() == 2 {
                self.code.extend([
                    I::LocalGet(cell),
                    I::I32WrapI64,
                    I::I64Load(records::field(1)),
                    I::I32WrapI64,
                ]);
            }
        } else {
            self.scratch_get(slot, ty);
        }
    }
    pub(super) fn write_binding(&mut self, slot: u32, ty: Type, initialize: bool) -> Result<()> {
        self.scratch_set(slot, ty);
        if let Some(cell) = self.cell_slots.get(&slot).copied() {
            if initialize {
                let bytes =
                    value::RECORD_META_BYTES + ty.width() as usize * abi::STRING_ROOT_SLOT_BYTES;
                self.code.push(I::I32Const(bytes as i32));
                self.string_helper(strings::Helper::Allocate)?;
                self.code.push(I::LocalSet(self.temporary));
                self.initialize_value(
                    strings::ALLOCATED | layout::BINDING_FLAG,
                    i32::from(ty.needs_root()),
                    self.binding_types[&ty] as i32,
                    ty.width() as i32,
                );
                self.pack_value(bytes);
                self.code.push(I::LocalSet(cell));
            }
            self.code.extend([
                I::LocalGet(cell),
                I::I32WrapI64,
                I::LocalGet(self.local_slots[slot as usize]),
            ]);
            self.pack_slot(ty.result_value().unwrap_or(ty));
            self.code.push(I::I64Store(records::field(0)));
            if ty.width() == 2 {
                self.code.extend([
                    I::LocalGet(cell),
                    I::I32WrapI64,
                    I::LocalGet(self.local_slots[slot as usize] + 1),
                    I::I64ExtendI32U,
                    I::I64Store(records::field(1)),
                ]);
            }
        }
        self.store_root(slot);
        Ok(())
    }
    pub(super) fn environment(&mut self, captures: &[u32]) -> Result<()> {
        if captures.is_empty() {
            self.code.push(I::I64Const(0));
        } else {
            let bytes = value::RECORD_META_BYTES + captures.len() * abi::STRING_ROOT_SLOT_BYTES;
            self.code.push(I::I32Const(bytes as i32));
            self.string_helper(strings::Helper::Allocate)?;
            self.code.push(I::LocalSet(self.temporary));
            self.initialize_value(
                strings::ALLOCATED | layout::ENVIRONMENT_FLAG,
                capture_mask(captures.len()),
                layout::INTERNAL_KIND,
                captures.len() as i32,
            );
            for (index, slot) in captures.iter().enumerate() {
                self.code.extend([
                    I::LocalGet(self.temporary),
                    I::LocalGet(self.cell_slots[slot]),
                    I::I64Store(records::field(index as u32)),
                ]);
            }
            self.pack_value(bytes);
            self.string_helper(strings::Helper::RootPush)?;
        }
        self.code.push(I::LocalSet(self.string_temporary));
        Ok(())
    }
    pub(super) fn begin_closure(
        &mut self,
        function: &ir::Function,
        environment: Option<u32>,
        index: usize,
    ) -> Result<()> {
        self.load_environment(function, environment, index)?;
        for slot in 0..function.params.len() as u32 {
            if function.cells.contains(&slot) {
                let ty = function.params[slot as usize];
                self.scratch_get(slot, ty);
                self.write_binding(slot, ty, true)?;
            }
        }
        Ok(())
    }
    /// Check the self value and the environment of a closure and load each
    /// captured binding cell into its cell local.
    pub(super) fn load_environment(
        &mut self,
        function: &ir::Function,
        environment: Option<u32>,
        index: usize,
    ) -> Result<()> {
        if let Some(slot) = function.self_slot {
            let Type::Function(signature) = function.locals[slot as usize] else {
                return Err(CompilerError::Internal("missing self signature".into()));
            };
            let environment = environment
                .ok_or_else(|| CompilerError::Internal("missing self environment slot".into()))?;
            self.function_guard(slot, signature);
            self.assert_word(layout::CODE_OFFSET, index as i32);
            self.code.extend([
                I::LocalGet(self.local_slots[slot as usize]),
                I::I32WrapI64,
                I::I64Load(wasm_encoder::MemArg {
                    offset: layout::ENVIRONMENT_OFFSET,
                    ..strings::ROOT
                }),
                I::LocalGet(environment),
                I::I64Ne,
            ]);
            self.trap_if();
        }
        if let Some(environment) = environment {
            if function.captures.is_empty() {
                self.code
                    .extend([I::LocalGet(environment), I::I64Eqz, I::I32Eqz]);
                self.trap_if();
            } else {
                let count = function.captures.len();
                self.managed_guard(
                    environment,
                    value::RECORD_META_BYTES + count * abi::STRING_ROOT_SLOT_BYTES,
                    layout::ENVIRONMENT_FLAG,
                    Some(capture_mask(count)),
                );
                self.assert_word(layout::SIGNATURE_OFFSET, layout::INTERNAL_KIND);
                self.assert_word(layout::RESERVED_OFFSETS[0], count as i32);
                for (index, slot) in function.captures.iter().enumerate() {
                    let physical = self.cell_slots[slot];
                    let ty = function.locals[*slot as usize];
                    self.code.extend([
                        I::LocalGet(environment),
                        I::I32WrapI64,
                        I::I64Load(records::field(index as u32)),
                        I::LocalSet(physical),
                    ]);
                    self.managed_guard(
                        physical,
                        value::RECORD_META_BYTES
                            + ty.width() as usize * abi::STRING_ROOT_SLOT_BYTES,
                        layout::BINDING_FLAG,
                        Some(i32::from(ty.needs_root())),
                    );
                    self.assert_word(layout::SIGNATURE_OFFSET, self.binding_types[&ty] as i32);
                    self.assert_word(layout::RESERVED_OFFSETS[0], ty.width() as i32);
                    match ty.result_value().unwrap_or(ty) {
                        Type::Boolean => {
                            self.code.extend([
                                I::LocalGet(self.temporary),
                                I::I64Load(records::field(0)),
                                I::I64Const(1),
                                I::I64GtU,
                            ]);
                            self.trap_if();
                        }
                        Type::Null | Type::Undefined | Type::Void => {
                            self.code.extend([
                                I::LocalGet(self.temporary),
                                I::I64Load(records::field(0)),
                                I::I64Eqz,
                                I::I32Eqz,
                            ]);
                            self.trap_if();
                        }
                        _ => {}
                    }
                    if ty.width() == 2 {
                        self.code.extend([
                            I::LocalGet(self.temporary),
                            I::I64Load(records::field(1)),
                            I::I64Const(strings::LENGTH_SHIFT as i64),
                            I::I64ShrU,
                            I::I64Eqz,
                            I::I32Eqz,
                        ]);
                        self.trap_if();
                    }
                    self.store_root(*slot);
                }
            }
        }
        Ok(())
    }
    pub(super) fn trap_if(&mut self) {
        self.code
            .extend([I::If(BlockType::Empty), I::Unreachable, I::End]);
    }
    pub(super) fn assert_word(&mut self, offset: u64, value: i32) {
        self.code.extend([
            I::LocalGet(self.temporary),
            I::I32Load(wasm_encoder::MemArg {
                offset,
                ..strings::WORD
            }),
            I::I32Const(value),
            I::I32Ne,
        ]);
        self.trap_if();
    }
    /// All loads follow checked alignment, payload/header and allocation bounds.
    pub(super) fn managed_guard(
        &mut self,
        physical: u32,
        bytes: usize,
        flag: i32,
        mask: Option<i32>,
    ) {
        self.code.extend([
            I::LocalGet(physical),
            I::I64Const(strings::LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I64Const(bytes as i64),
            I::I64Ne,
            I::LocalGet(physical),
            I::I32WrapI64,
            I::LocalTee(self.temporary),
            I::I32Const(self.heap_base() as i32 + abi::STRING_HEADER_BYTES as i32),
            I::I32LtU,
            I::I32Or,
            I::LocalGet(self.temporary),
            I::I32Const((value::HEAP_ALIGNMENT - 1) as i32),
            I::I32And,
            I::I32Const(0),
            I::I32Ne,
            I::I32Or,
        ]);
        self.trap_if();
        self.code.extend([
            I::LocalGet(self.temporary),
            I::MemorySize(strings::MEMORY),
            I::I32Const(abi::WASM_PAGE_BYTES.trailing_zeros() as i32),
            I::I32Shl,
            I::I32Const(bytes as i32),
            I::I32Sub,
            I::I32GtU,
        ]);
        self.trap_if();
        for (op, expected) in [
            (I::I32LtU, (abi::STRING_HEADER_BYTES + bytes) as i32),
            (I::I32And, (value::HEAP_ALIGNMENT - 1) as i32),
        ] {
            self.code.extend([
                I::LocalGet(self.temporary),
                I::I32Const(abi::STRING_HEADER_BYTES as i32),
                I::I32Sub,
                I::I32Load(strings::WORD),
                I::I32Const(expected),
                op,
            ]);
            self.trap_if();
        }
        self.code.extend([
            I::LocalGet(self.temporary),
            I::I32Const(abi::STRING_HEADER_BYTES as i32),
            I::I32Sub,
            I::I32Load(strings::WORD),
            I::MemorySize(strings::MEMORY),
            I::I32Const(abi::WASM_PAGE_BYTES.trailing_zeros() as i32),
            I::I32Shl,
            I::LocalGet(self.temporary),
            I::I32Const(abi::STRING_HEADER_BYTES as i32),
            I::I32Sub,
            I::I32Sub,
            I::I32GtU,
        ]);
        self.trap_if();
        self.code.extend([
            I::LocalGet(self.temporary),
            I::I32Const(abi::STRING_HEADER_BYTES as i32),
            I::I32Sub,
            I::I32Load(strings::FLAG),
            I::I32Const(strings::ALLOCATED | flag),
            I::I32Ne,
        ]);
        self.trap_if();
        if let Some(mask) = mask {
            self.code.extend([
                I::LocalGet(self.temporary),
                I::I32Const(abi::STRING_HEADER_BYTES as i32),
                I::I32Sub,
                I::I32Load(strings::LINK),
                I::I32Const(mask),
                I::I32Ne,
            ]);
            self.trap_if();
        }
    }
}
fn capture_mask(count: usize) -> i32 {
    u32::MAX.checked_shr(u32::BITS - count as u32).unwrap_or(0) as i32
}
