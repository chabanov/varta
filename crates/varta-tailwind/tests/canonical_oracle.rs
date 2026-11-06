//! Manual check of the canonical form against the Tailwind CSS v4.3.5
//! oracle output (`candidatesToCss` per candidate and full stylesheets).
//!
//! Run with `cargo test -p varta-tailwind --test canonical_oracle -- --ignored
//! --nocapture`. The oracle files live outside the repository; a missing file
//! skips the check with a note.
use std::collections::BTreeMap;
use std::path::Path;
use varta_tailwind::ast::Node;
use varta_tailwind::canonical::{VartaSupports, canonicalize};
use varta_tailwind::css_parser::parse;
use varta_tailwind::printer::to_css;

/// The oracle directory of the K8 Tailwind port.
const ORACLE_DIR: &str = "/private/tmp/claude-501/-Users-snaga--claude-spares-spare-cVA4jy/9fb1ad1c-83ef-4371-aecd-e4777fd9cedf/scratchpad/tw/out";
const CANDIDATES: &str = "candidates.jsonl";
const STYLESHEETS: &[&str] = &["preflight-theme.raw.css", "full.raw.css"];
const RANGE_OPERATORS: &[&str] = &[">=", "<=", "<", ">"];
const RANGE_PRELUDES: &[&str] = &["@media", "@container"];
const AT_SUPPORTS: &str = "@supports";

/// Whether a selector holds an unescaped `&` outside strings.
fn has_nesting_selector(selector: &str) -> bool {
    let mut chars = selector.chars();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        match (c, quote) {
            ('\\', _) => {
                chars.next();
            }
            (c, Some(open)) if c == open => quote = None,
            (_, Some(_)) => {}
            ('"' | '\'', None) => quote = Some(c),
            ('&', None) => return true,
            _ => {}
        }
    }
    false
}

/// The invariants of the canonical form that `ast` violates.
fn violations(ast: &[Node]) -> Vec<&'static str> {
    let mut found = Vec::new();
    let mut stack: Vec<(&Node, bool)> = ast.iter().map(|node| (node, false)).collect();
    while let Some((node, in_rule)) = stack.pop() {
        match node {
            Node::Comment(_) => found.push("comment"),
            Node::Rule { selector, nodes } => {
                if in_rule {
                    found.push("nested style rule");
                }
                if has_nesting_selector(selector) {
                    found.push("`&`");
                }
                stack.extend(nodes.iter().map(|child| (child, true)));
            }
            Node::AtRule {
                name,
                params,
                nodes,
            } => {
                if name == AT_SUPPORTS {
                    found.push("@supports");
                }
                if RANGE_PRELUDES.contains(&name.as_str())
                    && RANGE_OPERATORS
                        .iter()
                        .any(|operator| params.contains(operator))
                {
                    found.push("range operator");
                }
                stack.extend(nodes.iter().map(|child| (child, in_rule)));
            }
            Node::Context { .. } | Node::AtRoot(_) => found.push("marker"),
            Node::Decl { .. } => {}
        }
    }
    found
}

/// Counts of one oracle run.
#[derive(Debug, Default)]
struct Report {
    checked: usize,
    skipped_null: usize,
    parse_errors: usize,
    canonical_errors: BTreeMap<String, usize>,
    not_fixed_points: usize,
    violations: BTreeMap<&'static str, usize>,
    changed: usize,
    examples: Vec<String>,
}

const MAX_EXAMPLES: usize = 10;

impl Report {
    fn example(&mut self, text: String) {
        if self.examples.len() < MAX_EXAMPLES {
            self.examples.push(text);
        }
    }

    fn check(&mut self, label: &str, css: &str) {
        self.checked += 1;
        let ast = match parse(css) {
            Ok(ast) => ast,
            Err(error) => {
                self.parse_errors += 1;
                self.example(format!("{label}: parse error {error}"));
                return;
            }
        };
        let canonical = match canonicalize(ast, &VartaSupports) {
            Ok(canonical) => canonical,
            Err(error) => {
                *self.canonical_errors.entry(error.to_string()).or_default() += 1;
                self.example(format!("{label}: {error}"));
                return;
            }
        };
        let printed = to_css(&canonical);
        if printed != css {
            self.changed += 1;
        }
        let again = parse(&printed)
            .ok()
            .and_then(|ast| canonicalize(ast, &VartaSupports).ok());
        if again.as_ref() != Some(&canonical) {
            self.not_fixed_points += 1;
            self.example(format!("{label}: not a fixed point:\n{printed}"));
        }
        let found = violations(&canonical);
        if !found.is_empty() {
            self.example(format!("{label}: {found:?}:\n{printed}"));
        }
        for violation in found {
            *self.violations.entry(violation).or_default() += 1;
        }
    }

    fn assert_clean(&self) {
        eprintln!("{self:#?}");
        assert_eq!(self.parse_errors, 0);
        assert!(self.canonical_errors.is_empty());
        assert_eq!(self.not_fixed_points, 0);
        assert!(self.violations.is_empty());
    }
}

#[test]
#[ignore = "manual: needs the Tailwind oracle output outside the repository"]
fn oracle_candidates_canonicalize_to_fixed_points() {
    let path = Path::new(ORACLE_DIR).join(CANDIDATES);
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!("skipped: {} does not exist", path.display());
        return;
    };
    let mut report = Report::default();
    for line in text.lines() {
        let Ok(row) = serde_json::from_str::<serde_json::Value>(line) else {
            report.parse_errors += 1;
            continue;
        };
        let candidate = row["candidate"].as_str().unwrap_or_default();
        match row["css"].as_str() {
            Some(css) => report.check(candidate, css),
            None => report.skipped_null += 1,
        }
    }
    report.assert_clean();
}

#[test]
#[ignore = "manual: needs the Tailwind oracle output outside the repository"]
fn oracle_stylesheets_canonicalize_to_fixed_points() {
    for name in STYLESHEETS {
        let path = Path::new(ORACLE_DIR).join(name);
        let Ok(css) = std::fs::read_to_string(&path) else {
            eprintln!("skipped: {} does not exist", path.display());
            continue;
        };
        let mut report = Report::default();
        report.check(name, &css);
        report.assert_clean();
    }
}

#[test]
fn nesting_selector_detection_ignores_escapes_and_strings() {
    assert!(has_nesting_selector("& > .a"));
    assert!(!has_nesting_selector(".\\[\\&_p\\]\\:flex p"));
    assert!(!has_nesting_selector("[data-a='&']"));
}
