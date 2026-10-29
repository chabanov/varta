//! Resumable state machine over normalized IR. Suspension points are numbered in
//! statement order, so every structured statement covers one contiguous state
//! range. On resumption all saved locals are restored, the structured code is
//! re-entered through range guards (re-establishing handler, label and root
//! bookkeeping) and every statement before the target state is skipped.
use super::*;
use varta_common::function::frame::{self, Completion};

pub(super) struct Machine<'a> {
    pub locals: &'a mut Vec<Type>,
    pub types: &'a ir::ValueTypes,
    pub source: &'a Source,
    pub exception: Type,
    pub result: Type,
}

fn integer(span: swc_common::Span, value: i32) -> Expr {
    Expr {
        span,
        ty: Type::Number,
        kind: E::Integer(value),
    }
}
fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn boolean(span: swc_common::Span, op: BinaryOp, a: Expr, b: Expr) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Binary(op, Box::new(a), Box::new(b)),
    }
}
/// Admissible scalar resume inputs; host references need a validated transport.
pub(super) fn input_class(ty: Type) -> Option<frame::FrameInput> {
    Some(match ty {
        Type::Float64 => frame::FrameInput::Float64,
        Type::Boolean => frame::FrameInput::Boolean,
        Type::Undefined | Type::Void => frame::FrameInput::Undefined,
        _ => return None,
    })
}

struct Guards {
    span: swc_common::Span,
    state: u32,
    completion: u32,
}
impl Guards {
    fn state(&self) -> Expr {
        local(self.span, self.state, Type::Number)
    }
    fn is(&self, state: u32) -> Expr {
        boolean(
            self.span,
            BinaryOp::EqEqEq,
            self.state(),
            integer(self.span, state as i32),
        )
    }
    fn running(&self) -> Expr {
        self.is(frame::INITIAL_STATE)
    }
    fn completion(&self, kind: Completion) -> Expr {
        boolean(
            self.span,
            BinaryOp::EqEqEq,
            local(self.span, self.completion, Type::Number),
            integer(self.span, kind as i32),
        )
    }
    fn within(&self, (low, high): (u32, u32)) -> Expr {
        let span = self.span;
        boolean(
            span,
            BinaryOp::LogicalAnd,
            boolean(
                span,
                BinaryOp::GtEq,
                self.state(),
                integer(span, low as i32),
            ),
            boolean(
                span,
                BinaryOp::LtEq,
                self.state(),
                integer(span, high as i32),
            ),
        )
    }
    /// Enter normally, or when resuming to a state inside `range`.
    fn enter(&self, range: (u32, u32)) -> Expr {
        boolean(
            self.span,
            BinaryOp::LogicalOr,
            self.running(),
            self.within(range),
        )
    }
}

fn range(body: &[S]) -> Option<(u32, u32)> {
    let mut range: Option<(u32, u32)> = None;
    walk::statements(body, &mut |e| {
        if let E::Suspend { state, .. } = e.kind {
            range = Some(range.map_or((state, state), |(low, high)| {
                (low.min(state), high.max(state))
            }));
        }
    });
    range
}

