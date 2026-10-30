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
    ///
    /// With --profile-fuel, also compile an instrumented profile build, run it
    /// instead of the ordinary build (same grants, policy and budgets; the
    /// probes refund their own fuel) and print the fuel of every guest
    /// function, engine prelude and compiler helper to stderr, by descending
    /// self fuel. A trap or an exhausted budget still prints the profile up to
    /// that point. Profile builds are never written or packaged.
    Dev {
        #[arg(long, default_value = "varta.config.json")]
        config: PathBuf,
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
    Run { path: PathBuf },
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

fn output(result: varta_project::RunOutput) -> i32 {
    for line in result.lines {
        println!("{line}");
    }
    result.exit_code
}

fn execute(command: Option<Commands>, policy_path: Option<&Path>) -> Result<i32, ProjectError> {
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
            profile_fuel: true,
            json,
        }) => {
            let project = CheckedProject::load(&config)?;
            let run = project.profile_fuel(&config, policy.as_ref())?;
            let code = output(run.output);
            report_profile(&run.profile, json)?;
            if let Some(error) = run.error {
                return Err(error.into());
            }
            println!("varta dev: completed");
            return Ok(code);
        }
        Some(Commands::Dev { config, .. }) => {
            let project = CheckedProject::load(&config)?;
            let result = run(&project.artifact, policy.as_ref())?;
            let code = output(result);
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
        Some(Commands::Run { path }) => {
            let artifact = Artifact::read(&path)?;
            let result = run(&artifact, policy.as_ref())?;
            let code = output(result);
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
        None => {
            return Ok(output(run(
                &Artifact::read(&std::env::current_exe()?)?,
                policy.as_ref(),
            )?));
        }
    }
    Ok(0)
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
    varta_project::run_observed(
        artifact,
        policy,
        std::sync::Arc::new(|event| match event {
            varta_project::RuntimeEvent::Listening { address, .. } => {
                eprintln!("varta listening: http://{address}")
            }
            varta_project::RuntimeEvent::Output(line) => println!("{line}"),
            varta_project::RuntimeEvent::Diagnostic(message) => eprintln!("varta HTTP: {message}"),
        }),
    )
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
    match execute(cli.command, cli.policy.as_deref()) {
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
