//! Synchronous generators (v33 `generators`): `Generator<T, TReturn, TNext>`
//! annotations, `yield`, the `next`/`return`/`throw` protocol and `for...of`
//! with iterator closing. A generator object is its continuation frame; every
//! resumption goes through the closed step function of its generator type,
//! whose outcome code the lowered protocol code below turns into an
//! `IteratorResult` record or a throw.
use super::*;
use ir::{GeneratorOutcome as Outcome, GeneratorType};
use varta_common::function::frame::{self, Completion};
use varta_common::value::ErrorClass;

/// ECMAScript's message for re-entering an executing generator.
const RUNNING_MESSAGE: &str = "Generator is already running";
const DONE_FIELD: &str = "done";
const VALUE_FIELD: &str = "value";

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn undefined(span: swc_common::Span) -> Expr {
    Expr {
        span,
        ty: Type::Undefined,
        kind: E::Integer(0),
    }
}
fn boolean(span: swc_common::Span, value: bool) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Integer(i32::from(value)),
    }
}
fn is(span: swc_common::Span, slot: u32, outcome: Outcome) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Binary(
            ast::BinaryOp::EqEqEq,
            Box::new(local(span, slot, Type::Number)),
            Box::new(Expr {
                span,
                ty: Type::Number,
                kind: E::Integer(outcome as i32),
            }),
        ),
    }
}
fn rejected(what: &str) -> CompilerError {
    CompilerError::Unsupported {
        kind: what.into(),
        detail: "outside the synchronous generator subset of checked-f64-utf16-v33".into(),
    }
}

/// Generator objects have no value tag: they stay in locals, parameters,
/// results, closure cells and frames, never inside a value ABI aggregate.
pub(super) fn opaque(container: &str) -> CompilerError {
    rejected(&format!("generator inside {container}"))
}

/// `Generator<T>`, `Generator<T, TReturn>` or `Generator<T, TReturn, TNext>`.
/// TypeScript defaults omitted parameters to `any`; Varta reads an omitted
/// `TReturn` as `void` and an omitted `TNext` as `undefined`, which only
/// rejects programs that would need `any`. A `void` `TNext` is `undefined`.
pub(super) fn annotation(reference: &ast::TsTypeRef, types: &mut ir::ValueTypes) -> Result<Type> {
    let (yielded, returned, next) = arguments(reference, "Generator", types)?;
    intern(yielded, returned, next, types)
}

/// The `T`, `TReturn` and `TNext` of a `Generator` or (v36)
/// `AsyncGenerator`-like annotation named `name`, with the defaults above.
pub(super) fn arguments(
    reference: &ast::TsTypeRef,
    name: &str,
    types: &mut ir::ValueTypes,
) -> Result<(Type, Type, Type)> {
    let params = reference
        .type_params
        .as_ref()
        .map(|p| p.params.as_slice())
        .unwrap_or_default();
    if !(1..=3).contains(&params.len()) {
        return Err(rejected(&format!("{name} type argument count")));
    }
    let yielded = keyword_type(&params[0], types)?;
    let returned = params
        .get(1)
        .map(|ty| keyword_type(ty, types))
        .transpose()?
        .unwrap_or(Type::Void);
    let next = match params
        .get(2)
        .map(|ty| keyword_type(ty, types))
        .transpose()?
    {
        None | Some(Type::Void) => Type::Undefined,
        Some(ty) => ty,
    };
    for ty in [yielded, returned, next] {
        if matches!(ty, Type::Generator(_)) {
            return Err(rejected(
                "generator yielding, returning or receiving a generator",
            ));
        }
        if ty.width() > 1 {
            return Err(rejected("VartaResult inside a Generator type"));
        }
    }
    if yielded == Type::Void {
        return Err(rejected("void generator yield type"));
    }
    Ok((yielded, returned, next))
}

/// `IteratorResult<T>` or `IteratorResult<T, TReturn>`: the closed record
/// `{ done: boolean, value: T | TReturn | undefined }` that `next`, `return`
/// and `throw` produce (an omitted `TReturn` is `void`, as for `Generator`).
pub(super) fn iterator_result(
    reference: &ast::TsTypeRef,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let params = reference
        .type_params
        .as_ref()
        .map(|p| p.params.as_slice())
        .unwrap_or_default();
    if !(1..=2).contains(&params.len()) {
        return Err(rejected("IteratorResult type argument count"));
    }
    let yielded = keyword_type(&params[0], types)?;
    let returned = params
        .get(1)
        .map(|ty| keyword_type(ty, types))
        .transpose()?
        .unwrap_or(Type::Void);
    result_record(yielded, returned, types)
}