impl Machine<'_> {
    fn fresh(&mut self, ty: Type) -> u32 {
        let slot = self.locals.len() as u32;
        self.locals.push(ty);
        slot
    }
    pub(super) fn build(
        mut self,
        span: swc_common::Span,
        mut body: Vec<S>,
    ) -> Result<(Vec<S>, ir::Suspendable)> {
        let mut points = Vec::new();
        self.number(&mut body, &mut points)?;
        if points.len() > frame::MAX_SUSPENSION_STATES {
            return Err(self.source.locate(
                invalid("suspendable function exceeds the continuation state budget"),
                span,
            ));
        }
        let guards = Guards {
            span,
            state: self.fresh(Type::Number),
            completion: self.fresh(Type::Number),
        };
        let return_input = match self.result {
            Type::Float64 | Type::Boolean | Type::Undefined => Some(self.result),
            Type::Void => Some(Type::Undefined),
            _ => None,
        };
        let mut inputs = std::collections::BTreeMap::new();
        for ty in points
            .iter()
            .map(|point: &ir::SuspensionPoint| point.input)
            .chain(return_input)
            .chain([Type::Float64])
        {
            if let std::collections::btree_map::Entry::Vacant(entry) = inputs.entry(ty) {
                entry.insert(self.fresh(ty));
            }
        }
        let machine = ir::Suspendable {
            state: guards.state,
            completion: guards.completion,
            inputs,
            points,
            exception: self.exception,
            return_input,
        };
        // A frame that never started completes without entering user code.
        let mut result = vec![S::If(
            boolean(
                span,
                BinaryOp::LogicalAnd,
                guards.running(),
                guards.completion(Completion::Throw),
            ),
            vec![self.throw_delivery(&machine, span)?],
            vec![],
        )];
        if let Some(delivery) = self.return_delivery(&machine, span) {
            result.push(S::If(
                boolean(
                    span,
                    BinaryOp::LogicalAnd,
                    guards.running(),
                    guards.completion(Completion::Return),
                ),
                vec![delivery],
                vec![],
            ));
        }
        result.extend(self.list(body, &guards, &machine)?);
        Ok((result, machine))
    }
    /// Assign states in statement order and check the canonical normalized form.
    fn number(&self, body: &mut [S], points: &mut Vec<ir::SuspensionPoint>) -> Result<()> {
        for statement in body {
            match statement {
                S::Init(_, value) if matches!(value.kind, E::Suspend { .. }) => {
                    let input = value.ty;
                    let span = value.span;
                    let E::Suspend { value, state } = &mut value.kind else {
                        return Err(CompilerError::Internal("missing suspension point".into()));
                    };
                    if input_class(input).is_none() || input == Type::Void {
                        return Err(self
                            .source
                            .locate(unsupported_suspension(format!("input type {input}")), span));
                    }
                    if value.ty.width() != 1 {
                        return Err(self.source.locate(
                            unsupported_suspension(format!("of a {} value", value.ty)),
                            value.span,
                        ));
                    }
                    points.push(ir::SuspensionPoint {
                        input,
                        yielded: value.ty,
                    });
                    *state = points.len() as u32;
                }
                S::If(_, yes, no) => {
                    self.number(yes, points)?;
                    self.number(no, points)?;
                }
                S::For(init, _, update, body) => {
                    self.number(init, points)?;
                    self.number(body, points)?;
                    self.number(update, points)?;
                }
                S::While(_, body)
                | S::DoWhile(_, body)
                | S::Block(body)
                | S::Scope(_, body)
                | S::Label { body, .. }
                | S::TryCatch { body, .. }
                | S::TryFinally { body, .. } => self.number(body, points)?,
                _ => {}
            }
        }
        Ok(())
    }
    fn throw_delivery(&self, machine: &ir::Suspendable, span: swc_common::Span) -> Result<S> {
        let value = local(span, machine.inputs[&Type::Float64], Type::Float64);
        Ok(S::Throw(unions::widen(
            value,
            Some(self.exception),
            self.types,
        )?))
    }
    fn return_delivery(&self, machine: &ir::Suspendable, span: swc_common::Span) -> Option<S> {
        let input = machine.return_input?;
        Some(S::Return(if self.result == Type::Void {
            None
        } else {
            Some(local(span, machine.inputs[&input], input))
        }))
    }
    fn list(&mut self, body: Vec<S>, guards: &Guards, machine: &ir::Suspendable) -> Result<Vec<S>> {
        let mut result = Vec::new();
        let mut run = Vec::new();
        let flush = |run: &mut Vec<S>, result: &mut Vec<S>| {
            if !run.is_empty() {
                result.push(S::If(guards.running(), std::mem::take(run), vec![]));
            }
        };
        for statement in body {
            match range(std::slice::from_ref(&statement)) {
                None => run.push(statement),
                Some(states) => {
                    flush(&mut run, &mut result);
                    result.extend(self.compound(statement, states, guards, machine)?);
                }
            }
        }
        flush(&mut run, &mut result);
        Ok(result)
    }
    fn compound(
        &mut self,
        statement: S,
        states: (u32, u32),
        guards: &Guards,
        machine: &ir::Suspendable,
    ) -> Result<Vec<S>> {
        let span = guards.span;
        let enter = |body: Vec<S>| vec![S::If(guards.enter(states), body, vec![])];
        Ok(match statement {
            S::Init(target, value) => {
                let E::Suspend { value, state } = value.kind else {
                    return Err(CompilerError::Internal("noncanonical suspension".into()));
                };
                let ty = machine.points[state as usize - 1].input;
                let mut resumed = vec![
                    S::Set(guards.state, integer(span, frame::INITIAL_STATE as i32)),
                    S::If(
                        guards.completion(Completion::Throw),
                        vec![self.throw_delivery(machine, span)?],
                        vec![],
                    ),
                ];
                if let Some(delivery) = self.return_delivery(machine, span) {
                    resumed.push(S::If(
                        guards.completion(Completion::Return),
                        vec![delivery],
                        vec![],
                    ));
                }
                resumed.push(S::Init(target, local(span, machine.inputs[&ty], ty)));
                vec![
                    S::If(
                        guards.running(),
                        vec![S::Expr(Expr {
                            span: value.span,
                            ty: Type::Void,
                            kind: E::Suspend { value, state },
                        })],
                        vec![],
                    ),
                    S::If(guards.is(state), resumed, vec![]),
                ]
            }
            S::If(test, yes, no) => {
                let selected = self.fresh(Type::Boolean);
                let resume = range(&yes).map_or_else(
                    || Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Integer(0),
                    },
                    |states| guards.within(states),
                );
                let flag = |value: bool| {
                    S::Set(
                        selected,
                        Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::Integer(i32::from(value)),
                        },
                    )
                };
                let yes = self.list(yes, guards, machine)?;
                let no = self.list(no, guards, machine)?;
                enter(vec![
                    S::If(
                        guards.running(),
                        vec![S::If(test, vec![flag(true)], vec![flag(false)])],
                        vec![S::Set(selected, resume)],
                    ),
                    S::If(local(span, selected, Type::Boolean), yes, no),
                ])
            }
            S::While(test, body) => {
                let mut entry = Vec::new();
                if !matches!(test.kind, E::Integer(1)) {
                    entry.push(S::If(
                        guards.running(),
                        vec![S::If(test, vec![], vec![S::Break])],
                        vec![],
                    ));
                }
                entry.extend(self.list(body, guards, machine)?);
                let truth = Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Integer(1),
                };
                enter(vec![S::While(truth, entry)])
            }
            S::DoWhile(test, body) => {
                enter(vec![S::DoWhile(test, self.list(body, guards, machine)?)])
            }
            S::For(init, test, update, body) => {
                let mut prefix = self.list(init, guards, machine)?;
                let mut entry = Vec::new();
                if let Some(test) = test {
                    entry.push(S::If(
                        guards.running(),
                        vec![S::If(test, vec![], vec![S::Break])],
                        vec![],
                    ));
                }
                entry.extend(self.list(body, guards, machine)?);
                let update = self.list(update, guards, machine)?;
                prefix.push(S::For(vec![], None, update, entry));
                enter(prefix)
            }
            S::Block(body) => enter(vec![S::Block(self.list(body, guards, machine)?)]),
            S::Scope(slots, body) => {
                enter(vec![S::Scope(slots, self.list(body, guards, machine)?)])
            }
            S::Label {
                id,
                iteration,
                body,
            } => enter(vec![S::Label {
                id,
                iteration,
                body: self.list(body, guards, machine)?,
            }]),
            S::TryCatch {
                body,
                binding,
                handler,
                marker,
            } => enter(vec![S::TryCatch {
                body: self.list(body, guards, machine)?,
                binding,
                handler,
                marker,
            }]),
            S::TryFinally {
                body,
                finalizer,
                marker,
                completion,
                exception,
                result,
            } => enter(vec![S::TryFinally {
                body: self.list(body, guards, machine)?,
                finalizer,
                marker,
                completion,
                exception,
                result,
            }]),
            _ => {
                return Err(CompilerError::Internal(
                    "suspension point outside a resumable statement".into(),
                ));
            }
        })
    }
}
