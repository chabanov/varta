//! Port of Tailwind CSS v4.3.5 `utils/replace-shadow-colors.ts` (Tailwind
//! Labs, MIT): find the colour of each shadow in a `box-shadow`/`text-shadow`
//! list and replace it.

use super::is_color::is_named_color;
use super::js_number::{is_line_terminator, js_trim};
use super::segment::segment;
use crate::value_parser::{self, ValueNode};

/// Shadow keywords that are neither a length nor a colour.
const KEYWORDS: &[&str] = &["inset", "inherit", "initial", "revert", "unset"];
/// Functions that produce a length.
const LENGTH_FUNCTIONS: &[&str] = &["calc", "clamp", "max", "min", "--spacing"];
/// Functions that produce a colour.
const COLOR_FUNCTIONS: &[&str] = &[
    "color",
    "color-mix",
    "contrast-color",
    "device-cmyk",
    "hsl",
    "hsla",
    "hwb",
    "lab",
    "lch",
    "light-dark",
    "oklab",
    "oklch",
    "rgb",
    "rgba",
    "--alpha",
];
/// The colour a shadow without one uses.
const DEFAULT_SHADOW_COLOR: &str = "currentcolor";
/// The fewest lengths (x and y offsets) a shadow must have for its colour
/// to be located.
const MIN_SHADOW_LENGTHS: usize = 2;

/// Tailwind's `/^-?(\d+|\.\d+)(.*?)$/` on a lowercased word: a number
/// start followed by anything without a line terminator.
fn is_length(word: &str) -> bool {
    let unsigned = word.strip_prefix('-').unwrap_or(word);
    let bytes = unsigned.as_bytes();
    let starts_number = match bytes.first() {
        Some(b'0'..=b'9') => true,
        Some(b'.') => bytes.get(1).is_some_and(u8::is_ascii_digit),
        _ => false,
    };
    starts_number && !unsigned.chars().any(is_line_terminator)
}

/// How a top-level shadow node contributes to finding the colour.
enum Role {
    /// A keyword or separator.
    Ignored,
    Length,
    Color,
    /// Possibly the colour.
    Unknown,
}

fn role(node: &ValueNode) -> Role {
    match node {
        ValueNode::Word(value) => {
            let lower = value.to_lowercase();
            if KEYWORDS.contains(&lower.as_str()) {
                Role::Ignored
            } else if is_length(&lower) {
                Role::Length
            } else if value.starts_with('#') || is_named_color(value) {
                Role::Color
            } else {
                Role::Unknown
            }
        }
        ValueNode::Function { name, .. } => {
            let lower = name.to_lowercase();
            if COLOR_FUNCTIONS.contains(&lower.as_str()) {
                Role::Color
            } else if LENGTH_FUNCTIONS.contains(&lower.as_str()) {
                Role::Length
            } else {
                Role::Unknown
            }
        }
        ValueNode::Separator(_) => Role::Ignored,
    }
}

/// Replace `ast[index]` with the parsed replacement of its text and print
/// the shadow.
fn replace_node(
    mut ast: Vec<ValueNode>,
    index: usize,
    replacement: &dyn Fn(&str) -> String,
) -> String {
    let color = value_parser::to_css(std::slice::from_ref(&ast[index]));
    let replaced = value_parser::parse(&replacement(&color));
    ast.splice(index..=index, replaced);
    value_parser::to_css(&ast)
}

/// Replace the colour of every shadow in `input` with `replacement(colour)`.
/// A shadow without a colour gets `replacement("currentcolor")` appended; a
/// shadow whose offsets cannot be identified (e.g. one `var()` for several
/// fields) is kept as written.
#[must_use]
pub fn replace_shadow_colors(input: &str, replacement: &dyn Fn(&str) -> String) -> String {
    let shadows: Vec<String> = segment(input, ',')
        .iter()
        .map(|shadow| replace_shadow_color(js_trim(shadow), replacement))
        .collect();
    shadows.join(", ")
}