pub(super) fn result_record(
    yielded: Type,
    returned: Type,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    if [yielded, returned]
        .iter()
        .any(|ty| matches!(ty, Type::Generator(_)) || ty.width() > 1)
        || yielded == Type::Void
    {
        return Err(rejected("IteratorResult value type"));
    }
    let completed = if returned == Type::Void {
        Type::Undefined
    } else {
        returned
    };
    let value = unions::intern(vec![yielded, completed, Type::Undefined], types).map_err(|_| {
        rejected("Generator whose IteratorResult value union T | TReturn | undefined is not a checked union")
    })?;
    records::intern(
        vec![
            ir::RecordField {
                name: DONE_FIELD.into(),
                ty: Type::Boolean,
                optional: false,
            },
            ir::RecordField {
                name: VALUE_FIELD.into(),
                ty: value,
                optional: false,
            },
        ],
        types,
    )
}

fn intern(yielded: Type, returned: Type, next: Type, types: &mut ir::ValueTypes) -> Result<Type> {
    if let Some(id) = types.generators.iter().position(|g| {
        (g.yielded, g.returned, g.next) == (yielded, returned, next)
            && g.asynchronous.is_none()
            && !g.body
    }) {
        return Ok(Type::Generator(id as u32));
    }
    if types.generators.len() >= frame::MAX_GENERATOR_TYPES {
        return Err(invalid("generator type count exceeds limit"));
    }
    let result = result_record(yielded, returned, types)?;
    let id = types.generators.len() as u32;
    types.generators.push(GeneratorType {
        yielded,
        returned,
        next,
        result,
        asynchronous: None,
        body: false,
    });
    Ok(Type::Generator(id))
}

/// Whether `ty` accepts the value `undefined` (`void` counts for `TReturn`).
pub(super) fn accepts_undefined(ty: Type, types: &ir::ValueTypes) -> bool {
    ty == Type::Void || unions::members(ty, types).contains(&Type::Undefined)
}

/// Field slots of the IteratorResult record: names are sorted, so `done` then `value`.
pub(super) fn field(types: &ir::ValueTypes, result: Type, name: &str) -> Result<u32> {
    let Type::Record(id) = result else {
        return Err(CompilerError::Internal(
            "IteratorResult is not a record".into(),
        ));
    };
    types.records[id as usize]
        .fields
        .iter()
        .position(|f| f.name == name)
        .map(|i| i as u32)
        .ok_or_else(|| CompilerError::Internal("missing IteratorResult field".into()))
}

