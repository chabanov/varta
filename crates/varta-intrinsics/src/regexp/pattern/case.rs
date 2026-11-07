//! Case-insensitive matching (`i`), resolved when the pattern is compiled: a
//! character or class matches every character with the same Canonicalize
//! value (§22.2.2.7.3), so the pattern compiler widens it to the whole
//! equivalence class and the matcher compares characters exactly.
//!
//! Canonicalize uses the Unicode tables of the Rust core library:
//! `toUppercase` per code unit without `u`, and simple case folding with `u`
//! (derived as the lowercase of the single-character uppercase mapping, with
//! the Turkic dotted/dotless I kept apart as in CaseFolding.txt). Its
//! equivalence classes are checked-in tables (`case_table`), derived from
//! [`canonicalize`] over every cased character and kept equal to it by a
//! test, so a pattern compiled in the guest does not walk the code space.
use super::case_table::{FOLDING, UNICODE_CLASSES, UNICODE_MEMBERS, UNIT_CLASSES, UNIT_MEMBERS};
use super::set::{MAX_CODE_POINT, Set};
use alloc::vec::Vec;

/// Every code point above this one is caseless (checked against `core`).
pub const CASED_LIMIT: u32 = 0x1_FFFF;

/// Canonicalize(rer, ch), shared with the v38 linked matcher's
/// case-insensitive backreferences (`varta_common::regexp`).
#[must_use]
pub fn canonicalize(ch: u32, unicode: bool) -> u32 {
    varta_common::regexp::canonicalize(ch, unicode)
}

/// Equivalence classes with more than one member, and every member of one
/// with its class index, by member.
fn classes(unicode: bool) -> (&'static [&'static [u32]], &'static [(u32, u16)]) {
    if unicode {
        (UNICODE_CLASSES, UNICODE_MEMBERS)
    } else {
        (UNIT_CLASSES, UNIT_MEMBERS)
    }
}

/// `set` widened to whole equivalence classes: the classes of the members
/// inside its ranges, found by binary search.
#[must_use]
pub fn close(set: &Set, unicode: bool) -> Set {
    let (classes, members) = classes(unicode);
    let mut added: Vec<(u32, u32)> = Vec::new();
    for &(low, high) in set.ranges() {
        let first = members.partition_point(|&(member, _)| member < low);
        for &(member, class) in &members[first..] {
            if member > high {
                break;
            }
            if let Some(class) = classes.get(usize::from(class)) {
                added.extend(class.iter().map(|&other| (other, other)));
            }
        }
    }
    let mut closed = set.clone();
    closed.union(&Set::from_ranges(added));
    closed
}

/// MaybeSimpleCaseFolding of a set (v38 `v` with `i`): the simple case
/// folding of every member.
#[must_use]
pub fn fold(set: &Set) -> Set {
    let mut image = set.intersection(&universe());
    let folded: Vec<(u32, u32)> = FOLDING
        .iter()
        .filter(|(ch, _)| set.contains(*ch))
        .map(|&(_, canonical)| (canonical, canonical))
        .collect();
    image.union(&Set::from_ranges(folded));
    image
}

/// AllCharacters under `v` with `i`: the code points that are their own
/// simple case folding.
#[must_use]
pub fn universe() -> Set {
    Set::from_ranges(FOLDING.iter().map(|&(ch, _)| (ch, ch))).complement(MAX_CODE_POINT)
}

/// The equivalence classes of Canonicalize with more than one member, in
/// order of their canonical value, derived from [`canonicalize`] over
/// `0..=limit` (the generator and check of `case_table`).
#[cfg(test)]
pub(super) fn derived_classes(unicode: bool) -> Vec<Vec<u32>> {
    use alloc::collections::BTreeMap;
    let limit = if unicode {
        CASED_LIMIT
    } else {
        super::set::MAX_UNIT
    };
    let mut classes: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for ch in 0..=limit {
        classes
            .entry(canonicalize(ch, unicode))
            .or_default()
            .push(ch);
    }
    classes
        .into_values()
        .filter(|members| members.len() > 1)
        .collect()
}

/// Every member of `classes` with its class index, by member (the
/// generator and check of `case_table`).
#[cfg(test)]
pub(super) fn derived_members(classes: &[Vec<u32>]) -> Vec<(u32, u16)> {
    let mut members: Vec<(u32, u16)> = classes
        .iter()
        .enumerate()
        .flat_map(|(index, class)| {
            let index = u16::try_from(index).unwrap(); // tests: fewer than 2^16 classes
            class.iter().map(move |&member| (member, index))
        })
        .collect();
    members.sort_unstable();
    members
}

/// The code points whose simple case folding differs from themselves, with
/// their folding (the generator and check of `case_table`).
#[cfg(test)]
pub(super) fn derived_folding() -> Vec<(u32, u32)> {
    (0..=CASED_LIMIT)
        .filter_map(|ch| {
            let canonical = canonicalize(ch, true);
            (canonical != ch).then_some((ch, canonical))
        })
        .collect()
}
