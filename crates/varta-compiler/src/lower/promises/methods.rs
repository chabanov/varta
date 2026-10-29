//! `new Promise`, `Promise.resolve`/`Promise.reject` and the `then`, `catch`
//! and `finally` reactions, each a job frame of a synthesized function.
use super::*;

impl Lower<'_> {
    /// `new Promise<T>((resolve, reject) => ...)`: the executor runs now with
    /// two resolving closures sharing the promise's one-time resolution flag;
    /// an exception from the executor rejects unless it already resolved.
    pub(in crate::lower) fn new_promise(
        &mut self,
        new: &ast::NewExpr,
        expected: Option<Type>,
    ) -> Result<Expr> {
        let span = new.span;
        let value = match self.explicit_value(new.type_args.as_deref(), "Promise")? {
            Some(value) => value,
            None => self.expected_value(expected).ok_or_else(|| {
                self.source.locate(
                    type_error("new Promise requires a type argument or a Promise<T> context"),
                    span,
                )
            })?,
        };
        let ty = intern(value, self.types).map_err(|e| self.source.locate(e, span))?;
        let Type::Promise(id) = ty else {
            return Err(CompilerError::Internal("missing promise".into()));
        };
        let args = new.args.as_deref().unwrap_or_default();
        let [executor] = args else {
            return Err(self
                .source
                .locate(rejected("new Promise without exactly one executor"), span));
        };
        if executor.spread.is_some() {
            return Err(self
                .source
                .locate(rejected("spread executor argument"), executor.span()));
        }
        let reason = self.exception_type()?;
        let stored_value = stored(value);
        let optional = |count: usize| ir::ParameterShape {
            optional: vec![true; count],
            rest: false,
        };
        let resolve_ty = intern_full(
            None,
            vec![stored_value],
            if value == Type::Void {
                optional(1)
            } else {
                ir::ParameterShape::fixed(1)
            },
            Type::Void,
            self.types,
        )?;
        let reject_ty = intern_full(None, vec![reason], optional(1), Type::Void, self.types)?;
        let executor = self
            .promise_callback(&executor.expr, &[resolve_ty, reject_ty], Some(Type::Void))?
            .ok_or_else(|| {
                self.source.locate(
                    type_error("new Promise requires an executor function"),
                    span,
                )
            })?;
        let Type::Function(signature) = executor.ty else {
            return Err(CompilerError::Internal("missing executor type".into()));
        };
        let arity = self.types.functions[signature as usize].params.len();
        let executor_ty = executor.ty;
        let exec = self.hidden(executor_ty);
        let promise = self.hidden(ty);
        let binding = Binding {
            identity: (self.index, promise),
            slot: promise,
            ty: Some(ty),
            initialized: true,
            mutable: false,
            readonly: false,
        };
        let mut body = vec![
            S::Init(exec, executor),
            S::Init(promise, self.promise_object(span, id, None)?),
        ];
        let mut functions = Vec::new();
        for (helper, function_ty) in [
            (Helper::ResolveFunction, resolve_ty),
            (Helper::RejectFunction, reject_ty),
        ]
        .into_iter()
        .take(arity)
        {
            let index = self.resolving_function(span, id, helper, &binding)?;
            let slot = self.capture_binding(&binding)?;
            functions.push(Expr {
                span,
                ty: function_ty,
                kind: E::FunctionRef {
                    index,
                    fresh: true,
                    captures: vec![slot],
                },
            });
        }
        let invocation = self.invoke_handler(span, local(span, exec, executor_ty), functions)?;
        let e = self.hidden(reason);
        let reject = self.reject_helper(span, id)?;
        body.push(S::TryCatch {
            body: vec![S::Expr(invocation)],
            binding: Some(e),
            handler: vec![S::If(
                not(self.read_promise(local(span, promise, ty), LOCKED)?),
                vec![
                    self.write_promise(local(span, promise, ty), LOCKED, boolean(span, true))?,
                    invoke(
                        span,
                        reject,
                        vec![local(span, promise, ty), local(span, e, reason)],
                    ),
                ],
                vec![],
            )],
            marker: self.hidden(Type::Number),
        });
        let output = self.hidden(ty);
        body.push(S::Init(output, local(span, promise, ty)));
        self.flow_reentry();
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker: self.hidden(Type::Number),
                roots: vec![exec, output],
                body,
            },
        })
    }
    /// The resolve or reject closure of `new Promise`: it captures the promise
    /// and settles it only on its first call through either closure.
    fn resolving_function(
        &mut self,
        span: swc_common::Span,
        id: u32,
        helper: Helper,
        binding: &Binding,
    ) -> Result<usize> {
        let ty = Type::Promise(id);
        let value = stored(self.types.promises[id as usize].value);
        let reason = self.exception_type()?;
        let (param, optional) = if helper == Helper::ResolveFunction {
            (value, self.types.promises[id as usize].value == Type::Void)
        } else {
            (reason, true)
        };
        let binding = binding.clone();
        self.memoized(helper, vec![ty], |lower| {
            lower.synthesize(
                Synthetic {
                    span,
                    params: vec![param],
                    shape: ir::ParameterShape {
                        optional: vec![optional],
                        rest: false,
                    },
                    capture: Some(binding),
                    scheduled: false,
                },
                |child, params, captured| {
                    let p = captured
                        .ok_or_else(|| CompilerError::Internal("missing promise capture".into()))?;
                    let settle = if helper == Helper::ResolveFunction {
                        child.fulfill_helper(span, id)?
                    } else {
                        child.reject_helper(span, id)?
                    };
                    Ok(vec![S::If(
                        not(child.read_promise(local(span, p, ty), LOCKED)?),
                        vec![
                            child.write_promise(local(span, p, ty), LOCKED, boolean(span, true))?,
                            invoke(
                                span,
                                settle,
                                vec![local(span, p, ty), local(span, params[0], param)],
                            ),
                        ],
                        vec![],
                    )])
                },
            )
        })
    }
    /// `Promise.resolve(v)` and `Promise.reject(e)`; under v35 also the
    /// combinators, which earlier profiles reject by name.
    pub(in crate::lower) fn promise_static(
        &mut self,
        call: &ast::CallExpr,
        member: &ast::MemberExpr,
        expected: Option<Type>,
    ) -> Result<Expr> {
        let span = call.span;
        let ast::MemberProp::Ident(name) = &member.prop else {
            return Err(self
                .source
                .locate(rejected("computed Promise member"), member.span));
        };
        let method = name.sym.as_ref();
        if self.types.profile.promise_values()
            && let Some(combinator) = super::combinators::Combinator::from_name(method)
        {
            return self.promise_combinator(call, combinator);
        }
        if !matches!(method, "resolve" | "reject") {
            return Err(self
                .source
                .locate(rejected(&format!("Promise.{method}")), member.span));
        }
        if call.args.len() > 1 || call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(self
                .source
                .locate(rejected(&format!("Promise.{method} arguments")), span));
        }
        let explicit =
            self.explicit_value(call.type_args.as_deref(), &format!("Promise.{method}"))?;
        let context = explicit.or_else(|| self.expected_value(expected));
        if method == "resolve" {
            let Some(arg) = call.args.first() else {
                let value = context.unwrap_or(Type::Void);
                if stored(value) != Type::Undefined
                    && !unions::members(value, self.types).contains(&Type::Undefined)
                {
                    return Err(self.source.locate(
                        type_error(
                            "Promise.resolve() without a value requires a type accepting undefined",
                        ),
                        span,
                    ));
                }
                let ty = intern(value, self.types).map_err(|e| self.source.locate(e, span))?;
                let Type::Promise(id) = ty else {
                    return Err(CompilerError::Internal("missing promise".into()));
                };
                return self.promise_object(span, id, Some(undefined(span)));
            };
            let value = self.expr_expected(&arg.expr, context.map(stored))?;
            if let Type::Promise(_) = value.ty {
                // PromiseResolve returns an internal promise itself.
                if explicit.is_some_and(|ty| intern(ty, self.types).ok() != Some(value.ty)) {
                    return Err(self.source.locate(
                        type_error("Promise.resolve type argument does not match the promise"),
                        value.span,
                    ));
                }
                return Ok(value);
            }
            if self.promise_member(value.ty, value.span)?.is_some() {
                return self
                    .resolve_union(span, value, context)?
                    .ok_or_else(|| CompilerError::Internal("missing promise member".into()));
            }
            let value = self.guarded_unknown(value)?;
            let target = context.unwrap_or(value.ty);
            let ty = intern(target, self.types).map_err(|e| self.source.locate(e, span))?;
            let Type::Promise(id) = ty else {
                return Err(CompilerError::Internal("missing promise".into()));
            };
            let value = unions::widen(value, Some(stored(target)), self.types)?;
            if value.ty != stored(target) {
                return Err(self.source.locate(
                    type_error(format!(
                        "Promise.resolve value mismatch: expected {}, found {}",
                        stored(target),
                        value.ty
                    )),
                    value.span,
                ));
            }
            return self.promise_object(span, id, Some(value));
        }
        let reason = match call.args.first() {
            Some(arg) => {
                let value = self.expr(&arg.expr)?;
                self.thrown(value)?
            }
            None => {
                let ty = self.exception_type()?;
                unions::widen(undefined(span), Some(ty), self.types)?
            }
        };
        let value = context.unwrap_or(Type::Void);
        let ty = intern(value, self.types).map_err(|e| self.source.locate(e, span))?;
        let Type::Promise(id) = ty else {
            return Err(CompilerError::Internal("missing promise".into()));
        };
        let reason_ty = reason.ty;
        let reason_slot = self.hidden(reason_ty);
        let promise = self.hidden(ty);
        let reject = self.reject_helper(span, id)?;
        let body = vec![
            S::Init(reason_slot, reason),
            S::Init(promise, self.promise_object(span, id, None)?),
            invoke(
                span,
                reject,
                vec![
                    local(span, promise, ty),
                    local(span, reason_slot, reason_ty),
                ],
            ),
        ];
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output: promise,
                marker: self.hidden(Type::Number),
                roots: vec![reason_slot, promise],
                body,
            },
        })
    }
    /// `p.then(f, r)`, `p.catch(r)` and `p.finally(f)`.
    pub(in crate::lower) fn promise_method(
        &mut self,
        call: &ast::CallExpr,
        receiver: Expr,
        method: &str,
    ) -> Result<Expr> {
        let span = call.span;
        let Type::Promise(id) = receiver.ty else {
            return Err(self.source.locate(
                type_error(format!("{method} requires a Promise receiver")),
                receiver.span,
            ));
        };
        if call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(self
                .source
                .locate(rejected(&format!("spread {method} arguments")), span));
        }
        let limit = if method == "then" { 2 } else { 1 };
        if call.args.is_empty() || call.args.len() > limit {
            return Err(self.source.locate(
                rejected(&format!("{method} with {} arguments", call.args.len())),
                span,
            ));
        }
        let value = stored(self.types.promises[id as usize].value);
        let reason = self.exception_type()?;
        if method == "finally" {
            let callback = self.promise_callback(&call.args[0].expr, &[], None)?;
            return self.promise_finally(span, receiver, callback);
        }
        let (fulfilled, rejected_arg) = if method == "then" {
            (Some(&call.args[0]), call.args.get(1))
        } else {
            (None, Some(&call.args[0]))
        };
        let on_fulfilled = fulfilled
            .map(|arg| self.promise_callback(&arg.expr, &[value], None))
            .transpose()?
            .flatten();
        let on_rejected = rejected_arg
            .map(|arg| self.promise_callback(&arg.expr, &[reason], None))
            .transpose()?
            .flatten();
        let result = |lower: &Self, handler: &Option<Expr>| {
            handler.as_ref().map(|value| match value.ty {
                Type::Function(fid) => lower.outcome(lower.types.functions[fid as usize].result),
                _ => Outcome::Void,
            })
        };
        let fulfilled_outcome = result(self, &on_fulfilled);
        let rejected_outcome = result(self, &on_rejected);
        let source = self.types.promises[id as usize].value;
        let fulfilled_value = fulfilled_outcome.map_or_else(
            || (value, source == Type::Void),
            |outcome| self.contribution(outcome),
        );
        let target = match rejected_outcome.map(|outcome| self.contribution(outcome)) {
            None => fulfilled_value,
            Some(other) if other.0 == fulfilled_value.0 => (other.0, other.1 && fulfilled_value.1),
            Some(other) => (
                unions::intern(vec![fulfilled_value.0, other.0], self.types).map_err(|_| {
                    self.source.locate(
                        type_error(format!(
                            "then handler results {} and {} do not form a checked union",
                            fulfilled_value.0, other.0
                        )),
                        span,
                    )
                })?,
                false,
            ),
        };
        let result_value = if target.0 == Type::Undefined && target.1 {
            Type::Void
        } else {
            target.0
        };
        let derived = intern(result_value, self.types).map_err(|e| self.source.locate(e, span))?;
        let Type::Promise(derived_id) = derived else {
            return Err(CompilerError::Internal("missing promise".into()));
        };
        let handler_ty = |handler: &Option<Expr>| handler.as_ref().map_or(Type::Void, |h| h.ty);
        let key = vec![
            receiver.ty,
            handler_ty(&on_fulfilled),
            handler_ty(&on_rejected),
            derived,
        ];
        let mut params = vec![receiver.ty];
        params.extend(on_fulfilled.as_ref().map(|h| h.ty));
        params.extend(on_rejected.as_ref().map(|h| h.ty));
        params.push(derived);
        let has_fulfilled = on_fulfilled.is_some();
        let has_rejected = on_rejected.is_some();
        let receiver_ty = receiver.ty;
        let frame = self.memoized(Helper::Then, key, |lower| {
            lower.synthesize(
                Self::plain(span, params.clone(), true),
                |child, slots, _| {
                    let p = slots[0];
                    let f = has_fulfilled.then(|| slots[1]);
                    let r = has_rejected.then(|| slots[1 + usize::from(has_fulfilled)]);
                    let d = slots[slots.len() - 1];
                    let reject = child.reject_helper(span, derived_id)?;
                    let caught = child.hidden(Type::Boolean);
                    let caught_reason = child.hidden(reason);
                    let v = child.hidden(value);
                    let mut body = vec![
                        S::Init(v, zero(span, value)),
                        S::Init(caught, boolean(span, false)),
                        S::Init(caught_reason, zero(span, reason)),
                    ];
                    let awaited = child.await_promise(span, local(span, p, receiver_ty))?;
                    if r.is_some() {
                        let e = child.hidden(reason);
                        body.push(S::TryCatch {
                            body: vec![S::Set(v, awaited)],
                            binding: Some(e),
                            handler: vec![
                                S::Set(caught, boolean(span, true)),
                                S::Set(caught_reason, local(span, e, reason)),
                            ],
                            marker: child.hidden(Type::Number),
                        });
                    } else {
                        body.push(S::Set(v, awaited));
                    }
                    let fulfilled_result = match f {
                        Some(f) => {
                            let fty = params[1];
                            child.invoke_handler(
                                span,
                                local(span, f, fty),
                                vec![local(span, v, value)],
                            )?
                        }
                        None => local(span, v, value),
                    };
                    let fulfil =
                        child.settle_with(span, || local(span, d, derived), fulfilled_result)?;
                    match r {
                        Some(r) => {
                            let rty = params[1 + usize::from(has_fulfilled)];
                            let reaction = child.invoke_handler(
                                span,
                                local(span, r, rty),
                                vec![local(span, caught_reason, reason)],
                            )?;
                            let recover =
                                child.settle_with(span, || local(span, d, derived), reaction)?;
                            body.push(S::If(local(span, caught, Type::Boolean), recover, fulfil));
                        }
                        None => body.extend(fulfil),
                    }
                    let e = child.hidden(reason);
                    Ok(vec![S::TryCatch {
                        body,
                        binding: Some(e),
                        handler: vec![invoke(
                            span,
                            reject,
                            vec![local(span, d, derived), local(span, e, reason)],
                        )],
                        marker: child.hidden(Type::Number),
                    }])
                },
            )
        })?;
        let mut body = Vec::new();
        let mut args = Vec::new();
        for (value, ty) in std::iter::once(receiver)
            .chain(on_fulfilled)
            .chain(on_rejected)
            .map(|value| {
                let ty = value.ty;
                (value, ty)
            })
        {
            let slot = self.hidden(ty);
            body.push(S::Init(slot, value));
            args.push((slot, ty));
        }
        let output = self.hidden(derived);
        body.push(S::Init(
            output,
            self.promise_object(span, derived_id, None)?,
        ));
        let mut frame_args: Vec<Expr> = args
            .iter()
            .map(|&(slot, ty)| local(span, slot, ty))
            .collect();
        frame_args.push(local(span, output, derived));
        body.push(schedule(span, frame, frame_args, ScheduleMode::Start));
        self.flow_reentry();
        let mut roots: Vec<u32> = args.iter().map(|(slot, _)| *slot).collect();
        roots.push(output);
        Ok(Expr {
            span,
            ty: derived,
            kind: E::ValueBlock {
                output,
                marker: self.hidden(Type::Number),
                roots,
                body,
            },
        })
    }
    /// `p.finally(f)`: `f` runs after either settlement; the derived promise
    /// then adopts `PromiseResolve(f()).then(() => value or throw reason)`.
    fn promise_finally(
        &mut self,
        span: swc_common::Span,
        receiver: Expr,
        callback: Option<Expr>,
    ) -> Result<Expr> {
        let Some(callback) = callback else {
            return Err(self
                .source
                .locate(rejected("finally without a callback"), span));
        };
        let receiver_ty = receiver.ty;
        let (id, info) = self.promise_info(receiver_ty)?;
        let value = stored(info.value);
        let reason = self.exception_type()?;
        let Type::Function(fid) = callback.ty else {
            return Err(CompilerError::Internal("missing finally callback".into()));
        };
        let outcome = self.outcome(self.types.functions[fid as usize].result);
        let awaited_ty = match outcome {
            Outcome::Void => intern(Type::Void, self.types)?,
            Outcome::Value(ty) => {
                intern(ty, self.types).map_err(|e| self.source.locate(e, span))?
            }
            Outcome::Promise(source) => Type::Promise(source),
        };
        let callback_ty = callback.ty;
        let tail_value = self.finally_tail(span, Helper::TailValue, awaited_ty, id)?;
        let tail_throw = self.finally_tail(span, Helper::TailThrow, awaited_ty, id)?;
        let frame = self.memoized(Helper::Finally, vec![receiver_ty, callback_ty], |lower| {
            lower.synthesize(
                Self::plain(span, vec![receiver_ty, callback_ty, receiver_ty], true),
                |child, slots, _| {
                    let (p, f, d) = (slots[0], slots[1], slots[2]);
                    let resolve = child.resolve_helper(span, id)?;
                    let reject = child.reject_helper(span, id)?;
                    let caught = child.hidden(Type::Boolean);
                    let caught_reason = child.hidden(reason);
                    let v = child.hidden(value);
                    let e = child.hidden(reason);
                    let awaited = child.await_promise(span, local(span, p, receiver_ty))?;
                    let mut body = vec![
                        S::Init(v, zero(span, value)),
                        S::Init(caught, boolean(span, false)),
                        S::Init(caught_reason, zero(span, reason)),
                        S::TryCatch {
                            body: vec![S::Set(v, awaited)],
                            binding: Some(e),
                            handler: vec![
                                S::Set(caught, boolean(span, true)),
                                S::Set(caught_reason, local(span, e, reason)),
                            ],
                            marker: child.hidden(Type::Number),
                        },
                    ];
                    let result = child.invoke_handler(span, local(span, f, callback_ty), vec![])?;
                    let awaited_slot = child.hidden(awaited_ty);
                    let (awaited_id, _) = child.promise_info(awaited_ty)?;
                    match outcome {
                        Outcome::Void => {
                            body.push(S::Expr(result));
                            let resolved =
                                child.promise_object(span, awaited_id, Some(undefined(span)))?;
                            body.push(S::Init(awaited_slot, resolved));
                        }
                        Outcome::Promise(_) => body.push(S::Init(awaited_slot, result)),
                        Outcome::Value(ty) => {
                            let slot = child.hidden(ty);
                            body.push(S::Init(slot, result));
                            let resolved = child.promise_object(
                                span,
                                awaited_id,
                                Some(local(span, slot, ty)),
                            )?;
                            body.push(S::Init(awaited_slot, resolved));
                        }
                    }
                    let q = child.hidden(receiver_ty);
                    body.push(S::Init(q, child.promise_object(span, id, None)?));
                    body.push(S::If(
                        local(span, caught, Type::Boolean),
                        vec![schedule(
                            span,
                            tail_throw,
                            vec![
                                local(span, awaited_slot, awaited_ty),
                                local(span, caught_reason, reason),
                                local(span, q, receiver_ty),
                            ],
                            ScheduleMode::Start,
                        )],
                        vec![schedule(
                            span,
                            tail_value,
                            vec![
                                local(span, awaited_slot, awaited_ty),
                                local(span, v, value),
                                local(span, q, receiver_ty),
                            ],
                            ScheduleMode::Start,
                        )],
                    ));
                    body.push(invoke(
                        span,
                        resolve,
                        vec![local(span, d, receiver_ty), local(span, q, receiver_ty)],
                    ));
                    let outer = child.hidden(reason);
                    Ok(vec![S::TryCatch {
                        body,
                        binding: Some(outer),
                        handler: vec![invoke(
                            span,
                            reject,
                            vec![local(span, d, receiver_ty), local(span, outer, reason)],
                        )],
                        marker: child.hidden(Type::Number),
                    }])
                },
            )
        })?;
        let receiver_slot = self.hidden(receiver_ty);
        let callback_slot = self.hidden(callback_ty);
        let output = self.hidden(receiver_ty);
        let body = vec![
            S::Init(receiver_slot, receiver),
            S::Init(callback_slot, callback),
            S::Init(output, self.promise_object(span, id, None)?),
            schedule(
                span,
                frame,
                vec![
                    local(span, receiver_slot, receiver_ty),
                    local(span, callback_slot, callback_ty),
                    local(span, output, receiver_ty),
                ],
                ScheduleMode::Start,
            ),
        ];
        self.flow_reentry();
        Ok(Expr {
            span,
            ty: receiver_ty,
            kind: E::ValueBlock {
                output,
                marker: self.hidden(Type::Number),
                roots: vec![receiver_slot, callback_slot, output],
                body,
            },
        })
    }
    /// The `then` reaction inside `finally`: await the callback's promise,
    /// then fulfill with the original value (`TailValue`) or reject with the
    /// original reason (`TailThrow`); a rejected callback promise wins.
    fn finally_tail(
        &mut self,
        span: swc_common::Span,
        helper: Helper,
        awaited: Type,
        id: u32,
    ) -> Result<usize> {
        let ty = Type::Promise(id);
        let value = stored(self.types.promises[id as usize].value);
        let reason = self.exception_type()?;
        let carried = if helper == Helper::TailValue {
            value
        } else {
            reason
        };
        self.memoized(helper, vec![awaited, ty], |lower| {
            lower.synthesize(
                Self::plain(span, vec![awaited, carried, ty], true),
                |child, slots, _| {
                    let (source, carried_slot, q) = (slots[0], slots[1], slots[2]);
                    let reject = child.reject_helper(span, id)?;
                    let settle = if helper == Helper::TailValue {
                        child.fulfill_helper(span, id)?
                    } else {
                        reject
                    };
                    let awaited_value = child.await_promise(span, local(span, source, awaited))?;
                    let e = child.hidden(reason);
                    Ok(vec![S::TryCatch {
                        body: vec![
                            S::Expr(awaited_value),
                            invoke(
                                span,
                                settle,
                                vec![local(span, q, ty), local(span, carried_slot, carried)],
                            ),
                        ],
                        binding: Some(e),
                        handler: vec![invoke(
                            span,
                            reject,
                            vec![local(span, q, ty), local(span, e, reason)],
                        )],
                        marker: child.hidden(Type::Number),
                    }])
                },
            )
        })
    }
}
