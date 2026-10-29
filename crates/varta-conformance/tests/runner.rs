//! Exercise the real executable, pinned inputs, child protocol and classifications.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
fn executable() -> &'static str {
    env!("CARGO_BIN_EXE_varta-conformance")
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
struct Fixture {
    root: PathBuf,
    value: Value,
}
impl Fixture {
    fn new(source: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "varta-conformance-{}-{}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("cases")).unwrap();
        fs::write(root.join("cases/input.ts"), source).unwrap();
        let value = json!({"schemaVersion":1,"id":"fixture","edition":"ECMAScript 2025","sourceModel":"checked-ts-main-v1","profile":"checked-f64-utf16-v26","features":[{"id":"number","title":"Number","core":["CORE-01"],"origin":"ecmascript2025","reference":"https://tc39.es/ecma262/2025/multipage/numbers-and-dates.html"}],"cases":[{"id":"example","feature":"number","path":"cases/input.ts","sha256":digest(source.as_bytes()),"stdout":["varta log: 2"],"baseline":{"status":"supported"}}]});
        let fixture = Self { root, value };
        fixture.save();
        fixture
    }
    fn save(&self) {
        fs::write(
            self.root.join("suite.json"),
            serde_json::to_vec(&self.value).unwrap(),
        )
        .unwrap();
    }
    fn command(&self) -> Command {
        let mut command = Command::new(executable());
        command
            .current_dir(&self.root)
            .args(["--suite", "suite.json", "--json"]);
        command
    }
    fn run(&self, args: &[&str]) -> (std::process::Output, Value) {
        let output = self.command().args(args).output().unwrap();
        let value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|error| panic!("{error}: {}", String::from_utf8_lossy(&output.stderr)));
        (output, value)
    }
    fn unsupported(&mut self, code: &str) {
        self.value["cases"][0]["baseline"] = json!({"status":"known_unsupported","diagnostic":code,"reason":"The fixture names an explicit current feature gap."});
        self.save();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
const SOURCE: &str = "export function main():void{console.log(2);}";
#[test]
fn core_inventory_runs_every_pinned_case_and_keeps_known_gaps_separate() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/core-v1.json");
    let bytes = fs::read(&path).unwrap();
    let inventory: Value = serde_json::from_slice(&bytes).unwrap();
    let output = Command::new(executable())
        .args(["--suite", path.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["baselineMatches"], true);
    assert_eq!(
        report["counts"]["total"],
        inventory["cases"].as_array().unwrap().len()
    );
    assert_eq!(report["inventorySha256"], digest(&bytes));
    assert_eq!(
        report["dependencyLockSha256"],
        digest(&fs::read(path.parent().unwrap().join("../Cargo.lock")).unwrap())
    );
    let expected_supported = inventory["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["baseline"]["status"] == "supported")
        .count();
    assert_eq!(report["counts"]["passed"], expected_supported);
    assert_eq!(
        report["counts"]["knownUnsupported"].as_u64().unwrap() as usize,
        inventory["cases"].as_array().unwrap().len() - expected_supported
    );
    assert!(report["counts"]["knownUnsupported"].as_u64().unwrap() > 0);
    assert!(!report["untestedFeatures"].as_array().unwrap().is_empty());
    for (case, expected) in report["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(inventory["cases"].as_array().unwrap())
    {
        assert_eq!(case["id"], expected["id"]);
        assert_eq!(case["sourceSha256"], expected["sha256"]);
    }
}
#[test]
fn supported_case_is_observed_through_fresh_worker_and_strict_mode_succeeds() {
    let fixture = Fixture::new(SOURCE);
    let (output, report) = fixture.run(&["--require-supported"]);
    assert!(output.status.success());
    assert_eq!(report["counts"]["passed"], 1);
    assert_eq!(report["cases"][0]["observation"]["kind"], "executed");
    assert_eq!(
        report["cases"][0]["observation"]["lines"],
        json!(["varta log: 2"])
    );
}
#[test]
fn known_unsupported_is_baseline_matching_but_strict_readiness_fails() {
    let mut fixture = Fixture::new("export function main():void{console.log(typeof Symbol('x'));}");
    fixture.unsupported("unsupported_syntax");
    let (output, report) = fixture.run(&[]);
    assert!(output.status.success());
    assert_eq!(report["baselineMatches"], true);
    assert_eq!(report["counts"]["knownUnsupported"], 1);
    assert_eq!(report["counts"]["passed"], 0);
    let (output, _) = fixture.run(&["--require-supported"]);
    assert_eq!(output.status.code(), Some(1));
}
#[test]
fn untested_features_prevent_strict_readiness_even_when_selected_cases_pass() {
    let mut fixture = Fixture::new(SOURCE);
    let mut feature = fixture.value["features"][0].clone();
    feature["id"] = json!("future");
    fixture.value["features"]
        .as_array_mut()
        .unwrap()
        .push(feature);
    fixture.save();
    let (output, report) = fixture.run(&["--require-supported"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(report["baselineMatches"], true);
    assert_eq!(report["untestedFeatures"], json!(["future"]));
}
#[test]
fn wrong_observed_output_is_a_semantic_regression() {
    let mut fixture = Fixture::new(SOURCE);
    fixture.value["cases"][0]["stdout"] = json!(["varta log: 999"]);
    fixture.save();
    let (output, report) = fixture.run(&[]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(report["counts"]["regressions"], 1);
    assert_eq!(report["counts"]["passed"], 0);
    assert_eq!(
        report["cases"][0]["observation"]["lines"],
        json!(["varta log: 2"])
    );
}
#[test]
fn unexpected_support_requires_explicit_baseline_review() {
    let mut fixture = Fixture::new(SOURCE);
    fixture.unsupported("unsupported_syntax");
    let (output, report) = fixture.run(&[]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(report["counts"]["unexpectedSupport"], 1);
    assert_eq!(report["counts"]["passed"], 0);
    assert_eq!(report["baselineMatches"], false);
}
#[test]
fn changed_diagnostic_is_not_silently_accepted_as_the_same_gap() {
    let mut fixture = Fixture::new("export function main():void{console.log(typeof Symbol('x'));}");
    fixture.unsupported("type_error");
    let (output, report) = fixture.run(&[]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(report["counts"]["regressions"], 1);
    assert_eq!(report["counts"]["knownUnsupported"], 0);
    assert_eq!(
        report["cases"][0]["observation"]["code"],
        "unsupported_syntax"
    );
}
#[test]
fn runtime_exceptions_and_fuel_exhaustion_are_failures() {
    for (source, category) in [
        (
            "export function main():void{throw Error('boom');}",
            "uncaught_exception",
        ),
        ("export function main():void{while(true){}}", "execution"),
    ] {
        let fixture = Fixture::new(source);
        let (output, report) = fixture.run(&[]);
        assert_eq!(output.status.code(), Some(1));
        assert_eq!(report["counts"]["regressions"], 1);
        assert_eq!(report["cases"][0]["observation"]["kind"], "runtime_failed");
        assert_eq!(report["cases"][0]["observation"]["category"], category);
    }
}
/// A case runs with exactly its reviewed grants (package request and host
/// ceiling): the v36 timer case passes with `time.timer` and is a denied
/// runtime failure without it; any other, unsorted or duplicate grant fails
/// inventory loading.
#[test]
fn reviewed_case_grants_are_exact_and_limited_to_timers() {
    let mut fixture = Fixture::new(
        "export async function main():Promise<void>{await new Promise<void>((resolve)=>setTimeout(resolve,1));console.log(2);}",
    );
    fixture.value["profile"] = json!("checked-f64-utf16-v36");
    fixture.save();
    let (output, report) = fixture.run(&[]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(report["cases"][0]["observation"]["kind"], "runtime_failed");
    assert!(
        report["cases"][0]["observation"]["detail"]
            .as_str()
            .unwrap()
            .contains("time.timer")
    );
    fixture.value["cases"][0]["grants"] = json!(["time.timer"]);
    fixture.save();
    let (output, report) = fixture.run(&["--require-supported"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(report["counts"]["passed"], 1);
    for grants in [
        json!(["time.now"]),
        json!(["fs.read"]),
        json!(["process.exit"]),
        json!(["time.timer", "time.timer"]),
        json!(["unknown"]),
        json!([""]),
    ] {
        let mut v = fixture.value.clone();
        v["cases"][0]["grants"] = grants.clone();
        fs::write(fixture.root.join("suite.json"), v.to_string()).unwrap();
        assert!(
            varta_conformance::LoadedSuite::load(&fixture.root.join("suite.json")).is_err(),
            "{grants}"
        );
    }
}
#[test]
fn host_access_is_denied_and_parent_environment_is_not_disclosed() {
    let mut fixture = Fixture::new(
        "export function main():void{varta.env.get('VARTA_CONFORMANCE_SECRET');console.log(varta.result.status());}",
    );
    fixture.value["cases"][0]["stdout"] = json!([
        "varta log: <capability denied: env> (VARTA_CONFORMANCE_SECRET)",
        "varta log: 1"
    ]);
    fixture.save();
    let output = fixture
        .command()
        .env("VARTA_CONFORMANCE_SECRET", "NEVER_DISCLOSE")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("NEVER_DISCLOSE"));
}
#[test]
fn altered_source_digest_is_rejected_before_execution() {
    let fixture = Fixture::new(SOURCE);
    fs::write(
        fixture.root.join("cases/input.ts"),
        "export function main():void{console.log('BAD');}",
    )
    .unwrap();
    let output = fixture.command().output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("digest mismatch"));
    assert!(output.stdout.is_empty());
}
#[test]
fn loaded_source_snapshot_is_not_reread_during_execution() {
    let fixture = Fixture::new(SOURCE);
    let suite = varta_conformance::LoadedSuite::load(&fixture.root.join("suite.json")).unwrap();
    fs::write(
        fixture.root.join("cases/input.ts"),
        "export function main():void{throw Error('BAD');}",
    )
    .unwrap();
    let report =
        varta_conformance::run_suite(&suite, Path::new(executable()), Duration::from_secs(15))
            .unwrap();
    assert!(report.baseline_matches);
    assert_eq!(report.counts.passed, 1);
}
#[test]
fn wall_timeout_kills_and_reaps_a_worker_while_its_input_is_being_written() {
    let source = format!("/*{}*/{SOURCE}", "x".repeat(1024 * 1024 - 200));
    let fixture = Fixture::new(&source);
    let started = Instant::now();
    let (output, report) = fixture.run(&["--timeout-ms", "1"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(report["counts"]["infrastructureFailures"], 1);
    assert_eq!(report["cases"][0]["observation"]["category"], "timeout");
    assert!(started.elapsed() < Duration::from_secs(10));
}
#[test]
fn excessive_worker_output_is_bounded_and_never_a_semantic_pass() {
    let source = format!(
        "export function main():void{{for(let i=0;i<700;i++)console.log('{}');}}",
        "x".repeat(100)
    );
    let fixture = Fixture::new(&source);
    let (output, report) = fixture.run(&[]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(report["counts"]["infrastructureFailures"], 1);
    assert_eq!(
        report["cases"][0]["observation"]["category"],
        "output_limit"
    );
    assert!(output.stdout.len() < 8192);
}
#[test]
fn unknown_versions_fields_profiles_and_duplicate_metadata_fail_closed() {
    let fixture = Fixture::new(SOURCE);
    let original = fixture.value.clone();
    for (field, value) in [
        ("schemaVersion", json!(2)),
        ("edition", json!("ECMAScript 2026")),
        ("sourceModel", json!("test262")),
        ("profile", json!("unchecked-future")),
        ("unknown", json!(true)),
    ] {
        let mut value_copy = original.clone();
        value_copy[field] = value;
        fs::write(fixture.root.join("suite.json"), value_copy.to_string()).unwrap();
        assert!(
            varta_conformance::LoadedSuite::load(&fixture.root.join("suite.json")).is_err(),
            "{field}"
        );
    }
    let duplicate = original.to_string().replacen(
        "\"schemaVersion\":1",
        "\"schemaVersion\":1,\"schemaVersion\":1",
        1,
    );
    fs::write(fixture.root.join("suite.json"), duplicate).unwrap();
    assert!(varta_conformance::LoadedSuite::load(&fixture.root.join("suite.json")).is_err());
}
#[test]
fn invalid_inventory_mappings_and_baselines_cannot_hide_omissions_or_internal_errors() {
    let fixture = Fixture::new(SOURCE);
    let mut variants = Vec::new();
    let mut v = fixture.value.clone();
    v["cases"] = json!([]);
    variants.push(v);
    let mut v = fixture.value.clone();
    let duplicate = v["cases"][0].clone();
    v["cases"].as_array_mut().unwrap().push(duplicate);
    variants.push(v);
    let mut v = fixture.value.clone();
    v["cases"][0]["feature"] = json!("unknown");
    variants.push(v);
    let mut v = fixture.value.clone();
    v["features"][0]["core"] = json!(["CORE-99"]);
    variants.push(v);
    let mut v = fixture.value.clone();
    v["cases"][0]["baseline"] = json!({"status":"known_unsupported","diagnostic":"internal_error","reason":"Hide a compiler bug"});
    variants.push(v);
    let mut v = fixture.value.clone();
    v["cases"][0]["baseline"] =
        json!({"status":"known_unsupported","diagnostic":"unsupported_syntax","reason":""});
    variants.push(v);
    let mut v = fixture.value.clone();
    v["cases"][0]["stdout"] = json!(["x".repeat(16 * 1024 + 1)]);
    variants.push(v);
    for v in variants {
        fs::write(fixture.root.join("suite.json"), v.to_string()).unwrap();
        assert!(varta_conformance::LoadedSuite::load(&fixture.root.join("suite.json")).is_err());
    }
}
#[test]
fn source_paths_cannot_escape_or_alias_relative_components() {
    let fixture = Fixture::new(SOURCE);
    for path in [
        "../escape.ts",
        "/absolute.ts",
        "cases/../input.ts",
        "cases/./input.ts",
        "cases//input.ts",
        "cases\\input.ts",
    ] {
        let mut v = fixture.value.clone();
        v["cases"][0]["path"] = json!(path);
        fs::write(fixture.root.join("suite.json"), v.to_string()).unwrap();
        assert!(
            varta_conformance::LoadedSuite::load(&fixture.root.join("suite.json")).is_err(),
            "{path}"
        );
    }
}
#[cfg(unix)]
#[test]
fn sources_and_parent_directories_cannot_be_symlinks() {
    use std::os::unix::fs::symlink;
    for directory in [false, true] {
        let fixture = Fixture::new(SOURCE);
        if directory {
            fs::rename(fixture.root.join("cases"), fixture.root.join("real")).unwrap();
            symlink(fixture.root.join("real"), fixture.root.join("cases")).unwrap();
        } else {
            fs::rename(
                fixture.root.join("cases/input.ts"),
                fixture.root.join("real.ts"),
            )
            .unwrap();
            symlink(
                fixture.root.join("real.ts"),
                fixture.root.join("cases/input.ts"),
            )
            .unwrap();
        }
        assert!(varta_conformance::LoadedSuite::load(&fixture.root.join("suite.json")).is_err());
    }
}
#[test]
fn source_byte_limit_and_utf8_validation_precede_workers() {
    let mut fixture = Fixture::new(SOURCE);
    for bytes in [vec![b'x'; 1024 * 1024 + 1], vec![0xff]] {
        fs::write(fixture.root.join("cases/input.ts"), &bytes).unwrap();
        fixture.value["cases"][0]["sha256"] = json!(digest(&bytes));
        fixture.save();
        assert!(varta_conformance::LoadedSuite::load(&fixture.root.join("suite.json")).is_err());
    }
}
#[test]
fn malformed_worker_protocol_is_rejected_without_execution() {
    let mut child = Command::new(executable())
        .arg("--worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(br#"{"profile":"checked-f64-utf16-v26","source":"export function main():void{}","unknown":true}"#).unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown field"));
}

#[test]
fn corpus_files_cannot_be_silently_omitted_from_the_inventory() {
    let fixture = Fixture::new(SOURCE);
    fs::write(fixture.root.join("cases/omitted.ts"), SOURCE).unwrap();
    let error = varta_conformance::LoadedSuite::load(&fixture.root.join("suite.json"))
        .err()
        .unwrap();
    assert!(error.to_string().contains("unlisted"));
}
#[test]
fn oversized_manifest_and_case_counts_are_rejected_before_sources() {
    let fixture = Fixture::new(SOURCE);
    fs::write(fixture.root.join("suite.json"), vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert!(
        varta_conformance::LoadedSuite::load(&fixture.root.join("suite.json"))
            .err()
            .unwrap()
            .to_string()
            .contains("byte limit")
    );
    let mut value = fixture.value.clone();
    value["cases"] = json!(vec![value["cases"][0].clone(); 257]);
    fs::write(fixture.root.join("suite.json"), value.to_string()).unwrap();
    assert!(
        varta_conformance::LoadedSuite::load(&fixture.root.join("suite.json"))
            .err()
            .unwrap()
            .to_string()
            .contains("oversized")
    );
}
#[test]
fn total_source_bytes_are_bounded_across_individually_valid_files() {
    let source = format!("/*{}*/{SOURCE}", "x".repeat(1024 * 1024 - 200));
    let mut fixture = Fixture::new(&source);
    fs::remove_file(fixture.root.join("cases/input.ts")).unwrap();
    let mut cases = Vec::new();
    for i in 0..17 {
        let mut case = fixture.value["cases"][0].clone();
        let path = format!("cases/large-{i}.ts");
        case["path"] = json!(path);
        case["id"] = json!(format!("large-{i}"));
        fs::write(fixture.root.join(path), &source).unwrap();
        cases.push(case);
    }
    fixture.value["cases"] = json!(cases);
    fixture.save();
    let error = varta_conformance::LoadedSuite::load(&fixture.root.join("suite.json"))
        .err()
        .unwrap();
    assert!(error.to_string().contains("16 MiB"));
}
#[test]
fn worker_request_and_source_bytes_have_independent_limits() {
    let error =
        varta_conformance::run_worker(std::io::Cursor::new(vec![b' '; 8 * 1024 * 1024 + 1]))
            .unwrap_err();
    assert!(error.to_string().contains("byte limit"));
    let request = json!({"profile":"checked-f64-utf16-v26","source":"x".repeat(1024*1024+1)});
    let observation =
        varta_conformance::run_worker(std::io::Cursor::new(request.to_string())).unwrap();
    assert!(matches!(
        observation,
        varta_conformance::Observation::Infrastructure {
            category: varta_conformance::InfrastructureFailure::Protocol,
            ..
        }
    ));
}

#[test]
fn public_runner_rejects_unbounded_and_fractional_timeouts_before_launch() {
    let fixture = Fixture::new(SOURCE);
    let suite = varta_conformance::LoadedSuite::load(&fixture.root.join("suite.json")).unwrap();
    for timeout in [
        Duration::ZERO,
        Duration::from_secs(61),
        Duration::from_nanos(1_500_000),
    ] {
        let error =
            varta_conformance::run_suite(&suite, Path::new("missing-worker"), timeout).unwrap_err();
        assert!(error.to_string().contains("timeout"));
    }
}
