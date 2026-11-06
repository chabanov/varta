//! Port of Tailwind CSS v4.3.5 `utils/compare-breakpoints.ts` (Tailwind Labs,
//! MIT): order breakpoint values, grouped by unit or CSS function.

use std::cmp::Ordering;

use super::js_number::{compare_utf16, parse_float};

/// The sort direction of breakpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Asc,
    Desc,
}

/// The bucket of a breakpoint: the name of its CSS function when it has one
/// (`calc(100% - 1rem)` → `calc`), otherwise the value without digits and
/// dots (`64rem` → `rem`).
fn bucket(value: &str) -> String {
    match value.find('(') {
        Some(index) => value[..index].to_owned(),
        None => value
            .chars()
            .filter(|c| !(c.is_ascii_digit() || *c == '.'))
            .collect(),
    }
}

/// Compare two breakpoints: by bucket name first, then numerically in
/// `direction`; values that do not compare as numbers fall back to string
/// order so sorting stays deterministic.
#[must_use]
pub fn compare_breakpoints(a: &str, z: &str, direction: Direction) -> Ordering {
    if a == z {
        return Ordering::Equal;
    }

    let by_bucket = compare_utf16(&bucket(a), &bucket(z));
    if by_bucket.is_ne() {
        return by_bucket;
    }

    let order = match direction {
        Direction::Asc => parse_float(a) - parse_float(z),
        Direction::Desc => parse_float(z) - parse_float(a),
    };

    // Same bucket but not numbers (e.g. `calc(100%-1rem)` and
    // `calc(100%-2rem)`): compare the text.
    match order.partial_cmp(&0.0) {
        Some(order) => order,
        None if compare_utf16(a, z).is_lt() => Ordering::Less,
        None => Ordering::Greater,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Direction::{Asc, Desc};
    use Ordering::{Equal, Greater, Less};

    #[test]
    fn orders_by_bucket_then_value() {
        assert_eq!(compare_breakpoints("40rem", "40rem", Asc), Equal);
        assert_eq!(compare_breakpoints("40rem", "64rem", Asc), Less);
        assert_eq!(compare_breakpoints("40rem", "64rem", Desc), Greater);
        assert_eq!(compare_breakpoints("640px", "40rem", Asc), Less);
        assert_eq!(compare_breakpoints("40rem", "640px", Desc), Greater);
        assert_eq!(compare_breakpoints("40.0rem", "40rem", Asc), Equal);
    }

    #[test]
    fn falls_back_to_text_for_functions() {
        assert_eq!(
            compare_breakpoints("calc(100%-1rem)", "calc(100%-2rem)", Asc),
            Less
        );
        assert_eq!(
            compare_breakpoints("calc(100%-2rem)", "calc(100%-1rem)", Desc),
            Greater
        );
        assert_eq!(compare_breakpoints("calc(1px)", "min(1px)", Asc), Less);
    }
}
