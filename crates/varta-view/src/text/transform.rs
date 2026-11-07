//! CSS Text 3 §2.1 `text-transform`: case transforms (`capitalize`,
//! `uppercase`, `lowercase`), `full-width` and `full-size-kana`, applied in
//! that order to the text a box renders (after white-space collapsing,
//! CSS Text 3 §1.3), with the Unicode default case mappings (full mappings
//! of SpecialCasing, no language-specific tailoring: the view has no
//! content language).
use unicode_segmentation::UnicodeSegmentation;

/// A parsed `text-transform` value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TextTransform {
    case: Case,
    full_width: bool,
    full_size_kana: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Case {
    #[default]
    None,
    Capitalize,
    Uppercase,
    Lowercase,
}

impl TextTransform {
    /// The transform of a computed value; `None` outside the grammar.
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let mut transform = Self::default();
        if value == "none" {
            return Some(transform);
        }
        for word in value.split_ascii_whitespace() {
            let case = match word {
                "capitalize" => Case::Capitalize,
                "uppercase" => Case::Uppercase,
                "lowercase" => Case::Lowercase,
                "full-width" if !transform.full_width => {
                    transform.full_width = true;
                    continue;
                }
                "full-size-kana" if !transform.full_size_kana => {
                    transform.full_size_kana = true;
                    continue;
                }
                _ => return None,
            };
            if transform.case != Case::None {
                return None;
            }
            transform.case = case;
        }
        Some(transform)
    }

    /// Whether the transform changes nothing.
    pub(crate) fn is_none(self) -> bool {
        self == Self::default()
    }

    /// `text` transformed as a whole run of one box.
    pub(crate) fn apply(self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for (at, ch) in text.char_indices() {
            let mapped = self.map(ch, &out, &text[at + ch.len_utf8()..]);
            out.push_str(&mapped);
        }
        out
    }

    /// The transform of one character `ch`: `before` is the
    /// text the inline formatting context renders before it (already
    /// transformed) and `after` the rest of the box's own text, the context
    /// of word starts (`capitalize`) and of the final sigma (`lowercase`).
    pub(crate) fn map(self, ch: char, before: &str, after: &str) -> String {
        let mut cased = String::new();
        let mut out = String::new();
        match self.case {
            Case::None => cased.push(ch),
            Case::Uppercase => cased.extend(ch.to_uppercase()),
            Case::Lowercase if ch == CAPITAL_SIGMA && final_sigma(before, after) => {
                cased.push(FINAL_SIGMA);
            }
            Case::Lowercase => cased.extend(ch.to_lowercase()),
            Case::Capitalize if ch.is_lowercase() && first_letter_unit(before, ch) => {
                titlecase(ch, &mut cased);
            }
            Case::Capitalize => cased.push(ch),
        }
        for ch in cased.chars() {
            let ch = if self.full_width { full_width(ch) } else { ch };
            out.push(if self.full_size_kana {
                full_size_kana(ch)
            } else {
                ch
            });
        }
        out
    }
}

const CAPITAL_SIGMA: char = '\u{03A3}';
const FINAL_SIGMA: char = '\u{03C2}';
/// How many characters before a candidate word start decide the UAX #29
/// boundary (ignored extenders and the `ALetter MidLetter ALetter` rules
/// look back a few characters only).
const WORD_CONTEXT: usize = 16;

/// Whether `ch`, after `before`, is the first typographic letter unit (a
/// letter or number, CSS Text 3 §2.1) of its word (UAX #29 boundaries).
fn first_letter_unit(before: &str, ch: char) -> bool {
    let start = before
        .char_indices()
        .rev()
        .nth(WORD_CONTEXT - 1)
        .map_or(0, |(at, _)| at);
    let mut window = before[start..].to_owned();
    let at = window.len();
    window.push(ch);
    let letter = |c: char| c.is_alphabetic() || c.is_numeric();
    window
        .split_word_bound_indices()
        .find(|(begin, word)| *begin <= at && at < begin + word.len())
        .is_some_and(|(begin, word)| !word[..at - begin].chars().any(letter))
}

/// SpecialCasing.txt's `Final_Sigma` condition: a cased letter (skipping
/// case-ignorable characters) before it and none after it.
fn final_sigma(before: &str, after: &str) -> bool {
    let ignorable = |c: &char| {
        matches!(c, '\'' | '.' | ':' | '\u{00B7}' | '\u{2019}' | '\u{00AD}') || unicode_mark(*c)
    };
    let cased = |c: char| c.is_lowercase() || c.is_uppercase();
    before
        .chars()
        .rev()
        .find(|c| !ignorable(c))
        .is_some_and(cased)
        && !after.chars().find(|c| !ignorable(c)).is_some_and(cased)
}

