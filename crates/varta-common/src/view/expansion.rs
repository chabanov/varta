//! State-independent upper bounds for derived CSS component text. This is an
//! admission estimator, not the renderer's active-cycle substitution algorithm.
use std::collections::{BTreeMap, BTreeSet};

const CAP: usize = super::DRAFT_MAX_VALUE_BYTES / 2;
#[derive(Debug, Clone, PartialEq)]
enum Part {
    Literal {
        units: usize,
        wide: bool,
    },
    Reference {
        name: String,
        fallback: Option<Value>,
    },
}
#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    parts: Vec<Part>,
    references: BTreeSet<String>,
    choice: bool,
    plain_units: usize,
}
fn word(c: char) -> bool {
    c.is_alphanumeric() || "_-%.#".contains(c)
}
impl Value {
    /// Called on admitted source; unknown syntax conservatively costs the cap.
    pub fn new(text: &str) -> Self {
        let units = text.encode_utf16().count();
        let mut value = Self::parse(text, 0).unwrap_or_else(|| Self {
            parts: vec![Part::Literal {
                units: CAP,
                wide: false,
            }],
            references: BTreeSet::new(),
            choice: true,
            plain_units: units.max(CAP),
        });
        value.plain_units = value.plain_units.max(units);
        value
    }
    fn literal(text: &str) -> Part {
        Part::Literal {
            units: text.encode_utf16().count(),
            wide: ["initial", "inherit", "unset", "revert", "revert-layer"]
                .iter()
                .any(|keyword| text.trim().eq_ignore_ascii_case(keyword)),
        }
    }
    fn parse(text: &str, depth: usize) -> Option<Self> {
        if depth > super::css::MAX_VALUE_TOKENS {
            return None;
        }
        let mut out = Self {
            parts: Vec::new(),
            references: BTreeSet::new(),
            choice: true,
            plain_units: 0,
        };
        let (mut at, mut start) = (0, 0);
        while at < text.len() {
            let c = text[at..].chars().next()?;
            if c == '"' {
                at += 1;
                at += text[at..].find('"')? + 1;
                continue;
            }
            if !word(c) {
                at += c.len_utf8();
                continue;
            }
            let begin = at;
            at += c.len_utf8();
            while text[at..].chars().next().is_some_and(word) {
                at += text[at..].chars().next()?.len_utf8();
            }
            if !text[begin..at].eq_ignore_ascii_case("var")
                || text.as_bytes().get(at) != Some(&b'(')
            {
                continue;
            }
            let open = at;
            at += 1;
            let mut nesting = 1usize;
            let mut comma = None;
            while nesting > 0 {
                let c = text[at..].chars().next()?;
                if c == '"' {
                    at += 1;
                    at += text[at..].find('"')? + 1;
                    continue;
                }
                match c {
                    '(' => nesting += 1,
                    ')' => nesting -= 1,
                    ',' if nesting == 1 && comma.is_none() => comma = Some(at),
                    _ => {}
                }
                if nesting > 0 {
                    at += c.len_utf8();
                }
            }
            let name = text[open + 1..comma.unwrap_or(at)]
                .trim()
                .strip_prefix("--")?;
            if !super::css::ident(name) {
                return None;
            }
            let fallback = if let Some(comma) = comma {
                Some(Self::parse(text[comma + 1..at].trim(), depth + 1)?)
            } else {
                None
            };
            if start < begin {
                out.parts.push(Self::literal(&text[start..begin]));
            }
            out.references.insert(name.to_owned());
            if let Some(fallback) = &fallback {
                out.references.extend(fallback.references.iter().cloned());
            }
            out.parts.push(Part::Reference {
                name: name.to_owned(),
                fallback,
            });
            at += 1;
            start = at;
        }
        if start < text.len() {
            out.parts.push(Self::literal(&text[start..]));
        }
        out.choice = out.parts.len() <= 1
            && out.parts.iter().all(|part| match part {
                Part::Reference {
                    fallback: Some(value),
                    ..
                } => value.choice,
                _ => true,
            });
        Some(out)
    }
    fn part_upper(part: &Part, lookup: &impl Fn(&str) -> usize) -> usize {
        match part {
            Part::Literal { units, .. } => {
                if *units <= CAP {
                    *units
                } else {
                    0
                }
            }
            Part::Reference { name, fallback } => {
                lookup(name).max(fallback.as_ref().map_or(0, |value| value.upper(lookup)))
            }
        }
    }
    fn upper(&self, lookup: &impl Fn(&str) -> usize) -> usize {
        let mut units = 0usize;
        for part in &self.parts {
            let next = Self::part_upper(part, lookup);
            if next > 0 {
                units = units
                    .saturating_add(next)
                    .saturating_add(usize::from(units > 0))
                    .min(CAP);
            }
        }
        units
    }
    fn fresh(&self, lookup: &impl Fn(&str) -> usize) -> usize {
        let mut positive = self
            .parts
            .iter()
            .filter(|part| Self::part_upper(part, lookup) > 0);
        let Some(part) = positive.next() else {
            return 0;
        };
        if positive.next().is_some() {
            return self.upper(lookup);
        }
        match part {
            Part::Literal { wide: true, .. } => 0,
            Part::Literal { .. } => Self::part_upper(part, lookup),
            Part::Reference { fallback, .. } => {
                fallback.as_ref().map_or(0, |value| value.fresh(lookup))
            }
        }
    }
    fn terminal(&self, lookup: &impl Fn(&str) -> usize) -> usize {
        self.parts
            .iter()
            .map(|part| match part {
                Part::Literal { .. } => Self::part_upper(part, lookup),
                Part::Reference { name, fallback } => {
                    lookup(name).max(fallback.as_ref().map_or(0, |value| value.terminal(lookup)))
                }
            })
            .max()
            .unwrap_or(0)
    }
}

