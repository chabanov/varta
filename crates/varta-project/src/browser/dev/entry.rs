//! Public navigation input, never a credential or a capability grant.
pub(super) fn valid_prefix(prefix: &str) -> bool {
    prefix
        .strip_prefix('/')
        .is_some_and(|segment| segment != "__varta" && valid_segment(segment, 64))
}

fn valid_segment(segment: &str, max: usize) -> bool {
    !segment.is_empty()
        && segment.len() <= max
        && segment
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub(super) fn matches(prefix: &str, path: &str) -> bool {
    path.strip_prefix(prefix)
        .and_then(|suffix| suffix.strip_prefix('/'))
        .is_some_and(|id| valid_segment(id, 128))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn entry_paths_are_bounded_literal_identifiers() {
        assert!(valid_prefix("/apps"));
        for prefix in ["", "/", "apps", "/__varta", "/a/b", "/a%20", "/index.html"] {
            assert!(!valid_prefix(prefix), "{prefix}");
        }
        assert!(!valid_prefix(&format!("/{}", "a".repeat(65))));
        assert!(matches("/apps", "/apps/a-012_3"));
        for path in [
            "/apps",
            "/apps/",
            "/apps/a/b",
            "/apps/..",
            "/apps/%61",
            "/apps/a?b",
            "/apps/a#b",
            "/apps//a",
            "/apps2/id",
            "//apps/id",
        ] {
            assert!(!matches("/apps", path), "{path}");
        }
        assert!(!matches("/apps", &format!("/apps/{}", "a".repeat(129))));
    }
}
