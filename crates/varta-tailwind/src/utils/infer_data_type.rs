//! Port of Tailwind CSS v4.3.5 `utils/infer-data-type.ts` (Tailwind Labs,
//! MIT): infer the CSS data type of an arbitrary value from its syntax.
//!
//! Tailwind's regular expressions are matched by hand; `\d` is an ASCII
//! digit and `\s` is ECMAScript white space. Number checks use exact
//! JavaScript `Number(value)` and `String(number)` semantics.

use super::is_color::{is_color, starts_with_ignore_ascii_case};
use super::js_number::{
    is_integer, is_js_whitespace, is_line_terminator, number_to_string, to_number,
};
use super::math_operators::has_math_fn;
use super::segment::segment;

/// Tailwind's `DataType` names, in declaration order.
pub const DATA_TYPES: &[&str] = &[
    "color",
    "length",
    "percentage",
    "ratio",
    "number",
    "integer",
    "url",
    "position",
    "bg-size",
    "line-width",
    "image",
    "family-name",
    "generic-name",
    "absolute-size",
    "relative-size",
    "angle",
    "vector",
];

/// Length units (MDN "Values and units" and container query units).
const LENGTH_UNITS: &[&str] = &[
    "cm", "mm", "Q", "in", "pc", "pt", "px", "em", "ex", "ch", "cap", "ic", "rem", "rex", "rch",
    "rcap", "ric", "lh", "rlh", "vw", "vh", "vmin", "vmax", "vb", "vi", "svw", "svh", "svi", "svb",
    "svmin", "svmax", "lvw", "lvh", "lvi", "lvb", "lvmin", "lvmax", "dvw", "dvh", "dvi", "dvb",
    "dvmin", "dvmax", "cqw", "cqh", "cqi", "cqb", "cqmin", "cqmax",
];
/// Angle units.
const ANGLE_UNITS: &[&str] = &["deg", "rad", "grad", "turn"];
/// Functions that produce an image.
const IMAGE_FUNCTION_PREFIXES: &[&str] = &["element(", "image(", "cross-fade(", "image-set("];
/// Gradient kinds of `(repeating-)?(conic|linear|radial)-gradient(`.
const GRADIENT_KINDS: &[&str] = &["conic", "linear", "radial"];
const REPEATING_PREFIX: &str = "repeating-";
const GRADIENT_SUFFIX: &str = "-gradient(";
/// The spacing function, matched ASCII case-insensitively.
const SPACING_FUNCTION_PREFIX: &str = "--spacing(";
/// Values that hide their type behind a variable.
const VAR_PREFIX: &str = "var(";
const URL_PREFIX: &str = "url(";
const URL_SUFFIX: &str = ")";
const LINE_WIDTH_KEYWORDS: &[&str] = &["thin", "medium", "thick"];
const POSITION_KEYWORDS: &[&str] = &["center", "top", "right", "bottom", "left"];
const BACKGROUND_SIZE_KEYWORDS: &[&str] = &["cover", "contain"];
const AUTO: &str = "auto";
const GENERIC_NAMES: &[&str] = &[
    "serif",
    "sans-serif",
    "monospace",
    "cursive",
    "fantasy",
    "system-ui",
    "ui-serif",
    "ui-sans-serif",
    "ui-monospace",
    "ui-rounded",
    "math",
    "emoji",
    "fangsong",
];
const ABSOLUTE_SIZES: &[&str] = &[
    "xx-small",
    "x-small",
    "small",
    "medium",
    "large",
    "x-large",
    "xx-large",
    "xxx-large",
];
const RELATIVE_SIZES: &[&str] = &["larger", "smaller"];
/// The number of components of a rotation vector.
const VECTOR_COMPONENTS: usize = 3;
/// A background size has one or two values.
const BACKGROUND_SIZE_VALUES: std::ops::RangeInclusive<usize> = 1..=2;
/// Spacing multipliers and opacity values are multiples of this.
const QUARTER: f64 = 0.25;

/// The check of a data type, `None` for an unknown type name.
fn check(data_type: &str) -> Option<fn(&str) -> bool> {
    Some(match data_type {
        "color" => is_color,
        "length" => is_length,
        "percentage" => is_percentage,
        "ratio" => is_fraction,
        "number" => is_number,
        "integer" => is_positive_integer,
        "url" => is_url,
        "position" => is_background_position,
        "bg-size" => is_background_size,
        "line-width" => is_line_width,
        "image" => is_image,
        "family-name" => is_family_name,
        "generic-name" => is_generic_name,
        "absolute-size" => is_absolute_size,
        "relative-size" => is_relative_size,
        "angle" => is_angle,
        "vector" => is_vector,
        _ => return None,
    })
}

