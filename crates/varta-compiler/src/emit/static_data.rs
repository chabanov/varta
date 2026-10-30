//! Static data placement. Every profile fills the first page in order; a
//! `program_limits` profile (v38) places an item that no longer fits the first
//! page in the static extension after the root stack
//! ([`varta_common::abi::StaticRegion`]). Items never straddle the regions.
use crate::CompilerError;
use varta_common::{abi, language::LanguageProfile};

pub(super) struct Placement {
    extended: bool,
    /// Image of `[0, page.len())`.
    page: Vec<u8>,
    /// Image of `[abi::OWNED_STRING_BASE, OWNED_STRING_BASE + extension.len())`.
    extension: Vec<u8>,
}
impl Placement {
    pub(super) fn new(profile: LanguageProfile) -> Self {
        Self {
            extended: profile.program_limits(),
            page: Vec::new(),
            extension: Vec::new(),
        }
    }
    /// Append `bytes` to the first region with room and return its address,
    /// or `None` when it fits neither.
    pub(super) fn push(&mut self, bytes: &[u8]) -> Option<u32> {
        if self
            .page
            .len()
            .checked_add(bytes.len())
            .is_some_and(|end| end <= abi::RESULT_BASE)
        {
            let at = self.page.len();
            self.page.extend_from_slice(bytes);
            return u32::try_from(at).ok();
        }
        if !self.extended {
            return None;
        }
        let at = abi::OWNED_STRING_BASE.checked_add(self.extension.len())?;
        let end = at.checked_add(bytes.len())?;
        // Every static address is an i32 pointer below the memory budget.
        if end > abi::MAX_MEMORY_BYTES {
            return None;
        }
        self.extension.extend_from_slice(bytes);
        u32::try_from(at).ok()
    }
    /// The region the placed data needs; its heap base follows the extension.
    pub(super) fn region(&self) -> Result<abi::StaticRegion, CompilerError> {
        let region = abi::StaticRegion::for_extension_bytes(self.extension.len())
            .filter(|region| {
                region.minimum_pages() <= (abi::MAX_MEMORY_BYTES / abi::WASM_PAGE_BYTES) as u64
            })
            .ok_or_else(|| {
                CompilerError::Invalid("static data exceeds the memory budget".into())
            })?;
        Ok(region)
    }
    /// The nonempty region images with their base addresses.
    pub(super) fn images(&self) -> impl Iterator<Item = (u32, &[u8])> {
        [
            (0, self.page.as_slice()),
            (abi::OWNED_STRING_BASE as u32, &self.extension),
        ]
        .into_iter()
        .filter(|(_, image)| !image.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_profiles_keep_one_page() {
        let mut placement = Placement::new(LanguageProfile::CheckedF64Utf16V37);
        assert_eq!(placement.push(&[1; abi::RESULT_BASE - 1]), Some(0));
        assert_eq!(placement.push(&[2]), Some(abi::RESULT_BASE as u32 - 1));
        assert_eq!(placement.push(&[3]), None);
        assert_eq!(placement.region().unwrap(), abi::StaticRegion::SINGLE_PAGE);
    }

    #[test]
    fn program_limits_spill_into_the_extension_first_fit() {
        let mut placement = Placement::new(LanguageProfile::CheckedF64Utf16V38);
        assert_eq!(placement.push(&[1; abi::RESULT_BASE - 4]), Some(0));
        let spilled = placement.push(&[2; 8]).unwrap();
        assert_eq!(spilled as usize, abi::OWNED_STRING_BASE);
        // A later small item still fills the first page.
        assert_eq!(placement.push(&[3; 4]), Some(abi::RESULT_BASE as u32 - 4));
        let region = placement.region().unwrap();
        assert_eq!(region.extension_pages(), 1);
        assert!(region.contains(spilled as usize, 8));
        let images: Vec<_> = placement
            .images()
            .map(|(at, bytes)| (at, bytes.len()))
            .collect();
        assert_eq!(
            images,
            [(0, abi::RESULT_BASE), (abi::OWNED_STRING_BASE as u32, 8)]
        );
        // The memory budget bounds the extension.
        assert_eq!(placement.push(&vec![0; abi::MAX_MEMORY_BYTES]), None);
    }
}
