//! BigInt operands are evaluated once, rooted, and transported as canonical leaves.
use super::*;
use swc_common::Span;
use varta_common::{
    bigint::{BinaryOp as B, UnaryOp as U},
    value::ErrorClass,
};
fn local(span: Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn integer(span: Span, value: i32) -> Expr {
    Expr {
        span,
        ty: Type::Number,
        kind: E::Integer(value),
    }
}
fn number(span: Span, value: f64) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Float64(value),
    }
}
impl Lower<'_> {
    pub(super) fn bigint_literal(&mut self, literal: &ast::BigInt) -> Result<Expr> {
        if !self.types.profile.bigint() {
            return Err(unsupported("BigInt requires checked-f64-utf16-v28"));
        }
        let mut budget = varta_bigint::Budget::new(
            ((varta_common::bigint::MAX_PAYLOAD_BYTES - varta_common::bigint::META_BYTES) * 8)
                as u64,
            varta_common::abi::FUEL,
        );
        if literal.value.bits() > budget.max_result_bits() {
            return Err(invalid("BigInt literal exceeds binary transport capacity"));
        }
        // The grammar puts a leading minus in a separate UnaryExpr. SWC has
        // already parsed the unsigned magnitude, so avoid decimal formatting and
        // a second unmetered parser pass for long binary/hexadecimal literals.
        let digits = literal.value.to_u32_digits().1;
        let value = varta_bigint::Integer::from_unsigned_digits(&digits, &mut budget)
            .and_then(|n| n.to_payload(&mut budget))
            .map_err(|e| invalid(e.to_string()))?;
        Ok(Expr {
            span: literal.span,
            ty: Type::BigInt,
            kind: E::String(value),
        })
    }
    pub(super) fn bigint_checked(
        &mut self,
        span: Span,
        host: Host,
        args: Vec<Expr>,
        ty: Type,
    ) -> Result<Expr> {
        let status = self.hidden(Type::Number);
        self.bigint_checked_result(
            Expr {
                span,
                ty,
                kind: E::CheckedBigIntHost { host, args, status },
            },
            status,
        )
    }
    pub(super) fn bigint_checked_result(&mut self, value: Expr, status: u32) -> Result<Expr> {
        let span = value.span;
        let ty = value.ty;
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        let mut body = vec![S::Init(output, value)];
        for class in [
            ErrorClass::TypeError,
            ErrorClass::RangeError,
            ErrorClass::SyntaxError,
        ] {
            let error = self.bigint_error(span, class)?;
            body.push(S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Binary(
                        ast::BinaryOp::EqEqEq,
                        Box::new(local(span, status, Type::Number)),
                        Box::new(integer(span, class as i32)),
                    ),
                },
                vec![S::Throw(error)],
                vec![],
            ));
        }
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![output],
                body,
            },
        })
    }
    pub(super) fn bigint_error(&mut self, span: Span, class: ErrorClass) -> Result<Expr> {
        let message = Expr {
            span,
            ty: Type::String,
            kind: E::String(self.types.profile.string_bytes("invalid BigInt operation")),
        };
        let message_ty = unions::intern(vec![Type::String, Type::Undefined], self.types)?;
        let error = Expr {
            span,
            ty: Type::from_error_class(class),
            kind: E::NewError(
                class,
                Box::new(unions::widen(message, Some(message_ty), self.types)?),
            ),
        };
        let exception = self.exception_type()?;
        unions::widen(error, Some(exception), self.types)
    }
    pub(super) fn bigint_text(&mut self, value: Expr) -> Result<Expr> {
        let span = value.span;
        self.bigint_checked(
            span,
            Host::BigIntToString,
            vec![value, number(span, 10.0)],
            Type::String,
        )
    }
    pub(super) fn bigint_unary(
        &mut self,
        op: ast::UnaryOp,
        value: Expr,
        span: Span,
    ) -> Result<Expr> {
        let op = match op {
            ast::UnaryOp::Minus => U::Negate,
            ast::UnaryOp::Tilde => U::Not,
            ast::UnaryOp::Plus => U::Plus,
            _ => return Err(unsupported("BigInt unary operator")),
        };
        self.bigint_checked(
            span,
            Host::BigIntUnary,
            vec![integer(span, op as i32), value],
            Type::BigInt,
        )
    }
    pub(super) fn bigint_binary(
        &mut self,
        op: ast::BinaryOp,
        left: Expr,
        right: Expr,
        span: Span,
    ) -> Result<Expr> {
        let left_ty = left.ty;
        let right_ty = right.ty;
        let lhs = self.hidden(left_ty);
        let rhs = self.hidden(right_ty);
        let mut body = vec![S::Init(lhs, left), S::Init(rhs, right)];
        let a = local(span, lhs, left_ty);
        let b = local(span, rhs, right_ty);
        use ast::BinaryOp as O;
        let equality = matches!(op, O::EqEq | O::EqEqEq | O::NotEq | O::NotEqEq);
        let value = if op == O::Add
            && matches!(
                (left_ty, right_ty),
                (Type::BigInt, Type::String) | (Type::String, Type::BigInt)
            ) {
            let a = if left_ty == Type::BigInt {
                self.bigint_text(a)?
            } else {
                a
            };
            let b = if right_ty == Type::BigInt {
                self.bigint_text(b)?
            } else {
                b
            };
            Expr {
                span,
                ty: Type::String,
                kind: E::Binary(O::Add, Box::new(a), Box::new(b)),
            }
        } else if matches!(op, O::EqEqEq | O::NotEqEq) && left_ty != right_ty {
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Integer(i32::from(op == O::NotEqEq)),
            }
        } else if equality
            && matches!(
                (left_ty, right_ty),
                (Type::BigInt, Type::Null | Type::Undefined)
                    | (Type::Null | Type::Undefined, Type::BigInt)
            )
        {
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Integer(i32::from(matches!(op, O::NotEq | O::NotEqEq))),
            }
        } else if equality || matches!(op, O::Lt | O::LtEq | O::Gt | O::GtEq) {
            let reverse = left_ty != Type::BigInt;
            let (big, other, other_ty) = if reverse {
                (b, a, left_ty)
            } else {
                (a, b, right_ty)
            };
            let comparison = if other_ty == Type::BigInt {
                Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Host(Host::BigIntCompare, vec![big, other]),
                }
            } else if other_ty == Type::String {
                Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Host(Host::BigIntCompareString, vec![big, other]),
                }
            } else {
                let other = match other_ty {
                    Type::Float64 => other,
                    Type::Boolean => Expr {
                        span,
                        ty: Type::Float64,
                        kind: E::Conditional(
                            Box::new(other),
                            Box::new(number(span, 1.0)),
                            Box::new(number(span, 0.0)),
                        ),
                    },
                    Type::Null => number(span, 0.0),
                    Type::Undefined => number(span, f64::NAN),
                    _ => return Err(unsupported("BigInt comparison with this checked type")),
                };
                Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Host(Host::BigIntCompareNumber, vec![big, other]),
                }
            };
            let op = if reverse {
                match op {
                    O::Lt => O::Gt,
                    O::LtEq => O::GtEq,
                    O::Gt => O::Lt,
                    O::GtEq => O::LtEq,
                    other => other,
                }
            } else {
                op
            };
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Binary(op, Box::new(comparison), Box::new(number(span, 0.0))),
            }
        } else if left_ty == Type::BigInt && right_ty == Type::BigInt {
            let operation = match op {
                O::Add => B::Add,
                O::Sub => B::Subtract,
                O::Mul => B::Multiply,
                O::Div => B::Divide,
                O::Mod => B::Remainder,
                O::Exp => B::Power,
                O::BitAnd => B::And,
                O::BitOr => B::Or,
                O::BitXor => B::Xor,
                O::LShift => B::LeftShift,
                O::RShift => B::RightShift,
                O::ZeroFillRShift => B::UnsignedRightShift,
                _ => return Err(unsupported("BigInt binary operator")),
            };
            self.bigint_checked(
                span,
                Host::BigIntBinary,
                vec![integer(span, operation as i32), a, b],
                Type::BigInt,
            )?
        } else {
            body.push(S::Throw(self.bigint_error(span, ErrorClass::TypeError)?));
            Expr {
                span,
                ty: Type::BigInt,
                kind: E::String(vec![0; varta_common::bigint::META_BYTES]),
            }
        };
        let ty = value.ty;
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        body.push(S::Init(output, value));
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![lhs, rhs, output],
                body,
            },
        })
    }
    pub(super) fn bigint_constructor(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        if !self.types.profile.bigint() {
            return Err(unsupported("BigInt requires checked-f64-utf16-v28"));
        }
        let span = call.span;
        let input = if let Some(first) = call.args.first() {
            self.expr(&first.expr)?
        } else {
            Expr {
                span,
                ty: Type::Undefined,
                kind: E::Integer(0),
            }
        };
        let ty = input.ty;
        let slot = self.hidden(ty);
        let mut body = vec![S::Init(slot, input)];
        for arg in call.args.iter().skip(1) {
            body.push(S::Expr(self.expr(&arg.expr)?));
        }
        let value = local(span, slot, ty);
        let result = match ty {
            Type::Union(_) | Type::ReferenceUnion(_)
                if self.types.profile.bigint_values()
                    && unions::members(ty, self.types)
                        .iter()
                        .all(|ty| unions::primitive(*ty) || *ty == Type::BigInt) =>
            {
                self.bigint_checked(span, Host::BigIntFromValue, vec![value], Type::BigInt)?
            }
            Type::BigInt => value,
            Type::String => {
                self.bigint_checked(span, Host::BigIntFromString, vec![value], Type::BigInt)?
            }
            Type::Float64 => {
                self.bigint_checked(span, Host::BigIntFromNumber, vec![value], Type::BigInt)?
            }
            Type::Boolean => self.bigint_checked(
                span,
                Host::BigIntFromNumber,
                vec![Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Conditional(
                        Box::new(value),
                        Box::new(number(span, 1.0)),
                        Box::new(number(span, 0.0)),
                    ),
                }],
                Type::BigInt,
            )?,
            Type::Undefined | Type::Null => {
                body.push(S::Throw(self.bigint_error(span, ErrorClass::TypeError)?));
                Expr {
                    span,
                    ty: Type::BigInt,
                    kind: E::String(vec![0; varta_common::bigint::META_BYTES]),
                }
            }
            _ => return Err(unsupported("BigInt construction from this checked type")),
        };
        let output = self.hidden(Type::BigInt);
        let marker = self.hidden(Type::Number);
        body.push(S::Init(output, result));
        Ok(Expr {
            span,
            ty: Type::BigInt,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![slot, output],
                body,
            },
        })
    }
}

