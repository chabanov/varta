//! Noncoercive arithmetic reads the prepared old value before RHS effects.
use super::*;

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn matches(span: swc_common::Span, slot: u32, ty: Type, member: Type) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: if ty.is_union() {
            E::IsUnionMember(Box::new(local(span, slot, ty)), member)
        } else {
            E::Integer(i32::from(ty == member))
        },
    }
}
fn scalar(span: swc_common::Span, slot: u32, ty: Type, member: Type) -> Expr {
    if ty.is_union() {
        Expr {
            span,
            ty: member,
            kind: E::UnboxUnion(Box::new(local(span, slot, ty))),
        }
    } else {
        local(span, slot, member)
    }
}

impl Lower<'_> {
    pub(super) fn compound_assignment_value(&mut self, assign: &ast::AssignExpr) -> Result<Expr> {
        let op = match assign.op {
            ast::AssignOp::AddAssign => ast::BinaryOp::Add,
            ast::AssignOp::SubAssign => ast::BinaryOp::Sub,
            ast::AssignOp::MulAssign => ast::BinaryOp::Mul,
            ast::AssignOp::DivAssign => ast::BinaryOp::Div,
            ast::AssignOp::ModAssign => ast::BinaryOp::Mod,
            _ => {
                return Err(unsupported(
                    "compound assignment operator other than +=, -=, *=, /= or %=",
                ));
            }
        };
        if !self.types.profile.closures() {
            return Err(unsupported(
                "arithmetic compound assignments require checked-f64-utf16-v12",
            ));
        }
        let target = match &assign.left {
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(name)) => self
                .prepare_pattern_target(&ast::Pat::Ident(name.clone()))?
                .ok_or_else(|| invalid("missing compound assignment binding"))?,
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(member)) => {
                let base = self.expr(&member.obj)?;
                if let Type::Record(shape) = base.ty
                    && let ast::MemberProp::Computed(key) = &member.prop
                    && !matches!(
                        &*key.expr,
                        ast::Expr::Lit(ast::Lit::Str(_) | ast::Lit::Num(_))
                    )
                {
                    return self.dynamic_record_compound_value(assign, base, &key.expr, shape, op);
                }
                let field = self.place_value(member, base)?;
                self.prepare_member_reference(field, member.span)?
            }
            _ => return Err(unsupported("compound assignment target")),
        };
        self.prepared_compound_value(target, &assign.right, op, assign.span)
    }
    fn dynamic_record_compound_value(
        &mut self,
        assign: &ast::AssignExpr,
        base: Expr,
        key: &ast::Expr,
        shape: u32,
        op: ast::BinaryOp,
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
        // Retain the reference outside both nested blocks. The read happens
        // before RHS effects; the checked store uses the same receiver/key.
        let old = self.dynamic_record_read(
            assign.left.span(),
            local(span, source, record),
            local(span, name, Type::String),
            shape,
        )?;
        let computed = self.compound_operation(old, &assign.right, op, span)?;
        let value = self.checked_record_write_value(
            span,
            local(span, source, record),
            local(span, name, Type::String),
            computed,
            shape,
        )?;
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
    pub(super) fn compound_operation(
        &mut self,
        old: Expr,
        rhs: &ast::Expr,
        op: ast::BinaryOp,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let left = unions::members(old.ty, self.types);
        let candidates: Vec<_> = [Type::Float64, Type::String]
            .into_iter()
            .filter(|ty| left.contains(ty) && (*ty == Type::Float64 || op == ast::BinaryOp::Add))
            .collect();
        if candidates.is_empty() {
            return Err(self.source.locate(
                type_error("compound arithmetic requires a Number target or String target for +="),
                old.span,
            ));
        }
        let value = self.expr(rhs)?;
        let right = unions::members(value.ty, self.types);
        let cases: Vec<_> = candidates
            .into_iter()
            .filter(|ty| right.contains(ty))
            .collect();
        if cases.is_empty() {
            return Err(self.source.locate(type_error("compound arithmetic requires matching Number operands, or matching Strings for +=; coercion is unsupported"),value.span));
        }
        let ty = unions::intern(cases.clone(), self.types)?;
        let old_ty = old.ty;
        let rhs_ty = value.ty;
        let input = self.hidden(old_ty);
        let rhs_input = self.hidden(rhs_ty);
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        let error = self.array_error(
            varta_common::value::ErrorClass::TypeError,
            "Invalid compound arithmetic operands",
            span,
        )?;
        let mut branch = vec![S::Throw(error)];
        for member in cases.iter().rev().copied() {
            let test = Expr {
                span,
                ty: Type::Boolean,
                kind: E::Conditional(
                    Box::new(matches(span, input, old_ty, member)),
                    Box::new(matches(span, rhs_input, rhs_ty, member)),
                    Box::new(Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Integer(0),
                    }),
                ),
            };
            let result = Expr {
                span,
                ty: member,
                kind: E::Binary(
                    op,
                    Box::new(scalar(span, input, old_ty, member)),
                    Box::new(scalar(span, rhs_input, rhs_ty, member)),
                ),
            };
            branch = vec![S::If(
                test,
                vec![S::Set(output, unions::widen(result, Some(ty), self.types)?)],
                branch,
            )];
        }
        // The nonempty cases check above guarantees this canonical initial kind.
        let initial = Expr {
            span,
            ty: cases[0],
            kind: if cases[0] == Type::Float64 {
                E::Float64(0.0)
            } else {
                E::String(vec![])
            },
        };
        let mut body = vec![
            S::Init(input, old),
            S::Init(rhs_input, value),
            S::Init(output, unions::widen(initial, Some(ty), self.types)?),
        ];
        body.extend(branch);
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![input, rhs_input, output],
                body,
            },
        })
    }
}
