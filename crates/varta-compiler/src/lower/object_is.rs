//! SameValue over cached operands, without coercion or a new native boundary.
use super::*;
fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn boolean(span: swc_common::Span, value: bool) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Integer(i32::from(value)),
    }
}
fn reference(ty: Type) -> bool {
    ty.is_error()
        || matches!(
            ty,
            Type::Record(_) | Type::Class(_) | Type::Array(_) | Type::Function(_)
        )
}
fn member(span: swc_common::Span, slot: u32, source: Type, selected: Type) -> Expr {
    let value = local(span, slot, source);
    if source == selected {
        value
    } else {
        Expr {
            span,
            ty: selected,
            kind: E::UnboxUnion(Box::new(value)),
        }
    }
}
impl Lower<'_> {
    pub(super) fn object_is(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        if self.binding("Object").is_ok() || self.functions.contains_key("Object") {
            return Err(unsupported("shadowed Object.is intrinsic"));
        }
        if call.type_args.is_some() {
            return Err(invalid("Object.is has no type arguments"));
        }
        let span = call.span;
        let mut inputs = Vec::new();
        let mut roots = Vec::new();
        let mut body = Vec::new();
        for (index, arg) in call.args.iter().enumerate() {
            if arg.spread.is_some() {
                return Err(unsupported("Object.is argument spread"));
            }
            let value = self.expr(&arg.expr)?;
            if index >= 2 {
                body.push(S::Expr(value));
                continue;
            }
            let value = if value.ty == Type::Void {
                Expr {
                    span: value.span,
                    ty: Type::Undefined,
                    kind: E::Undefined(Box::new(value)),
                }
            } else {
                value
            };
            let ty = value.ty;
            if unions::members(ty, self.types).into_iter().any(|t| {
                !reference(t)
                    && !matches!(
                        t,
                        Type::Float64
                            | Type::String
                            | Type::Boolean
                            | Type::Null
                            | Type::Undefined
                            | Type::BigInt
                    )
            }) {
                return Err(type_error(
                    "Object.is requires language values; native result pairs require explicit fields",
                ));
            }
            let slot = self.hidden(ty);
            roots.push(slot);
            body.push(S::Init(slot, value));
            inputs.push((slot, ty));
        }
        while inputs.len() < 2 {
            let slot = self.hidden(Type::Undefined);
            body.push(S::Init(
                slot,
                Expr {
                    span,
                    ty: Type::Undefined,
                    kind: E::Integer(0),
                },
            ));
            inputs.push((slot, Type::Undefined));
        }
        let (a, at) = inputs[0];
        let (b, bt) = inputs[1];
        let mut result = boolean(span, false);
        for left in unions::members(at, self.types).into_iter().rev() {
            let mut right_result = boolean(span, false);
            for right in unions::members(bt, self.types).into_iter().rev() {
                let left_value = member(span, a, at, left);
                let right_value = member(span, b, bt, right);
                let equal = if left == Type::Float64 && right == Type::Float64 {
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::NumberSameValue(Box::new(left_value), Box::new(right_value)),
                    }
                } else if left == Type::BigInt && right == Type::BigInt {
                    self.bigint_binary(ast::BinaryOp::EqEqEq, left_value, right_value, span)?
                } else if left == right && matches!(left, Type::Null | Type::Undefined) {
                    boolean(span, true)
                } else if reference(left) && reference(right)
                    || left == right && matches!(left, Type::Boolean | Type::String)
                {
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Binary(
                            ast::BinaryOp::EqEqEq,
                            Box::new(left_value),
                            Box::new(right_value),
                        ),
                    }
                } else {
                    boolean(span, false)
                };
                right_result = if bt.is_union() {
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Conditional(
                            Box::new(Expr {
                                span,
                                ty: Type::Boolean,
                                kind: E::IsUnionMember(Box::new(local(span, b, bt)), right),
                            }),
                            Box::new(equal),
                            Box::new(right_result),
                        ),
                    }
                } else {
                    equal
                };
            }
            result = if at.is_union() {
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Conditional(
                        Box::new(Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::IsUnionMember(Box::new(local(span, a, at)), left),
                        }),
                        Box::new(right_result),
                        Box::new(result),
                    ),
                }
            } else {
                right_result
            };
        }
        let output = self.hidden(Type::Boolean);
        let marker = self.hidden(Type::Number);
        body.push(S::Init(output, result));
        Ok(Expr {
            span,
            ty: Type::Boolean,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }
}
