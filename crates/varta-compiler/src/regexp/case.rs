//! Case-insensitive matching (`i`), resolved at compile time: a character or
//! class matches every character with the same Canonicalize value
//! (§22.2.2.7.3), so the compiler widens it to the whole equivalence class
//! and the matcher compares characters exactly.
//!
//! Canonicalize uses the Unicode tables of the Rust standard library:
//! `toUppercase` per code unit without `u`, and simple case folding with `u`
//! (derived as the lowercase of the single-character uppercase mapping, with
//! the Turkic dotted/dotless I kept apart as in CaseFolding.txt).
use super::set::{MAX_CODE_POINT, MAX_UNIT, Set};
use std::{collections::BTreeMap, sync::OnceLock};

/// Every code point above this one is caseless in Unicode 16.
const CASED_LIMIT: u32 = 0x1_FFFF;

/// Canonicalize(rer, ch), shared with the v38 linked matcher's
/// case-insensitive backreferences (`varta_common::regexp`).
pub(crate) fn canonicalize(ch: u32, unicode: bool) -> u32 {
    varta_common::regexp::canonicalize(ch, unicode)
}

/// Equivalence classes with more than one member.
fn groups(unicode: bool) -> &'static [Vec<u32>] {
    static UNITS: OnceLock<Vec<Vec<u32>>> = OnceLock::new();
    static CODE_POINTS: OnceLock<Vec<Vec<u32>>> = OnceLock::new();
    let (cell, limit) = if unicode {
        (&CODE_POINTS, CASED_LIMIT)
    } else {
        (&UNITS, MAX_UNIT)
    };
    cell.get_or_init(|| {
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
    })
}

/// `set` widened to whole equivalence classes.
pub(crate) fn close(set: &Set, unicode: bool) -> Set {
    let mut closed = set.clone();
    for group in groups(unicode) {
        if group.iter().any(|member| set.contains(*member)) {
            for &member in group {
                closed.add(member, member);
            }
        }
    }
    closed
}

/// The code points whose simple case folding differs from themselves, with
/// their folding (v38 `v` with `i`).
fn folded() -> &'static [(u32, u32)] {
    static FOLDED: OnceLock<Vec<(u32, u32)>> = OnceLock::new();
    FOLDED.get_or_init(|| {
        (0..=CASED_LIMIT)
            .filter_map(|ch| {
                let canonical = canonicalize(ch, true);
                (canonical != ch).then_some((ch, canonical))
            })
            .collect()
    })
}

/// MaybeSimpleCaseFolding of a set (v38 `v` with `i`): the simple case
/// folding of every member.
pub(crate) fn fold(set: &Set) -> Set {
    let mut image = set.intersection(universe());
    for &(ch, canonical) in folded() {
        if set.contains(ch) {
            image.add(canonical, canonical);
        }
    }
    image
}

/// AllCharacters under `v` with `i`: the code points that are their own
/// simple case folding.
pub(crate) fn universe() -> &'static Set {
    static UNIVERSE: OnceLock<Set> = OnceLock::new();
    UNIVERSE.get_or_init(|| {
        Set::from_ranges(folded().iter().map(|&(ch, _)| (ch, ch))).complement(MAX_CODE_POINT)
    })
}
