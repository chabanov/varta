//! Cache original operands and narrow the branch selected by short circuiting.
use super::*;
use swc_common::Span;
fn local(span: Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn ordinary(value: &Expr) -> bool {
    value.ty.result_value().is_none() && value.ty != Type::Void
}
fn nullish(ty: Type) -> bool {
    matches!(ty, Type::Null | Type::Undefined)
}
fn reference(ty: Type) -> bool {
    matches!(
        ty,
        Type::Record(_)
            | Type::Dictionary(_)
            | Type::Class(_)
            | Type::Array(_)
            | Type::Function(_)
            | Type::Error
            | Type::NativeError(_)
    )
}
fn choose_right(op: ast::BinaryOp, ty: Type) -> Option<bool> {
    // `unknown` (v34) may hold any member, so neither operand is fixed.
    if ty == Type::Unknown {
        return None;
    }
    match op {
        ast::BinaryOp::NullishCoalescing => Some(nullish(ty)),
        ast::BinaryOp::LogicalAnd if nullish(ty) || reference(ty) => Some(reference(ty)),
        ast::BinaryOp::LogicalOr if nullish(ty) || reference(ty) => Some(nullish(ty)),
        _ => None,
    }
}
fn literal_choice(op: ast::BinaryOp, value: &Expr) -> Option<bool> {
    let truth = match (&value.kind, value.ty) {
        (E::Float64(n), Type::Float64) => *n != 0.0 && !n.is_nan(),
        (E::String(bytes), Type::String) => !bytes.is_empty(),
        (E::Integer(n), Type::Boolean) => *n != 0,
        (E::Negative(inner), Type::Float64) => {
            if let E::Float64(n) = inner.kind {
                n != 0.0 && !n.is_nan()
            } else {
                return None;
            }
        }
        _ => return None,
    };
    match op {
        ast::BinaryOp::LogicalAnd => Some(truth),
        ast::BinaryOp::LogicalOr => Some(!truth),
        _ => None,
    }
}
impl Lower<'_> {
    pub(super) fn guest_logical(
        &mut self,
        op: ast::BinaryOp,
        left: Expr,
        right: Expr,
        span: Span,
    ) -> Result<Expr> {
        for value in [&left, &right] {
            if !ordinary(value) {
                return Err(self.source.locate(type_error("logical operands require ordinary values; opaque result pairs need explicit fields"),value.span));
            }
        }
        let members = unions::members(left.ty, self.types);
        let selected = literal_choice(op, &left).or_else(|| {
            choose_right(op, members[0]).filter(|selected| {
                members
                    .iter()
                    .all(|&ty| choose_right(op, ty) == Some(*selected))
            })
        });
        if let Some(selected) = selected {
            return Ok(if selected {
                Expr {
                    span,
                    ty: right.ty,
                    kind: E::Sequence(Box::new(left), Box::new(right)),
                }
            } else {
                left
            });
        }
        let kept = members
            .into_iter()
            .filter(|&ty| choose_right(op, ty) != Some(true))
            .collect::<Vec<_>>();
        let saved_ty = if kept.len() == 1 {
            kept[0]
        } else {
            unions::intern(kept, self.types)?
        };
        let old_ty = left.ty;
        let input = self.hidden(old_ty);
        let test = if op == ast::BinaryOp::NullishCoalescing {
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::IsNullish(Box::new(local(span, input, old_ty))),
            }
        } else {
            self.truth(local(span, input, old_ty))?
        };
        let saved = if old_ty.is_union() && !saved_ty.is_union() {
            Expr {
                span,
                ty: saved_ty,
                kind: E::UnboxUnion(Box::new(local(span, input, old_ty))),
            }
        } else {
            local(span, input, saved_ty)
        };
        let (ty, saved, right) = if saved_ty == right.ty {
            (saved_ty, saved, right)
        } else {
            self.join(saved, right)?
        };
        let (yes, no) = if op == ast::BinaryOp::LogicalOr {
            (saved, right)
        } else {
            (right, saved)
        };
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![input, output],
                body: vec![
                    S::Init(input, left),
                    S::Init(
                        output,
                        Expr {
                            span,
                            ty,
                            kind: E::Conditional(Box::new(test), Box::new(yes), Box::new(no)),
                        },
                    ),
                ],
            },
        })
    }
}
