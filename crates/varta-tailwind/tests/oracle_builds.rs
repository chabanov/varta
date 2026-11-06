//! Manual whole-stylesheet conformance against builds of the official
//! Tailwind CSS v4.3.5 compiler (`compile(css, { polyfills: None })
//! .build(candidates)`, generated outside the repository). Both sides are put
//! in Varta's canonical form before comparison.
//!
//! Run with `cargo test -p varta-tailwind --test oracle_builds -- --ignored
//! --nocapture`. A missing oracle file skips the run with a note.
use std::path::Path;
use varta_tailwind::canonical::{VartaSupports, canonicalize};
use varta_tailwind::compile;
use varta_tailwind::css_parser::parse;
use varta_tailwind::printer::to_css;

const BUILDS: &str = "/private/tmp/claude-501/-Users-snaga--claude-spares-spare-cVA4jy/9fb1ad1c-83ef-4371-aecd-e4777fd9cedf/scratchpad/oracle-gen/builds.json";
/// Lines of context printed around the first difference.
const CONTEXT_LINES: usize = 6;

fn canonical(css: &str) -> String {
    to_css(&canonicalize(parse(css).unwrap(), &VartaSupports).unwrap())
}

fn first_difference(expected: &str, actual: &str) -> String {
    let e: Vec<&str> = expected.lines().collect();
    let a: Vec<&str> = actual.lines().collect();
    let index = e.iter().zip(&a).take_while(|(x, y)| x == y).count();
    let from = index.saturating_sub(CONTEXT_LINES);
    format!(
        "line {index}\nexpected:\n{}\nactual:\n{}",
        e[from..(index + CONTEXT_LINES).min(e.len())].join("\n"),
        a[from..(index + CONTEXT_LINES).min(a.len())].join("\n")
    )
}

#[test]
#[ignore = "manual: needs the oracle builds outside the repository"]
fn oracle_builds() {
    if !Path::new(BUILDS).exists() {
        println!("skipped: {BUILDS} not found");
        return;
    }
    let cases: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(BUILDS).unwrap()).unwrap();
    let mut failures = 0;
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let css = case["css"].as_str().unwrap();
        let candidates: Vec<&str> = case["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap())
            .collect();
        let Some(output) = case["output"].as_str() else {
            println!("{name}: oracle error {}", case["error"]);
            continue;
        };
        let expected = canonical(output);
        let actual =
            compile(css).and_then(|mut c| c.build(candidates.iter().copied(), &VartaSupports));
        match actual {
            Ok(build) if build.css == expected => {
                println!("{name}: ok ({} bytes)", build.css.len())
            }
            Ok(build) => {
                failures += 1;
                println!(
                    "{name}: MISMATCH {}",
                    first_difference(&expected, &build.css)
                );
            }
            Err(error) => {
                failures += 1;
                println!("{name}: ERROR {error}");
            }
        }
    }
    assert_eq!(failures, 0);
}
