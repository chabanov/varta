//! Evaluation-order normalization. Afterwards every suspension point is the
//! canonical statement `Init(t, Suspend { value })` with a suspension-free value
//! and a fresh temporary `t`; everything evaluated before it has been
//! materialized in hidden locals or explicit branches, in the original order.
use super::*;

/// Prefix statements plus the residual, suspension-free expression.
type Split = (Vec<S>, Expr);

pub(super) struct Normalizer<'a> {
    pub locals: &'a mut Vec<Type>,
    pub namer: &'a construct::Namer<'a>,
    pub source: &'a Source,
    /// Hidden locals owned by the statement being normalized; their roots are
    /// released when that statement completes.
    pending: Vec<u32>,
    /// Scheduled frames (v34) name their suspension points `await`.
    pub awaits: bool,
    /// Async generator bodies (v36): the state record local an `await`
    /// publishes, its type, and the input type of the suspension.
    pub publish: Option<(u32, Type, Type)>,
}

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn boolean(span: swc_common::Span, value: bool) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Integer(i32::from(value)),
    }
}
fn constant(expr: &Expr) -> bool {
    matches!(expr.kind, E::Integer(_) | E::Float64(_) | E::String(_))
}

impl<'a> Normalizer<'a> {
    pub(super) fn new(
        locals: &'a mut Vec<Type>,
        namer: &'a construct::Namer<'a>,
        source: &'a Source,
    ) -> Self {
        Self {
            locals,
            namer,
            source,
            pending: Vec::new(),
            awaits: false,
            publish: None,
        }
    }
    fn fresh(&mut self, ty: Type) -> u32 {
        let slot = self.locals.len() as u32;
        self.locals.push(ty);
        self.pending.push(slot);
        slot
    }
    fn reject(&self, span: swc_common::Span, construct: &str) -> CompilerError {
        let error = if self.awaits {
            unsupported_suspension_as("await", construct)
        } else {
            rejected(construct)
        };
        self.source.locate(error, span)
    }
    fn reject_named(&self, span: swc_common::Span) -> CompilerError {
        self.reject(span, &self.namer.describe(span).name)
    }

