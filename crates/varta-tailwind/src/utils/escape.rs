//! `CSS.escape` (CSSOM §2.1 "serialize an identifier") and CSS Syntax 3
//! §4.3.7 escape consumption, as ported in Tailwind's `utils/escape.ts`.
use std::fmt::Write as _;

/// Serialize `value` as a CSS identifier (CSSOM `CSS.escape`).
#[must_use]
pub fn escape(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let first = value.chars().next();
    if value.chars().count() == 1 && first == Some('-') {
        return "\\-".to_owned();
    }
    for (index, c) in value.chars().enumerate() {
        let code = c as u32;
        if code == 0 {
            result.push('\u{FFFD}');
        } else if (0x01..=0x1f).contains(&code)
            || code == 0x7f
            || (index == 0 && c.is_ascii_digit())
            || (index == 1 && c.is_ascii_digit() && first == Some('-'))
        {
            // Infallible: writing to a String.
            let _ = write!(result, "\\{code:x} ");
        } else if code >= 0x80 || c == '-' || c == '_' || c.is_ascii_alphanumeric() {
            result.push(c);
        } else {
            result.push('\\');
            result.push(c);
        }
    }
    result
}

/// Replace CSS escapes with the characters they stand for.
#[must_use]
pub fn unescape(escaped: &str) -> String {
    let mut out = String::with_capacity(escaped.len());
    let chars: Vec<char> = escaped.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '\\' || i + 1 >= chars.len() {
            // A trailing lone backslash is kept (the JS regex does not match it).
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let mut j = i + 1;
        let mut hex = String::new();
        while j < chars.len() && hex.len() < 6 && chars[j].is_ascii_hexdigit() {
            hex.push(chars[j]);
            j += 1;
        }
        if hex.is_empty() {
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if j < chars.len() && matches!(chars[j], '\t' | '\n' | '\u{c}' | '\r' | ' ') {
            j += 1;
        }
        let code = u32::from_str_radix(&hex, 16).unwrap_or(0);
        let c = if code == 0 || code > 0x10ffff || (0xd800..=0xdfff).contains(&code) {
            '\u{FFFD}'
        } else {
            char::from_u32(code).unwrap_or('\u{FFFD}')
        };
        out.push(c);
        i = j;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_like_css_escape() {
        assert_eq!(escape("hover:bg-red-500"), "hover\\:bg-red-500");
        assert_eq!(escape("w-1/2"), "w-1\\/2");
        assert_eq!(escape("-"), "\\-");
        assert_eq!(escape("2xl"), "\\32 xl");
        assert_eq!(escape("-2"), "-\\32 ");
        assert_eq!(escape("[&_p]"), "\\[\\&_p\\]");
        assert_eq!(escape("é"), "é");
    }

    #[test]
    fn unescapes() {
        assert_eq!(unescape("hover\\:bg"), "hover:bg");
        assert_eq!(unescape("\\32 xl"), "2xl");
        assert_eq!(unescape("--leading-1\\.5"), "--leading-1.5");
        assert_eq!(unescape("\\0"), "\u{FFFD}");
    }
}
