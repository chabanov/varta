//! Codegen v2 (v38) IR simplification after lowering.
//!
//! Lowering caches operands of primitive operators (string concatenation and
//! comparison, BigInt-aware arithmetic) in hidden locals of a value block so
//! that each operand is evaluated once, in source order, and stays rooted. When
//! every cached operand is a plain local read or a literal and the final
//! operation writes no binding and calls no guest code, the cache adds only
//! cost: re-reading the local at its use observes the same value, and the
//! emitter roots operands of an allocating operation itself. Such a block
//! collapses into its final operation. Hidden slots must be private to the
//! block (no mention outside it), a local operand is substituted for at most
//! one read, and suspendable functions are left unchanged because their
//! liveness layout was computed on the unsimplified body. A block caching the
//! two operands of a non-short-circuit binary operator collapses into that
//! operator for any operands, since the operator evaluates them once each in
//! the same order (this makes `s = s + (a + b)` a builder append). Earlier
//! profiles never run this pass and keep their bytes.
use super::*;
use std::collections::BTreeMap;

pub(super) fn simplify(program: &mut ir::Program) {
    for (index, function) in program.functions.iter_mut().enumerate() {
        if program.suspendable.contains_key(&index) {
            continue;
        }
        let simplifier = Simplifier {
            counts: suspension::mention_counts(&function.body),
            locals: &function.locals,
        };
        statements_mut(&mut function.body, &mut |expr| simplifier.collapse(expr));
    }
}

struct Simplifier<'a> {
    counts: BTreeMap<u32, usize>,
    locals: &'a [Type],
}

/// A cached operand that may be re-evaluated at its use.
enum Operand {
    Local(u32),
    Integer(i32),
    Float64(f64),
    String(Vec<u8>),
}
impl Operand {
    fn of(expr: &Expr) -> Option<Self> {
        Some(match &expr.kind {
            E::Local(slot) => Self::Local(*slot),
            E::Integer(value) => Self::Integer(*value),
            E::Float64(value) => Self::Float64(*value),
            E::String(bytes) if expr.ty != Type::BigInt => Self::String(bytes.clone()),
            _ => return None,
        })
    }
    fn expression(&self) -> E {
        match self {
            Self::Local(slot) => E::Local(*slot),
            Self::Integer(value) => E::Integer(*value),
            Self::Float64(value) => E::Float64(*value),
            Self::String(bytes) => E::String(bytes.clone()),
        }
    }
}

