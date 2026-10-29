//! Proven function identities bound captured-cell effects; unknown reentry widens.
use super::*;
use std::collections::BTreeSet;
type Identity = (usize, u32);
#[derive(Clone, Default)]
pub(super) struct Impact {
    pub writes: BTreeSet<Identity>,
    pub unknown: bool,
    pub parameters: BTreeSet<Identity>,
}
impl Impact {
    pub(super) fn unknown() -> Self {
        Self {
            unknown: true,
            ..Self::default()
        }
    }
    pub(super) fn extend(&mut self, other: Self) {
        self.writes.extend(other.writes);
        self.unknown |= other.unknown;
        self.parameters.extend(other.parameters);
    }
}
pub(super) fn callbacks(ty: Type, types: &ir::ValueTypes, depth: usize) -> bool {
    fn visit(ty: Type, types: &ir::ValueTypes, depth: usize, seen: &mut BTreeSet<Type>) -> bool {
        if depth >= 64 {
            return true;
        }
        if !seen.insert(ty) {
            return false;
        }
        match ty {
            Type::Function(_) => true,
            Type::Record(id) => types.records[id as usize]
                .fields
                .iter()
                .any(|f| visit(f.ty, types, depth + 1, seen)),
            Type::Array(id) => visit(types.arrays[id as usize], types, depth + 1, seen),
            Type::Class(id) => visit(
                Type::Record(types.classes.shape(id)),
                types,
                depth + 1,
                seen,
            ),
            Type::Union(id) | Type::ReferenceUnion(id) => types.unions[id as usize]
                .iter()
                .any(|&t| visit(t, types, depth + 1, seen)),
            _ => false,
        }
    }
    visit(ty, types, depth, &mut BTreeSet::new())
}
impl Lower<'_> {
    fn function_proof(&self, value: &Expr, depth: usize) -> Option<BTreeSet<usize>> {
        if depth >= 64 {
            return None;
        }
        match &value.kind {
            E::FunctionRef { index, .. } => Some(BTreeSet::from([*index])),
            E::Local(slot) => self
                .scopes
                .iter()
                .rev()
                .flat_map(|s| s.values())
                .chain(self.captures.values())
                .find(|b| b.slot == *slot && !b.mutable)
                .and_then(|b| self.known_calls.get(&b.identity))
                .cloned(),
            E::Conditional(_, yes, no) => {
                let mut proof = self.function_proof(yes, depth + 1)?;
                proof.extend(self.function_proof(no, depth + 1)?);
                Some(proof)
            }
            E::Sequence(_, value) | E::FunctionCheck { value, .. } => {
                self.function_proof(value, depth + 1)
            }
            E::ValueBlock { output, body, .. } => match body.last()? {
                S::Init(slot, value) if slot == output => self.function_proof(value, depth + 1),
                _ => None,
            },
            _ => None,
        }
    }
    pub(super) fn flow_known_binding(&mut self, binding: &Binding, value: &Expr) {
        if self.types.profile.refinement_writes()
            && !binding.mutable
            && matches!(value.ty, Type::Function(_))
            && let Some(proof) = self.function_proof(value, 0)
        {
            self.known_calls.insert(binding.identity, proof);
        } else if self.types.profile.refinement_writes()
            && !binding.mutable
            && matches!(value.ty, Type::Function(_))
            && let Some(origin) = self.parameter_value(value, 0)
        {
            self.parameter_values.insert(binding.identity, origin);
        }
    }
    pub(super) fn flow_summary(&self) -> Impact {
        let mut writes = self.call_writes.clone();
        writes.extend(
            self.captures
                .values()
                .filter(|b| b.mutable && self.direct_writes.contains(&b.slot))
                .map(|b| b.identity),
        );
        // Local cells belong to this activation, rather than its caller. Stable
        // callback inputs remain symbolic; unknown self/returned calls reenter.
        writes.retain(|identity| identity.0 != self.index);
        let (parameters, unknown) = self.stable_parameters();
        Impact {
            writes,
            unknown,
            parameters,
        }
    }
    pub(super) fn module_function(&self, index: usize) -> bool {
        self.functions
            .values()
            .any(|signature| signature.index == index)
    }
    fn callback_value(&self, value: &Expr, depth: usize) -> Impact {
        if depth >= 64 {
            return Impact::unknown();
        }
        if !callbacks(value.ty, self.types, 0) {
            return Impact::default();
        }
        if matches!(value.ty, Type::Function(_)) {
            if let Some(impact) = self.parameter_impact(value) {
                return impact;
            }
            return self
                .function_proof(value, depth + 1)
                .map_or_else(Impact::unknown, |proof| {
                    self.targets(&proof, &[], true, depth + 1)
                });
        }
        // Function-valued aggregates are not admitted yet. If a future profile
        // permits them, an opaque container requires the reentry fallback.
        Impact::unknown()
    }
    pub(super) fn targets(
        &self,
        proof: &BTreeSet<usize>,
        args: &[Expr],
        callback: bool,
        depth: usize,
    ) -> Impact {
        if depth >= 64 {
            return Impact::unknown();
        }
        let mut impact = Impact::default();
        for index in proof {
            if self.module_function(*index) {
                // A module function has no caller environment. It can reach a
                // caller cell only through function values reachable in inputs.
                if callback {
                    if self
                        .functions
                        .values()
                        .filter(|s| s.index == *index)
                        .any(|s| {
                            s.receiver
                                .into_iter()
                                .chain(s.params.iter().copied())
                                .any(|ty| callbacks(ty, self.types, 0))
                        })
                    {
                        impact.unknown = true;
                    }
                } else {
                    for arg in args {
                        impact.extend(self.callback_value(arg, depth + 1));
                    }
                }
            } else if let Some(summary) = self.call_effects.get(index) {
                impact.writes.extend(summary.writes.iter().copied());
                impact.unknown |= summary.unknown;
                for &(owner, position) in &summary.parameters {
                    if owner != *index {
                        impact.parameters.insert((owner, position));
                    } else if callback {
                        impact.unknown = true;
                    } else if let Some(value) = args.get(position as usize) {
                        impact.extend(self.callback_value(value, depth + 1));
                    } else {
                        impact.unknown = true;
                    }
                }
            } else {
                impact.unknown = true;
            }
        }
        impact
    }
    pub(super) fn flow_call(&mut self, call: &ast::CallExpr, value: &Expr) {
        if !self.types.profile.refinement_writes() || super::flow::native(call, self) {
            return;
        }
        let impact = match &value.kind {
            E::Call(index, args) if self.module_function(*index) => {
                self.targets(&BTreeSet::from([*index]), args, false, 0)
            }
            E::FunctionCall { callee, args, .. } => {
                self.parameter_impact(callee).unwrap_or_else(|| {
                    self.function_proof(callee, 0)
                        .map_or_else(Impact::unknown, |proof| {
                            self.targets(&proof, args, false, 0)
                        })
                })
            }
            // Spread helpers keep their own physical call. A stable AST callee
            // can still bound its effects; unproven targets retain the fallback.
            _ => self.flow_ast_call(call, 0, true),
        };
        self.call_writes.extend(impact.writes.iter().copied());
        self.call_unknown |= impact.unknown;
        self.call_parameters
            .extend(impact.parameters.iter().copied());
        let mut slots = self.flow_identity_slots(&impact.writes);
        if impact.unknown || !impact.parameters.is_empty() {
            slots.extend(self.mutable_cells());
        }
        for slot in slots {
            self.flow_write(slot);
        }
    }
    pub(super) fn flow_identity_slots(&self, writes: &BTreeSet<Identity>) -> BTreeSet<u32> {
        self.scopes
            .iter()
            .flat_map(|s| s.values())
            .chain(self.captures.values())
            .filter(|b| b.mutable && writes.contains(&b.identity))
            .map(|b| b.slot)
            .collect()
    }
}
