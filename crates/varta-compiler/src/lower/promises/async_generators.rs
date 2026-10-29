//! Async generators (v36 `async_generators`, K4.4 step 6): `async function*`,
//! `await` and `yield` in their bodies, the queued `next`/`return`/`throw`
//! protocol and `for await...of`.
//!
//! An async generator object is the frame of an internal body generator
//! (`GeneratorType::body`), owned by the object like a synchronous
//! generator frame and never registered in the live-frame table while it is
//! suspended. Every suspension of the body publishes the object's state
//! record `{ head, queue, state }` as the frame completion, and the body
//! type's step function keeps that cell while the body runs, so the protocol
//! helpers find the queue of pending requests (each with its promise) and the
//! ECMAScript `[[AsyncGeneratorState]]` at any time. The helpers follow
//! AsyncGeneratorEnqueue, Resume, CompleteStep, DrainQueue and AwaitReturn.
//!
//! The body is resumed only through its closed step function: synchronously
//! by a request, or, after an `await`, by a scheduled relay job frame that
//! awaits the promise and then steps the body. The body reads the settled
//! promise from its own saved local, as an async function does, and reads
//! every resumption value from the front request of its queue.
use super::*;
use ir::{AsyncGeneratorType, GeneratorType};
use varta_common::function::frame::{Completion, MAX_GENERATOR_TYPES};
use varta_common::value::ErrorClass;

const DETAIL: &str = "outside the async generator subset of checked-f64-utf16-v36";

pub(in crate::lower) fn rejected(what: &str) -> CompilerError {
    CompilerError::Unsupported {
        kind: what.into(),
        detail: DETAIL.into(),
    }
}

/// ECMAScript `[[AsyncGeneratorState]]`, stored as a Number field.
#[derive(Clone, Copy)]
#[repr(i32)]
enum State {
    SuspendedStart = 0,
    SuspendedYield = 1,
    Executing = 2,
    DrainingQueue = 3,
    Completed = 4,
}

/// Fields of the private state and request records.
mod names {
    pub(super) const HEAD_FIELD: &str = "head";
    pub(super) const QUEUE_FIELD: &str = "queue";
    pub(super) const STATE_FIELD: &str = "state";
    pub(super) const KIND_FIELD: &str = "kind";
    pub(super) const NEXT_FIELD: &str = "next";
    pub(super) const PROMISE_FIELD: &str = "promise";
    pub(super) const RETURNED_FIELD: &str = "returned";
    pub(super) const THROWN_FIELD: &str = "thrown";
    pub(super) const DONE_FIELD: &str = "done";
    pub(super) const VALUE_FIELD: &str = "value";
}
use names::*;

/// Synthesized async generator functions, memoized per generator type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Operation {
    /// `next(v)`, `return(v)` and `throw(e)`: queue the request, return its promise.
    Next,
    Return,
    Throw,
    /// AsyncGeneratorResume: mark the generator executing and step its body.
    Resume,
    /// AsyncGeneratorCompleteStep with a value or a throw.
    CompleteValue,
    CompleteThrow,
    /// AsyncGeneratorDrainQueue.
    Drain,
    /// AsyncGeneratorAwaitReturn and its job frame.
    AwaitReturn,
    AwaitReturnJob,
    /// The job frame that resumes a body once an awaited promise settled.
    Relay,
}

/// An async generator body while its declaration is lowered.
#[derive(Clone, Copy)]
pub(crate) struct AsyncBody {
    /// The async generator type the declaration returns.
    pub generator: u32,
    /// The state record parameter, right after the declared parameters.
    pub state: u32,
    /// The body's own frame handle, the input of its start point.
    pub frame: u32,
    /// The completion value (`TReturn`, `undefined` for `void`).
    pub result: u32,
    /// The label every `return` breaks to after storing `result`.
    pub exit: u32,
}

/// `AsyncGenerator<T, TReturn, TNext>` and the async iteration interfaces
/// `AsyncIterable`, `AsyncIterableIterator` and `AsyncIterator`, which in
/// this subset denote async generator objects; parameters default as for
/// `Generator`.
pub(in crate::lower) fn annotation(
    reference: &ast::TsTypeRef,
    name: &str,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let (yielded, returned, next) = generators::arguments(reference, name, types)?;
    intern(yielded, returned, next, types)
}

/// The annotation names that denote async generator objects.
pub(in crate::lower) fn named(name: &str) -> bool {
    matches!(
        name,
        "AsyncGenerator" | "AsyncIterable" | "AsyncIterableIterator" | "AsyncIterator"
    )
}

fn holds_promise(ty: Type, types: &ir::ValueTypes) -> bool {
    unions::members(ty, types)
        .iter()
        .any(|member| matches!(member, Type::Promise(_)))
}

/// Intern the async generator type and its body type, state and request
/// records and result promise.
pub(in crate::lower) fn intern(
    yielded: Type,
    returned: Type,
    next: Type,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    if let Some(id) = types.generators.iter().position(|g| {
        g.asynchronous.is_some() && (g.yielded, g.returned, g.next) == (yielded, returned, next)
    }) {
        return Ok(Type::Generator(id as u32));
    }
    if [yielded, returned, next]
        .iter()
        .any(|ty| holds_promise(*ty, types))
    {
        return Err(rejected(
            "promise in an async generator type; yield and return await their operands",
        ));
    }
    if types.generators.len() + 2 > MAX_GENERATOR_TYPES {
        return Err(invalid("generator type count exceeds limit"));
    }
    let result = generators::result_record(yielded, returned, types)?;
    let Type::Promise(promise) = super::intern(result, types)? else {
        return Err(CompilerError::Internal("missing result promise".into()));
    };
    let field = |name: &str, ty: Type| ir::RecordField {
        name: name.into(),
        ty,
        optional: false,
    };
    let exception = exceptions::exception_union(types)?;
    // `return()` from closing a `for await` carries `undefined` whatever the
    // `TReturn`, so requests and the completion value admit it.
    let closing = unions::intern(vec![stored(returned), Type::Undefined], types)?;
    let request = records::intern(
        vec![
            field(KIND_FIELD, Type::Float64),
            field(NEXT_FIELD, next),
            field(PROMISE_FIELD, Type::Promise(promise)),
            field(RETURNED_FIELD, closing),
            field(THROWN_FIELD, exception),
        ],
        types,
    )?;
    let queue = arrays::intern(request, types)?;
    let state = records::intern(
        vec![
            field(HEAD_FIELD, Type::Float64),
            field(QUEUE_FIELD, queue),
            field(STATE_FIELD, Type::Float64),
        ],
        types,
    )?;
    let body = types.generators.len() as u32;
    let body_result = generators::result_record(state, state, types)?;
    types.generators.push(GeneratorType {
        yielded: state,
        returned: state,
        next: Type::Generator(body),
        result: body_result,
        asynchronous: None,
        body: true,
    });
    let id = types.generators.len() as u32;
    types.generators.push(GeneratorType {
        yielded,
        returned,
        next,
        result,
        asynchronous: Some(AsyncGeneratorType {
            body,
            state,
            request,
            promise,
            returned: closing,
        }),
        body: false,
    });
    Ok(Type::Generator(id))
}

fn slot_of(types: &ir::ValueTypes, record: Type, name: &str) -> Result<(u32, Type)> {
    let Type::Record(id) = record else {
        return Err(CompilerError::Internal(format!("{record} is not a record")));
    };
    let fields = &types.records[id as usize].fields;
    fields
        .iter()
        .position(|field| field.name == name)
        .map(|slot| (slot as u32, fields[slot].ty))
        .ok_or_else(|| CompilerError::Internal(format!("missing record field {name}")))
}