fn replace_shadow_color(shadow: &str, replacement: &dyn Fn(&str) -> String) -> String {
    let ast = value_parser::parse(shadow);

    // Tailwind's walk never descends into functions here: colour functions
    // stop it, every other function is skipped.
    let mut unknown = None;
    let mut unknowns = 0_usize;
    let mut lengths = 0_usize;
    for (index, node) in ast.iter().enumerate() {
        match role(node) {
            Role::Ignored => {}
            Role::Length => lengths += 1,
            // We definitely found a colour.
            Role::Color => return replace_node(ast, index, replacement),
            Role::Unknown => {
                unknown = Some(index);
                unknowns += 1;
            }
        }
    }

    // Without the x and y offsets the shadow is invalid or a variable stands
    // for several fields: we cannot know what to replace.
    if lengths < MIN_SHADOW_LENGTHS {
        return shadow.to_owned();
    }

    // No colour: the shadow relies on the default colour.
    if unknowns == 0 {
        return format!("{shadow} {}", replacement(DEFAULT_SHADOW_COLOR));
    }

    // A single left-over is assumed to be the colour.
    match unknown {
        Some(index) if unknowns == 1 => replace_node(ast, index, replacement),
        _ => shadow.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replacer(color: &str) -> String {
        format!("var(--tw-shadow-color, {color})")
    }

    /// Tailwind's `replaceAlpha(color, '50%')` inside the shadow variable.
    fn alpha_replacer(color: &str) -> String {
        format!("var(--tw-shadow-color, oklab(from {color} l a b / 50%))")
    }

    #[test]
    fn handles_var_shadows() {
        assert_eq!(
            replace_shadow_colors("var(--my-shadow)", &replacer),
            "var(--my-shadow)"
        );
        assert_eq!(
            replace_shadow_colors("1px var(--my-shadow)", &replacer),
            "1px var(--my-shadow)"
        );
        assert_eq!(
            replace_shadow_colors("1px 1px var(--my-color)", &replacer),
            "1px 1px var(--tw-shadow-color, var(--my-color))"
        );
        assert_eq!(
            replace_shadow_colors("0 0 0 var(--my-color)", &replacer),
            "0 0 0 var(--tw-shadow-color, var(--my-color))"
        );
    }

    #[test]
    fn appends_currentcolor_without_a_color() {
        assert_eq!(
            replace_shadow_colors("1px 2px", &replacer),
            "1px 2px var(--tw-shadow-color, currentcolor)"
        );
        assert_eq!(
            replace_shadow_colors("1px 2px 3px", &replacer),
            "1px 2px 3px var(--tw-shadow-color, currentcolor)"
        );
        assert_eq!(
            replace_shadow_colors("1px 2px 3px 4px", &replacer),
            "1px 2px 3px 4px var(--tw-shadow-color, currentcolor)"
        );
    }

    #[test]
    fn finds_the_color_regardless_of_its_position() {
        let xs = ["calc(var(--spacing) * 1)", "1", "--spacing(1)"];
        let ys = ["calc(var(--spacing) * 2)", "2", "--spacing(2)"];
        let blurs = ["calc(var(--spacing) * 3)", "3", "--spacing(3)"];
        let spreads = ["calc(var(--spacing) * 4)", "4", "--spacing(4)"];
        let colors = [
            "black",
            "rgb(0, 0, 0)",
            "#000",
            "--alpha(var(--color) / 50%)",
            "var(--uknown-color)",
        ];
        for x in xs {
            for y in ys {
                for blur in blurs {
                    for spread in spreads {
                        for color in colors {
                            let expected_color = replacer(color);
                            for input in [
                                format!("{x} {color} {y} {blur} {spread}"),
                                format!("{x} {y} {color} {blur} {spread}"),
                                format!("{x} {y} {blur} {color} {spread}"),
                            ] {
                                assert_eq!(
                                    replace_shadow_colors(&input, &replacer),
                                    input.replacen(color, &expected_color, 1),
                                    "{input}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn finds_the_color_among_variables() {
        for color in [
            "black",
            "#000",
            "rgb(0, 0, 0)",
            "--alpha(var(--color) / 50%)",
        ] {
            let expected_color = replacer(color);
            for input in [
                format!("var(--x) var(--y) {color}"),
                format!("var(--x) var(--y) var(--blur) {color}"),
                format!("var(--x) var(--y) var(--blur) var(--spread) {color}"),
            ] {
                assert_eq!(
                    replace_shadow_colors(&input, &replacer),
                    input.replacen(color, &expected_color, 1),
                    "{input}"
                );
            }
        }
    }

    #[test]
    fn handles_multiple_shadows() {
        assert_eq!(
            replace_shadow_colors(
                "var(--my-shadow), 1px 1px var(--my-color), 0 0 1px var(--my-color)",
                &replacer
            ),
            "var(--my-shadow), 1px 1px var(--tw-shadow-color, var(--my-color)), 0 0 1px var(--tw-shadow-color, var(--my-color))"
        );
    }

    #[test]
    fn handles_replacers_with_intensity() {
        assert_eq!(
            replace_shadow_colors("1px 1px var(--my-color)", &alpha_replacer),
            "1px 1px var(--tw-shadow-color, oklab(from var(--my-color) l a b / 50%))"
        );
        assert_eq!(
            replace_shadow_colors("1px 2px 3px 4px", &alpha_replacer),
            "1px 2px 3px 4px var(--tw-shadow-color, oklab(from currentcolor l a b / 50%))"
        );
        assert_eq!(
            replace_shadow_colors(
                "var(--my-shadow), 1px 1px var(--my-color), 0 0 1px var(--my-color)",
                &alpha_replacer
            ),
            "var(--my-shadow), 1px 1px var(--tw-shadow-color, oklab(from var(--my-color) l a b / 50%)), 0 0 1px var(--tw-shadow-color, oklab(from var(--my-color) l a b / 50%))"
        );
    }
}
