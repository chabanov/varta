#![forbid(unsafe_code)]
//! Developer-only checked-case compatibility inventory; not a Test262 adapter.
mod inventory;
mod model;
mod process;
mod worker;
pub use inventory::LoadedSuite;
pub use model::*;
use std::{io::Read, path::Path, time::Duration};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("inventory: {0}")]
    Inventory(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
fn classify(case: &Case, observed: &Observation) -> CaseStatus {
    match observed {
        Observation::Infrastructure { .. } => CaseStatus::Infrastructure,
        Observation::CompileRejected { code, .. }
            if case.baseline.status == BaselineStatus::KnownUnsupported
                && Some(*code) == case.baseline.diagnostic =>
        {
            CaseStatus::KnownUnsupported
        }
        Observation::Executed {
            lines,
            exit_code: 0,
        } if lines == &case.stdout => {
            if case.baseline.status == BaselineStatus::Supported {
                CaseStatus::Passed
            } else {
                CaseStatus::UnexpectedSupport
            }
        }
        _ => CaseStatus::Regression,
    }
}
pub fn run_suite(
    suite: &LoadedSuite,
    executable: &Path,
    timeout: Duration,
) -> Result<Report, Error> {
    if !(Duration::from_millis(1)..=Duration::from_secs(60)).contains(&timeout)
        || !timeout.subsec_nanos().is_multiple_of(1_000_000)
    {
        return Err(Error::Inventory(
            "worker timeout must be 1..=60000 whole milliseconds".into(),
        ));
    }
    let i = &suite.inventory;
    let mut counts = Counts::default();
    let mut cases = Vec::new();
    for (case, source) in i.cases.iter().zip(&suite.sources) {
        let observation = process::invoke(
            executable,
            model::Request {
                profile: i.profile,
                source: source.clone(),
                grants: case.grants.clone(),
            },
            timeout,
        );
        let status = classify(case, &observation);
        counts.add(status);
        cases.push(CaseReport {
            id: case.id.clone(),
            feature: case.feature.clone(),
            path: case.path.clone(),
            source_sha256: case.sha256.clone(),
            status,
            baseline_reason: case.baseline.reason.clone(),
            observation,
        });
    }
    let mut untested_features = Vec::new();
    let mut features = Vec::new();
    for feature in &i.features {
        let mut counts = Counts::default();
        for case in &cases {
            if case.feature == feature.id {
                counts.add(case.status);
            }
        }
        if counts.total == 0 {
            untested_features.push(feature.id.clone());
        }
        features.push(FeatureReport {
            feature: feature.id.clone(),
            title: feature.title.clone(),
            reference: feature.reference.clone(),
            origin: feature.origin,
            core: feature.core.clone(),
            counts,
        });
    }
    Ok(Report {
        schema_version: 1,
        suite: i.id.clone(),
        edition: i.edition.clone(),
        source_model: i.source_model.clone(),
        profile: i.profile,
        inventory_sha256: suite.digest.clone(),
        dependency_lock_sha256: inventory::sha256(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../Cargo.lock"
        ))),
        worker_timeout_ms: u64::try_from(timeout.as_millis()).expect("validated timeout"),
        runtime_budgets: RuntimeBudgets {
            fuel: varta_common::abi::FUEL,
            memory_bytes: varta_common::abi::MAX_MEMORY_BYTES,
            host_calls: varta_common::abi::MAX_HOST_CALLS,
            output_bytes: varta_common::abi::MAX_OUTPUT_BYTES,
            deadline_seconds: varta_common::abi::DEADLINE_SECONDS,
        },
        package_version: env!("CARGO_PKG_VERSION").into(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        baseline_matches: counts.known_unsupported + counts.passed == counts.total,
        counts,
        untested_features,
        features,
        cases,
    })
}
/// Private worker protocol exposed only to this package's thin executable.
pub fn run_worker(input: impl std::io::Read) -> Result<Observation, Error> {
    let mut bytes = Vec::new();
    input.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(Error::Inventory("worker request exceeds byte limit".into()));
    }
    let request: model::Request = serde_json::from_slice(&bytes)?;
    Ok(worker::execute(request))
}