impl Simplifier<'_> {
    fn collapse(&self, expr: &mut Expr) {
        if let Some(operation) = self.collapsed(expr).or_else(|| self.ordered_pair(expr)) {
            *expr = operation;
        }
    }
    /// `{a = x; b = y; out = a op b}` is `x op y` for any operands: the binary
    /// operator evaluates its left operand before its right one, so `x` and
    /// `y` still run once each and in source order, and the emitter roots
    /// the left value while the right one allocates. Short-circuit operators
    /// would skip `y` and are excluded.
    fn ordered_pair(&self, expr: &mut Expr) -> Option<Expr> {
        let ty = expr.ty;
        let mut block = BTreeMap::new();
        suspension::expression_counts(expr, &mut block);
        let E::ValueBlock {
            output,
            roots,
            body,
            ..
        } = &expr.kind
        else {
            return None;
        };
        let [S::Init(a, left), S::Init(b, right), S::Init(out, operation)] = body.as_slice() else {
            return None;
        };
        let E::Binary(op, read_left, read_right) = &operation.kind else {
            return None;
        };
        let private = [*a, *b, *output];
        let mut operand_mentions = BTreeMap::new();
        suspension::expression_counts(left, &mut operand_mentions);
        suspension::expression_counts(right, &mut operand_mentions);
        let reads = matches!(
            (&read_left.kind, &read_right.kind),
            (E::Local(x), E::Local(y)) if x == a && y == b
        );
        if !reads
            || matches!(
                op,
                ast::BinaryOp::LogicalAnd
                    | ast::BinaryOp::LogicalOr
                    | ast::BinaryOp::NullishCoalescing
            )
            || out != output
            || a == b
            || *a == *output
            || *b == *output
            || operation.ty != ty
            || self.locals[*output as usize] != ty
            || self.locals[*a as usize] != left.ty
            || self.locals[*b as usize] != right.ty
            || read_left.ty != left.ty
            || read_right.ty != right.ty
            || !roots.iter().all(|slot| private.contains(slot))
            || private
                .iter()
                .any(|slot| block.get(slot) != self.counts.get(slot))
            || private
                .iter()
                .any(|slot| operand_mentions.contains_key(slot))
        {
            return None;
        }
        let op = *op;
        let E::ValueBlock { body, .. } = std::mem::replace(&mut expr.kind, E::Zero) else {
            return None;
        };
        let mut body = body.into_iter();
        let (Some(S::Init(_, left)), Some(S::Init(_, right)), Some(S::Init(_, operation))) =
            (body.next(), body.next(), body.next())
        else {
            return None;
        };
        Some(Expr {
            span: operation.span,
            ty: operation.ty,
            kind: E::Binary(op, Box::new(left), Box::new(right)),
        })
    }
    fn collapsed(&self, expr: &mut Expr) -> Option<Expr> {
        let ty = expr.ty;
        let mut block = BTreeMap::new();
        suspension::expression_counts(expr, &mut block);
        let E::ValueBlock {
            output,
            roots,
            body,
            ..
        } = &mut expr.kind
        else {
            return None;
        };
        let (S::Init(out, operation), cached) = body.split_last_mut()? else {
            return None;
        };
        if *out != *output || operation.ty != ty || self.locals[*output as usize] != ty {
            return None;
        }
        let mut operands = BTreeMap::new();
        for statement in cached.iter() {
            let S::Init(slot, value) = statement else {
                return None;
            };
            let operand = Operand::of(value)?;
            if *slot == *output
                || self.locals[*slot as usize] != value.ty
                || operands.insert(*slot, (operand, value.ty)).is_some()
            {
                return None;
            }
        }
        let hidden = |slot: &u32| *slot == *output || operands.contains_key(slot);
        // Every hidden slot is private to the block, and a cached local is
        // never another cached slot of it.
        if !roots.iter().all(hidden)
            || !operands
                .keys()
                .chain(std::iter::once(&*output))
                .all(|slot| block.get(slot) == self.counts.get(slot))
            || operands
                .values()
                .any(|(operand, _)| matches!(operand, Operand::Local(slot) if hidden(slot)))
            || !operation.write_free()
        {
            return None;
        }
        let mut reads = BTreeMap::new();
        let mut typed = true;
        expression_mut(operation, &mut |node| {
            if let E::Local(slot) = node.kind
                && let Some((_, ty)) = operands.get(&slot)
            {
                *reads.entry(slot).or_insert(0usize) += 1;
                typed &= node.ty == *ty;
            }
        });
        if !typed
            || reads
                .iter()
                .any(|(slot, count)| *count > 1 && matches!(operands[slot].0, Operand::Local(_)))
        {
            return None;
        }
        expression_mut(operation, &mut |node| {
            if let E::Local(slot) = node.kind
                && let Some((operand, _)) = operands.get(&slot)
            {
                node.kind = operand.expression();
            }
        });
        let E::ValueBlock { body, .. } = std::mem::replace(&mut expr.kind, E::Zero) else {
            return None;
        };
        body.into_iter()
            .last()
            .and_then(|statement| match statement {
                S::Init(_, operation) => Some(operation),
                _ => None,
            })
    }
}

/// Visit every expression of `body` after its nested expressions.
fn statements_mut(body: &mut [S], visit: &mut dyn FnMut(&mut Expr)) {
    for statement in body {
        statement_mut(statement, visit);
    }
}

fn statement_mut(statement: &mut S, visit: &mut dyn FnMut(&mut Expr)) {
    match statement {
        S::Trap
        | S::Rebind(_)
        | S::RecordRest { .. }
        | S::BreakLabel(_)
        | S::ContinueLabel(_)
        | S::Break
        | S::Continue
        | S::Return(None) => {}
        S::Throw(value)
        | S::Init(_, value)
        | S::Set(_, value)
        | S::InitModuleConstant(_, value)
        | S::Expr(value)
        | S::Return(Some(value)) => expression_mut(value, visit),
        S::TryCatch { body, handler, .. } => {
            statements_mut(body, visit);
            statements_mut(handler, visit);
        }
        S::TryFinally {
            body, finalizer, ..
        } => {
            statements_mut(body, visit);
            statements_mut(finalizer, visit);
        }
        S::RecordEnumerate { values, .. } => values
            .iter_mut()
            .for_each(|value| expression_mut(value, visit)),
        S::SetField(object, _, value) => {
            expression_mut(object, visit);
            expression_mut(value, visit);
        }
        S::ArrayLength {
            array,
            length,
            error,
        } => [array, length, error]
            .into_iter()
            .for_each(|value| expression_mut(value, visit)),
        S::SetIndex {
            array,
            index,
            value,
            ..
        } => [array, index, value]
            .into_iter()
            .for_each(|value| expression_mut(value, visit)),
        S::Block(body) | S::Scope(_, body) | S::Label { body, .. } => statements_mut(body, visit),
        S::If(test, yes, no) => {
            expression_mut(test, visit);
            statements_mut(yes, visit);
            statements_mut(no, visit);
        }
        S::While(test, body) | S::DoWhile(test, body) => {
            expression_mut(test, visit);
            statements_mut(body, visit);
        }
        S::For(init, test, update, body) => {
            statements_mut(init, visit);
            if let Some(test) = test {
                expression_mut(test, visit);
            }
            statements_mut(update, visit);
            statements_mut(body, visit);
        }
        S::Switch { value, cases, .. } => {
            expression_mut(value, visit);
            for (test, body) in cases {
                if let Some(test) = test {
                    expression_mut(test, visit);
                }
                statements_mut(body, visit);
            }
        }
    }
}

