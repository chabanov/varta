//! Sets of pattern characters (UTF-16 code units, or code points under `u`)
//! as sorted, disjoint, non-adjacent inclusive ranges.
use alloc::{vec, vec::Vec};

/// The largest UTF-16 code unit.
pub const MAX_UNIT: u32 = 0xFFFF;
/// The largest Unicode code point.
pub const MAX_CODE_POINT: u32 = 0x10_FFFF;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Set {
    ranges: Vec<(u32, u32)>,
}

impl Set {
    pub fn single(value: u32) -> Self {
        Self {
            ranges: vec![(value, value)],
        }
    }
    /// The set of every range (in any order, overlapping or not).
    pub fn from_ranges(ranges: impl IntoIterator<Item = (u32, u32)>) -> Self {
        Self::normalized(ranges.into_iter().collect())
    }
    /// Sorted, disjoint, non-adjacent ranges covering `ranges`, in
    /// O(n log n): run-time patterns build large property sets in the guest.
    fn normalized(mut ranges: Vec<(u32, u32)>) -> Self {
        ranges.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::with_capacity(ranges.len());
        for (low, high) in ranges {
            debug_assert!(low <= high);
            match merged.last_mut() {
                Some(last) if low <= last.1.saturating_add(1) => last.1 = last.1.max(high),
                _ => merged.push((low, high)),
            }
        }
        Self { ranges: merged }
    }
    pub fn ranges(&self) -> &[(u32, u32)] {
        &self.ranges
    }
    /// The only member, if the set has exactly one.
    pub fn only(&self) -> Option<u32> {
        match self.ranges.as_slice() {
            [(low, high)] if low == high => Some(*low),
            _ => None,
        }
    }
    pub fn contains(&self, value: u32) -> bool {
        self.ranges
            .binary_search_by(|(low, high)| {
                if *high < value {
                    core::cmp::Ordering::Less
                } else if *low > value {
                    core::cmp::Ordering::Greater
                } else {
                    core::cmp::Ordering::Equal
                }
            })
            .is_ok()
    }
    pub fn add(&mut self, low: u32, high: u32) {
        debug_assert!(low <= high);
        self.union(&Self {
            ranges: vec![(low, high)],
        });
    }
    /// Add every member of `other`: a linear merge of the two sorted range
    /// lists.
    pub fn union(&mut self, other: &Self) {
        if other.ranges.is_empty() {
            return;
        }
        let mine = core::mem::take(&mut self.ranges);
        let mut merged: Vec<(u32, u32)> = Vec::with_capacity(mine.len() + other.ranges.len());
        let (mut a, mut b) = (mine.iter().peekable(), other.ranges.iter().peekable());
        loop {
            let next = match (a.peek(), b.peek()) {
                (Some(left), Some(right)) if left <= right => a.next(),
                (Some(_), Some(_)) | (None, Some(_)) => b.next(),
                (Some(_), None) => a.next(),
                (None, None) => break,
            };
            let Some(&(low, high)) = next else { break };
            match merged.last_mut() {
                Some(last) if low <= last.1.saturating_add(1) => last.1 = last.1.max(high),
                _ => merged.push((low, high)),
            }
        }
        self.ranges = merged;
    }
    /// The members of both sets (v38 class intersection).
    pub fn intersection(&self, other: &Self) -> Self {
        let mut ranges = Vec::new();
        let (mut a, mut b) = (0, 0);
        while a < self.ranges.len() && b < other.ranges.len() {
            let (a_low, a_high) = self.ranges[a];
            let (b_low, b_high) = other.ranges[b];
            let (low, high) = (a_low.max(b_low), a_high.min(b_high));
            if low <= high {
                ranges.push((low, high));
            }
            if a_high < b_high {
                a += 1;
            } else {
                b += 1;
            }
        }
        Self { ranges }
    }
    /// The members of `self` outside `other` (v38 class subtraction).
    pub fn difference(&self, other: &Self) -> Self {
        self.intersection(&other.complement(MAX_CODE_POINT))
    }
    /// The complement within `0..=max`.
    pub fn complement(&self, max: u32) -> Self {
        let mut ranges = Vec::new();
        let mut next = 0;
        for &(low, high) in &self.ranges {
            if low > next {
                ranges.push((next, low - 1));
            }
            next = high + 1;
        }
        if next <= max {
            ranges.push((next, max));
        }
        Self { ranges }
    }
}

/// `\d`.
pub fn digits() -> Set {
    Set::from_ranges([(u32::from(b'0'), u32::from(b'9'))])
}
/// `\w` without the `u`+`i` extra word characters.
pub fn word() -> Set {
    Set::from_ranges([
        (u32::from(b'0'), u32::from(b'9')),
        (u32::from(b'A'), u32::from(b'Z')),
        (u32::from(b'_'), u32::from(b'_')),
        (u32::from(b'a'), u32::from(b'z')),
    ])
}
/// U+017F LATIN SMALL LETTER LONG S and U+212A KELVIN SIGN: the characters
/// outside `\w` whose simple case folding is in `\w` (WordCharacters with
/// `u` and `i`).
pub const EXTRA_WORD: [u32; 2] = [0x017F, 0x212A];
/// ECMAScript WhiteSpace and LineTerminator (`\s`).
pub fn space() -> Set {
    Set::from_ranges([
        (0x09, 0x0D),
        (0x20, 0x20),
        (0xA0, 0xA0),
        (0x1680, 0x1680),
        (0x2000, 0x200A),
        (0x2028, 0x2029),
        (0x202F, 0x202F),
        (0x205F, 0x205F),
        (0x3000, 0x3000),
        (0xFEFF, 0xFEFF),
    ])
}
/// LineTerminator: LF, CR, LS and PS.
pub const LINE_TERMINATORS: [u32; 4] = [0x0A, 0x0D, 0x2028, 0x2029];
pub fn line_terminators() -> Set {
    Set::from_ranges(LINE_TERMINATORS.map(|unit| (unit, unit)))
}
