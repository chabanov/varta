//! Cache receiver/arguments before mode-specific conversion and range ordering.
use super::*;
use swc_common::Span;
use varta_common::{number_format::NumberFormat, value::ErrorClass};
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
fn comparison(span: Span, op: ast::BinaryOp, left: Expr, right: Expr) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Binary(op, Box::new(left), Box::new(right)),
    }
}
impl Lower<'_> {
    pub(super) fn number_value_of(&mut self, call: &ast::CallExpr, value: Expr) -> Result<Expr> {
        if !self.types.profile.number_format() {
            return Err(unsupported("Number.valueOf requires checked-f64-utf16-v22"));
        }
        if value.ty != Type::Float64 {
            return Err(self.source.locate(
                type_error("valueOf requires a narrowed Number receiver"),
                value.span,
            ));
        }
        let span = call.span;
        let output = self.hidden(Type::Float64);
        let marker = self.hidden(Type::Number);
        let mut body = vec![S::Init(output, value)];
        for arg in &call.args {
            body.push(S::Expr(self.expr(&arg.expr)?));
        }
        Ok(Expr {
            span,
            ty: Type::Float64,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![output],
                body,
            },
        })
    }
    pub(super) fn number_format(
        &mut self,
        call: &ast::CallExpr,
        value: Expr,
        mode: NumberFormat,
    ) -> Result<Expr> {
        if !self.types.profile.number_format() {
            return Err(unsupported(
                "decimal Number formatting requires checked-f64-utf16-v22",
            ));
        }
        if value.ty != Type::Float64 {
            return Err(self.source.locate(
                type_error("decimal formatting requires a narrowed Number receiver"),
                value.span,
            ));
        }
        let span = call.span;
        let receiver = self.hidden(Type::Float64);
        let mut roots = vec![receiver];
        let mut body = vec![S::Init(receiver, value)];
        let omitted = self.hidden(Type::Boolean);
        roots.push(omitted);
        let raw = if let Some((first, rest)) = call.args.split_first() {
            let value = self.expr(&first.expr)?;
            if unions::members(value.ty, self.types)
                .iter()
                .any(|ty| !unions::primitive(*ty))
            {
                return Err(self.source.locate(
                    type_error(
                        "decimal precision requires primitives; object coercion is unsupported",
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
            body.push(S::Init(
                omitted,
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsUndefined(Box::new(local(span, input, ty))),
                },
            ));
            if ty == Type::Float64 {
                local(span, input, ty)
            } else if ty == Type::Undefined {
                number(span, 0.0)
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
                Expr {
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
                }
            }
        } else {
            body.push(S::Init(
                omitted,
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Integer(1),
                },
            ));
            number(span, 0.0)
        };
        let raw_slot = self.hidden(Type::Float64);
        roots.push(raw_slot);
        body.push(S::Init(
            raw_slot,
            Expr {
                span,
                ty: Type::Float64,
                kind: E::Conditional(
                    Box::new(local(span, omitted, Type::Boolean)),
                    Box::new(number(
                        span,
                        if mode == NumberFormat::Fixed {
                            0.0
                        } else {
                            -1.0
                        },
                    )),
                    Box::new(raw),
                ),
            },
        ));
        let digits = self.hidden(Type::Float64);
        roots.push(digits);
        body.push(S::Init(
            digits,
            Expr {
                span,
                ty: Type::Float64,
                kind: E::Conditional(
                    Box::new(comparison(
                        span,
                        ast::BinaryOp::NotEqEq,
                        local(span, raw_slot, Type::Float64),
                        local(span, raw_slot, Type::Float64),
                    )),
                    Box::new(number(span, 0.0)),
                    Box::new(Expr {
                        span,
                        ty: Type::Float64,
                        kind: E::Math(
                            ir::MathIntrinsic::Trunc,
                            vec![local(span, raw_slot, Type::Float64)],
                        ),
                    }),
                ),
            },
        ));
        let mut checks = Vec::new();
        let minimum = if mode == NumberFormat::Precision {
            1.0
        } else {
            0.0
        };
        for (op, bound) in [(ast::BinaryOp::Lt, minimum), (ast::BinaryOp::Gt, 100.0)] {
            let message = Expr {
                span,
                ty: Type::String,
                kind: E::String(
                    self.types
                        .profile
                        .string_bytes("decimal precision is outside the supported range"),
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
            let target = self.exception_type()?;
            checks.push(S::If(
                comparison(
                    span,
                    op,
                    local(span, digits, Type::Float64),
                    number(span, bound),
                ),
                vec![S::Throw(unions::widen(error, Some(target), self.types)?)],
                vec![],
            ));
        }
        if mode == NumberFormat::Fixed {
            body.extend(checks);
        } else {
            body.push(S::If(
                local(span, omitted, Type::Boolean),
                vec![],
                vec![S::If(
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::NumberPredicate(
                            ir::NumberPredicate::Finite,
                            vec![local(span, receiver, Type::Float64)],
                        ),
                    },
                    checks,
                    vec![],
                )],
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
                    mode.host(),
                    vec![
                        local(span, receiver, Type::Float64),
                        local(span, digits, Type::Float64),
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