#[derive(Default)]
pub struct Scope {
    values: BTreeMap<String, usize>,
}
pub struct Checkpoint(Vec<(String, Option<usize>)>);
impl Scope {
    pub fn upper(&self, value: &Value) -> usize {
        // Plain standard declarations bypass custom substitution and its cap.
        if value.references.is_empty() {
            value.plain_units
        } else {
            value.upper(&|name| self.values.get(name).copied().unwrap_or(0))
        }
    }
    /// Every candidate is possible, independently of media, pseudo state or
    /// winners. Parent values remain alternatives when candidates do not apply.
    /// Returns potential fresh component units, excluding reference-only aliases.
    pub fn extend(&mut self, raw: &BTreeMap<&str, Vec<&Value>>) -> (Checkpoint, usize) {
        let names: Vec<_> = raw.keys().copied().collect();
        let indices: BTreeMap<_, _> = names
            .iter()
            .enumerate()
            .map(|(i, name)| (*name, i))
            .collect();
        let mut graph = vec![Vec::new(); names.len()];
        let mut reverse = vec![Vec::new(); names.len()];
        for (i, name) in names.iter().enumerate() {
            let edges: BTreeSet<_> = raw[name]
                .iter()
                .flat_map(|value| &value.references)
                .filter_map(|name| indices.get(name.as_str()).copied())
                .collect();
            for edge in edges {
                graph[i].push(edge);
                reverse[edge].push(i);
            }
        }
        // Iterative Kosaraju: potential cyclic components, not CSS active cycles.
        let mut seen = vec![false; names.len()];
        let mut order = Vec::new();
        for root in 0..names.len() {
            let mut work = vec![(root, false)];
            while let Some((node, leave)) = work.pop() {
                if leave {
                    order.push(node);
                    continue;
                }
                if seen[node] {
                    continue;
                }
                seen[node] = true;
                work.push((node, true));
                work.extend(graph[node].iter().rev().map(|node| (*node, false)));
            }
        }
        let mut component = vec![usize::MAX; names.len()];
        let mut components = Vec::new();
        for root in order.into_iter().rev() {
            if component[root] != usize::MAX {
                continue;
            }
            let id = components.len();
            let mut members = Vec::new();
            let mut work = vec![root];
            while let Some(node) = work.pop() {
                if component[node] != usize::MAX {
                    continue;
                }
                component[node] = id;
                members.push(node);
                work.extend(&reverse[node]);
            }
            components.push(members);
        }
        let mut bounds = vec![0; names.len()];
        for (id, members) in components.iter().enumerate().rev() {
            let lookup = |name: &str| {
                indices.get(name).map_or_else(
                    || self.values.get(name).copied().unwrap_or(0),
                    |&i| if component[i] == id { 0 } else { bounds[i] },
                )
            };
            let cyclic = members.len() > 1 || graph[members[0]].contains(&members[0]);
            if cyclic {
                let mut seed = 0;
                let mut joins = false;
                for &i in members {
                    seed = seed.max(self.values.get(names[i]).copied().unwrap_or(0));
                    for value in &raw[names[i]] {
                        seed = seed.max(value.terminal(&lookup));
                        joins |= !value.choice;
                    }
                }
                // Choices cannot grow text; a seeded concatenating component
                // can approach the per-value cap in some candidate combination.
                let upper = if seed > 0 && joins { CAP } else { seed };
                for &i in members {
                    bounds[i] = upper;
                }
            } else {
                let i = members[0];
                let upper = raw[names[i]]
                    .iter()
                    .map(|value| value.upper(&lookup))
                    .max()
                    .unwrap_or(0)
                    .max(self.values.get(names[i]).copied().unwrap_or(0));
                bounds[i] = upper;
            }
        }
        let lookup = |name: &str| {
            indices.get(name).map_or_else(
                || self.values.get(name).copied().unwrap_or(0),
                |&i| bounds[i],
            )
        };
        let mut fresh = 0usize;
        for name in &names {
            fresh = fresh.saturating_add(
                raw[name]
                    .iter()
                    .map(|value| value.fresh(&lookup))
                    .max()
                    .unwrap_or(0),
            );
        }
        let previous = names
            .iter()
            .map(|name| ((*name).to_owned(), self.values.get(*name).copied()))
            .collect();
        for (i, name) in names.iter().enumerate() {
            self.values.insert((*name).to_owned(), bounds[i]);
        }
        (Checkpoint(previous), fresh)
    }
    pub fn restore(&mut self, checkpoint: Checkpoint) {
        for (name, previous) in checkpoint.0 {
            if let Some(previous) = previous {
                self.values.insert(name, previous);
            } else {
                self.values.remove(&name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn push(scope: &mut Scope, definitions: &[(&str, &str)]) -> (Checkpoint, usize) {
        let values: Vec<_> = definitions
            .iter()
            .map(|(name, text)| (*name, Value::new(text)))
            .collect();
        let mut raw: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
        for (name, value) in &values {
            raw.entry(name).or_default().push(value);
        }
        scope.extend(&raw)
    }
    #[test]
    fn alternatives_bound_inherited_values_and_restore_siblings() {
        let mut scope = Scope::default();
        let (_, fresh) = push(&mut scope, &[("base", "#123"), ("alias", "var(--base)")]);
        assert_eq!(fresh, 4);
        let (child, fresh) = push(&mut scope, &[("base", "#abc"), ("base", "#abcdef")]);
        assert_eq!(fresh, 7);
        assert_eq!(scope.upper(&Value::new("var(--base)")), 7);
        assert_eq!(scope.upper(&Value::new("var(--alias)")), 4);
        scope.restore(child);
        assert_eq!(scope.upper(&Value::new("var(--base)")), 4);
    }
    #[test]
    fn choices_and_seeded_growth_components_have_different_upper_bounds() {
        let mut scope = Scope::default();
        let (_, fresh) = push(
            &mut scope,
            &[
                ("a", "var(--b,var(--c))"),
                ("b", "var(--a)"),
                ("c", "var(--b,#abc)"),
            ],
        );
        assert_eq!(fresh, 4);
        assert_eq!(scope.upper(&Value::new("var(--a)")), 4);
        let (_, fresh) = push(&mut scope, &[("x", "var(--y)x"), ("y", "var(--x)")]);
        assert_eq!(fresh, CAP);
        assert_eq!(scope.upper(&Value::new("var(--y)")), CAP);
        let (_, fresh) = push(&mut scope, &[("empty", "var(--empty)var(--empty)")]);
        assert_eq!(fresh, 0);
        assert_eq!(scope.upper(&Value::new("var(--empty)")), 0);
    }
    #[test]
    fn empty_and_forward_alias_graphs_are_iterative() {
        let mut scope = Scope::default();
        let names: Vec<_> = (0..5000).map(|i| format!("v{i:04}")).collect();
        let values: Vec<_> = (0..5000)
            .map(|i| {
                Value::new(&if i == 4999 {
                    "var(--missing,)".to_owned()
                } else {
                    format!("var(--v{:04})var(--v{:04})", i + 1, i + 1)
                })
            })
            .collect();
        let raw = names
            .iter()
            .zip(&values)
            .map(|(name, value)| (name.as_str(), vec![value]))
            .collect();
        let (_, fresh) = scope.extend(&raw);
        assert_eq!(fresh, 0);
        assert_eq!(scope.upper(&Value::new("var(--v0000)")), 0);
    }
    #[test]
    fn quoted_unicode_and_keyword_aliases_count_component_units() {
        let mut scope = Scope::default();
        let (_, fresh) = push(&mut scope, &[("a", "\"😀\""), ("b", "var(--a,inherit)")]);
        assert_eq!(fresh, 4);
        assert_eq!(scope.upper(&Value::new("var(--b)")), 7); // The fallback is possible.
        let oversized = format!("\"{}\"", "😀".repeat(CAP / 2));
        let (_, fresh) = push(&mut scope, &[("over", &oversized)]);
        assert_eq!(fresh, 0);
        assert_eq!(scope.upper(&Value::new("var(--over)")), 0);
    }
}
