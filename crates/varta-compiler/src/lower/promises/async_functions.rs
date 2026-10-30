//! Async functions (v34 `async_functions`, K4.4 step 4): `await` and the
//! split of an `async function` into its wrapper and its scheduled body.
use super::*;

impl Lower<'_> {
    /// `await e` inside an async function body.
    pub(in crate::lower) fn await_expression(
        &mut self,
        expression: &ast::AwaitExpr,
    ) -> Result<Expr> {
        let span = expression.span;
        if self.asynchronous.is_none() && self.async_body().is_none() {
            return Err(self.source.locate(
                rejected("await outside an async function (top-level await is not supported)"),
                span,
            ));
        }
        let value = self.expr(&expression.arg)?;
        self.await_value(span, value)
    }
    /// `await value` of a lowered operand: a promise is awaited itself, any
    /// other value through a fulfilled promise (PromiseResolve).
    pub(in crate::lower) fn await_value(
        &mut self,
        span: swc_common::Span,
        mut value: Expr,
    ) -> Result<Expr> {
        if value.ty == Type::Void {
            // `await f()` of a void call awaits `undefined` after the call.
            value = Expr {
                span: value.span,
                ty: Type::Undefined,
                kind: E::Undefined(Box::new(value)),
            };
        }
        if self.promise_member(value.ty, value.span)?.is_some() {
            let result = self
                .await_union(span, value)?
                .ok_or_else(|| CompilerError::Internal("missing promise member".into()))?;
            self.flow_reentry();
            return Ok(result);
        }
        let value = self.guarded_unknown(value)?;
        let promise = match value.ty {
            Type::Promise(_) => value,
            ty => {
                // PromiseResolve: a non-promise value is awaited through a new
                // fulfilled promise, so it still takes one job.
                let id = match intern(ty, self.types) {
                    Ok(Type::Promise(id)) => id,
                    Ok(_) => return Err(CompilerError::Internal("missing promise".into())),
                    Err(error) => return Err(self.source.locate(error, value.span)),
                };
                self.promise_object(span, id, Some(value))?
            }
        };
        let result = self.await_promise(span, promise)?;
        self.flow_reentry();
        Ok(result)
    }
}

impl Lower<'_> {
    /// The body of an expression-bodied async arrow (v35). `result` is the
    /// promise's `T` when an annotation or context fixes it; otherwise the
    /// value's type does, and a promise value is adopted.
    pub(in crate::lower) fn async_expression_body(
        &mut self,
        expr: &ast::Expr,
        result: Option<Type>,
    ) -> Result<Vec<S>> {
        let value = self.expr_expected(expr, result.filter(|ty| *ty != Type::Void))?;
        let Some(result) = result else {
            return Ok(match value.ty {
                Type::Promise(id) => {
                    self.result = self.types.promises[id as usize].value;
                    self.asynchronous = Some(id);
                    vec![S::Return(Some(value))]
                }
                Type::Void => {
                    self.result = Type::Void;
                    vec![S::Expr(value), S::Return(None)]
                }
                ty => {
                    self.result = ty.erased();
                    let value = unions::widen(value, Some(self.result), self.types)?;
                    vec![S::Return(Some(value))]
                }
            });
        };
        if result == Type::Void && matches!(value.ty, Type::Void | Type::Undefined) {
            return Ok(vec![S::Expr(value), S::Return(None)]);
        }
        if self
            .asynchronous
            .is_some_and(|id| value.ty == Type::Promise(id))
        {
            return Ok(vec![S::Return(Some(value))]);
        }
        let value = unions::widen(value, Some(result), self.types)?;
        if value.ty != result {
            return Err(self.source.locate(
                type_error(format!(
                    "async arrow result mismatch: expected {result}, found {}",
                    value.ty
                )),
                value.span,
            ));
        }
        Ok(vec![S::Return(Some(value))])
    }
}