/// Builders over the private records of one async generator type.
#[derive(Clone, Copy)]
struct Shapes {
    span: swc_common::Span,
    generator: GeneratorType,
    info: AsyncGeneratorType,
}
impl Shapes {
    fn frame(&self) -> Type {
        Type::Generator(self.info.body)
    }
    /// `TReturn | undefined`: the value of a return request and of the
    /// body's completion.
    fn returned(&self) -> Type {
        self.info.returned
    }
    /// `T | TReturn | undefined`, the `value` of an iterator result.
    fn value(&self, types: &ir::ValueTypes) -> Result<Type> {
        Ok(slot_of(types, self.generator.result, VALUE_FIELD)?.1)
    }
    fn read(&self, types: &ir::ValueTypes, record: Expr, name: &str) -> Result<Expr> {
        let (slot, ty) = slot_of(types, record.ty, name)?;
        Ok(Expr {
            span: self.span,
            ty,
            kind: E::Field(Box::new(record), slot),
        })
    }
    fn write(&self, types: &ir::ValueTypes, record: Expr, name: &str, value: Expr) -> Result<S> {
        let (slot, _) = slot_of(types, record.ty, name)?;
        Ok(S::SetField(record, slot, value))
    }
    fn state(&self, slot: u32) -> Expr {
        local(self.span, slot, self.info.state)
    }
    fn is(&self, types: &ir::ValueTypes, state: u32, value: State) -> Result<Expr> {
        Ok(compare(
            self.span,
            ast::BinaryOp::EqEqEq,
            self.read(types, self.state(state), STATE_FIELD)?,
            float(self.span, f64::from(value as i32)),
        ))
    }
    fn set(&self, types: &ir::ValueTypes, state: u32, value: State) -> Result<S> {
        self.write(
            types,
            self.state(state),
            STATE_FIELD,
            float(self.span, f64::from(value as i32)),
        )
    }
    /// Whether the request queue is empty.
    fn empty(&self, types: &ir::ValueTypes, state: u32) -> Result<Expr> {
        Ok(compare(
            self.span,
            ast::BinaryOp::GtEq,
            self.read(types, self.state(state), HEAD_FIELD)?,
            Expr {
                span: self.span,
                ty: Type::Float64,
                kind: E::Length(Box::new(self.read(
                    types,
                    self.state(state),
                    QUEUE_FIELD,
                )?)),
            },
        ))
    }
    /// The first request of a non-empty queue.
    fn front(&self, types: &ir::ValueTypes, state: u32) -> Result<Expr> {
        Ok(Expr {
            span: self.span,
            ty: self.info.request,
            kind: E::Index(
                Box::new(self.read(types, self.state(state), QUEUE_FIELD)?),
                Box::new(self.read(types, self.state(state), HEAD_FIELD)?),
            ),
        })
    }
    fn kind_is(&self, types: &ir::ValueTypes, request: u32, kind: Completion) -> Result<Expr> {
        Ok(compare(
            self.span,
            ast::BinaryOp::EqEqEq,
            self.read(
                types,
                local(self.span, request, self.info.request),
                KIND_FIELD,
            )?,
            float(self.span, f64::from(kind as i32)),
        ))
    }
    /// `{ value, done }`, created in ECMAScript CreateIteratorResultObject order.
    fn result(&self, types: &ir::ValueTypes, value: Expr, done: bool) -> Result<Expr> {
        let (value_slot, _) = slot_of(types, self.generator.result, VALUE_FIELD)?;
        let (done_slot, _) = slot_of(types, self.generator.result, DONE_FIELD)?;
        Ok(Expr {
            span: self.span,
            ty: self.generator.result,
            kind: E::Record(vec![
                (value_slot, value),
                (done_slot, boolean(self.span, done)),
            ]),
        })
    }
    /// A resumption of the body: its own frame handle is the input.
    fn step(&self, frame: u32) -> Expr {
        Expr {
            span: self.span,
            ty: Type::Number,
            kind: E::GeneratorStep {
                generator: Box::new(local(self.span, frame, self.frame())),
                completion: Completion::Normal,
                value: Box::new(local(self.span, frame, self.frame())),
            },
        }
    }
}

/// The head of a `for await` statement.
#[derive(Clone, Copy)]
struct Head<'s> {
    statement: &'s ast::ForOfStmt,
    pattern: &'s ast::Pat,
    mode: patterns::Mode,
}

/// `value` of union type narrowed to its member or sub-union `target`, which
/// the caller proved.
fn narrowed(value: Expr, target: Type) -> Expr {
    if value.ty == target {
        value
    } else if target.is_cell() {
        Expr {
            ty: target,
            ..value
        }
    } else {
        Expr {
            span: value.span,
            ty: target,
            kind: E::UnboxUnion(Box::new(value)),
        }
    }
}

fn discard(value: Expr) -> S {
    S::Expr(value)
}

/// Rewrite every `return` of a body into a store of its value and a break to
/// the body's exit, so enclosing `finally` blocks run first. Afterwards the
/// only returns inside are the state machine's `Return` deliveries, which
/// return the state record: every `finally` keeps a pending one in a fresh
/// local of that type.
struct Exits<'l, 'a> {
    lower: &'l mut Lower<'a>,
    exit: u32,
    result: u32,
    result_ty: Type,
    state: Type,
}
impl Exits<'_, '_> {
    fn list(&mut self, body: Vec<S>) -> Result<Vec<S>> {
        body.into_iter().map(|s| self.statement(s)).collect()
    }
    fn statement(&mut self, statement: S) -> Result<S> {
        Ok(match statement {
            S::Return(None) => S::BreakLabel(self.exit),
            S::Return(Some(value)) => S::Block(vec![
                S::Set(
                    self.result,
                    unions::widen(value, Some(self.result_ty), self.lower.types)?,
                ),
                S::BreakLabel(self.exit),
            ]),
            S::TryCatch {
                body,
                binding,
                handler,
                marker,
            } => S::TryCatch {
                body: self.list(body)?,
                binding,
                handler: self.list(handler)?,
                marker,
            },
            S::TryFinally {
                body,
                finalizer,
                marker,
                completion,
                exception,
                result: _,
            } => S::TryFinally {
                body: self.list(body)?,
                finalizer: self.list(finalizer)?,
                marker,
                completion,
                exception,
                result: Some(self.lower.hidden(self.state)),
            },
            S::Block(body) => S::Block(self.list(body)?),
            S::Scope(slots, body) => S::Scope(slots, self.list(body)?),
            S::Label {
                id,
                iteration,
                body,
            } => S::Label {
                id,
                iteration,
                body: self.list(body)?,
            },
            S::If(test, yes, no) => S::If(test, self.list(yes)?, self.list(no)?),
            S::While(test, body) => S::While(test, self.list(body)?),
            S::DoWhile(test, body) => S::DoWhile(test, self.list(body)?),
            S::For(init, test, update, body) => {
                S::For(self.list(init)?, test, self.list(update)?, self.list(body)?)
            }
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
                    .map(|(test, body)| Ok((test, self.list(body)?)))
                    .collect::<Result<_>>()?,
            },
            other => other,
        })
    }
}

