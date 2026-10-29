use super::*;
// Type hints supply aggregate layout context without constraining an inferred
// element's final type or creating a synthetic union descriptor.
#[derive(Clone, Copy, Default)]
pub(super) struct AggregateHints {
    pub(super) array: Option<Type>,
    pub(super) record: Option<Type>,
}
impl AggregateHints {
    pub(super) fn from_members(members: &[Type]) -> Self {
        Self {
            array: members
                .iter()
                .find(|ty| matches!(ty, Type::Array(_)))
                .copied(),
            record: members
                .iter()
                .find(|ty| matches!(ty, Type::Record(_)))
                .copied(),
        }
    }
    pub(super) fn select(self, mut expression: &ast::Expr) -> Option<Type> {
        while let ast::Expr::Paren(paren) = expression {
            expression = &paren.expr;
        }
        match expression {
            ast::Expr::Array(_) => self.array,
            ast::Expr::Object(_) => self.record,
            _ => None,
        }
    }
}
#[derive(Clone, Copy)]
pub(super) enum Guard {
    Exact(Type),
    Error(varta_common::value::ErrorClass),
    Object,
    Array,
    Truth(bool),
    Nullish,
    /// `local === constant` against an interned singleton literal set (v32).
    Literal(Type),
}
impl Guard {
    pub fn matches(self, ty: Type) -> bool {
        match self {
            Self::Error(class) => {
                ty.is_error()
                    && (class == varta_common::value::ErrorClass::Error
                        || ty.error_class() == Some(class))
            }
            Self::Exact(t) => t == ty.erased(),
            // Literal guards refine through `literals::refine`, never by member.
            Self::Literal(_) => false,
            Self::Object => matches!(
                ty,
                Type::Error
                    | Type::NativeError(_)
                    | Type::Null
                    | Type::Record(_)
                    | Type::Class(_)
                    | Type::Array(_)
            ),
            Self::Array => matches!(ty, Type::Array(_)),
            Self::Nullish => matches!(ty, Type::Null | Type::Undefined),
            Self::Truth(true) => !matches!(ty, Type::Null | Type::Undefined),
            Self::Truth(false) => !matches!(
                ty,
                Type::Record(_)
                    | Type::Class(_)
                    | Type::Array(_)
                    | Type::Function(_)
                    | Type::Error
                    | Type::NativeError(_)
            ),
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
        hints: Option<AggregateHints>,
    ) -> Result<Expr> {
        let test = self.condition(&branch.test)?;
        let saved = self.flow_snapshot();
        self.guard(&branch.test, true)?;
        self.flow_edge(flow_control::truth(&branch.test) != Some(false));
        let yes = self.expr_contextual(&branch.cons, expected, hints)?;
        let yes_state = self.flow_snapshot();
        self.flow_alternate(&saved);
        self.guard(&branch.test, false)?;
        self.flow_edge(flow_control::truth(&branch.test) != Some(true));
        let no = self.expr_contextual(&branch.alt, expected, hints)?;
        self.flow_join(&saved, &[yes_state, self.flow_snapshot()])?;
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
        let kind = if value.ty.is_cell() {
            E::Host(
                self.types.profile.host_operation(Host::ValueIsArray),
                vec![value],
            )
        } else {
            if value.ty.value_kind().is_none()
                || value.ty == Type::BigInt && !self.types.profile.bigint_values()
            {
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
        self.refine_with(name, |ty, types| {
            if ty == Type::Unknown {
                unknown::refine(matched, positive)
            } else if let Guard::Literal(single) = matched {
                literals::refine(ty, single, positive, types).map(|refined| vec![refined])
            } else if ty.is_union() {
                Some(
                    unions::members(ty, types)
                        .into_iter()
                        .filter(|ty| matched.matches(*ty) == positive)
                        .collect(),
                )
            } else {
                None
            }
        })
    }
    /// Refine `name` to the types `select` keeps of its current type; `None`
    /// leaves the binding unrefined.
    pub(super) fn refine_with(
        &mut self,
        name: &str,
        select: impl FnOnce(Type, &mut ir::ValueTypes) -> Option<Vec<Type>>,
    ) -> Result<()> {
        let Ok(binding) = self.binding(name).or_else(|_| self.resolve_binding(name)) else {
            return Ok(());
        };
        if self.guard_blocked.contains(&binding.slot) {
            return Ok(());
        }
        let Some(ty) = self.refinements.get(&binding.slot).copied().or(binding.ty) else {
            return Ok(());
        };
        let Some(remaining) = select(ty, self.types) else {
            return Ok(());
        };
        if self.types.profile.truth_guards()
            && (remaining.is_empty() || remaining == unions::members(ty, self.types))
        {
            return Ok(());
        }
        if binding.mutable {
            self.guarded.insert(binding.slot);
        }
        if binding.mutable
            && self.cells.contains(&binding.slot)
            && !self.types.profile.refinement_writes()
        {
            return Err(unsupported(
                "guard refinement of a mutable captured binding",
            ));
        }
        if !remaining.is_empty() {
            let ty = unions::intern(remaining, self.types)?;
            self.refinements.insert(binding.slot, ty);
        }
        Ok(())
    }
}
