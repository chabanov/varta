//! Simple assignment expressions share rooted prepared references with patterns.
use super::*;

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}

impl Lower<'_> {
    pub(super) fn simple_assignment_value(&mut self, assign: &ast::AssignExpr) -> Result<Expr> {
        if assign.op != ast::AssignOp::Assign {
            return self.compound_assignment_value(assign);
        }
        if !self.types.profile.closures() {
            return Err(unsupported(
                "assignment expressions require checked-f64-utf16-v12",
            ));
        }
        let target = match &assign.left {
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(name)) => self
                .prepare_pattern_target(&ast::Pat::Ident(name.clone()))?
                .ok_or_else(|| invalid("missing assignment binding"))?,
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(member)) => {
                let base = self.write_member_base(member)?;
                if matches!(base.ty, Type::Dictionary(_)) {
                    return self.dictionary_write(member, base, &assign.right);
                }
                if matches!(base.ty, Type::Collection(ir::CollectionKind::Uint8Array, _)) {
                    return self.bytes_write(member, base, &assign.right);
                }
                if let Type::Tuple(_) | Type::Collection(..) = base.ty {
                    return Err(unsupported(&format!(
                        "assignment to a {} element or property",
                        base.ty
                    )));
                }
                if let Type::Record(shape) = base.ty
                    && let ast::MemberProp::Computed(key) = &member.prop
                    && !matches!(
                        &*key.expr,
                        ast::Expr::Lit(ast::Lit::Str(_) | ast::Lit::Num(_))
                    )
                {
                    return self.dynamic_record_write_value(assign, base, &key.expr, shape);
                }
                let field = self.place_value(member, base)?;
                self.prepare_member_reference(field, member.span)?
            }
            _ => return Err(unsupported("assignment expression target")),
        };
        self.prepared_assignment_value(target, &assign.right, assign.span)
    }
    /// Preserve declared storage invariants after computing an assignment result.
    /// A union's incompatible member throws before any write is published.
    pub(super) fn checked_assignment_result(
        &mut self,
        value: Expr,
        expected: Type,
    ) -> Result<Expr> {
        if !self.types.profile.bigint_values() || arguments::accepts(expected, value.ty, self.types)
        {
            return Ok(value);
        }
        let members = unions::members(value.ty, self.types);
        let allowed: Vec<_> = members
            .into_iter()
            .filter(|ty| arguments::accepts(expected, *ty, self.types))
            .collect();
        if allowed.is_empty() {
            return Err(self.source.locate(
                type_error("assignment result matches no declared target type"),
                value.span,
            ));
        }
        let span = value.span;
        let input_ty = value.ty;
        let ty = unions::intern(allowed.clone(), self.types)?;
        let input = self.hidden(input_ty);
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        let error = self.array_error(
            varta_common::value::ErrorClass::TypeError,
            "Invalid assignment result for target type",
            span,
        )?;
        let mut branch = vec![S::Throw(error)];
        for member in allowed.into_iter().rev() {
            let scalar = Expr {
                span,
                ty: member,
                kind: E::UnboxUnion(Box::new(local(span, input, input_ty))),
            };
            branch = vec![S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsUnionMember(Box::new(local(span, input, input_ty)), member),
                },
                vec![S::Init(
                    output,
                    unions::widen(scalar, Some(ty), self.types)?,
                )],
                branch,
            )];
        }
        let mut body = vec![S::Init(input, value)];
        body.extend(branch);
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![input, output],
                body,
            },
        })
    }
}
