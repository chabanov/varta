//! CSS-defined utilities: `@utility name { … }` (port of `createCssUtility`,
//! `resolveValueFunction`, `isValidStaticUtilityName` and
//! `isValidFunctionalUtilityName` of Tailwind CSS v4.3.5 `src/utilities.ts`,
//! MIT, Copyright (c) Tailwind Labs, Inc.).
//!
//! A static `@utility tab-4 { … }` registers its body as is. A functional
//! `@utility tab-* { … }` resolves `--value(…)` and `--modifier(…)` against
//! the candidate per declaration: theme keys (`--value(--tab-size-*)`), bare
//! values of a few data types (`--value(integer)`), literals
//! (`--value('inherit')`), arbitrary values (`--value([length])`,
//! `--value([*])`), ratios and `--default(…)`. A declaration whose function
//! does not resolve is dropped; the candidate is valid only when some
//! `--value(…)` resolves and every modifier is consumed.
//!
//! Suggestions (Tailwind's `suggest`) and the console warning about
//! unsupported bare data types are not ported.
use crate::ast::Node;
use crate::candidate::{Candidate, CandidateModifier, UtilityValue};
use crate::css_functions::substitute_functions;
use crate::design_system::DesignSystem;
use crate::theme::{Theme, options};
use crate::utilities::Compiled;
use crate::utils::infer_data_type::{
    infer_data_type, is_positive_integer, is_valid_spacing_multiplier,
};
use crate::utils::js_number::{
    is_integer, is_js_whitespace, is_line_terminator, js_trim, to_number,
};
use crate::utils::{segment, unescape};
use crate::value_parser::{self, ValueNode, WalkAction};
use crate::variants::substitute_at_variant;

/// Registers one `@utility` on a design system.
pub type CustomUtility = Box<dyn FnOnce(&mut DesignSystem)>;

/// The bare value data types `--value(…)`/`--modifier(…)` admit (Tailwind's
/// `BARE_VALUE_DATA_TYPES`); anything else (e.g. `color`, which would admit
/// `text-#0088cc`) never resolves a bare value.
pub const BARE_VALUE_DATA_TYPES: &[&str] = &["number", "integer", "ratio", "percentage"];

/// The at-rule name of a CSS-defined utility.
pub const UTILITY_AT_RULE: &str = "@utility";

/// The suffix marking the dynamic part of a functional utility name.
const FUNCTIONAL_SUFFIX: &str = "-*";
/// The function resolving the candidate value.
const VALUE_FN: &str = "--value";
/// The function resolving the candidate modifier.
const MODIFIER_FN: &str = "--modifier";
/// The function giving the value when the candidate has none.
const DEFAULT_FN: &str = "--default";
/// The prefix of an unnormalized `--default(…)` argument, kept as written.
const DEFAULT_CALL_PREFIX: &str = "--default(";
/// The prefix of a theme key argument.
const THEME_KEY_PREFIX: &str = "--";
/// The bare data type that resolves `lhs/rhs` fractions.
const RATIO_TYPE: &str = "ratio";
/// The bare data type of decimal multipliers.
const NUMBER_TYPE: &str = "number";
/// The bare data type of integer percentages.
const PERCENTAGE_TYPE: &str = "percentage";
/// The argument of `--value([*])`: any arbitrary value.
const ANY_ARBITRARY_TYPE: &str = "*";
/// The escaped spelling of `*` that formatters prefer (`--value(--x-\*)`).
const ESCAPED_WILDCARD: &str = "\\*";

/// Create the registration of a `@utility` rule (port of `createCssUtility`),
/// or `None` when `node` is not an at-rule or its name is neither a valid
/// functional nor a valid static utility name.
///
/// The registration normalizes `--value(…)`/`--modifier(…)` arguments, then
/// substitutes `@variant` rules and build-time CSS functions in the body (as
/// Tailwind's stylesheet build does to the `@utility` node after registering
/// it) and registers the utility. A substitution that fails leaves the body
/// unchanged, so the candidate-time substitution fails again and the
/// candidate produces nothing (Tailwind throws at build time instead).
#[must_use]
pub fn create_css_utility(node: &Node) -> Option<CustomUtility> {
    let Node::AtRule { params, .. } = node else {
        return None;
    };
    // Escapes are allowed for formatters: `@utility foo-1\/2` is `foo-1/2`.
    let name = unescape(params);

    if is_valid_functional_utility_name(&name) {
        let root = name[..name.len() - FUNCTIONAL_SUFFIX.len()].to_owned();
        let mut body = node.clone();
        return Some(Box::new(move |ds: &mut DesignSystem| {
            if let Some(children) = body.children_mut() {
                normalize_value_functions(children);
            }
            let body = substitute_build_time(body, ds);
            ds.utilities.functional(&root, move |candidate, ds| {
                compile_functional(&body, candidate, &ds.theme)
            });
        }));
    }

    if is_valid_static_utility_name(&name) {
        let body = node.clone();
        return Some(Box::new(move |ds: &mut DesignSystem| {
            let body = substitute_build_time(body, ds);
            let nodes = body.children().cloned().unwrap_or_default();
            ds.utilities
                .static_fn(&name, move |_, _| Compiled::Nodes(nodes.clone()));
        }));
    }

    None
}

/// Substitute `@variant` rules and then build-time functions in the body,
/// each step kept only when it succeeds.
fn substitute_build_time(body: Node, ds: &DesignSystem) -> Node {
    let mut ast = vec![body];
    let mut variants = ast.clone();
    if substitute_at_variant(&mut variants, ds).is_ok() {
        ast = variants;
    }
    let mut functions = ast.clone();
    if substitute_functions(&mut functions, ds).is_ok() {
        ast = functions;
    }
    // Invariant: both substitutions map the one `@utility` at-rule to itself.
    ast.pop().unwrap_or(Node::AtRoot(Vec::new()))
}

/* ------------------------------ Preprocessing ----------------------------- */

/// Normalize the arguments of every `--value(…)`/`--modifier(…)` in the
/// declarations of `nodes`, at any depth.
fn normalize_value_functions(nodes: &mut [Node]) {
    crate::ast::for_each_mut(nodes, &mut |node| {
        if let Node::Decl {
            value: Some(value), ..
        } = node
            && !value.is_empty()
            && (value.contains("--value(") || value.contains("--modifier("))
        {
            *value = normalize_declaration_value(value);
        }
    });
}

