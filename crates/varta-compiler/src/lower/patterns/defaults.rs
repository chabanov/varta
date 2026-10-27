use super::*;
impl Lower<'_> {
    pub(super) fn pattern_default(
        &mut self,
        pattern: &ast::Pat,
        value: Expr,
        default: &ast::Expr,
        expected: Option<Type>,
    ) -> Result<Vec<S>> {
        let span = value.span;
        let input_ty = value.ty;
        let input = self.hidden(input_ty);
        let present: Vec<_> = unions::members(input_ty, self.types)
            .into_iter()
            .filter(|t| *t != Type::Undefined)
            .collect();
        let present_ty = if present.is_empty() {
            None
        } else {
            Some(unions::intern(present, self.types)?)
        };
        let context = expected.or_else(|| self.context(default, present_ty));
        let fallback = self.expr_expected(default, context)?;
        value_type(fallback.ty).map_err(|e| self.source.locate(e, fallback.span))?;
        if expected.is_some_and(|ty| ty != fallback.ty) {
            return Err(self.source.locate(
                type_error(format!(
                    "default parameter mismatch: expected {}, found {}",
                    expected.unwrap_or(Type::Void),
                    fallback.ty
                )),
                fallback.span,
            ));
        }
        let ty = if let Some(ty) = expected {
            ty
        } else if let Some(present) = present_ty {
            unions::intern(vec![present, fallback.ty], self.types)
                .map_err(|e| self.source.locate(e, default.span()))?
        } else {
            fallback.ty
        };
        let output = self.hidden(ty);
        let fallback = unions::widen(fallback, Some(ty), self.types)?;
        let mut body = vec![S::Init(input, value)];
        if let Some(present) = present_ty {
            let source = if input_ty.is_union() && !present.is_union() {
                Expr {
                    span,
                    ty: present,
                    kind: E::UnboxUnion(Box::new(local(span, input, input_ty))),
                }
            } else {
                local(span, input, present)
            };
            let source = unions::widen(source, Some(ty), self.types)?;
            if source.ty != ty {
                return Err(type_error("default present value type mismatch"));
            }
            body.push(S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsUndefined(Box::new(local(span, input, input_ty))),
                },
                vec![S::Init(output, fallback)],
                vec![S::Init(output, source)],
            ));
        } else {
            body.push(S::Init(output, fallback));
        }
        body.extend(self.initialize_parameter_pattern(
            pattern,
            local(span, output, ty),
            None,
            None,
        )?);
        Ok(vec![S::Scope(vec![input, output], body)])
    }
}
