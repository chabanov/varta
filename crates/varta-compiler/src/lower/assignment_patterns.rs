//! Destructuring writes to existing cells without declaring or reinitializing them.
use super::*;
impl Lower<'_> {
    pub(super) fn assignment_binding(
        &mut self,
        name: &str,
        span: swc_common::Span,
    ) -> Result<Binding> {
        let binding = self
            .resolve_binding(name)
            .map_err(|e| self.source.locate(e, span))?;
        if !binding.mutable || !binding.initialized {
            return Err(self.source.locate(
                invalid("assignment to constant or uninitialized binding"),
                span,
            ));
        }
        if self.refinements.contains_key(&binding.slot) && !self.types.profile.refinement_writes() {
            return Err(self.source.locate(
                unsupported("assignment to a guard-refined union binding"),
                span,
            ));
        }
        Ok(binding)
    }
    pub(super) fn destructuring_assignment(
        &mut self,
        assign: &ast::AssignExpr,
        target: &ast::AssignTargetPat,
    ) -> Result<S> {
        Ok(S::Expr(
            self.destructuring_assignment_value(assign, target, None)?,
        ))
    }
    pub(super) fn destructuring_assignment_value(
        &mut self,
        assign: &ast::AssignExpr,
        target: &ast::AssignTargetPat,
        expected: Option<Type>,
    ) -> Result<Expr> {
        if assign.op != ast::AssignOp::Assign {
            return Err(unsupported("compound destructuring assignment"));
        }
        if !self.types.profile.closures() {
            return Err(unsupported(
                "destructuring assignment requires checked-f64-utf16-v12",
            ));
        }
        let pattern: ast::Pat = target.clone().into();
        // Validate the whole syntax/budget first. Repeated targets are allowed;
        // this collects references, never creates new source bindings.
        let names = patterns::targets(&pattern).map_err(|(e, span)| self.source.locate(e, span))?;
        for (name, span) in names {
            self.assignment_binding(&name, span)?;
        }
        let context = if expected.is_some() {
            expected
        } else {
            self.assignment_array_context(&pattern)?
        };
        let value = self.expr_expected(&assign.right, context)?;
        let ty = value.ty;
        let input = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        let body = self.binding_pattern(
            &pattern,
            Expr {
                span: assign.span,
                ty,
                kind: E::Local(input),
            },
            None,
            None,
            patterns::Mode::Assign,
        )?;
        Ok(Expr {
            span: assign.span,
            ty,
            kind: E::Assignment {
                value: Box::new(value),
                input,
                marker,
                body,
            },
        })
    }
    /// A homogeneous target context makes empty RHS arrays useful without
    /// inventing tuple shapes or restricting a record's unselected properties.
    fn assignment_array_context(&mut self, pattern: &ast::Pat) -> Result<Option<Type>> {
        let ast::Pat::Array(array) = pattern else {
            return Ok(None);
        };
        let mut base = None;
        let mut rest_element = None;
        for entry in array.elems.iter().flatten() {
            let (pattern, rest) = if let ast::Pat::Rest(rest) = entry {
                (&*rest.arg, true)
            } else {
                (entry, false)
            };
            let pattern = if let ast::Pat::Assign(default) = pattern {
                &*default.left
            } else {
                pattern
            };
            let ast::Pat::Ident(id) = pattern else {
                return Ok(None);
            };
            let ty = self
                .assignment_binding(id.id.sym.as_ref(), id.span())?
                .ty
                .ok_or_else(|| invalid("missing assignment target type"))?;
            if ty.value_kind().is_none() {
                return Ok(None);
            }
            let ty = if rest {
                let Type::Array(id) = ty else {
                    return Ok(None);
                };
                let ty = self.types.arrays[id as usize];
                rest_element = Some(ty);
                ty
            } else {
                ty
            };
            let members: Vec<_> = unions::members(ty, self.types)
                .into_iter()
                .filter(|t| *t != Type::Undefined)
                .collect();
            let present = if members.is_empty() {
                Type::Undefined
            } else {
                unions::intern(members, self.types)?
            };
            if base.is_some_and(|prior| prior != present) {
                return Ok(None);
            }
            base = Some(present);
        }
        let element = if let Some(rest) = rest_element {
            Some(rest)
        } else if let Some(base) = base {
            Some(unions::intern(vec![base, Type::Undefined], self.types)?)
        } else {
            None
        };
        element
            .map(|element| arrays::intern(element, self.types))
            .transpose()
    }
}