/// The first of `types` that `value` matches by CSS syntax, or `None`
/// (always for `var(…)`, whose type is unknown). Unknown type names never
/// match.
#[must_use]
pub fn infer_data_type<'a>(value: &str, types: &[&'a str]) -> Option<&'a str> {
    if value.starts_with(VAR_PREFIX) {
        return None;
    }
    types
        .iter()
        .copied()
        .find(|data_type| check(data_type).is_some_and(|check| check(value)))
}

/* -------------------------------------------------------------------------- */

/// `^url\(.*\)$`: `.` excludes line terminators.
fn is_url(value: &str) -> bool {
    value
        .strip_prefix(URL_PREFIX)
        .and_then(|rest| rest.strip_suffix(URL_SUFFIX))
        .is_some_and(|inside| !inside.chars().any(is_line_terminator))
}

fn is_line_width(value: &str) -> bool {
    segment(value, ' ').iter().all(|part| {
        is_length(part) || is_number(part) || LINE_WIDTH_KEYWORDS.contains(&part.as_str())
    })
}

/// `^(repeating-)?(conic|linear|radial)-gradient\(`.
fn is_gradient_function(value: &str) -> bool {
    let rest = value.strip_prefix(REPEATING_PREFIX).unwrap_or(value);
    GRADIENT_KINDS.iter().any(|kind| {
        rest.strip_prefix(kind)
            .is_some_and(|rest| rest.starts_with(GRADIENT_SUFFIX))
    })
}

fn is_image(value: &str) -> bool {
    let mut count = 0;
    for part in segment(value, ',') {
        if part.starts_with(VAR_PREFIX) {
            continue;
        }
        if is_url(&part)
            || is_gradient_function(&part)
            || IMAGE_FUNCTION_PREFIXES
                .iter()
                .any(|prefix| part.starts_with(prefix))
        {
            count += 1;
            continue;
        }
        return false;
    }
    count > 0
}

fn is_generic_name(value: &str) -> bool {
    GENERIC_NAMES.contains(&value)
}

fn is_family_name(value: &str) -> bool {
    let mut count = 0;
    for part in segment(value, ',') {
        // A part that starts with a digit is not a family name.
        if part.starts_with(|c: char| c.is_ascii_digit()) {
            return false;
        }
        if part.starts_with(VAR_PREFIX) {
            continue;
        }
        count += 1;
    }
    count > 0
}

fn is_absolute_size(value: &str) -> bool {
    ABSOLUTE_SIZES.contains(&value)
}

fn is_relative_size(value: &str) -> bool {
    RELATIVE_SIZES.contains(&value)
}

/* -------------------------------------------------------------------------- */

/// Tailwind's `^[+-]?\d*\.?\d+(?:[eE][+-]?\d+)?$`.
fn is_number_literal(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut pos = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let integer = count_digits(&bytes[pos..]);
    pos += integer;
    if bytes.get(pos) == Some(&b'.') {
        let fraction = count_digits(&bytes[pos + 1..]);
        if fraction == 0 {
            return false;
        }
        pos += 1 + fraction;
    } else if integer == 0 {
        return false;
    }
    if matches!(bytes.get(pos), Some(b'e' | b'E')) {
        pos += 1;
        if matches!(bytes.get(pos), Some(b'+' | b'-')) {
            pos += 1;
        }
        let exponent = count_digits(&bytes[pos..]);
        if exponent == 0 {
            return false;
        }
        pos += exponent;
    }
    pos == bytes.len()
}

/// The number of leading ASCII decimal digits.
fn count_digits(bytes: &[u8]) -> usize {
    bytes.iter().take_while(|b| b.is_ascii_digit()).count()
}

/// A number literal followed by exactly one of `units`. The number ends with
/// a digit and no unit contains one, so the unit is everything after the
/// last digit.
fn is_number_with_unit(value: &str, units: &[&str]) -> bool {
    let Some(last_digit) = value.rfind(|c: char| c.is_ascii_digit()) else {
        return false;
    };
    let (number, unit) = value.split_at(last_digit + 1);
    units.contains(&unit) && is_number_literal(number)
}

fn is_number(value: &str) -> bool {
    is_number_literal(value) || has_math_fn(value)
}

fn is_percentage(value: &str) -> bool {
    value.strip_suffix('%').is_some_and(is_number_literal) || has_math_fn(value)
}

