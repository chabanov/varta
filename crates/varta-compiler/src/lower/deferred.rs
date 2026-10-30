//! Deferred initialization `let x: T;` with definite assignment (v37
//! `language_core`).
//!
//! A `let` declaration without an initializer whose annotated type does not
//! admit `undefined` declares its storage (the all-zero pattern of `T`, never
//! observed) and starts *unassigned*. As in TypeScript's definite assignment
//! analysis, every read of the binding, and every compound, logical or update
//! assignment that reads it first, must be preceded by an assignment on every
//! path: the unassigned set is part of the flow snapshots, joins take the
//! union of the reachable paths, a `catch` or `finally` block starts from the
//! set at `try` entry, and loops keep their entry set. A closure may capture
//! the binding only where it is definitely assigned, because the closure may
//! run at any time; JavaScript would read `undefined` there.
use super::*;
impl Lower<'_> {
    /// The declaration statement of deferred `name` of type `ty`.
    pub(super) fn deferred(
        &mut self,
        name: &str,
        binding: &Binding,
        ty: Type,
        span: swc_common::Span,
    ) -> Result<S> {
        if binding.tdz.is_some() && self.core.early.contains(&binding.slot) {
            return Err(unsupported(
                "capture of a deferred let binding before its declaration",
            ));
        }
        value_type(ty)?;
        self.update(name, ty)?;
        self.core.unassigned.insert(binding.slot);
        Ok(S::Init(
            binding.slot,
            Expr {
                span,
                ty,
                kind: E::Zero,
            },
        ))
    }
    /// Reject a read of a deferred binding that is not definitely assigned.
    pub(super) fn require_assigned(&self, name: &str, binding: &Binding) -> Result<()> {
        if binding.identity.0 == self.index && self.core.unassigned.contains(&binding.slot) {
            return Err(invalid(format!(
                "binding used before being assigned: {name}"
            )));
        }
        Ok(())
    }
    /// [`Self::require_assigned`] for a read-modify-write target `name`.
    pub(super) fn require_assigned_target(&mut self, name: &str) -> Result<()> {
        match self.resolve_binding(name) {
            Ok(binding) => self.require_assigned(name, &binding),
            Err(_) => Ok(()),
        }
    }
    /// A closure sees the deferred bindings that are not definitely assigned
    /// where it is created as uninitialized, so it cannot capture them.
    pub(super) fn unassigned_outer(&self, outer: &mut BTreeMap<String, Binding>) {
        for binding in outer.values_mut() {
            if binding.identity.0 == self.index && self.core.unassigned.contains(&binding.slot) {
                binding.initialized = false;
                binding.tdz = None;
            }
        }
    }
}
