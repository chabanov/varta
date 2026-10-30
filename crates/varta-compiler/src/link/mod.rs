//! Function-level linking of the guest intrinsic library (v38
//! `linked_intrinsics`).
//!
//! After emission, every `varta.<name>` import whose operation the profile
//! links ([`LanguageProfile::links`]) is replaced by the library function of
//! the same name and signature, together with exactly the library functions,
//! globals and data segments it reaches. Guest code is not otherwise changed:
//! defined guest functions keep their order (catalog `code` numbers stay
//! valid), linked code is appended after them, and the library runs on a
//! second, private memory that is never exported. The library reaches guest
//! memory 0 and the result globals only through the accessor functions
//! synthesized here.
mod library;
mod merge;

use library::library;

use crate::CompilerError;
use varta_common::{HostOperation, language::LanguageProfile};

/// Link the intrinsics `guest` imports when `profile` links them; modules of
/// every other profile are returned unchanged.
pub(crate) fn link(guest: Vec<u8>, profile: LanguageProfile) -> Result<Vec<u8>, CompilerError> {
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
    merge::merge(&guest, &parts, library, &linked).map_err(CompilerError::Internal)
}

#[cfg(test)]
mod tests;
