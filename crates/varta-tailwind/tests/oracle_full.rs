//! Manual conformance run against the full Tailwind CSS v4.3.5 oracle corpus
//! (39,152 candidates of `candidatesToCss`, generated outside the
//! repository by the official compiler). Both sides are put in Varta's
//! canonical form before comparison.
//!
//! Run with `cargo test -p varta-tailwind --release --test oracle_full --
//! --ignored --nocapture`. A missing corpus skips the run with a note.
use std::collections::BTreeMap;
use std::path::Path;
use varta_tailwind::canonical::{VartaSupports, canonicalize};
use varta_tailwind::css_parser::parse;
use varta_tailwind::printer::to_css;
use varta_tailwind::stylesheet::{candidates_to_css, default_design_system};

const CORPUS: &str = "/private/tmp/claude-501/-Users-snaga--claude-spares-spare-cVA4jy/9fb1ad1c-83ef-4371-aecd-e4777fd9cedf/scratchpad/tw/out/candidates.jsonl";
/// Failures printed per family.
const SAMPLES_PER_FAMILY: usize = 2;

fn canonical(css: &str) -> Result<String, String> {
    let ast = parse(css).map_err(|e| e.message())?;
    let ast = canonicalize(ast, &VartaSupports).map_err(|e| e.to_string())?;
    Ok(to_css(&ast))
}

/// The utility family of a candidate: its root before the first value dash.
fn family(candidate: &str) -> String {
    let base = candidate.rsplit(':').next().unwrap_or(candidate);
    let base = base.trim_start_matches(['-', '!']);
    base.split(['-', '/', '['])
        .next()
        .unwrap_or(base)
        .to_owned()
}

#[test]
#[ignore = "manual: needs the oracle corpus outside the repository"]
fn full_oracle_corpus() {
    if !Path::new(CORPUS).exists() {
        println!("skipped: {CORPUS} not found");
        return;
    }
    let text = std::fs::read_to_string(CORPUS).unwrap();
    let mut ds = default_design_system();
    // group -> (pass, total); family -> failures
    let mut groups: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut families: BTreeMap<String, Vec<(String, String, String)>> = BTreeMap::new();
    for line in text.lines() {
        let record: serde_json::Value = serde_json::from_str(line).unwrap();
        let group = record["group"].as_str().unwrap().to_owned();
        let candidate = record["candidate"].as_str().unwrap();
        let expected = record["css"].as_str().map(canonical);
        let actual = candidates_to_css(&mut ds, &[candidate])
            .pop()
            .flatten()
            .map(|css| canonical(&css));
        let entry = groups.entry(group.clone()).or_default();
        entry.1 += 1;
        let ok = match (&expected, &actual) {
            (None, None) => true,
            (Some(Ok(e)), Some(Ok(a))) => e == a,
            _ => false,
        };
        if ok {
            entry.0 += 1;
        } else {
            let show = |v: &Option<Result<String, String>>| match v {
                None => "<none>".to_owned(),
                Some(Ok(css)) => css.clone(),
                Some(Err(e)) => format!("<error {e}>"),
            };
            families
                .entry(format!("{group}/{}", family(candidate)))
                .or_default()
                .push((candidate.to_owned(), show(&expected), show(&actual)));
        }
    }
    let mut worst: Vec<_> = families.iter().collect();
    worst.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(b.0)));
    for (family, failures) in &worst {
        println!("== {family}: {} failing", failures.len());
        for (candidate, expected, actual) in failures.iter().take(SAMPLES_PER_FAMILY) {
            println!("-- {candidate}\nexpected:\n{expected}\nactual:\n{actual}");
        }
    }
    let (mut pass, mut total) = (0, 0);
    for (group, (p, t)) in &groups {
        println!("group {group}: {p}/{t}");
        pass += p;
        total += t;
    }
    println!("total: {pass}/{total}");
}
