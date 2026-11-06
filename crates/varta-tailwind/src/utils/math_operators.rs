//! Port of Tailwind CSS v4.3.5 `utils/math-operators.ts` (Tailwind Labs,
//! MIT): detect CSS math functions and space their operators.

/// CSS math functions whose arguments are arithmetic expressions.
const MATH_FUNCTIONS: &[&str] = &[
    "calc", "min", "max", "clamp", "mod", "rem", "sin", "cos", "tan", "asin", "acos", "atan",
    "atan2", "pow", "sqrt", "hypot", "log", "exp", "round",
];

const OPEN_PAREN: char = '(';
const CLOSE_PAREN: char = ')';
const COMMA: char = ',';
const SPACE: char = ' ';
const PERCENT: char = '%';
const ADD: char = '+';
const SUB: char = '-';
const MUL: char = '*';
const DIV: char = '/';

/// Whether `input` calls one of the CSS math functions.
#[must_use]
pub fn has_math_fn(input: &str) -> bool {
    input.contains(OPEN_PAREN)
        && MATH_FUNCTIONS.iter().any(|function| {
            input
                .match_indices(function)
                .any(|(index, _)| input[index + function.len()..].starts_with(OPEN_PAREN))
        })
}

fn is_operator(c: Option<char>) -> bool {
    matches!(c, Some(ADD | MUL | DIV | SUB))
}

fn is_digit(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_ascii_digit())
}

/// Insert spaces around `+`, `-`, `*` and `/` (and after commas) inside math
/// functions, leaving keywords such as `fit-content`, `-infinity` and
/// scientific notation (`-3.4e-2`) intact.
#[must_use]
pub fn add_whitespace_around_math_operators(input: &str) -> String {
    // Bail early if there are no math functions in the input.
    if !MATH_FUNCTIONS
        .iter()
        .any(|function| input.contains(function))
    {
        return input.to_owned();
    }

    let chars: Vec<char> = input.chars().collect();
    let mut result = String::with_capacity(input.len());
    // Whether each open parenthesis (innermost last) formats its contents.
    let mut formattable: Vec<bool> = Vec::new();

    let mut value_pos: Option<usize> = None;
    let mut last_value_pos: Option<usize> = None;

    for (i, &c) in chars.iter().enumerate() {
        let formatting = formattable.last().copied().unwrap_or(false);

        // A digit, or a number followed by a unit (`123px`, `50%`), is a
        // value, not a function call.
        if c.is_ascii_digit() || (value_pos.is_some() && (c == PERCENT || c.is_ascii_alphabetic()))
        {
            value_pos = Some(i);
        } else {
            last_value_pos = value_pos;
            value_pos = None;
        }

        if c == OPEN_PAREN {
            result.push(c);

            // Scan backwards for the function name: lowercase alphanumerics.
            let start = chars[..i]
                .iter()
                .rposition(|inner| !(inner.is_ascii_digit() || inner.is_ascii_lowercase()))
                .map_or(0, |boundary| boundary + 1);
            let function: String = chars[start..i].iter().collect();

            // A known math function, or nested parentheses inside one, starts
            // formatting; anything else does not.
            let formats =
                MATH_FUNCTIONS.contains(&function.as_str()) || (formatting && function.is_empty());
            formattable.push(formats);
            continue;
        }

        if c == CLOSE_PAREN {
            result.push(c);
            formattable.pop();
        } else if c == COMMA && formatting {
            result.push_str(", ");
        } else if c == SPACE && formatting && result.ends_with(SPACE) {
            // Skip over consecutive whitespace.
        } else if is_operator(Some(c)) && formatting {
            push_operator(&mut result, &chars, i, last_value_pos);
        } else {
            result.push(c);
        }
    }

    result
}

/// Append the operator at `chars[i]`, spaced according to its neighbours.
fn push_operator(result: &mut String, chars: &[char], i: usize, last_value_pos: Option<usize>) {
    let c = chars[i];
    let mut trimmed = super::js_number::js_trim_end(result).chars().rev();
    let prev = trimmed.next();
    let prev_prev = trimmed.next();
    let next = chars.get(i + 1).copied();

    // Scientific notation (`-3.4e-2`), an operator after an operator, or the
    // start of an argument: no spaces.
    if (matches!(prev, Some('e' | 'E')) && is_digit(prev_prev))
        || is_operator(prev)
        || matches!(prev, Some(OPEN_PAREN | COMMA))
    {
        result.push(c);
    } else if i > 0 && chars[i - 1] == SPACE {
        // Already spaced before: add the space after only.
        result.push(c);
        result.push(SPACE);
    } else if is_digit(prev)
        || is_digit(next)
        || prev == Some(CLOSE_PAREN)
        || next == Some(OPEN_PAREN)
        || is_operator(next)
        || (i > 0 && last_value_pos == Some(i - 1))
    {
        result.push(SPACE);
        result.push(c);
        result.push(SPACE);
    } else {
        result.push(c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_math_functions() {
        assert!(has_math_fn("calc(1px + 2px)"));
        assert!(has_math_fn("var(--a, min(1px, 2px))"));
        assert!(!has_math_fn("calc"));
        assert!(!has_math_fn("var(--calc)"));
        assert!(!has_math_fn("1px"));
    }

    #[test]
    fn leaves_inputs_without_math_functions_alone() {
        assert_eq!(add_whitespace_around_math_operators("a+b"), "a+b");
        assert_eq!(
            add_whitespace_around_math_operators("env(safe-area-inset-bottom)"),
            "env(safe-area-inset-bottom)"
        );
    }

    #[test]
    fn spaces_operators_inside_math_functions() {
        assert_eq!(
            add_whitespace_around_math_operators("calc(1+2)"),
            "calc(1 + 2)"
        );
        assert_eq!(
            add_whitespace_around_math_operators("calc(1  +  2)"),
            "calc(1 + 2)"
        );
        assert_eq!(
            add_whitespace_around_math_operators("min(-3.4e-2-var(--foo),calc-size(auto))"),
            "min(-3.4e-2 - var(--foo), calc-size(auto))"
        );
        assert_eq!(
            add_whitespace_around_math_operators("atan(1 + -infinity)"),
            "atan(1 + -infinity)"
        );
    }
}
