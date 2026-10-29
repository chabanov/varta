//! Cache source values before primitive conversion; dispatch on numeric kinds.
use super::*;

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn is_string(span: swc_common::Span, value: Expr) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: if value.ty.is_union() {
            E::IsUnionMember(Box::new(value), Type::String)
        } else {
            E::Integer(i32::from(value.ty == Type::String))
        },
    }
}
impl Lower<'_> {
    pub(super) fn primitive_arithmetic(
        &mut self,
        op: ast::BinaryOp,
        left: Expr,
        right: Expr,
        span: swc_common::Span,
    ) -> Result<Expr> {
        if left.ty == Type::Float64 && right.ty == Type::Float64 {
            return if op == ast::BinaryOp::Exp {
                self.number_power(left, right, span)
            } else if ir::is_number_bitwise(op) {
                self.number_bitwise(op, left, right, span)
            } else {
                Ok(Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Binary(op, Box::new(left), Box::new(right)),
                })
            };
        }
        if left.ty == Type::BigInt && right.ty == Type::BigInt {
            return self.bigint_binary(op, left, right, span);
        }
        let lhs = unions::members(left.ty, self.types);
        let rhs = unions::members(right.ty, self.types);
        if lhs
            .iter()
            .chain(&rhs)
            .any(|ty| !unions::primitive(*ty) && *ty != Type::BigInt)
        {
            return Err(unsupported("object ToPrimitive in arithmetic"));
        }
        let left_ty = left.ty;
        let right_ty = right.ty;
        let a = self.hidden(left_ty);
        let b = self.hidden(right_ty);
        let strings = op == ast::BinaryOp::Add
            && (lhs.contains(&Type::String) || rhs.contains(&Type::String));
        let string_only =
            op == ast::BinaryOp::Add && (left_ty == Type::String || right_ty == Type::String);
        let numeric = if string_only {
            None
        } else {
            let left = self.checked_update_numeric(local(span, a, left_ty))?;
            let right = self.checked_update_numeric(local(span, b, right_ty))?;
            Some(self.bigint_union_binary(op, left, right, span)?)
        };
        let value = if strings {
            let cell_ty = unions::intern(
                vec![
                    Type::Float64,
                    Type::String,
                    Type::Boolean,
                    Type::Null,
                    Type::Undefined,
                    Type::BigInt,
                ],
                self.types,
            )?;
            let left = if left_ty == Type::String {
                local(span, a, left_ty)
            } else {
                Expr {
                    span,
                    ty: Type::String,
                    kind: E::Host(
                        self.types.profile.host_operation(Host::PrimitiveStringV2),
                        vec![unions::widen(
                            local(span, a, left_ty),
                            Some(cell_ty),
                            self.types,
                        )?],
                    ),
                }
            };
            let right = if right_ty == Type::String {
                local(span, b, right_ty)
            } else {
                Expr {
                    span,
                    ty: Type::String,
                    kind: E::Host(
                        self.types.profile.host_operation(Host::PrimitiveStringV2),
                        vec![unions::widen(
                            local(span, b, right_ty),
                            Some(cell_ty),
                            self.types,
                        )?],
                    ),
                }
            };
            let text = Expr {
                span,
                ty: Type::String,
                kind: E::Binary(ast::BinaryOp::Add, Box::new(left), Box::new(right)),
            };
            if let Some(numeric) = numeric {
                let (ty, text, numeric) = self.join(text, numeric)?;
                Expr {
                    span,
                    ty,
                    kind: E::Conditional(
                        Box::new(Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::Binary(
                                ast::BinaryOp::LogicalOr,
                                Box::new(is_string(span, local(span, a, left_ty))),
                                Box::new(is_string(span, local(span, b, right_ty))),
                            ),
                        }),
                        Box::new(text),
                        Box::new(numeric),
                    ),
                }
            } else {
                text
            }
        } else {
            numeric.ok_or_else(|| invalid("missing numeric arithmetic result"))?
        };
        let ty = value.ty;
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![a, b, output],
                body: vec![S::Init(a, left), S::Init(b, right), S::Init(output, value)],
            },
        })
    }
}
