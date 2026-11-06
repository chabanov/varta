//! Media and container query normalization.
//!
//! * Media Queries 4 §3 (syntax): a query list is split at its top-level
//!   commas; each query is `<media-condition>` or
//!   `[not | only]? <media-type> [and <media-condition-without-or>]?`.
//!   Keywords, media types and feature names are ASCII case-insensitive and
//!   are lowercased; whitespace is one space around keywords and
//!   `(name: value)` inside features.
//! * Media Queries 4 §2.4.4 and §4 (range context): for a range-type
//!   feature, `(name >= v)` is `(min-name: v)`, `(name <= v)` is
//!   `(max-name: v)`, `(name < v)` is `not (min-name: v)`, `(name > v)` is
//!   `not (max-name: v)`, `(name = v)` is `(name: v)`; reversed forms swap
//!   the comparison and a double range is the conjunction of its two
//!   comparisons. Containment 3 §6.1 container size features use the same
//!   range syntax.
//! * Logic: `not (not (x))` is `(x)` and nested conjunctions or disjunctions
//!   are flattened; both are exact in the Kleene three-valued logic of
//!   Media Queries 4 §3.2. A `not`, `and` or `or` operand of another
//!   operator is parenthesized to stay grammatical.
//! * Conditional 3 §3: a rule inside `@media A { @media B { … } }` applies
//!   when both conditions hold, so a sole nested `@media` merges into
//!   `@media A and B` when both are single queries without a media type.
//!
//! A query that does not parse is kept with collapsed whitespace.
use super::tokens::{Token, collapse_whitespace, significant, tokenize};
use super::{CanonicalError, MAX_CANONICAL_DEPTH, MAX_CONDITION_DEPTH};
use crate::ast::Node;
use crate::css_parser::js_trim;
use crate::utils::segment;

const AT_MEDIA: &str = "@media";
const AT_CONTAINER: &str = "@container";
const NOT: &str = "not";
const AND: &str = "and";
const OR: &str = "or";
const ONLY: &str = "only";
const MIN_PREFIX: &str = "min-";
const MAX_PREFIX: &str = "max-";
const LIST_SEPARATOR: &str = ", ";

/// Range-type features (Media Queries 4 §4, Containment 3 §6.1), which
/// accept `min-`/`max-` prefixes.
const RANGE_FEATURES: &[&str] = &[
    "width",
    "height",
    "inline-size",
    "block-size",
    "aspect-ratio",
    "device-width",
    "device-height",
    "device-aspect-ratio",
    "resolution",
    "color",
    "color-index",
    "monochrome",
];

/// A comparison of the range context (Media Queries 4 §3: `<mf-lt>`,
/// `<mf-gt>`, `<mf-eq>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Comparison {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
}

impl Comparison {
    fn parse(text: &str) -> Option<Comparison> {
        Some(match text {
            "<" => Comparison::Lt,
            "<=" => Comparison::Le,
            ">" => Comparison::Gt,
            ">=" => Comparison::Ge,
            "=" => Comparison::Eq,
            _ => return None,
        })
    }

    /// `v op name` is `name flipped(op) v`.
    fn flipped(self) -> Comparison {
        match self {
            Comparison::Lt => Comparison::Gt,
            Comparison::Le => Comparison::Ge,
            Comparison::Gt => Comparison::Lt,
            Comparison::Ge => Comparison::Le,
            Comparison::Eq => Comparison::Eq,
        }
    }

    fn text(self) -> &'static str {
        match self {
            Comparison::Lt => "<",
            Comparison::Le => "<=",
            Comparison::Gt => ">",
            Comparison::Ge => ">=",
            Comparison::Eq => "=",
        }
    }

    fn is_less(self) -> bool {
        matches!(self, Comparison::Lt | Comparison::Le)
    }

    fn is_greater(self) -> bool {
        matches!(self, Comparison::Gt | Comparison::Ge)
    }
}

