//! Bounded read access to guest linear memory.
use alloc::vec::Vec;

/// A read-only view of guest memory 0. The native runtime passes the
/// instance's memory as a byte slice; linked guest code copies ranges out of
/// memory 0 into the intrinsic's private memory. Every read is bounds-checked
/// and an out-of-range read is `None`/`false`, never a partial copy.
pub trait Memory {
    /// Current byte size of guest memory.
    fn size(&self) -> usize;
    /// Copy `out.len()` bytes starting at `at`; false when out of range.
    fn copy_to(&self, at: usize, out: &mut [u8]) -> bool;

    /// An owned copy of `len` bytes at `at`.
    fn bytes(&self, at: usize, len: usize) -> Option<Vec<u8>> {
        at.checked_add(len).filter(|end| *end <= self.size())?;
        let mut out = alloc::vec![0; len];
        self.copy_to(at, &mut out).then_some(out)
    }
    /// A little-endian `u32` at `at`.
    fn u32_at(&self, at: usize) -> Option<u32> {
        let mut word = [0; 4];
        self.copy_to(at, &mut word)
            .then(|| u32::from_le_bytes(word))
    }
    /// A little-endian `u64` at `at`.
    fn u64_at(&self, at: usize) -> Option<u64> {
        let mut word = [0; 8];
        self.copy_to(at, &mut word)
            .then(|| u64::from_le_bytes(word))
    }
}

impl Memory for [u8] {
    fn size(&self) -> usize {
        self.len()
    }
    fn copy_to(&self, at: usize, out: &mut [u8]) -> bool {
        match at.checked_add(out.len()).and_then(|end| self.get(at..end)) {
            Some(bytes) => {
                out.copy_from_slice(bytes);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_are_exact_or_absent() {
        let data: &[u8] = &[1, 0, 0, 0, 2, 0, 0, 0, 3];
        assert_eq!(data.u32_at(0), Some(1));
        assert_eq!(data.u64_at(0), Some(1 | (2 << 32)));
        assert_eq!(data.u32_at(6), None);
        assert_eq!(data.bytes(8, 1).as_deref(), Some(&[3][..]));
        assert_eq!(data.bytes(8, 2), None);
        assert_eq!(data.bytes(usize::MAX, 2), None);
    }
}
