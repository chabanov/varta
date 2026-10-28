//! Prepare receiver/all args before radix conversion, validation and formatting.
use super::*;
use swc_common::Span;
use varta_common::value::ErrorClass;
fn number(span: Span, value: f64) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Float64(value),
    }
}
fn local(span: Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn compare(span: Span, op: ast::BinaryOp, left: Expr, right: Expr) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Binary(op, Box::new(left), Box::new(right)),
    }
}
impl Lower<'_> {
    pub(super) fn number_to_string(&mut self, call: &ast::CallExpr, value: Expr) -> Result<Expr> {
        if !self.types.profile.number_radix() {
            return Err(unsupported(
                "Number.toString requires checked-f64-utf16-v21",
            ));
        }
        let span = call.span;
        let receiver = self.hidden(Type::Float64);
        let mut body = vec![S::Init(receiver, value)];
        let mut roots = vec![receiver];
        let radix = if let Some((first, rest)) = call.args.split_first() {
            let value = self.expr(&first.expr)?;
            if unions::members(value.ty, self.types)
                .iter()
                .any(|ty| !unions::primitive(*ty))
            {
                return Err(self.source.locate(
                    type_error(
                        "Number.toString radix requires primitives; object coercion is unsupported",
                    ),
                    value.span,
                ));
            }
            let ty = value.ty;
            let input = self.hidden(ty);
            roots.push(input);
            body.push(S::Init(input, value));
            for arg in rest {
                body.push(S::Expr(self.expr(&arg.expr)?));
            }
            let value = local(span, input, ty);
            if ty == Type::Undefined {
                number(span, 10.0)
            } else if ty == Type::Float64 {
                value
            } else {
                let target = unions::intern(
                    vec![
                        Type::Float64,
                        Type::Boolean,
                        Type::String,
                        Type::Null,
                        Type::Undefined,
                    ],
                    self.types,
                )?;
                let converted = Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Host(
                        Host::PrimitiveNumber,
                        vec![unions::widen(
                            local(span, input, ty),
                            Some(target),
                            self.types,
                        )?],
                    ),
                };
                if ty.is_union() && unions::members(ty, self.types).contains(&Type::Undefined) {
                    Expr {
                        span,
                        ty: Type::Float64,
                        kind: E::Conditional(
                            Box::new(Expr {
                                span,
                                ty: Type::Boolean,
                                kind: E::IsUndefined(Box::new(value)),
                            }),
                            Box::new(number(span, 10.0)),
                            Box::new(converted),
                        ),
                    }
                } else {
                    converted
                }
            }
        } else {
            number(span, 10.0)
        };
        let raw = self.hidden(Type::Float64);
        body.push(S::Init(raw, radix));
        roots.push(raw);
        let radix = self.hidden(Type::Float64);
        roots.push(radix);
        let raw_value = local(span, raw, Type::Float64);
        body.push(S::Init(
            radix,
            Expr {
                span,
                ty: Type::Float64,
                kind: E::Conditional(
                    Box::new(compare(
                        span,
                        ast::BinaryOp::NotEqEq,
                        local(span, raw, Type::Float64),
                        local(span, raw, Type::Float64),
                    )),
                    Box::new(number(span, 0.0)),
                    Box::new(Expr {
                        span,
                        ty: Type::Float64,
                        kind: E::Math(ir::MathIntrinsic::Trunc, vec![raw_value]),
                    }),
                ),
            },
        ));
        for (op, bound) in [(ast::BinaryOp::Lt, 2.0), (ast::BinaryOp::Gt, 36.0)] {
            let message = Expr {
                span,
                ty: Type::String,
                kind: E::String(
                    self.types
                        .profile
                        .string_bytes("radix must be between 2 and 36"),
                ),
            };
            let target = unions::intern(vec![Type::String, Type::Undefined], self.types)?;
            let error = Expr {
                span,
                ty: Type::from_error_class(ErrorClass::RangeError),
                kind: E::NewError(
                    ErrorClass::RangeError,
                    Box::new(unions::widen(message, Some(target), self.types)?),
                ),
            };
            let exception = self.exception_type()?;
            let error = unions::widen(error, Some(exception), self.types)?;
            body.push(S::If(
                compare(
                    span,
                    op,
                    local(span, radix, Type::Float64),
                    number(span, bound),
                ),
                vec![S::Throw(error)],
                vec![],
            ));
        }
        let output = self.hidden(Type::String);
        roots.push(output);
        let marker = self.hidden(Type::Number);
        body.push(S::Init(
            output,
            Expr {
                span,
                ty: Type::String,
                kind: E::Host(
                    Host::NumberToString,
                    vec![
                        local(span, receiver, Type::Float64),
                        local(span, radix, Type::Float64),
                    ],
                ),
            },
        ));
        Ok(Expr {
            span,
            ty: Type::String,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }
}
