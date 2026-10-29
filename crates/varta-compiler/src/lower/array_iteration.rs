//! Array traversal through rooted locals, snapshot length and live presence.
use super::array_callbacks::{Iteration, Method};
use super::*;
fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn number(span: swc_common::Span, value: f64) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Float64(value),
    }
}
fn binary(span: swc_common::Span, op: ast::BinaryOp, a: Expr, b: Expr, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Binary(op, Box::new(a), Box::new(b)),
    }
}
fn length(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Length(Box::new(local(span, slot, ty))),
    }
}
fn undefined(span: swc_common::Span) -> Expr {
    Expr {
        span,
        ty: Type::Undefined,
        kind: E::Integer(0),
    }
}
impl Lower<'_> {
    fn array_predicate(&mut self, value: Expr) -> Result<Expr> {
        let value = if value.ty == Type::Void {
            Expr {
                span: value.span,
                ty: Type::Undefined,
                kind: E::Undefined(Box::new(value)),
            }
        } else {
            value
        };
        if matches!(value.ty, Type::Record(_) | Type::Class(_) | Type::Array(_)) {
            return Ok(Expr {
                span: value.span,
                ty: Type::Boolean,
                kind: E::Sequence(
                    Box::new(value),
                    Box::new(Expr {
                        span: swc_common::DUMMY_SP,
                        ty: Type::Boolean,
                        kind: E::Integer(1),
                    }),
                ),
            });
        }
        let value = self.truth(value)?;
        if !matches!(value.ty, Type::Null | Type::Undefined) {
            scalar(value.ty)?;
        }
        Ok(value)
    }
    pub(super) fn array_iteration(
        &mut self,
        iteration: &Iteration,
        arity: usize,
        callback_result: Type,
    ) -> Result<Vec<S>> {
        let Iteration {
            method,
            array,
            item,
            callback,
            span,
            initial,
            this_arg,
        } = *iteration;
        let source = self.hidden(array);
        let callable = self.hidden(callback);
        let this_slot = this_arg.map(|ty| self.hidden(ty));
        // A supplied seed is the third wrapper parameter; otherwise this local
        // is initialized from the first present slot before any read.
        let accumulator = method.reduces().then(|| self.hidden(self.result));
        let ready = (method.reduces() && !initial).then(|| self.hidden(Type::Boolean));
        let limit = self.hidden(Type::Float64);
        let index = self.hidden(Type::Float64);
        let current = self.hidden(item);
        let output =
            matches!(method, Method::Map | Method::Filter).then(|| self.hidden(self.result));
        let checked = self.hidden(callback);
        let mut body = vec![
            S::Expr(Expr {
                span,
                ty: Type::Void,
                kind: E::FunctionCheck {
                    value: Box::new(local(span, callable, callback)),
                    slot: checked,
                },
            }),
            S::Init(limit, length(span, source, array)),
        ];
        if let Some(ready) = ready {
            body.push(S::Init(
                ready,
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Integer(0),
                },
            ));
        }
        if let Some(output) = output {
            let kind = if method == Method::Map && self.types.profile.sparse_arrays() {
                E::ArrayReserve {
                    length: Box::new(local(span, limit, Type::Float64)),
                    count: self.hidden(Type::Number),
                    bytes: self.hidden(Type::Number),
                }
            } else {
                E::Array(vec![])
            };
            body.push(S::Init(
                output,
                Expr {
                    span,
                    ty: self.result,
                    kind,
                },
            ));
        }
        let read = Expr {
            span,
            ty: item,
            kind: if method.visits_holes() {
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
        };
        let mut args = vec![
            local(span, current, item),
            local(span, index, Type::Float64),
            local(span, source, array),
        ];
        if let Some(accumulator) = accumulator {
            args.insert(0, local(span, accumulator, self.result));
        }
        let Type::Function(id) = callback else {
            return Err(invalid("missing callback type"));
        };
        let args = if self.types.functions[id as usize].shape.rest {
            args
        } else {
            args.into_iter().take(arity).collect()
        };
        let receiver = this_slot
            .zip(this_arg)
            .map(|(slot, ty)| local(span, slot, ty));
        let invocation =
            self.invoke_receiver(local(span, callable, callback), receiver, args, span)?;
        let mut step = vec![S::Init(current, read)];
        match method {
            Method::ForEach => step.push(S::Expr(invocation)),
            Method::Find
            | Method::FindIndex
            | Method::FindLast
            | Method::FindLastIndex
            | Method::Some
            | Method::Every => {
                let test = self.array_predicate(invocation)?;
                step.push(S::If(
                    test,
                    if method == Method::Every {
                        vec![]
                    } else {
                        vec![S::Return(Some(if method.index_result() {
                            local(span, index, Type::Float64)
                        } else if method.visits_holes() {
                            local(span, current, item)
                        } else {
                            Expr {
                                span,
                                ty: Type::Boolean,
                                kind: E::Integer(1),
                            }
                        }))]
                    },
                    if method == Method::Every {
                        vec![S::Return(Some(Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::Integer(0),
                        }))]
                    } else {
                        vec![]
                    },
                ));
            }
            Method::Reduce | Method::ReduceRight => {
                let accumulator =
                    accumulator.ok_or_else(|| invalid("missing reduce accumulator"))?;
                let update = S::Set(accumulator, invocation);
                if initial {
                    step.push(update);
                } else {
                    let ready =
                        ready.ok_or_else(|| invalid("missing reduce initialization flag"))?;
                    step.push(S::If(
                        local(span, ready, Type::Boolean),
                        vec![update],
                        vec![
                            S::Init(accumulator, local(span, current, item)),
                            S::Set(
                                ready,
                                Expr {
                                    span,
                                    ty: Type::Boolean,
                                    kind: E::Integer(1),
                                },
                            ),
                        ],
                    ));
                }
            }
            Method::Map | Method::Filter => {
                let output = output.ok_or_else(|| invalid("missing array callback output"))?;
                let (value, test) = if method == Method::Map {
                    (
                        if callback_result == Type::Void {
                            Expr {
                                span,
                                ty: Type::Undefined,
                                kind: E::Undefined(Box::new(invocation)),
                            }
                        } else {
                            invocation
                        },
                        None,
                    )
                } else {
                    (
                        local(span, current, item),
                        Some(self.array_predicate(invocation)?),
                    )
                };
                let append = if method == Method::Map && self.types.profile.sparse_arrays() {
                    let slots = Some((
                        self.hidden(self.result),
                        self.hidden(if self.types.profile.array_mutations() {
                            Type::Float64
                        } else {
                            Type::Number
                        }),
                    ));
                    S::SetIndex {
                        array: local(span, output, self.result),
                        index: local(span, index, Type::Float64),
                        value,
                        slots,
                    }
                } else {
                    let items = Expr {
                        span,
                        ty: self.result,
                        kind: E::Array(vec![Some(value)]),
                    };
                    S::Expr(Expr {
                        span,
                        ty: Type::Float64,
                        kind: E::ArrayPush(
                            Box::new(local(span, output, self.result)),
                            Box::new(items),
                        ),
                    })
                };
                if let Some(test) = test {
                    step.push(S::If(test, vec![append], vec![]));
                } else {
                    step.push(append);
                }
            }
        }
        let step = vec![S::Scope(vec![current], step)];
        let step = if self.types.profile.sparse_arrays() && !method.visits_holes() {
            let absent = vec![];
            vec![S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::ArrayHasOwn(
                        Box::new(local(span, source, array)),
                        Box::new(local(span, index, Type::Float64)),
                    ),
                },
                step,
                absent,
            )]
        } else if matches!(
            method,
            Method::Filter
                | Method::ForEach
                | Method::Some
                | Method::Every
                | Method::Reduce
                | Method::ReduceRight
        ) {
            vec![S::If(
                binary(
                    span,
                    ast::BinaryOp::Lt,
                    local(span, index, Type::Float64),
                    length(span, source, array),
                    Type::Boolean,
                ),
                step,
                vec![],
            )]
        } else {
            step
        };
        body.push(S::For(
            vec![S::Init(
                index,
                if method.reverse() {
                    binary(
                        span,
                        ast::BinaryOp::Sub,
                        local(span, limit, Type::Float64),
                        number(span, 1.0),
                        Type::Float64,
                    )
                } else {
                    number(span, 0.0)
                },
            )],
            Some(binary(
                span,
                if method.reverse() {
                    ast::BinaryOp::GtEq
                } else {
                    ast::BinaryOp::Lt
                },
                local(span, index, Type::Float64),
                if method.reverse() {
                    number(span, 0.0)
                } else {
                    local(span, limit, Type::Float64)
                },
                Type::Boolean,
            )),
            vec![S::Set(
                index,
                binary(
                    span,
                    if method.reverse() {
                        ast::BinaryOp::Sub
                    } else {
                        ast::BinaryOp::Add
                    },
                    local(span, index, Type::Float64),
                    number(span, 1.0),
                    Type::Float64,
                ),
            )],
            step,
        ));
        if method.reduces() && !initial {
            let ready = ready.ok_or_else(|| invalid("missing reduce initialization flag"))?;
            let error = self.array_error(
                varta_common::value::ErrorClass::TypeError,
                "Reduce of empty array with no initial value",
                span,
            )?;
            body.push(S::If(
                local(span, ready, Type::Boolean),
                vec![],
                vec![S::Throw(error)],
            ));
        }
        body.push(S::Return(match method {
            Method::Map | Method::Filter => Some(local(
                span,
                output.ok_or_else(|| invalid("missing callback output"))?,
                self.result,
            )),
            Method::Find | Method::FindLast => {
                Some(unions::widen(undefined(span), Some(item), self.types)?)
            }
            Method::FindIndex | Method::FindLastIndex => Some(number(span, -1.0)),
            Method::ForEach => None,
            Method::Some | Method::Every => Some(Expr {
                span,
                ty: Type::Boolean,
                kind: E::Integer(i32::from(method == Method::Every)),
            }),
            Method::Reduce | Method::ReduceRight => Some(local(
                span,
                accumulator.ok_or_else(|| invalid("missing reduce accumulator"))?,
                self.result,
            )),
        }));
        Ok(body)
    }
}