/// A combining mark (the case-ignorable marks of the final-sigma context).
fn unicode_mark(c: char) -> bool {
    matches!(u32::from(c), 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
}

/// The Unicode titlecase mapping (UnicodeData.txt and SpecialCasing.txt):
/// the uppercase mapping except for the characters below.
fn titlecase(ch: char, out: &mut String) {
    let special: Option<&str> = match ch {
        '\u{01C4}'..='\u{01C6}' => Some("\u{01C5}"),
        '\u{01C7}'..='\u{01C9}' => Some("\u{01C8}"),
        '\u{01CA}'..='\u{01CC}' => Some("\u{01CB}"),
        '\u{01F1}'..='\u{01F3}' => Some("\u{01F2}"),
        'ß' => Some("Ss"),
        '\u{FB00}' => Some("Ff"),
        '\u{FB01}' => Some("Fi"),
        '\u{FB02}' => Some("Fl"),
        '\u{FB03}' => Some("Ffi"),
        '\u{FB04}' => Some("Ffl"),
        '\u{FB05}' | '\u{FB06}' => Some("St"),
        '\u{0587}' => Some("\u{0535}\u{0582}"),
        '\u{FB13}' => Some("\u{0544}\u{0576}"),
        '\u{FB14}' => Some("\u{0544}\u{0565}"),
        '\u{FB15}' => Some("\u{0544}\u{056B}"),
        '\u{FB16}' => Some("\u{054E}\u{0576}"),
        '\u{FB17}' => Some("\u{0544}\u{056D}"),
        '\u{0149}' => Some("\u{02BC}N"),
        '\u{01F0}' => Some("J\u{030C}"),
        '\u{1E96}' => Some("H\u{0331}"),
        '\u{1E97}' => Some("T\u{0308}"),
        '\u{1E98}' => Some("W\u{030A}"),
        '\u{1E99}' => Some("Y\u{030A}"),
        '\u{1E9A}' => Some("A\u{02BE}"),
        '\u{1FB3}' => Some("\u{1FBC}"),
        '\u{1FC3}' => Some("\u{1FCC}"),
        '\u{1FF3}' => Some("\u{1FFC}"),
        _ => None,
    };
    // Greek with ypogegrammeni: the titlecase letter, eight code points on,
    // keeps the prosgegrammeni.
    let greek =
        matches!(ch, '\u{1F80}'..='\u{1F87}' | '\u{1F90}'..='\u{1F97}' | '\u{1FA0}'..='\u{1FA7}');
    match special {
        _ if greek => out.extend(char::from_u32(u32::from(ch) + 8)),
        Some(text) => out.push_str(text),
        None => out.extend(ch.to_uppercase()),
    }
}

/// CSS Text 3 §2.1 `full-width`: the inverse of the `<wide>` and the
/// `<narrow>` compatibility decompositions (UAX #11); a character without
/// a full-width form is left as is.
fn full_width(ch: char) -> char {
    let code = u32::from(ch);
    let mapped = match code {
        0x20 => 0x3000,
        0x21..=0x7E => code + 0xFEE0,
        0xA2 => 0xFFE0,
        0xA3 => 0xFFE1,
        0xAC => 0xFFE2,
        0xAF => 0xFFE3,
        0xA6 => 0xFFE4,
        0xA5 => 0xFFE5,
        0x20A9 => 0xFFE6,
        0x2985 => 0xFF5F,
        0x2986 => 0xFF60,
        0xFF61..=0xFF9F => u32::from(HALFWIDTH_KATAKANA[(code - 0xFF61) as usize]),
        0xFFA0 => 0x3164,
        0xFFA1..=0xFFBE => code - 0xFFA1 + 0x3131,
        0xFFC2..=0xFFC7 => code - 0xFFC2 + 0x314F,
        0xFFCA..=0xFFCF => code - 0xFFCA + 0x3155,
        0xFFD2..=0xFFD7 => code - 0xFFD2 + 0x315B,
        0xFFDA..=0xFFDC => code - 0xFFDA + 0x3161,
        0xFFE8 => 0x2502,
        0xFFE9 => 0x2190,
        0xFFEA => 0x2191,
        0xFFEB => 0x2192,
        0xFFEC => 0x2193,
        0xFFED => 0x25A0,
        0xFFEE => 0x25CB,
        _ => code,
    };
    char::from_u32(mapped).unwrap_or(ch)
}

/// The `<narrow>` decompositions of U+FF61–U+FF9F (halfwidth CJK
/// punctuation and katakana).
const HALFWIDTH_KATAKANA: [u16; 63] = [
    0x3002, 0x300C, 0x300D, 0x3001, 0x30FB, 0x30F2, 0x30A1, 0x30A3, 0x30A5, 0x30A7, 0x30A9, 0x30E3,
    0x30E5, 0x30E7, 0x30C3, 0x30FC, 0x30A2, 0x30A4, 0x30A6, 0x30A8, 0x30AA, 0x30AB, 0x30AD, 0x30AF,
    0x30B1, 0x30B3, 0x30B5, 0x30B7, 0x30B9, 0x30BB, 0x30BD, 0x30BF, 0x30C1, 0x30C4, 0x30C6, 0x30C8,
    0x30CA, 0x30CB, 0x30CC, 0x30CD, 0x30CE, 0x30CF, 0x30D2, 0x30D5, 0x30D8, 0x30DB, 0x30DE, 0x30DF,
    0x30E0, 0x30E1, 0x30E2, 0x30E4, 0x30E6, 0x30E8, 0x30E9, 0x30EA, 0x30EB, 0x30EC, 0x30ED, 0x30EF,
    0x30F3, 0x3099, 0x309A,
];

/// CSS Text 3 Appendix G: small kana and their full-size forms.
const SMALL_KANA: &[(u32, u32)] = &[
    (0x3041, 0x3042),
    (0x3043, 0x3044),
    (0x3045, 0x3046),
    (0x3047, 0x3048),
    (0x3049, 0x304A),
    (0x3063, 0x3064),
    (0x3083, 0x3084),
    (0x3085, 0x3086),
    (0x3087, 0x3088),
    (0x308E, 0x308F),
    (0x3095, 0x304B),
    (0x3096, 0x3051),
    (0x30A1, 0x30A2),
    (0x30A3, 0x30A4),
    (0x30A5, 0x30A6),
    (0x30A7, 0x30A8),
    (0x30A9, 0x30AA),
    (0x30C3, 0x30C4),
    (0x30E3, 0x30E4),
    (0x30E5, 0x30E6),
    (0x30E7, 0x30E8),
    (0x30EE, 0x30EF),
    (0x30F5, 0x30AB),
    (0x30F6, 0x30B1),
    (0x31F0, 0x30AF),
    (0x31F1, 0x30B7),
    (0x31F2, 0x30B9),
    (0x31F3, 0x30C8),
    (0x31F4, 0x30CC),
    (0x31F5, 0x30CF),
    (0x31F6, 0x30D2),
    (0x31F7, 0x30D5),
    (0x31F8, 0x30D8),
    (0x31F9, 0x30DB),
    (0x31FA, 0x30E0),
    (0x31FB, 0x30E9),
    (0x31FC, 0x30EA),
    (0x31FD, 0x30EB),
    (0x31FE, 0x30EC),
    (0x31FF, 0x30ED),
    (0xFF67, 0xFF71),
    (0xFF68, 0xFF72),
    (0xFF69, 0xFF73),
    (0xFF6A, 0xFF74),
    (0xFF6B, 0xFF75),
    (0xFF6C, 0xFF94),
    (0xFF6D, 0xFF95),
    (0xFF6E, 0xFF96),
    (0xFF6F, 0xFF82),
    (0x1B132, 0x3053),
    (0x1B150, 0x3090),
    (0x1B151, 0x3091),
    (0x1B152, 0x3092),
    (0x1B155, 0x30B3),
    (0x1B164, 0x30F0),
    (0x1B165, 0x30F1),
    (0x1B166, 0x30F2),
    (0x1B167, 0x30F3),
];

/// CSS Text 3 §2.1 `full-size-kana` (Appendix G).
fn full_size_kana(ch: char) -> char {
    let code = u32::from(ch);
    SMALL_KANA
        .iter()
        .find(|(small, _)| *small == code)
        .and_then(|(_, full)| char::from_u32(*full))
        .unwrap_or(ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(value: &str, text: &str) -> String {
        TextTransform::parse(value).unwrap().apply(text)
    }

    #[test]
    fn case_transforms_use_the_full_unicode_mappings() {
        assert_eq!(apply("uppercase", "straße ǆ"), "STRASSE Ǆ");
        assert_eq!(apply("lowercase", "ΟΔΟΣ ÀB"), "οδος àb");
        assert_eq!(apply("none", "aB"), "aB");
        assert!(TextTransform::parse("uppercase lowercase").is_none());
        assert!(TextTransform::parse("full-width full-width").is_none());
        assert!(TextTransform::parse("none").unwrap().is_none());
    }

    #[test]
    fn capitalize_titlecases_the_first_letter_unit_of_each_word() {
        assert_eq!(
            apply("capitalize", "hello-world don't 3d ǆem (ok) ßa"),
            "Hello-World Don't 3d ǅem (Ok) Ssa"
        );
        // A word that continues from the previous text keeps its letters.
        let transform = TextTransform::parse("capitalize").unwrap();
        assert_eq!(transform.map('o', "hel", ""), "o");
        assert_eq!(transform.map('o', "hel ", ""), "O");
        assert_eq!(apply("lowercase", "ΣΑΣ Σ"), "σας σ");
        assert_eq!(apply("capitalize", "ᾳ"), "ᾼ");
    }

    #[test]
    fn full_width_and_full_size_kana_map_their_forms() {
        assert_eq!(apply("full-width", "A1 ¥ｱﾞ"), "Ａ１\u{3000}￥ア\u{3099}");
        assert_eq!(apply("full-size-kana", "ぁァｧ\u{1B132}"), "あアｱこ");
        assert_eq!(apply("uppercase full-width", "a"), "Ａ");
    }
}