impl Lower<'_> {
    /// The async generator body whose declaration is being lowered, if any.
    pub(in crate::lower) fn async_body(&self) -> Option<AsyncBody> {
        self.types.asynchrony.bodies.get(&self.index).copied()
    }
    fn shapes(&self, span: swc_common::Span, generator: u32) -> Result<Shapes> {
        let generator = self.types.generators[generator as usize];
        let info = generator
            .asynchronous
            .ok_or_else(|| CompilerError::Internal("not an async generator type".into()))?;
        Ok(Shapes {
            span,
            generator,
            info,
        })
    }
    /// Reserve the body's hidden locals right after its parameters and make
    /// `await`, `yield` and `return` lower for an async generator body.
    pub(in crate::lower) fn begin_async_generator(&mut self, generator: u32) -> Result<AsyncBody> {
        let shapes = self.shapes(swc_common::DUMMY_SP, generator)?;
        let state = self.hidden(shapes.info.state);
        let frame = self.hidden(shapes.frame());
        let result = self.hidden(shapes.returned());
        let exit = self.next_label;
        self.next_label += 1;
        let body = AsyncBody {
            generator,
            state,
            frame,
            result,
            exit,
        };
        self.types.asynchrony.bodies.insert(self.index, body);
        Ok(body)
    }
    /// `await` in an async generator body: the relay job frame, started
    /// with the body's frame and the promise right before the suspension,
    /// registers the reaction on the promise and steps the body once it
    /// settled (`Expression::Await::relay`).
    pub(super) fn relay(
        &mut self,
        span: swc_common::Span,
        promise: Type,
    ) -> Result<Option<(usize, u32)>> {
        let Some(body) = self.async_body() else {
            return Ok(None);
        };
        let Type::Promise(id) = promise else {
            return Err(CompilerError::Internal("await of a non-promise".into()));
        };
        let shapes = self.shapes(span, body.generator)?;
        let relay = self.relay_helper(span, shapes.info.body, id)?;
        Ok(Some((relay, body.frame)))
    }
    /// The value of `return e` in an async generator body: `e` is awaited.
    pub(in crate::lower) fn async_return_value(&mut self, value: Expr) -> Result<Expr> {
        let span = value.span;
        let awaited = self.await_value(span, value)?;
        if self.result == Type::Void {
            return Ok(awaited);
        }
        unions::widen(awaited, Some(self.result), self.types)
    }
    /// `yield value` in an async generator body (AsyncGeneratorYield): the
    /// operand is awaited, the front request is fulfilled with
    /// `{ value, done: false }`, and the body suspends unless another request
    /// is queued. The front request then decides the result: its value, a
    /// throw at the `yield`, or an awaited `return` through enclosing
    /// `finally` blocks (AsyncGeneratorUnwrapYieldResumption).
    pub(in crate::lower) fn async_yield(
        &mut self,
        expression: &ast::YieldExpr,
        generator: GeneratorType,
    ) -> Result<Expr> {
        let span = expression.span;
        if expression.delegate {
            return self.async_delegate(expression);
        }
        let body = self
            .async_body()
            .ok_or_else(|| CompilerError::Internal("yield outside an async body".into()))?;
        let shapes = self.shapes(span, body.generator)?;
        let value = match &expression.arg {
            Some(arg) => self.expr_expected(arg, Some(generator.yielded))?,
            None => undefined(span),
        };
        let awaited = self.await_value(span, value)?;
        let awaited = unions::widen(awaited, Some(generator.yielded), self.types)?;
        if awaited.ty != generator.yielded {
            return Err(self.source.locate(
                type_error(format!(
                    "yield type mismatch: expected {}, found {}",
                    generator.yielded, awaited.ty
                )),
                awaited.span,
            ));
        }
        let complete = self.async_helper(span, body.generator, Operation::CompleteValue)?;
        let value_ty = shapes.value(self.types)?;
        let yielded = self.hidden(generator.yielded);
        let request = self.hidden(shapes.info.request);
        let input = self.hidden(shapes.frame());
        let output = self.hidden(generator.next);
        let marker = self.hidden(Type::Number);
        let returning = shapes.read(
            self.types,
            local(span, request, shapes.info.request),
            RETURNED_FIELD,
        )?;
        let returning = self.await_value(span, returning)?;
        let returning = unions::widen(returning, Some(shapes.returned()), self.types)?;
        let types = &*self.types;
        let block = vec![
            S::Init(yielded, awaited),
            invoke(
                span,
                complete,
                vec![
                    shapes.state(body.state),
                    unions::widen(
                        local(span, yielded, generator.yielded),
                        Some(value_ty),
                        types,
                    )?,
                    boolean(span, false),
                ],
            ),
            S::If(
                shapes.empty(types, body.state)?,
                vec![
                    shapes.set(types, body.state, State::SuspendedYield)?,
                    S::Init(
                        input,
                        Expr {
                            span,
                            ty: shapes.frame(),
                            kind: E::Suspend {
                                value: Box::new(shapes.state(body.state)),
                                state: 0,
                            },
                        },
                    ),
                ],
                vec![],
            ),
            S::Init(request, shapes.front(types, body.state)?),
            S::If(
                shapes.kind_is(types, request, Completion::Throw)?,
                vec![S::Throw(shapes.read(
                    types,
                    local(span, request, shapes.info.request),
                    THROWN_FIELD,
                )?)],
                vec![],
            ),
            S::If(
                shapes.kind_is(types, request, Completion::Return)?,
                vec![S::Set(body.result, returning), S::BreakLabel(body.exit)],
                vec![],
            ),
            S::Init(
                output,
                shapes.read(types, local(span, request, shapes.info.request), NEXT_FIELD)?,
            ),
        ];
        self.flow_reentry();
        Ok(Expr {
            span,
            ty: generator.next,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![yielded, request, input, output],
                body: block,
            },
        })
    }
    /// `yield* inner` over an async generator (the async kind of the
    /// ECMAScript `yield*` evaluation): every received request is forwarded
    /// to `inner` as `next`, `throw` or `return` and its promise awaited; a
    /// value the delegate yields is passed on (AsyncGeneratorYield, without
    /// awaiting it again); a done result ends the delegation with the
    /// delegate's return value, or completes a forwarded `return` with it.
    fn async_delegate(&mut self, expression: &ast::YieldExpr) -> Result<Expr> {
        let span = expression.span;
        let body = self
            .async_body()
            .ok_or_else(|| CompilerError::Internal("yield outside an async body".into()))?;
        let outer = self.shapes(span, body.generator)?;
        let arg = expression
            .arg
            .as_deref()
            .ok_or_else(|| invalid("yield* requires an operand"))?;
        let value = self.expr(arg)?;
        let inner_id = match value.ty {
            Type::Generator(id) if self.types.generators[id as usize].asynchronous.is_some() => id,
            _ => {
                return Err(self.source.locate(
                    rejected("yield* over a value other than an async generator"),
                    value.span,
                ));
            }
        };
        let inner = self.shapes(span, inner_id)?;
        if inner.generator.next != outer.generator.next {
            return Err(self.source.locate(
                type_error(format!(
                    "yield* delegate receives {}, but this generator receives {}",
                    inner.generator.next, outer.generator.next
                )),
                value.span,
            ));
        }
        let next = self.async_helper(span, inner_id, Operation::Next)?;
        let throw = self.async_helper(span, inner_id, Operation::Throw)?;
        let finish = self.async_helper(span, inner_id, Operation::Return)?;
        let complete = self.async_helper(span, body.generator, Operation::CompleteValue)?;
        let exception = self.exception_type()?;
        let outer_value = outer.value(self.types)?;
        let inner_returned = stored(inner.generator.returned);
        let iterator = self.hidden(inner.frame());
        let kind = self.hidden(Type::Float64);
        let sent = self.hidden(outer.generator.next);
        let returning = self.hidden(inner.returned());
        let thrown = self.hidden(exception);
        let result = self.hidden(inner.generator.result);
        let request = self.hidden(outer.info.request);
        let input = self.hidden(outer.frame());
        let output = self.hidden(inner_returned);
        let marker = self.hidden(Type::Number);
        let promise = Type::Promise(inner.info.promise);
        let call = |helper: usize, argument: Expr| Expr {
            span,
            ty: promise,
            kind: E::Call(helper, vec![local(span, iterator, inner.frame()), argument]),
        };
        let resumed = |kind_value: Completion| {
            compare(
                span,
                ast::BinaryOp::EqEqEq,
                local(span, kind, Type::Float64),
                float(span, f64::from(kind_value as i32)),
            )
        };
        let sent_next =
            self.await_promise(span, call(next, local(span, sent, outer.generator.next)))?;
        let sent_throw = self.await_promise(span, call(throw, local(span, thrown, exception)))?;
        let sent_return =
            self.await_promise(span, call(finish, local(span, returning, inner.returned())))?;
        // AsyncGeneratorUnwrapYieldResumption awaits a return request's value.
        let types = &*self.types;
        let requested = local(span, request, outer.info.request);
        let unwrapped = outer.read(types, requested, RETURNED_FIELD)?;
        let unwrapped = self.await_value(span, unwrapped)?;
        let types = &*self.types;
        let unwrapped = unions::widen(unwrapped, Some(inner.returned()), types).map_err(|_| {
            self.source.locate(
                type_error("yield* delegate does not accept this generator's return values"),
                value.span,
            )
        })?;
        let done_value = |types: &ir::ValueTypes| -> Result<Expr> {
            Ok(narrowed(
                inner.read(
                    types,
                    local(span, result, inner.generator.result),
                    VALUE_FIELD,
                )?,
                inner_returned,
            ))
        };
        let completed =
            unions::widen(done_value(types)?, Some(outer.returned()), types).map_err(|_| {
                self.source.locate(
                    type_error("yield* delegate returns values this generator cannot return"),
                    value.span,
                )
            })?;
        let yielded = narrowed(
            inner.read(
                types,
                local(span, result, inner.generator.result),
                VALUE_FIELD,
            )?,
            inner.generator.yielded,
        );
        let yielded = unions::widen(yielded, Some(outer_value), types).map_err(|_| {
            self.source.locate(
                type_error("yield* delegate yields values this generator cannot yield"),
                value.span,
            )
        })?;
        let front = outer.front(types, body.state)?;
        let next_value = outer.read(types, local(span, request, outer.info.request), NEXT_FIELD)?;
        let thrown_value = outer.read(
            types,
            local(span, request, outer.info.request),
            THROWN_FIELD,
        )?;
        let block = vec![
            S::Init(
                iterator,
                Expr {
                    span: value.span,
                    ty: inner.frame(),
                    kind: E::Reinterpret(Box::new(value)),
                },
            ),
            S::Init(kind, float(span, f64::from(Completion::Normal as i32))),
            S::Init(
                sent,
                unions::widen(undefined(span), Some(outer.generator.next), types)
                    .map_err(|_| invalid("yield* needs a TNext that accepts undefined"))?,
            ),
            S::Init(
                returning,
                unions::widen(undefined(span), Some(inner.returned()), types)?,
            ),
            S::Init(thrown, zero(span, exception)),
            S::Init(result, zero(span, inner.generator.result)),
            S::Init(request, zero(span, outer.info.request)),
            S::Init(output, zero(span, inner_returned)),
            S::While(
                boolean(span, true),
                vec![
                    S::If(
                        resumed(Completion::Normal),
                        vec![S::Set(result, sent_next)],
                        vec![S::If(
                            resumed(Completion::Throw),
                            vec![S::Set(result, sent_throw)],
                            vec![S::Set(result, sent_return)],
                        )],
                    ),
                    S::If(
                        inner.read(
                            types,
                            local(span, result, inner.generator.result),
                            DONE_FIELD,
                        )?,
                        vec![
                            S::If(
                                resumed(Completion::Return),
                                vec![S::Set(body.result, completed), S::BreakLabel(body.exit)],
                                vec![],
                            ),
                            S::Set(output, done_value(types)?),
                            S::Break,
                        ],
                        vec![],
                    ),
                    invoke(
                        span,
                        complete,
                        vec![outer.state(body.state), yielded, boolean(span, false)],
                    ),
                    S::If(
                        outer.empty(types, body.state)?,
                        vec![
                            outer.set(types, body.state, State::SuspendedYield)?,
                            S::Init(
                                input,
                                Expr {
                                    span,
                                    ty: outer.frame(),
                                    kind: E::Suspend {
                                        value: Box::new(outer.state(body.state)),
                                        state: 0,
                                    },
                                },
                            ),
                        ],
                        vec![],
                    ),
                    S::Set(request, front),
                    S::If(
                        outer.kind_is(types, request, Completion::Throw)?,
                        vec![
                            S::Set(kind, float(span, f64::from(Completion::Throw as i32))),
                            S::Set(thrown, thrown_value),
                        ],
                        vec![S::If(
                            outer.kind_is(types, request, Completion::Return)?,
                            vec![
                                S::Set(returning, unwrapped),
                                S::Set(kind, float(span, f64::from(Completion::Return as i32))),
                            ],
                            vec![
                                S::Set(kind, float(span, f64::from(Completion::Normal as i32))),
                                S::Set(sent, next_value),
                            ],
                        )],
                    ),
                ],
            ),
        ];
        self.flow_reentry();
        Ok(Expr {
            span,
            ty: inner_returned,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![
                    iterator, sent, returning, thrown, result, request, input, output,
                ],
                body: block,
            },
        })
    }
    /// `g.next(v)`, `g.return(v)` or `g.throw(e)` on an async generator.
    pub(in crate::lower) fn async_generator_request(
        &mut self,
        span: swc_common::Span,
        receiver: Expr,
        completion: Completion,
        input: Expr,
    ) -> Result<Expr> {
        let Type::Generator(generator) = receiver.ty else {
            return Err(CompilerError::Internal("missing async generator".into()));
        };
        let shapes = self.shapes(span, generator)?;
        let operation = match completion {
            Completion::Normal => Operation::Next,
            Completion::Return => Operation::Return,
            Completion::Throw => Operation::Throw,
        };
        let helper = self.async_helper(span, generator, operation)?;
        let input = if completion == Completion::Return {
            unions::widen(input, Some(shapes.returned()), self.types)?
        } else {
            input
        };
        Ok(Expr {
            span,
            ty: Type::Promise(shapes.info.promise),
            kind: E::Call(
                helper,
                vec![
                    Expr {
                        span: receiver.span,
                        ty: shapes.frame(),
                        kind: E::Reinterpret(Box::new(receiver)),
                    },
                    input,
                ],
            ),
        })
    }
    /// Split a lowered async generator declaration or method into its wrapper
    /// and its body generator. The wrapper keeps the declaration's index,
    /// receiver and parameter shape; it creates the state record, creates the
    /// body frame and runs the body's prologue (parameter defaults) to the
    /// start point, so exceptions there throw from the call, and returns the
    /// frame as the async generator object. The body takes the wrapper's
    /// physical parameters followed by the state record; its statements run
    /// inside a labelled block that `return` breaks out of, then the body
    /// completes the front request, drains the queue and returns the state
    /// record, which stays its completion.
    pub(in crate::lower) fn split_async_generator(
        &mut self,
        split: AsyncSplit<'_>,
        body: AsyncBody,
        prologue: Vec<S>,
        statements: Vec<S>,
    ) -> Result<ir::Function> {
        let AsyncSplit {
            incoming,
            shape,
            receiver,
            self_slot,
            captures,
            environment,
            promise: state,
            entry,
            span,
            source,
        } = split;
        self.types.asynchrony.bodies.remove(&self.index);
        if entry || environment || self_slot.is_some() {
            return Err(self
                .source
                .locate(rejected("async generator entry or closure"), span));
        }
        if state as usize != incoming.len() || body.state != state || body.frame != state + 1 {
            return Err(CompilerError::Internal(
                "async generator state must follow the parameters".into(),
            ));
        }
        let shapes = self.shapes(span, body.generator)?;
        let statements = Exits {
            lower: self,
            exit: body.exit,
            result: body.result,
            result_ty: shapes.returned(),
            state: shapes.info.state,
        }
        .list(statements)?;
        let exception = self.exception_type()?;
        let complete_value = self.async_helper(span, body.generator, Operation::CompleteValue)?;
        let complete_throw = self.async_helper(span, body.generator, Operation::CompleteThrow)?;
        let drain = self.async_helper(span, body.generator, Operation::Drain)?;
        let value_ty = shapes.value(self.types)?;
        let caught = self.hidden(exception);
        let marker = self.hidden(Type::Number);
        let initial = unions::widen(undefined(span), Some(shapes.returned()), self.types)?;
        let types = &*self.types;
        let finish = |completed: S| -> Result<Vec<S>> {
            Ok(vec![
                shapes.set(types, state, State::DrainingQueue)?,
                completed,
                invoke(span, drain, vec![shapes.state(state)]),
                S::Return(Some(shapes.state(state))),
            ])
        };
        let mut code = prologue;
        code.push(S::Init(
            body.frame,
            Expr {
                span,
                ty: shapes.frame(),
                kind: E::Suspend {
                    value: Box::new(shapes.state(state)),
                    state: 0,
                },
            },
        ));
        code.push(S::Init(body.result, initial));
        let completed = invoke(
            span,
            complete_value,
            vec![
                shapes.state(state),
                unions::widen(
                    local(span, body.result, shapes.returned()),
                    Some(value_ty),
                    types,
                )?,
                boolean(span, true),
            ],
        );
        let threw = invoke(
            span,
            complete_throw,
            vec![shapes.state(state), local(span, caught, exception)],
        );
        code.push(S::TryCatch {
            body: vec![S::Label {
                id: body.exit,
                iteration: false,
                body: statements,
            }],
            binding: Some(caught),
            handler: finish(threw)?,
            marker,
        });
        code.extend(finish(completed)?);
        let mut params = incoming.clone();
        params.push(shapes.info.state);
        let index = *self.next_function;
        if index >= varta_common::abi::MAX_USER_FUNCTIONS {
            return Err(invalid(
                "too many functions including async generator bodies",
            ));
        }
        *self.next_function += 1;
        let locals: Vec<Type> = (0..self.next)
            .map(|slot| self.local_types.get(&slot).copied().unwrap_or(Type::Number))
            .collect();
        // The wrapper needs its parameters, the state record and the frame.
        let wrapper_locals = locals[..=body.frame as usize].to_vec();
        self.generated.insert(
            index,
            ir::Function {
                shape: ir::ParameterShape::fixed(params.len()),
                self_slot: None,
                receiver: None,
                cells: std::mem::take(&mut self.cells),
                captures: Vec::new(),
                environment: false,
                generator: Some(shapes.info.body),
                source: source.clone(),
                span,
                params,
                result: shapes.frame(),
                locals,
                body: code,
            },
        );
        self.types
            .asynchrony
            .published
            .insert(index, (state, shapes.info.state, shapes.frame()));
        let (head, _) = slot_of(self.types, shapes.info.state, HEAD_FIELD)?;
        let (queue, queue_ty) = slot_of(self.types, shapes.info.state, QUEUE_FIELD)?;
        let (current, _) = slot_of(self.types, shapes.info.state, STATE_FIELD)?;
        let mut args: Vec<Expr> = incoming
            .iter()
            .enumerate()
            .map(|(index, ty)| local(span, index as u32, *ty))
            .collect();
        args.push(shapes.state(state));
        let wrapper = vec![
            S::Init(
                state,
                Expr {
                    span,
                    ty: shapes.info.state,
                    kind: E::Record(vec![
                        (head, float(span, 0.0)),
                        (
                            queue,
                            Expr {
                                span,
                                ty: queue_ty,
                                kind: E::Array(Vec::new()),
                            },
                        ),
                        (
                            current,
                            float(span, f64::from(State::SuspendedStart as i32)),
                        ),
                    ]),
                },
            ),
            S::Init(
                body.frame,
                Expr {
                    span,
                    ty: shapes.frame(),
                    kind: E::Call(index, args),
                },
            ),
            discard(shapes.step(body.frame)),
            S::Return(Some(Expr {
                span,
                ty: Type::Generator(body.generator),
                kind: E::Reinterpret(Box::new(local(span, body.frame, shapes.frame()))),
            })),
        ];
        Ok(ir::Function {
            shape,
            self_slot: None,
            receiver,
            cells: Default::default(),
            captures,
            environment: false,
            generator: None,
            source: source.clone(),
            span,
            params: incoming,
            result: Type::Generator(body.generator),
            locals: wrapper_locals,
            body: wrapper,
        })
    }
}

