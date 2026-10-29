//! Promises (v34 `promises`, K4.4 step 3) and async functions (v34
//! `async_functions`, K4.4 step 4) over scheduled continuation frames.
//!
//! A `Promise<T>` object is a guest-heap record with private fields and no
//! value tag. Every promise job is a scheduled frame: `await` suspends the
//! frame and registers its resume token as a reaction of the awaited promise
//! (or queues it when the promise has already settled); the runtime scheduler
//! resumes it through `varta_resume` and the frame then reads the settled
//! state. `then`, `catch` and `finally` reactions, promise adoption and async
//! function bodies are frames of compiler-synthesized functions, so their job
//! order is the ECMAScript order: one job per reaction, two extra jobs for the
//! adoption of a promise (NewPromiseResolveThenableJob, then its reaction).
use super::*;
use functions::{LiftContext, intern_full};
mod async_functions;
mod methods;
pub(super) use async_functions::AsyncSplit;
use ir::{PromiseType, ScheduleMode};
use std::collections::BTreeSet;
use varta_common::scheduler::PromiseState;
use varta_common::value::ErrorClass;

/// Private fields of a promise record; emission addresses the scheduler's
/// fields by these names.
pub(crate) mod fields {
    pub(crate) const HANDLED: &str = "handled";
    pub(crate) const LOCKED: &str = "locked";
    pub(crate) const REASON: &str = "reason";
    pub(crate) const SERIAL: &str = "serial";
    pub(crate) const STATE: &str = "state";
    pub(crate) const TRACKED: &str = "tracked";
    pub(crate) const VALUE: &str = "value";
}
use fields::*;
/// V8's message for resolving a promise with itself.
const CYCLE_MESSAGE: &str = "Chaining cycle detected for promise #<Promise>";
/// Distinct `Promise<T>` types per module; each one owns its helpers and jobs.
pub(crate) const MAX_PROMISE_TYPES: usize = 64;
const DETAIL: &str = "outside the promise and async subset of checked-f64-utf16-v34";

/// Synthesized promise functions, memoized by kind and the types they serve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Helper {
    Fulfill,
    Reject,
    Resolve,
    Adopt,
    ResolveFunction,
    RejectFunction,
    Then,
    Finally,
    TailValue,
    TailThrow,
}

/// Promise lowering state shared by every function of one program.
#[derive(Default)]
pub(crate) struct Asynchrony {
    /// Functions whose frames the runtime scheduler resumes: async function
    /// bodies and promise jobs.
    pub scheduled: BTreeSet<usize>,
    helpers: BTreeMap<(Helper, Vec<Type>), usize>,
}

pub(super) fn rejected(what: &str) -> CompilerError {
    CompilerError::Unsupported {
        kind: what.into(),
        detail: DETAIL.into(),
    }
}
/// Promise objects have no value tag: they stay in locals, parameters,
/// results, closure cells and frames, never inside a value ABI aggregate.
pub(super) fn opaque(container: &str) -> CompilerError {
    rejected(&format!("promise inside {container}"))
}
/// The type a promise stores and an `await` produces: `undefined` for `void`.
pub(super) fn stored(value: Type) -> Type {
    if value == Type::Void {
        Type::Undefined
    } else {
        value
    }
}

/// `Promise<T>`.
pub(super) fn annotation(reference: &ast::TsTypeRef, types: &mut ir::ValueTypes) -> Result<Type> {
    let params = reference
        .type_params
        .as_ref()
        .map(|p| p.params.as_slice())
        .unwrap_or_default();
    let [value] = params else {
        return Err(rejected("Promise type argument count"));
    };
    let value = keyword_type(value, types)?;
    intern(value, types)
}

