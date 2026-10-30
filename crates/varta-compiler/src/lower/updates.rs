//! Checked numeric updates return old/new values only after a successful store.
use super::*;

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn next(span: swc_common::Span, input: Expr, op: ast::UpdateOp) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Binary(
            match op {
                ast::UpdateOp::PlusPlus => ast::BinaryOp::Add,
                ast::UpdateOp::MinusMinus => ast::BinaryOp::Sub,
            },
            Box::new(input),
            Box::new(Expr {
                span,
                ty: Type::Float64,
                kind: E::Float64(1.0),
            }),
        ),
    }
}
impl Lower<'_> {
    pub(super) fn update_value(&mut self, update: &ast::UpdateExpr) -> Result<Expr> {
        if let Some(value) = self.global_update(update)? {
            return Ok(value);
        }
        if !self.types.profile.closures() {
            return Err(unsupported(
                "increment/decrement require checked-f64-utf16-v12",
            ));
        }
        let target = match stmt::unparen(&update.arg) {
            ast::Expr::Ident(name) => {
                self.require_assigned_target(name.sym.as_ref())?;
                self.prepare_pattern_target(&ast::Pat::Ident(ast::BindingIdent {
                    id: name.clone(),
                    type_ann: None,
                }))?
                .ok_or_else(|| invalid("missing update binding"))?
            }
            ast::Expr::Member(member) => {
                let base = self.write_member_base(member)?;
                if self.types.profile.mixed_index_records()
                    && matches!(base.ty, Type::Dictionary(_))
                {
                    let step = dictionary_updates::Update::Step(update);
                    return self.dictionary_update(member, base, step, update.span);
                }
                if let Type::Record(shape) = base.ty
                    && let ast::MemberProp::Computed(key) = &member.prop
                    && !matches!(
                        &*key.expr,
                        ast::Expr::Lit(ast::Lit::Str(_) | ast::Lit::Num(_))
                    )
                {
                    return self.dynamic_record_update_value(update, base, &key.expr, shape);
                }
                let field = self.place_value(member, base)?;
                self.prepare_member_reference(field, member.span)?
            }
            _ => return Err(unsupported("increment/decrement target")),
        };
        self.prepared_update_value(target, update)
    }
    fn dynamic_record_update_value(
        &mut self,
        update: &ast::UpdateExpr,
        base: Expr,
        key: &ast::Expr,
        shape: u32,
    ) -> Result<Expr> {
        let key = self.expr(key)?;
        if key.ty != Type::String {
            return Err(self.source.locate(
                type_error(
                    "computed record key must be a narrowed string; coercion is unsupported",
                ),
                key.span,
            ));
        }
        let span = update.span;
        let record = Type::Record(shape);
        let source = self.hidden(record);
        let name = self.hidden(Type::String);
        let read = self.dynamic_record_read(
            update.arg.span(),
            local(span, source, record),
            local(span, name, Type::String),
            shape,
        )?;
        let old = self.checked_update_numeric(read)?;
        let input_ty = old.ty;
        let input = self.hidden(input_ty);
        let next = self.update_next(span, input, input_ty, update.op)?;
        let expected = if self.types.profile.bigint_values() {
            let fields = &self.types.records[shape as usize].fields;
            let numeric: Vec<_> = [Type::Float64, Type::BigInt]
                .into_iter()
                .filter(|ty| {
                    fields
                        .iter()
                        .any(|field| arguments::accepts(field.ty, *ty, self.types))
                })
                .collect();
            if numeric.is_empty() {
                return Err(self.source.locate(
                    type_error("numeric update result matches no declared record field type"),
                    span,
                ));
            }
            unions::intern(numeric, self.types)?
        } else {
            next.ty
        };
        let next = self.checked_assignment_result(next, expected)?;
        let ty = next.ty;
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        let store = self.checked_record_write_value(
            span,
            local(span, source, record),
            local(span, name, Type::String),
            next,
            shape,
        )?;
        let mut body = vec![
            S::Init(source, base),
            S::Init(name, key),
            S::Init(input, old),
            S::Init(output, store),
        ];
        if !update.prefix && input_ty != ty {
            body.push(S::Set(
                output,
                Expr {
                    span,
                    ty,
                    kind: E::UnboxUnion(Box::new(local(span, input, input_ty))),
                },
            ));
        }
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output: if update.prefix || input_ty != ty {
                    output
                } else {
                    input
                },
                marker,
                roots: vec![source, name, input, output],
                body,
            },
        })
    }
    pub(super) fn checked_update_numeric(&mut self, value: Expr) -> Result<Expr> {
        if self.types.profile.bigint_values() {
            return self.primitive_update_numeric(value);
        }
        let members = unions::members(value.ty, self.types);
        let candidates: Vec<_> = [Type::Float64]
            .into_iter()
            .filter(|ty| members.contains(ty))
            .collect();
        if candidates.is_empty() {
            return Err(self.source.locate(
                type_error("increment/decrement require a Number target; coercion is unsupported"),
                value.span,
            ));
        }
        if candidates.contains(&value.ty) {
            return Ok(value);
        }
        let span = value.span;
        let ty = value.ty;
        let result_ty = unions::intern(candidates.clone(), self.types)?;
        let input = self.hidden(ty);
        let output = self.hidden(result_ty);
        let marker = self.hidden(Type::Number);
        let error = self.array_error(
            varta_common::value::ErrorClass::TypeError,
            "Invalid update operand",
            span,
        )?;
        let mut branch = vec![S::Throw(error)];
        for member in candidates.into_iter().rev() {
            let number = Expr {
                span,
                ty: member,
                kind: E::UnboxUnion(Box::new(local(span, input, ty))),
            };
            let test = Expr {
                span,
                ty: Type::Boolean,
                kind: E::IsUnionMember(Box::new(local(span, input, ty)), member),
            };
            branch = vec![S::If(
                test,
                vec![S::Init(
                    output,
                    unions::widen(number, Some(result_ty), self.types)?,
                )],
                branch,
            )];
        }
        let mut body = vec![S::Init(input, value)];
        body.extend(branch);
        Ok(Expr {
            span,
            ty: result_ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![input, output],
                body,
            },
        })
    }
    fn primitive_update_numeric(&mut self, value: Expr) -> Result<Expr> {
        if matches!(value.ty, Type::Float64 | Type::BigInt) {
            return Ok(value);
        }
        let span = value.span;
        if unions::primitive(value.ty) {
            return self.update_number(value);
        }
        let members = unions::members(value.ty, self.types);
        let admitted: Vec<_> = members
            .into_iter()
            .filter(|ty| unions::primitive(*ty) || *ty == Type::BigInt)
            .collect();
        if admitted.is_empty() {
            return Err(self.source.locate(
                unsupported("object ToPrimitive in increment/decrement"),
                span,
            ));
        }
        let mut numeric = Vec::new();
        if admitted.contains(&Type::BigInt) {
            numeric.push(Type::BigInt);
        }
        if admitted.iter().any(|ty| *ty != Type::BigInt) {
            numeric.push(Type::Float64);
        }
        let result_ty = unions::intern(numeric, self.types)?;
        let ty = value.ty;
        let input = self.hidden(ty);
        let output = self.hidden(result_ty);
        let marker = self.hidden(Type::Number);
        let error = self.array_error(
            varta_common::value::ErrorClass::TypeError,
            "Object ToPrimitive in increment/decrement is unsupported",
            span,
        )?;
        let mut branch = vec![S::Throw(error)];
        for member in admitted.into_iter().rev() {
            let scalar = Expr {
                span,
                ty: member,
                kind: E::UnboxUnion(Box::new(local(span, input, ty))),
            };
            let numeric = if member == Type::BigInt {
                scalar
            } else {
                self.update_number(scalar)?
            };
            branch = vec![S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsUnionMember(Box::new(local(span, input, ty)), member),
                },
                vec![S::Init(
                    output,
                    unions::widen(numeric, Some(result_ty), self.types)?,
                )],
                branch,
            )];
        }
        let mut body = vec![S::Init(input, value)];
        body.extend(branch);
        Ok(Expr {
            span,
            ty: result_ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![input, output],
                body,
            },
        })
    }
    fn update_number(&mut self, value: Expr) -> Result<Expr> {
        if value.ty == Type::Float64 {
            return Ok(value);
        }
        let span = value.span;
        let ty = unions::intern(
            vec![
                Type::Float64,
                Type::String,
                Type::Boolean,
                Type::Null,
                Type::Undefined,
            ],
            self.types,
        )?;
        Ok(Expr {
            span,
            ty: Type::Float64,
            kind: E::Host(
                self.types.profile.host_operation(Host::PrimitiveNumberV2),
                vec![unions::widen(value, Some(ty), self.types)?],
            ),
        })
    }
    pub(super) fn update_next(
        &mut self,
        span: swc_common::Span,
        input: u32,
        ty: Type,
        op: ast::UpdateOp,
    ) -> Result<Expr> {
        if ty == Type::Float64 {
            return Ok(next(span, local(span, input, ty), op));
        }
        let mut budget = varta_bigint::Budget::new(u64::from(u32::BITS), varta_common::abi::FUEL);
        let payload = varta_bigint::Integer::from_i64(1, &mut budget)
            .and_then(|n| n.to_payload(&mut budget))
            .map_err(|e| invalid(e.to_string()))?;
        let bigint = if ty.is_union() {
            Expr {
                span,
                ty: Type::BigInt,
                kind: E::UnboxUnion(Box::new(local(span, input, ty))),
            }
        } else {
            local(span, input, ty)
        };
        let incremented = self.bigint_binary(
            match op {
                ast::UpdateOp::PlusPlus => ast::BinaryOp::Add,
                ast::UpdateOp::MinusMinus => ast::BinaryOp::Sub,
            },
            bigint,
            Expr {
                span,
                ty: Type::BigInt,
                kind: E::String(payload),
            },
            span,
        )?;
        if ty == Type::BigInt {
            return Ok(incremented);
        }
        // Input is already cached and contains exactly Number|BigInt. Select
        // the matching unit instead of mixing a Number 1 with a BigInt value.
        let number = Expr {
            span,
            ty: Type::Float64,
            kind: E::UnboxUnion(Box::new(local(span, input, ty))),
        };
        let number = next(span, number, op);
        Ok(Expr {
            span,
            ty,
            kind: E::Conditional(
                Box::new(Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsUnionMember(Box::new(local(span, input, ty)), Type::BigInt),
                }),
                Box::new(unions::widen(incremented, Some(ty), self.types)?),
                Box::new(unions::widen(number, Some(ty), self.types)?),
            ),
        })
    }
}
