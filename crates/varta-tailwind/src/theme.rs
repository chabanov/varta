//! Theme variables (port of Tailwind's `theme.ts`).
//!
//! Values keep JavaScript `Map` semantics: re-setting a key keeps its
//! position, deleting and adding moves it to the end. Iteration follows that
//! insertion order through a sequence index, so output is deterministic.
use crate::ast::Node;
use crate::utils::{escape, unescape};
use std::collections::BTreeMap;

/// Theme option flags (`ThemeOptions`).
pub mod options {
    pub const NONE: u8 = 0;
    pub const INLINE: u8 = 1 << 0;
    pub const REFERENCE: u8 = 1 << 1;
    pub const DEFAULT: u8 = 1 << 2;
    pub const STATIC: u8 = 1 << 3;
    pub const USED: u8 = 1 << 4;
}

/// Theme namespaces whose keys are ignored by a shorter namespace.
const IGNORED_THEME_KEYS: &[(&str, &[&str])] = &[
    ("--font", &["--font-weight", "--font-size"]),
    ("--inset", &["--inset-shadow", "--inset-ring"]),
    (
        "--text",
        &[
            "--text-color",
            "--text-decoration-color",
            "--text-decoration-thickness",
            "--text-indent",
            "--text-shadow",
            "--text-underline-offset",
        ],
    ),
    ("--grid-column", &["--grid-column-start", "--grid-column-end"]),
    ("--grid-row", &["--grid-row-start", "--grid-row-end"]),
];

fn ignored_for(namespace: &str) -> &'static [&'static str] {
    IGNORED_THEME_KEYS
        .iter()
        .find(|(ns, _)| *ns == namespace)
        .map_or(&[], |(_, ignored)| ignored)
}

fn is_ignored_theme_key(key: &str, namespace: &str) -> bool {
    ignored_for(namespace).iter().any(|ignored| {
        key == *ignored
            || (key.starts_with(ignored) && key[ignored.len()..].starts_with('-'))
    })
}

/// One theme value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemeValue {
    pub value: String,
    pub options: u8,
}

/// A theme error (invalid namespace reset).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid theme value `{value}` for namespace `{key}`")]
pub struct ThemeError {
    pub key: String,
    pub value: String,
}

/// The theme: CSS variables from `@theme` blocks.
#[derive(Debug, Clone, Default)]
pub struct Theme {
    pub prefix: Option<String>,
    values: BTreeMap<String, (u64, ThemeValue)>,
    order: BTreeMap<u64, String>,
    next: u64,
    keyframes: Vec<Node>,
}

impl Theme {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The number of values.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    fn set(&mut self, key: &str, value: ThemeValue) {
        if let Some(entry) = self.values.get_mut(key) {
            entry.1 = value;
            return;
        }
        let seq = self.next;
        self.next += 1;
        self.order.insert(seq, key.to_owned());
        self.values.insert(key.to_owned(), (seq, value));
    }

    fn delete(&mut self, key: &str) {
        if let Some((seq, _)) = self.values.remove(key) {
            self.order.remove(&seq);
        }
    }

    fn clear(&mut self) {
        self.values.clear();
        self.order.clear();
    }

