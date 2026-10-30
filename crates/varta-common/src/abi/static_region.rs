//! The static data region of a module (host ABI 34, v38 `program_limits`).
//!
//! Every earlier profile keeps static data in the single page
//! `[0, RESULT_BASE)`. A v38 module may extend the region with whole Wasm
//! pages placed after the root stack, `[OWNED_STRING_BASE, heap_base)`,
//! declared once by the [`STATIC_SECTION`] custom section; its managed heap
//! then begins at `heap_base`. The result buffer and the root stack keep
//! their addresses, so host results and exception roots are unchanged.
//!
//! A v38 module that links intrinsics (`linked_intrinsics`) also reserves the
//! intrinsic region after the extension, declared once by the
//! [`INTRINSIC_SECTION`] custom section: the linked library's shadow stack
//! and static data. It is neither static data nor heap; the heap follows it.
use super::{OWNED_STRING_BASE, RESULT_BASE, WASM_PAGE_BYTES};

/// Custom section declaring the static extension: one u32 LE page count.
pub const STATIC_SECTION: &str = "varta.static";
/// Custom section declaring the intrinsic region (v38 `linked_intrinsics`):
/// one u32 LE page count of at least one page.
pub const INTRINSIC_SECTION: &str = "varta.intrinsics";
/// Wasm memory32 address space in pages (the Wasm implementation limit).
pub const WASM_MEMORY32_PAGES: u64 = 1 << 16;
/// Pages the extension and the intrinsic region may declare together: the
/// memory32 address space after the fixed regions. The memory budget bounds
/// a module much earlier: its minimum memory must cover `heap_base` and may
/// not exceed its maximum.
pub const MAX_STATIC_EXTENSION_PAGES: u32 =
    (WASM_MEMORY32_PAGES - (OWNED_STRING_BASE / WASM_PAGE_BYTES) as u64) as u32;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StaticRegionError {
    #[error("static region section must hold one u32 page count")]
    Malformed,
    #[error("static region extension exceeds {MAX_STATIC_EXTENSION_PAGES} pages")]
    TooLarge,
    #[error("static region extension must declare at least one page")]
    Empty,
    #[error("intrinsic region must declare at least one page")]
    EmptyIntrinsics,
}

