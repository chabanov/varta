#![forbid(unsafe_code)]
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use varta_project::{Artifact, CheckedProject, HostPolicy, ProjectError};

#[derive(Parser)]
#[command(
    name = "varta",
    version,
    about = "Varta secure TypeScript subset compiler and runtime"
)]
struct Cli {
    /// Limit run/dev/invoke/inspect/validate (or this packaged app) with a host policy JSON file.
    #[arg(long, global = true, value_name = "PATH")]
    policy: Option<PathBuf>,
    /// Stored permission decisions for run/dev (or this packaged app): the headless policy file, keyed by artifact hash (checked-f64-utf16-v38).
    #[arg(long, global = true, value_name = "PATH")]
    decisions: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new project (refuses to overwrite an existing directory).
    Init { name: PathBuf },
    /// Check the supported language, types, policy schema and emitted Wasm.
    Check {
        #[arg(long, default_value = "varta.config.json")]
        config: PathBuf,
        #[arg(long)]
        json: bool,
    },
    /// Check, compile and execute portable Wasm with resource budgets.
    /// Output captured before a failure is printed before the diagnostic.
    ///
    /// With --profile-fuel, also compile an instrumented profile build, run it
    /// instead of the ordinary build (same grants, policy, budgets, decisions
    /// and execution flags; the probes refund their own fuel) and print the
    /// fuel of every guest function, engine prelude and compiler helper to
    /// stderr, by descending self fuel, summed over every turn. A trap or an
    /// exhausted budget still prints the profile up to that point; with
    /// --budget-report the per-turn report follows it. Profile builds are
    /// never written or packaged.
    Dev {
        #[arg(long, default_value = "varta.config.json")]
        config: PathBuf,
        #[command(flatten)]
        execution: ExecutionFlags,
        /// Print per-function fuel (self, total, calls) of this run to stderr.
        #[arg(long)]
        profile_fuel: bool,
        /// With --profile-fuel, print the profile as one JSON object instead.
        #[arg(long, requires = "profile_fuel")]
        json: bool,
    },
    /// Build validated Wasm; --single embeds portable Wasm in a native launcher.
    Build {
        #[arg(long, default_value = "varta.config.json")]
        config: PathBuf,
        #[arg(long)]
        single: bool,
        /// Use this trusted native runtime companion instead of the sibling varta-runtime.
        #[arg(long, requires = "single", value_name = "PATH")]
        launcher: Option<PathBuf>,
    },
    /// Execute portable Wasm or a Varta container (native AOT is rejected).
    /// Output captured before a failure is printed before the diagnostic.
    Run {
        path: PathBuf,
        #[command(flatten)]
        execution: ExecutionFlags,
    },
    /// Invoke one published tool with checked JSON parameters, without running main.
    Invoke {
        path: PathBuf,
        name: String,
        /// One named-argument JSON object (at most 64 KiB; default is an empty object).
        #[arg(long, default_value = "{}", value_name = "JSON")]
        params: String,
        /// Return one JSON record containing the result or a stable invocation error code.
        #[arg(long)]
        json: bool,
    },
    /// Inspect validated bytecode, ABI, public tool contracts and full capability scopes without running the app.
    Inspect {
        path: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Validate bytecode, ABI, manifest and required capability declarations.
    Validate {
        path: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(clap::Args)]
struct ExecutionFlags {
    /// Print each turn's fuel, host calls, wall time and memory (and the totals) to stderr after the run.
    #[arg(long)]
    budget_report: bool,
    /// Run a checked-f64-utf16-v38 program as a long-lived application instance: the deadline renews per turn and output streams per turn.
    #[arg(long)]
    app: bool,
}

/// Print the output captured before any failure, then the budget report,
/// then return the failure (printed by `main` as the diagnostic).
fn finish(report: varta_project::RunReport, budget_report: bool) -> Result<i32, ProjectError> {
    for line in &report.output.lines {
        println!("{line}");
    }
    if budget_report && let Some(budget) = &report.budget {
        for line in budget.lines() {
            eprintln!("{line}");
        }
    }
    match report.failure {
        Some(error) => Err(error.into()),
        None => Ok(report.output.exit_code),
    }
}

fn output(result: varta_project::RunOutput) -> i32 {
    for line in result.lines {
        println!("{line}");
    }
    result.exit_code
}

fn execute(
    command: Option<Commands>,
    policy_path: Option<&Path>,
    decisions_path: Option<&Path>,
) -> Result<i32, ProjectError> {
    if decisions_path.is_some()
        && !matches!(
            command,
            None | Some(Commands::Run { .. } | Commands::Dev { .. })
        )
    {
        return Err(ProjectError::Usage(
            "--decisions applies to run, dev or a packaged app".into(),
        ));
    }
    let decisions = decisions_path
        .map(varta_project::read_decisions)
        .transpose()?;
    if policy_path.is_some()
        && matches!(
            command,
            Some(Commands::Init { .. } | Commands::Check { .. } | Commands::Build { .. })
        )
    {
        return Err(ProjectError::Usage("--policy applies to run, dev, invoke, inspect, validate, or a packaged app; it is never embedded in a build".into()));
    }
    let policy = policy_path.map(HostPolicy::read).transpose()?;

    match command {
        Some(Commands::Init { name }) => {
            varta_project::init(&name)?;
            println!("initialized varta project: {}", name.display());
        }
        Some(Commands::Check { config, json }) => {
            let project = CheckedProject::load(&config)?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({"status":"ok", "type_check":"internal", "diagnostics":[], "entry":project.config.entry, "modules":project.modules,
                "policy":project.config.policy, "target":project.config.target, "languageProfile":project.config.language_profile, "abiVersion":project.artifact.manifest.abi_version, "capabilities":project.artifact.manifest.capabilities})
                );
            } else {
                println!("varta check: ok ({})", config.display());
            }
        }
        Some(Commands::Dev {
            config,
            execution,
            profile_fuel: true,
            json,
        }) => {
            let project = CheckedProject::load(&config)?;
            let mut report =
                project.profile_fuel(&config, policy.as_ref(), settings(decisions, &execution))?;
            if let Some(profile) = report.fuel_profile.take() {
                report_profile(&profile, json)?;
            }
            let code = finish(report, execution.budget_report)?;
            println!("varta dev: completed");
            return Ok(code);
        }
        Some(Commands::Dev {
            config, execution, ..
        }) => {
            let project = CheckedProject::load(&config)?;
            let report = run_report(&project.artifact, policy.as_ref(), decisions, &execution)?;
            let code = finish(report, execution.budget_report)?;
            println!("varta dev: completed");
            return Ok(code);
        }
        Some(Commands::Build {
            config,
            single,
            launcher,
        }) => {
            let project = CheckedProject::load(&config)?;
            let executable = std::env::current_exe()?;
            let launcher = launcher.unwrap_or_else(|| varta_project::runtime_launcher(&executable));
            for path in project.build(single, &launcher)? {
                println!("wrote artifact: {}", path.display());
            }
        }
        Some(Commands::Run { path, execution }) => {
            let artifact = Artifact::read(&path)?;
            let report = run_report(&artifact, policy.as_ref(), decisions, &execution)?;
            let code = finish(report, execution.budget_report)?;
            println!("varta run: completed {}", path.display());
            return Ok(code);
        }
        Some(Commands::Invoke {
            path,
            name,
            params,
            json,
        }) => {
            let artifact = Artifact::read(&path)?;
            let output = match &policy {
                Some(policy) => varta_project::invoke_tool_with_policy(
                    &artifact,
                    &name,
                    params.as_bytes(),
                    policy,
                )?,
                None => varta_project::invoke_tool(&artifact, &name, params.as_bytes())?,
            };
            if json {
                println!(
                    "{}",
                    serde_json::json!({"status":"ok", "result":output.result, "logs":output.logs})
                );
            } else {
                for line in output.logs {
                    eprintln!("{line}");
                }
                println!(
                    "{}",
                    serde_json::to_string(&output.result)
                        .map_err(|e| ProjectError::Usage(e.to_string()))?
                );
            }
        }
        Some(Commands::Inspect { path, json }) => {
            let path = path.unwrap_or_else(|| "dist/app".into());
            let metadata = varta_project::inspect_with_policy(
                &Artifact::read(&path)?,
                &path,
                policy.as_ref(),
            )?;
            if json {
                println!("{metadata}");
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&metadata)
                        .map_err(|e| ProjectError::Artifact(e.to_string()))?
                );
            }
        }
        Some(Commands::Validate { path, json }) => {
            let path = path.unwrap_or_else(|| "dist/app".into());
            let artifact = Artifact::read(&path)?;
            varta_project::validate(&artifact)?;
            let authorization = varta_project::authorization(&artifact.manifest, policy.as_ref())?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({"status":"ok", "artifact":path, "authorization":authorization})
                );
            } else {
                println!("varta validate: ok ({})", path.display());
            }
        }
        None if decisions.is_some() => {
            let artifact = Artifact::read(&std::env::current_exe()?)?;
            let flags = ExecutionFlags {
                budget_report: false,
                app: false,
            };
            let report = run_report(&artifact, policy.as_ref(), decisions, &flags)?;
            return finish(report, false);
        }
        None => {
            return Ok(output(run(
                &Artifact::read(&std::env::current_exe()?)?,
                policy.as_ref(),
            )?));
        }
    }
    Ok(0)
}