impl Lower<'_> {
    fn async_helper(
        &mut self,
        span: swc_common::Span,
        generator: u32,
        operation: Operation,
    ) -> Result<usize> {
        let shapes = self.shapes(span, generator)?;
        let key = (
            Helper::AsyncGenerator(operation),
            vec![Type::Generator(generator)],
        );
        if let Some(&index) = self.types.asynchrony.helpers.get(&key) {
            return Ok(index);
        }
        // The helpers call each other (draining awaits a return whose job
        // drains again): memoize the index `synthesize` allocates first.
        let reserved = *self.next_function;
        self.types.asynchrony.helpers.insert(key, reserved);
        let index = (|lower: &mut Self| {
            let exception = lower.exception_type()?;
            let promise = Type::Promise(shapes.info.promise);
            let (params, result, scheduled) = match operation {
                Operation::Next => (vec![shapes.frame(), shapes.generator.next], promise, false),
                Operation::Return => (vec![shapes.frame(), shapes.returned()], promise, false),
                Operation::Throw => (vec![shapes.frame(), exception], promise, false),
                Operation::Resume => (vec![shapes.frame(), shapes.info.state], Type::Void, false),
                Operation::CompleteValue => (
                    vec![shapes.info.state, shapes.value(lower.types)?, Type::Boolean],
                    Type::Void,
                    false,
                ),
                Operation::CompleteThrow => (vec![shapes.info.state, exception], Type::Void, false),
                Operation::Drain | Operation::AwaitReturn => {
                    (vec![shapes.info.state], Type::Void, false)
                }
                Operation::AwaitReturnJob => {
                    let returned = match super::intern(shapes.returned(), lower.types)? {
                        ty @ Type::Promise(_) => ty,
                        _ => return Err(CompilerError::Internal("missing promise".into())),
                    };
                    (vec![shapes.info.state, returned], Type::Void, true)
                }
                Operation::Relay => {
                    return Err(CompilerError::Internal("relay is keyed by promise".into()));
                }
            };
            lower.synthesize(
                Synthetic {
                    span,
                    shape: ir::ParameterShape::fixed(params.len()),
                    params,
                    capture: None,
                    scheduled,
                    result,
                },
                |child, params, _| child.async_operation(generator, shapes, operation, params),
            )
        })(self)?;
        if index != reserved {
            return Err(CompilerError::Internal(
                "async generator helper index changed".into(),
            ));
        }
        Ok(index)
    }
    fn async_operation(
        &mut self,
        generator: u32,
        shapes: Shapes,
        operation: Operation,
        params: &[u32],
    ) -> Result<Vec<S>> {
        let span = shapes.span;
        let promise = Type::Promise(shapes.info.promise);
        Ok(match operation {
            Operation::Next | Operation::Return | Operation::Throw => {
                let (frame, input) = (params[0], params[1]);
                let input_ty = self.local_types[&input];
                let state = self.hidden(shapes.info.state);
                let p = self.hidden(promise);
                let resume = self.async_helper(span, generator, Operation::Resume)?;
                let fulfill = self.fulfill_helper(span, shapes.info.promise)?;
                let reject = self.reject_helper(span, shapes.info.promise)?;
                let await_return = self.async_helper(span, generator, Operation::AwaitReturn)?;
                let kind = match operation {
                    Operation::Next => Completion::Normal,
                    Operation::Return => Completion::Return,
                    _ => Completion::Throw,
                };
                let request = self.request(shapes, kind, local(span, input, input_ty), p)?;
                let types = &*self.types;
                let queue = shapes.read(types, shapes.state(state), QUEUE_FIELD)?;
                let queue_ty = queue.ty;
                let enqueue = discard(Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::ArrayPush(
                        Box::new(queue),
                        Box::new(Expr {
                            span,
                            ty: queue_ty,
                            kind: E::Array(vec![Some(request)]),
                        }),
                    ),
                });
                let resume_now = || {
                    invoke(
                        span,
                        resume,
                        vec![local(span, frame, shapes.frame()), shapes.state(state)],
                    )
                };
                let finished = |value: S| vec![value, S::Return(Some(local(span, p, promise)))];
                let mut body = vec![
                    S::Init(
                        state,
                        Expr {
                            span,
                            ty: shapes.info.state,
                            kind: E::GeneratorValue(Box::new(local(span, frame, shapes.frame()))),
                        },
                    ),
                    S::Init(p, self.promise_object(span, shapes.info.promise, None)?),
                ];
                let types = &*self.types;
                let either = |a: State, b: State| -> Result<Expr> {
                    Ok(Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Binary(
                            ast::BinaryOp::LogicalOr,
                            Box::new(shapes.is(types, state, a)?),
                            Box::new(shapes.is(types, state, b)?),
                        ),
                    })
                };
                match kind {
                    Completion::Normal => {
                        let done = shapes.result(
                            types,
                            unions::widen(undefined(span), Some(shapes.value(types)?), types)?,
                            true,
                        )?;
                        body.extend([
                            S::If(
                                shapes.is(types, state, State::Completed)?,
                                finished(invoke(
                                    span,
                                    fulfill,
                                    vec![local(span, p, promise), done],
                                )),
                                vec![],
                            ),
                            enqueue,
                            S::If(
                                either(State::SuspendedStart, State::SuspendedYield)?,
                                vec![resume_now()],
                                vec![],
                            ),
                        ]);
                    }
                    Completion::Return => body.extend([
                        enqueue,
                        S::If(
                            either(State::SuspendedStart, State::Completed)?,
                            vec![
                                shapes.set(types, state, State::DrainingQueue)?,
                                invoke(span, await_return, vec![shapes.state(state)]),
                            ],
                            vec![S::If(
                                shapes.is(types, state, State::SuspendedYield)?,
                                vec![resume_now()],
                                vec![],
                            )],
                        ),
                    ]),
                    Completion::Throw => body.extend([
                        S::If(
                            shapes.is(types, state, State::SuspendedStart)?,
                            vec![shapes.set(types, state, State::Completed)?],
                            vec![],
                        ),
                        S::If(
                            shapes.is(types, state, State::Completed)?,
                            finished(invoke(
                                span,
                                reject,
                                vec![local(span, p, promise), local(span, input, input_ty)],
                            )),
                            vec![],
                        ),
                        enqueue,
                        S::If(
                            shapes.is(types, state, State::SuspendedYield)?,
                            vec![resume_now()],
                            vec![],
                        ),
                    ]),
                }
                body.push(S::Return(Some(local(span, p, promise))));
                body
            }
            Operation::Resume => {
                let (frame, state) = (params[0], params[1]);
                vec![
                    shapes.set(self.types, state, State::Executing)?,
                    discard(shapes.step(frame)),
                ]
            }
            Operation::CompleteValue | Operation::CompleteThrow => {
                let state = params[0];
                let request = self.hidden(shapes.info.request);
                let mut body = self.dequeue(shapes, state, request)?;
                let types = &*self.types;
                let target = shapes.read(
                    types,
                    local(span, request, shapes.info.request),
                    PROMISE_FIELD,
                )?;
                if operation == Operation::CompleteValue {
                    let (value, done) = (params[1], params[2]);
                    let fulfill = self.fulfill_helper(span, shapes.info.promise)?;
                    let types = &*self.types;
                    let value_ty = shapes.value(types)?;
                    let record = Expr {
                        span,
                        ty: shapes.generator.result,
                        kind: E::Record(vec![
                            (
                                slot_of(types, shapes.generator.result, VALUE_FIELD)?.0,
                                local(span, value, value_ty),
                            ),
                            (
                                slot_of(types, shapes.generator.result, DONE_FIELD)?.0,
                                local(span, done, Type::Boolean),
                            ),
                        ]),
                    };
                    body.push(invoke(span, fulfill, vec![target, record]));
                } else {
                    let reject = self.reject_helper(span, shapes.info.promise)?;
                    let exception = self.exception_type()?;
                    body.push(invoke(
                        span,
                        reject,
                        vec![target, local(span, params[1], exception)],
                    ));
                }
                body
            }
            Operation::Drain => {
                let state = params[0];
                let request = self.hidden(shapes.info.request);
                let await_return = self.async_helper(span, generator, Operation::AwaitReturn)?;
                let complete_value =
                    self.async_helper(span, generator, Operation::CompleteValue)?;
                let complete_throw =
                    self.async_helper(span, generator, Operation::CompleteThrow)?;
                let types = &*self.types;
                let value_ty = shapes.value(types)?;
                let thrown = shapes.read(
                    types,
                    local(span, request, shapes.info.request),
                    THROWN_FIELD,
                )?;
                vec![
                    S::Init(request, zero(span, shapes.info.request)),
                    S::While(
                        boolean(span, true),
                        vec![
                            S::If(
                                shapes.empty(types, state)?,
                                vec![shapes.set(types, state, State::Completed)?, S::Break],
                                vec![],
                            ),
                            S::Set(request, shapes.front(types, state)?),
                            S::If(
                                shapes.kind_is(types, request, Completion::Return)?,
                                vec![
                                    invoke(span, await_return, vec![shapes.state(state)]),
                                    S::Break,
                                ],
                                vec![],
                            ),
                            S::If(
                                shapes.kind_is(types, request, Completion::Throw)?,
                                vec![invoke(
                                    span,
                                    complete_throw,
                                    vec![shapes.state(state), thrown],
                                )],
                                vec![invoke(
                                    span,
                                    complete_value,
                                    vec![
                                        shapes.state(state),
                                        unions::widen(undefined(span), Some(value_ty), types)?,
                                        boolean(span, true),
                                    ],
                                )],
                            ),
                        ],
                    ),
                ]
            }
            Operation::AwaitReturn => {
                // PromiseResolve of the return value, awaited by a job; the
                // value is never a promise (TReturn excludes promises).
                let state = params[0];
                let job = self.async_helper(span, generator, Operation::AwaitReturnJob)?;
                let Type::Promise(returned) = super::intern(shapes.returned(), self.types)? else {
                    return Err(CompilerError::Internal("missing promise".into()));
                };
                let request = shapes.front(self.types, state)?;
                let value = shapes.read(self.types, request, RETURNED_FIELD)?;
                let wrapped = self.promise_object(span, returned, Some(value))?;
                vec![schedule(
                    span,
                    job,
                    vec![shapes.state(state), wrapped],
                    ScheduleMode::Start,
                )]
            }
            Operation::AwaitReturnJob => {
                let (state, source) = (params[0], params[1]);
                let source_ty = self.local_types[&source];
                let complete_value =
                    self.async_helper(span, generator, Operation::CompleteValue)?;
                let complete_throw =
                    self.async_helper(span, generator, Operation::CompleteThrow)?;
                let drain = self.async_helper(span, generator, Operation::Drain)?;
                let exception = self.exception_type()?;
                let value = self.hidden(shapes.returned());
                let caught = self.hidden(exception);
                let awaited = self.await_promise(span, local(span, source, source_ty))?;
                let value_ty = shapes.value(self.types)?;
                vec![
                    S::Init(value, zero(span, shapes.returned())),
                    S::TryCatch {
                        body: vec![
                            S::Set(value, awaited),
                            invoke(
                                span,
                                complete_value,
                                vec![
                                    shapes.state(state),
                                    unions::widen(
                                        local(span, value, shapes.returned()),
                                        Some(value_ty),
                                        self.types,
                                    )?,
                                    boolean(span, true),
                                ],
                            ),
                        ],
                        binding: Some(caught),
                        handler: vec![invoke(
                            span,
                            complete_throw,
                            vec![shapes.state(state), local(span, caught, exception)],
                        )],
                        marker: self.hidden(Type::Number),
                    },
                    invoke(span, drain, vec![shapes.state(state)]),
                ]
            }
            Operation::Relay => {
                return Err(CompilerError::Internal("relay is keyed by promise".into()));
            }
        })
    }
    /// A new request record of `kind` with its input and promise.
    fn request(
        &mut self,
        shapes: Shapes,
        kind: Completion,
        input: Expr,
        promise: u32,
    ) -> Result<Expr> {
        let span = shapes.span;
        let exception = exceptions::exception_union(self.types)?;
        let mut input = Some(input);
        let mut take = |wanted: bool, ty: Type| {
            if wanted {
                input.take().unwrap_or_else(|| zero(span, ty))
            } else {
                zero(span, ty)
            }
        };
        let next = take(kind == Completion::Normal, shapes.generator.next);
        let returned = take(kind == Completion::Return, shapes.returned());
        let thrown = take(kind == Completion::Throw, exception);
        let fields = [
            (KIND_FIELD, float(span, f64::from(kind as i32))),
            (NEXT_FIELD, next),
            (
                PROMISE_FIELD,
                local(span, promise, Type::Promise(shapes.info.promise)),
            ),
            (RETURNED_FIELD, returned),
            (THROWN_FIELD, thrown),
        ];
        let mut record = Vec::with_capacity(fields.len());
        for (name, value) in fields {
            record.push((slot_of(self.types, shapes.info.request, name)?.0, value));
        }
        Ok(Expr {
            span,
            ty: shapes.info.request,
            kind: E::Record(record),
        })
    }
    /// Remove the first request into `request`: the queue array is cleared
    /// once every request was taken, so it never grows past the requests
    /// pending at once.
    fn dequeue(&mut self, shapes: Shapes, state: u32, request: u32) -> Result<Vec<S>> {
        let span = shapes.span;
        let error = self.array_error(ErrorClass::RangeError, "Invalid array length", span)?;
        let types = &*self.types;
        let head = || shapes.read(types, shapes.state(state), HEAD_FIELD);
        Ok(vec![
            S::Init(request, shapes.front(types, state)?),
            shapes.write(
                types,
                shapes.state(state),
                HEAD_FIELD,
                Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Binary(
                        ast::BinaryOp::Add,
                        Box::new(head()?),
                        Box::new(float(span, 1.0)),
                    ),
                },
            )?,
            S::If(
                shapes.empty(types, state)?,
                vec![
                    S::ArrayLength {
                        array: shapes.read(types, shapes.state(state), QUEUE_FIELD)?,
                        length: float(span, 0.0),
                        error,
                    },
                    shapes.write(types, shapes.state(state), HEAD_FIELD, float(span, 0.0))?,
                ],
                vec![],
            ),
        ])
    }
    /// The relay job of `await` in an async generator body: a scheduled frame
    /// `(frame, promise)` that waits for the promise to settle and resumes
    /// the body, which reads the settled promise itself.
    fn relay_helper(&mut self, span: swc_common::Span, body: u32, promise: u32) -> Result<usize> {
        let frame = Type::Generator(body);
        let ty = Type::Promise(promise);
        self.memoized(
            Helper::AsyncGenerator(Operation::Relay),
            vec![frame, ty],
            |lower| {
                lower.synthesize(
                    Self::plain(span, vec![frame, ty], true),
                    |child, params, _| {
                        let (handle, settled) = (params[0], params[1]);
                        let awaited = child.await_promise(span, local(span, settled, ty))?;
                        let step = Expr {
                            span,
                            ty: Type::Number,
                            kind: E::GeneratorStep {
                                generator: Box::new(local(span, handle, frame)),
                                completion: Completion::Normal,
                                value: Box::new(local(span, handle, frame)),
                            },
                        };
                        Ok(vec![
                            S::TryCatch {
                                body: vec![discard(awaited)],
                                binding: None,
                                handler: vec![],
                                marker: child.hidden(Type::Number),
                            },
                            discard(step),
                        ])
                    },
                )
            },
        )
    }
}

