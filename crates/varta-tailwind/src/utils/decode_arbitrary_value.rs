//! Port of Tailwind CSS v4.3.5 `utils/decode-arbitrary-value.ts` (Tailwind
//! Labs, MIT): turn the text of an arbitrary value (`[calc(100%-1rem)]`) into
//! CSS, decoding `_` to spaces and spacing math operators.

use super::math_operators::add_whitespace_around_math_operators;
use crate::value_parser::{self, ValueNode};

/// Function names whose argument is a URL: underscores stay as written.
const URL_FUNCTION: &str = "url";
/// Function names whose first argument is a variable or theme key:
/// underscores in that argument stay underscores.
const KEY_FUNCTIONS: &[&str] = &["var", "theme"];

/// Decode an arbitrary value: `_` becomes a space and `\_` an underscore
/// (except inside `url()` and the first argument of `var()`/`theme()`), and
/// math operators inside math functions get surrounding spaces.
#[must_use]
pub fn decode_arbitrary_value(input: &str) -> String {
    // There are definitely no functions in the input, so bail early.
    if !input.contains('(') {
        return convert_underscores_to_whitespace(input, false);
    }

    let mut ast = value_parser::parse(input);
    recursively_decode_arbitrary_values(&mut ast);
    add_whitespace_around_math_operators(&value_parser::to_css(&ast))
}

/// Convert `_` to ` `, and the escaped `\_` to `_`. With
/// `skip_underscore_to_space`, unescaped underscores are kept.
fn convert_underscores_to_whitespace(input: &str, skip_underscore_to_space: bool) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'_') {
            output.push('_');
            chars.next();
        } else if c == '_' && !skip_underscore_to_space {
            output.push(' ');
        } else {
            output.push(c);
        }
    }
    output
}

/// Whether a function name is `name` or ends with `_name` (an underscore
/// that decodes to a space before the call).
fn is_function_named(function: &str, name: &str) -> bool {
    function == name
        || function
            .strip_suffix(name)
            .is_some_and(|prefix| prefix.ends_with('_'))
}

