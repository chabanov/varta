//! Compare primitive kinds without replaying operands or losing BigInt precision.
use super::*;
fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
impl Lower<'_> {
    pub(super) fn primitive_comparison(
        &mut self,
        op: ast::BinaryOp,
        left: Expr,
        right: Expr,
        span: swc_common::Span,
    ) -> Result<Expr> {
        use ast::BinaryOp as O;
        let equality = matches!(op, O::EqEq | O::EqEqEq | O::NotEq | O::NotEqEq);
        if equality
            && !left.ty.is_union()
            && !right.ty.is_union()
            && (matches!(left.ty, Type::Null | Type::Undefined)
                || matches!(right.ty, Type::Null | Type::Undefined))
        {
            let both_nullish = matches!(left.ty, Type::Null | Type::Undefined)
                && matches!(right.ty, Type::Null | Type::Undefined);
            let equal = if matches!(op, O::EqEqEq | O::NotEqEq) {
                left.ty == right.ty
            } else {
                both_nullish
            };
            let result = equal ^ matches!(op, O::NotEq | O::NotEqEq);
            return Ok(Expr {
                span,
                ty: Type::Boolean,
                kind: E::Sequence(
                    Box::new(left),
                    Box::new(Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Sequence(
                            Box::new(right),
                            Box::new(Expr {
                                span,
                                ty: Type::Boolean,
                                kind: E::Integer(i32::from(result)),
                            }),
                        ),
                    }),
                ),
            });
        }
        if left.ty == right.ty {
            if left.ty == Type::BigInt {
                return self.bigint_binary(op, left, right, span);
            }
            if matches!(left.ty, Type::Float64 | Type::String)
                || equality && matches!(left.ty, Type::Boolean | Type::Null | Type::Undefined)
            {
                return Ok(Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Binary(op, Box::new(left), Box::new(right)),
                });
            }
        }
        let left_ty = left.ty;
        let right_ty = right.ty;
        let a = self.hidden(left_ty);
        let b = self.hidden(right_ty);
        // V38: cells of only the kinds the operands may hold.
        let members = if self.types.profile.linked_intrinsics() {
            unions::comparison_cells(&[left_ty, right_ty], self.types)
        } else {
            vec![
                Type::Float64,
                Type::String,
                Type::Boolean,
                Type::Null,
                Type::Undefined,
                Type::BigInt,
            ]
        };
        let cell = unions::intern(members, self.types)?;
        let aa = self.hidden(cell);
        let bb = self.hidden(cell);
        let mut args = vec![local(span, aa, cell), local(span, bb, cell)];
        let value = if equality {
            args.insert(
                0,
                Expr {
                    span,
                    ty: Type::Number,
                    kind: E::Integer(i32::from(matches!(op, O::EqEqEq | O::NotEqEq))),
                },
            );
            let equal = Expr {
                span,
                ty: Type::Boolean,
                kind: E::Host(Host::PrimitiveEqual, args),
            };
            if matches!(op, O::NotEq | O::NotEqEq) {
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Not(Box::new(equal)),
                }
            } else {
                equal
            }
        } else {
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Binary(
                    op,
                    Box::new(Expr {
                        span,
                        ty: Type::Float64,
                        kind: E::Host(Host::PrimitiveCompare, args),
                    }),
                    Box::new(Expr {
                        span,
                        ty: Type::Float64,
                        kind: E::Float64(0.0),
                    }),
                ),
            }
        };
        let output = self.hidden(Type::Boolean);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty: Type::Boolean,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![a, b, aa, bb, output],
                body: vec![
                    S::Init(a, left),
                    S::Init(b, right),
                    S::Init(
                        aa,
                        unions::widen(local(span, a, left_ty), Some(cell), self.types)?,
                    ),
                    S::Init(
                        bb,
                        unions::widen(local(span, b, right_ty), Some(cell), self.types)?,
                    ),
                    S::Init(output, value),
                ],
            },
        })
    }
}
