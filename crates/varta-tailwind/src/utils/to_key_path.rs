//! Port of Tailwind CSS v4.3.5 `utils/to-key-path.ts` (Tailwind Labs, MIT):
//! split a theme key path into its segments.

use super::segment::segment;

/// Parse a path into its segments. Square brackets may "escape" dots that
/// would otherwise separate segments:
///
/// - `a.b.c` → `["a", "b", "c"]`
/// - `a[b].c` → `["a", "b", "c"]`
/// - `a[b.c].e.f` → `["a", "b.c", "e", "f"]`
/// - `a[b][c][d]` → `["a", "b", "c", "d"]`
#[must_use]
pub fn to_key_path(path: &str) -> Vec<String> {
    let mut keypath = Vec::new();

    for part in segment(path, '.') {
        if !part.contains('[') {
            keypath.push(part);
            continue;
        }

        let mut current = 0;
        while let Some(left) = part[current..].find('[').map(|offset| current + offset) {
            let Some(right) = part[left..].find(']').map(|offset| left + offset) else {
                break;
            };

            // The part before the bracket is a key.
            if left > current {
                keypath.push(part[current..left].to_owned());
            }

            // The part inside the bracket is a key.
            keypath.push(part[left + 1..right].to_owned());
            current = right + 1;
        }

        // The part after the last bracket is a key.
        if current < part.len() {
            keypath.push(part[current..].to_owned());
        }
    }

    keypath
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_key_paths_to_arrays() {
        let cases: &[(&str, &[&str])] = &[
            ("fontSize.xs", &["fontSize", "xs"]),
            (
                "fontSize.xs[1].lineHeight",
                &["fontSize", "xs", "1", "lineHeight"],
            ),
            ("colors.red.500", &["colors", "red", "500"]),
            ("colors[red].500", &["colors", "red", "500"]),
            ("colors[red].[500]", &["colors", "red", "500"]),
            ("colors[red]500", &["colors", "red", "500"]),
            ("colors[red][500]", &["colors", "red", "500"]),
            ("colors[red]500[50]5", &["colors", "red", "500", "50", "5"]),
            ("a[b.c].e.f", &["a", "b.c", "e", "f"]),
            ("a[b", &["a[b"]),
        ];
        for (input, expected) in cases {
            assert_eq!(to_key_path(input), *expected, "{input}");
        }
    }
}