/// `^NUMBER\s*/\s*NUMBER$`.
fn is_fraction(value: &str) -> bool {
    let literal = value
        .split_once('/')
        .is_some_and(|(numerator, denominator)| {
            is_number_literal(numerator.trim_end_matches(is_js_whitespace))
                && is_number_literal(denominator.trim_start_matches(is_js_whitespace))
        });
    literal || has_math_fn(value)
}

/// Whether `value` is a length: a number with a length unit, a
/// `--spacing(…)` call or a math function.
#[must_use]
pub fn is_length(value: &str) -> bool {
    is_number_with_unit(value, LENGTH_UNITS)
        || starts_with_ignore_ascii_case(value, SPACING_FUNCTION_PREFIX)
        || has_math_fn(value)
}

/* -------------------------------------------------------------------------- */

fn is_background_position(value: &str) -> bool {
    let mut count = 0;
    for part in segment(value, ' ') {
        if POSITION_KEYWORDS.contains(&part.as_str()) {
            count += 1;
            continue;
        }
        if part.starts_with(VAR_PREFIX) {
            continue;
        }
        if is_length(&part) || is_percentage(&part) {
            count += 1;
            continue;
        }
        return false;
    }
    count > 0
}

/// Whether `value` is valid for `background-size`:
///
/// ```text
/// background-size = <bg-size>#
/// <bg-size> = [ <length-percentage [0,∞]> | auto ]{1,2} | cover | contain
/// ```
fn is_background_size(value: &str) -> bool {
    let mut count = 0;
    for size in segment(value, ',') {
        if BACKGROUND_SIZE_KEYWORDS.contains(&size.as_str()) {
            count += 1;
            continue;
        }

        let values = segment(&size, ' ');
        // Sizes must have exactly one or two values.
        if !BACKGROUND_SIZE_VALUES.contains(&values.len()) {
            return false;
        }

        if values
            .iter()
            .all(|value| value == AUTO || is_length(value) || is_percentage(value))
        {
            count += 1;
        }
    }
    count > 0
}

/* -------------------------------------------------------------------------- */

/// `<angle> = <number> deg | rad | grad | turn`.
fn is_angle(value: &str) -> bool {
    is_number_with_unit(value, ANGLE_UNITS)
}

/// The vector of `rotate`: `^NUMBER +NUMBER +NUMBER$` with spaces only.
fn is_vector(value: &str) -> bool {
    if value.starts_with(' ') || value.ends_with(' ') {
        return false;
    }
    let components: Vec<&str> = value.split(' ').filter(|part| !part.is_empty()).collect();
    components.len() == VECTOR_COMPONENTS && components.iter().all(|part| is_number_literal(part))
}

/* -------------------------------------------------------------------------- */

/// Whether `value` is a whole number `>= 0` written canonically: `Number`
/// parses it to an integer whose `String` is `value` itself (`"01"` and
/// `"1.0"` are not).
#[must_use]
pub fn is_positive_integer(value: &str) -> bool {
    let num = to_number(value);
    is_integer(num) && num >= 0.0 && number_to_string(num) == value
}

/// Like [`is_positive_integer`], but `0` is excluded.
#[must_use]
pub fn is_strict_positive_integer(value: &str) -> bool {
    let num = to_number(value);
    is_integer(num) && num > 0.0 && number_to_string(num) == value
}

/// Whether `value` is a canonically written multiple of `0.25`, `>= 0`.
#[must_use]
pub fn is_valid_spacing_multiplier(value: &str) -> bool {
    is_multiple_of(value, QUARTER)
}

/// Whether `value` is a canonically written multiple of `0.25`, `>= 0`.
#[must_use]
pub fn is_valid_opacity_value(value: &str) -> bool {
    is_multiple_of(value, QUARTER)
}