impl Lower<'_> {
    /// `for await (x of source)` in an async function or async generator
    /// body over an async generator, a synchronous generator or a checked
    /// array (async-from-sync iteration).
    pub(in crate::lower) fn for_await(
        &mut self,
        statement: &ast::ForOfStmt,
        pattern: &ast::Pat,
        mode: patterns::Mode,
        value: Expr,
    ) -> Result<S> {
        if self.asynchronous.is_none() && self.async_body().is_none() {
            return Err(self.source.locate(
                invalid("for await...of outside an async function"),
                statement.span,
            ));
        }
        match value.ty {
            Type::Generator(id) if self.types.generators[id as usize].asynchronous.is_some() => {
                self.for_await_generator(statement, pattern, mode, value)
            }
            Type::Generator(_) => self.for_await_sync(statement, pattern, mode, value),
            _ => self.for_await_array(statement, pattern, mode, value),
        }
    }
    /// The loop shared by every `for await` source. `step` runs at the top of
    /// each iteration and either breaks out of the loop (the source is done)
    /// or leaves the next value in `current`; an abrupt exit from the body
    /// runs `close` (AsyncIteratorClose), and a throw from the body runs it
    /// with its errors suppressed, so the body's throw keeps precedence. A
    /// throw from the source itself does not close it.
    fn await_loop(
        &mut self,
        head: Head<'_>,
        setup: (Vec<u32>, Vec<S>),
        current: (u32, Type),
        step: Vec<S>,
        close: &mut dyn FnMut(&mut Self) -> Result<Vec<S>>,
    ) -> Result<S> {
        let Head {
            statement,
            pattern,
            mode,
        } = head;
        let span = statement.span;
        let entry = self.flow_snapshot();
        let target = self.flow_iteration();
        let active = self.hidden(Type::Boolean);
        let (current, item_ty) = current;
        let mut item = self.binding_pattern(
            pattern,
            local(pattern.span(), current, item_ty),
            None,
            None,
            mode,
        )?;
        self.loops += 1;
        item.extend(self.single(&statement.body)?);
        self.loops -= 1;
        if self.types.profile.refinement_writes() {
            self.flow_take(target, true);
            let mut paths = self.flow_end(target);
            paths.push(entry.clone()); // An empty source can skip every body store.
            self.flow_join(&entry, &paths)?;
        }
        let slots = self
            .scopes
            .pop()
            .ok_or_else(|| invalid("missing for-await scope"))?
            .values()
            .map(|b| b.slot)
            .collect::<Vec<_>>();
        let mut loop_body = vec![S::Set(active, boolean(span, false))];
        loop_body.extend(step);
        loop_body.push(S::Set(active, boolean(span, true)));
        loop_body.push(S::Scope(slots, item));
        let exception = self.exception_type()?;
        let caught = self.hidden(exception);
        let mut closing = |lower: &mut Self| -> Result<Vec<S>> {
            let mut body = vec![S::Set(active, boolean(span, false))];
            body.extend(close(lower)?);
            Ok(body)
        };
        let quiet = S::TryCatch {
            body: closing(self)?,
            binding: None,
            handler: vec![],
            marker: self.hidden(Type::Number),
        };
        let handler = vec![S::Scope(
            vec![caught],
            vec![
                S::If(local(span, active, Type::Boolean), vec![quiet], vec![]),
                S::Throw(local(span, caught, exception)),
            ],
        )];
        let guarded = S::TryCatch {
            body: vec![S::While(boolean(span, true), loop_body)],
            binding: Some(caught),
            handler,
            marker: self.hidden(Type::Number),
        };
        let marker = self.hidden(Type::Number);
        let completion = self.hidden(Type::Number);
        let pending = self.hidden(exception);
        let result = (self.result != Type::Void).then(|| self.hidden(self.result));
        let mut roots = vec![pending];
        roots.extend(result);
        let protected = S::Scope(
            roots,
            vec![S::TryFinally {
                body: vec![guarded],
                finalizer: vec![S::If(
                    local(span, active, Type::Boolean),
                    closing(self)?,
                    vec![],
                )],
                marker,
                completion,
                exception: pending,
                result,
            }],
        );
        let (mut roots, mut body) = setup;
        roots.push(current);
        body.push(S::Init(active, boolean(span, false)));
        body.push(protected);
        Ok(S::Scope(roots, body))
    }
    /// An awaited `undefined`: one job, as the Await of an already-settled
    /// iterator result in async-from-sync iteration.
    fn tick(&mut self, span: swc_common::Span) -> Result<S> {
        let value = self.await_value(span, undefined(span))?;
        Ok(discard(value))
    }
    /// `for await` over an async generator: each step awaits `next()`, and
    /// closing awaits `return()`.
    fn for_await_generator(
        &mut self,
        statement: &ast::ForOfStmt,
        pattern: &ast::Pat,
        mode: patterns::Mode,
        value: Expr,
    ) -> Result<S> {
        let span = statement.span;
        let Type::Generator(id) = value.ty else {
            return Err(CompilerError::Internal("missing async generator".into()));
        };
        let shapes = self.shapes(span, id)?;
        // Closing sends `return(undefined)`, which every async generator
        // admits; stepping sends `next(undefined)`.
        if !generators::accepts_undefined(shapes.generator.next, self.types) {
            return Err(self.source.locate(
                type_error(
                    "for await...of over an async generator requires TNext to accept undefined",
                ),
                value.span,
            ));
        }
        let ty = value.ty;
        let iterator = self.hidden(ty);
        let result = self.hidden(shapes.generator.result);
        let current = self.hidden(shapes.generator.yielded);
        let next = self.async_helper(span, id, Operation::Next)?;
        let finish = self.async_helper(span, id, Operation::Return)?;
        let handle = move || Expr {
            span,
            ty: shapes.frame(),
            kind: E::Reinterpret(Box::new(local(span, iterator, ty))),
        };
        let call = |helper: usize, input: Type, lower: &Self| -> Result<Expr> {
            Ok(Expr {
                span,
                ty: Type::Promise(shapes.info.promise),
                kind: E::Call(
                    helper,
                    vec![
                        handle(),
                        unions::widen(undefined(span), Some(input), lower.types)?,
                    ],
                ),
            })
        };
        let requested = call(next, shapes.generator.next, self)?;
        let awaited = self.await_promise(span, requested)?;
        let types = &*self.types;
        let outcome = local(span, result, shapes.generator.result);
        let done = shapes.read(types, outcome, DONE_FIELD)?;
        let item = narrowed(
            shapes.read(
                types,
                local(span, result, shapes.generator.result),
                VALUE_FIELD,
            )?,
            shapes.generator.yielded,
        );
        let step = vec![
            S::Init(result, awaited),
            S::If(done, vec![S::Break], vec![]),
            S::Init(current, item),
        ];
        let mut close = |lower: &mut Self| -> Result<Vec<S>> {
            let request = call(finish, shapes.returned(), lower)?;
            let awaited = lower.await_promise(span, request)?;
            Ok(vec![discard(awaited)])
        };
        self.await_loop(
            Head {
                statement,
                pattern,
                mode,
            },
            (vec![iterator, result], vec![S::Init(iterator, value)]),
            (current, shapes.generator.yielded),
            step,
            &mut close,
        )
    }
    /// The await of one value of async-from-sync iteration: PromiseResolve of
    /// the value and its unwrapping reaction (one job), then the Await of the
    /// unwrapped result (another job). A rejection runs `rejected` (the sync
    /// iterator's closing) in the first job and throws after the second.
    fn unwrap_value(
        &mut self,
        span: swc_common::Span,
        value: Expr,
        current: u32,
        rejected: Vec<S>,
    ) -> Result<Vec<S>> {
        let exception = self.exception_type()?;
        let failed = self.hidden(Type::Boolean);
        let reason = self.hidden(exception);
        let caught = self.hidden(exception);
        let ty = self.local_types[&current];
        let awaited = self.await_element(span, value, ty)?;
        let mut handler = rejected;
        handler.extend([
            S::Set(failed, boolean(span, true)),
            S::Set(reason, local(span, caught, exception)),
        ]);
        Ok(vec![
            S::Init(failed, boolean(span, false)),
            S::Init(reason, zero(span, exception)),
            S::Init(current, zero(span, ty)),
            S::TryCatch {
                body: vec![S::Set(current, awaited)],
                binding: Some(caught),
                handler,
                marker: self.hidden(Type::Number),
            },
            self.tick(span)?,
            S::If(
                local(span, failed, Type::Boolean),
                vec![S::Throw(local(span, reason, exception))],
                vec![],
            ),
        ])
    }
    /// The members of a sparse element type `E | undefined` whose `E` holds a
    /// promise and another member: `E` alone, so the hole is awaited apart.
    fn holed(&self, ty: Type) -> Option<Vec<Type>> {
        let members = unions::members(ty, self.types);
        (members.len() > 2
            && members.contains(&Type::Undefined)
            && members.iter().any(|m| matches!(m, Type::Promise(_))))
        .then(|| {
            members
                .into_iter()
                .filter(|m| *m != Type::Undefined)
                .collect()
        })
    }
    /// The awaited value of an element or yielded value `value` (a local),
    /// widened to `target`. A hole of a sparse array is awaited as
    /// `undefined`, the element otherwise.
    fn await_element(&mut self, span: swc_common::Span, value: Expr, target: Type) -> Result<Expr> {
        let Some(members) = self.holed(value.ty) else {
            let awaited = self.await_value(span, value)?;
            return unions::widen(awaited, Some(target), self.types);
        };
        let E::Local(slot) = value.kind else {
            return Err(CompilerError::Internal(
                "holed element is not a local".into(),
            ));
        };
        let ty = value.ty;
        let element = unions::intern(members, self.types)?;
        let hole = self.await_value(span, undefined(span))?;
        let hole = unions::widen(hole, Some(target), self.types)?;
        let present = self.await_value(span, narrowed(local(span, slot, ty), element))?;
        let present = unions::widen(present, Some(target), self.types)?;
        Ok(Expr {
            span,
            ty: target,
            kind: E::Conditional(
                Box::new(Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsUndefined(Box::new(local(span, slot, ty))),
                }),
                Box::new(hole),
                Box::new(present),
            ),
        })
    }
    /// The awaited type of an element or yielded value of a sync source.
    fn awaited_type(&mut self, ty: Type) -> Result<Type> {
        if let Some(members) = self.holed(ty) {
            let element = unions::intern(members, self.types)?;
            let awaited = self.awaited_type(element)?;
            return unions::intern(vec![awaited, Type::Undefined], self.types);
        }
        match ty {
            Type::Promise(id) => Ok(stored(self.types.promises[id as usize].value)),
            ty => match self.promise_member(ty, swc_common::DUMMY_SP)? {
                Some((Type::Promise(id), rest)) => {
                    let value = stored(self.types.promises[id as usize].value);
                    unions::intern(vec![value, rest], self.types)
                }
                _ => Ok(ty),
            },
        }
    }
    /// `for await` over a checked array (CreateAsyncFromSyncIterator over the
    /// array iterator): every element and the final done result take two
    /// jobs; closing takes one, because the array iterator has no `return`.
    fn for_await_array(
        &mut self,
        statement: &ast::ForOfStmt,
        pattern: &ast::Pat,
        mode: patterns::Mode,
        value: Expr,
    ) -> Result<S> {
        let span = statement.span;
        let (source, array, mut roots, mut init) = self.for_of_source(value).map_err(|_| {
            self.source.locate(
                type_error(
                    "for await...of requires an async generator, a generator or a checked array",
                ),
                statement.right.span(),
            )
        })?;
        let Type::Array(id) = array else {
            return Err(invalid("missing for-await array type"));
        };
        let element = self.types.arrays[id as usize];
        let item = if self.types.profile.sparse_arrays() {
            unions::intern(vec![element, Type::Undefined], self.types)?
        } else {
            element
        };
        let awaited = self.awaited_type(item)?;
        let index = self.hidden(Type::Float64);
        let read = self.hidden(item);
        let current = self.hidden(awaited);
        roots.extend([index, read]);
        init.push(S::Init(index, float(span, 0.0)));
        let length = Expr {
            span,
            ty: Type::Float64,
            kind: E::Length(Box::new(local(span, source, array))),
        };
        let mut step = vec![S::If(
            compare(
                span,
                ast::BinaryOp::GtEq,
                local(span, index, Type::Float64),
                length,
            ),
            vec![self.tick(span)?, self.tick(span)?, S::Break],
            vec![],
        )];
        step.push(S::Init(
            read,
            Expr {
                span: statement.right.span(),
                ty: item,
                kind: if self.types.profile.sparse_arrays() {
                    E::MaybeIndex(
                        Box::new(local(span, source, array)),
                        Box::new(local(span, index, Type::Float64)),
                    )
                } else {
                    E::Index(
                        Box::new(local(span, source, array)),
                        Box::new(local(span, index, Type::Float64)),
                    )
                },
            },
        ));
        step.push(S::Set(
            index,
            Expr {
                span,
                ty: Type::Float64,
                kind: E::Binary(
                    ast::BinaryOp::Add,
                    Box::new(local(span, index, Type::Float64)),
                    Box::new(float(span, 1.0)),
                ),
            },
        ));
        step.extend(self.unwrap_value(span, local(span, read, item), current, Vec::new())?);
        let mut close = |lower: &mut Self| -> Result<Vec<S>> { Ok(vec![lower.tick(span)?]) };
        self.await_loop(
            Head {
                statement,
                pattern,
                mode,
            },
            (roots, init),
            (current, awaited),
            step,
            &mut close,
        )
    }
    /// `for await` over a synchronous generator (CreateAsyncFromSyncIterator):
    /// a step resumes the generator synchronously and awaits its value in
    /// two jobs; a rejected value closes the generator in the first job.
    /// Closing calls `return()` synchronously, then takes two jobs (one when
    /// `return()` throws).
    fn for_await_sync(
        &mut self,
        statement: &ast::ForOfStmt,
        pattern: &ast::Pat,
        mode: patterns::Mode,
        value: Expr,
    ) -> Result<S> {
        let span = statement.span;
        let Type::Generator(id) = value.ty else {
            return Err(CompilerError::Internal("missing generator".into()));
        };
        let generator = self.types.generators[id as usize];
        if !generators::accepts_undefined(generator.next, self.types)
            || !generators::accepts_undefined(generator.returned, self.types)
        {
            return Err(self.source.locate(
                type_error(
                    "for await...of over a generator requires TNext and TReturn to accept undefined",
                ),
                value.span,
            ));
        }
        let ty = value.ty;
        let iterator = self.hidden(ty);
        let outcome = self.hidden(Type::Number);
        let awaited = self.awaited_type(generator.yielded)?;
        let current = self.hidden(awaited);
        let exception = self.exception_type()?;
        let it = move || local(span, iterator, ty);
        let returned = generator.returned;
        // The sync `return()`: `Some(reason)` receives a throw from it.
        let sync_return = |lower: &mut Self| -> Result<(Vec<S>, u32, u32)> {
            let input = if returned == Type::Void {
                undefined(span)
            } else {
                unions::widen(undefined(span), Some(returned), lower.types)?
            };
            let closing = lower.hidden(Type::Number);
            let failed = lower.hidden(Type::Boolean);
            let reason = lower.hidden(exception);
            let caught = lower.hidden(exception);
            let steps = lower.generator_step(span, it(), Completion::Return, input, closing)?;
            Ok((
                vec![
                    S::Init(failed, boolean(span, false)),
                    S::Init(reason, zero(span, exception)),
                    S::TryCatch {
                        body: steps,
                        binding: Some(caught),
                        handler: vec![
                            S::Set(failed, boolean(span, true)),
                            S::Set(reason, local(span, caught, exception)),
                        ],
                        marker: lower.hidden(Type::Number),
                    },
                ],
                failed,
                reason,
            ))
        };
        // Rejected value: IteratorClose with a throw completion ignores errors.
        let (rejected, _, _) = sync_return(self)?;
        let failed = self.hidden(Type::Boolean);
        let reason = self.hidden(exception);
        let caught = self.hidden(exception);
        let steps =
            self.generator_step(span, it(), Completion::Normal, undefined(span), outcome)?;
        let mut step = vec![
            S::Init(failed, boolean(span, false)),
            S::Init(reason, zero(span, exception)),
            S::TryCatch {
                body: steps,
                binding: Some(caught),
                handler: vec![
                    S::Set(failed, boolean(span, true)),
                    S::Set(reason, local(span, caught, exception)),
                ],
                marker: self.hidden(Type::Number),
            },
            // A throwing `next()` rejects the step's promise: one job.
            S::If(
                local(span, failed, Type::Boolean),
                vec![self.tick(span)?, S::Throw(local(span, reason, exception))],
                vec![],
            ),
            S::If(
                compare(
                    span,
                    ast::BinaryOp::EqEqEq,
                    local(span, outcome, Type::Number),
                    Expr {
                        span,
                        ty: Type::Number,
                        kind: E::Integer(ir::GeneratorOutcome::Yielded as i32),
                    },
                ),
                vec![],
                vec![self.tick(span)?, self.tick(span)?, S::Break],
            ),
        ];
        // The yielded value is read directly after the step that produced it.
        let read = self.hidden(generator.yielded);
        step.push(S::Init(
            read,
            Expr {
                span: statement.right.span(),
                ty: generator.yielded,
                kind: E::GeneratorValue(Box::new(it())),
            },
        ));
        step.extend(self.unwrap_value(
            span,
            local(span, read, generator.yielded),
            current,
            rejected,
        )?);
        let mut close = |lower: &mut Self| -> Result<Vec<S>> {
            let (mut body, failed, reason) = sync_return(lower)?;
            body.push(S::If(
                local(span, failed, Type::Boolean),
                vec![lower.tick(span)?, S::Throw(local(span, reason, exception))],
                vec![lower.tick(span)?, lower.tick(span)?],
            ));
            Ok(body)
        };
        self.await_loop(
            Head {
                statement,
                pattern,
                mode,
            },
            (vec![iterator], vec![S::Init(iterator, value)]),
            (current, awaited),
            step,
            &mut close,
        )
    }
}