fn boxed(values: &mut [&mut Box<Expr>], visit: &mut dyn FnMut(&mut Expr)) {
    for value in values {
        expression_mut(value, visit);
    }
}

/// Exhaustive on purpose: a new IR form must decide how it is traversed.
fn expression_mut(expr: &mut Expr, visit: &mut dyn FnMut(&mut Expr)) {
    match &mut expr.kind {
        E::Integer(_)
        | E::Float64(_)
        | E::String(_)
        | E::Local(_)
        | E::Status
        | E::ModuleConstant(_)
        | E::Zero
        | E::FunctionRef { .. } => {}
        E::Assignment { value, body, .. } => {
            expression_mut(value, visit);
            statements_mut(body, visit);
        }
        E::Await {
            promise, result, ..
        } => boxed(&mut [promise, result], visit),
        E::Schedule { args, .. } => args
            .iter_mut()
            .for_each(|value| expression_mut(value, visit)),
        E::ValueBlock { body, .. } => statements_mut(body, visit),
        E::GeneratorStep {
            generator, value, ..
        } => boxed(&mut [generator, value], visit),
        E::FunctionCheck { value, .. }
        | E::GeneratorValue(value)
        | E::Reinterpret(value)
        | E::Suspend { value, .. }
        | E::StringTrim { value, .. }
        | E::TextUnits { value, .. }
        | E::ArrayEnumerate { value, .. }
        | E::ArrayReserve { length: value, .. }
        | E::NewError(_, value)
        | E::ErrorName(value)
        | E::ErrorMessage(value)
        | E::ErrorText(value)
        | E::ErrorOwn(value)
        | E::BoxUnion(value)
        | E::UnboxUnion(value)
        | E::IsUndefined(value)
        | E::IsError(value)
        | E::IsClass { value, .. }
        | E::ArrayKind(value, _)
        | E::IsNullish(value)
        | E::IsUnionMember(value, _)
        | E::Undefined(value)
        | E::Negative(value)
        | E::ToBoolean(value)
        | E::Not(value)
        | E::Capture(value)
        | E::ResultField(_, value)
        | E::HasOwn(value, _)
        | E::Field(value, _)
        | E::Json(value, _)
        | E::ParseJson(value)
        | E::KvStage(_, value)
        | E::KvDecode(_, value)
        | E::ArrayPop(value)
        | E::ArrayReverse(value)
        | E::IndexKey(value)
        | E::Length(value) => expression_mut(value, visit),
        E::FunctionCall {
            callee,
            ignored_receiver,
            args,
            ..
        } => {
            expression_mut(callee, visit);
            if let Some(receiver) = ignored_receiver {
                expression_mut(receiver, visit);
            }
            args.iter_mut()
                .for_each(|value| expression_mut(value, visit));
        }
        E::StringSearch(_, a, b, c)
        | E::Conditional(a, b, c)
        | E::ArraySearch(_, a, b, c)
        | E::ArraySlice(a, b, c) => boxed(&mut [a, b, c], visit),
        E::StringRange {
            value, start, end, ..
        } => boxed(&mut [value, start, end], visit),
        E::StringRepeat {
            value,
            count,
            error,
            ..
        } => boxed(&mut [value, count, error], visit),
        E::UnionLogical(_, a, b)
        | E::Sequence(a, b)
        | E::Binary(_, a, b)
        | E::NumberSameValue(a, b)
        | E::MaybeIndex(a, b)
        | E::ArrayHasOwn(a, b)
        | E::ArrayStringHasOwn(a, b)
        | E::ArrayPush(a, b)
        | E::ArrayAt(a, b)
        | E::Index(a, b) => boxed(&mut [a, b], visit),
        E::RecordDelete { object, absent, .. } => boxed(&mut [object, absent], visit),
        E::ArrayFill(a, b, c, d) => boxed(&mut [a, b, c, d], visit),
        E::Math(_, args)
        | E::NumberPredicate(_, args)
        | E::Intrinsic(_, args)
        | E::RegExp(_, args)
        | E::Call(_, args)
        | E::Host(_, args)
        | E::CheckedBigIntHost { args, .. } => args
            .iter_mut()
            .for_each(|value| expression_mut(value, visit)),
        E::Record(fields) | E::OptionalRecord(fields, _) => fields
            .iter_mut()
            .for_each(|(_, value)| expression_mut(value, visit)),
        E::Array(elements) => elements
            .iter_mut()
            .flatten()
            .for_each(|value| expression_mut(value, visit)),
        E::ArrayDelete {
            array,
            index,
            error,
        } => {
            expression_mut(array, visit);
            if let Some(index) = index {
                expression_mut(index, visit);
            }
            if let Some(error) = error {
                expression_mut(error, visit);
            }
        }
    }
    visit(expr);
}
