#![forbid(unsafe_code)]
use clap::{Parser, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use varta_project::{Artifact, CheckedProject, HostPolicy, ProjectError};

/// The values of one `--view-font`: a font file path and its CSS family.
const VIEW_FONT_VALUES: usize = 2;

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
    ///
    /// A checked-f64-utf16-v38 program reads files at virtual paths
    /// /<root>/... under the roots of the configuration's "fs": {"roots"}
    /// (directories inside the configuration directory, pinned at startup);
    /// a --policy that maps "fsRoots" replaces them.
    Dev {
        #[arg(long, default_value = "varta.config.json")]
        config: PathBuf,
        #[command(flatten)]
        execution: ExecutionFlags,
        /// Execute natively (default) or serve a local browser app with rebuild/reload.
        #[arg(long, value_enum)]
        target: Option<DevTarget>,
        /// Browser development listener; loopback only, default 127.0.0.1:0.
        #[arg(long, value_name = "IP:PORT", requires = "target")]
        listen: Option<std::net::SocketAddr>,
        /// Print per-function fuel (self, total, calls) of this run to stderr.
        #[arg(long)]
        profile_fuel: bool,
        /// With --profile-fuel, print the profile as one JSON object instead.
        #[arg(long, requires = "profile_fuel")]
        json: bool,
        /// Run a v38 view application headlessly as an application instance:
        /// perform this JSON script's platform inputs (clicks, typing, keys
        /// with modifiers, focus, select, scroll, prompt answers) as trusted
        /// events between turns, every record as its own turn, then print the
        /// final view tree and accessibility snapshot as JSON. The session
        /// runs on virtual time: an input is performed only when no task is
        /// runnable (every record, arrived network completion and due timer
        /// has run, including zero-delay timers and their chains, each as its
        /// own turn); {"input":"wait","ms":N} advances virtual time by N
        /// milliseconds and fires every timer due by then. An explicit
        /// {"input":"wait_network","ms":N} waits for armed network work
        /// under a bounded wall-clock deadline without advancing virtual
        /// timers. Other steps never wait for requests implicitly.
        #[arg(long, value_name = "PATH", conflicts_with = "profile_fuel")]
        view_script: Option<PathBuf>,
        /// With --view-script, add a "geometry" section to the printed
        /// snapshot (the rest is unchanged): the viewport, the base text size
        /// and, per displayed element in document order, its id, used display
        /// (an inline child of a flex or grid container is "block", while
        /// the tree keeps its cascaded style) and border-box rectangle
        /// x, y, width, height in CSS pixels. Layout uses the script's
        /// viewport width, the default headless viewport height and only the
        /// --view-font faces, never OS fonts. The viewport width must have an
        /// exact single-precision value (300 or 300.5, not 300.1); otherwise
        /// the run fails before the session starts. Unsupported layout content
        /// or CSS fails the run.
        #[arg(long, requires = "view_script", requires = "view_font")]
        view_geometry: bool,
        /// With --view-geometry: an explicit font file and the CSS family it
        /// provides; repeat for more faces. The first family is the default
        /// when CSS names none.
        #[arg(
            long,
            num_args = VIEW_FONT_VALUES,
            value_names = ["PATH", "FAMILY"],
            requires = "view_geometry"
        )]
        view_font: Vec<String>,
        /// With --view-script or --app: perform real transfers and write
        /// every completion the instance received from them (heads, body
        /// chunks with their read mode, ends, failures) and every abort of
        /// an open transfer, in delivery order and with its script step (an
        /// --app run has no script), to this new network log file (never
        /// overwritten; also written when the instance fails after it
        /// started). Heads keep only the response
        /// headers the grant showed. The log is bounded; exceeding it fails
        /// the session. Only fetch requests are recorded: a module that
        /// imports varta.net.get or varta.net.fetch is refused before it
        /// starts, and no log is written.
        #[arg(
            long,
            value_name = "PATH",
            conflicts_with_all = ["network_replay", "profile_fuel", "target"]
        )]
        network_record: Option<PathBuf>,
        /// With --view-script or --app: serve this recorded network log
        /// instead of the network (no socket is opened). A module that imports
        /// varta.net.get or varta.net.fetch, which the log cannot serve, is
        /// refused before it starts. Every logged request must be granted,
        /// or the session fails before it starts; each request is
        /// still authorized when it starts and must match the next logged
        /// request's method and URL. Logged events reach the guest in log
        /// order, each once its operation is armed and the script reached
        /// its step (an --app run has no script, so only arming orders
        /// them), without waiting for the wall clock; timers keep the wall
        /// clock. A divergence, an armed request a wait_network step cannot
        /// complete from the log, or logged events left undelivered fail
        /// the instance.
        #[arg(
            long,
            value_name = "PATH",
            conflicts_with_all = ["profile_fuel", "target"]
        )]
        network_replay: Option<PathBuf>,
    },
    /// Build validated Wasm; --single embeds it in a native launcher.
    /// --target browser writes a checked console/view bundle to dist/web
    /// (web beside the configured output). Unsupported browser effects fail
    /// before writing. An existing browser directory is never overwritten.
    Build {
        #[arg(long, default_value = "varta.config.json")]
        config: PathBuf,
        #[arg(long, conflicts_with = "target")]
        single: bool,
        /// Distribution target; omission preserves the existing Wasm/--single behavior.
        #[arg(long, value_enum)]
        target: Option<BuildTarget>,
        /// Use this trusted native runtime companion instead of the sibling varta-runtime.
        #[arg(long, value_name = "PATH")]
        launcher: Option<PathBuf>,
    },
    /// Execute portable Wasm or a Varta container (native AOT is rejected).
    /// Output captured before a failure is printed before the diagnostic.
    ///
    /// A checked-f64-utf16-v38 artifact reads files only under the roots a
    /// --policy maps ("fsRoots": {"<root>": "/absolute/host/dir"}); an
    /// unmapped root is denied.
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