fn observer() -> varta_project::RuntimeObserver {
    std::sync::Arc::new(|event| match event {
        varta_project::RuntimeEvent::Listening { address, .. } => {
            eprintln!("varta listening: http://{address}")
        }
        varta_project::RuntimeEvent::Output(line) => println!("{line}"),
        varta_project::RuntimeEvent::Diagnostic(message) => eprintln!("varta HTTP: {message}"),
    })
}

/// The fuel profile table (or JSON object) on stderr, keeping stdout for the
/// program's own output.
fn report_profile(profile: &varta_project::FuelProfile, json: bool) -> Result<(), ProjectError> {
    if json {
        eprintln!(
            "{}",
            serde_json::to_string(profile).map_err(|e| ProjectError::Usage(e.to_string()))?
        );
        return Ok(());
    }
    eprintln!(
        "varta fuel profile: {} of {} fuel ({} outside guest functions{})",
        profile.consumed,
        profile.budget,
        profile.outside,
        if profile.unfinished > 0 {
            format!(", {} activations unfinished", profile.unfinished)
        } else {
            String::new()
        }
    );
    eprintln!("{:>10} {:>10} {:>8}  function", "self", "total", "calls");
    for row in &profile.functions {
        eprintln!(
            "{:>10} {:>10} {:>8}  {}",
            row.self_fuel, row.total_fuel, row.calls, row.name
        );
    }
    Ok(())
}