/// A media or container condition.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Condition {
    /// `(name: value)`
    Plain {
        name: String,
        value: String,
    },
    /// `(name)`
    Boolean(String),
    /// A range that has no `min-`/`max-` form, kept as `(text)`.
    Range(String),
    /// Anything else, kept verbatim (functions such as `style(…)`,
    /// `<general-enclosed>`).
    Opaque(String),
    Not(Box<Condition>),
    And(Vec<Condition>),
    Or(Vec<Condition>),
}

fn not(inner: Condition) -> Condition {
    match inner {
        Condition::Not(inner) => *inner,
        inner => Condition::Not(Box::new(inner)),
    }
}

fn and(operands: Vec<Condition>) -> Condition {
    let mut flat = Vec::new();
    for operand in operands {
        match operand {
            Condition::And(inner) => flat.extend(inner),
            operand => flat.push(operand),
        }
    }
    if flat.len() == 1 {
        flat.remove(0)
    } else {
        Condition::And(flat)
    }
}

fn or(operands: Vec<Condition>) -> Condition {
    let mut flat = Vec::new();
    for operand in operands {
        match operand {
            Condition::Or(inner) => flat.extend(inner),
            operand => flat.push(operand),
        }
    }
    if flat.len() == 1 {
        flat.remove(0)
    } else {
        Condition::Or(flat)
    }
}

impl Condition {
    fn print(&self) -> String {
        match self {
            Condition::Plain { name, value } => format!("({name}: {value})"),
            Condition::Boolean(name) => format!("({name})"),
            Condition::Range(text) => format!("({text})"),
            Condition::Opaque(text) => text.clone(),
            Condition::Not(inner) => format!("{NOT} {}", inner.in_parens()),
            Condition::And(operands) => join(operands, AND),
            Condition::Or(operands) => join(operands, OR),
        }
    }

    /// `<media-in-parens>`: operators are parenthesized.
    fn in_parens(&self) -> String {
        match self {
            Condition::Not(_) | Condition::And(_) | Condition::Or(_) => {
                format!("({})", self.print())
            }
            _ => self.print(),
        }
    }

    /// `<media-condition-without-or>`.
    fn print_without_or(&self) -> String {
        match self {
            Condition::Or(_) => self.in_parens(),
            _ => self.print(),
        }
    }
}

fn join(operands: &[Condition], keyword: &str) -> String {
    operands
        .iter()
        .map(Condition::in_parens)
        .collect::<Vec<_>>()
        .join(&format!(" {keyword} "))
}

