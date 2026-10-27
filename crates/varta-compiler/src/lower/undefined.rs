//! Statically known nullish values retain source evaluation order, with no coercion.
use super::*;
pub(super) enum Binary {
    Folded(Expr),
    Unchanged(Expr, Expr),
}
pub(super) fn binary(op: ast::BinaryOp, left: Expr, right: Expr, span: swc_common::Span) -> Binary {
    use ast::BinaryOp::*;
    let nullish = |ty| matches!(ty, Type::Null | Type::Undefined);
    let comparison = matches!(op, EqEq | EqEqEq | NotEq | NotEqEq);
    let strict = matches!(op, EqEqEq | NotEqEq);
    if comparison
        && left.ty.value_kind().is_some()
        && right.ty.value_kind().is_some()
        && (nullish(left.ty) || nullish(right.ty) || strict && left.ty != right.ty)
    {
        let equal = if strict {
            left.ty == right.ty
        } else {
            nullish(left.ty) && nullish(right.ty)
        };
        let value = equal != matches!(op, NotEq | NotEqEq);
        let constant = Expr {
            span,
            ty: Type::Boolean,
            kind: E::Integer(i32::from(value)),
        };
        let tail = Expr {
            span,
            ty: Type::Boolean,
            kind: E::Sequence(Box::new(right), Box::new(constant)),
        };
        return Binary::Folded(Expr {
            span,
            ty: Type::Boolean,
            kind: E::Sequence(Box::new(left), Box::new(tail)),
        });
    }
    if nullish(left.ty) && op == LogicalOr {
        return Binary::Folded(Expr {
            span,
            ty: right.ty,
            kind: E::Sequence(Box::new(left), Box::new(right)),
        });
    }
    if nullish(left.ty) && op == LogicalAnd {
        return Binary::Folded(left);
    }
    Binary::Unchanged(left, right)
}
