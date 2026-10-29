//! Logical assignment reads once and executes the checked store in one branch.
use super::*;

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn always_truthy(ty: Type) -> bool {
    matches!(
        ty,
        Type::Record(_)
            | Type::Dictionary(_)
            | Type::Class(_)
            | Type::Array(_)
            | Type::Function(_)
            | Type::Error
            | Type::NativeError(_)
    )
}
fn skipped_member(ty: Type, op: ast::AssignOp) -> bool {
    if op == ast::AssignOp::AndAssign {
        !always_truthy(ty)
    } else {
        !matches!(ty, Type::Null | Type::Undefined)
    }
}

impl Lower<'_> {
    pub(super) fn logical_assignment_value(&mut self, assign: &ast::AssignExpr) -> Result<Expr> {
        if !self.types.profile.closures() {
            return Err(unsupported(
                "logical assignments require checked-f64-utf16-v12",
            ));
        }
        let target = match &assign.left {
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(name)) => self
                .prepare_pattern_target(&ast::Pat::Ident(name.clone()))?
                .ok_or_else(|| invalid("missing logical assignment binding"))?,
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(member)) => {
                let base = self.write_member_base(member)?;
                if self.types.profile.mixed_index_records()
                    && matches!(base.ty, Type::Dictionary(_))
                {
                    let update = dictionary_updates::Update::Logical(assign.op, &assign.right);
                    return self.dictionary_update(member, base, update, assign.span);
                }
                if let Type::Record(shape) = base.ty
                    && let ast::MemberProp::Computed(key) = &member.prop
                    && !matches!(
                        &*key.expr,
                        ast::Expr::Lit(ast::Lit::Str(_) | ast::Lit::Num(_))
                    )
                {
                    return self.dynamic_record_logical_value(assign, base, &key.expr, shape);
                }
                let field = self.place_value(member, base)?;
                self.prepare_member_reference(field, member.span)?
            }
            _ => return Err(unsupported("logical assignment target")),
        };
        self.prepared_logical_value(target, &assign.right, assign.op, assign.span)
    }
    fn dynamic_record_logical_value(
        &mut self,
        assign: &ast::AssignExpr,
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
        let span = assign.span;
        let record = Type::Record(shape);
        let source = self.hidden(record);
        let name = self.hidden(Type::String);
        let old = self.dynamic_record_read(
            assign.left.span(),
            local(span, source, record),
            local(span, name, Type::String),
            shape,
        )?;
        let saved = self.flow_snapshot();
        let value = self.record_assignment_rhs(&assign.right, shape)?;
        self.flow_restore(&saved);
        let store = self.checked_record_write_value(
            span,
            local(span, source, record),
            local(span, name, Type::String),
            value,
            shape,
        )?;
        let value = self.logical_assignment_operation(old, store, assign.op, span)?;
        let ty = value.ty;
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![source, name, output],
                body: vec![
                    S::Init(source, base),
                    S::Init(name, key),
                    S::Init(output, value),
                ],
            },
        })
    }
    pub(super) fn logical_assignment_operation(
        &mut self,
        old: Expr,
        store: Expr,
        op: ast::AssignOp,
        span: swc_common::Span,
    ) -> Result<Expr> {
        if old.ty.width() != 1 || store.ty.width() != 1 {
            return Err(self
                .source
                .locate(unsupported("logical assignment of result pairs"), old.span));
        }
        let old_ty = old.ty;
        let input = self.hidden(old_ty);
        let skipped: Vec<_> = unions::members(old_ty, self.types)
            .into_iter()
            .filter(|ty| skipped_member(*ty, op))
            .collect();
        let value = if skipped.is_empty() {
            store
        } else {
            let skipped_ty = if skipped.len() == 1 {
                skipped[0]
            } else {
                unions::intern(skipped, self.types)?
            };
            let saved = if old_ty.is_union() && !skipped_ty.is_union() {
                Expr {
                    span,
                    ty: skipped_ty,
                    kind: E::UnboxUnion(Box::new(local(span, input, old_ty))),
                }
            } else {
                local(span, input, skipped_ty)
            };
            let (ty, store, saved) = if store.ty == saved.ty {
                (store.ty, store, saved)
            } else {
                self.join(store, saved)?
            };
            let test = if op == ast::AssignOp::NullishAssign {
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsNullish(Box::new(local(span, input, old_ty))),
                }
            } else {
                let truth = if always_truthy(old_ty) {
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Integer(1),
                    }
                } else {
                    self.truth(local(span, input, old_ty))?
                };
                if op == ast::AssignOp::OrAssign {
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Not(Box::new(truth)),
                    }
                } else {
                    truth
                }
            };
            Expr {
                span,
                ty,
                kind: E::Conditional(Box::new(test), Box::new(store), Box::new(saved)),
            }
        };
        let ty = value.ty;
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![input, output],
                body: vec![S::Init(input, old), S::Init(output, value)],
            },
        })
    }
}