fn recursively_decode_arbitrary_values(ast: &mut [ValueNode]) {
    for node in ast {
        match node {
            ValueNode::Function { name, nodes } => {
                if is_function_named(name, URL_FUNCTION) {
                    // Don't decode underscores in url() but do decode the
                    // function name.
                    *name = convert_underscores_to_whitespace(name, false);
                    continue;
                }

                if KEY_FUNCTIONS
                    .iter()
                    .any(|key_function| is_function_named(name, key_function))
                {
                    *name = convert_underscores_to_whitespace(name, false);
                    for (index, child) in nodes.iter_mut().enumerate() {
                        // Don't decode underscores to spaces in the first
                        // argument of var() or theme().
                        if index == 0
                            && let ValueNode::Word(value) = child
                        {
                            *value = convert_underscores_to_whitespace(value, true);
                            continue;
                        }
                        recursively_decode_arbitrary_values(std::slice::from_mut(child));
                    }
                    continue;
                }

                *name = convert_underscores_to_whitespace(name, false);
                recursively_decode_arbitrary_values(nodes);
            }
            ValueNode::Word(value) | ValueNode::Separator(value) => {
                *value = convert_underscores_to_whitespace(value, false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_underscores_with_spaces() {
        assert_eq!(decode_arbitrary_value("foo_bar"), "foo bar");
        assert_eq!(decode_arbitrary_value("__foo__bar__"), "  foo  bar  ");
        assert_eq!(decode_arbitrary_value("foo\\_bar"), "foo_bar");
    }

    #[test]
    fn keeps_underscores_in_url() {
        assert_eq!(
            decode_arbitrary_value("url(./my_file.jpg)"),
            "url(./my_file.jpg)"
        );
        assert_eq!(
            decode_arbitrary_value("no-repeat_url(./my_file.jpg)"),
            "no-repeat url(./my_file.jpg)"
        );
    }

    #[test]
    fn keeps_underscores_in_the_first_argument_of_var() {
        assert_eq!(
            decode_arbitrary_value("var(--spacing-1_5)"),
            "var(--spacing-1_5)"
        );
        assert_eq!(
            decode_arbitrary_value("var(--spacing-1_5,_1rem)"),
            "var(--spacing-1_5, 1rem)"
        );
        assert_eq!(
            decode_arbitrary_value("var(--spacing-1_5,_var(--spacing-2_5,_1rem))"),
            "var(--spacing-1_5, var(--spacing-2_5, 1rem))"
        );
    }

    #[test]
    fn keeps_underscores_in_the_first_argument_of_theme() {
        assert_eq!(
            decode_arbitrary_value("theme(--spacing-1_5)"),
            "theme(--spacing-1_5)"
        );
        assert_eq!(
            decode_arbitrary_value("theme(--spacing-1_5,_1rem)"),
            "theme(--spacing-1_5, 1rem)"
        );
        assert_eq!(
            decode_arbitrary_value("theme(--spacing-1_5,_theme(--spacing-2_5,_1rem))"),
            "theme(--spacing-1_5, theme(--spacing-2_5, 1rem))"
        );
    }

    #[test]
    fn leaves_var_as_is() {
        assert_eq!(decode_arbitrary_value("var(--foo)"), "var(--foo)");
        assert_eq!(
            decode_arbitrary_value("var(--headings-h1-size)"),
            "var(--headings-h1-size)"
        );
    }

    #[test]
    fn adds_spaces_around_math_operators() {
        let table = [
            // Math functions like calc(…) get spaces around operators.
            ("calc(1+2)", "calc(1 + 2)"),
            ("calc(100%+1rem)", "calc(100% + 1rem)"),
            ("calc(1+calc(100%-20px))", "calc(1 + calc(100% - 20px))"),
            (
                "calc(var(--headings-h1-size)*100)",
                "calc(var(--headings-h1-size) * 100)",
            ),
            (
                "calc(var(--headings-h1-size)*calc(100%+50%))",
                "calc(var(--headings-h1-size) * calc(100% + 50%))",
            ),
            ("min(1+2)", "min(1 + 2)"),
            ("max(1+2)", "max(1 + 2)"),
            ("clamp(1+2,1+3,1+4)", "clamp(1 + 2, 1 + 3, 1 + 4)"),
            (
                "var(--width, calc(100%+1rem))",
                "var(--width, calc(100% + 1rem))",
            ),
            ("calc(1px*(7--12/24))", "calc(1px * (7 - -12 / 24))"),
            ("calc((7-32)/(1400-782))", "calc((7 - 32) / (1400 - 782))"),
            ("calc((7-3)/(1400-782))", "calc((7 - 3) / (1400 - 782))"),
            ("calc((70-3)/(1400-782))", "calc((70 - 3) / (1400 - 782))"),
            ("calc((70-32)/(1400-782))", "calc((70 - 32) / (1400 - 782))"),
            ("calc((704-3)/(1400-782))", "calc((704 - 3) / (1400 - 782))"),
            ("calc((704-320))", "calc((704 - 320))"),
            ("calc((704-320)/1)", "calc((704 - 320) / 1)"),
            (
                "calc((704-320)/(1400-782))",
                "calc((704 - 320) / (1400 - 782))",
            ),
            ("calc(24px+-1rem)", "calc(24px + -1rem)"),
            ("calc(24px+(-1rem))", "calc(24px + (-1rem))"),
            ("calc(24px_+_-1rem)", "calc(24px + -1rem)"),
            ("calc(24px_+_(-1rem))", "calc(24px + (-1rem))"),
            (
                "calc(var(--10-10px,calc(-20px-(-30px--40px)-50px)))",
                "calc(var(--10-10px,calc(-20px - (-30px - -40px) - 50px)))",
            ),
            ("calc(theme(spacing.1-bar))", "calc(theme(spacing.1-bar))"),
            ("theme(spacing.1-bar)", "theme(spacing.1-bar)"),
            (
                "calc(1rem-theme(spacing.1-bar))",
                "calc(1rem - theme(spacing.1-bar))",
            ),
            ("calc(theme(spacing.foo-2))", "calc(theme(spacing.foo-2))"),
            (
                "calc(theme(spacing.foo-bar))",
                "calc(theme(spacing.foo-bar))",
            ),
            // With percentages.
            ("calc(100%-var(--foo))", "calc(100% - var(--foo))"),
            // With uppercase units.
            (
                "calc(100PX-theme(spacing.1))",
                "calc(100PX - theme(spacing.1))",
            ),
            // Keywords like fit-content keep their hyphens.
            (
                "min(fit-content,calc(100dvh-4rem))",
                "min(fit-content, calc(100dvh - 4rem))",
            ),
            (
                "min(theme(spacing.foo-bar),fit-content,calc(20*calc(40-30)))",
                "min(theme(spacing.foo-bar), fit-content, calc(20 * calc(40 - 30)))",
            ),
            (
                "min(fit-content,calc(100dvh-4rem)-calc(50dvh--2px))",
                "min(fit-content, calc(100dvh - 4rem) - calc(50dvh - -2px))",
            ),
            (
                "min(-3.4e-2-var(--foo),calc-size(auto))",
                "min(-3.4e-2 - var(--foo), calc-size(auto))",
            ),
            (
                "clamp(-10e3-var(--foo),calc-size(max-content),var(--foo)+-10e3)",
                "clamp(-10e3 - var(--foo), calc-size(max-content), var(--foo) + -10e3)",
            ),
            // A negative number immediately after a `,` gets no spaces.
            (
                "clamp(-3px+4px,-3px+4px,-3px+4px)",
                "clamp(-3px + 4px, -3px + 4px, -3px + 4px)",
            ),
            // No formatting inside `var()` or `env()`.
            ("calc(var(--foo-bar-bar)*2)", "calc(var(--foo-bar-bar) * 2)"),
            (
                "calc(env(safe-area-inset-bottom)*2)",
                "calc(env(safe-area-inset-bottom) * 2)",
            ),
            // Dashed functions that look like known dashed idents.
            (
                "fit-content(min(max-content,max(min-content,calc(20px+1em))))",
                "fit-content(min(max-content, max(min-content, calc(20px + 1em))))",
            ),
            // `calc()` nested in `env()` is formatted.
            (
                "env(safe-area-inset-bottom,calc(10px+20px))",
                "env(safe-area-inset-bottom,calc(10px + 20px))",
            ),
            (
                "calc(env(safe-area-inset-bottom,calc(10px+20px))+5px)",
                "calc(env(safe-area-inset-bottom,calc(10px + 20px)) + 5px)",
            ),
            // Keywords are not formatted.
            ("minmax(min-content,25%)", "minmax(min-content,25%)"),
            (
                "radial-gradient(calc(1+2)),radial-gradient(calc(1+2))",
                "radial-gradient(calc(1 + 2)),radial-gradient(calc(1 + 2))",
            ),
            (
                "w-[calc(anchor-size(width)+8px)]",
                "w-[calc(anchor-size(width) + 8px)]",
            ),
            (
                "w-[calc(anchor-size(foo(bar))+8px)]",
                "w-[calc(anchor-size(foo(bar)) + 8px)]",
            ),
            (
                "[content-start]_calc(100%-1px)_[content-end]_minmax(1rem,1fr)",
                "[content-start] calc(100% - 1px) [content-end] minmax(1rem,1fr)",
            ),
            // round(…).
            ("round(1+2,1+3)", "round(1 + 2, 1 + 3)"),
            ("round(to-zero,1+2,1+3)", "round(to-zero, 1 + 2, 1 + 3)"),
            // Nested parentheses in non-math functions are not formatted.
            (
                "env((safe-area-inset-bottom))",
                "env((safe-area-inset-bottom))",
            ),
            // `-infinity` is a keyword.
            ("atan(1 + -infinity)", "atan(1 + -infinity)"),
        ];
        for (input, expected) in table {
            assert_eq!(decode_arbitrary_value(input), expected, "{input}");
        }
    }
}
