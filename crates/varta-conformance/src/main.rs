#![forbid(unsafe_code)]
use clap::Parser;
use std::{path::PathBuf, time::Duration};
#[derive(Parser)]
#[command(about = "Run digest-pinned checked ECMAScript cases; does not run Test262")]
struct Args {
    #[arg(long, default_value = "conformance/core-v1.json")]
    suite: PathBuf,
    #[arg(long)]
    json: bool,
    /// Fail on known unsupported cases and inventory features without a case.
    #[arg(long)]
    require_supported: bool,
    /// Wall limit includes source compilation, validation and default-budget execution.
    #[arg(long,default_value_t=15_000,value_parser=clap::value_parser!(u64).range(1..=60_000))]
    timeout_ms: u64,
    #[arg(long, hide = true)]
    worker: bool,
}
fn main() -> std::process::ExitCode {
    match execute() {
        Ok(code) => std::process::ExitCode::from(code),
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::from(2)
        }
    }
}
fn execute() -> Result<u8, Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.worker {
        let result = varta_conformance::run_worker(std::io::stdin().lock())?;
        serde_json::to_writer(std::io::stdout().lock(), &result)?;
        return Ok(0);
    }
    let suite = varta_conformance::LoadedSuite::load(&args.suite)?;
    let report = varta_conformance::run_suite(
        &suite,
        &std::env::current_exe()?,
        Duration::from_millis(args.timeout_ms),
    )?;
    if args.json {
        serde_json::to_writer_pretty(std::io::stdout().lock(), &report)?;
        println!();
    } else {
        println!(
            "{} / {} / {:?}",
            report.suite, report.source_model, report.profile
        );
        for case in &report.cases {
            println!("{:?}: {}", case.status, case.id);
        }
        println!(
            "{} passed; {} known unsupported; {} unexpected support; {} regressions; {} infrastructure failures; {} untested features",
            report.counts.passed,
            report.counts.known_unsupported,
            report.counts.unexpected_support,
            report.counts.regressions,
            report.counts.infrastructure_failures,
            report.untested_features.len()
        );
        println!(
            "Baseline matches: {}. Results cover this checked-case inventory only.",
            report.baseline_matches
        );
    }
    Ok(u8::from(
        !report.baseline_matches || args.require_supported && !report.all_cases_supported(),
    ))
}
