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

/// Link the intrinsics `guest` imports when `profile` links them; modules of
/// every other profile are returned unchanged. `heap` is the guest
/// allocator's heap-end global, when the module has a managed heap.
pub(crate) fn link(
    guest: Vec<u8>,
    profile: LanguageProfile,
    heap: Option<u32>,
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
            let operation = (import.module == "varta")
                .then(|| HostOperation::from_name(import.name))
                .flatten()?;
            profile
                .links(operation)
                .then_some((index as u32, operation))
        })
        .collect();
    if linked.is_empty() {
        return Ok(guest);
    }
    let library = library().map_err(CompilerError::Internal)?;
    merge::merge(&guest, &parts, library, &linked, heap).map_err(CompilerError::Internal)
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
        .filter(|&operation| profile.links(operation))
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