/// Parse a `<media-condition>` (also a `<container-query>`); `Ok(None)` when
/// it does not match the grammar.
fn parse_condition(
    tokens: &[Token<'_>],
    depth: usize,
) -> Result<Option<Condition>, CanonicalError> {
    let Some((first, rest)) = tokens.split_first() else {
        return Ok(None);
    };
    if let Token::Ident(keyword) = first
        && keyword.eq_ignore_ascii_case(NOT)
    {
        let [operand] = rest else {
            return Ok(None);
        };
        return Ok(in_parens(operand, depth)?.map(not));
    }
    let Some(head) = in_parens(first, depth)? else {
        return Ok(None);
    };
    let Some(Token::Ident(operator)) = rest.first() else {
        return Ok(rest.is_empty().then_some(head));
    };
    let is_and = operator.eq_ignore_ascii_case(AND);
    if !is_and && !operator.eq_ignore_ascii_case(OR) {
        return Ok(None);
    }
    let mut operands = vec![head];
    for pair in rest.chunks(2) {
        let [Token::Ident(keyword), operand] = pair else {
            return Ok(None);
        };
        if !keyword.eq_ignore_ascii_case(operator) {
            return Ok(None);
        }
        let Some(operand) = in_parens(operand, depth)? else {
            return Ok(None);
        };
        operands.push(operand);
    }
    Ok(Some(if is_and { and(operands) } else { or(operands) }))
}

/// Parse a `<media-in-parens>` / `<query-in-parens>`.
fn in_parens(token: &Token<'_>, depth: usize) -> Result<Option<Condition>, CanonicalError> {
    match token {
        Token::Block(inner) => {
            if depth >= MAX_CONDITION_DEPTH {
                return Err(CanonicalError::ConditionTooDeep((*inner).to_owned()));
            }
            let tokens = tokenize(inner).map(|tokens| significant(&tokens));
            if let Some(tokens) = &tokens
                && let Some(condition) = parse_condition(tokens, depth + 1)?
            {
                return Ok(Some(condition));
            }
            Ok(Some(feature(inner)))
        }
        Token::Function(name, inner) => Ok(Some(Condition::Opaque(format!(
            "{}({})",
            name.to_ascii_lowercase(),
            collapse_whitespace(inner)
        )))),
        Token::Space | Token::Ident(_) | Token::Other(_) | Token::Comma => Ok(None),
    }
}

/// Whether `text` is one identifier.
fn is_ident(text: &str) -> bool {
    matches!(tokenize(text).as_deref(), Some([Token::Ident(_)]))
}

/// A `<media-feature>` (Media Queries 4 §3): plain, boolean or range; any
/// other text is a `<general-enclosed>`.
fn feature(inner: &str) -> Condition {
    let text = js_trim(inner);
    if let Some((name, value)) = text.split_once(':') {
        let name = js_trim(name);
        if is_ident(name) {
            return Condition::Plain {
                name: name.to_ascii_lowercase(),
                value: collapse_whitespace(value),
            };
        }
    } else if is_ident(text) {
        return Condition::Boolean(text.to_ascii_lowercase());
    } else if let Some(range) = range(text) {
        return range;
    }
    Condition::Opaque(format!("({})", collapse_whitespace(text)))
}

/// Split a range at its comparisons: operands and operators alternate.
fn range_parts(text: &str) -> Option<(Vec<String>, Vec<Comparison>)> {
    let mut operands = Vec::new();
    let mut comparisons = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            '<' | '>' | '=' if depth == 0 => {
                let mut operator = c.to_string();
                if c != '=' && chars.peek() == Some(&'=') {
                    operator.push('=');
                    chars.next();
                }
                comparisons.push(Comparison::parse(&operator)?);
                operands.push(collapse_whitespace(&current));
                current.clear();
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    operands.push(collapse_whitespace(&current));
    if operands.iter().any(String::is_empty) {
        return None;
    }
    Some((operands, comparisons))
}

/// A range feature (`<mf-range>`), rewritten to `min-`/`max-` features
/// where the feature is a range type.
fn range(text: &str) -> Option<Condition> {
    let (operands, comparisons) = range_parts(text)?;
    match (operands.as_slice(), comparisons.as_slice()) {
        ([left, right], [comparison]) => {
            if is_ident(left) {
                Some(compare(&left.to_ascii_lowercase(), *comparison, right))
            } else if is_ident(right) {
                Some(compare(
                    &right.to_ascii_lowercase(),
                    comparison.flipped(),
                    left,
                ))
            } else {
                None
            }
        }
        ([low, name, high], [first, second])
            if is_ident(name)
                && first.is_less() == second.is_less()
                && first.is_greater() == second.is_greater()
                && *first != Comparison::Eq
                && *second != Comparison::Eq =>
        {
            let name = name.to_ascii_lowercase();
            Some(and(vec![
                compare(&name, first.flipped(), low),
                compare(&name, *second, high),
            ]))
        }
        _ => None,
    }
}

/// `(name comparison value)` in canonical form.
fn compare(name: &str, comparison: Comparison, value: &str) -> Condition {
    let plain = |prefix: &str| Condition::Plain {
        name: format!("{prefix}{name}"),
        value: value.to_owned(),
    };
    if !RANGE_FEATURES.contains(&name) {
        if comparison == Comparison::Eq {
            return plain("");
        }
        return Condition::Range(format!("{name} {} {value}", comparison.text()));
    }
    match comparison {
        Comparison::Ge => plain(MIN_PREFIX),
        Comparison::Le => plain(MAX_PREFIX),
        Comparison::Lt => not(plain(MIN_PREFIX)),
        Comparison::Gt => not(plain(MAX_PREFIX)),
        Comparison::Eq => plain(""),
    }
}

/// One parsed media query.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Query {
    Condition(Condition),
    Typed {
        modifier: Option<String>,
        media_type: String,
        condition: Option<Condition>,
    },
}

impl Query {
    fn print(&self) -> String {
        match self {
            Query::Condition(condition) => condition.print(),
            Query::Typed {
                modifier,
                media_type,
                condition,
            } => {
                let mut text = String::new();
                if let Some(modifier) = modifier {
                    text.push_str(modifier);
                    text.push(' ');
                }
                text.push_str(media_type);
                if let Some(condition) = condition {
                    text.push_str(&format!(" {AND} {}", condition.print_without_or()));
                }
                text
            }
        }
    }
}

/// Parse one `<media-query>`.
fn parse_query(text: &str) -> Result<Option<Query>, CanonicalError> {
    let Some(tokens) = tokenize(text) else {
        return Ok(None);
    };
    let tokens = significant(&tokens);
    let Some((first, rest)) = tokens.split_first() else {
        return Ok(None);
    };
    let Token::Ident(first) = first else {
        return Ok(parse_condition(&tokens, 0)?.map(Query::Condition));
    };
    let keyword = first.to_ascii_lowercase();
    if keyword == NOT && matches!(rest.first(), Some(Token::Block(_))) {
        return Ok(parse_condition(&tokens, 0)?.map(Query::Condition));
    }
    let (modifier, rest) = if keyword == NOT || keyword == ONLY {
        (Some(keyword), rest)
    } else {
        (None, tokens.as_slice())
    };
    let Some((Token::Ident(media_type), rest)) = rest.split_first() else {
        return Ok(None);
    };
    let media_type = media_type.to_ascii_lowercase();
    if [NOT, AND, OR, ONLY].contains(&media_type.as_str()) {
        return Ok(None);
    }
    let condition = match rest.split_first() {
        None => None,
        Some((Token::Ident(keyword), condition)) if keyword.eq_ignore_ascii_case(AND) => {
            // `<media-condition-without-or>`: no top-level `or`.
            if condition
                .iter()
                .any(|token| matches!(token, Token::Ident(word) if word.eq_ignore_ascii_case(OR)))
            {
                return Ok(None);
            }
            match parse_condition(condition, 0)? {
                Some(condition) => Some(condition),
                None => return Ok(None),
            }
        }
        Some(_) => return Ok(None),
    };
    Ok(Some(Query::Typed {
        modifier,
        media_type,
        condition,
    }))
}

/// Normalize a `<media-query-list>`.
fn media_list(params: &str) -> Result<String, CanonicalError> {
    let mut queries = Vec::new();
    for query in segment(params, ',') {
        queries.push(match parse_query(&query)? {
            Some(query) => query.print(),
            None => collapse_whitespace(&query),
        });
    }
    Ok(queries.join(LIST_SEPARATOR))
}

/// Normalize a container prelude: `[<container-name>]? <container-query>`,
/// possibly a list (Containment 3 §6).
fn container_list(params: &str) -> Result<String, CanonicalError> {
    let mut preludes = Vec::new();
    for prelude in segment(params, ',') {
        let normalized = match tokenize(&prelude).map(|tokens| significant(&tokens)) {
            Some(tokens) => {
                let (name, query) = match tokens.split_first() {
                    Some((Token::Ident(name), rest))
                        if ![NOT, AND, OR]
                            .iter()
                            .any(|keyword| name.eq_ignore_ascii_case(keyword)) =>
                    {
                        (Some(*name), rest)
                    }
                    _ => (None, tokens.as_slice()),
                };
                match (name, parse_condition(query, 0)?) {
                    (Some(name), Some(condition)) => Some(format!("{name} {}", condition.print())),
                    (None, Some(condition)) => Some(condition.print()),
                    (Some(name), None) if query.is_empty() => Some(name.to_owned()),
                    _ => None,
                }
            }
            None => None,
        };
        preludes.push(normalized.unwrap_or_else(|| collapse_whitespace(&prelude)));
    }
    Ok(preludes.join(LIST_SEPARATOR))
}

/// The single type-less condition of a normalized `@media` prelude.
fn single_condition(params: &str) -> Result<Option<Condition>, CanonicalError> {
    if segment(params, ',').len() != 1 {
        return Ok(None);
    }
    Ok(match parse_query(params)? {
        Some(Query::Condition(condition)) => Some(condition),
        _ => None,
    })
}

/// Normalize every `@media` and `@container` prelude and merge directly
/// nested single `@media` conditions.
pub(super) fn normalize(nodes: Vec<Node>, depth: usize) -> Result<Vec<Node>, CanonicalError> {
    if depth > MAX_CANONICAL_DEPTH {
        return Err(CanonicalError::TooDeep);
    }
    let mut out = Vec::with_capacity(nodes.len());
    for node in nodes {
        out.push(match node {
            Node::AtRule {
                name,
                params,
                nodes,
            } => {
                let nodes = normalize(nodes, depth + 1)?;
                let params = if name == AT_MEDIA {
                    media_list(&params)?
                } else if name == AT_CONTAINER {
                    container_list(&params)?
                } else {
                    params
                };
                merge_media(name, params, nodes)?
            }
            Node::Rule { selector, nodes } => Node::rule(selector, normalize(nodes, depth + 1)?),
            other => other,
        });
    }
    Ok(out)
}

/// `@media A { @media B { … } }` → `@media A and B` (Conditional 3 §3).
fn merge_media(name: String, params: String, nodes: Vec<Node>) -> Result<Node, CanonicalError> {
    if name != AT_MEDIA {
        return Ok(Node::at_rule(name, params, nodes));
    }
    let [
        Node::AtRule {
            name: inner_name,
            params: inner_params,
            ..
        },
    ] = nodes.as_slice()
    else {
        return Ok(Node::at_rule(name, params, nodes));
    };
    if inner_name != AT_MEDIA {
        return Ok(Node::at_rule(name, params, nodes));
    }
    let (Some(outer), Some(inner)) = (single_condition(&params)?, single_condition(inner_params)?)
    else {
        return Ok(Node::at_rule(name, params, nodes));
    };
    let merged = and(vec![outer, inner]).print();
    let children = match nodes.into_iter().next() {
        Some(Node::AtRule { nodes, .. }) => nodes,
        _ => Vec::new(),
    };
    Ok(Node::at_rule(name, merged, children))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media(params: &str) -> String {
        media_list(params).unwrap()
    }

    #[test]
    fn ranges_become_min_and_max_features() {
        // Media Queries 4 §2.4.4 / §4: range context.
        assert_eq!(media("(width >= 40rem)"), "(min-width: 40rem)");
        assert_eq!(media("(width<=40rem)"), "(max-width: 40rem)");
        assert_eq!(media("(width < 40rem)"), "not (min-width: 40rem)");
        assert_eq!(media("(width > 40rem)"), "not (max-width: 40rem)");
        assert_eq!(media("(width = 40rem)"), "(width: 40rem)");
        assert_eq!(media("(40rem <= width)"), "(min-width: 40rem)");
        assert_eq!(media("(40rem > width)"), "not (min-width: 40rem)");
        assert_eq!(
            media("(aspect-ratio > 16 / 9)"),
            "not (max-aspect-ratio: 16 / 9)"
        );
        assert_eq!(
            media("(40rem <= width < 48rem)"),
            "(min-width: 40rem) and (not (min-width: 48rem))"
        );
        assert_eq!(
            media("(48rem > width >= 40rem)"),
            "(not (min-width: 48rem)) and (min-width: 40rem)"
        );
        // Not a range-type feature: the range stays.
        assert_eq!(media("(hover > 1)"), "(hover > 1)");
    }

    #[test]
    fn logic_and_spacing() {
        // Media Queries 4 §3: grammar; §3.2: three-valued logic.
        assert_eq!(media("not (width < 40rem)"), "(min-width: 40rem)");
        assert_eq!(media("not (not (hover: hover))"), "(hover: hover)");
        assert_eq!(media("(min-width:900px)"), "(min-width: 900px)");
        assert_eq!(
            media("(HOVER:hover)  AND ((x) and (width >= 1px))"),
            "(hover: hover) and (x) and (min-width: 1px)"
        );
        assert_eq!(
            media("(a) or (width < 1px)"),
            "(a) or (not (min-width: 1px))"
        );
        assert_eq!(media("not ((a) or (b))"), "not ((a) or (b))");
        assert_eq!(media("print"), "print");
        assert_eq!(media("NOT  print"), "not print");
        assert_eq!(
            media("only screen and (width >= 1px), print and ((a) or (b))"),
            "only screen and (min-width: 1px), print and ((a) or (b))"
        );
        assert_eq!(
            media("screen and (width < 1px)"),
            "screen and not (min-width: 1px)"
        );
        assert_eq!(
            media("(prefers-color-scheme: dark)"),
            "(prefers-color-scheme: dark)"
        );
        assert_eq!(media("garbage  here"), "garbage here");
    }

    #[test]
    fn container_queries_use_the_same_ranges() {
        // Containment 3 §6.1: size features in range syntax.
        assert_eq!(
            container_list("(width >= 18rem)").unwrap(),
            "(min-width: 18rem)"
        );
        assert_eq!(
            container_list("main (width < 24rem)").unwrap(),
            "main not (min-width: 24rem)"
        );
        assert_eq!(
            container_list("card (inline-size > 30em) and style(--x: 1)").unwrap(),
            "card (not (max-inline-size: 30em)) and style(--x: 1)"
        );
    }

    #[test]
    fn nested_media_merge() {
        // Conditional 3 §3: nested conditions must all hold.
        let inner = Node::at_rule("@media", "(hover: hover)", vec![Node::rule(".a", vec![])]);
        let outer = Node::at_rule("@media", "not (width >= 40rem)", vec![inner.clone()]);
        assert_eq!(
            normalize(vec![outer], 0).unwrap(),
            vec![Node::at_rule(
                "@media",
                "(not (min-width: 40rem)) and (hover: hover)",
                vec![Node::rule(".a", vec![])]
            )]
        );
        let typed = Node::at_rule("@media", "print", vec![inner.clone()]);
        assert_eq!(normalize(vec![typed.clone()], 0).unwrap(), vec![typed]);
    }

    #[test]
    fn normalization_is_idempotent() {
        for query in [
            "(40rem <= width < 48rem)",
            "not (width < 40rem)",
            "(a) or (width < 1px)",
            "screen and (width < 1px)",
            "not ((a) or (b))",
        ] {
            let once = media(query);
            assert_eq!(media(&once), once, "{query}");
        }
    }

    #[test]
    fn deep_conditions_are_rejected() {
        let deep = "(".repeat(MAX_CONDITION_DEPTH + 2) + "a" + &")".repeat(MAX_CONDITION_DEPTH + 2);
        assert!(matches!(
            media_list(&deep),
            Err(CanonicalError::ConditionTooDeep(_))
        ));
    }
}
