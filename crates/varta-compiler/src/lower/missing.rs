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
        let mut present = 0u32;
        let mut fields = Vec::new();
        for (name, value) in values {
            let slot = shape
                .fields
                .iter()
                .position(|f| f.name == name)
                .ok_or_else(|| invalid(format!("unknown object initializer field: {name}")))?;
            let bit = 1u32 << slot;
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
            if present & (1u32 << slot) != 0 {
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
            return Err(invalid("Object.hasOwn expects a record and a literal key"));
        }
        let value = self.expr(&call.args[0].expr)?;
        if self.types.profile.sparse_arrays() && matches!(value.ty, Type::Array(_)) {
            let index = match &*call.args[1].expr {
                ast::Expr::Lit(ast::Lit::Str(text)) => {
                    let key = text
                        .value
                        .as_str()
                        .ok_or_else(|| unsupported("noncanonical array own key"))?;
                    let number = key
                        .parse::<u32>()
                        .ok()
                        .filter(|n| n.to_string() == key && *n != u32::MAX);
                    if let Some(number) = number {
                        Expr {
                            span: call.args[1].expr.span(),
                            ty: Type::Float64,
                            kind: E::Float64(number as f64),
                        }
                    } else {
                        return Ok(Expr {
                            span: call.span,
                            ty: Type::Boolean,
                            kind: E::Sequence(
                                Box::new(value),
                                Box::new(Expr {
                                    span: call.span,
                                    ty: Type::Boolean,
                                    kind: E::Integer(i32::from(key == "length")),
                                }),
                            ),
                        });
                    }
                }
                _ => self.expr(&call.args[1].expr)?,
            };
            if index.ty != Type::Float64 {
                return Err(type_error(
                    "array own index must be a number or literal key",
                ));
            }
            return Ok(Expr {
                span: call.span,
                ty: Type::Boolean,
                kind: E::ArrayHasOwn(Box::new(value), Box::new(index)),
            });
        }
        if self.error_value(value.ty) {
            if !matches!(&*call.args[1].expr, ast::Expr::Lit(ast::Lit::Str(_))) {
                return Err(unsupported("dynamic Error own key"));
            }
            let message = records::expression_key(&call.args[1].expr)? == "message";
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
        let value = self.expr(&member.obj)?;
        match value.ty {
            Type::Error | Type::NativeError(_) => Err(unsupported("Error property mutation")),
            Type::Record(_) => self.record_field(member, value, false),
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
            && !element.is_union()
            && !(self.types.profile.reference_unions()
                && matches!(
                    element,
                    Type::Error | Type::NativeError(_) | Type::Record(_) | Type::Array(_)
                ))
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
