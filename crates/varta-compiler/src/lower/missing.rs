//! Presence-aware closed records and nullable primitive reads.
use super::*;
impl Lower<'_> {
    pub(super) fn optional_record(
        &mut self,
        span: swc_common::Span,
        id: u32,
        values: Vec<(String, Expr)>,
    ) -> Result<Expr> {
        let shape = self.types.records[id as usize].clone();
        let mut present = 0u128;
        let mut fields = Vec::new();
        for (name, value) in values {
            let slot = shape
                .fields
                .iter()
                .position(|f| f.name == name)
                .ok_or_else(|| invalid(format!("unknown object initializer field: {name}")))?;
            let bit = 1u128 << slot;
            if present & bit != 0 {
                return Err(unsupported("duplicate object key"));
            }
            present |= bit;
            if value.ty != shape.fields[slot].ty {
                return Err(type_error("optional record field type mismatch"));
            }
            fields.push((slot as u32, value));
        }
        for (slot, field) in shape.fields.iter().enumerate() {
            if present & (1u128 << slot) != 0 {
                continue;
            }
            if !field.optional {
                return Err(type_error(format!(
                    "missing required object field: {}",
                    field.name
                )));
            }
            let value = Expr {
                span,
                ty: Type::Undefined,
                kind: E::Integer(0),
            };
            fields.push((
                slot as u32,
                unions::widen(value, Some(field.ty), self.types)?,
            ));
        }
        Ok(Expr {
            span,
            ty: Type::Record(id),
            kind: E::OptionalRecord(fields, present),
        })
    }
    pub(super) fn own_property(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        if self.binding("Object").is_ok() || self.functions.contains_key("Object") {
            return Err(unsupported("shadowed Object.hasOwn intrinsic"));
        }
        if call.args.len() != 2 {
            return Err(invalid("Object.hasOwn expects two arguments"));
        }
        let value = self.object_value(&call.args[0].expr)?;
        if matches!(value.ty, Type::Dictionary(_)) {
            let key = self.expr(&call.args[1].expr)?;
            return self.dictionary_has(value, key, false);
        }
        if self.types.profile.sparse_arrays() && matches!(value.ty, Type::Array(_)) {
            if let ast::Expr::Lit(ast::Lit::Str(text)) = &*call.args[1].expr {
                // Literal lookups do not need to materialize an unknown key.
                let key = text.value.as_str();
                let number = key.and_then(|key| {
                    key.parse::<u32>()
                        .ok()
                        .filter(|n| n.to_string() == key && *n != u32::MAX)
                });
                let kind = if let Some(number) = number {
                    E::ArrayHasOwn(
                        Box::new(value),
                        Box::new(Expr {
                            span: call.args[1].expr.span(),
                            ty: Type::Float64,
                            kind: E::Float64(number as f64),
                        }),
                    )
                } else {
                    E::Sequence(
                        Box::new(value),
                        Box::new(Expr {
                            span: call.span,
                            ty: Type::Boolean,
                            kind: E::Integer(i32::from(key == Some("length"))),
                        }),
                    )
                };
                return Ok(Expr {
                    span: call.span,
                    ty: Type::Boolean,
                    kind,
                });
            }
            let key = self.expr(&call.args[1].expr)?;
            let kind = match key.ty {
                Type::String => E::ArrayStringHasOwn(Box::new(value), Box::new(key)),
                Type::Float64 => E::ArrayHasOwn(Box::new(value), Box::new(key)),
                _ => return Err(self.source.locate(type_error(
                    "array own key must be a narrowed string or number; coercion is unsupported",
                ), key.span)),
            };
            return Ok(Expr {
                span: call.span,
                ty: Type::Boolean,
                kind,
            });
        }
        if self.error_value(value.ty) {
            let message =
                match &*call.args[1].expr {
                    ast::Expr::Lit(ast::Lit::Str(text)) if self.types.profile.closures() => {
                        text.value.as_str() == Some("message")
                    }
                    _ if self.types.profile.closures() => {
                        let key = self.expr(&call.args[1].expr)?;
                        if key.ty != Type::String {
                            return Err(self.source.locate(type_error(
                            "Error own key must be a narrowed string; coercion is unsupported",
                        ), key.span));
                        }
                        return Ok(self.dynamic_error_own(call.span, value, key));
                    }
                    ast::Expr::Lit(ast::Lit::Str(_)) => {
                        records::expression_key(&call.args[1].expr)? == "message"
                    }
                    _ => return Err(unsupported("dynamic Error own key")),
                };
            return Ok(Expr {
                span: call.span,
                ty: Type::Boolean,
                kind: if message {
                    E::ErrorOwn(Box::new(value))
                } else {
                    E::Sequence(
                        Box::new(value),
                        Box::new(Expr {
                            span: call.span,
                            ty: Type::Boolean,
                            kind: E::Integer(0),
                        }),
                    )
                },
            });
        }
        let Type::Record(id) = value.ty else {
            return Err(unsupported("Object.hasOwn on a non-record value"));
        };
        if self.types.profile.closures() {
            match &*call.args[1].expr {
                ast::Expr::Lit(ast::Lit::Str(text)) => {
                    // Lookup keys do not create a shape. Unrepresentable names,
                    // including lone surrogates, simply cannot match a field.
                    let slot = text.value.as_str().and_then(|name| {
                        self.types.records[id as usize]
                            .fields
                            .iter()
                            .position(|field| field.name == name)
                            .map(|slot| slot as u32)
                    });
                    return Ok(Expr {
                        span: call.span,
                        ty: Type::Boolean,
                        kind: E::HasOwn(Box::new(value), slot),
                    });
                }
                ast::Expr::Lit(ast::Lit::Num(_)) => {}
                _ => {
                    let key = self.expr(&call.args[1].expr)?;
                    if key.ty != Type::String {
                        return Err(self.source.locate(type_error("dynamic record own key must be a narrowed string; coercion is unsupported"), key.span));
                    }
                    return Ok(self.dynamic_record_own(call.span, value, key, id));
                }
            }
        }
        if !matches!(
            &*call.args[1].expr,
            ast::Expr::Lit(ast::Lit::Str(_) | ast::Lit::Num(_))
        ) {
            return Err(unsupported("dynamic Object.hasOwn key"));
        }
        let name = records::expression_key(&call.args[1].expr)?;
        let slot = self.types.records[id as usize]
            .fields
            .iter()
            .position(|f| f.name == name)
            .map(|slot| slot as u32);
        Ok(Expr {
            span: call.span,
            ty: Type::Boolean,
            kind: E::HasOwn(Box::new(value), slot),
        })
    }
    pub(super) fn place(&mut self, member: &ast::MemberExpr) -> Result<Expr> {
        let value = self.write_member_base(member)?;
        self.place_value(member, value)
    }
    pub(super) fn place_value(&mut self, member: &ast::MemberExpr, value: Expr) -> Result<Expr> {
        match value.ty {
            Type::Error | Type::NativeError(_) => Err(unsupported("Error property mutation")),
            Type::Record(_) => self.record_field(member, value, false),
            Type::Class(_) => self.class_member(member, value, false),
            Type::Array(_) => self.array_member(member, value, false),
            _ => Err(unsupported(
                "assignment to immutable result property or array length",
            )),
        }
    }
    pub(super) fn maybe_index(
        &mut self,
        member: &ast::MemberExpr,
        value: Expr,
        index: &ast::Expr,
        element: Type,
    ) -> Result<Expr> {
        if !unions::primitive(element)
            && !(self.types.profile.bigint_values() && element == Type::BigInt)
            && !element.is_cell()
            && !(self.types.profile.reference_unions()
                && matches!(
                    element,
                    Type::Error
                        | Type::NativeError(_)
                        | Type::Record(_)
                        | Type::Dictionary(_)
                        | Type::Class(_)
                        | Type::Array(_)
                ))
            && !(self.types.profile.promise_values() && matches!(element, Type::Promise(_)))
            && !(self.types.profile.function_fields() && matches!(element, Type::Function(_)))
        {
            return Err(unsupported(
                "nullable reads of object/array elements require reference unions",
            ));
        }
        let index = self.expr(index)?;
        if index.ty != Type::Float64 {
            return Err(type_error(
                "index must be a number; property-key coercion is unsupported",
            ));
        }
        let ty = unions::intern(vec![element, Type::Undefined], self.types)?;
        Ok(Expr {
            span: member.span,
            ty,
            kind: E::MaybeIndex(Box::new(value), Box::new(index)),
        })
    }
}
