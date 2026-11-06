//! Port of Tailwind CSS v4.3.5 `utils/is-valid-arbitrary.ts` (Tailwind Labs,
//! MIT): reject arbitrary values with unbalanced brackets or a top-level `;`.

/// The deepest `(`/`[` nesting tracked. Tailwind keeps the closing brackets
/// in a fixed 256-entry `Uint8Array`; a deeper value can never close
/// correctly there, so it is invalid here.
pub const MAX_ARBITRARY_DEPTH: usize = 256;

/// Whether `input` might be a valid arbitrary value: `(` and `[` are balanced
/// (a stray `}` is tolerated, `{` opens nothing), strings and escapes are
/// skipped, and there is no top-level `;`.
#[must_use]
pub fn is_valid_arbitrary(input: &str) -> bool {
    let mut closing: Vec<char> = Vec::new();
    let mut chars = input.chars();

    while let Some(c) = chars.next() {
        match c {
            // The next character is escaped, so we skip it.
            '\\' => {
                chars.next();
            }
            // Strings are taken as-is until their closing quote.
            '\'' | '"' => {
                while let Some(next) = chars.next() {
                    if next == '\\' {
                        chars.next();
                        continue;
                    }
                    if next == c {
                        break;
                    }
                }
            }
            '(' | '[' => {
                if closing.len() == MAX_ARBITRARY_DEPTH {
                    return false;
                }
                closing.push(if c == '(' { ')' } else { ']' });
            }
            // `{` intentionally does not open a level: a candidate like
            // `[&{color:red}]:flex` must not be valid.
            ')' | ']' | '}' => {
                if closing.is_empty() {
                    return false;
                }
                if closing.last() == Some(&c) {
                    closing.pop();
                } else if c != '}' {
                    return false;
                }
            }
            ';' if closing.is_empty() => return false,
            _ => {}
        }
    }

    closing.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_balanced_values() {
        assert!(is_valid_arbitrary("calc(100%-1rem)"));
        assert!(is_valid_arbitrary("[foo]"));
        assert!(is_valid_arbitrary("url('a;b)')"));
        assert!(is_valid_arbitrary("a\\)"));
        assert!(is_valid_arbitrary("var(--a;b)"));
        assert!(is_valid_arbitrary("(})"));
        assert!(is_valid_arbitrary(""));
    }

    #[test]
    fn rejects_unbalanced_values() {
        assert!(!is_valid_arbitrary("calc(1px"));
        assert!(!is_valid_arbitrary("1px)"));
        assert!(!is_valid_arbitrary("(]"));
        assert!(!is_valid_arbitrary("}"));
        assert!(!is_valid_arbitrary("&{color:red}"));
        assert!(!is_valid_arbitrary("a;b"));
    }

    #[test]
    fn bounds_nesting() {
        let deepest = format!(
            "{}{}",
            "(".repeat(MAX_ARBITRARY_DEPTH),
            ")".repeat(MAX_ARBITRARY_DEPTH)
        );
        assert!(is_valid_arbitrary(&deepest));
        let too_deep = format!(
            "{}{}",
            "(".repeat(MAX_ARBITRARY_DEPTH + 1),
            ")".repeat(MAX_ARBITRARY_DEPTH + 1)
        );
        assert!(!is_valid_arbitrary(&too_deep));
    }
}
