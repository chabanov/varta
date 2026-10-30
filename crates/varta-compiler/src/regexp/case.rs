//! Case-insensitive matching (`i`), resolved at compile time: a character or
//! class matches every character with the same Canonicalize value
//! (§22.2.2.7.3), so the compiler widens it to the whole equivalence class
//! and the matcher compares characters exactly.
//!
//! Canonicalize uses the Unicode tables of the Rust standard library:
//! `toUppercase` per code unit without `u`, and simple case folding with `u`
//! (derived as the lowercase of the single-character uppercase mapping, with
//! the Turkic dotted/dotless I kept apart as in CaseFolding.txt).
use super::set::{MAX_UNIT, Set};
use std::{collections::BTreeMap, sync::OnceLock};

/// Every code point above this one is caseless in Unicode 16.
const CASED_LIMIT: u32 = 0x1_FFFF;
/// U+0130 and U+0131 have only Turkic (`T`) or full (`F`) foldings.
const TURKIC: [u32; 2] = [0x0130, 0x0131];
/// The first code unit outside ASCII.
const NON_ASCII: u32 = 0x80;

fn single(mut mapped: impl Iterator<Item = char>) -> Option<char> {
    let first = mapped.next()?;
    mapped.next().is_none().then_some(first)
}

/// Canonicalize(rer, ch).
pub(crate) fn canonicalize(ch: u32, unicode: bool) -> u32 {
    let Some(character) = char::from_u32(ch) else {
        return ch;
    };
    if unicode {
        if TURKIC.contains(&ch) {
            return ch;
        }
        let upper = single(character.to_uppercase()).unwrap_or(character);
        return single(upper.to_lowercase())
            .or_else(|| single(character.to_lowercase()))
            .map_or(ch, u32::from);
    }
    let Some(upper) = single(character.to_uppercase()).map(u32::from) else {
        return ch;
    };
    if upper > MAX_UNIT || ch >= NON_ASCII && upper < NON_ASCII {
        return ch;
    }
    upper
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
