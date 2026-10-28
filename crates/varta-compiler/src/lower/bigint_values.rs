//! Specialize admitted primitive union pairs after caching source operands once.
//! Runtime dispatch preserves BigInt/Number results and mixed-numeric TypeErrors.
use super::*;
use swc_common::Span;
fn local(span: Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn member(span: Span, slot: u32, ty: Type, member: Type) -> Expr {
    let value = local(span, slot, ty);
    if ty.is_union() {
        Expr {
            span,
            ty: member,
            kind: E::UnboxUnion(Box::new(value)),
        }
    } else {
        value
    }
}
impl Lower<'_> {
    fn numeric_primitive(&mut self, value: Expr) -> Result<Expr> {
        if value.ty == Type::Float64 {
            return Ok(value);
        }
        let span = value.span;
        let ty = unions::intern(
            vec![
                Type::Float64,
                Type::String,
                Type::Boolean,
                Type::Null,
                Type::Undefined,
            ],
            self.types,
        )?;
        Ok(Expr {
            span,
            ty: Type::Float64,
            kind: E::Host(
                Host::PrimitiveNumberV2,
                vec![unions::widen(value, Some(ty), self.types)?],
            ),
        })
    }
    fn text_primitive(&mut self, value: Expr) -> Result<Expr> {
        if value.ty == Type::String {
            return Ok(value);
        }
        if value.ty == Type::BigInt {
            return self.bigint_text(value);
        }
        let span = value.span;
        let ty = unions::intern(
            vec![
                Type::Float64,
                Type::String,
                Type::Boolean,
                Type::Null,
                Type::Undefined,
            ],
            self.types,
        )?;
        Ok(Expr {
            span,
            ty: Type::String,
            kind: E::Host(
                Host::PrimitiveStringV2,
                vec![unions::widen(value, Some(ty), self.types)?],
            ),
        })
    }
    pub(super) fn bigint_union_binary(
        &mut self,
        op: ast::BinaryOp,
        left: Expr,
        right: Expr,
        span: Span,
    ) -> Result<Expr> {
        let left_ty = left.ty;
        let right_ty = right.ty;
        let lhs = unions::members(left_ty, self.types);
        let rhs = unions::members(right_ty, self.types);
        if lhs
            .iter()
            .chain(&rhs)
            .any(|ty| !unions::primitive(*ty) && *ty != Type::BigInt)
        {
            return Err(unsupported("object coercion in a numeric union"));
        }
        let left_slot = self.hidden(left_ty);
        let right_slot = self.hidden(right_ty);
        let mut branches = Vec::new();
        for &a in &lhs {
            for &b in &rhs {
                let left = member(span, left_slot, left_ty, a);
                let right = member(span, right_slot, right_ty, b);
                let string_add =
                    op == ast::BinaryOp::Add && (a == Type::String || b == Type::String);
                if (a == Type::BigInt || b == Type::BigInt) && a != b && !string_add {
                    branches.push((a, b, None));
                    continue;
                }
                let value = if string_add {
                    Expr {
                        span,
                        ty: Type::String,
                        kind: E::Binary(
                            op,
                            Box::new(self.text_primitive(left)?),
                            Box::new(self.text_primitive(right)?),
                        ),
                    }
                } else if a == Type::BigInt || b == Type::BigInt {
                    self.bigint_binary(op, left, right, span)?
                } else {
                    let left = self.numeric_primitive(left)?;
                    let right = self.numeric_primitive(right)?;
                    if op == ast::BinaryOp::Exp {
                        self.number_power(left, right, span)?
                    } else if ir::is_number_bitwise(op) {
                        self.number_bitwise(op, left, right, span)?
                    } else {
                        Expr {
                            span,
                            ty: Type::Float64,
                            kind: E::Binary(op, Box::new(left), Box::new(right)),
                        }
                    }
                };
                branches.push((a, b, Some(value)));
            }
        }
        let mut result_types = branches
            .iter()
            .filter_map(|(_, _, value)| value.as_ref().map(|v| v.ty))
            .collect::<Vec<_>>();
        if result_types.is_empty() {
            // All pairs throw. Keep an unreachable result compatible with the
            // cached left value; a failing operation must not invent BigInt.
            result_types.push(left_ty);
        }
        result_types.sort();
        result_types.dedup();
        let result_ty = if result_types.len() == 1 {
            result_types[0]
        } else {
            unions::intern(result_types, self.types)?
        };
        let mut selected = None;
        for (a, b, value) in branches.into_iter().rev() {
            let value = if let Some(value) = value {
                unions::widen(value, Some(result_ty), self.types)?
            } else {
                let output = self.hidden(result_ty);
                let marker = self.hidden(Type::Number);
                let error = self.bigint_error(span, varta_common::value::ErrorClass::TypeError)?;
                Expr {
                    span,
                    ty: result_ty,
                    kind: E::ValueBlock {
                        output,
                        marker,
                        roots: vec![output],
                        body: vec![S::Throw(error)],
                    },
                }
            };
            selected = Some(if let Some(other) = selected {
                let test = match (left_ty.is_union(), right_ty.is_union()) {
                    (true, true) => Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Binary(
                            ast::BinaryOp::LogicalAnd,
                            Box::new(Expr {
                                span,
                                ty: Type::Boolean,
                                kind: E::IsUnionMember(
                                    Box::new(local(span, left_slot, left_ty)),
                                    a,
                                ),
                            }),
                            Box::new(Expr {
                                span,
                                ty: Type::Boolean,
                                kind: E::IsUnionMember(
                                    Box::new(local(span, right_slot, right_ty)),
                                    b,
                                ),
                            }),
                        ),
                    },
                    (true, false) => Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::IsUnionMember(Box::new(local(span, left_slot, left_ty)), a),
                    },
                    (false, true) => Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::IsUnionMember(Box::new(local(span, right_slot, right_ty)), b),
                    },
                    _ => {
                        return Err(CompilerError::Internal(
                            "missing primitive union dispatch".into(),
                        ));
                    }
                };
                Expr {
                    span,
                    ty: result_ty,
                    kind: E::Conditional(Box::new(test), Box::new(value), Box::new(other)),
                }
            } else {
                value
            });
        }
        let selected = selected
            .ok_or_else(|| CompilerError::Internal("empty primitive union dispatch".into()))?;
        let output = self.hidden(result_ty);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty: result_ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![left_slot, right_slot, output],
                body: vec![
                    S::Init(left_slot, left),
                    S::Init(right_slot, right),
                    S::Init(output, selected),
                ],
            },
        })
    }
}