/// A number (as text) `>= 0` that is a multiple of `divisor` with no
/// unnecessary leading or trailing zeros.
fn is_multiple_of(value: &str, divisor: f64) -> bool {
    let num = to_number(value);
    num >= 0.0 && num % divisor == 0.0 && number_to_string(num) == value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_type_names_have_checks() {
        assert!(
            DATA_TYPES
                .iter()
                .all(|data_type| check(data_type).is_some())
        );
        assert_eq!(infer_data_type("red", &["toString", "unknown"]), None);
    }

    #[test]
    fn infers_the_first_matching_type() {
        assert_eq!(infer_data_type("var(--a)", DATA_TYPES), None);
        assert_eq!(infer_data_type("red", &["length", "color"]), Some("color"));
        assert_eq!(
            infer_data_type("12px", &["number", "length"]),
            Some("length")
        );
        assert_eq!(infer_data_type("12", &["length", "number"]), Some("number"));
        assert_eq!(
            infer_data_type("12", &["integer", "number"]),
            Some("integer")
        );
        assert_eq!(
            infer_data_type("50%", &["length", "percentage"]),
            Some("percentage")
        );
        assert_eq!(infer_data_type("16/9", &["ratio"]), Some("ratio"));
        assert_eq!(infer_data_type("16 / 9", &["ratio"]), Some("ratio"));
        assert_eq!(infer_data_type("url(a.png)", &["url"]), Some("url"));
        assert_eq!(
            infer_data_type("45deg", &["length", "angle"]),
            Some("angle")
        );
        assert_eq!(infer_data_type("1 0 0", &["vector"]), Some("vector"));
    }

    #[test]
    fn numbers_and_lengths() {
        for value in ["1", "-1.5", "+.5", "1e3", "1E-3", "calc(1px)"] {
            assert!(is_number(value), "{value}");
        }
        for value in ["", "1.", ".", "1e", "1px", "e3", "--1"] {
            assert!(!is_number(value), "{value}");
        }
        for value in [
            "1px",
            "-1.5rem",
            "1e3Q",
            "10cqmin",
            "--spacing(4)",
            "--SPACING(4)",
            "min(1px, 2px)",
        ] {
            assert!(is_length(value), "{value}");
        }
        for value in ["1", "1PX", "px", "1px ", "1.px", "spacing(4)"] {
            assert!(!is_length(value), "{value}");
        }
        assert!(is_percentage("50%"));
        assert!(!is_percentage("50 %"));
        assert!(is_fraction("1\t/\n2"));
        assert!(!is_fraction("1/2/3"));
        assert!(!is_fraction("a/2"));
        assert!(is_angle("1.5turn"));
        assert!(!is_angle("1.5turns"));
        assert!(is_vector("1  -1 .5"));
        assert!(!is_vector(" 1 1 1"));
        assert!(!is_vector("1 1"));
        assert!(!is_vector("1\t1 1"));
    }

    #[test]
    fn urls_and_images() {
        assert!(is_url("url()"));
        assert!(!is_url("url(a\nb)"));
        assert!(!is_url("url("));
        assert!(is_image("url(a.png),var(--b)"));
        // Parts are not trimmed, as in Tailwind.
        assert!(!is_image("url(a.png), var(--b)"));
        assert!(is_image("repeating-linear-gradient(red, blue)"));
        assert!(is_image("image-set(a 1x)"));
        assert!(!is_image("var(--a)"));
        assert!(!is_image("red"));
        assert!(!is_image("repeating-gradient(red)"));
    }

    #[test]
    fn keywords_and_lists() {
        assert!(is_line_width("thin 1px 2"));
        assert!(!is_line_width("thin red"));
        assert!(is_generic_name("ui-monospace"));
        assert!(is_family_name("Inter, var(--fallback)"));
        assert!(!is_family_name("1st, Inter"));
        assert!(!is_family_name("var(--a)"));
        assert!(is_absolute_size("xxx-large"));
        assert!(is_relative_size("smaller"));
        assert!(is_background_position("center 10% var(--y)"));
        assert!(!is_background_position("center red"));
        assert!(is_background_size("cover,auto 50%"));
        assert!(!is_background_size("cover, auto 50%"));
        assert!(is_background_size("red,cover"));
        assert!(!is_background_size("1px 2px 3px"));
        assert!(!is_background_size("red"));
    }

    #[test]
    fn positive_integers_use_javascript_number_semantics() {
        for value in ["0", "1", "10", "123456"] {
            assert!(is_positive_integer(value), "{value}");
        }
        for value in [
            "01", "1.0", "-1", "-0", "1e3", "", " 1", "0x10", "1.5", "abc",
        ] {
            assert!(!is_positive_integer(value), "{value}");
        }
        assert!(is_strict_positive_integer("1"));
        assert!(!is_strict_positive_integer("0"));
        assert!(!is_strict_positive_integer("01"));
    }

    #[test]
    fn multiples_of_a_quarter() {
        for value in ["0", "0.25", "0.5", "2.5", "100"] {
            assert!(is_valid_spacing_multiplier(value), "{value}");
            assert!(is_valid_opacity_value(value), "{value}");
        }
        for value in ["0.50", "0.3", "-0.25", ".5", "1.", "", "Infinity", "NaN"] {
            assert!(!is_valid_spacing_multiplier(value), "{value}");
            assert!(!is_valid_opacity_value(value), "{value}");
        }
    }
}
