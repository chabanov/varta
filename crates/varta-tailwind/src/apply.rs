//! `@apply` (port of Tailwind's `apply.ts`): replaces `@apply` rules by the
//! declarations of the named utilities, compiled without the design system's
//! `important` flag.
use crate::ast::Node;
use crate::compile::compile_candidates;
use crate::design_system::DesignSystem;
use crate::error::TailwindError;
use crate::stylesheet::compiles;
use crate::utils::segment;

/// Split `@apply` parameters on whitespace (JS `split(/\s+/g)` of trimmed
/// prelude text; empty parts are skipped).
fn apply_candidates(params: &str) -> Vec<&str> {
    params.split_whitespace().collect()
}

/// The utility roots an `@apply` depends on (port of
/// `resolveApplyDependencies`).
#[must_use]
pub fn apply_dependencies(params: &str, ds: &DesignSystem) -> Vec<String> {
    let mut roots = Vec::new();
    for candidate in apply_candidates(params) {
        for parsed in ds.parse_candidate(candidate) {
            if parsed.utility_kind().is_some() {
                roots.push(parsed.root().to_owned());
            }
        }
    }
    roots
}

/// Substitute every `@apply` in `nodes` (port of the substitution walk of
/// `substituteAtApply`). Returns whether any was found.
pub fn substitute_apply(nodes: &mut Vec<Node>, ds: &DesignSystem) -> Result<bool, TailwindError> {
    let mut found = false;
    let mut out = Vec::with_capacity(nodes.len());
    for mut node in std::mem::take(nodes) {
        match &mut node {
            Node::AtRule {
                name, nodes: body, ..
            } if name == "@keyframes" => {
                if contains_apply(body) {
                    return Err(TailwindError::Apply(
                        "you cannot use `@apply` inside `@keyframes`".into(),
                    ));
                }
                out.push(node);
            }
            Node::AtRule {
                name,
                params,
                nodes: body,
            } if name == "@apply" => {
                found = true;
                let candidates = apply_candidates(params);
                let (dashed, normal): (Vec<&str>, Vec<&str>) =
                    candidates.iter().partition(|c| c.starts_with("--"));
                if !dashed.is_empty() {
                    if normal.is_empty() {
                        // A mixin `@apply --x;` stays for the browser.
                        out.push(node);
                        continue;
                    }
                    return Err(TailwindError::Apply(format!(
                        "you cannot use `@apply` with both mixins and utilities; move `@apply {}` into a separate rule",
                        dashed.join(" ")
                    )));
                }
                if !body.is_empty() && !normal.is_empty() {
                    return Err(TailwindError::Apply(format!(
                        "the rule `@apply {}` must not have a body",
                        normal.join(" ")
                    )));
                }
                let mut unique: Vec<&str> = Vec::new();
                for candidate in &normal {
                    if !unique.contains(candidate) {
                        unique.push(candidate);
                    }
                }
                let compiled = compile_candidates(unique.iter().copied(), ds, false);
                if let Some(invalid) = compiled.invalid.first() {
                    return Err(invalid_apply(invalid, ds));
                }
                for compiled_node in compiled.nodes {
                    match compiled_node {
                        Node::Rule { nodes, .. } => out.extend(nodes),
                        other => out.push(other),
                    }
                }
            }
            _ => {
                if let Some(children) = node.children_mut() {
                    found |= substitute_apply(children, ds)?;
                }
                out.push(node);
            }
        }
    }
    *nodes = out;
    Ok(found)
}

fn contains_apply(nodes: &[Node]) -> bool {
    let mut found = false;
    crate::ast::for_each(nodes, &mut |node, _| {
        if matches!(node, Node::AtRule { name, .. } if name == "@apply") {
            found = true;
        }
    });
    found
}

/// The error of an `@apply` candidate that produced nothing.
fn invalid_apply(candidate: &str, ds: &DesignSystem) -> TailwindError {
    if let Some(prefix) = &ds.theme.prefix
        && !candidate.starts_with(prefix.as_str())
    {
        return TailwindError::Apply(format!(
            "cannot apply unprefixed utility class `{candidate}`; did you mean `{prefix}:{candidate}`?"
        ));
    }
    if ds.invalid_candidates.contains(candidate) {
        return TailwindError::Apply(format!(
            "cannot apply utility class `{candidate}` because it has been explicitly disabled"
        ));
    }
    let mut parts = segment(candidate, ':');
    if parts.len() > 1
        && let Some(utility) = parts.pop()
        && compiles(ds, &utility)
    {
        let unknown: Vec<String> = parts
            .iter()
            .filter(|variant| !compiles(ds, &format!("{variant}:[--tw-variant-check:1]")))
            .map(|variant| format!("`{variant}`"))
            .collect();
        match unknown.len() {
            0 => {}
            1 => {
                return TailwindError::Apply(format!(
                    "cannot apply utility class `{candidate}` because the {} variant does not exist",
                    unknown[0]
                ));
            }
            _ => {
                return TailwindError::Apply(format!(
                    "cannot apply utility class `{candidate}` because the {} variants do not exist",
                    english_list(&unknown)
                ));
            }
        }
    }
    TailwindError::UnknownApply(candidate.to_owned())
}

/// `Intl.ListFormat('en', { style: 'long', type: 'conjunction' })`.
fn english_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [init @ .., last] => format!("{}, and {last}", init.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_join_like_intl() {
        assert_eq!(english_list(&["a".into(), "b".into()]), "a and b");
        assert_eq!(
            english_list(&["a".into(), "b".into(), "c".into()]),
            "a, b, and c"
        );
    }
}