/// Static data placement shared by the compiler, runtime and artifact checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StaticRegion {
    extension_pages: u32,
    intrinsic_pages: u32,
}
impl StaticRegion {
    /// Static data in `[0, RESULT_BASE)` only: every profile before v38 and
    /// v38 modules without a [`STATIC_SECTION`].
    pub const SINGLE_PAGE: Self = Self {
        extension_pages: 0,
        intrinsic_pages: 0,
    };
    #[must_use]
    pub fn with_extension_pages(pages: u32) -> Option<Self> {
        (pages <= MAX_STATIC_EXTENSION_PAGES).then_some(Self {
            extension_pages: pages,
            intrinsic_pages: 0,
        })
    }
    /// This region followed by an intrinsic region of `pages` pages, within
    /// [`MAX_STATIC_EXTENSION_PAGES`] together with the extension.
    #[must_use]
    pub fn with_intrinsic_pages(self, pages: u32) -> Option<Self> {
        self.extension_pages
            .checked_add(pages)
            .filter(|total| *total <= MAX_STATIC_EXTENSION_PAGES)
            .map(|_| Self {
                intrinsic_pages: pages,
                ..self
            })
    }
    /// The smallest region whose extension holds `bytes`.
    #[must_use]
    pub fn for_extension_bytes(bytes: usize) -> Option<Self> {
        u32::try_from(bytes.div_ceil(WASM_PAGE_BYTES))
            .ok()
            .and_then(Self::with_extension_pages)
    }
    #[must_use]
    pub fn extension_pages(self) -> u32 {
        self.extension_pages
    }
    #[must_use]
    pub fn extension_bytes(self) -> usize {
        self.extension_pages as usize * WASM_PAGE_BYTES
    }
    /// First address of the extension; equal to [`Self::heap_base`] when the
    /// region is a single page.
    #[must_use]
    pub fn extension_base(self) -> usize {
        OWNED_STRING_BASE
    }
    #[must_use]
    pub fn intrinsic_pages(self) -> u32 {
        self.intrinsic_pages
    }
    #[must_use]
    pub fn intrinsic_bytes(self) -> usize {
        self.intrinsic_pages as usize * WASM_PAGE_BYTES
    }
    /// First address of the intrinsic region, right after the extension.
    #[must_use]
    pub fn intrinsic_base(self) -> usize {
        OWNED_STRING_BASE + self.extension_bytes()
    }
    /// Whether `[start, start + len)` lies wholly in the intrinsic region.
    #[must_use]
    pub fn intrinsic_contains(self, start: usize, len: usize) -> bool {
        start
            .checked_add(len)
            .is_some_and(|end| start >= self.intrinsic_base() && end <= self.heap_base())
    }
    /// First address of the managed heap, after the extension and the
    /// intrinsic region.
    #[must_use]
    pub fn heap_base(self) -> usize {
        self.intrinsic_base() + self.intrinsic_bytes()
    }
    /// Minimum Wasm memory pages of a module with a managed heap, an
    /// extension or an intrinsic region.
    #[must_use]
    pub fn minimum_pages(self) -> u64 {
        (self.heap_base() / WASM_PAGE_BYTES) as u64
    }
    /// Whether `[start, start + len)` lies wholly in the first page or wholly
    /// in the extension. Static metadata never straddles the two.
    #[must_use]
    pub fn contains(self, start: usize, len: usize) -> bool {
        let Some(end) = start.checked_add(len) else {
            return false;
        };
        end <= RESULT_BASE || start >= self.extension_base() && end <= self.intrinsic_base()
    }
    /// Read a [`STATIC_SECTION`] payload. A declared extension has at least
    /// one page; a single-page module omits the section.
    pub fn read_section(data: &[u8]) -> Result<Self, StaticRegionError> {
        let pages = u32::from_le_bytes(data.try_into().map_err(|_| StaticRegionError::Malformed)?);
        if pages == 0 {
            return Err(StaticRegionError::Empty);
        }
        Self::with_extension_pages(pages).ok_or(StaticRegionError::TooLarge)
    }
    #[must_use]
    pub fn section_bytes(self) -> [u8; 4] {
        self.extension_pages.to_le_bytes()
    }
    /// This region with the intrinsic region of an [`INTRINSIC_SECTION`]
    /// payload.
    pub fn read_intrinsic_section(self, data: &[u8]) -> Result<Self, StaticRegionError> {
        let pages = u32::from_le_bytes(data.try_into().map_err(|_| StaticRegionError::Malformed)?);
        if pages == 0 {
            return Err(StaticRegionError::EmptyIntrinsics);
        }
        self.with_intrinsic_pages(pages)
            .ok_or(StaticRegionError::TooLarge)
    }
    #[must_use]
    pub fn intrinsic_section_bytes(self) -> [u8; 4] {
        self.intrinsic_pages.to_le_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::super::STRING_ROOT_BASE;
    use super::*;

    #[test]
    fn single_page_keeps_the_legacy_layout() {
        let region = StaticRegion::SINGLE_PAGE;
        assert_eq!(region.heap_base(), OWNED_STRING_BASE);
        assert_eq!(region.minimum_pages(), 18);
        assert!(region.contains(0, RESULT_BASE));
        assert!(!region.contains(RESULT_BASE - 1, 2));
        assert!(!region.contains(OWNED_STRING_BASE, 1));
        assert!(!region.contains(usize::MAX, 2));
    }

    #[test]
    fn extension_follows_the_root_stack_and_moves_the_heap() {
        let region = StaticRegion::for_extension_bytes(WASM_PAGE_BYTES + 1).unwrap();
        assert_eq!(region.extension_pages(), 2);
        assert_eq!(region.heap_base(), OWNED_STRING_BASE + 2 * WASM_PAGE_BYTES);
        assert_eq!(region.minimum_pages(), 20);
        assert!(region.contains(OWNED_STRING_BASE, 2 * WASM_PAGE_BYTES));
        assert!(!region.contains(OWNED_STRING_BASE, 2 * WASM_PAGE_BYTES + 1));
        // The result buffer and the root stack are never static.
        assert!(!region.contains(RESULT_BASE, 1));
        assert!(!region.contains(STRING_ROOT_BASE, 1));
        assert!(!region.contains(RESULT_BASE - 4, OWNED_STRING_BASE));
    }

    #[test]
    fn intrinsic_region_follows_the_extension_and_is_never_static() {
        let region = StaticRegion::with_extension_pages(2)
            .unwrap()
            .with_intrinsic_pages(3)
            .unwrap();
        let base = OWNED_STRING_BASE + 2 * WASM_PAGE_BYTES;
        assert_eq!(region.intrinsic_base(), base);
        assert_eq!(region.heap_base(), base + 3 * WASM_PAGE_BYTES);
        assert_eq!(region.minimum_pages(), 23);
        assert!(region.intrinsic_contains(base, 3 * WASM_PAGE_BYTES));
        assert!(!region.intrinsic_contains(base - 1, 1));
        assert!(!region.intrinsic_contains(region.heap_base() - 1, 2));
        assert!(!region.intrinsic_contains(usize::MAX, 2));
        // The extension stays static data; the intrinsic region never is.
        assert!(region.contains(OWNED_STRING_BASE, 2 * WASM_PAGE_BYTES));
        assert!(!region.contains(base, 1));
        assert!(!region.contains(base - 1, 2));
        // Without an extension the region starts right after the root stack.
        let only = StaticRegion::SINGLE_PAGE.with_intrinsic_pages(1).unwrap();
        assert_eq!(only.intrinsic_base(), OWNED_STRING_BASE);
        assert!(!only.contains(OWNED_STRING_BASE, 1));
        assert_eq!(
            StaticRegion::SINGLE_PAGE.read_intrinsic_section(&only.intrinsic_section_bytes()),
            Ok(only)
        );
        assert_eq!(
            StaticRegion::SINGLE_PAGE.read_intrinsic_section(&0u32.to_le_bytes()),
            Err(StaticRegionError::EmptyIntrinsics)
        );
        assert_eq!(
            StaticRegion::SINGLE_PAGE.read_intrinsic_section(&[1]),
            Err(StaticRegionError::Malformed)
        );
        // Extension and intrinsic region share the memory32 address space.
        let full = StaticRegion::with_extension_pages(MAX_STATIC_EXTENSION_PAGES).unwrap();
        assert_eq!(full.with_intrinsic_pages(1), None);
        assert_eq!(
            StaticRegion::with_extension_pages(1)
                .unwrap()
                .read_intrinsic_section(&MAX_STATIC_EXTENSION_PAGES.to_le_bytes()),
            Err(StaticRegionError::TooLarge)
        );
    }

    #[test]
    fn sections_round_trip_and_reject_malformed_payloads() {
        let region = StaticRegion::with_extension_pages(3).unwrap();
        assert_eq!(
            StaticRegion::read_section(&region.section_bytes()),
            Ok(region)
        );
        assert_eq!(
            StaticRegion::read_section(&[1, 0, 0]),
            Err(StaticRegionError::Malformed)
        );
        assert_eq!(
            StaticRegion::read_section(&0u32.to_le_bytes()),
            Err(StaticRegionError::Empty)
        );
        assert_eq!(
            StaticRegion::read_section(&(MAX_STATIC_EXTENSION_PAGES + 1).to_le_bytes()),
            Err(StaticRegionError::TooLarge)
        );
        assert!(StaticRegion::with_extension_pages(MAX_STATIC_EXTENSION_PAGES).is_some());
        assert_eq!(MAX_STATIC_EXTENSION_PAGES, 65_518);
    }
}
