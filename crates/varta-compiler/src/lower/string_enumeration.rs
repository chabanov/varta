//! UTF16 boxed-string own indices, using only existing guest array/string helpers.
use super::object_enumeration::Kind;
use super::*;
fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn number(span: swc_common::Span, n: f64) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Float64(n),
    }
}
impl Lower<'_> {
    pub(super) fn empty_primitive_enumeration(
        &mut self,
        value: Expr,
        kind: Kind,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let ty = value.ty;
        let input = self.hidden(ty);
        let mut body = vec![S::Init(input, value)];
        if unions::members(ty, self.types)
            .iter()
            .any(|t| matches!(t, Type::Null | Type::Undefined))
        {
            let error = self.array_error(
                varta_common::value::ErrorClass::TypeError,
                "Cannot convert undefined or null to object",
                span,
            )?;
            body.push(S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsNullish(Box::new(local(span, input, ty))),
                },
                vec![S::Throw(error)],
                vec![],
            ));
        }
        let element = match kind {
            Kind::Keys => Type::String,
            Kind::Values => Type::Undefined,
        };
        let result = arrays::intern(element, self.types)?;
        let output = self.hidden(result);
        let marker = self.hidden(Type::Number);
        body.push(S::Init(
            output,
            Expr {
                span,
                ty: result,
                kind: E::Array(vec![]),
            },
        ));
        Ok(Expr {
            span,
            ty: result,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![input, output],
                body,
            },
        })
    }
    pub(super) fn mixed_primitive_enumeration(
        &mut self,
        value: Expr,
        kind: Kind,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let ty = value.ty;
        let input = self.hidden(ty);
        let result = arrays::intern(Type::String, self.types)?;
        let output = self.hidden(result);
        let marker = self.hidden(Type::Number);
        let mut body = vec![S::Init(input, value)];
        if unions::members(ty, self.types)
            .iter()
            .any(|t| matches!(t, Type::Null | Type::Undefined))
        {
            let error = self.array_error(
                varta_common::value::ErrorClass::TypeError,
                "Cannot convert undefined or null to object",
                span,
            )?;
            body.push(S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsNullish(Box::new(local(span, input, ty))),
                },
                vec![S::Throw(error)],
                vec![],
            ));
        }
        let text = Expr {
            span,
            ty: Type::String,
            kind: E::UnboxUnion(Box::new(local(span, input, ty))),
        };
        let values = self.string_enumeration(text, kind, span)?;
        body.push(S::Init(
            output,
            Expr {
                span,
                ty: result,
                kind: E::Conditional(
                    Box::new(Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::IsUnionMember(Box::new(local(span, input, ty)), Type::String),
                    }),
                    Box::new(values),
                    Box::new(Expr {
                        span,
                        ty: result,
                        kind: E::Array(vec![]),
                    }),
                ),
            },
        ));
        Ok(Expr {
            span,
            ty: result,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![input, output],
                body,
            },
        })
    }
    pub(super) fn string_enumeration(
        &mut self,
        value: Expr,
        kind: Kind,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let result = arrays::intern(Type::String, self.types)?;
        let (source, mut roots, mut body) = self.enumeration_source(value, Type::String, span)?;
        let output = self.hidden(result);
        let index = self.hidden(Type::Float64);
        let length = self.hidden(Type::Float64);
        let marker = self.hidden(Type::Number);
        roots.extend([output, index, length]);
        body.push(S::Init(
            length,
            Expr {
                span,
                ty: Type::Float64,
                kind: E::Length(Box::new(local(span, source, Type::String))),
            },
        ));
        let error = self.array_error(
            varta_common::value::ErrorClass::RangeError,
            "String enumeration exceeds checked array capacity",
            span,
        )?;
        body.push(S::If(
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Binary(
                    ast::BinaryOp::Gt,
                    Box::new(local(span, length, Type::Float64)),
                    Box::new(number(span, varta_common::value::MAX_ARRAY_ELEMENTS as f64)),
                ),
            },
            vec![S::Throw(error)],
            vec![],
        ));
        body.extend([
            S::Init(
                output,
                Expr {
                    span,
                    ty: result,
                    kind: E::Array(vec![]),
                },
            ),
            S::Init(index, number(span, 0.0)),
        ]);
        let next = || Expr {
            span,
            ty: Type::Float64,
            kind: E::Binary(
                ast::BinaryOp::Add,
                Box::new(local(span, index, Type::Float64)),
                Box::new(number(span, 1.0)),
            ),
        };
        let character = Expr {
            span,
            ty: Type::String,
            kind: match kind {
                Kind::Keys => E::String(vec![]),
                Kind::Values => E::StringRange {
                    value: Box::new(local(span, source, Type::String)),
                    start: Box::new(local(span, index, Type::Float64)),
                    end: Box::new(next()),
                    substring: false,
                },
            },
        };
        body.push(S::While(
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Binary(
                    ast::BinaryOp::Lt,
                    Box::new(local(span, index, Type::Float64)),
                    Box::new(local(span, length, Type::Float64)),
                ),
            },
            vec![
                S::Expr(Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::ArrayPush(
                        Box::new(local(span, output, result)),
                        Box::new(Expr {
                            span,
                            ty: result,
                            kind: E::Array(vec![Some(character)]),
                        }),
                    ),
                }),
                S::Set(index, next()),
            ],
        ));
        if matches!(kind, Kind::Keys) {
            body.push(S::Set(
                output,
                Expr {
                    span,
                    ty: result,
                    kind: E::ArrayEnumerate {
                        value: Box::new(local(span, output, result)),
                        keys: true,
                    },
                },
            ));
        }
        Ok(Expr {
            span,
            ty: result,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }
}
