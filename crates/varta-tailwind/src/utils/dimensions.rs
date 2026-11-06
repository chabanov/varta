//! Port of Tailwind CSS v4.3.5 `utils/dimensions.ts` (Tailwind Labs, MIT):
//! split a dimension such as `64rem` into its number and unit.
//!
//! Tailwind memoizes the parse in a `DefaultMap`; the function is pure, so
//! the port computes it directly.

use super::js_number::to_number;

/// Parse a dimension (Tailwind's `/^[-+]?(?:\d*\.)?\d+([a-z]+|%)?$/i`):
/// `64rem` gives `(64, Some("rem"))`, `-.5` gives `(-0.5, None)`.
#[must_use]
pub fn parse_dimension(input: &str) -> Option<(f64, Option<String>)> {
    let bytes = input.as_bytes();
    let sign = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let integer_end = sign + count_digits(&bytes[sign..]);
    let number_end = if bytes.get(integer_end) == Some(&b'.') {
        let fraction = count_digits(&bytes[integer_end + 1..]);
        if fraction == 0 {
            return None;
        }
        integer_end + 1 + fraction
    } else if integer_end > sign {
        integer_end
    } else {
        return None;
    };

    let unit = &input[number_end..];
    let unit_is_valid =
        unit.is_empty() || unit == "%" || unit.bytes().all(|b| b.is_ascii_alphabetic());
    if !unit_is_valid {
        return None;
    }

    let value = to_number(&input[..number_end]);
    if value.is_nan() {
        return None;
    }
    Some((value, (!unit.is_empty()).then(|| unit.to_owned())))
}

/// The number of leading ASCII decimal digits.
fn count_digits(bytes: &[u8]) -> usize {
    bytes.iter().take_while(|b| b.is_ascii_digit()).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dimensions() {
        assert_eq!(parse_dimension("64rem"), Some((64.0, Some("rem".into()))));
        assert_eq!(parse_dimension("50%"), Some((50.0, Some("%".into()))));
        assert_eq!(parse_dimension("100PX"), Some((100.0, Some("PX".into()))));
        assert_eq!(parse_dimension("-.5"), Some((-0.5, None)));
        assert_eq!(parse_dimension("+1.25em"), Some((1.25, Some("em".into()))));
        assert_eq!(parse_dimension("12"), Some((12.0, None)));
    }

    #[test]
    fn rejects_non_dimensions() {
        for input in [
            "",
            "rem",
            "1.",
            ".",
            "-",
            "1e3px",
            "1%%",
            "1px%",
            "calc(1px)",
            " 1px",
        ] {
            assert_eq!(parse_dimension(input), None, "{input}");
        }
    }
}
