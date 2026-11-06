//! Conformance with the official Tailwind CSS v4.3.5 compiler on the
//! committed oracle subset (see `fixtures/NOTICE`): per-candidate CSS
//! (`candidatesToCss`) and whole entry-stylesheet builds. Both sides are put
//! in Varta's canonical form before comparison; every category must pass.
use std::collections::BTreeMap;
use varta_tailwind::canonical::{VartaSupports, canonicalize};
use varta_tailwind::compile;
use varta_tailwind::css_parser::parse;
use varta_tailwind::printer::to_css;
use varta_tailwind::stylesheet::{candidates_to_css, default_design_system};

const CANDIDATES: &str = include_str!("fixtures/oracle-candidates.jsonl");
const PROPERTIES: &str = include_str!("fixtures/oracle-properties.json");
const BUILDS: &str = include_str!("fixtures/oracle-builds.json");
/// Failures shown in the assertion message.
const SHOWN_FAILURES: usize = 5;

fn canonical(css: &str) -> String {
    to_css(&canonicalize(parse(css).unwrap(), &VartaSupports).unwrap())
}

#[test]
fn candidates_match_the_oracle() {
    let properties: Vec<String> = serde_json::from_str(PROPERTIES).unwrap();
    let mut ds = default_design_system();
    // group -> (passed, total)
    let mut rates: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut failures = Vec::new();
    for line in CANDIDATES.lines() {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        let group = row["group"].as_str().unwrap().to_owned();
        let candidate = row["candidate"].as_str().unwrap();
        let expected = row["css"].as_str().map(|css| {
            let mut full = css.to_owned();
            for index in row["properties"].as_array().unwrap() {
                full.push_str(&properties[index.as_u64().unwrap() as usize]);
            }
            canonical(&full)
        });
        let actual = candidates_to_css(&mut ds, &[candidate])
            .pop()
            .flatten()
            .map(|css| canonical(&css));
        let rate = rates.entry(group).or_default();
        rate.1 += 1;
        if expected == actual {
            rate.0 += 1;
        } else {
            failures.push(format!(
                "{candidate}\nexpected: {expected:?}\nactual: {actual:?}"
            ));
        }
    }
    let summary: Vec<String> = rates
        .iter()
        .map(|(group, (passed, total))| format!("{group}: {passed}/{total}"))
        .collect();
    println!("{}", summary.join(", "));
    assert!(
        failures.is_empty(),
        "{} failures ({}):\n{}",
        failures.len(),
        summary.join(", "),
        failures[..failures.len().min(SHOWN_FAILURES)].join("\n")
    );
    assert!(rates.len() >= 5, "every category is present: {summary:?}");
}

#[test]
fn whole_stylesheets_match_the_oracle() {
    let cases: serde_json::Value = serde_json::from_str(BUILDS).unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let candidates: Vec<&str> = case["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap())
            .collect();
        let expected = canonical(case["output"].as_str().unwrap());
        let mut compiler = compile(case["css"].as_str().unwrap()).unwrap();
        let build = compiler
            .build(candidates.iter().copied(), &VartaSupports)
            .unwrap();
        assert!(build.css == expected, "{name}: differs from the oracle");
    }
}