    pub(super) fn statements(&mut self, body: Vec<S>) -> Result<Vec<S>> {
        let mut result = Vec::with_capacity(body.len());
        for statement in body {
            result.extend(self.statement(statement)?);
        }
        Ok(result)
    }
    fn statement(&mut self, statement: S) -> Result<Vec<S>> {
        if !walk::body_suspends(std::slice::from_ref(&statement)) {
            return Ok(vec![statement]);
        }
        let outer = std::mem::take(&mut self.pending);
        let result = self.split_statement(statement);
        let owned = std::mem::replace(&mut self.pending, outer);
        let body = result?;
        Ok(if owned.is_empty() {
            body
        } else {
            vec![S::Scope(owned, body)]
        })
    }
    fn split_statement(&mut self, statement: S) -> Result<Vec<S>> {
        Ok(match statement {
            S::Expr(value) => {
                let (mut prefix, value) = self.expr(value)?;
                if !matches!(value.kind, E::Local(_)) {
                    prefix.push(S::Expr(value));
                }
                prefix
            }
            S::Init(slot, value) => self.then(value, |value| S::Init(slot, value))?,
            S::Set(slot, value) => self.then(value, |value| S::Set(slot, value))?,
            S::Throw(value) => self.then(value, S::Throw)?,
            S::Return(Some(value)) => self.then(value, |value| S::Return(Some(value)))?,
            S::If(test, yes, no) => {
                let (mut prefix, test) = self.expr(test)?;
                let yes = self.statements(yes)?;
                let no = self.statements(no)?;
                prefix.push(S::If(test, yes, no));
                prefix
            }
            S::While(test, body) => {
                let body = self.statements(body)?;
                if walk::suspends(&test) {
                    // The test runs at the top of every iteration; continue
                    // re-enters it there, exactly as the loop head would.
                    let span = test.span;
                    let mut entry = self.loop_test(test)?;
                    entry.extend(body);
                    vec![S::While(boolean(span, true), entry)]
                } else {
                    vec![S::While(test, body)]
                }
            }
            S::DoWhile(test, body) => {
                if walk::suspends(&test) {
                    return Err(self.reject(test.span, "do-while condition"));
                }
                vec![S::DoWhile(test, self.statements(body)?)]
            }
            S::For(init, test, update, body) => {
                let init = self.statements(init)?;
                let update = self.statements(update)?;
                let mut body = self.statements(body)?;
                let test = match test {
                    Some(test) if walk::suspends(&test) => {
                        let mut entry = self.loop_test(test)?;
                        entry.append(&mut body);
                        body = entry;
                        None
                    }
                    test => test,
                };
                vec![S::For(init, test, update, body)]
            }
            S::Switch {
                value,
                discriminant,
                selected,
                cases,
            } => {
                // Case bodies may suspend; the state machine selects the case
                // once. Case tests run lazily during selection and stay rejected.
                if let Some(span) = cases
                    .iter()
                    .find_map(|(test, _)| test.iter().find_map(first_suspension))
                {
                    return Err(self.reject(span, "switch case test"));
                }
                let (mut prefix, value) = self.expr(value)?;
                let cases = cases
                    .into_iter()
                    .map(|(test, body)| Ok((test, self.statements(body)?)))
                    .collect::<Result<_>>()?;
                prefix.push(S::Switch {
                    value,
                    discriminant,
                    selected,
                    cases,
                });
                prefix
            }
            S::TryCatch {
                body,
                binding,
                handler,
                marker,
            } => {
                let body = self.statements(body)?;
                if body_span(&handler).is_none() {
                    return Ok(vec![S::TryCatch {
                        body,
                        binding,
                        handler,
                        marker,
                    }]);
                }
                // A suspending handler runs after the protected statement: the
                // catch only binds the exception and records that it ran. The
                // handler is never protected by its own catch, so this keeps
                // its completions, and its state range is entered directly.
                let span = normalize_span(&handler);
                let caught = self.fresh(Type::Boolean);
                let handler = self.statements(handler)?;
                vec![
                    S::Init(caught, boolean(span, false)),
                    S::TryCatch {
                        body,
                        binding,
                        handler: vec![S::Set(caught, boolean(span, true))],
                        marker,
                    },
                    S::If(local(span, caught, Type::Boolean), handler, vec![]),
                ]
            }
            S::TryFinally {
                body,
                finalizer,
                marker,
                completion,
                exception,
                result,
            } => {
                vec![S::TryFinally {
                    body: self.statements(body)?,
                    finalizer: self.statements(finalizer)?,
                    marker,
                    completion,
                    exception,
                    result,
                }]
            }
            S::Block(body) => vec![S::Block(self.statements(body)?)],
            S::Scope(slots, body) => vec![S::Scope(slots, self.statements(body)?)],
            S::Label {
                id,
                iteration,
                body,
            } => vec![S::Label {
                id,
                iteration,
                body: self.statements(body)?,
            }],
            S::SetField(object, field, value) => {
                let (mut prefix, mut values) = self.operands(vec![object, value])?;
                let value = values.pop();
                let object = values.pop();
                if let (Some(object), Some(value)) = (object, value) {
                    prefix.push(S::SetField(object, field, value));
                }
                prefix
            }
            S::SetIndex {
                array,
                index,
                value,
                slots,
            } => {
                let (mut prefix, mut values) = self.operands(vec![array, index, value])?;
                let (value, index, array) = (values.pop(), values.pop(), values.pop());
                if let (Some(array), Some(index), Some(value)) = (array, index, value) {
                    prefix.push(S::SetIndex {
                        array,
                        index,
                        value,
                        slots,
                    });
                }
                prefix
            }
            S::ArrayLength {
                array,
                length,
                error,
            } => {
                if let Some(span) = first_suspension(&error) {
                    return Err(self.reject(span, "array length assignment"));
                }
                let (mut prefix, mut values) = self.operands(vec![array, length])?;
                let (length, array) = (values.pop(), values.pop());
                if let (Some(array), Some(length)) = (array, length) {
                    prefix.push(S::ArrayLength {
                        array,
                        length,
                        error,
                    });
                }
                prefix
            }
            S::RecordEnumerate { values, .. } => {
                let span = values.iter().find_map(first_suspension);
                return Err(self.reject(span.unwrap_or_default(), "object enumeration"));
            }
            S::InitModuleConstant(_, value) => {
                return Err(self.reject(value.span, "module constant initializer"));
            }
            other @ (S::Trap
            | S::Rebind(_)
            | S::RecordRest { .. }
            | S::Return(None)
            | S::BreakLabel(_)
            | S::ContinueLabel(_)
            | S::Break
            | S::Continue) => vec![other],
        })
    }
    fn then(&mut self, value: Expr, finish: impl FnOnce(Expr) -> S) -> Result<Vec<S>> {
        let (mut prefix, value) = self.expr(value)?;
        prefix.push(finish(value));
        Ok(prefix)
    }
    /// `if (!test) break;` with the test's own temporaries released each iteration.
    fn loop_test(&mut self, test: Expr) -> Result<Vec<S>> {
        let outer = std::mem::take(&mut self.pending);
        let split = self.expr(test);
        let owned = std::mem::replace(&mut self.pending, outer);
        let (mut prefix, test) = split?;
        prefix.push(S::If(test, vec![], vec![S::Break]));
        Ok(if owned.is_empty() {
            prefix
        } else {
            vec![S::Scope(owned, prefix)]
        })
    }

