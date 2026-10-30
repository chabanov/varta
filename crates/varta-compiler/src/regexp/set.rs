//! Sets of pattern characters (UTF-16 code units, or code points under `u`)
//! as sorted, disjoint, non-adjacent inclusive ranges.

/// The largest UTF-16 code unit.
pub(crate) const MAX_UNIT: u32 = 0xFFFF;
/// The largest Unicode code point.
pub(crate) const MAX_CODE_POINT: u32 = 0x10_FFFF;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Set {
    ranges: Vec<(u32, u32)>,
}

impl Set {
    pub(crate) fn single(value: u32) -> Self {
        Self {
            ranges: vec![(value, value)],
        }
    }
    pub(crate) fn from_ranges(ranges: impl IntoIterator<Item = (u32, u32)>) -> Self {
        let mut set = Self::default();
        for (low, high) in ranges {
            set.add(low, high);
        }
        set
    }
    pub(crate) fn ranges(&self) -> &[(u32, u32)] {
        &self.ranges
    }
    /// The only member, if the set has exactly one.
    pub(crate) fn only(&self) -> Option<u32> {
        match self.ranges.as_slice() {
            [(low, high)] if low == high => Some(*low),
            _ => None,
        }
    }
    pub(crate) fn contains(&self, value: u32) -> bool {
        self.ranges
            .binary_search_by(|(low, high)| {
                if *high < value {
                    std::cmp::Ordering::Less
                } else if *low > value {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .is_ok()
    }
    pub(crate) fn add(&mut self, low: u32, high: u32) {
        debug_assert!(low <= high);
        let mut merged = Vec::with_capacity(self.ranges.len() + 1);
        let (mut low, mut high) = (low, high);
        let mut placed = false;
        for &(a, b) in &self.ranges {
            if b.saturating_add(1) < low {
                merged.push((a, b));
            } else if high.saturating_add(1) < a {
                if !placed {
                    merged.push((low, high));
                    placed = true;
                }
                merged.push((a, b));
            } else {
                low = low.min(a);
                high = high.max(b);
            }
        }
        if !placed {
            merged.push((low, high));
        }
        merged.sort_unstable();
        self.ranges = merged;
    }
    pub(crate) fn union(&mut self, other: &Self) {
        for &(low, high) in &other.ranges {
            self.add(low, high);
        }
    }
    /// The complement within `0..=max`.
    pub(crate) fn complement(&self, max: u32) -> Self {
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
pub(crate) fn digits() -> Set {
    Set::from_ranges([(u32::from(b'0'), u32::from(b'9'))])
}
/// `\w` without the `u`+`i` extra word characters.
pub(crate) fn word() -> Set {
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
pub(crate) const EXTRA_WORD: [u32; 2] = [0x017F, 0x212A];
/// ECMAScript WhiteSpace and LineTerminator (`\s`).
pub(crate) fn space() -> Set {
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
pub(crate) const LINE_TERMINATORS: [u32; 4] = [0x0A, 0x0D, 0x2028, 0x2029];
pub(crate) fn line_terminators() -> Set {
    Set::from_ranges(LINE_TERMINATORS.map(|unit| (unit, unit)))
}