/// Intern `Promise<value>` and its private record shape.
pub(super) fn intern(value: Type, types: &mut ir::ValueTypes) -> Result<Type> {
    // Literal types are compile-time sets of their base primitive.
    let value = value.erased();
    if matches!(value, Type::Promise(_)) {
        return Err(rejected(
            "Promise<Promise<T>>; a promise never fulfills with a promise",
        ));
    }
    if matches!(value, Type::Generator(_)) {
        return Err(generators::opaque("a promise"));
    }
    if value.width() > 1
        || matches!(value, Type::Function(_))
        || value != Type::Void && value.value_kind().is_none()
    {
        return Err(rejected(&format!("Promise value type {value}")));
    }
    if let Some(id) = types.promises.iter().position(|p| p.value == value) {
        return Ok(Type::Promise(id as u32));
    }
    if types.promises.len() >= MAX_PROMISE_TYPES {
        return Err(invalid("promise type count exceeds limit"));
    }
    let reason = exceptions::exception_union(types)?;
    let field = |name: &str, ty: Type| ir::RecordField {
        name: name.into(),
        ty,
        optional: false,
    };
    let Type::Record(shape) = records::intern(
        vec![
            field(HANDLED, Type::Boolean),
            field(LOCKED, Type::Boolean),
            field(REASON, reason),
            field(SERIAL, Type::Float64),
            field(STATE, Type::Float64),
            field(TRACKED, Type::Float64),
            field(VALUE, stored(value)),
        ],
        types,
    )?
    else {
        return Err(CompilerError::Internal(
            "promise record is not a record".into(),
        ));
    };
    let id = types.promises.len() as u32;
    types.promises.push(PromiseType { value, shape });
    Ok(Type::Promise(id))
}

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn float(span: swc_common::Span, value: f64) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Float64(value),
    }
}
fn state(span: swc_common::Span, state: PromiseState) -> Expr {
    float(span, f64::from(state as i32))
}
fn boolean(span: swc_common::Span, value: bool) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Integer(i32::from(value)),
    }
}
fn zero(span: swc_common::Span, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Zero,
    }
}
fn undefined(span: swc_common::Span) -> Expr {
    Expr {
        span,
        ty: Type::Undefined,
        kind: E::Integer(0),
    }
}
fn compare(span: swc_common::Span, op: ast::BinaryOp, a: Expr, b: Expr) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Binary(op, Box::new(a), Box::new(b)),
    }
}
fn not(value: Expr) -> Expr {
    Expr {
        span: value.span,
        ty: Type::Boolean,
        kind: E::Not(Box::new(value)),
    }
}
fn invoke(span: swc_common::Span, index: usize, args: Vec<Expr>) -> S {
    S::Expr(Expr {
        span,
        ty: Type::Void,
        kind: E::Call(index, args),
    })
}
fn host(span: swc_common::Span, operation: Host, args: Vec<Expr>) -> S {
    S::Expr(Expr {
        span,
        ty: Type::Void,
        kind: E::Host(operation, args),
    })
}
fn schedule(span: swc_common::Span, function: usize, args: Vec<Expr>, mode: ScheduleMode) -> S {
    S::Expr(Expr {
        span,
        ty: Type::Void,
        kind: E::Schedule {
            function,
            args,
            mode,
        },
    })
}

/// A synthesized function: a promise helper, a promise job frame or a
/// resolving closure over one promise binding.
struct Synthetic {
    span: swc_common::Span,
    params: Vec<Type>,
    shape: ir::ParameterShape,
    capture: Option<Binding>,
    scheduled: bool,
}

/// What a promise handler returns: nothing, a value, or a promise to adopt.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Void,
    Value(Type),
    Promise(u32),
}

