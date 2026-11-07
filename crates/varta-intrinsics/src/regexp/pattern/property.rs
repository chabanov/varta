//! Unicode property escapes `\p{…}`/`\P{…}` (v38), resolved at compile time
//! into class ranges: `General_Category` (`gc`) values and groups, `Script`
//! (`sc`), `Script_Extensions` (`scx`), the binary properties of ECMA-262's
//! table, `Any`, `ASCII` and `Assigned` from the ICU4X 2.4 compiled data,
//! and under `v` the emoji properties of strings of Unicode Emoji 17.0
//! (`emoji_sequences.txt`, compiled into `emoji_table`). Names match
//! strictly, as the specification requires (`\p{l}` and `\p{ L}` are
//! errors).
use super::set::{MAX_CODE_POINT, Set};
use alloc::{collections::BTreeSet, vec::Vec};
use icu_properties::{
    CodePointMapData, CodePointSetData, PropertyParser,
    props::{GeneralCategory, GeneralCategoryGroup, Script},
    script::ScriptWithExtensions,
};

/// A resolved property: code points, and under `v` strings of several.
pub(super) struct Property {
    pub set: Set,
    pub strings: BTreeSet<Vec<u32>>,
}

const GENERAL_CATEGORY: [&str; 2] = ["General_Category", "gc"];
const SCRIPT: [&str; 2] = ["Script", "sc"];
const SCRIPT_EXTENSIONS: [&str; 2] = ["Script_Extensions", "scx"];
const ASCII_LAST: u32 = 0x7F;
/// The union of every emoji property of strings (UTS #51 ED-27).
const RGI_EMOJI: &str = "RGI_Emoji";
/// The properties of strings `RGI_Emoji` unites.
const RGI_PARTS: [&str; 6] = [
    "Basic_Emoji",
    "Emoji_Keycap_Sequence",
    "RGI_Emoji_Modifier_Sequence",
    "RGI_Emoji_Flag_Sequence",
    "RGI_Emoji_Tag_Sequence",
    "RGI_Emoji_ZWJ_Sequence",
];

fn ranges(ranges: impl Iterator<Item = core::ops::RangeInclusive<u32>>) -> Set {
    Set::from_ranges(ranges.map(|range| (*range.start(), *range.end())))
}
fn general_category(value: &str) -> Option<Set> {
    let group = PropertyParser::<GeneralCategoryGroup>::new().get_strict(value)?;
    Some(ranges(
        CodePointMapData::<GeneralCategory>::new().iter_ranges_for_group(group),
    ))
}
fn script(value: &str, extensions: bool) -> Option<Set> {
    let script = PropertyParser::<Script>::new().get_strict(value)?;
    if extensions {
        return Some(ranges(
            ScriptWithExtensions::new().get_script_extensions_ranges(script),
        ));
    }
    Some(ranges(
        CodePointMapData::<Script>::new().iter_ranges_for_value(script),
    ))
}
/// A lone name: a binary property, a pseudo-property or a
/// `General_Category` value.
fn lone(name: &str) -> Option<Set> {
    match name {
        "Any" => return Some(Set::from_ranges([(0, MAX_CODE_POINT)])),
        "ASCII" => return Some(Set::from_ranges([(0, ASCII_LAST)])),
        "Assigned" => return Some(general_category("Cn")?.complement(MAX_CODE_POINT)),
        _ => {}
    }
    if let Some(set) = general_category(name) {
        return Some(set);
    }
    CodePointSetData::new_for_ecma262(name.as_bytes()).map(|data| ranges(data.iter_ranges()))
}
/// The sequences of the emoji property of strings `name`.
fn emoji(name: &str) -> Option<Vec<Vec<u32>>> {
    if name == RGI_EMOJI {
        let mut all = Vec::new();
        for part in RGI_PARTS {
            all.extend(emoji(part)?);
        }
        return Some(all);
    }
    let (_, mut table) = *super::emoji_table::PROPERTIES
        .iter()
        .find(|(property, _)| *property == name)?;
    let mut sequences = Vec::new();
    while let Some((&length, rest)) = table.split_first() {
        let (sequence, next) = rest.split_at_checked(usize::try_from(length).ok()?)?;
        sequences.push(sequence.to_vec());
        table = next;
    }
    Some(sequences)
}

/// The sequences of the emoji property of strings `name` in the text of
/// `emoji_sequences.txt` (the generator and check of `emoji_table`).
#[cfg(test)]
pub(super) fn emoji_text(text: &str, name: &str) -> Option<Vec<Vec<u32>>> {
    fn header(line: &str) -> Option<&str> {
        line.strip_prefix('[')?.strip_suffix(']')
    }
    let mut lines = text.lines().skip_while(|line| header(line) != Some(name));
    lines.next()?;
    let hex = |text: &str| u32::from_str_radix(text, 16).ok();
    let mut sequences = Vec::new();
    for line in lines.take_while(|line| !line.starts_with('[')) {
        if let Some((first, last)) = line.split_once("..") {
            for code_point in hex(first)?..=hex(last)? {
                sequences.push(alloc::vec![code_point]);
            }
        } else {
            sequences.push(line.split(' ').map(hex).collect::<Option<Vec<_>>>()?);
        }
    }
    Some(sequences)
}

/// The emoji properties of strings `RGI_Emoji` unites.
#[cfg(test)]
pub(super) const EMOJI_PARTS: [&str; 6] = RGI_PARTS;

/// The property `name` (`name=value` when `value` is present), with the
/// properties of strings when `strings` (`v`, not negated); `None` for an
/// unknown name or value.
pub(super) fn lookup(name: &str, value: Option<&str>, strings: bool) -> Option<Property> {
    let set = match value {
        Some(value) if GENERAL_CATEGORY.contains(&name) => general_category(value),
        Some(value) if SCRIPT.contains(&name) => script(value, false),
        Some(value) if SCRIPT_EXTENSIONS.contains(&name) => script(value, true),
        Some(_) => None,
        None => lone(name),
    };
    if let Some(set) = set {
        return Some(Property {
            set,
            strings: BTreeSet::new(),
        });
    }
    if value.is_some() || !strings || !(name == RGI_EMOJI || RGI_PARTS.contains(&name)) {
        return None;
    }
    let mut singles = Vec::new();
    let mut strings = BTreeSet::new();
    for sequence in emoji(name)? {
        match sequence.as_slice() {
            [single] => singles.push((*single, *single)),
            _ => {
                strings.insert(sequence);
            }
        }
    }
    Some(Property {
        set: Set::from_ranges(singles),
        strings,
    })
}
