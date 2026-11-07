//! Pattern template literal types (`` `on${string}` ``, `` `${number}px` ``)
//! and the intrinsic string types over `string` (`Uppercase<string>`), as
//! values of string literal sets (v38 `type_level`, step 3). A pattern has
//! infinitely many strings, so it is a member of a set
//! (`LiteralValue::Pattern`) beside the set's finite strings; values of a
//! pattern type are strings, physically and in every string operation, and
//! only assignability changes: a string literal is assignable to a pattern
//! when it matches it (TypeScript's `isTypeMatchedByTemplateLiteralType`),
//! a pattern to another when every string it describes matches the other,
//! and `string` to none.
use super::*;
use crate::lower::literals::LiteralValue;
use template_types::Intrinsic;

/// One piece of a pattern.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Piece {
    /// Literal text in UTF-16 code units.
    Text(Vec<u16>),
    /// A placeholder of infinitely many texts.
    Hole(Hole),
}

/// The placeholder kinds of a pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Hole {
    /// `${string}`: any text.
    String,
    /// `${number}`: a text whose ECMAScript `ToNumber` is finite.
    Number,
    /// `${bigint}`: the text of a bigint literal.
    BigInt,
    /// `${Uppercase<string>}` and the other intrinsic types over `string`:
    /// a text the mapping leaves unchanged.
    Mapped(Intrinsic),
}

/// A pattern: its pieces, adjacent texts merged, with at least one hole.
pub(crate) type Pattern = Vec<Piece>;

/// The canonical form of `pieces`: adjacent texts merged, empty texts
/// dropped.
pub(super) fn canonical(pieces: Vec<Piece>) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    for piece in pieces {
        match (out.last_mut(), piece) {
            (_, Piece::Text(text)) if text.is_empty() => {}
            (Some(Piece::Text(last)), Piece::Text(text)) => last.extend(text),
            (_, piece) => out.push(piece),
        }
    }
    out
}

/// The text of a pattern as TypeScript writes it.
pub(in crate::lower) fn describe(pattern: &[Piece]) -> String {
    if let [Piece::Hole(Hole::Mapped(mapping))] = pattern {
        return format!("{}<string>", mapping.name());
    }
    let mut text = String::from("`");
    for piece in pattern {
        match piece {
            Piece::Text(units) => text.push_str(&String::from_utf16_lossy(units)),
            Piece::Hole(Hole::String) => text.push_str("${string}"),
            Piece::Hole(Hole::Number) => text.push_str("${number}"),
            Piece::Hole(Hole::BigInt) => text.push_str("${bigint}"),
            Piece::Hole(Hole::Mapped(mapping)) => {
                text.push_str(&format!("${{{}<string>}}", mapping.name()));
            }
        }
    }
    text.push('`');
    text
}

/// Whether the text `units` is a value of `pattern`: TypeScript's
/// inference from a literal to a template, where a placeholder followed by
/// text takes the text up to that text's first occurrence, one followed by
/// another placeholder one code unit and the last the rest, and each taken
/// text must be a value of its placeholder.
pub(in crate::lower) fn text_matches(units: &[u16], pattern: &[Piece]) -> bool {
    let mut texts: Vec<&[u16]> = vec![&[]];
    let mut holes = Vec::new();
    for piece in pattern {
        match piece {
            Piece::Text(text) => {
                let last = texts.len() - 1;
                if texts[last].is_empty() {
                    texts[last] = text;
                } else {
                    // Canonical patterns never hold adjacent texts.
                    return false;
                }
            }
            Piece::Hole(hole) => {
                holes.push(*hole);
                texts.push(&[]);
            }
        }
    }
    let (first, last) = (texts[0], texts[texts.len() - 1]);
    if !units.starts_with(first) {
        return false;
    }
    if holes.is_empty() {
        return units == first;
    }
    if units.len() < first.len() + last.len() || !units.ends_with(last) {
        return false;
    }
    let end = units.len() - last.len();
    let mut position = first.len();
    for (index, hole) in holes.iter().enumerate() {
        let following = texts[index + 1];
        let piece_end = if index + 1 == holes.len() {
            end
        } else if following.is_empty() {
            if position >= end {
                return false;
            }
            position + 1
        } else {
            match units[position..end]
                .windows(following.len())
                .position(|window| window == following)
            {
                Some(offset) => position + offset,
                None => return false,
            }
        };
        if !hole_accepts(*hole, &units[position..piece_end]) {
            return false;
        }
        position = piece_end
            + if index + 1 == holes.len() {
                0
            } else {
                following.len()
            };
    }
    true
}

/// Whether `text` is a value of the placeholder `hole` (TypeScript's
/// `isValidTypeForTemplateLiteralPlaceholder`).
pub(super) fn hole_accepts(hole: Hole, text: &[u16]) -> bool {
    match hole {
        Hole::String => true,
        Hole::Number => number_string(text),
        Hole::BigInt => bigint_string(text),
        Hole::Mapped(mapping) => mapping.apply(text) == text,
    }
}