    /// Materialize an already evaluated operand before a later suspension point.
    fn hoist(&mut self, value: Expr, prefix: &mut Vec<S>) -> Result<Expr> {
        if constant(&value) {
            return Ok(value);
        }
        if value.ty == Type::Void {
            return Err(self.reject(value.span, "void operand"));
        }
        let (span, ty) = (value.span, value.ty);
        let slot = self.fresh(ty);
        prefix.push(S::Init(slot, value));
        Ok(local(span, slot, ty))
    }
    /// Unconditional operands evaluated strictly left to right.
    fn operands(&mut self, values: Vec<Expr>) -> Result<(Vec<S>, Vec<Expr>)> {
        let last = values.iter().rposition(walk::suspends);
        let mut prefix = Vec::new();
        let mut result = Vec::with_capacity(values.len());
        for (index, value) in values.into_iter().enumerate() {
            match last {
                Some(last) if index <= last => {
                    let (mut before, value) = self.expr(value)?;
                    prefix.append(&mut before);
                    result.push(if index < last {
                        self.hoist(value, &mut prefix)?
                    } else {
                        value
                    });
                }
                _ => result.push(value),
            }
        }
        Ok((prefix, result))
    }
    fn unary(&mut self, value: Expr, rebuild: impl FnOnce(Box<Expr>) -> E) -> Result<(Vec<S>, E)> {
        let (prefix, value) = self.expr(value)?;
        Ok((prefix, rebuild(Box::new(value))))
    }
    fn pair(
        &mut self,
        a: Expr,
        b: Expr,
        rebuild: impl FnOnce(Box<Expr>, Box<Expr>) -> E,
    ) -> Result<(Vec<S>, E)> {
        let (prefix, mut values) = self.operands(vec![a, b])?;
        let (b, a) = (values.pop(), values.pop());
        let (Some(a), Some(b)) = (a, b) else {
            return Err(CompilerError::Internal("missing normalized operand".into()));
        };
        Ok((prefix, rebuild(Box::new(a), Box::new(b))))
    }
    fn triple(
        &mut self,
        values: [Expr; 3],
        rebuild: impl FnOnce(Box<Expr>, Box<Expr>, Box<Expr>) -> E,
    ) -> Result<(Vec<S>, E)> {
        let (prefix, values) = self.operands(values.into())?;
        let [a, b, c]: [Expr; 3] = values
            .try_into()
            .map_err(|_| CompilerError::Internal("missing normalized operand".into()))?;
        Ok((prefix, rebuild(Box::new(a), Box::new(b), Box::new(c))))
    }

