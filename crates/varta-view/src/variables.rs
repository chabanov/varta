//! Computed custom-property scopes. References use arena indices, so neither
//! dependency depth nor dropping a long alias chain consumes the native stack.
use std::collections::BTreeMap;
use varta_common::view::{DRAFT_MAX_VALUE_BYTES, css};

#[derive(Default)]
pub(crate) struct Scope<'a> {
    values: BTreeMap<&'a str, Option<usize>>,
    nodes: Vec<Value<'a>>,
}
#[derive(Clone)]
enum Piece<'a> {
    Text(&'a str),
    Reference(usize),
}
struct Value<'a> {
    pieces: Vec<Piece<'a>>,
    units: usize,
    first: Option<char>,
    last: Option<char>,
    token: Token,
}
#[derive(Clone, Copy)]
enum Token {
    Blank,
    Initial,
    Inherited,
    Other,
}
impl Token {
    fn text(text: &str) -> Self {
        let text = text.trim();
        if text.is_empty() {
            Self::Blank
        } else if text.eq_ignore_ascii_case("initial") {
            Self::Initial
        } else if varta_common::view::css::CSS_WIDE_KEYWORDS
            .iter()
            .any(|word| text.eq_ignore_ascii_case(word))
        {
            Self::Inherited
        } else {
            Self::Other
        }
    }
    fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Blank, other) => other,
            (this, Self::Blank) => this,
            _ => Self::Other,
        }
    }
}
#[derive(Default)]
struct Template<'a> {
    parts: Vec<Part<'a>>,
}
enum Part<'a> {
    Text(&'a str),
    Variable {
        name: &'a str,
        fallback: Option<Template<'a>>,
    },
}
fn word(c: char) -> bool {
    c.is_alphanumeric() || "_-%.#".contains(c)
}
impl<'a> Value<'a> {
    fn new() -> Self {
        Self {
            pieces: Vec::new(),
            units: 0,
            first: None,
            last: None,
            token: Token::Blank,
        }
    }
    fn append(
        &mut self,
        pieces: impl IntoIterator<Item = Piece<'a>>,
        units: usize,
        first: Option<char>,
        last: Option<char>,
        token: Token,
    ) -> Option<()> {
        // Keep empty as a valid computed value, but do not retain its graph
        // edges. Repeatedly doubling an empty reference otherwise produces
        // exponential traversal with zero output, bypassing the size ceiling.
        if units == 0 {
            return Some(());
        }
        // Substitution preserves component-token boundaries: `var(--n)px`
        // must never turn a number token and an identifier into a dimension.
        let separator = self
            .last
            .zip(first)
            .is_some_and(|(a, b)| word(a) && word(b));
        let next = self
            .units
            .checked_add(units)?
            .checked_add(usize::from(separator))?;
        if next > DRAFT_MAX_VALUE_BYTES / 2 {
            return None;
        }
        if separator {
            self.pieces.push(Piece::Text(" "));
        }
        self.pieces.extend(pieces);
        self.units = next;
        self.token = self.token.join(token);
        if self.first.is_none() {
            self.first = first;
        }
        if last.is_some() {
            self.last = last;
        }
        Some(())
    }
    fn text(&mut self, text: &'a str) -> Option<()> {
        self.append(
            [Piece::Text(text)],
            text.encode_utf16().count(),
            text.chars().next(),
            text.chars().next_back(),
            Token::text(text),
        )
    }
}
impl<'a> Template<'a> {
    fn parse(text: &'a str) -> Option<Self> {
        Self::parse_at(text, 0)
    }
    fn parse_at(text: &'a str, depth: usize) -> Option<Self> {
        if depth > css::MAX_VALUE_TOKENS {
            return None;
        }
        let mut out = Self::default();
        let mut at = 0;
        let mut start = 0;
        while at < text.len() {
            let c = text[at..].chars().next()?;
            if c == '"' {
                at += 1;
                at += text[at..].find('"')? + 1;
                continue;
            }
            if word(c) {
                let begin = at;
                at += c.len_utf8();
                while at < text.len() && text[at..].chars().next().is_some_and(word) {
                    at += text[at..].chars().next()?.len_utf8();
                }
                if text[begin..at].eq_ignore_ascii_case("var")
                    && text.as_bytes().get(at) == Some(&b'(')
                {
                    let open = at;
                    let mut level = 1usize;
                    let mut comma = None;
                    at += 1;
                    while level > 0 {
                        let c = text[at..].chars().next()?;
                        if c == '"' {
                            at += 1;
                            at += text[at..].find('"')? + 1;
                            continue;
                        }
                        match c {
                            '(' => level += 1,
                            ')' => level -= 1,
                            ',' if level == 1 && comma.is_none() => comma = Some(at),
                            _ => {}
                        }
                        if level == 0 {
                            break;
                        }
                        at += c.len_utf8();
                    }
                    let name = text[open + 1..comma.unwrap_or(at)]
                        .trim()
                        .strip_prefix("--")?;
                    if !css::ident(name) {
                        return None;
                    }
                    let fallback = if let Some(comma) = comma {
                        let value = text[comma + 1..at].trim();
                        Some(Self::parse_at(value, depth + 1)?)
                    } else {
                        None
                    };
                    if start < begin {
                        out.parts.push(Part::Text(&text[start..begin]));
                    }
                    out.parts.push(Part::Variable { name, fallback });
                    at += 1;
                    start = at;
                }
            } else {
                at += c.len_utf8();
            }
        }
        if start < text.len() {
            out.parts.push(Part::Text(&text[start..]));
        }
        Some(out)
    }
}
impl<'a> Scope<'a> {
    /// Resolve this element's declarations before making them inherited values.
    pub(crate) fn extend(&mut self, raw: BTreeMap<&'a str, &'a str>) {
        if raw.is_empty() {
            return;
        }
        let names: Vec<_> = raw.keys().copied().collect();
        let indices: BTreeMap<_, _> = names
            .iter()
            .enumerate()
            .map(|(i, name)| (*name, i))
            .collect();
        let templates: Vec<_> = raw.values().map(|value| Template::parse(value)).collect();
        enum Work<'t, 'a> {
            Definition(usize),
            FinishDefinition(usize),
            Template(&'t Template<'a>),
            FinishTemplate(usize),
            Part(&'t Part<'a>),
            Variable(Option<usize>, Option<usize>, Option<&'t Template<'a>>),
        }
        let mut resolved = vec![None; names.len()];
        let mut done = vec![false; names.len()];
        let mut positions = vec![None; names.len()];
        let mut cyclic = vec![false; names.len()];
        let mut active = Vec::new();
        for root in 0..names.len() {
            if done[root] {
                continue;
            }
            let mut work = vec![Work::Definition(root)];
            let mut results = Vec::<Option<Value<'a>>>::new();
            while let Some(task) = work.pop() {
                match task {
                    Work::Template(_) | Work::Part(_) | Work::Variable(..)
                        if active.last().is_some_and(|&i| cyclic[i]) =>
                    {
                        results.push(None);
                    }
                    Work::Definition(i) => {
                        let Some(template) = templates[i].as_ref() else {
                            done[i] = true;
                            continue;
                        };
                        positions[i] = Some(active.len());
                        active.push(i);
                        work.push(Work::FinishDefinition(i));
                        work.push(Work::Template(template));
                    }
                    Work::FinishDefinition(i) => {
                        let value = results.pop().flatten();
                        if !cyclic[i]
                            && let Some(value) = value
                        {
                            match value.token {
                                Token::Initial => {}
                                Token::Inherited => {
                                    resolved[i] = self.values.get(names[i]).copied().flatten()
                                }
                                _ => {
                                    if let [Piece::Reference(id)] = value.pieces.as_slice() {
                                        resolved[i] = Some(*id);
                                    } else {
                                        resolved[i] = Some(self.nodes.len());
                                        self.nodes.push(value);
                                    }
                                }
                            }
                        }
                        debug_assert_eq!(active.pop(), Some(i));
                        positions[i] = None;
                        done[i] = true;
                    }
                    Work::Template(template) => {
                        work.push(Work::FinishTemplate(results.len()));
                        work.extend(template.parts.iter().rev().map(Work::Part));
                    }
                    Work::FinishTemplate(start) => {
                        let mut joined = Some(Value::new());
                        // Keep visiting sibling substitutions after an invalid part:
                        // later references can still mark an active context cyclic.
                        for part in results.drain(start..) {
                            joined = match (joined, part) {
                                (Some(mut joined), Some(part)) => joined
                                    .append(
                                        part.pieces,
                                        part.units,
                                        part.first,
                                        part.last,
                                        part.token,
                                    )
                                    .map(|()| joined),
                                _ => None,
                            };
                        }
                        results.push(joined);
                    }
                    Work::Part(Part::Text(text)) => {
                        let mut value = Value::new();
                        results.push(value.text(text).map(|()| value));
                    }
                    Work::Part(Part::Variable { name, fallback }) => {
                        let index = indices.get(name).copied();
                        let inherited = if index.is_none() {
                            self.values.get(name).copied().flatten()
                        } else {
                            None
                        };
                        work.push(Work::Variable(index, inherited, fallback.as_ref()));
                        if let Some(i) = index
                            && !done[i]
                        {
                            if let Some(position) = positions[i] {
                                // A known-invalid cycle is a value failure, not a
                                // new cycle involving an outside fallback consumer.
                                if !cyclic[i] {
                                    for &member in &active[position..] {
                                        cyclic[member] = true;
                                    }
                                }
                            } else {
                                work.push(Work::Definition(i));
                            }
                        }
                    }
                    Work::Variable(index, inherited, fallback) => {
                        let value = index.map_or(inherited, |i| resolved[i]);
                        if let Some(id) = value {
                            results.push(self.reference(id));
                        } else if let Some(fallback) = fallback {
                            work.push(Work::Template(fallback));
                        } else {
                            results.push(None);
                        }
                    }
                }
            }
            debug_assert!(active.is_empty());
            debug_assert!(results.is_empty());
        }
        self.values.extend(names.into_iter().zip(resolved));
    }
    fn reference<'b>(&self, id: usize) -> Option<Value<'b>> {
        let value = &self.nodes[id];
        let mut out = Value::new();
        out.append(
            [Piece::Reference(id)],
            value.units,
            value.first,
            value.last,
            value.token,
        )?;
        Some(out)
    }
    fn evaluate<'b>(
        &self,
        template: &Template<'b>,
        lookup: &impl Fn(&str) -> Option<usize>,
    ) -> Option<Value<'b>> {
        let mut out = Value::new();
        for part in &template.parts {
            match part {
                Part::Text(text) => out.text(text)?,
                Part::Variable { name, fallback } => {
                    if let Some(id) = lookup(name) {
                        let value = &self.nodes[id];
                        out.append(
                            [Piece::Reference(id)],
                            value.units,
                            value.first,
                            value.last,
                            value.token,
                        )?;
                    } else {
                        let value = self.evaluate(fallback.as_ref()?, lookup)?;
                        out.append(
                            value.pieces,
                            value.units,
                            value.first,
                            value.last,
                            value.token,
                        )?;
                    }
                }
            }
        }
        Some(out)
    }
    pub(crate) fn substitute(&self, source: &str) -> Option<String> {
        let template = Template::parse(source)?;
        let value = self.evaluate(&template, &|name| self.values.get(name).copied().flatten())?;
        let mut out = String::new();
        let mut pending: Vec<_> = value.pieces.iter().rev().collect();
        while let Some(piece) = pending.pop() {
            match piece {
                Piece::Text(text) => out.push_str(text),
                Piece::Reference(id) => pending.extend(self.nodes[*id].pieces.iter().rev()),
            }
        }
        if out.len() != out.trim().len() {
            return Some(out.trim().to_owned());
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scope<'a>(definitions: impl IntoIterator<Item = (&'a str, &'a str)>) -> Scope<'a> {
        let mut scope = Scope::default();
        scope.extend(definitions.into_iter().collect());
        scope
    }
    #[test]
    fn aliases_functions_and_nested_fallbacks_resolve_as_components() {
        let scope = scope([
            ("r", "10"),
            ("red", "rgb(var(--r),20,30)"),
            ("alias", "var(--red)"),
            ("tone", "var(--absent,var(--alias))"),
        ]);
        assert_eq!(
            scope.substitute("var(--tone)").as_deref(),
            Some("rgb(10,20,30)")
        );
        assert_eq!(
            scope.substitute("1px solid var(--missing,#111)").as_deref(),
            Some("1px solid #111")
        );
        assert_eq!(scope.substitute("var(--missing)"), None);
    }
    #[test]
    fn computed_inheritance_freezes_parent_aliases_and_invalid_children_mask_them() {
        let mut scope = scope([("base", "#123"), ("alias", "var(--base)")]);
        scope.extend(BTreeMap::from([("base", "#456")]));
        assert_eq!(scope.substitute("var(--alias)").as_deref(), Some("#123"));
        scope.extend(BTreeMap::from([("alias", "var(--missing)")]));
        assert_eq!(
            scope.substitute("var(--alias,#abc)").as_deref(),
            Some("#abc")
        );
        let mut cross = Scope::default();
        cross.extend(BTreeMap::from([("a", "#123")]));
        cross.extend(BTreeMap::from([("b", "var(--a)")]));
        cross.extend(BTreeMap::from([("a", "var(--b)")]));
        assert_eq!(cross.substitute("var(--a)").as_deref(), Some("#123"));
    }
    #[test]
    fn active_cycles_invalidate_members_without_evaluating_unused_fallbacks() {
        let scope = scope([
            ("a", "var(--b,#123)"),
            ("b", "var(--a,#456)"),
            ("c", "var(--a,#abc)"),
            ("safe", "#111"),
            ("self", "var(--safe,var(--self))"),
        ]);
        assert_eq!(scope.substitute("var(--a,#999)").as_deref(), Some("#999"));
        assert_eq!(scope.substitute("var(--b,#999)").as_deref(), Some("#999"));
        assert_eq!(scope.substitute("var(--c)").as_deref(), Some("#abc"));
        assert_eq!(
            scope.substitute("var(--self,#999)").as_deref(),
            Some("#111")
        );
    }
    #[test]
    fn empty_and_wide_fallbacks_are_computed_values_not_missing_bindings() {
        let mut env = scope([("tone", "#123")]);
        env.extend(BTreeMap::from([
            ("tone", "var(--missing,inherit)"),
            ("empty", "var(--missing,)"),
        ]));
        assert_eq!(env.substitute("var(--tone)").as_deref(), Some("#123"));
        assert_eq!(env.substitute("var(--empty,#abc)").as_deref(), Some(""));
        assert_eq!(env.substitute("var(--absent,) 2px").as_deref(), Some("2px"));
        let mut escape = Scope::default();
        escape.extend(BTreeMap::from([
            ("a", "var(--c,#abc)"),
            ("b", "var(--c,var(--a))"),
            ("c", "var(--b)"),
        ]));
        assert_eq!(escape.substitute("var(--a)").as_deref(), Some("#abc"));
        let invalid = scope([("a", "var(--missing) var(--b)"), ("b", "var(--a,#abc)")]);
        assert_eq!(invalid.substitute("var(--b,#789)").as_deref(), Some("#789"));
    }
    #[test]
    fn empty_fragments_preserve_boundaries_keywords_and_quoted_empty_strings() {
        let mut env = scope([("keyword", "#456")]);
        env.extend(BTreeMap::from([
            ("a", "var(--empty)var(--b)"),
            ("b", "var(--a)"),
            ("empty", "var(--missing,)"),
            ("keyword", "var(--empty)InHeRiT"),
            ("n", "10"),
            ("quoted", "\"\""),
        ]));
        assert_eq!(env.substitute("var(--empty,#abc)").as_deref(), Some(""));
        assert_eq!(
            env.substitute("var(--n)var(--empty)px").as_deref(),
            Some("10 px")
        );
        assert_eq!(env.substitute("var(--keyword)").as_deref(), Some("#456"));
        assert_eq!(env.substitute("var(--quoted)").as_deref(), Some("\"\""));
        assert_eq!(env.substitute("var(--absent)"), None);
        assert_eq!(env.substitute("var(--a,#abc)").as_deref(), Some("#abc"));
    }
    #[test]
    fn quoted_text_and_identifier_boundaries_do_not_become_variable_tokens() {
        let scope = scope([
            ("n", "10"),
            ("bad", "var(--n)px"),
            ("literal", "\"var(--missing)\""),
        ]);
        assert_eq!(scope.substitute("var(--bad)").as_deref(), Some("10 px"));
        assert_eq!(
            scope.substitute("var(--literal)").as_deref(),
            Some("\"var(--missing)\"")
        );
        assert_eq!(
            scope.substitute("\"var(--missing)\"").as_deref(),
            Some("\"var(--missing)\"")
        );
        assert_eq!(
            scope.substitute("myvar(--missing)").as_deref(),
            Some("myvar(--missing)")
        );
    }
    #[test]
    fn css_wide_keywords_apply_before_substitution_and_names_are_case_sensitive() {
        let mut scope = scope([("tone", "#123"), ("Tone", "#456")]);
        scope.extend(BTreeMap::from([
            ("tone", "InHeRiT"),
            ("alias", "VAR(--Tone)"),
        ]));
        assert_eq!(scope.substitute("var(--tone)").as_deref(), Some("#123"));
        assert_eq!(scope.substitute("var(--alias)").as_deref(), Some("#456"));
        scope.extend(BTreeMap::from([("tone", "initial")]));
        assert_eq!(
            scope.substitute("var(--tone,#abc)").as_deref(),
            Some("#abc")
        );
    }
    #[test]
    fn expanded_lengths_are_checked_before_growth_and_shared_aliases_do_not_duplicate_text() {
        let literal = "x".repeat(16383);
        let mut scope = scope([
            ("a", literal.as_str()),
            ("b", "var(--a)var(--a)"),
            ("c", "var(--b)/"),
            ("d", "var(--c)/"),
        ]);
        assert_eq!(
            scope.substitute("var(--c)").unwrap().encode_utf16().count(),
            DRAFT_MAX_VALUE_BYTES / 2
        );
        assert_eq!(
            scope.substitute("var(--d,fallback)").as_deref(),
            Some("fallback")
        );
        let names: Vec<_> = (0..20).map(|i| format!("p{i}")).collect();
        let mut values = vec!["x".to_owned()];
        for i in 1..20 {
            values.push(format!("var(--p{})var(--p{})", i - 1, i - 1));
        }
        scope.extend(
            names
                .iter()
                .zip(&values)
                .map(|(a, b)| (a.as_str(), b.as_str()))
                .collect(),
        );
        assert_eq!(
            scope.substitute("var(--p19,fallback)").as_deref(),
            Some("fallback")
        );
        drop(scope);
    }
    #[test]
    fn expanded_limits_count_utf16_units_instead_of_utf8_bytes() {
        let literal = format!("\"{}\"", "😀".repeat(8191));
        let scope = scope([
            ("a", literal.as_str()),
            ("b", "var(--a)var(--a)"),
            ("c", "var(--b)/"),
        ]);
        assert_eq!(
            scope.substitute("var(--b)").unwrap().encode_utf16().count(),
            DRAFT_MAX_VALUE_BYTES / 2
        );
        assert_eq!(
            scope.substitute("var(--c,fallback)").as_deref(),
            Some("fallback")
        );
    }
    #[test]
    fn long_alias_chains_and_their_drop_fit_a_normal_test_thread_stack() {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                let names: Vec<_> = (0..5000).map(|i| format!("v{i:04}")).collect();
                let mut values: Vec<_> =
                    (0..4999).map(|i| format!("var(--v{:04})", i + 1)).collect();
                values.push("#123".to_owned());
                let raw = names
                    .iter()
                    .zip(&values)
                    .map(|(a, b)| (a.as_str(), b.as_str()))
                    .collect();
                let mut scope = Scope::default();
                scope.extend(raw);
                assert_eq!(scope.substitute("var(--v0000)").as_deref(), Some("#123"));
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
