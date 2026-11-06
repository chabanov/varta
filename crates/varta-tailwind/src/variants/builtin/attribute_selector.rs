//! Attribute selector check: a port of Tailwind CSS v4.3.5
//! `attribute-selector-parser.ts` (MIT, Copyright (c) Tailwind Labs, Inc.;
//! see `LICENSE-TAILWIND`).
//!
//! `aria-*` and `data-*` only ask whether their selector parses, so the
//! parsed parts (attribute, operator, quote, value, sensitivity) are not
//! returned. The scan works on UTF-16 code units, as Tailwind's does.

const TAB: u16 = 9;
const LINE_BREAK: u16 = 10;
const CARRIAGE_RETURN: u16 = 13;
const SPACE: u16 = 32;
const DOUBLE_QUOTE: u16 = 34;
const DOLLAR: u16 = 36;
const SINGLE_QUOTE: u16 = 39;
const ASTERISK: u16 = 42;
const EQUALS: u16 = 61;
const UPPER_I: u16 = 73;
const UPPER_S: u16 = 83;
const BACKSLASH: u16 = 92;
const CARET: u16 = 94;
const LOWER_I: u16 = 105;
const LOWER_S: u16 = 115;
const PIPE: u16 = 124;
const TILDE: u16 = 126;
const LOWER_A: u16 = 97;
const LOWER_Z: u16 = 122;
const UPPER_A: u16 = 65;
const UPPER_Z: u16 = 90;
const ZERO: u16 = 48;
const NINE: u16 = 57;
const DASH: u16 = 45;
const UNDERSCORE: u16 = 95;
const NON_ASCII: u16 = 128;
const OPEN_BRACKET: u16 = 91;
const CLOSE_BRACKET: u16 = 93;

/// The operators written as one character before `=` (`~=`, `|=`, `^=`,
/// `$=`, `*=`).
const OPERATOR_PREFIXES: &[u16] = &[TILDE, PIPE, CARET, DOLLAR, ASTERISK];
/// The case-sensitivity flags (`i`, `s`, either case).
const SENSITIVITY_FLAGS: &[u16] = &[LOWER_I, UPPER_I, LOWER_S, UPPER_S];

/// `Some(())` when `input` is one attribute selector such as
/// `[data-foo="bar" i]` (port of `parse`, which returns `null` otherwise).
pub(super) fn parse(input: &str) -> Option<()> {
    let units: Vec<u16> = input.encode_utf16().collect();
    // JavaScript's `charCodeAt` past the end is NaN, which matches nothing.
    let at = |i: usize| units.get(i).copied();
    let is_whitespace_at = |i: usize| at(i).is_some_and(is_ascii_whitespace);

    // Must start with `[` and end with `]`.
    if units.first() != Some(&OPEN_BRACKET) || units.last() != Some(&CLOSE_BRACKET) {
        return None;
    }

    let mut i = 1;
    let end = units.len() - 1;

    // Whitespace before the name: `[   data-foo]`.
    while is_whitespace_at(i) {
        i += 1;
    }

    // Attribute name: `[data-foo]`.
    let start = i;
    while i < end {
        let Some(current) = at(i) else { break };
        // Skip an escaped character.
        if current == BACKSLASH {
            i += 2;
            continue;
        }
        if is_name_unit(current) {
            i += 1;
            continue;
        }
        break;
    }
    // At least one character in the name.
    if start == i {
        return None;
    }

    // Whitespace after the name: `[data-foo   =value]`.
    while is_whitespace_at(i) {
        i += 1;
    }

    // The end: `[data-foo]`.
    if i == end {
        return Some(());
    }

    // Operator: `[data-foo*=value]`.
    let current = at(i)?;
    if current == EQUALS {
        i += 1;
    } else if OPERATOR_PREFIXES.contains(&current) && at(i + 1) == Some(EQUALS) {
        i += 2;
    } else {
        return None;
    }

    // Whitespace after the operator: `[data-foo*=   value]`.
    while is_whitespace_at(i) {
        i += 1;
    }

    // An operator without a value: `[data-foo*=]`.
    if i == end {
        return None;
    }

    // Value: quoted (`"value"`, up to the last matching quote) or unquoted
    // (up to whitespace or the end).
    let current = at(i)?;
    if current == SINGLE_QUOTE || current == DOUBLE_QUOTE {
        i += 1;
        let mut j = i;
        while j < end {
            let unit = at(j)?;
            if unit == current {
                i = j + 1;
            } else if unit == BACKSLASH {
                j += 1;
            }
            j += 1;
        }
    } else {
        while i < end && !is_whitespace_at(i) {
            i += 1;
        }
    }

    // Whitespace after the value: `[data-foo*=value   ]`.
    while is_whitespace_at(i) {
        i += 1;
    }

    // The end: `[data-foo=value]`.
    if i == end {
        return Some(());
    }

    // Sensitivity: `[data-foo=value i]`.
    if !SENSITIVITY_FLAGS.contains(&at(i)?) {
        return None;
    }
    i += 1;

    // Whitespace after the flag: `[data-foo=value i   ]`.
    while is_whitespace_at(i) {
        i += 1;
    }

    // Nothing may follow the flag: `[data-foo=value iX]`.
    (i == end).then_some(())
}

/// A code unit of a CSS identifier: ASCII letters, digits, `-`, `_` and
/// any non-ASCII code unit (`[data-état]`).
fn is_name_unit(unit: u16) -> bool {
    (UPPER_A..=UPPER_Z).contains(&unit)
        || (LOWER_A..=LOWER_Z).contains(&unit)
        || (ZERO..=NINE).contains(&unit)
        || unit == DASH
        || unit == UNDERSCORE
        || unit >= NON_ASCII
}

fn is_ascii_whitespace(unit: u16) -> bool {
    matches!(unit, SPACE | TAB | LINE_BREAK | CARRIAGE_RETURN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_attribute_selectors() {
        for input in [
            "[data-foo]",
            "[ data-foo ]",
            "[data-foo=bar]",
            "[data-foo*=bar]",
            "[data-foo = 'bar baz']",
            "[data-foo=\"bar\" i]",
            "[data-foo=bar S ]",
            "[data-état]",
            "[data-foo\\:bar]",
        ] {
            assert_eq!(parse(input), Some(()), "{input}");
        }
    }

    #[test]
    fn rejects_malformed_selectors() {
        for input in [
            "data-foo",
            "[]",
            "[data-foo=]",
            "[data-foo!=bar]",
            "[data-foo bar]",
            "[data-foo=bar x]",
            "[data-foo=bar iX]",
            "[data-foo=\"bar\"baz]",
        ] {
            assert_eq!(parse(input), None, "{input}");
        }
    }
}
