#![forbid(unsafe_code)]
//! Experimental native preview for an already compiled, unprivileged view
//! module: no manifest, no grants, no host ceiling and no storage. Packaged
//! applications run in `varta-shell`.
use std::path::Path;
use std::time::Instant;
use varta_desktop::shell::{
    Launch, read_font,
    window::{self, Console, WindowConfig},
};

const USAGE: &str = "usage: varta-preview <app.wasm> [<font-file> <font-family>]";
const RENDERER_INFO: &str = "--renderer-info";
/// The environment variable that selects the GPU renderer (`gpu`/`auto`).
const RENDERER_VARIABLE: &str = "VARTA_PREVIEW_RENDERER";
const LABEL: &str = "varta-preview";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let started = Instant::now();
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
    let wasm =
        varta_artifact::read_bounded(Path::new(&args[0]), varta_common::abi::MAX_WASM_BYTES)?;
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
            label: LABEL,
            font,
            gpu: std::env::var_os(RENDERER_VARIABLE).is_some_and(|v| v == "gpu" || v == "auto"),
            console: Console {
                output: |line| eprintln!("{line}"),
                diagnostic: |line| eprintln!("{line}"),
            },
        },
        started,
    )?;
    Ok(())
}
