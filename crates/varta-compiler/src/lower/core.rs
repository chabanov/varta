//! Per-function state of the v37 language-core stream (`language_core`).
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(super) struct State {
    /// Bindings of this function that a closure captured before their
    /// initialization (`tdz`): their declaring block allocates the binding
    /// cell and its flag at entry, and their initialization only stores.
    pub(super) early: BTreeSet<u32>,
    /// Deferred `let x: T;` bindings of this function that are not
    /// definitely assigned on the current path (`deferred`).
    pub(super) unassigned: BTreeSet<u32>,
    /// Property-path facts on the current path (`paths`).
    pub(super) paths: BTreeMap<super::paths::Path, super::paths::Fact>,
    /// Set while guarding a test that may write properties or run code.
    pub(super) impure_guard: bool,
    /// The span of an index read whose context admits `undefined` (`dense`):
    /// it reads `T | undefined` as in JavaScript instead of a checked `T`.
    pub(super) optional_index: Option<swc_common::Span>,
    /// Names the enclosing blocks compare with `undefined` (`dense`): an
    /// inferred binding of such a name initialized by an index read, and a
    /// `for...of` variable of such a name, read `T | undefined` unchecked.
    pub(super) undefined_checked: BTreeSet<String>,
}
