//! Shared helpers for the documentation drift tests: a generated block sits
//! between `<!-- generated:NAME:begin ... -->` and `<!-- generated:NAME:end -->`
//! and must equal the text its generator derives from the registries.
use std::path::{Path, PathBuf};
use varta_common::language;

pub fn workspace_file(relative: &str) -> (PathBuf, String) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    // Test-only read of a checked-in file; a missing file is a test failure.
    let text = std::fs::read_to_string(&path).unwrap();
    (path, text)
}

/// Returns the text between `<!-- generated:NAME:begin ... -->` and its end marker.
pub fn block<'a>(text: &'a str, name: &str) -> &'a str {
    let begin = format!("<!-- generated:{name}:begin");
    let end = format!("<!-- generated:{name}:end -->");
    let start = text
        .find(&begin)
        .unwrap_or_else(|| panic!("missing {begin}"));
    let body = start + text[start..].find("-->").unwrap() + "-->".len();
    let stop = text[body..]
        .find(&end)
        .unwrap_or_else(|| panic!("missing {end}"));
    text[body..body + stop].trim()
}

pub fn assert_block(relative: &str, name: &str, expected: &str) {
    let (path, text) = workspace_file(relative);
    let actual = block(&text, name);
    assert!(
        actual == expected.trim(),
        "{} block `{name}` is stale; replace it with:\n\n{}\n",
        path.display(),
        expected.trim()
    );
}

pub fn profile_name(profile: language::LanguageProfile) -> String {
    // Infallible: the profile enum serializes to its registered string name.
    serde_json::to_value(profile)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

/// Renders rows as a Markdown table with the given header.
pub fn table(header: &[&str], rows: impl IntoIterator<Item = Vec<String>>) -> String {
    let mut lines = vec![
        format!("| {} |", header.join(" | ")),
        format!("|{}", " --- |".repeat(header.len())),
    ];
    lines.extend(
        rows.into_iter()
            .map(|cells| format!("| {} |", cells.join(" | "))),
    );
    lines.join("\n")
}
