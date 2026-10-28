//! V30 deletes cached own references while preserving closed storage layouts.
use super::*;
fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn yes(span: swc_common::Span) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Integer(1),
    }
}
impl Lower<'_> {
    pub(super) fn property_delete(
        &mut self,
        member: &ast::MemberExpr,
        base: Expr,
        span: swc_common::Span,
    ) -> Result<Expr> {
        if !matches!(base.ty, Type::Record(_) | Type::Array(_)) {
            return Err(type_error(
                "delete requires a narrowed record or array receiver",
            ));
        }
        let key = match &member.prop {
            ast::MemberProp::Ident(id) => Expr {
                span,
                ty: Type::String,
                kind: E::String(self.types.profile.string_bytes(&id.sym)),
            },
            ast::MemberProp::Computed(key) => self.expr(&key.expr)?,
            _ => return Err(unsupported("private delete key")),
        };
        if matches!(base.ty, Type::Array(_)) && key.ty == Type::Float64 {
            return Ok(Expr {
                span,
                ty: Type::Boolean,
                kind: E::ArrayDelete {
                    array: Box::new(base),
                    index: Some(Box::new(key)),
                    error: None,
                },
            });
        }
        let key = if key.ty == Type::String {
            key
        } else {
            if unions::members(key.ty, self.types)
                .iter()
                .any(|ty| !unions::primitive(*ty) && *ty != Type::BigInt)
            {
                return Err(unsupported("object ToPropertyKey in delete"));
            }
            let cell = unions::intern(
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
            Expr {
                span,
                ty: Type::String,
                kind: E::Host(
                    Host::PrimitiveStringV2,
                    vec![unions::widen(key, Some(cell), self.types)?],
                ),
            }
        };
        let ty = base.ty;
        let object = self.hidden(ty);
        let name = self.hidden(Type::String);
        let output = self.hidden(Type::Boolean);
        let marker = self.hidden(Type::Number);
        let mut body = vec![
            S::Init(object, base),
            S::Init(name, key),
            S::Init(output, yes(span)),
        ];
        if let Type::Record(shape) = ty {
            let fields = self.types.records[shape as usize].fields.clone();
            let mut entries = Vec::new();
            for (slot, field) in fields.into_iter().enumerate() {
                let action = if field.optional {
                    let absent = unions::widen(
                        Expr {
                            span,
                            ty: Type::Undefined,
                            kind: E::Integer(0),
                        },
                        Some(field.ty),
                        self.types,
                    )?;
                    let clear = Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::RecordDelete {
                            object: Box::new(local(span, object, ty)),
                            absent: Box::new(absent),
                            slot: slot as u32,
                            scan: self.hidden(Type::Number),
                            write: self.hidden(Type::Number),
                            count: self.hidden(Type::Number),
                        },
                    };
                    vec![S::Set(
                        output,
                        Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::Conditional(
                                Box::new(Expr {
                                    span,
                                    ty: Type::Boolean,
                                    kind: E::HasOwn(
                                        Box::new(local(span, object, ty)),
                                        Some(slot as u32),
                                    ),
                                }),
                                Box::new(clear),
                                Box::new(yes(span)),
                            ),
                        },
                    )]
                } else {
                    vec![S::Throw(self.array_error(
                        varta_common::value::ErrorClass::TypeError,
                        "Cannot delete required checked record field",
                        span,
                    )?)]
                };
                entries.push((field.name, action));
            }
            entries.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
            let mut entries = entries
                .into_iter()
                .map(|(key, body)| (self.types.profile.string_bytes(&key), body))
                .collect::<Vec<_>>();
            body.extend(own_keys::lookup(span, name, &mut entries));
        } else {
            let length = Expr {
                span,
                ty: Type::Boolean,
                kind: E::Binary(
                    ast::BinaryOp::EqEqEq,
                    Box::new(local(span, name, Type::String)),
                    Box::new(Expr {
                        span,
                        ty: Type::String,
                        kind: E::String(self.types.profile.string_bytes("length")),
                    }),
                ),
            };
            let error = self.array_error(
                varta_common::value::ErrorClass::TypeError,
                "Cannot delete array length",
                span,
            )?;
            let number = self.checked_update_numeric(local(span, name, Type::String))?;
            let delete = Expr {
                span,
                ty: Type::Boolean,
                kind: E::ArrayDelete {
                    array: Box::new(local(span, object, ty)),
                    index: Some(Box::new(number)),
                    error: None,
                },
            };
            body.push(S::If(
                length,
                vec![S::Throw(error)],
                vec![S::If(
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::ArrayStringHasOwn(
                            Box::new(local(span, object, ty)),
                            Box::new(local(span, name, Type::String)),
                        ),
                    },
                    vec![S::Set(output, delete)],
                    vec![],
                )],
            ));
        }
        Ok(Expr {
            span,
            ty: Type::Boolean,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![object, name, output],
                body,
            },
        })
    }
}
