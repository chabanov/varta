//! Function-level linking of the guest intrinsic library (v38
//! `linked_intrinsics`).
//!
//! After emission, every `varta.<name>` import whose operation the profile
//! links ([`LanguageProfile::links`]) is replaced by the library function of
//! the same name and signature, together with exactly the library functions,
//! stack pointer and data segments it reaches. Guest code is not otherwise
//! changed: defined guest functions keep their order (catalog `code` numbers
//! stay valid) and linked code is appended after them.
//!
//! A linked module keeps one memory. Emission reserves the intrinsic region
//! of memory 0 ([`abi::StaticRegion::intrinsic_base`], after the static
//! extension and before the heap) sized by [`region_pages`]; linking places
//! the library's shadow stack and relocated data there. The library reaches
//! guest values and the result globals only through the accessor functions
//! synthesized here, and its per-call arena lives above the guest heap's end.
mod library;
mod merge;

use library::library;

use crate::CompilerError;
use varta_common::{HostOperation, abi, language::LanguageProfile};

/// The guest functions and global linked JSON builds values with, as
/// emitted (before linking): the allocator (`(len) -> ptr`), the retention
/// helper (`(handle) -> handle`, pushing a root) and the root-stack top.
#[derive(Debug, Clone, Copy)]
pub(crate) struct GuestValues {
    pub allocate: u32,
    pub retain: u32,
    pub roots: u32,
}

/// Whether a module of `profile` links `operation`: the profile's linked
/// intrinsics, except (test-only, [`json_hook`]) the checked JSON parse and
/// (test-only, [`import_hook`]) the operations the hook imports.
pub(crate) fn links(profile: LanguageProfile, operation: HostOperation) -> bool {
    profile.links(operation)
        && !(operation == HostOperation::JsonParseV19 && json_hook::enabled())
        && !import_hook::imports(operation)
}

/// Whether the linked `operation` returns its text as an owned guest string
/// (v38): its pointer, with the byte length in `env_result_len`, a fresh
/// unrooted block from the guest allocator that the call site packs into a
/// string handle instead of copying it out of the result buffer.
pub(crate) fn owned_text(profile: LanguageProfile, operation: HostOperation) -> bool {
    links(profile, operation)
        && (operation.returns_string()
            || matches!(
                operation,
                HostOperation::BigIntToString | HostOperation::BigIntToStringV2
            ))
}

/// Test-only switch: while set on the current thread, v38 modules import
/// the listed linked operations from the host instead of linking them, so
/// tests compare linked serialization with the host imports. Importing
/// `value_format_v12` means logging through the host's `value_log_v12`.
pub(crate) mod import_hook {
    #[cfg(any(test, feature = "suspension-test-hook"))]
    use std::cell::RefCell;
    use varta_common::HostOperation;
    #[cfg(any(test, feature = "suspension-test-hook"))]
    thread_local! {
        static IMPORTED: RefCell<Vec<HostOperation>> = const { RefCell::new(Vec::new()) };
    }
    /// Whether the switch imports `operation` on this thread.
    #[cfg(any(test, feature = "suspension-test-hook"))]
    pub(crate) fn imports(operation: HostOperation) -> bool {
        IMPORTED.with(|imported| imported.borrow().contains(&operation))
    }
    /// Without the test switch nothing linked is imported.
    #[cfg(not(any(test, feature = "suspension-test-hook")))]
    pub(crate) fn imports(_: HostOperation) -> bool {
        false
    }
    /// Run `f` with `operations` imported on this thread.
    #[cfg(any(test, feature = "suspension-test-hook"))]
    pub(crate) fn with<T>(operations: &[HostOperation], f: impl FnOnce() -> T) -> T {
        IMPORTED.with(|imported| *imported.borrow_mut() = operations.to_vec());
        let result = f();
        IMPORTED.with(|imported| imported.borrow_mut().clear());
        result
    }
}

/// Test-only switch: while enabled on the current thread, v38 modules import
/// `json_parse_v19` from the host instead of linking it, so tests compare the
/// linked decoder with the host import (the same shared code).
pub(crate) mod json_hook {
    #[cfg(any(test, feature = "suspension-test-hook"))]
    use std::cell::Cell;
    #[cfg(any(test, feature = "suspension-test-hook"))]
    thread_local! {
        static ENABLED: Cell<bool> = const { Cell::new(false) };
    }
    /// Whether the switch is on for this thread.
    pub(crate) fn enabled() -> bool {
        #[cfg(any(test, feature = "suspension-test-hook"))]
        return ENABLED.with(Cell::get);
        #[cfg(not(any(test, feature = "suspension-test-hook")))]
        false
    }
    /// Run `f` with the switch on for this thread.
    #[cfg(any(test, feature = "suspension-test-hook"))]
    pub(crate) fn with<T>(f: impl FnOnce() -> T) -> T {
        ENABLED.with(|enabled| enabled.set(true));
        let result = f();
        ENABLED.with(|enabled| enabled.set(false));
        result
    }
}

/// Link the intrinsics `guest` imports when `profile` links them; modules of
/// every other profile are returned unchanged. `heap` is the guest
/// allocator's heap-end global and `values` its value helpers, when the
/// module has a managed heap.
pub(crate) fn link(
    guest: Vec<u8>,
    profile: LanguageProfile,
    heap: Option<u32>,
    values: Option<GuestValues>,
) -> Result<Vec<u8>, CompilerError> {
    if !profile.linked_intrinsics() {
        return Ok(guest);
    }
    let parts = merge::Guest::parse(&guest).map_err(CompilerError::Internal)?;
    let linked: Vec<(u32, HostOperation)> = parts
        .imports
        .iter()
        .enumerate()
        .filter_map(|(index, import)| {
            let operation = (import.module == varta_common::abi::HOST_MODULE)
                .then(|| HostOperation::from_name(import.name))
                .flatten()?;
            links(profile, operation).then_some((index as u32, operation))
        })
        .collect();
    if linked.is_empty() {
        return Ok(guest);
    }
    let library = library().map_err(CompilerError::Internal)?;
    merge::merge(&guest, &parts, library, &linked, heap, values).map_err(CompilerError::Internal)
}

/// Pages of the intrinsic region a module of `profile` importing `hosts`
/// needs: zero when it links nothing, otherwise the library's stack and the
/// data its linked operations reach, placed after `region`'s extension.
pub(crate) fn region_pages(
    profile: LanguageProfile,
    hosts: impl IntoIterator<Item = HostOperation>,
    region: abi::StaticRegion,
) -> Result<u32, CompilerError> {
    let operations: Vec<HostOperation> = hosts
        .into_iter()
        .filter(|&operation| links(profile, operation))
        .collect();
    if operations.is_empty() {
        return Ok(0);
    }
    let library = library().map_err(CompilerError::Internal)?;
    let roots = operations
        .iter()
        .map(|operation| {
            library.export(operation.name()).ok_or_else(|| {
                CompilerError::Internal(format!(
                    "intrinsic library lacks linked operation {}",
                    operation.name()
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let closure = library.closure(&roots).map_err(CompilerError::Internal)?;
    library
        .place(&closure, region.intrinsic_base())
        .and_then(|placement| placement.pages())
        .map_err(CompilerError::Internal)
}

#[cfg(test)]
mod tests;