/// Normalize the value functions of one declaration value:
///
/// - `--value(--spacing)` -> `--value(--spacing-*)`
/// - `--value(--spacing- *)` -> `--value(--spacing-*)`
/// - `--value(--text- * --line-height)` -> `--value(--text-*--line-height)`
/// - `--value(--text --line-height)` -> `--value(--text-*--line-height)`
/// - `--value(--text-\* --line-height)` -> `--value(--text-*--line-height)`
/// - `--value([ *])` -> `--value([*])`
fn normalize_declaration_value(value: &str) -> String {
    let mut ast = value_parser::parse(value);
    value_parser::walk_mut(&mut ast, &mut |node| {
        if let ValueNode::Function { name, nodes } = node
            && (name == VALUE_FN || name == MODIFIER_FN)
        {
            let args: Vec<String> = segment(&value_parser::to_css(nodes), ',')
                .iter()
                .map(|arg| normalize_argument(arg))
                .collect();
            *nodes = value_parser::parse(&args.join(","));
        }
        WalkAction::Continue
    });
    value_parser::to_css(&ast)
}

/// Normalize one argument of a value function.
fn normalize_argument(arg: &str) -> String {
    let arg = js_trim(arg);
    // The value of `--default(…)` is emitted as is.
    if arg.starts_with(DEFAULT_CALL_PREFIX) {
        return arg.to_owned();
    }
    let arg = arg.replace(ESCAPED_WILDCARD, ANY_ARBITRARY_TYPE);
    let arg = join_spaced_theme_keys(&arg);
    let arg: String = arg.chars().filter(|c| !is_js_whitespace(*c)).collect();
    let mut arg = collapse_repeated_wildcards(&arg);
    if arg.starts_with(THEME_KEY_PREFIX) && !arg.contains('(') && !arg.contains(FUNCTIONAL_SUFFIX) {
        arg.push_str(FUNCTIONAL_SUFFIX);
    }
    arg
}