impl Lower<'_> {
    fn generator_type(&self, id: u32) -> GeneratorType {
        self.types.generators[id as usize]
    }
    fn coerce(&mut self, value: Expr, ty: Type) -> Result<Expr> {
        let span = value.span;
        let value =
            unions::widen(value, Some(ty), self.types).map_err(|e| self.source.locate(e, span))?;
        if value.ty != ty {
            return Err(self.source.locate(
                type_error(format!("expected {ty}, found {}", value.ty)),
                value.span,
            ));
        }
        Ok(value)
    }
    /// `yield value` or bare `yield` inside a `function*` body.
    pub(super) fn yield_expression(&mut self, expression: &ast::YieldExpr) -> Result<Expr> {
        let Some(id) = self.generator else {
            return Err(self.source.locate(
                invalid("yield outside a generator function"),
                expression.span,
            ));
        };
        let generator = self.generator_type(id);
        if generator.asynchronous.is_some() {
            return self.async_yield(expression, generator);
        }
        if expression.delegate {
            return Err(self
                .source
                .locate(rejected("yield* delegation"), expression.span));
        }
        let value = match &expression.arg {
            Some(arg) => self.expr_expected(arg, Some(generator.yielded))?,
            None => undefined(expression.span),
        };
        let value = self.coerce(value, generator.yielded)?;
        Ok(Expr {
            span: expression.span,
            ty: generator.next,
            kind: E::Suspend {
                value: Box::new(value),
                state: 0,
            },
        })
    }
    fn running_error(&mut self, span: swc_common::Span) -> Result<S> {
        let error = self.array_error(ErrorClass::TypeError, RUNNING_MESSAGE, span)?;
        Ok(S::Throw(error))
    }
    /// `Init(outcome, step)` followed by the re-entry `TypeError`, for other
    /// protocols over synchronous generators (v36 `for await`).
    pub(super) fn generator_step(
        &mut self,
        span: swc_common::Span,
        generator: Expr,
        completion: Completion,
        value: Expr,
        outcome: u32,
    ) -> Result<Vec<S>> {
        self.step(span, generator, completion, value, outcome)
    }
    /// `Init(outcome, step)` followed by the re-entry `TypeError`.
    fn step(
        &mut self,
        span: swc_common::Span,
        generator: Expr,
        completion: Completion,
        value: Expr,
        outcome: u32,
    ) -> Result<Vec<S>> {
        Ok(vec![
            S::Init(
                outcome,
                Expr {
                    span,
                    ty: Type::Number,
                    kind: E::GeneratorStep {
                        generator: Box::new(generator),
                        completion,
                        value: Box::new(value),
                    },
                },
            ),
            S::If(
                is(span, outcome, Outcome::Running),
                vec![self.running_error(span)?],
                vec![],
            ),
        ])
    }
    /// `g.next(v)`, `g.return(v)` or `g.throw(e)` on a generator receiver.
    pub(super) fn generator_method(
        &mut self,
        call: &ast::CallExpr,
        receiver: Expr,
        method: &str,
    ) -> Result<Expr> {
        let span = call.span;
        let Type::Generator(id) = receiver.ty else {
            return Err(self.source.locate(
                type_error(format!("{method} requires a generator receiver")),
                receiver.span,
            ));
        };
        let generator = self.generator_type(id);
        if call.args.iter().any(|arg| arg.spread.is_some()) || call.args.len() > 1 {
            return Err(self
                .source
                .locate(rejected("generator method arguments"), span));
        }
        let argument = call.args.first().map(|arg| &*arg.expr);
        let (completion, input_ty) = match method {
            "next" => (Completion::Normal, generator.next),
            "return" => (
                Completion::Return,
                if generator.returned == Type::Void {
                    Type::Undefined
                } else {
                    generator.returned
                },
            ),
            _ => (Completion::Throw, self.exception_type()?),
        };
        let input = match argument {
            Some(arg) if completion == Completion::Throw => {
                let value = self.expr(arg)?;
                self.thrown(value)?
            }
            Some(arg) => {
                let value = self.expr_expected(arg, Some(input_ty))?;
                self.coerce(value, input_ty)?
            }
            None if completion == Completion::Throw || accepts_undefined(input_ty, self.types) => {
                let value = undefined(span);
                if completion == Completion::Throw {
                    self.thrown(value)?
                } else {
                    self.coerce(value, input_ty)?
                }
            }
            None => {
                return Err(self.source.locate(
                    type_error(format!(
                        "{method}() without an argument requires a generator type whose {} accepts undefined",
                        if completion == Completion::Normal { "TNext" } else { "TReturn" }
                    )),
                    span,
                ));
            }
        };
        if generator.asynchronous.is_some() {
            // An async generator queues the request and returns its promise (v36).
            return self.async_generator_request(span, receiver, completion, input);
        }
        let gen_slot = self.hidden(receiver.ty);
        let input_slot = self.hidden(input.ty);
        let outcome = self.hidden(Type::Number);
        let output = self.hidden(generator.result);
        let done = self.hidden(Type::Boolean);
        let Type::Record(shape) = generator.result else {
            return Err(CompilerError::Internal(
                "IteratorResult is not a record".into(),
            ));
        };
        let value_ty = self.types.records[shape as usize].fields
            [field(self.types, generator.result, VALUE_FIELD)? as usize]
            .ty;
        let value = self.hidden(value_ty);
        let marker = self.hidden(Type::Number);
        let gen_ty = receiver.ty;
        let handle = || local(span, gen_slot, gen_ty);
        let input_ty = input.ty;
        let mut body = vec![S::Init(gen_slot, receiver), S::Init(input_slot, input)];
        body.extend(self.step(
            span,
            handle(),
            completion,
            local(span, input_slot, input_ty),
            outcome,
        )?);
        if completion == Completion::Throw {
            body.push(S::If(
                is(span, outcome, Outcome::ThrowInput),
                vec![S::Throw(local(span, input_slot, input_ty))],
                vec![],
            ));
        }
        let read = |ty: Type| Expr {
            span,
            ty,
            kind: E::GeneratorValue(Box::new(handle())),
        };
        let yielded = unions::widen(read(generator.yielded), Some(value_ty), self.types)?;
        let returned = if generator.returned == Type::Void {
            undefined(span)
        } else {
            read(generator.returned)
        };
        let returned = unions::widen(returned, Some(value_ty), self.types)?;
        let fallback = unions::widen(undefined(span), Some(value_ty), self.types)?;
        body.extend([
            S::Init(done, boolean(span, true)),
            S::Init(value, fallback),
            S::If(
                is(span, outcome, Outcome::Yielded),
                vec![S::Set(done, boolean(span, false)), S::Set(value, yielded)],
                vec![S::If(
                    is(span, outcome, Outcome::Returned),
                    vec![S::Set(value, returned)],
                    vec![],
                )],
            ),
        ]);
        if completion == Completion::Return {
            let echoed = unions::widen(
                local(span, input_slot, input_ty),
                Some(value_ty),
                self.types,
            )?;
            body.push(S::If(
                is(span, outcome, Outcome::DoneInput),
                vec![S::Set(value, echoed)],
                vec![],
            ));
        }
        let done_field = field(self.types, generator.result, DONE_FIELD)?;
        let value_field = field(self.types, generator.result, VALUE_FIELD)?;
        body.push(S::Init(
            output,
            Expr {
                span,
                ty: generator.result,
                kind: E::Record(vec![
                    (done_field, local(span, done, Type::Boolean)),
                    (value_field, local(span, value, value_ty)),
                ]),
            },
        ));
        Ok(Expr {
            span,
            ty: generator.result,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![gen_slot, input_slot, value, output],
                body,
            },
        })
    }
    /// `for (const x of generator)`: each step resumes with `undefined`; an
    /// abrupt exit from the body closes the generator through `return()`, so
    /// its `finally` blocks run first. A throw from the body suppresses a throw
    /// from closing; a throw from the generator itself does not close it.
    pub(super) fn for_of_generator(
        &mut self,
        statement: &ast::ForOfStmt,
        pattern: &ast::Pat,
        mode: patterns::Mode,
        value: Expr,
    ) -> Result<S> {
        let span = statement.span;
        let Type::Generator(id) = value.ty else {
            return Err(CompilerError::Internal("missing for-of generator".into()));
        };
        let generator = self.generator_type(id);
        if generator.asynchronous.is_some() {
            return Err(self.source.locate(
                type_error("for-of over an async generator requires for await...of"),
                value.span,
            ));
        }
        if !accepts_undefined(generator.next, self.types)
            || !accepts_undefined(generator.returned, self.types)
        {
            return Err(self.source.locate(
                type_error(
                    "for-of over a generator requires TNext and TReturn to accept undefined",
                ),
                value.span,
            ));
        }
        let entry = self.flow_snapshot();
        let target = self.flow_iteration();
        let ty = value.ty;
        let iterator = self.hidden(ty);
        let active = self.hidden(Type::Boolean);
        let outcome = self.hidden(Type::Number);
        let closing = self.hidden(Type::Number);
        let current = self.hidden(generator.yielded);
        let it = || local(span, iterator, ty);
        let returned = generator.returned;
        let close = |lower: &mut Self| -> Result<Vec<S>> {
            let input = if returned == Type::Void {
                undefined(span)
            } else {
                unions::widen(undefined(span), Some(returned), lower.types)?
            };
            let mut close = vec![S::Set(active, boolean(span, false))];
            close.extend(lower.step(span, it(), Completion::Return, input, closing)?);
            Ok(close)
        };
        // IteratorStepValue precedes evaluation of an assignment reference.
        let mut item = vec![S::Init(
            current,
            Expr {
                span: statement.right.span(),
                ty: generator.yielded,
                kind: E::GeneratorValue(Box::new(it())),
            },
        )];
        item.extend(self.binding_pattern(
            pattern,
            local(pattern.span(), current, generator.yielded),
            None,
            None,
            mode,
        )?);
        self.loops += 1;
        item.extend(self.single(&statement.body)?);
        self.loops -= 1;
        if self.types.profile.refinement_writes() {
            self.flow_take(target, true);
            let mut paths = self.flow_end(target);
            paths.push(entry.clone()); // An empty generator can skip every body store.
            self.flow_join(&entry, &paths)?;
        }
        let slots = self
            .scopes
            .pop()
            .ok_or_else(|| invalid("missing for-of scope"))?
            .values()
            .map(|b| b.slot)
            .collect::<Vec<_>>();
        let mut iteration = vec![current];
        iteration.extend(slots);
        let mut loop_body = vec![S::Set(active, boolean(span, false))];
        loop_body.extend(self.step(span, it(), Completion::Normal, undefined(span), outcome)?);
        loop_body.extend([
            S::If(is(span, outcome, Outcome::Yielded), vec![], vec![S::Break]),
            S::Set(active, boolean(span, true)),
            S::Scope(iteration, item),
        ]);
        let exception = self.exception_type()?;
        let caught = self.hidden(exception);
        let quiet = S::TryCatch {
            body: close(self)?,
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
                    close(self)?,
                    vec![],
                )],
                marker,
                completion,
                exception: pending,
                result,
            }],
        );
        Ok(S::Scope(
            vec![iterator],
            vec![
                S::Init(iterator, value),
                S::Init(active, boolean(span, false)),
                protected,
            ],
        ))
    }
}