    /// Keys in insertion order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.order.values().map(String::as_str)
    }

    /// Entries in insertion order (keys unprefixed).
    pub fn raw_entries(&self) -> impl Iterator<Item = (&str, &ThemeValue)> {
        self.order
            .values()
            .filter_map(|key| self.values.get(key).map(|(_, v)| (key.as_str(), v)))
    }

    /// Entries in insertion order, keys with the theme prefix applied.
    #[must_use]
    pub fn entries(&self) -> Vec<(String, ThemeValue)> {
        self.raw_entries()
            .map(|(key, value)| (self.prefix_key(key), value.clone()))
            .collect()
    }

    /// Add a value; `--ns-*: initial` clears a namespace.
    pub fn add(&mut self, key: &str, value: &str, opts: u8) -> Result<(), ThemeError> {
        if let Some(namespace) = key.strip_suffix("-*") {
            if value != "initial" {
                return Err(ThemeError {
                    key: key.to_owned(),
                    value: value.to_owned(),
                });
            }
            if key == "--*" {
                self.clear();
            } else {
                self.clear_namespace(namespace, options::NONE);
            }
        }
        if opts & options::DEFAULT != 0
            && let Some((_, existing)) = self.values.get(key)
            && existing.options & options::DEFAULT == 0
        {
            return Ok(());
        }
        if value == "initial" {
            self.delete(key);
        } else {
            self.set(
                key,
                ThemeValue {
                    value: value.to_owned(),
                    options: opts,
                },
            );
        }
        Ok(())
    }

    /// Value names (without the namespace) under each namespace.
    #[must_use]
    pub fn keys_in_namespaces(&self, namespaces: &[&str]) -> Vec<String> {
        let mut keys = Vec::new();
        for namespace in namespaces {
            let prefix = format!("{namespace}-");
            for key in self.keys() {
                if !key.starts_with(&prefix) {
                    continue;
                }
                if key.get(2..).is_some_and(|rest| rest.contains("--")) {
                    continue;
                }
                if is_ignored_theme_key(key, namespace) {
                    continue;
                }
                keys.push(key[prefix.len()..].to_owned());
            }
        }
        keys
    }

    /// The first value among `keys`.
    #[must_use]
    pub fn get(&self, keys: &[&str]) -> Option<String> {
        keys.iter()
            .find_map(|key| self.values.get(*key).map(|(_, v)| v.value.clone()))
    }

    #[must_use]
    pub fn has_default(&self, key: &str) -> bool {
        self.get_options(key) & options::DEFAULT == options::DEFAULT
    }

    /// The options of a (possibly prefixed and escaped) key.
    #[must_use]
    pub fn get_options(&self, key: &str) -> u8 {
        let key = unescape(&self.unprefix_key(key));
        self.values.get(&key).map_or(options::NONE, |(_, v)| v.options)
    }

    #[must_use]
    pub fn prefix_key(&self, key: &str) -> String {
        match &self.prefix {
            Some(prefix) => format!("--{prefix}-{}", &key[2.min(key.len())..]),
            None => key.to_owned(),
        }
    }

    fn unprefix_key(&self, key: &str) -> String {
        match &self.prefix {
            Some(prefix) => format!("--{}", key.get(3 + prefix.len()..).unwrap_or_default()),
            None => key.to_owned(),
        }
    }

    /// Remove every key of `namespace` (with `clear_options` all present).
    pub fn clear_namespace(&mut self, namespace: &str, clear_options: u8) {
        let ignored = ignored_for(namespace);
        let keys: Vec<String> = self.keys().map(str::to_owned).collect();
        'outer: for key in keys {
            if !key.starts_with(namespace) {
                continue;
            }
            if clear_options != options::NONE {
                let opts = self.get_options(&key);
                if opts & clear_options != clear_options {
                    continue;
                }
            }
            for ignored in ignored {
                if key.starts_with(ignored) {
                    continue 'outer;
                }
            }
            self.delete(&key);
        }
    }

    fn resolve_key(&self, candidate_value: Option<&str>, namespaces: &[&str]) -> Option<String> {
        for namespace in namespaces {
            let mut key = match candidate_value {
                Some(value) => format!("{namespace}-{value}"),
                None => (*namespace).to_owned(),
            };
            if !self.values.contains_key(&key) {
                match candidate_value {
                    Some(value) if value.contains('.') => {
                        key = format!("{namespace}-{}", value.replace('.', "_"));
                        if !self.values.contains_key(&key) {
                            continue;
                        }
                    }
                    _ => continue,
                }
            }
            if is_ignored_theme_key(&key, namespace) {
                continue;
            }
            return Some(key);
        }
        None
    }

    fn var(&self, key: &str) -> Option<String> {
        let (_, value) = self.values.get(key)?;
        let fallback = (value.options & options::REFERENCE != 0).then(|| value.value.clone());
        Some(match fallback {
            Some(fallback) => format!("var({}, {fallback})", escape(&self.prefix_key(key))),
            None => format!("var({})", escape(&self.prefix_key(key))),
        })
    }

    /// Mark a variable used; true when it was not used before.
    pub fn mark_used_variable(&mut self, key: &str) -> bool {
        let key = unescape(&self.unprefix_key(key));
        let Some((_, value)) = self.values.get_mut(&key) else {
            return false;
        };
        let was_used = value.options & options::USED != 0;
        value.options |= options::USED;
        !was_used
    }

    /// Resolve a candidate value to `var(--key)` (or the inline value).
    #[must_use]
    pub fn resolve(
        &self,
        candidate_value: Option<&str>,
        namespaces: &[&str],
        opts: u8,
    ) -> Option<String> {
        let key = self.resolve_key(candidate_value, namespaces)?;
        let (_, value) = self.values.get(&key)?;
        if (opts | value.options) & options::INLINE != 0 {
            return Some(value.value.clone());
        }
        self.var(&key)
    }

    /// Resolve a candidate value to its raw theme value.
    #[must_use]
    pub fn resolve_value(&self, candidate_value: Option<&str>, namespaces: &[&str]) -> Option<String> {
        let key = self.resolve_key(candidate_value, namespaces)?;
        self.values.get(&key).map(|(_, v)| v.value.clone())
    }

    /// Resolve with nested keys (e.g. `--line-height` of a font size).
    #[must_use]
    pub fn resolve_with(
        &self,
        candidate_value: &str,
        namespaces: &[&str],
        nested_keys: &[&str],
    ) -> Option<(String, BTreeMap<String, String>)> {
        let key = self.resolve_key(Some(candidate_value), namespaces)?;
        let mut extra = BTreeMap::new();
        for name in nested_keys {
            let nested = format!("{key}{name}");
            let Some((_, value)) = self.values.get(&nested) else {
                continue;
            };
            let resolved = if value.options & options::INLINE != 0 {
                value.value.clone()
            } else {
                self.var(&nested)?
            };
            extra.insert((*name).to_owned(), resolved);
        }
        let (_, value) = self.values.get(&key)?;
        if value.options & options::INLINE != 0 {
            return Some((value.value.clone(), extra));
        }
        Some((self.var(&key)?, extra))
    }

    /// Values of a namespace, keyed by the remainder (`None` for the bare key).
    #[must_use]
    pub fn namespace(&self, namespace: &str) -> Vec<(Option<String>, String)> {
        let prefix = format!("{namespace}-");
        let nested = format!("{prefix}-");
        let mut out: Vec<(Option<String>, String)> = Vec::new();
        let mut push = |key: Option<String>, value: String| {
            if let Some(existing) = out.iter_mut().find(|(k, _)| *k == key) {
                existing.1 = value;
            } else {
                out.push((key, value));
            }
        };
        for (key, value) in self.raw_entries() {
            if key == namespace {
                push(None, value.value.clone());
            } else if key.starts_with(&nested) {
                push(Some(key[namespace.len()..].to_owned()), value.value.clone());
            } else if let Some(rest) = key.strip_prefix(&prefix) {
                push(Some(rest.to_owned()), value.value.clone());
            }
        }
        out
    }

    pub fn add_keyframes(&mut self, keyframes: Node) {
        if !self.keyframes.contains(&keyframes) {
            self.keyframes.push(keyframes);
        }
    }

    #[must_use]
    pub fn keyframes(&self) -> &[Node] {
        &self.keyframes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_order_and_resolution() {
        let mut theme = Theme::new();
        theme.add("--color-red-500", "red", options::NONE).unwrap();
        theme.add("--color-blue-500", "blue", options::NONE).unwrap();
        theme.add("--color-red-500", "crimson", options::NONE).unwrap();
        assert_eq!(
            theme.keys().collect::<Vec<_>>(),
            vec!["--color-red-500", "--color-blue-500"]
        );
        assert_eq!(
            theme.resolve(Some("red-500"), &["--color"], options::NONE),
            Some("var(--color-red-500)".into())
        );
        theme.add("--color-*", "initial", options::NONE).unwrap();
        assert!(theme.is_empty());
        assert!(theme.add("--color-*", "red", options::NONE).is_err());
    }

    #[test]
    fn ignored_and_dotted_keys() {
        let mut theme = Theme::new();
        theme.add("--text-sm", "0.875rem", options::NONE).unwrap();
        theme.add("--text-color-x", "red", options::NONE).unwrap();
        theme.add("--leading-1_5", "1.5", options::NONE).unwrap();
        assert_eq!(theme.keys_in_namespaces(&["--text"]), vec!["sm"]);
        assert_eq!(theme.resolve(Some("color-x"), &["--text"], options::NONE), None);
        assert_eq!(
            theme.resolve_value(Some("1.5"), &["--leading"]),
            Some("1.5".into())
        );
    }
}
