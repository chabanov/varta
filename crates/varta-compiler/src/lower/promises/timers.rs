//! Timers (v36 `timers`, K4.4 step 5): `setTimeout`, `setInterval`,
//! `clearTimeout` and `clearInterval` over the runtime event loop.
//!
//! A timer is a scheduled frame of a synthesized function that awaits an
//! engine-internal pending `Promise<void>` no source code can reach. Starting
//! the timer creates the promise, starts the frame (it suspends at once and
//! registers its token as the promise's one reaction) and arms the timer for
//! that reaction's serial with `timer_start`, which returns the timer id. When
//! the timer fires or is cleared the runtime queues the reaction; the resumed
//! frame asks `timer_take` which one happened, calls the callback with the
//! saved arguments only when the timer fired, and completes (a timeout) or
//! awaits again (an interval). The frame keeps the callback and its arguments
//! rooted for as long as the timer is pending, and the runtime never holds a
//! guest reference.
use super::*;
use varta_common::scheduler::{TIMER_INTERVAL, TIMER_UNREF};

/// The source names of the timer globals.
const SET_TIMEOUT: &str = "setTimeout";
const SET_INTERVAL: &str = "setInterval";
const CLEAR_TIMEOUT: &str = "clearTimeout";
const CLEAR_INTERVAL: &str = "clearInterval";
/// The engine-prelude intrinsic for an unreferenced timeout.
const UNREF_TIMEOUT: &str = "varta.web.unrefTimeout";
const TIMER_DETAIL: &str = "outside the timer subset of checked-f64-utf16-v36";

/// `ReturnType<typeof setTimeout>` or `ReturnType<typeof setInterval>` (v37
/// `language_core`): the timer handle, a `number` as these timers return.
/// The query names the built-in timers; a type named `ReturnType` wins.
pub(in crate::lower) fn handle_type(
    reference: &ast::TsTypeRef,
    types: &ir::ValueTypes,
) -> Option<Type> {
    if !types.profile.language_core()
        || !matches!(&reference.type_name, ast::TsEntityName::Ident(id) if id.sym == "ReturnType")
        || named::declared(types, "ReturnType")
    {
        return None;
    }
    let [argument] = reference.type_params.as_ref()?.params.as_slice() else {
        return None;
    };
    let ast::TsType::TsTypeQuery(query) = &**argument else {
        return None;
    };
    match &query.expr_name {
        ast::TsTypeQueryExpr::TsEntityName(ast::TsEntityName::Ident(id))
            if query.type_args.is_none()
                && matches!(id.sym.as_ref(), SET_TIMEOUT | SET_INTERVAL) =>
        {
            Some(types.number())
        }
        _ => None,
    }
}

fn timer_rejected(what: &str) -> CompilerError {
    CompilerError::Unsupported {
        kind: what.into(),
        detail: TIMER_DETAIL.into(),
    }
}

/// How a timer frame runs its callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum TimerKind {
    /// Once, unless cleared first (`setTimeout`).
    Timeout,
    /// After every firing until cleared (`setInterval`).
    Interval,
}