    fn expr(&mut self, expr: Expr) -> Result<Split> {
        if !walk::suspends(&expr) {
            return Ok((vec![], expr));
        }
        let Expr { span, ty, kind } = expr;
        let (prefix, kind) = match kind {
            E::Suspend { value, state } => {
                let (mut prefix, value) = self.expr(*value)?;
                let slot = self.fresh(ty);
                prefix.push(S::Init(
                    slot,
                    Expr {
                        span,
                        ty,
                        kind: E::Suspend {
                            value: Box::new(value),
                            state,
                        },
                    },
                ));
                return Ok((prefix, local(span, slot, ty)));
            }
            E::Await {
                promise,
                slot,
                result,
                relay,
            } => {
                // `await p` (v34): the promise moves into its saved hidden
                // local, the frame suspends on it, and the residual reads
                // the settled promise after resumption.
                let promise_ty = promise.ty;
                let (mut prefix, promise) = self.expr(*promise)?;
                self.pending.push(slot);
                prefix.push(S::Init(slot, promise));
                if let Some((function, frame)) = relay {
                    let frame_ty = self.locals[frame as usize];
                    prefix.push(S::Expr(Expr {
                        span,
                        ty: Type::Void,
                        kind: E::Schedule {
                            function,
                            args: vec![local(span, frame, frame_ty), local(span, slot, promise_ty)],
                            mode: ir::ScheduleMode::Start,
                        },
                    }));
                }
                // An async generator body (v36) is a generator frame whose
                // relay job resumes it: every suspension publishes the state
                // record instead of the promise.
                let (published, input_ty) = match self.publish {
                    Some((state, state_ty, input_ty)) => (local(span, state, state_ty), input_ty),
                    None => (local(span, slot, promise_ty), Type::Undefined),
                };
                let input = self.fresh(input_ty);
                prefix.push(S::Init(
                    input,
                    Expr {
                        span,
                        ty: input_ty,
                        kind: E::Suspend {
                            value: Box::new(published),
                            state: 0,
                        },
                    },
                ));
                return Ok((prefix, *result));
            }
            E::Binary(op @ (BinaryOp::LogicalAnd | BinaryOp::LogicalOr), a, b) => {
                if !walk::suspends(&b) {
                    self.unary(*a, |a| E::Binary(op, a, b))?
                } else {
                    let skip_when_true = op == BinaryOp::LogicalOr;
                    return self.short_circuit(span, ty, *a, *b, |slot| {
                        let test = local(span, slot, ty);
                        (test, skip_when_true)
                    });
                }
            }
            E::UnionLogical(BinaryOp::NullishCoalescing, a, b)
                if a.ty == ty && b.ty == ty && walk::suspends(&b) =>
            {
                return self.short_circuit(span, ty, *a, *b, |slot| {
                    let test = Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::IsNullish(Box::new(local(span, slot, ty))),
                    };
                    (test, false)
                });
            }
            E::UnionLogical(op, a, b) if !walk::suspends(&b) => {
                self.unary(*a, |a| E::UnionLogical(op, a, b))?
            }
            E::Conditional(test, yes, no) => {
                let (mut prefix, test) = self.expr(*test)?;
                if !walk::suspends(&yes) && !walk::suspends(&no) {
                    (prefix, E::Conditional(Box::new(test), yes, no))
                } else {
                    if yes.ty != ty || no.ty != ty {
                        return Err(
                            self.reject(span, "conditional expression with mixed branch types")
                        );
                    }
                    let slot = self.fresh(ty);
                    let yes = self.branch(*yes, slot)?;
                    let no = self.branch(*no, slot)?;
                    prefix.push(S::If(test, yes, no));
                    return Ok((prefix, local(span, slot, ty)));
                }
            }
            E::Sequence(a, b) => {
                if walk::suspends(&b) {
                    let (mut prefix, a) = self.expr(*a)?;
                    if !constant(&a) {
                        prefix.push(S::Expr(a));
                    }
                    let (mut rest, b) = self.expr(*b)?;
                    prefix.append(&mut rest);
                    return Ok((prefix, b));
                }
                self.unary(*a, |a| E::Sequence(a, b))?
            }
            E::ValueBlock {
                output,
                roots,
                body,
                ..
            } => {
                let construct = self.namer.describe(span);
                if !construct.splittable {
                    return Err(self.reject(span, &construct.name));
                }
                self.pending.extend(roots);
                let prefix = self.statements(body)?;
                return Ok((prefix, local(span, output, ty)));
            }
            E::Assignment {
                value, input, body, ..
            } => {
                let construct = self.namer.describe(span);
                if !construct.splittable {
                    return Err(self.reject(span, &construct.name));
                }
                let value_ty = value.ty;
                let (mut prefix, value) = self.expr(*value)?;
                self.pending.push(input);
                prefix.push(S::Init(input, value));
                prefix.extend(self.statements(body)?);
                return Ok((prefix, local(span, input, value_ty)));
            }
            E::Binary(op, a, b) => self.pair(*a, *b, |a, b| E::Binary(op, a, b))?,
            E::NumberSameValue(a, b) => self.pair(*a, *b, E::NumberSameValue)?,
            E::Index(a, b) => self.pair(*a, *b, E::Index)?,
            E::ArrayPush(a, b) => self.pair(*a, *b, E::ArrayPush)?,
            E::StringSearch(mode, a, b, c) => {
                self.triple([*a, *b, *c], |a, b, c| E::StringSearch(mode, a, b, c))?
            }
            E::StringRange {
                value,
                start,
                end,
                substring,
            } => self.triple([*value, *start, *end], |value, start, end| E::StringRange {
                value,
                start,
                end,
                substring,
            })?,
            E::Call(index, args) => {
                let (prefix, args) = self.operands(args)?;
                (prefix, E::Call(index, args))
            }
            E::Host(host, args) => {
                let (prefix, args) = self.operands(args)?;
                (prefix, E::Host(host, args))
            }
            E::Math(mode, args) => {
                let (prefix, args) = self.operands(args)?;
                (prefix, E::Math(mode, args))
            }
            E::Intrinsic(op, args) => {
                let (prefix, args) = self.operands(args)?;
                (prefix, E::Intrinsic(op, args))
            }
            E::NumberPredicate(mode, args) => {
                let (prefix, args) = self.operands(args)?;
                (prefix, E::NumberPredicate(mode, args))
            }
            E::Record(fields) => {
                let (slots, values): (Vec<_>, Vec<_>) = fields.into_iter().unzip();
                let (prefix, values) = self.operands(values)?;
                (prefix, E::Record(slots.into_iter().zip(values).collect()))
            }
            E::Array(elements) if elements.iter().all(Option::is_some) => {
                let (prefix, values) = self.operands(elements.into_iter().flatten().collect())?;
                (prefix, E::Array(values.into_iter().map(Some).collect()))
            }
            E::FunctionCall {
                callee,
                ignored_receiver,
                args,
                slot,
                arguments,
                output,
            } => {
                let receiver = ignored_receiver.is_some();
                let mut values = vec![*callee];
                values.extend(ignored_receiver.map(|value| *value));
                values.extend(args);
                let (prefix, mut values) = self.operands(values)?;
                let args = values.split_off(1 + usize::from(receiver));
                let ignored_receiver = receiver.then(|| values.pop()).flatten().map(Box::new);
                let callee = values
                    .pop()
                    .ok_or_else(|| CompilerError::Internal("missing normalized callee".into()))?;
                (
                    prefix,
                    E::FunctionCall {
                        callee: Box::new(callee),
                        ignored_receiver,
                        args,
                        slot,
                        arguments,
                        output,
                    },
                )
            }
            E::NewError(class, value) => self.unary(*value, |value| E::NewError(class, value))?,
            E::ErrorName(value) => self.unary(*value, E::ErrorName)?,
            E::ErrorMessage(value) => self.unary(*value, E::ErrorMessage)?,
            E::ErrorText(value) => self.unary(*value, E::ErrorText)?,
            E::ErrorOwn(value) => self.unary(*value, E::ErrorOwn)?,
            E::BoxUnion(value) => self.unary(*value, E::BoxUnion)?,
            E::UnboxUnion(value) => self.unary(*value, E::UnboxUnion)?,
            E::IsUndefined(value) => self.unary(*value, E::IsUndefined)?,
            E::IsError(value) => self.unary(*value, E::IsError)?,
            E::IsNullish(value) => self.unary(*value, E::IsNullish)?,
            E::IsUnionMember(value, member) => {
                self.unary(*value, |value| E::IsUnionMember(value, member))?
            }
            E::Undefined(value) => self.unary(*value, E::Undefined)?,
            E::Negative(value) => self.unary(*value, E::Negative)?,
            E::ToBoolean(value) => self.unary(*value, E::ToBoolean)?,
            E::Not(value) => self.unary(*value, E::Not)?,
            E::Capture(value) => self.unary(*value, E::Capture)?,
            E::ResultField(field, value) => {
                self.unary(*value, |value| E::ResultField(field, value))?
            }
            E::Field(value, field) => self.unary(*value, |value| E::Field(value, field))?,
            E::Length(value) => self.unary(*value, E::Length)?,
            E::Json(value, status) => self.unary(*value, |value| E::Json(value, status))?,
            E::ParseJson(value) => self.unary(*value, E::ParseJson)?,
            E::StringTrim { value, mode } => {
                self.unary(*value, |value| E::StringTrim { value, mode })?
            }
            E::TextUnits { value, op } => self.unary(*value, |value| E::TextUnits { value, op })?,
            E::FunctionCheck { value, slot } => {
                self.unary(*value, |value| E::FunctionCheck { value, slot })?
            }
            E::Reinterpret(value) => self.unary(*value, E::Reinterpret)?,
            E::Schedule {
                function,
                args,
                mode,
            } => {
                let (prefix, args) = self.operands(args)?;
                (
                    prefix,
                    E::Schedule {
                        function,
                        args,
                        mode,
                    },
                )
            }
            _ => return Err(self.reject_named(span)),
        };
        Ok((prefix, Expr { span, ty, kind }))
    }
    /// `a && b`, `a || b` and `a ?? b`: the right operand runs only on the
    /// branch the operator selects; the result lives in one hidden local.
    fn short_circuit(
        &mut self,
        span: swc_common::Span,
        ty: Type,
        left: Expr,
        right: Expr,
        test: impl FnOnce(u32) -> (Expr, bool),
    ) -> Result<Split> {
        let (mut prefix, left) = self.expr(left)?;
        let slot = self.fresh(ty);
        prefix.push(S::Init(slot, left));
        let (test, skip_when_true) = test(slot);
        let branch = self.branch(right, slot)?;
        prefix.push(if skip_when_true {
            S::If(test, vec![], branch)
        } else {
            S::If(test, branch, vec![])
        });
        Ok((prefix, local(span, slot, ty)))
    }
    fn branch(&mut self, value: Expr, slot: u32) -> Result<Vec<S>> {
        let (mut body, value) = self.expr(value)?;
        body.push(S::Set(slot, value));
        Ok(body)
    }
}

fn first_suspension(expr: &Expr) -> Option<swc_common::Span> {
    let mut span = None;
    walk::expression(expr, &mut |e| {
        if span.is_none() && walk::point(e) {
            span = Some(e.span);
        }
    });
    span
}
fn normalize_span(body: &[S]) -> swc_common::Span {
    body_span(body).unwrap_or_default()
}
pub(super) fn body_span(body: &[S]) -> Option<swc_common::Span> {
    let mut span = None;
    walk::statements(body, &mut |e| {
        if span.is_none() && walk::point(e) {
            span = Some(e.span);
        }
    });
    span
}
