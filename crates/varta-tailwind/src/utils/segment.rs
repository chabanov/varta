//! Split a string on a top-level separator (port of `utils/segment.ts`).

/// The deepest bracket nesting tracked; deeper brackets are not balanced
/// (Tailwind tracks 256 levels in a fixed `Uint8Array`).
pub const MAX_SEGMENT_DEPTH: usize = 256;

/// Split `input` on `separator` outside of `()`, `[]`, `{}` and strings,
/// honouring backslash escapes.
#[must_use]
pub fn segment(input: &str, separator: char) -> Vec<String> {
    let mut stack: Vec<char> = Vec::new();
    let mut parts = Vec::new();
    let mut last = 0;
    let mut chars = input.char_indices().peekable();
    while let Some((index, c)) = chars.next() {
        if stack.is_empty() && c == separator {
            parts.push(input[last..index].to_owned());
            last = index + c.len_utf8();
            continue;
        }
        match c {
            '\\' => {
                chars.next();
            }
            '\'' | '"' => {
                while let Some((_, next)) = chars.next() {
                    if next == '\\' {
                        chars.next();
                        continue;
                    }
                    if next == c {
                        break;
                    }
                }
            }
            '(' | '[' | '{' => {
                if stack.len() < MAX_SEGMENT_DEPTH {
                    stack.push(match c {
                        '(' => ')',
                        '[' => ']',
                        _ => '}',
                    });
                }
            }
            ')' | ']' | '}' if stack.last() == Some(&c) => {
                stack.pop();
            }
            _ => {}
        }
    }
    parts.push(input[last..].to_owned());
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_at_top_level_only() {
        assert_eq!(
            segment("var(--a, 0 0 1px rgb(0, 0, 0)), 0 0 1px rgb(0, 0, 0)", ','),
            vec!["var(--a, 0 0 1px rgb(0, 0, 0))", " 0 0 1px rgb(0, 0, 0)"]
        );
        assert_eq!(segment("a:b:[c:d]", ':'), vec!["a", "b", "[c:d]"]);
        assert_eq!(segment("a\\:b:c", ':'), vec!["a\\:b", "c"]);
        assert_eq!(segment("'a:b':c", ':'), vec!["'a:b'", "c"]);
        assert_eq!(segment("", ','), vec![""]);
    }
}
