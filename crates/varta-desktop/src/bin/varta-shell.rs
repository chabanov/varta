#![forbid(unsafe_code)]
//! `varta-shell`: run a validated packaged view application in a native
//! window with its manifest grants, narrowed by an optional host ceiling.
use std::process::ExitCode;
use std::time::Instant;
use varta_desktop::shell::{
    FontSet, Launch, Package,
    args::{self, Command, USAGE},
    default_storage_base, read_font,
    window::{self, Console, WindowConfig},
};

const LABEL: &str = "varta-shell";

fn run(arguments: args::ShellArgs, started: Instant) -> Result<(), Box<dyn std::error::Error>> {
    // Every cheap input is checked before the artifact is prepared, because
    // preparing it creates the artifact's storage directories (after its
    // fonts are parsed).
    window::check_renderer(arguments.renderer)?;
    let font = arguments
        .font
        .map(|(path, family)| read_font(&path).map(|bytes| FontSet::single(bytes, family)))
        .transpose()?;
    let storage = match arguments.data_dir {
        Some(directory) => std::path::absolute(directory)?,
        None => default_storage_base()?,
    };
    let launch = Launch::packaged(&Package {
        artifact: &arguments.artifact,
        policy: arguments.policy.as_deref(),
        storage: &storage,
        font: font.as_ref(),
    })?;
    window::run(
        launch,
        WindowConfig {
            title: "Varta",
            renderer: arguments.renderer,
            console: Console {
                label: LABEL,
                output: |line| println!("{line}"),
                diagnostic: |line| eprintln!("{line}"),
            },
        },
        started,
    )?;
    Ok(())
}

fn main() -> ExitCode {
    let started = Instant::now();
    let command = match args::parse(std::env::args_os().skip(1).collect()) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("{LABEL}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let arguments = match command {
        Command::Help => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Command::Version => {
            println!("{LABEL} {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Command::Run(arguments) => arguments,
    };
    match run(arguments, started) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{LABEL}: {error}");
            ExitCode::FAILURE
        }
    }
}