impl Lower<'_> {
    pub(super) fn bigint_method(
        &mut self,
        call: &ast::CallExpr,
        value: Expr,
        value_of: bool,
    ) -> Result<Expr> {
        if !value_of && self.types.profile.bigint_values() {
            return self.bigint_primitive_call(call, Some(value), false);
        }
        let span = call.span;
        let receiver = self.hidden(Type::BigInt);
        let mut body = vec![S::Init(receiver, value)];
        let mut roots = vec![receiver];
        let mut radix = number(span, 10.0);
        for (index, arg) in call.args.iter().enumerate() {
            let value = self.expr(&arg.expr)?;
            if !value_of && index == 0 {
                let ty = value.ty;
                if !matches!(ty, Type::Float64 | Type::Undefined) {
                    return Err(unsupported(
                        "BigInt.toString currently requires a Number/Undefined radix",
                    ));
                }
                let slot = self.hidden(ty);
                roots.push(slot);
                body.push(S::Init(slot, value));
                if ty == Type::Float64 {
                    radix = local(span, slot, ty);
                }
            } else {
                body.push(S::Expr(value));
            }
        }
        let value = local(span, receiver, Type::BigInt);
        let result = if value_of {
            value
        } else {
            self.bigint_checked(span, Host::BigIntToString, vec![value, radix], Type::String)?
        };
        let ty = result.ty;
        let output = self.hidden(ty);
        roots.push(output);
        let marker = self.hidden(Type::Number);
        body.push(S::Init(output, result));
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }
    pub(super) fn bigint_width(&mut self, call: &ast::CallExpr, signed: bool) -> Result<Expr> {
        if !self.types.profile.bigint() {
            return Err(unsupported("BigInt requires checked-f64-utf16-v28"));
        }
        if self.types.profile.bigint_values() {
            return self.bigint_primitive_call(call, None, signed);
        }
        if call.args.len() < 2 {
            return Err(unsupported(
                "BigInt widths currently require explicit Number width and BigInt value",
            ));
        }
        let span = call.span;
        let mut body = Vec::new();
        let mut roots = Vec::new();
        let mut values = Vec::new();
        for (index, arg) in call.args.iter().enumerate() {
            let value = self.expr(&arg.expr)?;
            if index < 2 {
                if value.ty
                    != if index == 0 {
                        Type::Float64
                    } else {
                        Type::BigInt
                    }
                {
                    return Err(unsupported(
                        "BigInt widths currently require Number width and BigInt value",
                    ));
                }
                let slot = self.hidden(value.ty);
                let ty = value.ty;
                roots.push(slot);
                body.push(S::Init(slot, value));
                values.push(local(span, slot, ty));
            } else {
                body.push(S::Expr(value));
            }
        }
        let value = values.pop().unwrap();
        let width = values.pop().unwrap();
        let result = self.bigint_checked(
            span,
            Host::BigIntWidth,
            vec![integer(span, i32::from(signed)), value, width],
            Type::BigInt,
        )?;
        let output = self.hidden(Type::BigInt);
        roots.push(output);
        let marker = self.hidden(Type::Number);
        body.push(S::Init(output, result));
        Ok(Expr {
            span,
            ty: Type::BigInt,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }
    fn bigint_primitive_call(
        &mut self,
        call: &ast::CallExpr,
        receiver: Option<Expr>,
        signed: bool,
    ) -> Result<Expr> {
        let span = call.span;
        let is_method = receiver.is_some();
        let count = if is_method { 1 } else { 2 };
        let mut roots = Vec::new();
        let mut body = Vec::new();
        let mut args = Vec::new();
        if let Some(value) = receiver {
            let slot = self.hidden(Type::BigInt);
            roots.push(slot);
            body.push(S::Init(slot, value));
            args.push(local(span, slot, Type::BigInt));
        } else {
            args.push(integer(span, i32::from(signed)));
        }
        let mut values = Vec::new();
        for (index, arg) in call.args.iter().enumerate() {
            if arg.spread.is_some() {
                return Err(unsupported("BigInt method argument spread"));
            }
            let value = self.expr(&arg.expr)?;
            if index < count {
                let ty = value.ty;
                let slot = self.hidden(ty);
                roots.push(slot);
                body.push(S::Init(slot, value));
                values.push(local(span, slot, ty));
            } else {
                body.push(S::Expr(value));
            }
        }
        while values.len() < count {
            values.push(Expr {
                span,
                ty: Type::Undefined,
                kind: E::Integer(0),
            });
        }
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
        for value in values {
            if unions::members(value.ty, self.types)
                .iter()
                .any(|ty| !unions::primitive(*ty) && *ty != Type::BigInt)
            {
                return Err(unsupported(
                    "BigInt method arguments require primitives; object coercion is unsupported",
                ));
            }
            // Retain each boxed argument before allocating the following cell.
            let slot = self.hidden(cell_ty);
            roots.push(slot);
            body.push(S::Init(
                slot,
                unions::widen(value, Some(cell_ty), self.types)?,
            ));
            args.push(local(span, slot, cell_ty));
        }
        let (host, ty) = if is_method {
            (Host::BigIntToStringV2, Type::String)
        } else {
            (Host::BigIntWidthV2, Type::BigInt)
        };
        let result = self.bigint_checked(span, host, args, ty)?;
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        roots.push(output);
        body.push(S::Init(output, result));
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }
}

impl Lower<'_> {
    pub(super) fn bigint_new(&mut self, new: &ast::NewExpr) -> Result<Expr> {
        let span = new.span;
        let mut body = Vec::new();
        for arg in new.args.as_deref().unwrap_or_default() {
            if arg.spread.is_some() {
                return Err(unsupported("BigInt constructor spread"));
            }
            body.push(S::Expr(self.expr(&arg.expr)?));
        }
        body.push(S::Throw(self.bigint_error(span, ErrorClass::TypeError)?));
        let output = self.hidden(Type::BigInt);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty: Type::BigInt,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![output],
                body,
            },
        })
    }
}
