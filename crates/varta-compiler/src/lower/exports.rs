//! Module export tables, shared by every binding namespace (functions, named
//! types and, once linked, module constants). A namespace supplies its local
//! bindings and binding identity; this module applies the common ECMAScript
//! rules for explicit re-exports (`export { x as y } from`, v32) and star
//! re-exports (`export * from`, v32).
//!
//! Extension point: a new binding kind implements [`Namespace`], keeps one
//! [`Table`] per module in dependency order and calls [`link`] after the
//! module's local bindings exist. Re-exports and stars then work for it
//! without further changes; import resolution must treat
//! [`Lookup::Ambiguous`] as an error, like [`ambiguous`].
use super::*;
/// `export *` never forwards a default export (ECMAScript `GetExportedNames`).
pub(super) use crate::modules::DEFAULT_EXPORT;
use crate::modules::{Export, Unit};
use std::collections::{BTreeSet, btree_map};
use std::path::{Path, PathBuf};

/// The exported names of one module in one binding namespace.
pub(super) struct Table<V> {
    bound: BTreeMap<String, V>,
    /// Names that several `export *` sources provide with distinct bindings
    /// and no explicit export shadows. They are excluded, and only an import or
    /// explicit re-export naming one is an error.
    ambiguous: BTreeSet<String>,
}
impl<V> Default for Table<V> {
    fn default() -> Self {
        Self {
            bound: BTreeMap::new(),
            ambiguous: BTreeSet::new(),
        }
    }
}

pub(super) enum Lookup<'a, V> {
    Bound(&'a V),
    Ambiguous,
    Missing,
}

impl<V> Table<V> {
    pub(super) fn lookup(&self, name: &str) -> Lookup<'_, V> {
        match self.bound.get(name) {
            Some(binding) => Lookup::Bound(binding),
            None if self.ambiguous.contains(name) => Lookup::Ambiguous,
            None => Lookup::Missing,
        }
    }
}

/// One binding namespace of the module linker.
pub(super) trait Namespace {
    type Binding: Clone;
    /// Two star sources providing equal identities export the same binding,
    /// which is not ambiguous.
    type Identity: Eq;
    /// The binding a local export (no `from`) names in this namespace, `None`
    /// when it belongs to another namespace, or an unlocated error.
    fn local(&self, export: &Export) -> Result<Option<Self::Binding>>;
    /// Whether an explicit re-export may forward a binding of this namespace.
    fn admits(&self, export: &Export) -> bool;
    /// An explicit re-export whose source lacks the name in this namespace:
    /// `Ok` when another namespace supplies it, otherwise an unlocated error.
    fn missing(&self, export: &Export, target: &Path) -> Result<()>;
    fn identity(&self, binding: &Self::Binding) -> Self::Identity;
}

/// The diagnostic for importing or re-exporting an ambiguous star export.
pub(super) fn ambiguous(target: &Path, name: &str) -> CompilerError {
    CompilerError::Module(format!(
        "module {} exports {name} ambiguously through export *",
        target.display()
    ))
}

/// Compute `unit`'s export table in one namespace. `tables` holds the tables of
/// every already-linked dependency of this namespace.
pub(super) fn link<N: Namespace>(
    unit: &Unit,
    namespace: &N,
    tables: &BTreeMap<PathBuf, Table<N::Binding>>,
) -> Result<Table<N::Binding>> {
    let mut table = Table::default();
    for export in &unit.exports {
        explicit(export, namespace, tables, &mut table)
            .map_err(|e| unit.source.locate(e, export.span))?;
    }
    if unit.stars.is_empty() {
        return Ok(table);
    }
    // Explicit exports shadow star exports of the same name in every namespace.
    let shadowed: BTreeSet<&str> = unit.exports.iter().map(|e| e.exported.as_str()).collect();
    let mut candidates: BTreeMap<String, Option<N::Binding>> = BTreeMap::new();
    for target in &unit.stars {
        let Some(source) = tables.get(target) else {
            continue;
        };
        let visible = |name: &String| name != DEFAULT_EXPORT && !shadowed.contains(name.as_str());
        for name in source.ambiguous.iter().filter(|name| visible(name)) {
            candidates.insert(name.clone(), None);
        }
        for (name, binding) in source.bound.iter().filter(|(name, _)| visible(name)) {
            match candidates.entry(name.clone()) {
                btree_map::Entry::Vacant(entry) => {
                    entry.insert(Some(binding.clone()));
                }
                btree_map::Entry::Occupied(mut entry) => {
                    let same = entry.get().as_ref().is_some_and(|existing| {
                        namespace.identity(existing) == namespace.identity(binding)
                    });
                    if !same {
                        entry.insert(None);
                    }
                }
            }
        }
    }
    for (name, binding) in candidates {
        match binding {
            Some(binding) => {
                table.bound.insert(name, binding);
            }
            None => {
                table.ambiguous.insert(name);
            }
        }
    }
    Ok(table)
}

fn explicit<N: Namespace>(
    export: &Export,
    namespace: &N,
    tables: &BTreeMap<PathBuf, Table<N::Binding>>,
    table: &mut Table<N::Binding>,
) -> Result<()> {
    let binding = match &export.from {
        None => namespace.local(export)?,
        Some(_) if !namespace.admits(export) => None,
        Some(target) => match tables.get(target).map(|t| t.lookup(&export.local)) {
            Some(Lookup::Bound(binding)) => Some(binding.clone()),
            Some(Lookup::Ambiguous) => return Err(ambiguous(target, &export.local)),
            Some(Lookup::Missing) | None => {
                namespace.missing(export, target)?;
                None
            }
        },
    };
    let Some(binding) = binding else {
        return Ok(());
    };
    if table
        .bound
        .insert(export.exported.clone(), binding)
        .is_some()
    {
        return Err(CompilerError::Module(format!(
            "duplicate export: {}",
            export.exported
        )));
    }
    Ok(())
}
