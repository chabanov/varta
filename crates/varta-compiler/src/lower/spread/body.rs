//! Bounded expansion into typed private slots; target code runs only after validation.
use super::*;
struct Destination<'a> {
    signature: &'a ir::FunctionType,
    slots: Vec<u32>,
    rest: Option<u32>,
    position: u32,
    span: swc_common::Span,
}
impl Lower<'_> {
    pub(super) fn spread_body(
        &mut self,
        signature: &ir::FunctionType,
        receiver: Option<u32>,
        segments: &[Segment],
        span: swc_common::Span,
    ) -> Result<Vec<S>> {
        let callable = Type::Function(
            self.types
                .functions
                .iter()
                .position(|s| s == signature)
                .ok_or_else(|| invalid("missing spread signature"))? as u32,
        );
        let count = self.hidden(Type::Float64);
        let position = self.hidden(Type::Float64);
        let check = self.hidden(callable);
        let mut body = vec![
            S::Expr(Expr {
                span,
                ty: Type::Void,
                kind: E::FunctionCheck {
                    value: Box::new(local(span, 0, callable)),
                    slot: check,
                },
            }),
            S::Init(count, number(span, 0)),
            S::Init(position, number(span, 0)),
        ];
        for segment in segments {
            let length = if segment.spread {
                Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Length(Box::new(local(segment.span, segment.slot, segment.ty))),
                }
            } else {
                number(span, 1)
            };
            body.push(S::Set(
                count,
                binary(
                    span,
                    ast::BinaryOp::Add,
                    local(span, count, Type::Float64),
                    length,
                    Type::Float64,
                ),
            ));
        }
        body.push(S::If(
            binary(
                span,
                ast::BinaryOp::Gt,
                local(span, count, Type::Float64),
                number(span, varta_common::abi::MAX_SPREAD_ARGUMENTS),
                Type::Boolean,
            ),
            vec![S::Trap],
            vec![],
        ));
        body.push(S::If(
            binary(
                span,
                ast::BinaryOp::Lt,
                local(span, count, Type::Float64),
                number(span, signature.shape.minimum()),
                Type::Boolean,
            ),
            vec![self.spread_error("Invalid spread argument count", span)?],
            vec![],
        ));
        if !signature.shape.rest {
            body.push(S::If(
                binary(
                    span,
                    ast::BinaryOp::Gt,
                    local(span, count, Type::Float64),
                    number(span, signature.params.len()),
                    Type::Boolean,
                ),
                vec![self.spread_error("Invalid spread argument count", span)?],
                vec![],
            ));
        }
        let mut slots = Vec::new();
        for (index, ty) in signature
            .params
            .iter()
            .take(signature.shape.fixed_count())
            .enumerate()
        {
            let slot = self.hidden(*ty);
            slots.push(slot);
            if signature.shape.optional[index] {
                body.push(S::Init(
                    slot,
                    unions::widen(undefined(span), Some(*ty), self.types)?,
                ));
            }
        }
        let rest = if signature.shape.rest {
            let ty = *signature
                .params
                .last()
                .ok_or_else(|| invalid("missing rest type"))?;
            let slot = self.hidden(ty);
            let kind = if self.types.profile.sparse_arrays() {
                let length = self.hidden(Type::Float64);
                body.push(S::Init(
                    length,
                    binary(
                        span,
                        ast::BinaryOp::Sub,
                        local(span, count, Type::Float64),
                        number(span, signature.shape.fixed_count()),
                        Type::Float64,
                    ),
                ));
                body.push(S::If(
                    binary(
                        span,
                        ast::BinaryOp::Lt,
                        local(span, length, Type::Float64),
                        number(span, 0),
                        Type::Boolean,
                    ),
                    vec![S::Set(length, number(span, 0))],
                    vec![],
                ));
                E::ArrayReserve {
                    length: Box::new(local(span, length, Type::Float64)),
                    count: self.hidden(Type::Number),
                    bytes: self.hidden(Type::Number),
                }
            } else {
                E::Array(vec![])
            };
            body.push(S::Init(slot, Expr { span, ty, kind }));
            Some(slot)
        } else {
            None
        };
        let destination = Destination {
            signature,
            slots,
            rest,
            position,
            span,
        };
        let mut known = Some(0usize);
        for segment in segments {
            if segment.spread {
                let Type::Array(id) = segment.ty else {
                    return Err(invalid("missing spread array type"));
                };
                let element = self.types.arrays[id as usize];
                let index = self.hidden(Type::Float64);
                let value = self.hidden(element);
                let read = Expr {
                    span: segment.span,
                    ty: element,
                    kind: E::Index(
                        Box::new(local(span, segment.slot, segment.ty)),
                        Box::new(local(span, index, Type::Float64)),
                    ),
                };
                let mut step = vec![S::Init(value, read)];
                step.extend(self.spread_consume(
                    local(segment.span, value, element),
                    None,
                    &destination,
                )?);
                let step = vec![S::Scope(vec![value], step)];
                let mut step = if self.types.profile.sparse_arrays() {
                    vec![S::If(
                        Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::ArrayHasOwn(
                                Box::new(local(span, segment.slot, segment.ty)),
                                Box::new(local(span, index, Type::Float64)),
                            ),
                        },
                        step,
                        self.spread_consume(undefined(segment.span), None, &destination)?,
                    )]
                } else {
                    step
                };
                step.push(S::Set(
                    position,
                    binary(
                        span,
                        ast::BinaryOp::Add,
                        local(span, position, Type::Float64),
                        number(span, 1),
                        Type::Float64,
                    ),
                ));
                body.push(S::For(
                    vec![S::Init(index, number(span, 0))],
                    Some(binary(
                        span,
                        ast::BinaryOp::Lt,
                        local(span, index, Type::Float64),
                        Expr {
                            span,
                            ty: Type::Float64,
                            kind: E::Length(Box::new(local(span, segment.slot, segment.ty))),
                        },
                        Type::Boolean,
                    )),
                    vec![S::Set(
                        index,
                        binary(
                            span,
                            ast::BinaryOp::Add,
                            local(span, index, Type::Float64),
                            number(span, 1),
                            Type::Float64,
                        ),
                    )],
                    step,
                ));
                known = None;
            } else {
                body.extend(self.spread_consume(
                    local(segment.span, segment.slot, segment.ty),
                    known,
                    &destination,
                )?);
                body.push(S::Set(
                    position,
                    binary(
                        span,
                        ast::BinaryOp::Add,
                        local(span, position, Type::Float64),
                        number(span, 1),
                        Type::Float64,
                    ),
                ));
                known = known.map(|v| v + 1);
            }
        }
        let mut args: Vec<_> = destination
            .slots
            .iter()
            .zip(&signature.params)
            .map(|(slot, ty)| local(span, *slot, *ty))
            .collect();
        if let Some(slot) = rest {
            args.push(local(
                span,
                slot,
                *signature
                    .params
                    .last()
                    .ok_or_else(|| invalid("missing rest type"))?,
            ));
        }
        let receiver = receiver.map(|slot| local(span, slot, self.local_types[&slot]));
        let invoke = self.invoke_bound_receiver(local(span, 0, callable), receiver, args, span)?;
        if signature.result == Type::Void {
            body.push(S::Expr(invoke));
            body.push(S::Return(None));
        } else {
            body.push(S::Return(Some(invoke)));
        }
        Ok(body)
    }
    fn spread_error(&mut self, message: &str, span: swc_common::Span) -> Result<S> {
        Ok(S::Throw(self.array_error(
            varta_common::value::ErrorClass::TypeError,
            message,
            span,
        )?))
    }
    fn spread_consume(
        &mut self,
        value: Expr,
        known: Option<usize>,
        destination: &Destination<'_>,
    ) -> Result<Vec<S>> {
        if let Some(index) = known {
            return self.spread_position(value, index, destination);
        }
        if destination.slots.is_empty() {
            return self.spread_position(value, 0, destination);
        }
        // The source value is already a rooted local (or Undefined), so each
        // mutually exclusive case reads it once without repeating source effects.
        let mut cases = Vec::new();
        for index in 0..=destination.slots.len() {
            let copy = if let E::Local(slot) = value.kind {
                local(value.span, slot, value.ty)
            } else {
                undefined(value.span)
            };
            let mut body = self.spread_position(copy, index, destination)?;
            body.push(S::Break);
            cases.push((
                (index < destination.slots.len()).then(|| number(destination.span, index)),
                body,
            ));
        }
        Ok(vec![S::Switch {
            value: local(destination.span, destination.position, Type::Float64),
            discriminant: self.hidden(Type::Float64),
            selected: self.hidden(Type::Number),
            cases,
        }])
    }
    fn spread_position(
        &mut self,
        value: Expr,
        index: usize,
        destination: &Destination<'_>,
    ) -> Result<Vec<S>> {
        let expected = arguments::context(
            index,
            &destination.signature.params,
            &destination.signature.shape,
            self.types,
        )
        .ok_or_else(|| invalid("missing spread destination type"));
        let expected = match expected {
            Ok(ty) => ty,
            Err(_) => {
                return Ok(vec![self.spread_error(
                    "Invalid spread argument count",
                    destination.span,
                )?]);
            }
        };
        if arguments::accepts(expected, value.ty, self.types) {
            return Ok(vec![self.spread_store(
                unions::widen(value, Some(expected), self.types)?,
                index,
                destination,
            )?]);
        }
        let members = unions::members(value.ty, self.types);
        let present: Vec<_> = members
            .iter()
            .copied()
            .filter(|t| *t != Type::Undefined)
            .collect();
        if value.ty.is_union() && members.contains(&Type::Undefined) && !present.is_empty() {
            let narrowed = unions::intern(present, self.types)?;
            if arguments::accepts(expected, narrowed, self.types) {
                let E::Local(slot) = value.kind else {
                    return Err(invalid("spread value is not a local"));
                };
                let test = Expr {
                    span: value.span,
                    ty: Type::Boolean,
                    kind: E::IsUndefined(Box::new(local(value.span, slot, value.ty))),
                };
                let present = if narrowed.is_union() {
                    Expr {
                        ty: narrowed,
                        ..value
                    }
                } else {
                    Expr {
                        span: value.span,
                        ty: narrowed,
                        kind: E::UnboxUnion(Box::new(value)),
                    }
                };
                let value = unions::widen(present, Some(expected), self.types)?;
                return Ok(vec![S::If(
                    test,
                    vec![self.spread_error("Invalid spread argument type", destination.span)?],
                    vec![self.spread_store(value, index, destination)?],
                )]);
            }
        }
        Ok(vec![self.spread_error(
            "Invalid spread argument type",
            destination.span,
        )?])
    }
    fn spread_store(
        &mut self,
        value: Expr,
        index: usize,
        destination: &Destination<'_>,
    ) -> Result<S> {
        if let Some(slot) = destination.slots.get(index) {
            return Ok(S::Set(*slot, value));
        }
        let slot = destination
            .rest
            .ok_or_else(|| invalid("missing rest destination"))?;
        let ty = *destination
            .signature
            .params
            .last()
            .ok_or_else(|| invalid("missing rest type"))?;
        if self.types.profile.sparse_arrays() {
            Ok(S::SetIndex {
                array: local(destination.span, slot, ty),
                index: binary(
                    destination.span,
                    ast::BinaryOp::Sub,
                    local(destination.span, destination.position, Type::Float64),
                    number(destination.span, destination.slots.len()),
                    Type::Float64,
                ),
                value,
                slots: Some((
                    self.hidden(ty),
                    self.hidden(if self.types.profile.array_mutations() {
                        Type::Float64
                    } else {
                        Type::Number
                    }),
                )),
            })
        } else {
            Ok(S::Expr(Expr {
                span: destination.span,
                ty: Type::Float64,
                kind: E::ArrayPush(
                    Box::new(local(destination.span, slot, ty)),
                    Box::new(Expr {
                        span: destination.span,
                        ty,
                        kind: E::Array(vec![Some(value)]),
                    }),
                ),
            }))
        }
    }
}
