//! `Promise.all`, `allSettled`, `race` and `any` (v35 `promise_values`) over
//! an array of `Promise<T>`. The call snapshots the array, creates the result
//! promise and starts one synthesized job frame per element, in index order;
//! each job awaits its element (registering one reaction, as the combinator's
//! `then` does in ECMAScript) and updates a shared private state record. The
//! last element to settle builds the result from the settled elements, so the
//! job order and the result match Node.
use super::*;

/// The four combinators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Combinator {
    All,
    AllSettled,
    Race,
    Any,
}
impl Combinator {
    pub(super) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "all" => Self::All,
            "allSettled" => Self::AllSettled,
            "race" => Self::Race,
            "any" => Self::Any,
            _ => return None,
        })
    }
    fn name(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::AllSettled => "allSettled",
            Self::Race => "race",
            Self::Any => "any",
        }
    }
}

/// The argument of a combinator call, as `race_items` lowers it.
enum RaceItems {
    /// Not an array literal of promises of different types: lower as usual.
    Ordinary,
    /// An array literal of promises of one type, already lowered.
    Array(Expr),
    /// The whole result of a race over promises of different types.
    Mixed(Expr),
}

/// Private fields of a combinator's state record.
const ITEMS: &str = "items";
const REMAINING: &str = "remaining";
const RESULT: &str = "result";
/// Fields and statuses of an `allSettled` result element.
const STATUS: &str = "status";
const FULFILLED: &str = "fulfilled";
const REJECTED: &str = "rejected";
/// Message of the `Promise.any` rejection (V8's `AggregateError` message).
/// Varta has no `AggregateError` class: the reason is a native `Error`.
const AGGREGATE_MESSAGE: &str = "All promises were rejected";