/// `arg.replace(/--(.*?)\s--(.*?)/g, '--$1-*--$2')`: join `--a --b` into
/// `--a-*--b`. `.` excludes line terminators; the lazy second group is empty.
fn join_spaced_theme_keys(arg: &str) -> String {
    let chars: Vec<char> = arg.chars().collect();
    let is_double_dash = |at: usize| chars.get(at) == Some(&'-') && chars.get(at + 1) == Some(&'-');
    let mut out = String::with_capacity(arg.len());
    let mut i = 0;
    while i < chars.len() {
        if is_double_dash(i) {
            let mut end = None;
            let mut j = i + 2;
            while j < chars.len() {
                if is_js_whitespace(chars[j]) && is_double_dash(j + 1) {
                    end = Some(j);
                    break;
                }
                if is_line_terminator(chars[j]) {
                    break;
                }
                j += 1;
            }
            if let Some(j) = end {
                out.push_str(THEME_KEY_PREFIX);
                out.extend(&chars[i + 2..j]);
                out.push_str(FUNCTIONAL_SUFFIX);
                out.push_str(THEME_KEY_PREFIX);
                i = j + 1 + THEME_KEY_PREFIX.len();
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// `arg.replace(/(-\*){2,}/g, '-*')`.
fn collapse_repeated_wildcards(arg: &str) -> String {
    let mut out = String::with_capacity(arg.len());
    let mut rest = arg;
    while !rest.is_empty() {
        let mut repeats = 0;
        let mut after = rest;
        while let Some(next) = after.strip_prefix(FUNCTIONAL_SUFFIX) {
            repeats += 1;
            after = next;
        }
        if repeats >= 2 {
            out.push_str(FUNCTIONAL_SUFFIX);
            rest = after;
            continue;
        }
        let mut chars = rest.chars();
        if let Some(c) = chars.next() {
            out.push(c);
        }
        rest = chars.as_str();
    }
    out
}

/* ------------------------------- Compilation ------------------------------ */

/// What the value functions of one candidate resolved.
#[derive(Default)]
struct Resolution {
    /// Whether `--value(…)` was used.
    used_value: bool,
    /// Whether any `--value(…)` resolved.
    resolved_value: bool,
    /// Whether `--modifier(…)` was used.
    used_modifier: bool,
    /// Whether any `--modifier(…)` resolved.
    resolved_modifier: bool,
    /// Whether a `--value(ratio)` resolved.
    resolved_ratio: bool,
    /// Paths (child indexes from the `@utility` rule) of the kept
    /// declarations that resolved a non-ratio `--value(…)`.
    non_ratio_declarations: Vec<Vec<usize>>,
}

/// The value or modifier of a candidate, as `resolveValueFunction` sees it.
#[derive(Clone, Copy)]
enum Input<'a> {
    Named {
        value: &'a str,
        /// `Some` for a utility value (which has a `fraction` field, possibly
        /// `null`), `None` for a modifier (which has none).
        fraction: Option<Option<&'a str>>,
    },
    Arbitrary {
        value: &'a str,
        data_type: Option<&'a str>,
    },
}

impl<'a> Input<'a> {
    fn from_value(value: &'a UtilityValue) -> Self {
        match value {
            UtilityValue::Named { value, fraction } => Input::Named {
                value,
                fraction: Some(fraction.as_deref()),
            },
            UtilityValue::Arbitrary { value, data_type } => Input::Arbitrary {
                value,
                data_type: data_type.as_deref(),
            },
        }
    }

    fn from_modifier(modifier: &'a CandidateModifier) -> Self {
        match modifier {
            CandidateModifier::Named(value) => Input::Named {
                value,
                fraction: None,
            },
            CandidateModifier::Arbitrary(value) => Input::Arbitrary {
                value,
                data_type: None,
            },
        }
    }
}

/// The inputs and theme of one candidate compilation.
struct Context<'a> {
    value: Option<Input<'a>>,
    modifier: Option<Input<'a>>,
    theme: &'a Theme,
}

/// The compile function of a functional CSS utility.
fn compile_functional(body: &Node, candidate: &Candidate, theme: &Theme) -> Compiled {
    let mut at_rule = body.clone();
    let context = Context {
        value: candidate.value().map(Input::from_value),
        modifier: candidate.modifier().map(Input::from_modifier),
        theme,
    };
    let mut resolution = Resolution::default();
    let Some(children) = at_rule.children_mut() else {
        return Compiled::Invalid;
    };
    let mut path = Vec::new();
    resolve_children(children, true, &mut path, &context, &mut resolution);

    // A functional CSS utility needs a `--value(…)`, and one must resolve.
    if !resolution.used_value || !resolution.resolved_value {
        return Compiled::Invalid;
    }
    // A `--modifier(…)` was used but none resolved.
    if resolution.used_modifier && !resolution.resolved_modifier && context.modifier.is_some() {
        return Compiled::Invalid;
    }
    // A ratio value consumes the modifier, so a resolved modifier is invalid.
    if resolution.resolved_ratio && resolution.resolved_modifier {
        return Compiled::Invalid;
    }
    // A modifier must be consumed by `--modifier(…)` or `--value(ratio)`.
    if context.modifier.is_some() && !resolution.resolved_ratio && !resolution.resolved_modifier {
        return Compiled::Invalid;
    }
    // With a ratio value, declarations resolving other value types go: e.g.
    // `--value(number)` would otherwise also resolve `foo-1/2`.
    if resolution.resolved_ratio {
        let mut paths = std::mem::take(&mut resolution.non_ratio_declarations);
        paths.sort_unstable_by(|a, z| z.cmp(a));
        paths.dedup();
        for path in &paths {
            remove_at_path(children, path);
        }
    }
    Compiled::Nodes(std::mem::take(children))
}

/// Remove the node at `path` (child indexes) under `nodes`.
fn remove_at_path(nodes: &mut Vec<Node>, path: &[usize]) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    let mut current = nodes;
    for index in parents {
        let Some(children) = current.get_mut(*index).and_then(Node::children_mut) else {
            return;
        };
        current = children;
    }
    if *last < current.len() {
        current.remove(*last);
    }
}

/// Resolve the value functions of the declarations among `nodes` (whose
/// parent is a rule when `in_rule`), dropping those that do not resolve.
fn resolve_children(
    nodes: &mut Vec<Node>,
    in_rule: bool,
    path: &mut Vec<usize>,
    context: &Context<'_>,
    resolution: &mut Resolution,
) {
    let mut out = Vec::with_capacity(nodes.len());
    for mut node in std::mem::take(nodes) {
        let is_rule = node.is_rule_like();
        match &mut node {
            Node::Decl {
                value: Some(value), ..
            } if in_rule && !value.is_empty() => {
                let mut ast = value_parser::parse(value);
                let mut non_ratio = false;
                if resolve_value_nodes(&mut ast, context, resolution, &mut non_ratio).is_err() {
                    continue;
                }
                *value = value_parser::to_css(&ast);
                if non_ratio {
                    let mut declaration = path.clone();
                    declaration.push(out.len());
                    resolution.non_ratio_declarations.push(declaration);
                }
            }
            _ => {
                if let Some(children) = node.children_mut() {
                    path.push(out.len());
                    resolve_children(children, is_rule, path, context, resolution);
                    path.pop();
                }
            }
        }
        out.push(node);
    }
    *nodes = out;
}

/// A value function that did not resolve: its declaration is dropped.
struct Unresolved;

/// Replace each `--value(…)`/`--modifier(…)` among `nodes` (depth-first) by
/// what it resolves to; stop at the first that does not resolve.
fn resolve_value_nodes(
    nodes: &mut Vec<ValueNode>,
    context: &Context<'_>,
    resolution: &mut Resolution,
    non_ratio: &mut bool,
) -> Result<(), Unresolved> {
    let mut out = Vec::with_capacity(nodes.len());
    for node in std::mem::take(nodes) {
        match node {
            ValueNode::Function { name, nodes: args } if name == VALUE_FN => {
                resolution.used_value = true;
                let resolved = resolve_value_function(context.value, &args, context.theme)
                    .ok_or(Unresolved)?;
                resolution.resolved_value = true;
                if resolved.ratio {
                    resolution.resolved_ratio = true;
                } else {
                    *non_ratio = true;
                }
                out.extend(resolved.nodes);
            }
            ValueNode::Function { name, nodes: args } if name == MODIFIER_FN => {
                resolution.used_modifier = true;
                let resolved = resolve_value_function(context.modifier, &args, context.theme)
                    .ok_or(Unresolved)?;
                resolution.resolved_modifier = true;
                out.extend(resolved.nodes);
            }
            ValueNode::Function {
                name,
                nodes: mut args,
            } => {
                resolve_value_nodes(&mut args, context, resolution, non_ratio)?;
                out.push(ValueNode::Function { name, nodes: args });
            }
            other => out.push(other),
        }
    }
    *nodes = out;
    Ok(())
}

/// What a value function resolved to.
struct Resolved {
    nodes: Vec<ValueNode>,
    /// Whether it resolved through `ratio`.
    ratio: bool,
}

impl Resolved {
    fn parse(value: &str, ratio: bool) -> Self {
        Self {
            nodes: value_parser::parse(value),
            ratio,
        }
    }
}

/// The text inside a quoted literal argument (`'a'` or `"a"`), if quoted.
fn literal(arg: &str) -> Option<&str> {
    let quote = arg.chars().next().filter(|c| matches!(c, '\'' | '"'))?;
    if !arg.ends_with(quote) {
        return None;
    }
    // A lone quote is its own closing quote: its content is empty.
    Some(arg.get(1..arg.len() - 1).unwrap_or_default())
}

/// The data type inside an arbitrary argument (`[length]`), if bracketed.
fn arbitrary_type(arg: &str) -> Option<&str> {
    if arg.starts_with('[') && arg.ends_with(']') {
        Some(arg.get(1..arg.len() - 1).unwrap_or_default())
    } else {
        None
    }
}

/// Resolve one value function against a candidate value or modifier (port
/// of `resolveValueFunction`); `args` are the function's nodes.
fn resolve_value_function(
    input: Option<Input<'_>>,
    args: &[ValueNode],
    theme: &Theme,
) -> Option<Resolved> {
    // No value: only `--default(…)` resolves.
    let Some(input) = input else {
        return args.iter().find_map(|arg| match arg {
            ValueNode::Function { name, nodes } if name == DEFAULT_FN => Some(Resolved {
                nodes: nodes.clone(),
                ratio: false,
            }),
            _ => None,
        });
    };

    for arg in args {
        let ValueNode::Word(arg) = arg else {
            continue;
        };
        let resolved = match input {
            Input::Named { value, fraction } => resolve_named(arg, value, fraction, theme),
            Input::Arbitrary { value, data_type } => resolve_arbitrary(arg, value, data_type),
        };
        if resolved.is_some() {
            return resolved;
        }
    }
    None
}

/// Resolve a named value or modifier through one argument word.
fn resolve_named(
    arg: &str,
    value: &str,
    fraction: Option<Option<&str>>,
    theme: &Theme,
) -> Option<Resolved> {
    // A literal, e.g. `--modifier('closest-side')`.
    if let Some(literal) = literal(arg) {
        return (literal == value).then(|| Resolved::parse(value, false));
    }
    // A theme value, e.g. `--value(--color-*)` or `--value(--text-*--line-height)`.
    if arg.starts_with(THEME_KEY_PREFIX) {
        return resolve_theme(arg, value, theme);
    }
    // A bare value, e.g. `--value(integer)`; only a few types are admitted.
    if !BARE_VALUE_DATA_TYPES.contains(&arg) {
        return None;
    }
    let resolved = match fraction {
        Some(fraction) if arg == RATIO_TYPE => fraction?,
        _ => value,
    };
    if resolved.is_empty() {
        return None;
    }
    let data_type = infer_data_type(resolved, &[arg])?;
    if data_type == RATIO_TYPE {
        // A ratio must be `<integer>/<integer>`.
        let parts = segment(resolved, '/');
        let [lhs, rhs, ..] = parts.as_slice() else {
            return None;
        };
        if !is_positive_integer_number(lhs) || !is_positive_integer_number(rhs) {
            return None;
        }
        return Some(Resolved::parse(
            &format!("{} / {}", js_trim(lhs), js_trim(rhs)),
            true,
        ));
    }
    // A non-integer number must be a valid multiplier, e.g. `1.5`.
    if data_type == NUMBER_TYPE && !is_valid_spacing_multiplier(resolved) {
        return None;
    }
    // A percentage must be an integer, e.g. `50%`.
    if data_type == PERCENTAGE_TYPE
        && !is_positive_integer(resolved.strip_suffix('%').unwrap_or(resolved))
    {
        return None;
    }
    Some(Resolved::parse(resolved, false))
}

/// `isPositiveInteger(Number(text))`: the number of `text` is a whole number
/// `>= 0` (its own `String` trivially round-trips).
fn is_positive_integer_number(text: &str) -> bool {
    let number = to_number(text);
    is_integer(number) && number >= 0.0
}

/// Resolve a theme key argument (`--key-*` or `--key-*--nested`).
fn resolve_theme(key: &str, value: &str, theme: &Theme) -> Option<Resolved> {
    if let Some(namespace) = key.strip_suffix(FUNCTIONAL_SUFFIX) {
        let resolved = theme.resolve(Some(value), &[namespace], options::NONE)?;
        return (!resolved.is_empty()).then(|| Resolved::parse(&resolved, false));
    }
    // `--text-*--line-height`: the `--line-height` value of `--text-<value>`.
    let mut keys: Vec<&str> = key.split(FUNCTIONAL_SUFFIX).collect();
    if keys.len() <= 1 {
        return None;
    }
    let namespace = keys.remove(0);
    let (_, nested) = theme.resolve_with(value, &[namespace], &keys)?;
    let resolved = nested.get(*keys.last()?)?;
    (!resolved.is_empty()).then(|| Resolved::parse(resolved, false))
}

/// Resolve an arbitrary value or modifier through one argument word.
fn resolve_arbitrary(arg: &str, value: &str, data_type: Option<&str>) -> Option<Resolved> {
    let expected = arbitrary_type(arg)?;
    // Any data type, e.g. `--value([*])`.
    if expected == ANY_ARBITRARY_TYPE {
        return Some(Resolved::parse(value, false));
    }
    // A type hint must match: `tab-(color:var(--x))` is not `[integer]`.
    if let Some(data_type) = data_type.filter(|t| !t.is_empty()) {
        return (data_type == expected).then(|| Resolved::parse(value, false));
    }
    infer_data_type(value, &[expected]).map(|_| Resolved::parse(value, false))
}

/* ---------------------------------- Names --------------------------------- */

/// The length of the match of `^-?[a-z][a-zA-Z0-9_-]*` (Tailwind's
/// `UTILITY_ROOT`), or `None`.
fn utility_root_len(name: &str) -> Option<usize> {
    let bytes = name.as_bytes();
    let start = usize::from(bytes.first() == Some(&b'-'));
    if !bytes.get(start).is_some_and(u8::is_ascii_lowercase) {
        return None;
    }
    let rest = bytes[start + 1..]
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
        .count();
    Some(start + 1 + rest)
}

/// Whether `name` is a valid static utility name (port of
/// `isValidStaticUtilityName`): a lowercase root, then optionally a value
/// with digits around `.`, a final `%` after a digit, and one inner `/`.
#[must_use]
pub fn is_valid_static_utility_name(name: &str) -> bool {
    let Some(root_len) = utility_root_len(name) else {
        return false;
    };
    let (root, value) = name.split_at(root_len);
    // `tab-size-` has a root ending in `-` and no value.
    if value.is_empty() {
        return !root.ends_with('-');
    }
    let value = value.as_bytes();
    let root_last = root.as_bytes().last().copied();
    let previous = |i: usize| {
        if i == 0 {
            root_last
        } else {
            value.get(i - 1).copied()
        }
    };
    let is_digit = |b: Option<u8>| b.is_some_and(|b| b.is_ascii_digit());
    let mut seen_slash = false;
    for (i, &byte) in value.iter().enumerate() {
        match byte {
            // A percentage only at the end, after a digit: `w-%` is invalid.
            b'%' => {
                if i != value.len() - 1 || !is_digit(previous(i)) {
                    return false;
                }
            }
            // One slash, followed by something: `foo/` and `a/b/c` are invalid.
            b'/' => {
                if i == value.len() - 1 || seen_slash {
                    return false;
                }
                seen_slash = true;
            }
            // Dots only between digits: `p-1.a` is invalid.
            b'.' => {
                if !is_digit(previous(i)) || !is_digit(value.get(i + 1).copied()) {
                    return false;
                }
            }
            b'_' | b'-' => {}
            b if b.is_ascii_alphanumeric() => {}
            _ => return false,
        }
    }
    true
}

/// Whether `name` is a valid functional utility name (port of
/// `isValidFunctionalUtilityName`): a root followed by `-*`. A root ending
/// in `-` stays valid for compatibility (`border--*` gives `border--0`).
#[must_use]
pub fn is_valid_functional_utility_name(name: &str) -> bool {
    let Some(root) = name.strip_suffix(FUNCTIONAL_SUFFIX) else {
        return false;
    };
    utility_root_len(root) == Some(root.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::{VartaSupports, canonicalize};
    use crate::css_parser;
    use crate::printer::to_css;
    use crate::stylesheet::{candidates_to_css, default_design_system};

    const REFERENCE: u8 = options::REFERENCE;
    const INLINE: u8 = options::INLINE;
    const NONE: u8 = options::NONE;

    /// A design system with the default theme, `theme` values added and
    /// every top-level `@utility` of `css` registered in order.
    fn design_system(css: &str, theme: &[(&str, &str, u8)]) -> DesignSystem {
        let mut ds = default_design_system();
        for (key, value, opts) in theme {
            ds.theme.add(key, value, *opts).expect("valid theme value");
        }
        for node in css_parser::parse(css).expect("valid CSS") {
            if let Node::AtRule { name, .. } = &node
                && name == UTILITY_AT_RULE
            {
                let register = create_css_utility(&node).expect("valid utility name");
                register(&mut ds);
            }
        }
        ds
    }

    fn normalize(css: &str) -> String {
        let ast = css_parser::parse(css).expect("valid CSS");
        to_css(&canonicalize(ast, &VartaSupports).expect("canonical CSS"))
    }

    /// Compile each class and compare with the expected CSS (`None`: no CSS).
    fn check(ds: &mut DesignSystem, cases: &[(&str, Option<&str>)]) {
        let classes: Vec<&str> = cases.iter().map(|(class, _)| *class).collect();
        let actual = candidates_to_css(ds, &classes);
        for ((class, expected), actual) in cases.iter().zip(actual) {
            assert_eq!(
                actual.as_deref().map(normalize),
                expected.map(normalize),
                "class `{class}`"
            );
        }
    }

    fn run(css: &str, theme: &[(&str, &str, u8)], cases: &[(&str, Option<&str>)]) {
        let mut ds = design_system(css, theme);
        check(&mut ds, cases);
    }

    #[test]
    fn valid_static_utility_names() {
        for (name, valid) in [
            ("foo", true),
            ("foo-123", true),
            ("foo-2.5", true),
            ("-foo", true),
            ("foo-bar", true),
            ("foo_bar", true),
            ("foo-50%", true),
            ("foo-1/2", true),
            ("foo-sm/8", true),
            ("foo-4/snug", true),
            ("foo_", true),
            ("foo/bar", true),
            ("Foo", false),
            ("-Foo", false),
            ("foo-", false),
            ("foo-1/", false),
            ("foo-p%", false),
            ("foo.bar", false),
            ("foo-1..5", false),
            ("foo..bar", false),
            ("foo/bar/baz", false),
            ("💨", false),
            ("", false),
        ] {
            assert_eq!(is_valid_static_utility_name(name), valid, "`{name}`");
        }
    }

    #[test]
    fn valid_functional_utility_names() {
        for (name, valid) in [
            ("foo", false),
            ("foo-*", true),
            ("foo--*", true),
            ("-foo-*", true),
            ("foo-bar-*", true),
            ("foo_bar-*", true),
            ("Foo-*", false),
            ("-Foo-*", false),
            ("foo!-*", false),
            ("foo-[…]", false),
            ("-*", false),
            ("my-*-utility", false),
        ] {
            assert_eq!(is_valid_functional_utility_name(name), valid, "`{name}`");
        }
    }

    #[test]
    fn invalid_names_create_no_utility() {
        for css in [
            "@utility push-| { right: 100%; }",
            "@utility ~push { right: 100%; }",
            "@utility foo* { color: red; }",
            "@utility my-*-utility { color: red; }",
            "@utility ui/button/sm { display: inline-flex; }",
        ] {
            let node = css_parser::parse(css).expect("valid CSS").remove(0);
            assert!(create_css_utility(&node).is_none(), "{css}");
        }
        assert!(create_css_utility(&Node::decl("color", "red")).is_none());
    }

    #[test]
    fn argument_normalization() {
        for (input, expected) in [
            ("--value(--spacing)", "--value(--spacing-*)"),
            ("--value(--spacing- *)", "--value(--spacing-*)"),
            (
                "--value(--text- * --line-height)",
                "--value(--text-*--line-height)",
            ),
            (
                "--value(--text --line-height)",
                "--value(--text-*--line-height)",
            ),
            (
                "--value(--text-\\* --line-height)",
                "--value(--text-*--line-height)",
            ),
            ("--value([ *])", "--value([*])"),
            (
                "--value(integer, --default(4))",
                "--value(integer,--default(4))",
            ),
            (
                "--modifier(--a, --default(box alphabetic))",
                "--modifier(--a-*,--default(box alphabetic))",
            ),
            ("calc(--value(number) * 1%)", "calc(--value(number) * 1%)"),
        ] {
            assert_eq!(normalize_declaration_value(input), expected, "{input}");
        }
    }

    #[test]
    fn custom_static_utility() {
        run(
            "@utility text-trim { text-box-trim: both; text-box-edge: cap alphabetic; }",
            &[],
            &[(
                "text-trim",
                Some(".text-trim { text-box-trim: both; text-box-edge: cap alphabetic; }"),
            )],
        );
    }

    #[test]
    fn custom_static_utility_negative() {
        run(
            "@utility -example { value: -1; }",
            &[],
            &[("-example", Some(".-example { value: -1; }"))],
        );
    }

    #[test]
    fn multiple_static_utilities_are_merged() {
        run(
            "@utility really-round { --custom-prop: hi; border-radius: 50rem; }
             @utility really-round { border-radius: 30rem; }",
            &[],
            &[(
                "really-round",
                // Both definitions compile; the optimizer merges equal rules.
                Some(
                    ".really-round { --custom-prop: hi; border-radius: 50rem; border-radius: 30rem; }",
                ),
            )],
        );
    }

    #[test]
    fn static_utilities_with_special_characters_and_escapes() {
        run(
            "@utility push-1/2 { right: 50%; }
             @utility push-50\\% { right: 50%; }
             @utility ui/button { display: inline-flex; }",
            &[],
            &[
                ("push-1/2", Some(".push-1\\/2 { right: 50%; }")),
                ("push-50%", Some(".push-50\\% { right: 50%; }")),
                ("ui/button", Some(".ui\\/button { display: inline-flex; }")),
            ],
        );
    }

    #[test]
    fn static_utility_with_nested_rules() {
        run(
            "@utility bar { &:before { content: 'bar'; } }",
            &[],
            &[("bar", Some(".bar:before { content: 'bar'; }"))],
        );
    }

    #[test]
    fn functional_utilities_require_a_value_function() {
        run(
            "@utility example-* { --resolved-value: 4; }",
            &[],
            &[("example", None), ("example-foo", None)],
        );
    }

    #[test]
    fn functional_utilities_must_resolve_a_value() {
        run(
            "@utility example-* { --resolved-value: --value(integer); }",
            &[],
            &[
                ("example-1", Some(".example-1 { --resolved-value: 1; }")),
                ("example-2", Some(".example-2 { --resolved-value: 2; }")),
                ("example", None),
                ("example-foo", None),
                ("example-2.5", None),
            ],
        );
    }

    const EXAMPLE_THEME: &[(&str, &str, u8)] = &[
        ("--example-1", "1", REFERENCE),
        ("--example-2", "2", REFERENCE),
        ("--example-4", "4", REFERENCE),
        ("--example-a", "8", REFERENCE),
    ];

    fn theme_cases() -> Vec<(&'static str, Option<&'static str>)> {
        vec![
            (
                "example-1",
                Some(".example-1 { --resolved-value: var(--example-1, 1); }"),
            ),
            (
                "example-2",
                Some(".example-2 { --resolved-value: var(--example-2, 2); }"),
            ),
            (
                "example-4",
                Some(".example-4 { --resolved-value: var(--example-4, 4); }"),
            ),
            (
                "example-a",
                Some(".example-a { --resolved-value: var(--example-a, 8); }"),
            ),
            ("example-3", None),
            ("example-gitlab", None),
        ]
    }

    #[test]
    fn resolving_values_from_theme() {
        for spelling in ["--example", "--example-*", "--example-\\*"] {
            run(
                &format!("@utility example-* {{ --resolved-value: --value({spelling}); }}"),
                EXAMPLE_THEME,
                &theme_cases(),
            );
        }
    }

    #[test]
    fn functional_utility_with_double_dash_separator() {
        run(
            "@utility border--* { border-color: --value(--color-border-*, [color]); }",
            &[
                ("--color-border-0", "#e5e7eb", REFERENCE),
                ("--color-border-1", "#d1d5db", REFERENCE),
            ],
            &[
                (
                    "border--0",
                    Some(".border--0 { border-color: var(--color-border-0, #e5e7eb); }"),
                ),
                (
                    "border--1",
                    Some(".border--1 { border-color: var(--color-border-1, #d1d5db); }"),
                ),
                ("border--3", None),
            ],
        );
    }

    #[test]
    fn resolving_bare_values() {
        run(
            "@utility example-* { --resolved-value: --value(integer); }",
            &[],
            &[
                ("example-1", Some(".example-1 { --resolved-value: 1; }")),
                ("example-76", Some(".example-76 { --resolved-value: 76; }")),
                (
                    "example-971",
                    Some(".example-971 { --resolved-value: 971; }"),
                ),
                ("example-foo", None),
            ],
        );
    }

    #[test]
    fn unsupported_bare_data_types_never_resolve() {
        run(
            "@utility paint-* { paint: --value([color], color); }",
            &[],
            &[("paint-#0088cc", None), ("paint-red", None)],
        );
        run(
            "@utility example-* { --resolved-value: --value(color); }",
            &[],
            &[("example-#0088cc", None), ("example-foo", None)],
        );
    }

    #[test]
    fn resolve_literal_values() {
        run(
            "@utility example-* { --resolved-value: --value('revert'); }",
            &[],
            &[
                (
                    "example-revert",
                    Some(".example-revert { --resolved-value: revert; }"),
                ),
                ("example-initial", None),
            ],
        );
    }

    #[test]
    fn bare_values_with_constraints() {
        run(
            "@utility example-* {
               --value-as-number: --value(number);
               --value-as-percentage: --value(percentage);
               --value-as-ratio: --value(ratio);
             }",
            &[],
            &[
                ("example-1", Some(".example-1 { --value-as-number: 1; }")),
                (
                    "example-0.5",
                    Some(".example-0\\.5 { --value-as-number: 0.5; }"),
                ),
                (
                    "example-20%",
                    Some(".example-20\\% { --value-as-percentage: 20%; }"),
                ),
                (
                    "example-2/3",
                    Some(".example-2\\/3 { --value-as-ratio: 2 / 3; }"),
                ),
                ("example-1.23", None),
                ("example-12.34%", None),
                ("example-1.2/3", None),
                ("example-1/2.3", None),
                ("example-1.2/3.4", None),
            ],
        );
    }

    #[test]
    fn resolving_arbitrary_values() {
        run(
            "@utility example-* { --resolved-value: --value([integer]); }",
            &[],
            &[
                (
                    "example-[1]",
                    Some(".example-\\[1\\] { --resolved-value: 1; }"),
                ),
                (
                    "example-[971]",
                    Some(".example-\\[971\\] { --resolved-value: 971; }"),
                ),
                (
                    "example-[integer:var(--my-value)]",
                    Some(
                        ".example-\\[integer\\:var\\(--my-value\\)\\] { --resolved-value: var(--my-value); }",
                    ),
                ),
                ("example-[#0088cc]", None),
                ("example-[1px]", None),
                ("example-[var(--my-value)]", None),
                ("example-(--my-value)", None),
                ("example-[color:var(--my-value)]", None),
                ("example-(color:--my-value)", None),
            ],
        );
    }

    #[test]
    fn resolving_any_arbitrary_values() {
        for spelling in ["[*]", "[ *]", "[\\*]"] {
            run(
                &format!("@utility example-* {{ --resolved-value: --value({spelling}); }}"),
                &[],
                &[
                    (
                        "example-[1]",
                        Some(".example-\\[1\\] { --resolved-value: 1; }"),
                    ),
                    (
                        "example-[var(--my-value)]",
                        Some(
                            ".example-\\[var\\(--my-value\\)\\] { --resolved-value: var(--my-value); }",
                        ),
                    ),
                    (
                        "example-(--my-value)",
                        Some(".example-\\(--my-value\\) { --resolved-value: var(--my-value); }"),
                    ),
                ],
            );
        }
    }

    #[test]
    fn theme_bare_and_arbitrary_values_at_once() {
        run(
            "@utility example-* {
               --resolved-value: --value([integer]);
               --resolved-value: --value(integer);
               --resolved-value: --value(--example);
             }",
            &[("--example-a", "8", REFERENCE)],
            &[
                (
                    "example-a",
                    Some(".example-a { --resolved-value: var(--example-a, 8); }"),
                ),
                ("example-76", Some(".example-76 { --resolved-value: 76; }")),
                (
                    "example-[123]",
                    Some(".example-\\[123\\] { --resolved-value: 123; }"),
                ),
                ("example-[#0088cc]", None),
                ("example-[1px]", None),
            ],
        );
    }

    #[test]
    fn calc_and_shorthand_values() {
        let theme = &[("--example-full", "100%", REFERENCE)];
        run(
            "@utility example-* {
               --resolved-value: --value([percentage]);
               --resolved-value: calc(--value(integer) * 1%);
               --resolved-value: --value(--example);
             }",
            theme,
            &[
                (
                    "example-12",
                    Some(".example-12 { --resolved-value: calc(12 * 1%); }"),
                ),
                (
                    "example-[20%]",
                    Some(".example-\\[20\\%\\] { --resolved-value: 20%; }"),
                ),
                (
                    "example-full",
                    Some(".example-full { --resolved-value: var(--example-full, 100%); }"),
                ),
                ("example-half", None),
                ("example-[#0088cc]", None),
            ],
        );
        run(
            "@utility example-* {
               --resolved-value: calc(--value(integer) * 1%);
               --resolved-value: --value(--example, [percentage]);
             }",
            theme,
            &[
                (
                    "example-37",
                    Some(".example-37 { --resolved-value: calc(37 * 1%); }"),
                ),
                (
                    "example-[50%]",
                    Some(".example-\\[50\\%\\] { --resolved-value: 50%; }"),
                ),
                (
                    "example-full",
                    Some(".example-full { --resolved-value: var(--example-full, 100%); }"),
                ),
                ("example-foo", None),
                ("example-[13px]", None),
            ],
        );
    }

    #[test]
    fn negative_values() {
        run(
            "@utility example-* { --resolved-value: --value(--example, [percentage], [length]); }
             @utility -example-* {
               --resolved-value: calc(--value(--example, [percentage], [length]) * -1);
             }",
            &[("--example-full", "100%", REFERENCE)],
            &[
                (
                    "-example-[10px]",
                    Some(".-example-\\[10px\\] { --resolved-value: calc(10px * -1); }"),
                ),
                (
                    "-example-full",
                    Some(
                        ".-example-full { --resolved-value: calc(var(--example-full, 100%) * -1); }",
                    ),
                ),
                (
                    "example-[20%]",
                    Some(".example-\\[20\\%\\] { --resolved-value: 20%; }"),
                ),
                ("example-10", None),
            ],
        );
    }

    #[test]
    fn spacing_shorthand() {
        run(
            "@utility example-* { margin: --spacing(--value(number)); }",
            &[],
            &[
                (
                    "example-12",
                    Some(".example-12 { margin: calc(var(--spacing) * 12); }"),
                ),
                // Substituted when the utility is registered, before the value
                // is known: no `0px`/multiplier shortcut.
                (
                    "example-0",
                    Some(".example-0 { margin: calc(var(--spacing) * 0); }"),
                ),
            ],
        );
        run(
            "@utility example-* { margin: --spacing(--value(number)); }",
            &[("--spacing", "4px", INLINE | REFERENCE)],
            &[(
                "example-12",
                Some(".example-12 { margin: calc(4px * 12); }"),
            )],
        );
    }

    #[test]
    fn default_values() {
        run(
            "@utility example-* { --resolved-value: calc(--value(integer, --default(4)) * 2); }",
            &[],
            &[
                (
                    "example",
                    Some(".example { --resolved-value: calc(4 * 2); }"),
                ),
                (
                    "example-123",
                    Some(".example-123 { --resolved-value: calc(123 * 2); }"),
                ),
                ("example-foo", None),
            ],
        );
        run(
            "@utility example-* {
               --resolved-value: --value(integer, --default(4));
               --resolved-modifier: --modifier(integer);
             }",
            &[],
            &[
                ("example", Some(".example { --resolved-value: 4; }")),
                (
                    "example/25",
                    Some(".example\\/25 { --resolved-value: 4; --resolved-modifier: 25; }"),
                ),
                ("example/foo", None),
            ],
        );
    }

    #[test]
    fn default_values_in_modifiers() {
        run(
            "@utility example-* {
               --resolved-value: --value(integer, --default(12));
               --resolved-modifier: --modifier(integer, --default(34));
             }",
            &[],
            &[
                (
                    "example",
                    Some(".example { --resolved-value: 12; --resolved-modifier: 34; }"),
                ),
                (
                    "example-1",
                    Some(".example-1 { --resolved-value: 1; --resolved-modifier: 34; }"),
                ),
                (
                    "example-1/1",
                    Some(".example-1\\/1 { --resolved-value: 1; --resolved-modifier: 1; }"),
                ),
                (
                    "example/1",
                    Some(".example\\/1 { --resolved-value: 12; --resolved-modifier: 1; }"),
                ),
                ("example-123/foo", None),
            ],
        );
    }

    #[test]
    fn default_value_is_emitted_as_is() {
        run(
            "@utility trim-* {
               text-box: --value(--text-box-trim-*)
                 --modifier(--text-box-edge-*, --default(box alphabetic));
             }",
            &[
                ("--text-box-trim-trim-both", "trim-both", REFERENCE),
                ("--text-box-edge-cap", "cap alphabetic", REFERENCE),
            ],
            &[
                (
                    "trim-trim-both",
                    Some(
                        ".trim-trim-both { text-box: var(--text-box-trim-trim-both, trim-both) box alphabetic; }",
                    ),
                ),
                (
                    "trim-trim-both/cap",
                    Some(
                        ".trim-trim-both\\/cap { text-box: var(--text-box-trim-trim-both, trim-both) var(--text-box-edge-cap, cap alphabetic); }",
                    ),
                ),
            ],
        );
    }

    #[test]
    fn modifiers() {
        run(
            "@utility example-* {
               --resolved-value: --value(--value, [length]);
               --resolved-modifier: --modifier(--modifier, [length]);
               --resolved-modifier-with-calc: calc(--modifier(--modifier, [length]) * 2);
               --resolved-modifier-literals: --modifier('literal', 'literal-2');
             }",
            &[
                ("--value-sm", "14px", REFERENCE),
                ("--modifier-7", "28px", REFERENCE),
            ],
            &[
                (
                    "example-[12px]/[16px]",
                    Some(
                        ".example-\\[12px\\]\\/\\[16px\\] {
                           --resolved-value: 12px;
                           --resolved-modifier: 16px;
                           --resolved-modifier-with-calc: calc(16px * 2);
                         }",
                    ),
                ),
                (
                    "example-sm/7",
                    Some(
                        ".example-sm\\/7 {
                           --resolved-value: var(--value-sm, 14px);
                           --resolved-modifier: var(--modifier-7, 28px);
                           --resolved-modifier-with-calc: calc(var(--modifier-7, 28px) * 2);
                         }",
                    ),
                ),
                (
                    "example-sm/literal",
                    Some(
                        ".example-sm\\/literal {
                           --resolved-value: var(--value-sm, 14px);
                           --resolved-modifier-literals: literal;
                         }",
                    ),
                ),
                (
                    "example-sm/literal-2",
                    Some(
                        ".example-sm\\/literal-2 {
                           --resolved-value: var(--value-sm, 14px);
                           --resolved-modifier-literals: literal-2;
                         }",
                    ),
                ),
                (
                    "example-[12px]",
                    Some(".example-\\[12px\\] { --resolved-value: 12px; }"),
                ),
                (
                    "example-sm",
                    Some(".example-sm { --resolved-value: var(--value-sm, 14px); }"),
                ),
                ("example-foo", None),
                ("example-foo/[12px]", None),
                ("example-foo/12", None),
                ("example-sm/unknown-literal", None),
            ],
        );
    }

    #[test]
    fn fractions() {
        run(
            "@utility example-* { --resolved-value: --value(--example, ratio, [ratio]); }",
            &[("--example-video", "16 / 9", REFERENCE)],
            &[
                (
                    "example-1/1",
                    Some(".example-1\\/1 { --resolved-value: 1 / 1; }"),
                ),
                (
                    "example-[7/9]",
                    Some(".example-\\[7\\/9\\] { --resolved-value: 7/9; }"),
                ),
                (
                    "example-video",
                    Some(".example-video { --resolved-value: var(--example-video, 16 / 9); }"),
                ),
                ("example-foo", None),
            ],
        );
    }

    #[test]
    fn ratio_drops_other_value_declarations() {
        run(
            "@utility aspect-x-* {
               --number: --value(integer);
               --ratio: --value(ratio);
             }",
            &[],
            &[
                (
                    "aspect-x-16/9",
                    Some(".aspect-x-16\\/9 { --ratio: 16 / 9; }"),
                ),
                ("aspect-x-4", Some(".aspect-x-4 { --number: 4; }")),
            ],
        );
    }

    #[test]
    fn sub_namespaces() {
        let theme = &[
            ("--text-xs", "0.75rem", REFERENCE),
            ("--text-xs--line-height", "calc(1 / 0.75)", REFERENCE),
        ];
        for spelling in [
            "--text-* --line-height",
            "--text-\\* --line-height",
            "--text --line-height",
            "--text-*--line-height",
        ] {
            run(
                &format!(
                    "@utility example-* {{
                       font-size: --value(--text);
                       line-height: --value({spelling});
                       line-height: --modifier(number);
                     }}"
                ),
                theme,
                &[
                    (
                        "example-xs/6",
                        Some(
                            ".example-xs\\/6 {
                               font-size: var(--text-xs, 0.75rem);
                               line-height: var(--text-xs--line-height, calc(1 / 0.75));
                               line-height: 6;
                             }",
                        ),
                    ),
                    (
                        "example-xs",
                        Some(
                            ".example-xs {
                               font-size: var(--text-xs, 0.75rem);
                               line-height: var(--text-xs--line-height, calc(1 / 0.75));
                             }",
                        ),
                    ),
                    ("example-foo", None),
                    ("example-xs/foo", None),
                ],
            );
        }
    }

    #[test]
    fn sub_namespaces_in_different_theme_blocks() {
        let css = "@utility example-* {
                     font-size: --value(--text);
                     line-height: --value(--text-*--line-height);
                   }";
        run(
            css,
            &[
                ("--text-xs", "0.75rem", REFERENCE),
                (
                    "--text-xs--line-height",
                    "calc(1 / 0.75)",
                    INLINE | REFERENCE,
                ),
            ],
            &[(
                "example-xs",
                Some(
                    ".example-xs { font-size: var(--text-xs, 0.75rem); line-height: calc(1 / 0.75); }",
                ),
            )],
        );
        run(
            css,
            &[
                ("--text-xs", "0.75rem", INLINE | REFERENCE),
                ("--text-xs--line-height", "calc(1 / 0.75)", REFERENCE),
            ],
            &[(
                "example-xs",
                Some(
                    ".example-xs { font-size: 0.75rem; line-height: var(--text-xs--line-height, calc(1 / 0.75)); }",
                ),
            )],
        );
    }

    #[test]
    fn theme_options_decide_the_resolved_value() {
        let css = "@utility example-* { --resolved-value: --value(--example); }";
        for (opts, expected) in [
            (NONE, "var(--example-a)"),
            (REFERENCE, "var(--example-a, 8)"),
            (INLINE, "8"),
            (INLINE | REFERENCE, "8"),
        ] {
            run(
                css,
                &[("--example-a", "8", opts)],
                &[(
                    "example-a",
                    Some(&format!(".example-a {{ --resolved-value: {expected}; }}")),
                )],
            );
        }
    }

    #[test]
    fn declarations_are_removed_once() {
        run(
            "@utility fade-r-* {
               --mask-right: linear-gradient(to left, transparent, black --value(percentage));
               --mask-right: linear-gradient(
                 to left,
                 transparent calc(var(--spacing) * --modifier(integer)),
                 black calc(var(--spacing) * --value(integer))
               );
               mask-image: var(--mask-linear), var(--mask-radial), var(--mask-conic);
             }",
            &[],
            &[(
                "fade-r-20%",
                Some(
                    ".fade-r-20\\% {
                       --mask-right: linear-gradient(to left, transparent, black 20%);
                       mask-image: var(--mask-linear), var(--mask-radial), var(--mask-conic);
                     }",
                ),
            )],
        );
    }

    #[test]
    fn same_value_used_multiple_times() {
        run(
            "@utility example-* {
               --resolved-value: calc(var(--spacing) * --value(number)) calc(var(--spacing) * --value(number));
             }",
            &[],
            &[(
                "example-12",
                Some(".example-12 { --resolved-value: calc(var(--spacing) * 12) calc(var(--spacing) * 12); }"),
            )],
        );
    }

    #[test]
    fn multiple_definitions_with_different_value_types() {
        run(
            "@utility foo-* { color: --value(--color-*); }
             @utility foo-* { font-size: --spacing(--value(number)); }",
            &[("--color-red-500", "#ef4444", NONE)],
            &[
                (
                    "foo-red-500",
                    Some(".foo-red-500 { color: var(--color-red-500); }"),
                ),
                (
                    "foo-123",
                    Some(".foo-123 { font-size: calc(var(--spacing) * 123); }"),
                ),
            ],
        );
    }

    #[test]
    fn nested_rules_resolve_values() {
        run(
            "@utility bg-x-* {
               background-color: --value(--color-*);
               & > * { --nearest-bg: --value(--color-*); }
               &:hover { --hover-bg: --value(--color-*); }
             }",
            &[("--color-red-500", "#ef4444", REFERENCE)],
            &[
                (
                    "bg-x-red-500",
                    Some(
                        ".bg-x-red-500 { background-color: var(--color-red-500, #ef4444); }
                         .bg-x-red-500 > * { --nearest-bg: var(--color-red-500, #ef4444); }
                         .bg-x-red-500:hover { --hover-bg: var(--color-red-500, #ef4444); }",
                    ),
                ),
                ("bg-x-blue-1", None),
            ],
        );
    }

    #[test]
    fn at_variant_inside_utility() {
        let mut ds = default_design_system();
        ds.variants.from_ast(
            "hocus",
            vec![Node::rule(
                "&:hover, &:focus",
                vec![Node::at_rule("@slot", "", Vec::new())],
            )],
        );
        for node in css_parser::parse(
            "@utility tabby-* { tab-size: --value(integer); @variant hocus { tab-size: calc(--value(integer) * 2); } }
             @utility glow { color: red; @variant hocus { color: blue; } }",
        )
        .expect("valid CSS")
        {
            create_css_utility(&node).expect("valid name")(&mut ds);
        }
        check(
            &mut ds,
            &[
                (
                    "tabby-4",
                    Some(
                        ".tabby-4 { tab-size: 4; } .tabby-4:hover, .tabby-4:focus { tab-size: calc(4 * 2); }",
                    ),
                ),
                (
                    "glow",
                    Some(".glow { color: red; } .glow:hover, .glow:focus { color: blue; }"),
                ),
            ],
        );
    }

    #[test]
    fn variables_in_static_utility_are_kept() {
        run(
            "@utility fancy-text { font-size: var(--text-4xl); font-weight: var(--font-weight-bold); }",
            &[],
            &[(
                "fancy-text",
                Some(
                    ".fancy-text { font-size: var(--text-4xl); font-weight: var(--font-weight-bold); }",
                ),
            )],
        );
    }
}
