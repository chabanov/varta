//! Malformed entry stylesheets and class tokens fail with typed errors or
//! produce nothing; they never panic. Inputs are cut and spliced at every
//! `STRIDE`-th character of committed fixtures (multi-byte characters
//! included, to catch byte-index slicing).
use varta_tailwind::stylesheet::{candidates_to_css, default_design_system};
use varta_tailwind::{VartaSupports, compile};

const BUILDS: &str = include_str!("fixtures/oracle-builds.json");
const CANDIDATES: &str = include_str!("fixtures/oracle-candidates.jsonl");
/// Cut positions are every `STRIDE`-th character.
const STRIDE: usize = 5;
/// Candidates mutated (every `CANDIDATE_STRIDE`-th fixture line).
const CANDIDATE_STRIDE: usize = 9;
const SPLICES: &[&str] = &["€", "{", "}", "\"", "(", ")", "[", "]", "\\", ":", "/"];

#[test]
fn malformed_entry_stylesheets_never_panic() {
    let cases: serde_json::Value = serde_json::from_str(BUILDS).unwrap();
    let css = cases
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["name"] == "custom")
        .unwrap()["css"]
        .as_str()
        .unwrap();
    let chars: Vec<char> = css.chars().collect();
    for cut in (0..chars.len()).step_by(STRIDE) {
        let head: String = chars[..cut].iter().collect();
        let tail: String = chars[cut..].iter().collect();
        for splice in SPLICES {
            let input = format!("{head}{splice}{tail}");
            let _ = compile(&input)
                .and_then(|mut c| c.build(["flex", "hocus:p-4", "tab-4", "btn"], &VartaSupports));
        }
        let _ = compile(&head);
    }
}

#[test]
fn malformed_class_tokens_never_panic() {
    let mut ds = default_design_system();
    for line in CANDIDATES.lines().step_by(CANDIDATE_STRIDE) {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        let chars: Vec<char> = row["candidate"].as_str().unwrap().chars().collect();
        for cut in 0..=chars.len() {
            let head: String = chars[..cut].iter().collect();
            let tail: String = chars[cut..].iter().collect();
            for splice in SPLICES {
                let token = format!("{head}{splice}{tail}");
                let _ = candidates_to_css(&mut ds, &[token.as_str()]);
            }
        }
    }
}