/// Exit of a rewritten async body: a `return` stores its value and leaves
/// the labelled body, which then settles the async function's promise.
struct Exit {
    label: u32,
    value: Option<u32>,
    promise: u32,
    adopt: u32,
    promise_ty: Type,
}
fn rewrite(body: Vec<S>, exit: &Exit) -> Vec<S> {
    body.into_iter()
        .map(|s| rewrite_statement(s, exit))
        .collect()
}
fn rewrite_statement(statement: S, exit: &Exit) -> S {
    match statement {
        S::Return(None) => S::BreakLabel(exit.label),
        S::Return(Some(value)) => {
            let span = value.span;
            let mut exit_body = if value.ty == exit.promise_ty {
                vec![
                    S::Set(exit.promise, value),
                    S::Set(exit.adopt, boolean(span, true)),
                ]
            } else if let Some(slot) = exit.value {
                vec![S::Set(slot, value)]
            } else {
                vec![S::Expr(value)]
            };
            exit_body.push(S::BreakLabel(exit.label));
            S::Block(exit_body)
        }
        S::TryCatch {
            body,
            binding,
            handler,
            marker,
        } => S::TryCatch {
            body: rewrite(body, exit),
            binding,
            handler: rewrite(handler, exit),
            marker,
        },
        S::TryFinally {
            body,
            finalizer,
            marker,
            completion,
            exception,
            result,
        } => S::TryFinally {
            body: rewrite(body, exit),
            finalizer: rewrite(finalizer, exit),
            marker,
            completion,
            exception,
            result,
        },
        S::Block(body) => S::Block(rewrite(body, exit)),
        S::Scope(slots, body) => S::Scope(slots, rewrite(body, exit)),
        S::Label {
            id,
            iteration,
            body,
        } => S::Label {
            id,
            iteration,
            body: rewrite(body, exit),
        },
        S::If(test, yes, no) => S::If(test, rewrite(yes, exit), rewrite(no, exit)),
        S::While(test, body) => S::While(test, rewrite(body, exit)),
        S::DoWhile(test, body) => S::DoWhile(test, rewrite(body, exit)),
        S::For(init, test, update, body) => S::For(
            rewrite(init, exit),
            test,
            rewrite(update, exit),
            rewrite(body, exit),
        ),
        S::Switch {
            value,
            discriminant,
            selected,
            cases,
        } => S::Switch {
            value,
            discriminant,
            selected,
            cases: cases
                .into_iter()
                .map(|(test, body)| (test, rewrite(body, exit)))
                .collect(),
        },
        other => other,
    }
}

