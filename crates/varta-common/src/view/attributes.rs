//! Checked attribute value grammars of the draft v0 attribute table, shared by
//! the TSX compiler (static values) and every applier
//! ([`super::AttributeValue`]): an `href` admits the internal route grammar
//! here; whether an external link may appear is the embedder's decision.
use super::{AttributeValue, BUTTON_TYPES, INPUT_TYPES, Tag};

/// Longest `id`/`for` ident.
pub const MAX_IDENT_CHARS: usize = 64;

/// `[A-Za-z][A-Za-z0-9_-]*`, at most [`MAX_IDENT_CHARS`].
pub fn ident(text: &str) -> bool {
    text.len() <= MAX_IDENT_CHARS
        && text.starts_with(|c: char| c.is_ascii_alphabetic())
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn digits(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_digit())
}

/// `0` or a positive integer without sign or leading zeros.
fn count(text: &str) -> bool {
    text == "0" || positive_count(text)
}
fn positive_count(text: &str) -> bool {
    digits(text) && !text.starts_with('0')
}
/// A finite nonnegative decimal number (`digits[.digits]`).
fn number(text: &str) -> bool {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, "0"));
    digits(whole) && digits(fraction)
}

/// RFC 3986 `pchar` minus percent-encoding, which is checked separately.
fn url_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "-._~!$&'()*+,;=:@/".contains(c)
}
/// Characters with valid `%HH` escapes, plus `?` where `query` admits it.
fn url_part(text: &str, extra: &str) -> bool {
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == '%' {
            if !(bytes.get(i + 1).is_some_and(char::is_ascii_hexdigit)
                && bytes.get(i + 2).is_some_and(char::is_ascii_hexdigit))
            {
                return false;
            }
            i += 3;
            continue;
        }
        if !(url_char(c) || extra.contains(c)) {
            return false;
        }
        i += 1;
    }
    true
}

/// An internal route: `#fragment`, or `/path[?query][#fragment]` whose path
/// does not start with `//`.
pub fn internal_route(text: &str) -> bool {
    if let Some(fragment) = text.strip_prefix('#') {
        return url_part(fragment, "?");
    }
    let Some(rest) = text.strip_prefix('/') else {
        return false;
    };
    if rest.starts_with('/') {
        return false;
    }
    let (before_fragment, fragment) = rest.split_once('#').unwrap_or((rest, ""));
    let (path, query) = before_fragment
        .split_once('?')
        .unwrap_or((before_fragment, ""));
    url_part(path, "") && url_part(query, "?") && url_part(fragment, "?")
}

/// Whether `text` is shaped like an external link (`https://` or `http://`);
/// whether it may appear is the package request's decision.
pub fn external(text: &str) -> bool {
    text.starts_with("https://") || text.starts_with("http://")
}

/// A bundled resource path: `[A-Za-z0-9._-]` segments separated by `/`, no
/// `..` and no leading `/`.
fn resource(text: &str) -> bool {
    !text.is_empty()
        && text.split('/').all(|segment| {
            !segment.is_empty()
                && segment != ".."
                && segment != "."
                && segment
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
        })
}

/// Whether `value` follows `grammar` on an element `tag`. `Href` admits the
/// internal route grammar here; external links are checked by the caller.
pub fn valid(grammar: AttributeValue, tag: Tag, value: &str) -> bool {
    use AttributeValue as A;
    match grammar {
        A::Ident => ident(value),
        A::IdentList => {
            !value.is_empty()
                && value.split(' ').all(ident)
                && !value.starts_with(' ')
                && !value.ends_with(' ')
        }
        A::Text => true,
        A::Boolean => value.is_empty(),
        A::Count => count(value),
        A::Number => number(value),
        A::PositiveCount => positive_count(value),
        A::Keywords(words) => words.contains(&value),
        A::ElementType => match tag {
            Tag::Input => INPUT_TYPES.contains(&value),
            Tag::Button => BUTTON_TYPES.contains(&value),
            _ => false,
        },
        A::Href => internal_route(value),
        A::Resource => resource(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grammars_accept_and_reject_their_samples() {
        use AttributeValue as A;
        let tag = Tag::Div;
        assert!(valid(A::Ident, tag, "main-1"));
        assert!(!valid(A::Ident, tag, "1main"));
        assert!(!valid(A::Ident, tag, &"a".repeat(MAX_IDENT_CHARS + 1)));
        assert!(valid(A::IdentList, tag, "a b"));
        assert!(!valid(A::IdentList, tag, "a  b"));
        assert!(valid(A::Boolean, tag, ""));
        assert!(!valid(A::Boolean, tag, "false"));
        assert!(valid(A::Count, tag, "0") && valid(A::Count, tag, "12"));
        assert!(!valid(A::Count, tag, "012") && !valid(A::Count, tag, "-1"));
        assert!(!valid(A::PositiveCount, tag, "0"));
        assert!(valid(A::Number, tag, "2.5") && !valid(A::Number, tag, "1e3"));
        assert!(valid(A::ElementType, Tag::Input, "checkbox"));
        assert!(!valid(A::ElementType, Tag::Button, "checkbox"));
        for route in ["#top", "/a/b?x=1#y", "/", "/%20"] {
            assert!(valid(A::Href, Tag::A, route), "{route}");
        }
        for route in [
            "//evil.test",
            "javascript:alert(1)",
            "/a b",
            "/a\\b",
            "/%2",
            "https://a.test",
        ] {
            assert!(!valid(A::Href, Tag::A, route), "{route}");
        }
        assert!(valid(A::Resource, Tag::Img, "icons/logo.png"));
        for path in ["/logo.png", "../x.png", "a//b.png", "a/./b.png"] {
            assert!(!valid(A::Resource, Tag::Img, path), "{path}");
        }
    }
}