/// TypeScript's `isValidNumberString(s, false)`: `s` is not empty and
/// ECMAScript `ToNumber(s)` (StringToNumber) is finite.
fn number_string(text: &[u16]) -> bool {
    if text.is_empty() {
        return false;
    }
    let Ok(text) = String::from_utf16(text) else {
        return false;
    };
    // StrWhiteSpaceChar: WhiteSpace and LineTerminator.
    let trimmed = text.trim_matches(|c: char| {
        matches!(
            c,
            '\u{9}'
                | '\u{b}'
                | '\u{c}'
                | ' '
                | '\u{a0}'
                | '\u{feff}'
                | '\n'
                | '\r'
                | '\u{2028}'
                | '\u{2029}'
        ) || c.is_whitespace() && c != '\u{85}'
    });
    if trimmed.is_empty() {
        return true;
    }
    let digits = |s: &str, radix: u32| !s.is_empty() && s.chars().all(|c| c.is_digit(radix));
    let lower = trimmed.get(..2).map(str::to_ascii_lowercase);
    match lower.as_deref() {
        Some("0x") => return digits(&trimmed[2..], 16),
        Some("0o") => return digits(&trimmed[2..], 8),
        Some("0b") => return digits(&trimmed[2..], 2),
        _ => {}
    }
    let unsigned = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
    // StrUnsignedDecimalLiteral without `Infinity` (not finite).
    let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
        Some(index) => (&unsigned[..index], Some(&unsigned[index + 1..])),
        None => (unsigned, None),
    };
    let (whole, fraction) = match mantissa.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (mantissa, None),
    };
    let decimal = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    let mantissa_ok = decimal(whole)
        && fraction.is_none_or(decimal)
        && !(whole.is_empty() && fraction.is_none_or(str::is_empty));
    let exponent_ok = exponent.is_none_or(|e| digits(e.strip_prefix(['+', '-']).unwrap_or(e), 10));
    if !(mantissa_ok && exponent_ok) {
        return false;
    }
    trimmed.parse::<f64>().is_ok_and(f64::is_finite)
}

/// TypeScript's `isValidBigIntString(s, false)`: an optional `-` and a
/// bigint literal without its `n` (decimal without leading zeros, or
/// `0x`/`0o`/`0b` digits; no separators or whitespace).
fn bigint_string(text: &[u16]) -> bool {
    let Ok(text) = String::from_utf16(text) else {
        return false;
    };
    let body = text.strip_prefix('-').unwrap_or(&text);
    let digits = |s: &str, radix: u32| !s.is_empty() && s.chars().all(|c| c.is_digit(radix));
    match body.get(..2) {
        Some("0x" | "0X") => digits(&body[2..], 16),
        Some("0o" | "0O") => digits(&body[2..], 8),
        Some("0b" | "0B") => digits(&body[2..], 2),
        _ => body == "0" || digits(body, 10) && !body.starts_with('0'),
    }
}

/// One symbol of a pattern being matched against another: a code unit of
/// its text or one of its placeholders.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Symbol {
    Unit(u16),
    Hole(Hole),
}

/// Whether every string of the pattern `inner` is a string of `outer`. A
/// placeholder of `outer` takes a run of `inner`'s symbols: `${string}`
/// any run, another placeholder the same placeholder or a text it accepts;
/// a text of `outer` only the same text. The match is exact over every
/// split, so it only admits patterns whose strings all match.
pub(in crate::lower) fn covers(outer: &[Piece], inner: &[Piece]) -> bool {
    if outer == inner {
        return true;
    }
    let symbols: Vec<Symbol> = inner
        .iter()
        .flat_map(|piece| match piece {
            Piece::Text(units) => units.iter().map(|unit| Symbol::Unit(*unit)).collect(),
            Piece::Hole(hole) => vec![Symbol::Hole(*hole)],
        })
        .collect();
    let mut memo = BTreeMap::new();
    covers_from(outer, &symbols, 0, 0, &mut memo)
}

fn covers_from(
    outer: &[Piece],
    symbols: &[Symbol],
    piece: usize,
    at: usize,
    memo: &mut BTreeMap<(usize, usize), bool>,
) -> bool {
    if let Some(known) = memo.get(&(piece, at)) {
        return *known;
    }
    let result = match outer.get(piece) {
        None => at == symbols.len(),
        Some(Piece::Text(units)) => {
            let end = at + units.len();
            end <= symbols.len()
                && units
                    .iter()
                    .zip(&symbols[at..end])
                    .all(|(unit, symbol)| *symbol == Symbol::Unit(*unit))
                && covers_from(outer, symbols, piece + 1, end, memo)
        }
        Some(Piece::Hole(hole)) => (at..=symbols.len()).any(|end| {
            run_accepted(*hole, &symbols[at..end])
                && covers_from(outer, symbols, piece + 1, end, memo)
        }),
    };
    memo.insert((piece, at), result);
    result
}

/// Whether the placeholder `hole` accepts every string the run of symbols
/// describes.
fn run_accepted(hole: Hole, run: &[Symbol]) -> bool {
    if hole == Hole::String {
        return true;
    }
    if let [Symbol::Hole(inner)] = run {
        return *inner == hole;
    }
    let units: Option<Vec<u16>> = run
        .iter()
        .map(|symbol| match symbol {
            Symbol::Unit(unit) => Some(*unit),
            Symbol::Hole(_) => None,
        })
        .collect();
    units.is_some_and(|units| hole_accepts(hole, &units))
}

/// Whether the literal value `value` is a value of the literal set `set`:
/// a member, a string matched by a pattern member, or a pattern covered by
/// one.
pub(in crate::lower) fn value_in(
    value: &LiteralValue,
    set: &std::collections::BTreeSet<LiteralValue>,
    types: &ir::ValueTypes,
) -> bool {
    if set.contains(value) {
        return true;
    }
    let patterns = || {
        set.iter().filter_map(|member| match member {
            LiteralValue::Pattern(id) => Some(types.literals.pattern(*id)),
            _ => None,
        })
    };
    match value {
        LiteralValue::String(_) => {
            let Some(units) = literals::string_units(value, types.profile) else {
                return false;
            };
            patterns().any(|pattern| text_matches(&units, pattern))
        }
        LiteralValue::Pattern(id) => {
            let inner = types.literals.pattern(*id);
            patterns().any(|pattern| covers(pattern, inner))
        }
        LiteralValue::Number(_) | LiteralValue::Boolean(_) => false,
    }
}