impl Lower<'_> {
    fn promise_info(&self, ty: Type) -> Result<(u32, PromiseType)> {
        match ty {
            Type::Promise(id) => Ok((id, self.types.promises[id as usize])),
            _ => Err(CompilerError::Internal(format!("{ty} is not a promise"))),
        }
    }
    /// The private record of `promise`, its field slot and the field's type.
    fn promise_field(&self, promise: Type, name: &str) -> Result<(Type, u32, Type)> {
        let (_, info) = self.promise_info(promise)?;
        let fields = &self.types.records[info.shape as usize].fields;
        let slot = fields
            .iter()
            .position(|field| field.name == name)
            .ok_or_else(|| CompilerError::Internal("missing promise field".into()))?;
        Ok((Type::Record(info.shape), slot as u32, fields[slot].ty))
    }
    fn view(&self, promise: Expr) -> Result<Expr> {
        let (_, info) = self.promise_info(promise.ty)?;
        Ok(Expr {
            span: promise.span,
            ty: Type::Record(info.shape),
            kind: E::Reinterpret(Box::new(promise)),
        })
    }
    fn read_promise(&self, promise: Expr, name: &str) -> Result<Expr> {
        let (_, slot, ty) = self.promise_field(promise.ty, name)?;
        let span = promise.span;
        Ok(Expr {
            span,
            ty,
            kind: E::Field(Box::new(self.view(promise)?), slot),
        })
    }
    fn write_promise(&self, promise: Expr, name: &str, value: Expr) -> Result<S> {
        let (_, slot, _) = self.promise_field(promise.ty, name)?;
        Ok(S::SetField(self.view(promise)?, slot, value))
    }
    /// A new pending promise, or one already fulfilled with `fulfilled`.
    fn promise_object(
        &mut self,
        span: swc_common::Span,
        id: u32,
        fulfilled: Option<Expr>,
    ) -> Result<Expr> {
        let info = self.types.promises[id as usize];
        let shape = self.types.records[info.shape as usize].clone();
        let initial = if fulfilled.is_some() {
            PromiseState::Fulfilled
        } else {
            PromiseState::Pending
        };
        let mut fulfilled = fulfilled;
        let mut fields = Vec::with_capacity(shape.fields.len());
        for (slot, field) in shape.fields.iter().enumerate() {
            let value = match field.name.as_str() {
                HANDLED | LOCKED => boolean(span, false),
                SERIAL | TRACKED => float(span, 0.0),
                STATE => state(span, initial),
                VALUE => match fulfilled.take() {
                    Some(value) => unions::widen(value, Some(field.ty), self.types)?,
                    None => zero(span, field.ty),
                },
                _ => zero(span, field.ty),
            };
            fields.push((slot as u32, value));
        }
        let record = Expr {
            span,
            ty: Type::Record(info.shape),
            kind: E::Record(fields),
        };
        Ok(Expr {
            span,
            ty: Type::Promise(id),
            kind: E::Reinterpret(Box::new(record)),
        })
    }
    fn memoized(
        &mut self,
        helper: Helper,
        key: Vec<Type>,
        build: impl FnOnce(&mut Self) -> Result<usize>,
    ) -> Result<usize> {
        if let Some(&index) = self.types.asynchrony.helpers.get(&(helper, key.clone())) {
            return Ok(index);
        }
        let index = build(self)?;
        self.types.asynchrony.helpers.insert((helper, key), index);
        Ok(index)
    }
    /// Lower a synthesized function with its own locals and insert it into
    /// the program; returns its function index.
    fn synthesize(
        &mut self,
        spec: Synthetic,
        build: impl FnOnce(&mut Lower<'_>, &[u32], Option<u32>) -> Result<Vec<S>>,
    ) -> Result<usize> {
        let index = *self.next_function;
        if index >= varta_common::abi::MAX_USER_FUNCTIONS {
            return Err(invalid("too many functions including promise jobs"));
        }
        *self.next_function += 1;
        let mut child = Lower {
            source: self.source,
            types: self.types,
            functions: self.functions,
            scopes: vec![BTreeMap::new()],
            next: 0,
            local_types: BTreeMap::new(),
            refinements: BTreeMap::new(),
            writes: Default::default(),
            call_writes: Default::default(),
            call_unknown: false,
            direct_writes: Default::default(),
            call_parameters: Default::default(),
            parameter_values: Default::default(),
            parameter_roots: Default::default(),
            known_calls: Default::default(),
            call_effects: self.call_effects,
            reachable: true,
            control: Default::default(),
            guard_blocked: Default::default(),
            outer_narrows: Default::default(),
            result: Type::Void,
            infer_returns: false,
            entry: self.entry,
            loops: 0,
            labels: Vec::new(),
            next_label: 0,
            switches: 0,
            depth: self.depth,
            repeat_slots: None,
            generated: self.generated,
            next_function: self.next_function,
            index,
            outer: Default::default(),
            captures: Default::default(),
            cells: Default::default(),
            guarded: Default::default(),
            generator: None,
            asynchronous: None,
        };
        let params: Vec<u32> = spec.params.iter().map(|ty| child.hidden(*ty)).collect();
        let captured = spec
            .capture
            .as_ref()
            .map(|binding| child.capture_binding(binding))
            .transpose()?;
        let body = build(&mut child, &params, captured)?;
        let function = ir::Function {
            shape: spec.shape,
            self_slot: None,
            receiver: None,
            cells: std::mem::take(&mut child.cells),
            captures: captured.into_iter().collect(),
            environment: spec.capture.is_some(),
            generator: None,
            source: child.source.clone(),
            span: spec.span,
            params: spec.params,
            result: Type::Void,
            locals: (0..child.next)
                .map(|slot| {
                    child
                        .local_types
                        .get(&slot)
                        .copied()
                        .unwrap_or(Type::Number)
                })
                .collect(),
            body,
        };
        child.generated.insert(index, function);
        if spec.scheduled {
            child.types.asynchrony.scheduled.insert(index);
        }
        Ok(index)
    }
    fn plain(span: swc_common::Span, params: Vec<Type>, scheduled: bool) -> Synthetic {
        Synthetic {
            span,
            shape: ir::ParameterShape::fixed(params.len()),
            params,
            capture: None,
            scheduled,
        }
    }
    /// `if (p.state === pending) { settle; trigger reactions; track }`.
    fn settle_body(
        &mut self,
        span: swc_common::Span,
        promise: impl Fn() -> Expr,
        outcome: PromiseState,
        value: Expr,
    ) -> Result<Vec<S>> {
        let field = if outcome == PromiseState::Fulfilled {
            VALUE
        } else {
            REASON
        };
        let mut settled = vec![
            self.write_promise(promise(), STATE, state(span, outcome))?,
            self.write_promise(promise(), field, value)?,
            S::If(
                compare(
                    span,
                    ast::BinaryOp::NotEqEq,
                    self.read_promise(promise(), SERIAL)?,
                    float(span, 0.0),
                ),
                vec![host(
                    span,
                    Host::PromiseSettle,
                    vec![self.read_promise(promise(), SERIAL)?],
                )],
                vec![],
            ),
        ];
        if outcome == PromiseState::Rejected {
            settled.push(S::If(
                not(self.read_promise(promise(), HANDLED)?),
                vec![self.write_promise(
                    promise(),
                    TRACKED,
                    Expr {
                        span,
                        ty: Type::Float64,
                        kind: E::Host(
                            Host::PromiseUnhandled,
                            vec![self.read_promise(promise(), REASON)?],
                        ),
                    },
                )?],
                vec![],
            ));
        }
        Ok(vec![S::If(
            compare(
                span,
                ast::BinaryOp::EqEqEq,
                self.read_promise(promise(), STATE)?,
                state(span, PromiseState::Pending),
            ),
            settled,
            vec![],
        )])
    }
    /// FulfillPromise for `Promise<T>`: `(promise, value) -> void`.
    fn fulfill_helper(&mut self, span: swc_common::Span, id: u32) -> Result<usize> {
        let ty = Type::Promise(id);
        let value = stored(self.types.promises[id as usize].value);
        self.memoized(Helper::Fulfill, vec![ty], |lower| {
            lower.synthesize(
                Self::plain(span, vec![ty, value], false),
                |child, params, _| {
                    let (p, v) = (params[0], params[1]);
                    child.settle_body(
                        span,
                        || local(span, p, ty),
                        PromiseState::Fulfilled,
                        local(span, v, value),
                    )
                },
            )
        })
    }
    /// RejectPromise with rejection tracking: `(promise, reason) -> void`.
    fn reject_helper(&mut self, span: swc_common::Span, id: u32) -> Result<usize> {
        let ty = Type::Promise(id);
        let reason = self.exception_type()?;
        self.memoized(Helper::Reject, vec![ty], |lower| {
            lower.synthesize(
                Self::plain(span, vec![ty, reason], false),
                |child, params, _| {
                    let (p, e) = (params[0], params[1]);
                    child.settle_body(
                        span,
                        || local(span, p, ty),
                        PromiseState::Rejected,
                        local(span, e, reason),
                    )
                },
            )
        })
    }
    /// NewPromiseResolveThenableJob for an internal promise: a scheduled frame
    /// `(target, source)` that awaits `source` and settles `target` alike.
    fn adopt_helper(&mut self, span: swc_common::Span, id: u32) -> Result<usize> {
        let ty = Type::Promise(id);
        self.memoized(Helper::Adopt, vec![ty], |lower| {
            lower.synthesize(Self::plain(span, vec![ty, ty], true), |child, params, _| {
                let (target, source) = (params[0], params[1]);
                let value = stored(child.types.promises[id as usize].value);
                let reason = child.exception_type()?;
                let fulfill = child.fulfill_helper(span, id)?;
                let reject = child.reject_helper(span, id)?;
                let v = child.hidden(value);
                let e = child.hidden(reason);
                let marker = child.hidden(Type::Number);
                let awaited = child.await_promise(span, local(span, source, ty))?;
                Ok(vec![
                    S::Init(v, zero(span, value)),
                    S::TryCatch {
                        body: vec![
                            S::Set(v, awaited),
                            invoke(
                                span,
                                fulfill,
                                vec![local(span, target, ty), local(span, v, value)],
                            ),
                        ],
                        binding: Some(e),
                        handler: vec![invoke(
                            span,
                            reject,
                            vec![local(span, target, ty), local(span, e, reason)],
                        )],
                        marker,
                    },
                ])
            })
        })
    }
    /// Resolve `target` with the promise `source`: a self-resolution is a
    /// `TypeError` rejection; otherwise adoption is queued as a job.
    fn resolve_helper(&mut self, span: swc_common::Span, id: u32) -> Result<usize> {
        let ty = Type::Promise(id);
        self.memoized(Helper::Resolve, vec![ty], |lower| {
            lower.synthesize(
                Self::plain(span, vec![ty, ty], false),
                |child, params, _| {
                    let (target, source) = (params[0], params[1]);
                    let reject = child.reject_helper(span, id)?;
                    let adopt = child.adopt_helper(span, id)?;
                    let error = child.array_error(ErrorClass::TypeError, CYCLE_MESSAGE, span)?;
                    Ok(vec![S::If(
                        compare(
                            span,
                            ast::BinaryOp::EqEqEq,
                            child.view(local(span, target, ty))?,
                            child.view(local(span, source, ty))?,
                        ),
                        vec![invoke(span, reject, vec![local(span, target, ty), error])],
                        vec![schedule(
                            span,
                            adopt,
                            vec![local(span, target, ty), local(span, source, ty)],
                            ScheduleMode::Enqueue,
                        )],
                    )])
                },
            )
        })
    }
    /// `await promise` inside a scheduled frame: the result reads the settled
    /// promise, throwing its reason when it was rejected.
    fn await_promise(&mut self, span: swc_common::Span, promise: Expr) -> Result<Expr> {
        let ty = promise.ty;
        let (_, info) = self.promise_info(ty)?;
        let value = stored(info.value);
        let slot = self.hidden(ty);
        let output = self.hidden(value);
        let marker = self.hidden(Type::Number);
        let body = vec![
            S::If(
                compare(
                    span,
                    ast::BinaryOp::EqEqEq,
                    self.read_promise(local(span, slot, ty), STATE)?,
                    state(span, PromiseState::Rejected),
                ),
                vec![S::Throw(self.read_promise(local(span, slot, ty), REASON)?)],
                vec![],
            ),
            S::Init(output, self.read_promise(local(span, slot, ty), VALUE)?),
        ];
        let result = Expr {
            span,
            ty: value,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![output],
                body,
            },
        };
        Ok(Expr {
            span,
            ty: value,
            kind: E::Await {
                promise: Box::new(promise),
                slot,
                result: Box::new(result),
            },
        })
    }
    /// Settle `target` (a `Promise<U>`) with a handler result: a value
    /// fulfills, a promise is adopted, a void result fulfills with undefined.
    fn settle_with(
        &mut self,
        span: swc_common::Span,
        target: impl Fn() -> Expr,
        value: Expr,
    ) -> Result<Vec<S>> {
        let (id, info) = self.promise_info(target().ty)?;
        let fulfill = self.fulfill_helper(span, id)?;
        Ok(match value.ty {
            Type::Void => vec![
                S::Expr(value),
                invoke(
                    span,
                    fulfill,
                    vec![
                        target(),
                        unions::widen(undefined(span), Some(stored(info.value)), self.types)?,
                    ],
                ),
            ],
            Type::Promise(source) if source == id => {
                let resolve = self.resolve_helper(span, id)?;
                vec![invoke(span, resolve, vec![target(), value])]
            }
            Type::Promise(_) => {
                return Err(self.source.locate(
                    type_error(
                        "promise handlers that return promises must agree on the result type",
                    ),
                    value.span,
                ));
            }
            _ => {
                let value = unions::widen(value, Some(stored(info.value)), self.types)?;
                if value.ty != stored(info.value) {
                    return Err(self.source.locate(
                        type_error(format!(
                            "promise handler result mismatch: expected {}, found {}",
                            stored(info.value),
                            value.ty
                        )),
                        value.span,
                    ));
                }
                vec![invoke(span, fulfill, vec![target(), value])]
            }
        })
    }
    /// Call a checked handler value with the leading arguments it declares.
    fn invoke_handler(
        &mut self,
        span: swc_common::Span,
        handler: Expr,
        args: Vec<Expr>,
    ) -> Result<Expr> {
        let Type::Function(id) = handler.ty else {
            return Err(CompilerError::Internal(
                "promise handler is not a function".into(),
            ));
        };
        let signature = self.types.functions[id as usize].clone();
        let args = args
            .into_iter()
            .zip(&signature.params)
            .map(|(value, ty)| unions::widen(value, Some(*ty), self.types))
            .collect::<Result<Vec<_>>>()?;
        self.invoke_bound_receiver(handler, None, args, span)
    }
    /// Values that may become pending in a job may be observed only through
    /// later reads: an `await` or a callback that runs user code acts like an
    /// unknown call for write-aware refinements.
    fn flow_reentry(&mut self) {
        if !self.types.profile.refinement_writes() {
            return;
        }
        self.call_unknown = true;
        for slot in self.mutable_cells() {
            self.flow_write(slot);
        }
    }
    /// Whether `callee` names the built-in `Promise` (not shadowed).
    pub(super) fn promise_builtin(&self, callee: &ast::Expr) -> bool {
        matches!(stmt::unparen(callee), ast::Expr::Ident(id) if id.sym == "Promise")
            && self.binding("Promise").is_err()
            && !self.outer.contains_key("Promise")
            && !self.functions.contains_key("Promise")
    }
    fn explicit_value(
        &mut self,
        args: Option<&ast::TsTypeParamInstantiation>,
        what: &str,
    ) -> Result<Option<Type>> {
        let Some(args) = args else {
            return Ok(None);
        };
        let [value] = args.params.as_slice() else {
            return Err(rejected(&format!("{what} type argument count")));
        };
        Ok(Some(keyword_type(value, self.types)?))
    }
    fn expected_value(&self, expected: Option<Type>) -> Option<Type> {
        match expected {
            Some(Type::Promise(id)) => Some(self.types.promises[id as usize].value),
            _ => None,
        }
    }
    /// A handler or executor argument: a contextually typed arrow or function
    /// expression, a function value, or `undefined`/`null` for "absent".
    fn promise_callback(
        &mut self,
        expr: &ast::Expr,
        params: &[Type],
        result: Option<Type>,
    ) -> Result<Option<Expr>> {
        let value = match expr {
            ast::Expr::Paren(paren) => return self.promise_callback(&paren.expr, params, result),
            ast::Expr::Lit(ast::Lit::Null(_)) => return Ok(None),
            ast::Expr::Ident(id) if id.sym == "undefined" && self.binding("undefined").is_err() => {
                return Ok(None);
            }
            ast::Expr::Arrow(arrow) if arrow.is_async && !self.types.profile.async_methods() => {
                return Err(self
                    .source
                    .locate(rejected("async arrow function"), arrow.span));
            }
            ast::Expr::Arrow(arrow) => self.arrow_context(
                arrow,
                LiftContext {
                    params: Some(params.to_vec()),
                    shape: None,
                    result,
                    infer: true,
                    receiver: None,
                    lexical: true,
                    self_name: None,
                },
            ),
            ast::Expr::Fn(function) => self.function_expression_context(
                function,
                Some(params.to_vec()),
                result,
                None,
                None,
            ),
            _ => self.expr(expr),
        }
        .map_err(|error| self.source.locate(error, expr.span()))?;
        let Type::Function(id) = value.ty else {
            return Err(self.source.locate(
                type_error("promise callback must be a typed function"),
                value.span,
            ));
        };
        let signature = &self.types.functions[id as usize];
        if signature.receiver.is_some()
            || signature.shape.rest
            || signature.params.len() > params.len()
            || signature
                .params
                .iter()
                .zip(params)
                .any(|(expected, actual)| !arguments::accepts(*expected, *actual, self.types))
        {
            return Err(self.source.locate(
                type_error(
                    "promise callback parameters do not accept the values the promise passes",
                ),
                value.span,
            ));
        }
        Ok(Some(value))
    }
    fn outcome(&self, result: Type) -> Outcome {
        match result {
            Type::Void => Outcome::Void,
            Type::Promise(id) => Outcome::Promise(id),
            ty => Outcome::Value(ty),
        }
    }
    /// The settled value type a handler outcome contributes, and whether it is void.
    fn contribution(&self, outcome: Outcome) -> (Type, bool) {
        match outcome {
            Outcome::Void => (Type::Undefined, true),
            Outcome::Value(ty) => (ty, false),
            Outcome::Promise(id) => {
                let value = self.types.promises[id as usize].value;
                (stored(value), value == Type::Void)
            }
        }
    }
}
