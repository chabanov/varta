#![forbid(unsafe_code)]
//! Experimental native preview for an already compiled, unprivileged view
//! module: no manifest, no grants, no host ceiling and no storage. Packaged
//! applications run in `varta-shell`.
use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;
use varta_desktop::shell::{
    Launch,
    args::Renderer,
    read_font, read_module,
    window::{self, Console, WindowConfig},
};

const USAGE: &str = "usage: varta-preview <app.wasm> [<font-file> <font-family>]";
const RENDERER_INFO: &str = "--renderer-info";
/// The environment variable that selects the renderer by its `varta-shell
/// --renderer` name (`cpu`, `gpu` or `auto`); anything else is the CPU.
const RENDERER_VARIABLE: &str = "VARTA_PREVIEW_RENDERER";
const LABEL: &str = "varta-preview";

fn run(started: Instant) -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == RENDERER_INFO {
        println!(
            "{{\"schemaVersion\":1,\"gpuCompiled\":{},\"default\":\"cpu\"}}",
            cfg!(feature = "gpu")
        );
        return Ok(());
    }
    if args.len() != 1 && args.len() != 3 {
        return Err(USAGE.into());
    }
    let renderer = std::env::var(RENDERER_VARIABLE)
        .ok()
        .and_then(|name| Renderer::from_name(&name))
        .unwrap_or_default();
    // Every input is checked before the window opens.
    window::check_renderer(renderer)?;
    let wasm = read_module(Path::new(&args[0]))?;
    let font = if args.len() == 3 {
        Some((
            read_font(Path::new(&args[1]))?,
            args[2]
                .to_str()
                .ok_or("font family must be UTF-8")?
                .to_owned(),
        ))
    } else {
        None
    };
    window::run(
        Launch::unprivileged(wasm),
        WindowConfig {
            title: "Varta preview",
            font,
            renderer,
            console: Console {
                label: LABEL,
                output: |line| eprintln!("{line}"),
                diagnostic: |line| eprintln!("{line}"),
            },
        },
        started,
    )?;
    Ok(())
}

fn main() -> ExitCode {
    let started = Instant::now();
    match run(started) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{LABEL}: {error}");
            ExitCode::FAILURE
        }
    }
}
