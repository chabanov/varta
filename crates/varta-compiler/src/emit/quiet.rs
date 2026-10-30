//! Codegen v2 (v38): allocation-free, root-neutral expressions and statements.
//!
//! An expression is *quiet* when emitting it calls no allocator (so no
//! collection can run while it is evaluated), calls no guest function and
//! never moves the shadow-root stack pointer. A quiet subtree therefore needs
//! neither an expression root mark nor roots for its intermediate operands:
//! every operand it reads is consumed before any later allocation, and every
//! binding it reads stays rooted by its own frame slot, because a quiet
//! subtree writes no binding outside its own hidden value-block slots. The
//! result of a quiet expression is still rooted when it is a reference and its
//! consumer may allocate. A checked element read (`dense`) is quiet although
//! its missing-element path allocates an error: that path throws, abandoning
//! every operand the enclosing quiet expression held. Earlier profiles never
//! take these paths and keep their bytes.
use super::*;

impl Emitter<'_> {
    /// Whether the v38 code generator (`codegen_v2`) is active.
    pub(super) fn codegen_v2(&self) -> bool {
        crate::codegen_v2(self.profile)
    }
    /// The whitelist is closed: an unknown form is never quiet.
    pub(super) fn quiet(&self, expr: &Expr) -> bool {
        match &expr.kind {
            E::Integer(_) | E::Float64(_) | E::Status | E::ModuleConstant(_) | E::Local(_) => true,
            // A BigInt literal snapshots into the heap.
            E::String(_) => expr.ty != Type::BigInt,
            E::Binary(op, a, b) => {
                !(a.ty == Type::String && *op == BinaryOp::Add) && self.quiet(a) && self.quiet(b)
            }
            E::NumberSameValue(a, b) | E::Sequence(a, b) => self.quiet(a) && self.quiet(b),
            E::Conditional(test, yes, no) => self.quiet(test) && self.quiet(yes) && self.quiet(no),
            // Searches read both strings in place.
            E::StringSearch(_, value, needle, position) => {
                self.quiet(value) && self.quiet(needle) && self.quiet(position)
            }
            E::Not(value)
            | E::Negative(value)
            | E::ToBoolean(value)
            | E::Length(value)
            | E::Field(value, _)
            | E::Undefined(value)
            | E::IsUndefined(value)
            | E::IsNullish(value)
            | E::UnboxUnion(value)
            | E::Reinterpret(value) => self.quiet(value),
            E::Index(array, index) => self.quiet(array) && self.quiet(index),
            E::Math(_, args) | E::NumberPredicate(_, args) => args.iter().all(|a| self.quiet(a)),
            // Code-unit reads load from the string in place.
            E::Intrinsic(ir::Intrinsic::CodeUnit | ir::Intrinsic::CharCodeAt, args) => {
                args.iter().all(|a| self.quiet(a))
            }
            // A checked element read throws on its only allocating path.
            E::ValueBlock { body, .. } => match super::dense::dense_read(expr) {
                Some(read) => self.quiet(read.base) && self.quiet(read.index),
                None => body.iter().all(|s| self.quiet_statement(s)),
            },
            E::Assignment {
                value, input, body, ..
            } => {
                !self.cell_slots.contains_key(input)
                    && self.quiet(value)
                    && body.iter().all(|s| self.quiet_statement(s))
            }
            _ => false,
        }
    }
    /// An allocating string operation whose operands are quiet and write no
    /// binding: every operand value is a literal or reachable from a binding
    /// that stays unchanged until the helper consumes it, so neither the
    /// operands nor the operation need temporary roots or a mark; only the
    /// result is rooted.
    pub(super) fn stable_operands(&self, expr: &Expr) -> bool {
        let operands: Vec<&Expr> = match &expr.kind {
            // A chain within one helper call: its operands are its leaves.
            E::Binary(BinaryOp::Add, a, _) if a.ty == Type::String => {
                let leaves = super::concat_chain::leaves(expr);
                if leaves.len() > strings::MAX_CONCATENATED {
                    return false;
                }
                leaves
            }
            E::StringRange {
                value, start, end, ..
            } => vec![value, start, end],
            E::StringTrim { value, .. } | E::TextUnits { value, .. } => vec![value],
            E::Intrinsic(ir::Intrinsic::CharAt | ir::Intrinsic::FromUnits, args) => {
                args.iter().collect()
            }
            // An appended argument list (no argument array): the target and
            // every slot stay reachable from their bindings while it grows.
            E::ArrayPush(base, items) => match super::arrays::appended(self.profile, items) {
                Some(values) => std::iter::once(&**base).chain(values).collect(),
                None => return false,
            },
            _ => return false,
        };
        operands
            .into_iter()
            .all(|operand| self.quiet(operand) && operand.write_free())
    }
    /// A call of a function value whose callee is quiet and write-free and
    /// whose arguments are quiet, with no suspendable or self-referencing
    /// target: the callee value stays reachable from its binding until the
    /// checked dispatch has read it, and the target roots its parameters and
    /// captured cells before any allocation and never reads the value again,
    /// so the call needs no mark and no operand roots. (A named function
    /// expression keeps its own value in an unrooted local, which only the
    /// caller's expression root keeps alive.)
    pub(super) fn quiet_function_call(&self, expr: &Expr) -> bool {
        let E::FunctionCall {
            callee,
            ignored_receiver: None,
            args,
            ..
        } = &expr.kind
        else {
            return false;
        };
        let Type::Function(signature) = callee.ty else {
            return false;
        };
        self.quiet(callee)
            && callee.write_free()
            && args.iter().all(|arg| self.quiet(arg))
            && self.function_targets[&signature].iter().all(|target| {
                !self.frame_functions.contains_key(target) && !self.self_functions.contains(target)
            })
    }
    /// A statement that allocates nothing and leaves the root stack pointer
    /// unchanged, so it needs no temporary-root reset after it.
    pub(super) fn quiet_statement(&self, statement: &S) -> bool {
        match statement {
            // Initializing a closure-shared binding allocates its cell.
            S::Init(slot, value) => !self.cell_slots.contains_key(slot) && self.quiet(value),
            S::Set(_, value) | S::Expr(value) => self.quiet(value),
            S::SetField(object, _, value) => self.quiet(object) && self.quiet(value),
            S::Block(body) | S::Scope(_, body) => body.iter().all(|s| self.quiet_statement(s)),
            S::If(test, yes, no) => {
                self.quiet(test)
                    && yes.iter().all(|s| self.quiet_statement(s))
                    && no.iter().all(|s| self.quiet_statement(s))
            }
            _ => false,
        }
    }
    /// Release the temporaries of a completed leaf statement unless it is
    /// quiet under v38 (then it pushed nothing to release).
    pub(super) fn end_statement(&mut self) {
        if !self.quiet_mode {
            self.end_temporaries();
        }
    }
    /// Evaluate a branch condition as an i32 whose nonzero values mean true.
    /// `If`, `BrIf` and `i32.eqz` already treat every nonzero i32 as true, so
    /// v38 omits the 0/1 normalization of an i32 `ToBoolean` operand.
    pub(super) fn test(&mut self, test: &Expr) -> Result<()> {
        if self.codegen_v2()
            && let E::ToBoolean(value) = &test.kind
            && matches!(value.ty, Type::Boolean | Type::Number)
        {
            return self.expr_unrooted(value);
        }
        self.expr_unrooted(test)?;
        self.truthy(test.ty);
        Ok(())
    }
    /// V38: `value === "lit"` (either order, also `!==`, `==`, `!=`) against
    /// a literal of at most sixteen bytes compares the length, then one or two
    /// (possibly overlapping) in-bounds words of the value with constants of
    /// the literal, with no helper call. False when the comparison has another
    /// shape; nothing is emitted then.
    pub(super) fn literal_equality(
        &mut self,
        op: BinaryOp,
        left: &Expr,
        right: &Expr,
    ) -> Result<bool> {
        const WIDTHS: [usize; 4] = [size_of::<u64>(), size_of::<u32>(), size_of::<u16>(), 1];
        const MAX_LITERAL_BYTES: usize = 2 * size_of::<u64>();
        let equal = match op {
            BinaryOp::EqEq | BinaryOp::EqEqEq => true,
            BinaryOp::NotEq | BinaryOp::NotEqEq => false,
            _ => return Ok(false),
        };
        let (value, literal) = match (&left.kind, &right.kind) {
            (_, E::String(bytes)) if bytes.len() <= MAX_LITERAL_BYTES => (left, bytes),
            (E::String(bytes), _) if bytes.len() <= MAX_LITERAL_BYTES => (right, bytes),
            _ => return Ok(false),
        };
        if !self.codegen_v2() || value.ty != Type::String || right.ty != Type::String {
            return Ok(false);
        }
        // The literal itself has no effects; only the other operand runs.
        self.expr_unrooted(value)?;
        let text = self.string_temporary;
        let len = literal.len();
        self.code.extend([
            I::LocalTee(text),
            I::I64Const(strings::LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I64Const(len as i64),
            I::I64Eq,
            I::If(BlockType::Result(ValType::I32)),
        ]);
        let Some(width) = WIDTHS.into_iter().find(|width| *width <= len) else {
            // The empty literal: equal lengths are equal strings.
            self.code
                .extend([I::I32Const(1), I::Else, I::I32Const(0), I::End]);
            if !equal {
                self.code.push(I::I32Eqz);
            }
            return Ok(true);
        };
        let memarg = |offset: usize| wasm_encoder::MemArg {
            offset: offset as u64,
            align: 0,
            memory_index: strings::MEMORY,
        };
        // Little-endian word of the literal at `offset`.
        let word = |offset: usize| {
            literal[offset..offset + width]
                .iter()
                .rev()
                .fold(0u64, |acc, byte| acc << u8::BITS | u64::from(*byte))
        };
        let offsets = if width == len {
            vec![0]
        } else {
            vec![0, len - width]
        };
        for (index, offset) in offsets.into_iter().enumerate() {
            self.code.extend([I::LocalGet(text), I::I32WrapI64]);
            self.code.extend(match width {
                8 => [
                    I::I64Load(memarg(offset)),
                    I::I64Const(word(offset) as i64),
                    I::I64Eq,
                ],
                4 => [
                    I::I32Load(memarg(offset)),
                    I::I32Const(word(offset) as u32 as i32),
                    I::I32Eq,
                ],
                2 => [
                    I::I32Load16U(memarg(offset)),
                    I::I32Const(word(offset) as i32),
                    I::I32Eq,
                ],
                _ => [
                    I::I32Load8U(memarg(offset)),
                    I::I32Const(word(offset) as i32),
                    I::I32Eq,
                ],
            });
            if index > 0 {
                self.code.push(I::I32And);
            }
        }
        self.code.extend([I::Else, I::I32Const(0), I::End]);
        if !equal {
            self.code.push(I::I32Eqz);
        }
        Ok(true)
    }
}