impl Lower<'_> {
    /// Whether `name` is an unshadowed timer global of a timers profile.
    pub(in crate::lower) fn timer_builtin(&self, name: &str) -> bool {
        self.types.profile.timers()
            && matches!(
                name,
                SET_TIMEOUT | SET_INTERVAL | CLEAR_TIMEOUT | CLEAR_INTERVAL
            )
            && self.binding(name).is_err()
            && !self.outer.contains_key(name)
            && !self.functions.contains_key(name)
    }
    /// `setTimeout(...)`, `setInterval(...)`, `clearTimeout(id)` or
    /// `clearInterval(id)`.
    pub(in crate::lower) fn timer_call(
        &mut self,
        call: &ast::CallExpr,
        name: &str,
    ) -> Result<Expr> {
        if call.type_args.is_some() {
            return Err(self
                .source
                .locate(timer_rejected(&format!("{name} type arguments")), call.span));
        }
        if let Some(arg) = call.args.iter().find(|arg| arg.spread.is_some()) {
            return Err(self.source.locate(
                timer_rejected(&format!("spread {name} arguments")),
                arg.expr.span(),
            ));
        }
        match name {
            SET_TIMEOUT => self.set_timer(call, TimerKind::Timeout, 0, name),
            SET_INTERVAL => self.set_timer(call, TimerKind::Interval, TIMER_INTERVAL, name),
            _ => self.clear_timer(call, name),
        }
    }
    /// Whether `callee` is a static member that an engine prelude implements
    /// as a function (`AbortSignal.timeout`), bound under its dotted name.
    pub(in crate::lower) fn prelude_member(&self, callee: &ast::Expr) -> bool {
        let Some(name) = super::expr::callee_name(stmt::unparen(callee)) else {
            return false;
        };
        name.split_once('.').is_some_and(|(object, _)| {
            self.binding(object).is_err() && !self.outer.contains_key(object)
        }) && self.functions.contains_key(&name)
    }
    /// `varta.web.unrefTimeout(callback, ms)`, the engine-prelude intrinsic
    /// behind `AbortSignal.timeout`: a timeout that does not keep the event
    /// loop alive (Node's unreferenced timer). Program modules cannot call it.
    pub(in crate::lower) fn prelude_timer(
        &mut self,
        call: &ast::CallExpr,
        name: &str,
    ) -> Result<Option<Expr>> {
        if name != UNREF_TIMEOUT
            || !self.types.profile.timers()
            || !crate::modules::engine::is_prelude_source(self.source)
        {
            return Ok(None);
        }
        self.set_timer(call, TimerKind::Timeout, TIMER_UNREF, SET_TIMEOUT)
            .map(Some)
    }
    fn set_timer(
        &mut self,
        call: &ast::CallExpr,
        kind: TimerKind,
        flags: i32,
        name: &str,
    ) -> Result<Expr> {
        let span = call.span;
        let Some((callback, rest)) = call.args.split_first() else {
            return Err(self.source.locate(
                type_error(format!("{name} requires a callback function")),
                span,
            ));
        };
        // Arguments run in source order: callback, delay, then the extra
        // arguments the callback receives.
        let (delay, extra) = match rest.split_first() {
            Some((delay, extra)) => (Some(delay), extra),
            None => (None, rest),
        };
        let mut arg_types = Vec::with_capacity(extra.len());
        // The callback is typed by the arguments it receives, so lower those
        // first into slots and evaluate them in order inside the value block.
        let mut lowered_extra = Vec::with_capacity(extra.len());
        for arg in extra {
            // Literal types erase to their base primitive (as in promises).
            let value = self.expr(&arg.expr)?;
            let erased = value.ty.erased();
            let value = unions::widen(value, Some(erased), self.types)?;
            let ty = value.ty;
            if ty == Type::Void || ty.width() != 1 {
                return Err(self.source.locate(
                    timer_rejected(&format!("{name} argument of type {ty}")),
                    value.span,
                ));
            }
            arg_types.push(ty);
            lowered_extra.push(value);
        }
        let callback = self.timer_callback(&callback.expr, &arg_types, name)?;
        let delay = match delay {
            None => float(span, 0.0),
            Some(delay) => {
                let value = self.expr(&delay.expr)?;
                let erased = value.ty.erased();
                let value = unions::widen(value, Some(erased), self.types)?;
                match value.ty {
                    Type::Float64 => value,
                    // An undefined delay is 0 after it is evaluated.
                    Type::Undefined => {
                        let output = self.hidden(Type::Float64);
                        Expr {
                            span: value.span,
                            ty: Type::Float64,
                            kind: E::ValueBlock {
                                output,
                                marker: self.hidden(Type::Number),
                                roots: vec![],
                                body: vec![S::Expr(value), S::Init(output, float(span, 0.0))],
                            },
                        }
                    }
                    other => {
                        return Err(self.source.locate(
                            type_error(format!("{name} delay must be a number, found {other}")),
                            value.span,
                        ));
                    }
                }
            }
        };
        let promise = intern(Type::Void, self.types).map_err(|e| self.source.locate(e, span))?;
        let Type::Promise(promise_id) = promise else {
            return Err(CompilerError::Internal("missing timer promise".into()));
        };
        let callback_ty = callback.ty;
        let frame = self.timer_frame(span, kind, promise, callback_ty, &arg_types)?;
        let callback_slot = self.hidden(callback_ty);
        let delay_slot = self.hidden(Type::Float64);
        let mut body = vec![S::Init(callback_slot, callback), S::Init(delay_slot, delay)];
        let mut roots = vec![callback_slot];
        let mut frame_args = Vec::with_capacity(arg_types.len() + 2);
        let promise_slot = self.hidden(promise);
        frame_args.push(local(span, promise_slot, promise));
        frame_args.push(local(span, callback_slot, callback_ty));
        for value in lowered_extra {
            let ty = value.ty;
            let slot = self.hidden(ty);
            body.push(S::Init(slot, value));
            if ty.needs_root() {
                roots.push(slot);
            }
            frame_args.push(local(span, slot, ty));
        }
        body.push(S::Init(
            promise_slot,
            self.promise_object(span, promise_id, None)?,
        ));
        roots.push(promise_slot);
        body.push(schedule(span, frame, frame_args, ScheduleMode::Start));
        let output = self.hidden(Type::Float64);
        body.push(S::Init(
            output,
            Expr {
                span,
                ty: Type::Float64,
                kind: E::Host(
                    self.types.profile.host_operation(Host::TimerStart),
                    vec![
                        self.read_promise(local(span, promise_slot, promise), SERIAL)?,
                        local(span, delay_slot, Type::Float64),
                        Expr {
                            span,
                            ty: Type::Number,
                            kind: E::Integer(flags),
                        },
                    ],
                ),
            },
        ));
        // The callback runs in a later task and may write captured bindings.
        self.flow_reentry();
        Ok(Expr {
            span,
            ty: Type::Float64,
            kind: E::ValueBlock {
                output,
                marker: self.hidden(Type::Number),
                roots,
                body,
            },
        })
    }
    /// The callback: a contextually typed arrow or function expression, or a
    /// typed function value. Its leading parameters must accept the extra
    /// arguments, and parameters beyond them must be optional, so a callback
    /// is never called with fewer arguments than it requires.
    fn timer_callback(&mut self, expr: &ast::Expr, args: &[Type], name: &str) -> Result<Expr> {
        let value = match expr {
            ast::Expr::Paren(paren) => return self.timer_callback(&paren.expr, args, name),
            ast::Expr::Arrow(arrow) if arrow.is_async && !self.types.profile.async_methods() => {
                return Err(self
                    .source
                    .locate(rejected("async arrow function"), arrow.span));
            }
            ast::Expr::Arrow(arrow) => self.arrow_context(
                arrow,
                LiftContext {
                    params: Some(args.to_vec()),
                    shape: None,
                    result: None,
                    infer: true,
                    receiver: None,
                    lexical: true,
                    self_name: None,
                },
            ),
            ast::Expr::Fn(function) => {
                self.function_expression_context(function, Some(args.to_vec()), None, None, None)
            }
            _ => self.expr(expr),
        }
        .map_err(|error| self.source.locate(error, expr.span()))?;
        let Type::Function(id) = value.ty else {
            return Err(self.source.locate(
                type_error(format!(
                    "{name} requires a typed callback function, found {}",
                    value.ty
                )),
                value.span,
            ));
        };
        let signature = &self.types.functions[id as usize];
        // Arguments beyond the fixed parameters go to a rest parameter, whose
        // element types the frame's call checks, or are ignored.
        let fixed = signature.shape.fixed_count();
        let accepted = signature.receiver.is_none()
            && signature
                .params
                .iter()
                .take(fixed)
                .zip(args)
                .all(|(expected, actual)| arguments::accepts(*expected, *actual, self.types))
            && (args.len()..fixed).all(|index| signature.shape.optional[index]);
        if !accepted {
            return Err(self.source.locate(
                type_error(format!(
                    "{name} callback parameters do not accept the arguments it is called with"
                )),
                value.span,
            ));
        }
        Ok(value)
    }
    /// The timer frame for one callback type and argument list:
    /// `(promise, callback, ...args)`.
    fn timer_frame(
        &mut self,
        span: swc_common::Span,
        kind: TimerKind,
        promise: Type,
        callback: Type,
        args: &[Type],
    ) -> Result<usize> {
        let mut params = vec![promise, callback];
        params.extend_from_slice(args);
        let mut key = vec![callback];
        key.extend_from_slice(args);
        let arity = match callback {
            Type::Function(id) => {
                let signature = &self.types.functions[id as usize];
                if signature.shape.rest {
                    args.len()
                } else {
                    signature.params.len().min(args.len())
                }
            }
            _ => return Err(CompilerError::Internal("timer callback type".into())),
        };
        self.memoized(Helper::Timer(kind), key, |lower| {
            lower.synthesize(
                Self::plain(span, params.clone(), true),
                |child, slots, _| {
                    let (p, f) = (slots[0], slots[1]);
                    let reason = child.exception_type()?;
                    let passed = slots[2..2 + arity]
                        .iter()
                        .zip(&params[2..])
                        .map(|(slot, ty)| local(span, *slot, *ty))
                        .collect();
                    let invocation =
                        child.invoke_receiver(local(span, f, callback), None, passed, span)?;
                    let e = child.hidden(reason);
                    let guarded = S::TryCatch {
                        body: vec![S::Expr(invocation)],
                        binding: Some(e),
                        handler: vec![host(
                            span,
                            Host::TimerUncaught,
                            vec![local(span, e, reason)],
                        )],
                        marker: child.hidden(Type::Number),
                    };
                    let take = Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Host(
                            Host::TimerTake,
                            vec![child.read_promise(local(span, p, promise), SERIAL)?],
                        ),
                    };
                    let first = child.await_promise(span, local(span, p, promise))?;
                    Ok(match kind {
                        TimerKind::Timeout => {
                            vec![S::Expr(first), S::If(take, vec![guarded], vec![])]
                        }
                        TimerKind::Interval => {
                            let again = child.await_promise(span, local(span, p, promise))?;
                            vec![
                                S::Expr(first),
                                S::While(take, vec![guarded, S::Expr(again)]),
                            ]
                        }
                    })
                },
            )
        })
    }
    /// `clearTimeout(id)` / `clearInterval(id)`: the id is converted with
    /// `Number`, so `undefined`, `null` and ids that name no armed timer are
    /// no-ops, as in JavaScript.
    fn clear_timer(&mut self, call: &ast::CallExpr, name: &str) -> Result<Expr> {
        let span = call.span;
        let id = match call.args.as_slice() {
            [] => float(span, 0.0),
            [id] => {
                let value = self.expr(&id.expr)?;
                if value.ty == Type::Float64 {
                    value
                } else {
                    self.primitive_convert(value, &[], true, span)
                        .map_err(|error| {
                            self.source.locate(
                                type_error(format!("{name} requires a timer id number: {error}")),
                                span,
                            )
                        })?
                }
            }
            _ => {
                return Err(self.source.locate(
                    timer_rejected(&format!("{name} with more than one argument")),
                    span,
                ));
            }
        };
        Ok(Expr {
            span,
            ty: Type::Void,
            kind: E::Host(
                self.types.profile.host_operation(Host::TimerClear),
                vec![id],
            ),
        })
    }
}