/// Split a lowered async function, method or closure into its wrapper and its
/// scheduled body. The wrapper (at the declaration's own index, so every call,
/// dispatch and function value reaches it) creates the promise, starts the
/// body frame and returns the promise. The body takes the wrapper's physical
/// parameters (a method's receiver is its first, plain parameter) followed by
/// the promise, and settles it on every completion: a returned value fulfills,
/// a returned promise is adopted and an uncaught exception rejects.
///
/// A closure's body is itself an environment function with the closure's
/// captures (v35 `async_methods`): the wrapper forwards its own environment,
/// so the body frame keeps the very binding cells the closure captured.
pub(in crate::lower) struct AsyncSplit<'s> {
    /// The wrapper's physical parameters: receiver, parameters, self value.
    pub incoming: Vec<Type>,
    pub shape: ir::ParameterShape,
    pub receiver: Option<Type>,
    pub self_slot: Option<u32>,
    /// Captured binding slots of an async closure (empty otherwise).
    pub captures: Vec<u32>,
    pub environment: bool,
    pub promise: u32,
    pub entry: bool,
    pub span: swc_common::Span,
    pub source: &'s Source,
}
impl<'s> AsyncSplit<'s> {
    /// The split of a declaration or class method lowered from `signature`.
    pub(in crate::lower) fn declaration(
        signature: &Signature,
        promise: u32,
        entry: bool,
        span: swc_common::Span,
        source: &'s Source,
    ) -> Self {
        Self {
            incoming: signature
                .receiver
                .into_iter()
                .chain(signature.params.iter().copied())
                .collect(),
            shape: signature.shape.clone(),
            receiver: signature.receiver,
            self_slot: None,
            captures: Vec::new(),
            environment: false,
            promise,
            entry,
            span,
            source,
        }
    }
}
impl Lower<'_> {
    pub(in crate::lower) fn split_async(
        &mut self,
        split: AsyncSplit<'_>,
        statements: Vec<S>,
    ) -> Result<ir::Function> {
        let AsyncSplit {
            incoming,
            shape,
            receiver,
            self_slot,
            captures,
            environment,
            promise,
            entry,
            span,
            source,
        } = split;
        if promise as usize != incoming.len() {
            return Err(CompilerError::Internal(
                "async promise slot must follow the parameters".into(),
            ));
        }
        let promise_ty = Type::Promise(promise_id(self, promise)?);
        let Type::Promise(id) = promise_ty else {
            return Err(CompilerError::Internal("missing async promise".into()));
        };
        let value = self.types.promises[id as usize].value;
        let reason = self.exception_type()?;
        let fulfill = self.fulfill_helper(span, id)?;
        let reject = self.reject_helper(span, id)?;
        let resolve = self.resolve_helper(span, id)?;
        let label = self.next_label;
        self.next_label += 1;
        let exit = Exit {
            label,
            value: (value != Type::Void).then(|| self.hidden(value)),
            promise: self.hidden(promise_ty),
            adopt: self.hidden(Type::Boolean),
            promise_ty,
        };
        let e = self.hidden(reason);
        let marker = self.hidden(Type::Number);
        let p = || local(span, promise, promise_ty);
        let fulfilled = match exit.value {
            Some(slot) => local(span, slot, value),
            None => unions::widen(undefined(span), Some(stored(value)), self.types)?,
        };
        let mut body = Vec::new();
        if let Some(slot) = exit.value {
            body.push(S::Init(slot, zero(span, value)));
        }
        body.extend([
            S::Init(exit.promise, zero(span, promise_ty)),
            S::Init(exit.adopt, boolean(span, false)),
            S::TryCatch {
                body: vec![
                    S::Label {
                        id: label,
                        iteration: false,
                        body: rewrite(statements, &exit),
                    },
                    S::If(
                        local(span, exit.adopt, Type::Boolean),
                        vec![invoke(
                            span,
                            resolve,
                            vec![p(), local(span, exit.promise, promise_ty)],
                        )],
                        vec![invoke(span, fulfill, vec![p(), fulfilled])],
                    ),
                ],
                binding: Some(e),
                handler: vec![invoke(span, reject, vec![p(), local(span, e, reason)])],
                marker,
            },
        ]);
        let mut params = incoming.clone();
        params.push(promise_ty);
        let index = *self.next_function;
        if index >= self.types.function_limit() {
            return Err(invalid("too many functions including async bodies"));
        }
        *self.next_function += 1;
        let locals: Vec<Type> = (0..self.next)
            .map(|slot| self.local_types.get(&slot).copied().unwrap_or(Type::Number))
            .collect();
        // The wrapper needs its parameters, the promise and the captured
        // cells it validates; every other slot is an unused scalar.
        let used = captures.iter().copied().fold(promise, u32::max) as usize + 1;
        let wrapper_locals = locals[..used]
            .iter()
            .enumerate()
            .map(|(slot, ty)| {
                let slot = slot as u32;
                if slot <= promise || captures.contains(&slot) {
                    *ty
                } else {
                    Type::Number
                }
            })
            .collect();
        let body_function = ir::Function {
            shape: ir::ParameterShape::fixed(params.len()),
            self_slot: None,
            receiver: None,
            cells: std::mem::take(&mut self.cells),
            captures: captures.clone(),
            environment,
            generator: None,
            source: source.clone(),
            span,
            params,
            result: Type::Void,
            locals,
            body,
        };
        self.generated.insert(index, body_function);
        self.types.asynchrony.scheduled.insert(index);
        let mut args: Vec<Expr> = incoming
            .iter()
            .enumerate()
            .map(|(index, ty)| local(span, index as u32, *ty))
            .collect();
        args.push(local(span, promise, promise_ty));
        let mut wrapper = vec![
            S::Init(promise, self.promise_object(span, id, None)?),
            schedule(span, index, args, ScheduleMode::Start),
        ];
        wrapper.push(if entry {
            S::Return(None)
        } else {
            S::Return(Some(local(span, promise, promise_ty)))
        });
        Ok(ir::Function {
            shape,
            self_slot,
            receiver,
            cells: captures.iter().copied().collect(),
            captures,
            environment,
            generator: None,
            source: source.clone(),
            span,
            params: incoming,
            result: if entry { Type::Void } else { promise_ty },
            locals: wrapper_locals,
            body: wrapper,
        })
    }
}
fn promise_id(lower: &Lower<'_>, slot: u32) -> Result<u32> {
    match lower.local_types.get(&slot) {
        Some(Type::Promise(id)) => Ok(*id),
        _ => Err(CompilerError::Internal("missing async promise slot".into())),
    }
}
