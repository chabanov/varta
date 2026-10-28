//! Symbolic callback inputs are instantiated only at a proven invocation.
use super::*;
use flow_calls::Impact;
use std::collections::BTreeSet;
impl Lower<'_> {
    pub(super) fn flow_parameter_slot(&mut self, slot: u32, ty: Type) {
        if self.types.profile.refinement_writes() && matches!(ty, Type::Function(_)) {
            self.parameter_values
                .insert((self.index, slot), (self.index, slot));
            self.parameter_roots.insert(slot, (slot, true));
        }
    }
    pub(super) fn flow_parameter_binding(&mut self, binding: &Binding, position: u32) {
        if self.types.profile.refinement_writes() && matches!(binding.ty, Some(Type::Function(_))) {
            self.parameter_values
                .insert(binding.identity, (self.index, position));
            self.parameter_roots
                .insert(position, (binding.slot, binding.mutable));
        }
    }
    pub(super) fn parameter_value(&self, value: &Expr, depth: usize) -> Option<(usize, u32)> {
        if depth >= 64 {
            return None;
        }
        match &value.kind {
            E::Local(slot) => {
                let identity = self
                    .scopes
                    .iter()
                    .rev()
                    .flat_map(|s| s.values())
                    .chain(self.captures.values())
                    .find(|b| b.slot == *slot)
                    .map_or((self.index, *slot), |b| b.identity);
                self.parameter_values.get(&identity).copied()
            }
            E::Sequence(_, value) | E::FunctionCheck { value, .. } => {
                self.parameter_value(value, depth + 1)
            }
            _ => None,
        }
    }
    pub(super) fn parameter_impact(&self, value: &Expr) -> Option<Impact> {
        self.parameter_value(value, 0).map(|origin| Impact {
            parameters: BTreeSet::from([origin]),
            ..Impact::default()
        })
    }
    pub(super) fn stable_parameters(&self) -> (BTreeSet<(usize, u32)>, bool) {
        let mut parameters = self.call_parameters.clone();
        let mut unknown = self.call_unknown;
        parameters.retain(|&(owner, position)| {
            if owner != self.index {
                return true;
            }
            let stable = self
                .parameter_roots
                .get(&position)
                .is_some_and(|&(slot, mutable)| {
                    // A captured input still denotes its original argument if
                    // no admitted source store can replace it, including stores
                    // inside nested functions that may escape through a callback.
                    !self.direct_writes.contains(&slot)
                        && (!mutable
                            || !self
                                .call_effects
                                .values()
                                .any(|effect| effect.writes.contains(&(self.index, slot))))
                });
            unknown |= !stable;
            stable
        });
        (parameters, unknown)
    }
}
