//! Variables used by a value: a port of Tailwind CSS v4.3.5
//! `utils/variables.ts` (MIT, Copyright (c) Tailwind Labs, Inc.; see
//! `LICENSE-TAILWIND`).
use crate::value_parser::{self, ValueNode, WalkAction};

/// The name of the function whose arguments name variables.
const VAR_FUNCTION: &str = "var";
/// The prefix of a custom property name.
const CUSTOM_PROPERTY_PREFIX: &str = "--";

/// Every `--name` word inside the `var(…)` functions of `raw`, in order,
/// including the variables of nested `var(…)` fallbacks.
#[must_use]
pub fn extract_used_variables(raw: &str) -> Vec<String> {
    let mut variables = Vec::new();
    value_parser::walk(&value_parser::parse(raw), &mut |node| {
        let ValueNode::Function { name, nodes } = node else {
            return WalkAction::Continue;
        };
        if name != VAR_FUNCTION {
            return WalkAction::Continue;
        }
        value_parser::walk(nodes, &mut |child| {
            if let ValueNode::Word(word) = child
                && word.starts_with(CUSTOM_PROPERTY_PREFIX)
            {
                variables.push(word.clone());
            }
            WalkAction::Continue
        });
        WalkAction::Skip
    });
    variables
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_variables_and_nested_fallbacks() {
        assert_eq!(
            extract_used_variables("calc(var(--spacing) * var(--a, var(--b, 1px)))"),
            vec!["--spacing", "--a", "--b"]
        );
        assert_eq!(
            extract_used_variables("--not-a-var red"),
            Vec::<String>::new()
        );
        assert_eq!(
            extract_used_variables("var(--tw-shadow-color, rgb(0 0 0 / 0.1))"),
            vec!["--tw-shadow-color"]
        );
    }
}
