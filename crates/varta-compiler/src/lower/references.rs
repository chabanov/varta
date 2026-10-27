use super::*;
#[derive(Clone, Copy)]
pub(super) enum Guard {
    Exact(Type),
    Error(varta_common::value::ErrorClass),
    Object,
    Array,
}
impl Guard {
    pub fn matches(self, ty: Type) -> bool {
        match self {
            Self::Error(class) => {
                ty.is_error()
                    && (class == varta_common::value::ErrorClass::Error
                        || ty.error_class() == Some(class))
            }
            Self::Exact(t) => t == ty,
            Self::Object => matches!(
                ty,
                Type::Error | Type::NativeError(_) | Type::Null | Type::Record(_) | Type::Array(_)
            ),
            Self::Array => matches!(ty, Type::Array(_)),
        }
    }
}
impl Lower<'_> {
    pub(super) fn context(&self, expression: &ast::Expr, expected: Option<Type>) -> Option<Type> {
        if !self.types.profile.reference_unions() {
            return expected;
        }
        let ty = expected?;
        if !ty.is_union() {
            return Some(ty);
        }
        unions::members(ty, self.types)
            .into_iter()
            .find(|ty| match expression {
                ast::Expr::Object(_) => matches!(ty, Type::Record(_)),
                ast::Expr::Array(_) => matches!(ty, Type::Array(_)),
                _ => false,
            })
    }
    pub(super) fn conditional(
        &mut self,
        branch: &ast::CondExpr,
        expected: Option<Type>,
    ) -> Result<Expr> {
        let test = self.condition(&branch.test)?;
        let saved = self.refinements.clone();
        self.guard(&branch.test, true)?;
        let yes = self.expr_expected(&branch.cons, expected)?;
        self.refinements = saved.clone();
        self.guard(&branch.test, false)?;
        let no = self.expr_expected(&branch.alt, expected)?;
        self.refinements = saved;
        let (ty, yes, no) = if yes.ty == no.ty {
            (yes.ty, yes, no)
        } else {
            self.join(yes, no)?
        };
        if ty.width() != 1 {
            return Err(unsupported("conditional result type"));
        }
        Ok(Expr {
            span: branch.span,
            ty,
            kind: E::Conditional(Box::new(test), Box::new(yes), Box::new(no)),
        })
    }
    pub(super) fn array_is_array(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        if self.binding("Array").is_ok() || self.functions.contains_key("Array") {
            return Err(unsupported("shadowed Array.isArray intrinsic"));
        }
        if call.args.len() != 1 {
            return Err(invalid("Array.isArray expects one value"));
        }
        let value = self.expr(&call.args[0].expr)?;
        let kind = if value.ty.is_union() {
            E::Host(
                self.types.profile.host_operation(Host::ValueIsArray),
                vec![value],
            )
        } else {
            if value.ty.value_kind().is_none() {
                return Err(unsupported("Array.isArray argument type"));
            }
            let boolean = i32::from(matches!(value.ty, Type::Array(_)));
            E::Sequence(
                Box::new(value),
                Box::new(Expr {
                    span: call.span,
                    ty: Type::Boolean,
                    kind: E::Integer(boolean),
                }),
            )
        };
        Ok(Expr {
            span: call.span,
            ty: Type::Boolean,
            kind,
        })
    }
    pub(super) fn refine(&mut self, name: &str, matched: Guard, positive: bool) -> Result<()> {
        let Ok(binding) = self.binding(name) else {
            return Ok(());
        };
        let Some(ty) = self.refinements.get(&binding.slot).copied().or(binding.ty) else {
            return Ok(());
        };
        if !ty.is_union() {
            return Ok(());
        }
        if binding.mutable {
            self.guarded.insert(binding.slot);
        }
        if binding.mutable && self.cells.contains(&binding.slot) {
            return Err(unsupported(
                "guard refinement of a mutable captured binding",
            ));
        }
        let remaining: Vec<_> = unions::members(ty, self.types)
            .into_iter()
            .filter(|ty| matched.matches(*ty) == positive)
            .collect();
        if !remaining.is_empty() {
            let ty = unions::intern(remaining, self.types)?;
            self.refinements.insert(binding.slot, ty);
        }
        Ok(())
    }
}
