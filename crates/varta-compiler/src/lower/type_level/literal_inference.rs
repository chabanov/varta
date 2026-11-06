//! Literal type inference of TypeScript (Handbook, Literal Types and
//! Literal Inference; `tsc`'s widening rules) under v38:
//!
//! - `const x = "a"` (likewise a number or boolean literal) declares `x`
//!   with the literal type `"a"`, a *widening* literal type;
//! - a `let`/`var` initialized by a literal or by such a binding widens to
//!   the base primitive (`let y = x` is `string`), while a non-widening
//!   literal type (an annotation, `as const`) is kept;
//! - array elements, object properties and other mutable locations take
//!   the erased (widened) type, as the earlier lowering already does.
//!
//! The values of a template literal whose holes all have finite literal
//! types are recorded per hole, so the build-time class scanner can
//! enumerate a composed class name (`` `text-${size}` `` with
//! `const size = "sm"`) instead of refusing it.
use super::*;

/// The most values one template's recorded combinations may have.
const MAX_TEMPLATE_VALUES: usize = literals::MAX_LITERAL_MEMBERS;

impl Lower<'_> {
    /// The value of an unannotated declaration with its declaration type.
    pub(in crate::lower) fn declaration_type(
        &mut self,
        binding: &Binding,
        init: &ast::Expr,
        value: Expr,
        constant: bool,
    ) -> Expr {
        if !self.types.profile.type_level() {
            return value;
        }
        let from_widening = match stmt::unparen(init) {
            ast::Expr::Ident(id) => {
                let name = id.sym.as_ref();
                let found = self.binding(name).or_else(|_| self.resolve_binding(name));
                found.is_ok_and(|b| self.types.type_level.widening.contains(&b.identity))
                    || self.module_widening(name)
            }
            _ => false,
        };
        if constant {
            if let Some(literal) = literals::fresh(&value, self.types) {
                self.types.type_level.widening.insert(binding.identity);
                return Expr {
                    ty: literal,
                    ..value
                };
            }
            if from_widening {
                self.types.type_level.widening.insert(binding.identity);
            }
            return value;
        }
        if from_widening {
            return literals::erase(value, self.types);
        }
        value
    }

    /// Whether `name` is an unshadowed module constant declared by a
    /// literal (materialized in the outermost scope of a body).
    fn module_widening(&self, name: &str) -> bool {
        let mut scopes = self.scopes.iter();
        scopes.next().is_some_and(|outer| outer.contains_key(name))
            && scopes.all(|scope| !scope.contains_key(name))
            && queries::literal_constant(self.types, named::current(self.types), name)
    }

    /// One template hole, lowered once; its literal type is kept in `holes`.
    pub(in crate::lower) fn template_hole(
        &mut self,
        hole: &ast::Expr,
        holes: &mut Vec<Type>,
    ) -> Result<Expr> {
        if !self.types.profile.type_level() {
            return self.expr(hole);
        }
        let value = self.expr_preserving(hole)?;
        holes.push(value.ty);
        Ok(literals::erase(value, self.types))
    }

    /// Record every value `template` may produce when each hole's type is a
    /// finite literal set.
    pub(in crate::lower) fn note_template_values(&mut self, template: &ast::Tpl, holes: &[Type]) {
        if !self.types.profile.type_level()
            || holes.is_empty()
            || holes.len() != template.exprs.len()
        {
            return;
        }
        let Some(values) =
            template_types::values(&template.quasis, holes, MAX_TEMPLATE_VALUES, self.types)
        else {
            return;
        };
        let module = named::current(self.types);
        for hole in &template.exprs {
            let span = hole.span();
            self.types
                .type_level
                .compositions
                .insert((module, span.lo.0, span.hi.0), values.clone());
        }
    }
}

/// The values the template whose hole spans `span` in module `module` may
/// produce, when its holes have finite literal types (v38).
pub(crate) fn composition_values(
    types: &ir::ValueTypes,
    module: usize,
    span: swc_common::Span,
) -> Option<&Vec<String>> {
    types
        .type_level
        .compositions
        .get(&(module, span.lo.0, span.hi.0))
}
