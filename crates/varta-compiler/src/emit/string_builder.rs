//! Codegen v2 (v38): amortized `s = s + t` appends for local string bindings.
//!
//! Strings stay immutable packed `(ptr, len)` slices. A *builder binding* is a
//! function-local, non-closure-shared String binding of a non-suspendable
//! function that the body appends to with `s = s + t` (or `s += t`, which
//! lowers to the same statement). Two hidden i32 locals describe the block the
//! binding's current value was built in: `tail`, the address just after the
//! value's last byte, and `cap`, the end of that block's payload. Only the
//! binding that allocated the block holds them, and every other write of the
//! binding resets both to zero, so the bytes in `[tail, cap)` belong to no
//! other value. An append whose value still ends at `tail` and fits in the
//! block copies only the new bytes there and lengthens the binding's slice;
//! older aliases keep their own shorter length, so no observable string
//! changes. Otherwise a fresh block with doubled capacity (bounded by the
//! per-value limit) receives both operands. The binding's frame root slot is
//! rewritten after every append, before any later allocation; the right
//! operand stays rooted while the slow path allocates.
use super::*;
use varta_common::abi;

/// The smallest payload capacity of a builder block.
pub(super) const MIN_CAPACITY: i32 = 32;

/// Builder-binding candidates of one function: String locals that some
/// statement sets to `binding + value`.
pub(super) fn candidates(function: &ir::Function) -> BTreeSet<u32> {
    fn visit(body: &[S], found: &mut BTreeSet<u32>) {
        for statement in body {
            match statement {
                S::Set(slot, value) if append_operand(*slot, value).is_some() => {
                    found.insert(*slot);
                }
                S::Block(body)
                | S::Scope(_, body)
                | S::Label { body, .. }
                | S::While(_, body)
                | S::DoWhile(_, body) => visit(body, found),
                S::If(_, yes, no) => {
                    visit(yes, found);
                    visit(no, found);
                }
                S::For(init, _, update, body) => {
                    visit(init, found);
                    visit(update, found);
                    visit(body, found);
                }
                S::Switch { cases, .. } => {
                    for (_, body) in cases {
                        visit(body, found);
                    }
                }
                S::TryCatch { body, handler, .. } => {
                    visit(body, found);
                    visit(handler, found);
                }
                S::TryFinally {
                    body, finalizer, ..
                } => {
                    visit(body, found);
                    visit(finalizer, found);
                }
                _ => {}
            }
        }
    }
    let mut found = BTreeSet::new();
    visit(&function.body, &mut found);
    found.retain(|slot| {
        !function.cells.contains(slot) && function.locals[*slot as usize] == Type::String
    });
    found
}

/// The appended operand of `slot = slot + value` on Strings.
pub(super) fn append_operand(slot: u32, value: &Expr) -> Option<&Expr> {
    match &value.kind {
        E::Binary(BinaryOp::Add, left, right)
            if value.ty == Type::String
                && left.ty == Type::String
                && right.ty == Type::String
                && matches!(left.kind, E::Local(read) if read == slot) =>
        {
            Some(right)
        }
        _ => None,
    }
}

impl Emitter<'_> {
    /// Emit `slot = slot + value` as an amortized append when `slot` is a
    /// builder binding; false leaves the statement to the ordinary path.
    pub(super) fn string_builder(&mut self, slot: u32, value: &Expr) -> Result<bool> {
        let (Some(&(tail, cap)), Some(right)) =
            (self.builders.get(&slot), append_operand(slot, value))
        else {
            return Ok(false);
        };
        let target = self.local_slots[slot as usize];
        let length = self.temporary;
        // The operand stays rooted until the binding owns the result, unless
        // it is quiet and write-free (reachable from an unchanged binding).
        if self.quiet(right) && right.write_free() {
            self.quiet_mode = true;
            let emitted = self.expr(right);
            self.quiet_mode = false;
            emitted?;
        } else {
            self.expr(right)?;
        }
        self.code.extend([
            I::LocalTee(self.string_temporary),
            I::I64Const(strings::LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I32WrapI64,
            I::LocalTee(length),
            I::If(BlockType::Empty),
            // In place: the value still ends at the owned tail and fits.
            I::LocalGet(target),
            I::I32WrapI64,
            I::LocalGet(target),
            I::I64Const(strings::LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I32WrapI64,
            I::I32Add,
            I::LocalGet(tail),
            I::I32Eq,
            I::LocalGet(tail),
            I::LocalGet(length),
            I::I32Add,
            I::LocalGet(cap),
            I::I32LeU,
            I::I32And,
            I::If(BlockType::Empty),
            I::LocalGet(tail),
            I::LocalGet(self.string_temporary),
            I::I32WrapI64,
            I::LocalGet(length),
            I::MemoryCopy {
                src_mem: strings::MEMORY,
                dst_mem: strings::MEMORY,
            },
            I::LocalGet(tail),
            I::LocalGet(length),
            I::I32Add,
            I::LocalSet(tail),
            I::LocalGet(target),
            I::LocalGet(length),
            I::I64ExtendI32U,
            I::I64Const(strings::LENGTH_SHIFT as i64),
            I::I64Shl,
            I::I64Add,
            I::LocalSet(target),
            I::Else,
            I::LocalGet(target),
            I::LocalGet(self.string_temporary),
        ]);
        self.string_helper(strings::Helper::StringGrow)?;
        self.code.extend([
            I::LocalTee(target),
            I::I32WrapI64,
            I::LocalGet(target),
            I::I64Const(strings::LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I32WrapI64,
            I::I32Add,
            I::LocalSet(tail),
            // The block header's size word bounds the payload capacity.
            I::LocalGet(target),
            I::I32WrapI64,
            I::I32Const(abi::STRING_HEADER_BYTES as i32),
            I::I32Sub,
            I::LocalTee(length),
            I::LocalGet(length),
            I::I32Load(strings::WORD),
            I::I32Add,
            I::LocalSet(cap),
            I::End,
            I::End,
        ]);
        self.store_root(slot);
        self.end_temporaries();
        Ok(true)
    }
    /// Every write of a builder binding other than an append forfeits its
    /// block. Both words reset: with `cap` zero no append fits in place, even
    /// for a value (like the empty string) whose end matches a zero `tail`.
    pub(super) fn reset_builder(&mut self, slot: u32) {
        if let Some(&(tail, cap)) = self.builders.get(&slot) {
            self.code.extend([
                I::I32Const(0),
                I::LocalSet(tail),
                I::I32Const(0),
                I::LocalSet(cap),
            ]);
        }
    }
}