#[derive(Clone, Copy, ValueEnum)]
enum BuildTarget {
    Wasm,
    Native,
    Browser,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DevTarget {
    Native,
    Browser,
}

#[derive(clap::Args)]
struct ExecutionFlags {
    /// Print execution budgets to stderr; browser dev displays them in the page.
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
        console_line(&line);
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
        console_line(&line);
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
    if matches!(&command,Some(Commands::Dev{listen:Some(_),target,..}) if *target!=Some(DevTarget::Browser))
    {
        return Err(ProjectError::Usage(
            "--listen requires dev --target browser".into(),
        ));
    }

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
            target: Some(DevTarget::Browser),
            listen,
            profile_fuel,
            view_script,
            ..
        }) => {
            if profile_fuel || view_script.is_some() {
                return Err(ProjectError::Usage(
                    "browser dev does not accept native fuel profiling or view scripts".into(),
                ));
            }
            let mut options = varta_project::browser::BrowserDevOptions {
                application: execution.app,
                report_budgets: execution.budget_report,
                ..Default::default()
            };
            if let Some(address) = listen {
                options.address = address;
            }
            let server = varta_project::browser::BrowserDevServer::bind(
                &config,
                policy.as_ref(),
                decisions.as_ref(),
                options,
            )?;
            println!("varta dev: browser {}", server.url());
            server.run()?;
            return Ok(0);
        }
        Some(Commands::Dev {
            config,
            execution,
            view_script: Some(script),
            view_geometry,
            view_font,
            network_record,
            network_replay,
            ..
        }) => {
            let project = CheckedProject::load(&config)?;
            let script = varta_project::read_view_script(&script)?;
            let network = network_mode(network_record.as_deref(), network_replay.as_deref())?;
            let mut geometry = if view_geometry {
                let fonts: Vec<(PathBuf, String)> = view_font
                    .as_chunks::<VIEW_FONT_VALUES>()
                    .0
                    .iter()
                    .map(|[path, family]| (PathBuf::from(path), family.clone()))
                    .collect();
                Some(varta_project::read_view_fonts(&fonts)?)
            } else {
                None
            };
            let varta_project::ViewReport {
                mut report,
                snapshot,
                ..
            } = varta_project::run_view_with_geometry(
                &project.artifact,
                policy.as_ref(),
                project.dev_settings(varta_project::RunSettings {
                    network,
                    ..settings(decisions, &execution)
                }),
                &script,
                geometry.as_mut(),
            )?;
            if let (Some(path), Some(log)) = (&network_record, report.network_log.take()) {
                varta_project::write_network_log(path, &log)?;
            }
            // The output, the snapshot of a run that did not fail, then the
            // budget report and the failure.
            for line in std::mem::take(&mut report.output.lines) {
                console_line(&line);
            }
            let snapshot_json = if report.failure.is_none() {
                snapshot
                    .as_ref()
                    .map(|value| {
                        varta_project::encode_view_snapshot(
                            value,
                            varta_project::ViewSnapshotFormat::Pretty,
                        )
                    })
                    .transpose()
            } else {
                Ok(None)
            };
            if let Ok(Some(bytes)) = &snapshot_json {
                println!(
                    "{}",
                    std::str::from_utf8(bytes).expect("JSON encoding is UTF-8")
                );
            }
            let code = finish(report, execution.budget_report)?;
            snapshot_json?;
            println!("varta dev: completed");
            return Ok(code);
        }
        Some(Commands::Dev {
            config,
            execution,
            profile_fuel: true,
            json,
            ..
        }) => {
            let project = CheckedProject::load(&config)?;
            let mut report = project.profile_fuel(
                &config,
                policy.as_ref(),
                project.dev_settings(settings(decisions, &execution)),
            )?;
            if let Some(profile) = report.fuel_profile.take() {
                report_profile(&profile, json)?;
            }
            let code = finish(report, execution.budget_report)?;
            println!("varta dev: completed");
            return Ok(code);
        }
        Some(Commands::Dev {
            config,
            execution,
            network_record,
            network_replay,
            ..
        }) => {
            let recording = network_record.is_some() || network_replay.is_some();
            if recording && !execution.app {
                return Err(ProjectError::Usage(
                    "--network-record and --network-replay need --view-script or --app".into(),
                ));
            }
            let network = network_mode(network_record.as_deref(), network_replay.as_deref())?;
            let project = CheckedProject::load(&config)?;
            let mut report = varta_project::run_report(
                &project.artifact,
                policy.as_ref(),
                project.dev_settings(varta_project::RunSettings {
                    network,
                    ..settings(decisions, &execution)
                }),
            )?;
            if let (Some(path), Some(log)) = (&network_record, report.network_log.take()) {
                varta_project::write_network_log(path, &log)?;
            }
            let code = finish(report, execution.budget_report)?;
            println!("varta dev: completed");
            return Ok(code);
        }
        Some(Commands::Build {
            config,
            single,
            launcher,
            target,
        }) => {
            if launcher.is_some() && !single && !matches!(target, Some(BuildTarget::Native)) {
                return Err(ProjectError::Usage(
                    "--launcher requires --single or --target native".into(),
                ));
            }
            if matches!(target, Some(BuildTarget::Browser)) {
                for path in varta_project::browser::build_browser(&config)? {
                    println!("wrote artifact: {}", path.display());
                }
                return Ok(0);
            }
            let project = CheckedProject::load(&config)?;
            let executable = std::env::current_exe()?;
            let launcher = launcher.unwrap_or_else(|| varta_project::runtime_launcher(&executable));
            for path in project.build(
                single || matches!(target, Some(BuildTarget::Native)),
                &launcher,
            )? {
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

fn observer(budget_report: bool) -> varta_project::RuntimeObserver {
    std::sync::Arc::new(move |event| match event {
        varta_project::RuntimeEvent::Listening {
            address, scheme, ..
        } => eprintln!("varta listening: {scheme}://{address}"),
        varta_project::RuntimeEvent::Output(line) => console_line(&line),
        varta_project::RuntimeEvent::Diagnostic(message) => eprintln!("varta HTTP: {message}"),
        varta_project::RuntimeEvent::HttpRequest(report) => {
            if budget_report && let Some(budget) = &report.budget {
                eprintln!(
                    "varta HTTP request {}: {:?}",
                    report.sequence, report.outcome
                );
                for line in budget.lines() {
                    eprintln!("{line}");
                }
            }
        }
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
    varta_project::run_observed(artifact, policy, observer(false))
}

fn run_report(
    artifact: &Artifact,
    policy: Option<&HostPolicy>,
    decisions: Option<varta_project::DecisionFile>,
    flags: &ExecutionFlags,
) -> Result<varta_project::RunReport, ProjectError> {
    varta_project::run_report(artifact, policy, settings(decisions, flags))
}

/// The network mode of `--network-record`/`--network-replay`: a recording
/// never overwrites an existing file.
fn network_mode(
    record: Option<&std::path::Path>,
    replay: Option<&std::path::Path>,
) -> Result<varta_project::NetworkMode, ProjectError> {
    Ok(match (record, replay) {
        (Some(path), _) if path.symlink_metadata().is_ok() => {
            return Err(ProjectError::Usage(format!(
                "network log {} already exists",
                path.display()
            )));
        }
        (Some(_), _) => varta_project::NetworkMode::Record,
        (None, Some(path)) => {
            varta_project::NetworkMode::Replay(varta_project::read_network_log(path)?)
        }
        (None, None) => varta_project::NetworkMode::Live,
    })
}

/// How the execution flags run a program.
fn settings(
    decisions: Option<varta_project::DecisionFile>,
    flags: &ExecutionFlags,
) -> varta_project::RunSettings {
    varta_project::RunSettings {
        application: flags.app,
        decisions,
        observer: Some(observer(flags.budget_report)),
        file_roots: None,
        network: varta_project::NetworkMode::Live,
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

/// Print one captured console line to its stream: standard-error lines
/// (`console.error`, `console.warn` and the other v38 standard-error
/// methods) to standard error, every other line to standard output.
fn console_line(line: &str) {
    if varta_project::is_standard_error(line) {
        eprintln!("{line}");
    } else {
        println!("{line}");
    }
}