fn run(
    artifact: &Artifact,
    policy: Option<&HostPolicy>,
) -> Result<varta_project::RunOutput, ProjectError> {
    varta_project::run_observed(artifact, policy, observer())
}

fn run_report(
    artifact: &Artifact,
    policy: Option<&HostPolicy>,
    decisions: Option<varta_project::DecisionFile>,
    flags: &ExecutionFlags,
) -> Result<varta_project::RunReport, ProjectError> {
    varta_project::run_report(artifact, policy, settings(decisions, flags))
}

/// How the execution flags run a program.
fn settings(
    decisions: Option<varta_project::DecisionFile>,
    flags: &ExecutionFlags,
) -> varta_project::RunSettings {
    varta_project::RunSettings {
        application: flags.app,
        decisions,
        observer: Some(observer()),
    }
}

fn main() {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    let json = matches!(
        &cli.command,
        Some(
            Commands::Check { json: true, .. }
                | Commands::Validate { json: true, .. }
                | Commands::Inspect { json: true, .. }
                | Commands::Invoke { json: true, .. }
        )
    );
    match execute(cli.command, cli.policy.as_deref(), cli.decisions.as_deref()) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({"status":"failed", "error":error.to_string(), "code":error.invocation_code(), "diagnostics": error.diagnostic().into_iter().collect::<Vec<_>>()})
                );
            }
            eprintln!("varta failed: {error}");
            std::process::exit(1);
        }
    }
}
