//! Types of watched class-name holes (K8, see [`crate::classes`]): while a
//! class scan type-checks the program, every lowered expression whose span
//! is a watched hole records whether its type is a finite union of string
//! literals, and which.
use super::*;
use crate::classes::HoleType;
use swc_common::Span;

impl Lower<'_> {
    /// Record the type of `span` when a class scan watches it.
    pub(super) fn observe_class_hole(&mut self, span: Span, ty: Type) {
        if self.types.class_holes.is_none() {
            return;
        }
        let observed = finite_strings(ty, self.types).map_or(HoleType::Open, HoleType::Finite);
        let key = (self.source.file.name.to_string(), span.lo.0, span.hi.0);
        if let Some(tracker) = self.types.class_holes.as_mut() {
            tracker.observe(key, observed);
        }
    }
}

/// The members of a finite union of string literals (a literal set, or a
/// union whose every member is one).
fn finite_strings(ty: Type, types: &ir::ValueTypes) -> Option<std::collections::BTreeSet<String>> {
    let mut values = std::collections::BTreeSet::new();
    for member in unions::members(ty, types) {
        values.extend(literals::string_members(member, types)?);
    }
    (!values.is_empty()).then_some(values)
}