impl Lower<'_> {
    /// The record type of one `allSettled` element of `Promise<T>`:
    /// `{ status: string; value?: T; reason?: unknown }`.
    fn settled_record(&mut self, value: Type) -> Result<Type> {
        let value = unions::intern(vec![stored(value), Type::Undefined], self.types)?;
        records::intern(
            vec![
                ir::RecordField {
                    name: STATUS.into(),
                    ty: Type::String,
                    optional: false,
                },
                ir::RecordField {
                    name: VALUE.into(),
                    ty: value,
                    optional: true,
                },
                ir::RecordField {
                    name: REASON.into(),
                    ty: Type::Unknown,
                    optional: true,
                },
            ],
            self.types,
        )
    }
    /// The value type of the combinator's result promise.
    fn combinator_result(&mut self, combinator: Combinator, id: u32) -> Result<u32> {
        let value = self.types.promises[id as usize].value;
        let result = match combinator {
            Combinator::All => arrays::intern(stored(value), self.types)?,
            Combinator::AllSettled => {
                let element = self.settled_record(value)?;
                arrays::intern(element, self.types)?
            }
            Combinator::Race | Combinator::Any => return Ok(id),
        };
        match intern(result, self.types)? {
            Type::Promise(result) => Ok(result),
            _ => Err(CompilerError::Internal("missing promise".into())),
        }
    }
    fn combinator_state(&mut self, id: u32, result: u32) -> Result<Type> {
        let items = arrays::intern(Type::Promise(id), self.types)?;
        let field = |name: &str, ty: Type| ir::RecordField {
            name: name.into(),
            ty,
            optional: false,
        };
        records::intern(
            vec![
                field(ITEMS, items),
                field(REMAINING, Type::Float64),
                field(RESULT, Type::Promise(result)),
            ],
            self.types,
        )
    }
    fn state_field(&self, state: Expr, name: &str) -> Result<Expr> {
        let Type::Record(shape) = state.ty else {
            return Err(CompilerError::Internal(
                "combinator state is not a record".into(),
            ));
        };
        let fields = &self.types.records[shape as usize].fields;
        let slot = fields
            .iter()
            .position(|field| field.name == name)
            .ok_or_else(|| CompilerError::Internal("missing combinator state field".into()))?;
        Ok(Expr {
            span: state.span,
            ty: fields[slot].ty,
            kind: E::Field(Box::new(state), slot as u32),
        })
    }
    fn set_state(&self, state: Expr, name: &str, value: Expr) -> Result<S> {
        let Type::Record(shape) = state.ty else {
            return Err(CompilerError::Internal(
                "combinator state is not a record".into(),
            ));
        };
        let slot = self.types.records[shape as usize]
            .fields
            .iter()
            .position(|field| field.name == name)
            .ok_or_else(|| CompilerError::Internal("missing combinator state field".into()))?;
        Ok(S::SetField(state, slot as u32, value))
    }
    /// `Promise.all(items)` and the other combinators.
    pub(super) fn promise_combinator(
        &mut self,
        call: &ast::CallExpr,
        combinator: Combinator,
        expected: Option<Type>,
    ) -> Result<Expr> {
        let span = call.span;
        let what = format!("Promise.{}", combinator.name());
        if !self.types.profile.promise_values() {
            return Err(rejected(&what));
        }
        let [argument] = call.args.as_slice() else {
            return Err(self
                .source
                .locate(rejected(&format!("{what} arguments")), span));
        };
        if argument.spread.is_some() || call.type_args.is_some() {
            return Err(self
                .source
                .locate(rejected(&format!("{what} arguments")), span));
        }
        let items = match self.race_items(combinator, &argument.expr, expected, span)? {
            RaceItems::Mixed(result) => return Ok(result),
            RaceItems::Array(items) => items,
            RaceItems::Ordinary => self.expr(&argument.expr)?,
        };
        let id = match items.ty {
            Type::Array(array) => match self.types.arrays[array as usize] {
                Type::Promise(id) => id,
                other => {
                    return Err(self.source.locate(
                        type_error(format!(
                            "{what} takes an array of promises of one type, not of {other}"
                        )),
                        items.span,
                    ));
                }
            },
            other => {
                return Err(self.source.locate(
                    type_error(format!("{what} takes an array of promises, not {other}")),
                    items.span,
                ));
            }
        };
        let array = items.ty;
        let result = self.combinator_result(combinator, id)?;
        let result_ty = Type::Promise(result);
        let state_ty = self.combinator_state(id, result)?;
        let job = self.combinator_job(span, combinator, id, result, state_ty)?;
        let number = self.types.number();
        let source = self.hidden(array);
        let snapshot = self.hidden(array);
        let count = self.hidden(number);
        let state = self.hidden(state_ty);
        let index = self.hidden(number);
        let output = self.hidden(result_ty);
        let length = |slot| Expr {
            span,
            ty: number,
            kind: E::Length(Box::new(local(span, slot, array))),
        };
        let Type::Record(shape) = state_ty else {
            return Err(CompilerError::Internal(
                "combinator state is not a record".into(),
            ));
        };
        let mut fields = Vec::new();
        for (slot, field) in self.types.records[shape as usize]
            .fields
            .clone()
            .iter()
            .enumerate()
        {
            let value = match field.name.as_str() {
                ITEMS => local(span, snapshot, array),
                REMAINING => local(span, count, number),
                _ => self.promise_object(span, result, None)?,
            };
            fields.push((slot as u32, value));
        }
        let empty =
            self.combinator_empty(span, combinator, result, local(span, output, result_ty))?;
        let body = vec![
            S::Init(source, items),
            // The array is read once, at the call, as by the iteration of the
            // combinator; later writes to it are not observed.
            S::Init(
                snapshot,
                Expr {
                    span,
                    ty: array,
                    kind: E::ArraySlice(
                        Box::new(local(span, source, array)),
                        Box::new(float(span, 0.0)),
                        Box::new(length(source)),
                    ),
                },
            ),
            S::Init(count, length(snapshot)),
            S::Init(
                state,
                Expr {
                    span,
                    ty: state_ty,
                    kind: E::Record(fields),
                },
            ),
            S::Init(
                output,
                self.state_field(local(span, state, state_ty), RESULT)?,
            ),
            S::For(
                vec![S::Init(index, float(span, 0.0))],
                Some(compare(
                    span,
                    ast::BinaryOp::Lt,
                    local(span, index, number),
                    local(span, count, number),
                )),
                vec![S::Set(
                    index,
                    Expr {
                        span,
                        ty: number,
                        kind: E::Binary(
                            ast::BinaryOp::Add,
                            Box::new(local(span, index, number)),
                            Box::new(float(span, 1.0)),
                        ),
                    },
                )],
                vec![schedule(
                    span,
                    job,
                    vec![local(span, state, state_ty), local(span, index, number)],
                    ScheduleMode::Start,
                )],
            ),
            S::If(
                compare(
                    span,
                    ast::BinaryOp::EqEqEq,
                    local(span, count, number),
                    float(span, 0.0),
                ),
                empty,
                vec![],
            ),
        ];
        Ok(Expr {
            span,
            ty: result_ty,
            kind: E::ValueBlock {
                output,
                marker: self.hidden(Type::Number),
                roots: vec![source, snapshot, state, output],
                body,
            },
        })
    }
    /// `Promise.race([p0, p1, …])` over an array literal (v38 `web_bytes`):
    /// the elements are lowered one by one, so promises of different value
    /// types race into `Promise<T0 | T1 | …>` without forming an array of a
    /// union of promise types. Elements of one type form the ordinary array.
    fn race_items(
        &mut self,
        combinator: Combinator,
        argument: &ast::Expr,
        expected: Option<Type>,
        span: swc_common::Span,
    ) -> Result<RaceItems> {
        let ast::Expr::Array(array) = stmt::unparen(argument) else {
            return Ok(RaceItems::Ordinary);
        };
        if combinator != Combinator::Race
            || !self.types.profile.web_bytes()
            || array.elems.is_empty()
            || array
                .elems
                .iter()
                .any(|element| element.as_ref().is_none_or(|e| e.spread.is_some()))
        {
            return Ok(RaceItems::Ordinary);
        }
        let mut elements = Vec::new();
        for element in array.elems.iter().flatten() {
            let value = self.expr(&element.expr)?;
            if !matches!(value.ty, Type::Promise(_)) {
                return Err(self.source.locate(
                    type_error(format!(
                        "Promise.race takes an array of promises, not of {}",
                        value.ty
                    )),
                    value.span,
                ));
            }
            elements.push(value);
        }
        let first = elements[0].ty;
        if elements.iter().all(|element| element.ty == first) {
            let ty = arrays::intern(first, self.types)?;
            return Ok(RaceItems::Array(Expr {
                span: array.span,
                ty,
                kind: E::Array(elements.into_iter().map(Some).collect()),
            }));
        }
        let mut values = Vec::new();
        for element in &elements {
            let Type::Promise(id) = element.ty else {
                return Err(CompilerError::Internal(
                    "missing race element promise".into(),
                ));
            };
            values.push(stored(self.types.promises[id as usize].value));
        }
        // The contextual promise type, when every element's value is one of
        // its members (TypeScript infers the race from its context).
        // A value whose primitive base a literal member of the context has
        // (a `Promise<'timeout'>` fulfills with `string`, since promise values
        // erase literal types) is accepted and checked when it settles.
        let contextual = expected.and_then(|ty| match ty {
            Type::Promise(id) => {
                let members =
                    unions::members(stored(self.types.promises[id as usize].value), self.types);
                values
                    .iter()
                    .flat_map(|value| unions::members(*value, self.types))
                    .all(|member| {
                        literals::member_of(member, &members, self.types)
                            || members.iter().any(
                                |m| matches!(m, Type::Literal(base, _) if base.erased() == member),
                            )
                    })
                    .then_some(id)
            }
            _ => None,
        });
        let result = match contextual {
            Some(id) => id,
            None => {
                let value = unions::intern(values, self.types)?;
                let Type::Promise(result) = intern(value, self.types)? else {
                    return Err(CompilerError::Internal(
                        "missing race result promise".into(),
                    ));
                };
                result
            }
        };
        let result_ty = Type::Promise(result);
        let output = self.hidden(result_ty);
        let mut roots = vec![output];
        let mut body = Vec::new();
        let mut starts = Vec::new();
        for element in elements {
            let Type::Promise(id) = element.ty else {
                return Err(CompilerError::Internal(
                    "missing race element promise".into(),
                ));
            };
            let slot = self.hidden(element.ty);
            roots.push(slot);
            body.push(S::Init(slot, element));
            let job = self.race_job(span, id, result)?;
            starts.push(schedule(
                span,
                job,
                vec![
                    local(span, slot, Type::Promise(id)),
                    local(span, output, result_ty),
                ],
                ScheduleMode::Start,
            ));
        }
        body.push(S::Init(output, self.promise_object(span, result, None)?));
        // One job per element, in index order, as `race` calls each
        // element's `then`.
        body.extend(starts);
        Ok(RaceItems::Mixed(Expr {
            span,
            ty: result_ty,
            kind: E::ValueBlock {
                output,
                marker: self.hidden(Type::Number),
                roots,
                body,
            },
        }))
    }
    /// The job of one element of a mixed race: `(element, result) -> void`
    /// awaits the element and settles the result with its value widened to
    /// the result's union (a settled result ignores later elements).
    fn race_job(&mut self, span: swc_common::Span, id: u32, result: u32) -> Result<usize> {
        self.memoized(
            Helper::Combinator(Combinator::Race),
            vec![Type::Promise(id), Type::Promise(result)],
            |lower| {
                lower.synthesize(
                    Self::plain(span, vec![Type::Promise(id), Type::Promise(result)], true),
                    |child, params, _| child.race_job_body(span, (id, result), params),
                )
            },
        )
    }
    fn race_job_body(
        &mut self,
        span: swc_common::Span,
        (id, result): (u32, u32),
        params: &[u32],
    ) -> Result<Vec<S>> {
        let [element, target] = params else {
            return Err(CompilerError::Internal("race job parameters".into()));
        };
        let value = stored(self.types.promises[id as usize].value);
        let union = stored(self.types.promises[result as usize].value);
        let reason = self.exception_type()?;
        let e = self.hidden(reason);
        let marker = self.hidden(Type::Number);
        let fulfilled = self.hidden(value);
        let awaited = self.await_promise(span, local(span, *element, Type::Promise(id)))?;
        let fulfill = self.fulfill_helper(span, result)?;
        let reject = self.reject_helper(span, result)?;
        let members = unions::members(union, self.types);
        let exact = unions::members(value, self.types)
            .into_iter()
            .all(|member| literals::member_of(member, &members, self.types));
        let widened = if exact {
            unions::widen(local(span, fulfilled, value), Some(union), self.types)?
        } else {
            // A primitive standing for a literal member of the contextual
            // union: a checked assertion (`TypeError` outside the literals).
            let name = subtyping::describe(union, self.types);
            let message = format!("Promise.race element fulfilled with a value outside {name}");
            self.assert_value(local(span, fulfilled, value), union, &name, &message, span)?
        };
        Ok(vec![
            S::Init(fulfilled, zero(span, value)),
            S::TryCatch {
                body: vec![
                    S::Set(fulfilled, awaited),
                    invoke(
                        span,
                        fulfill,
                        vec![local(span, *target, Type::Promise(result)), widened],
                    ),
                ],
                binding: Some(e),
                handler: vec![invoke(
                    span,
                    reject,
                    vec![
                        local(span, *target, Type::Promise(result)),
                        local(span, e, reason),
                    ],
                )],
                marker,
            },
        ])
    }
    /// The settlement of a combinator over an empty array: `all` and
    /// `allSettled` fulfill with `[]` and `any` rejects at once; `race` stays
    /// pending forever.
    fn combinator_empty(
        &mut self,
        span: swc_common::Span,
        combinator: Combinator,
        result: u32,
        promise: Expr,
    ) -> Result<Vec<S>> {
        Ok(match combinator {
            Combinator::All | Combinator::AllSettled => {
                let value = stored(self.types.promises[result as usize].value);
                let fulfill = self.fulfill_helper(span, result)?;
                vec![invoke(
                    span,
                    fulfill,
                    vec![
                        promise,
                        Expr {
                            span,
                            ty: value,
                            kind: E::Array(vec![]),
                        },
                    ],
                )]
            }
            Combinator::Any => {
                let reject = self.reject_helper(span, result)?;
                let error = self.aggregate_error(span)?;
                vec![invoke(span, reject, vec![promise, error])]
            }
            Combinator::Race => vec![],
        })
    }
    /// The `Promise.any` rejection reason, widened to the exception type.
    fn aggregate_error(&mut self, span: swc_common::Span) -> Result<Expr> {
        let error = self.array_error(ErrorClass::Error, AGGREGATE_MESSAGE, span)?;
        let reason = self.exception_type()?;
        unions::widen(error, Some(reason), self.types)
    }
    /// The job frame of one element: `(state, index) -> void`.
    fn combinator_job(
        &mut self,
        span: swc_common::Span,
        combinator: Combinator,
        id: u32,
        result: u32,
        state_ty: Type,
    ) -> Result<usize> {
        let number = self.types.number();
        self.memoized(
            Helper::Combinator(combinator),
            vec![Type::Promise(id)],
            |lower| {
                lower.synthesize(
                    Self::plain(span, vec![state_ty, number], true),
                    |child, params, _| {
                        child.combinator_job_body(
                            span,
                            combinator,
                            (id, result),
                            local(span, params[0], state_ty),
                            local(span, params[1], number),
                        )
                    },
                )
            },
        )
    }
    fn combinator_job_body(
        &mut self,
        span: swc_common::Span,
        combinator: Combinator,
        (id, result): (u32, u32),
        state: Expr,
        index: Expr,
    ) -> Result<Vec<S>> {
        let state_slot = match state.kind {
            E::Local(slot) => slot,
            _ => return Err(CompilerError::Internal("combinator state parameter".into())),
        };
        let state_ty = state.ty;
        let holder = || local(span, state_slot, state_ty);
        let target = |lower: &Self| lower.state_field(holder(), RESULT);
        let promise_ty = Type::Promise(id);
        let items = self.state_field(holder(), ITEMS)?;
        let element = self.hidden(promise_ty);
        let value = stored(self.types.promises[id as usize].value);
        let reason = self.exception_type()?;
        let e = self.hidden(reason);
        let marker = self.hidden(Type::Number);
        let fulfilled = self.hidden(value);
        let awaited = self.await_promise(span, local(span, element, promise_ty))?;
        let remaining = self.state_field(holder(), REMAINING)?;
        let decrement = self.set_state(
            holder(),
            REMAINING,
            Expr {
                span,
                ty: Type::Float64,
                kind: E::Binary(
                    ast::BinaryOp::Sub,
                    Box::new(self.state_field(holder(), REMAINING)?),
                    Box::new(float(span, 1.0)),
                ),
            },
        )?;
        let last = compare(span, ast::BinaryOp::EqEqEq, remaining, float(span, 0.0));
        let mut body = vec![
            S::Init(
                element,
                Expr {
                    span,
                    ty: promise_ty,
                    kind: E::Index(Box::new(items), Box::new(index)),
                },
            ),
            S::Init(fulfilled, zero(span, value)),
        ];
        let fulfill = self.fulfill_helper(span, result)?;
        let reject = self.reject_helper(span, result)?;
        body.push(match combinator {
            Combinator::All => {
                let gathered = self.gather(span, combinator, id, result, state_slot, state_ty)?;
                S::Block(vec![
                    S::TryCatch {
                        body: vec![S::Set(fulfilled, awaited)],
                        binding: Some(e),
                        handler: vec![
                            invoke(span, reject, vec![target(self)?, local(span, e, reason)]),
                            S::Return(None),
                        ],
                        marker,
                    },
                    decrement,
                    S::If(last, gathered, vec![]),
                ])
            }
            Combinator::AllSettled => {
                let gathered = self.gather(span, combinator, id, result, state_slot, state_ty)?;
                S::Block(vec![
                    S::TryCatch {
                        body: vec![S::Set(fulfilled, awaited)],
                        binding: Some(e),
                        handler: vec![],
                        marker,
                    },
                    decrement,
                    S::If(last, gathered, vec![]),
                ])
            }
            Combinator::Race => S::TryCatch {
                body: vec![
                    S::Set(fulfilled, awaited),
                    invoke(
                        span,
                        fulfill,
                        vec![target(self)?, local(span, fulfilled, value)],
                    ),
                ],
                binding: Some(e),
                handler: vec![invoke(
                    span,
                    reject,
                    vec![target(self)?, local(span, e, reason)],
                )],
                marker,
            },
            Combinator::Any => {
                let error = self.aggregate_error(span)?;
                S::TryCatch {
                    body: vec![
                        S::Set(fulfilled, awaited),
                        invoke(
                            span,
                            fulfill,
                            vec![target(self)?, local(span, fulfilled, value)],
                        ),
                    ],
                    binding: Some(e),
                    handler: vec![
                        decrement,
                        S::If(
                            last,
                            vec![invoke(span, reject, vec![target(self)?, error])],
                            vec![],
                        ),
                    ],
                    marker,
                }
            }
        });
        Ok(body)
    }
    /// Build the `all`/`allSettled` result from every element (all settled
    /// by now) and fulfill the result promise, unless it already settled.
    fn gather(
        &mut self,
        span: swc_common::Span,
        combinator: Combinator,
        id: u32,
        result: u32,
        state_slot: u32,
        state_ty: Type,
    ) -> Result<Vec<S>> {
        let record = || local(span, state_slot, state_ty);
        let number = self.types.number();
        let array = stored(self.types.promises[result as usize].value);
        let Type::Array(array_id) = array else {
            return Err(CompilerError::Internal(
                "combinator result is not an array".into(),
            ));
        };
        let element_ty = self.types.arrays[array_id as usize];
        let promise_ty = Type::Promise(id);
        let values = self.hidden(array);
        let index = self.hidden(number);
        let item = self.hidden(promise_ty);
        let items = self.state_field(record(), ITEMS)?;
        let count = Expr {
            span,
            ty: number,
            kind: E::Length(Box::new(self.state_field(record(), ITEMS)?)),
        };
        let read = Expr {
            span,
            ty: promise_ty,
            kind: E::Index(Box::new(items), Box::new(local(span, index, number))),
        };
        let entry = match combinator {
            Combinator::AllSettled => {
                let Type::Record(shape) = element_ty else {
                    return Err(CompilerError::Internal(
                        "settled element is not a record".into(),
                    ));
                };
                let value_ty = self.types.records[shape as usize]
                    .fields
                    .iter()
                    .find(|field| field.name == VALUE)
                    .map(|field| field.ty)
                    .ok_or_else(|| CompilerError::Internal("missing settled value".into()))?;
                let status = |text: &str, lower: &Self| Expr {
                    span,
                    ty: Type::String,
                    kind: E::String(lower.types.profile.string_bytes(text)),
                };
                let value = unions::widen(
                    self.read_promise(local(span, item, promise_ty), VALUE)?,
                    Some(value_ty),
                    self.types,
                )?;
                let fulfilled = self.optional_record(
                    span,
                    shape,
                    vec![
                        (STATUS.into(), status(FULFILLED, self)),
                        (VALUE.into(), value),
                    ],
                )?;
                let reason = unknown::widen(
                    self.read_promise(local(span, item, promise_ty), REASON)?,
                    self.types,
                )?;
                let rejected = self.optional_record(
                    span,
                    shape,
                    vec![
                        (STATUS.into(), status(REJECTED, self)),
                        (REASON.into(), reason),
                    ],
                )?;
                Expr {
                    span,
                    ty: element_ty,
                    kind: E::Conditional(
                        Box::new(compare(
                            span,
                            ast::BinaryOp::EqEqEq,
                            self.read_promise(local(span, item, promise_ty), STATE)?,
                            state(span, PromiseState::Fulfilled),
                        )),
                        Box::new(fulfilled),
                        Box::new(rejected),
                    ),
                }
            }
            _ => self.read_promise(local(span, item, promise_ty), VALUE)?,
        };
        let fulfill = self.fulfill_helper(span, result)?;
        let target = self.state_field(record(), RESULT)?;
        let pending = compare(
            span,
            ast::BinaryOp::EqEqEq,
            self.read_promise(self.state_field(record(), RESULT)?, STATE)?,
            state(span, PromiseState::Pending),
        );
        Ok(vec![S::If(
            pending,
            vec![
                S::Init(
                    values,
                    Expr {
                        span,
                        ty: array,
                        kind: E::Array(vec![]),
                    },
                ),
                S::For(
                    vec![S::Init(index, float(span, 0.0))],
                    Some(compare(
                        span,
                        ast::BinaryOp::Lt,
                        local(span, index, number),
                        count,
                    )),
                    vec![S::Set(
                        index,
                        Expr {
                            span,
                            ty: number,
                            kind: E::Binary(
                                ast::BinaryOp::Add,
                                Box::new(local(span, index, number)),
                                Box::new(float(span, 1.0)),
                            ),
                        },
                    )],
                    vec![
                        S::Init(item, read),
                        S::Expr(Expr {
                            span,
                            ty: Type::Float64,
                            kind: E::ArrayPush(
                                Box::new(local(span, values, array)),
                                Box::new(Expr {
                                    span,
                                    ty: array,
                                    kind: E::Array(vec![Some(entry)]),
                                }),
                            ),
                        }),
                    ],
                ),
                invoke(span, fulfill, vec![target, local(span, values, array)]),
            ],
            vec![],
        )])
    }
}
