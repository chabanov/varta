#![forbid(unsafe_code)]
//! Execution-only native companion. No project/source/compiler dependency.
mod args;
use args::{Args, Mode};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use varta_artifact::{Artifact, MAX_MANIFEST_BYTES, read_bounded};
use varta_common::{
    Manifest,
    launcher::{INFO_ARGUMENT, LauncherInfo},
};
use varta_runtime::{RuntimeEvent, VartaRuntime};

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("{0}")]
    Usage(&'static str),
    #[error(transparent)]
    Artifact(#[from] varta_common::VartaError),
    #[error(transparent)]
    Runtime(#[from] varta_runtime::RuntimeError),
    #[error(transparent)]
    Invocation(#[from] varta_common::ToolInvocationError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
impl Error {
    fn code(&self) -> Option<varta_common::ToolInvocationCode> {
        match self {
            Self::Invocation(error) => Some(error.code()),
            Self::Artifact(varta_common::VartaError::InvalidArtifact(_)) => {
                Some(varta_common::ToolInvocationCode::InvalidArtifact)
            }
            _ => None,
        }
    }
}
const HELP: &str = "Varta execution runtime\n\nPackaged application:\n  app [--policy PATH]\n  app --invoke NAME [--params JSON] [--json] [--policy PATH]\n  app --inspect [--json] [--policy PATH]\n\nStandalone runtime:\n  varta-runtime run ARTIFACT [--policy PATH]\n  varta-runtime invoke ARTIFACT NAME [--params JSON] [--json] [--policy PATH]\n  varta-runtime inspect ARTIFACT [--json] [--policy PATH]\n  varta-runtime --validate ARTIFACT\n  varta-runtime --runtime-info\n\nThis runtime executes portable Wasm; source compilation uses varta.\n";
fn artifact(path: Option<PathBuf>) -> Result<(Artifact, PathBuf), Error> {
    if let Some(path) = path {
        return Ok((Artifact::read(&path)?, path));
    }
    let path = std::env::current_exe()?;
    let artifact = Artifact::embedded(&path)?.ok_or(Error::Usage(
        "no embedded application; use 'varta-runtime run ARTIFACT'",
    ))?;
    Ok((artifact, path))
}
fn policy(path: Option<&Path>) -> Result<Option<Manifest>, Error> {
    path.map(|path| Ok(Manifest::parse(&read_bounded(path, MAX_MANIFEST_BYTES)?)?))
        .transpose()
}
fn execute(args: Args) -> Result<i32, Error> {
    let host = policy(args.policy.as_deref())?;
    let ceiling = host.as_ref().map(Manifest::grants).transpose()?;
    let runtime = VartaRuntime::new();
    match args.mode {
        Mode::Run(path) => {
            let (artifact, _) = artifact(path)?;
            let output = runtime.run_observed(
                &artifact.wasm,
                &artifact.grants()?,
                ceiling.as_deref(),
                Arc::new(|event| match event {
                    RuntimeEvent::Listening { address, .. } => {
                        eprintln!("varta listening: http://{address}")
                    }
                    RuntimeEvent::Output(line) => println!("{line}"),
                    RuntimeEvent::Diagnostic(message) => eprintln!("varta HTTP: {message}"),
                }),
            )?;
            for line in output.lines {
                println!("{line}");
            }
            Ok(output.exit_code)
        }
        Mode::Invoke { path, name, params } => {
            let (artifact, _) = artifact(path)?;
            let grants = artifact.grants()?;
            let output = match ceiling.as_deref() {
                Some(ceiling) => runtime.invoke_tool_with_policy(
                    &artifact.wasm,
                    &name,
                    params.as_bytes(),
                    &grants,
                    ceiling,
                )?,
                None => runtime.invoke_tool(&artifact.wasm, &name, params.as_bytes(), &grants)?,
            };
            if args.json {
                println!(
                    "{}",
                    serde_json::json!({"status":"ok","result":output.result,"logs":output.logs})
                );
            } else {
                for line in output.logs {
                    eprintln!("{line}");
                }
                println!("{}", serde_json::to_string(&output.result)?);
            }
            Ok(0)
        }
        Mode::Inspect(path) => {
            let (artifact, path) = artifact(path)?;
            let module = runtime.inspect_module(&artifact.wasm)?;
            println!(
                "{}",
                serde_json::json!({"artifact":path,"manifest":artifact.manifest,"toolCatalog":module.tool_catalog.as_ref().map(|catalog|catalog.discovery()),"module":module,"hostCeiling":host.map(|m|m.capabilities),"combination":"allOf"})
            );
            Ok(0)
        }
        Mode::Validate(path) => {
            let artifact = Artifact::read(&path)?;
            runtime.validate_module(&artifact.wasm)?;
            println!("{}", serde_json::json!({"status":"ok"}));
            Ok(0)
        }
    }
}
fn main() {
    let values = std::env::args_os().skip(1).collect::<Vec<_>>();
    if values.len() == 1 {
        match values[0].to_str() {
            Some("--help" | "-h") => {
                print!("{HELP}");
                return;
            }
            Some("--version" | "-V") => {
                println!("varta-runtime {}", env!("CARGO_PKG_VERSION"));
                return;
            }
            Some(INFO_ARGUMENT) => {
                match serde_json::to_string(&LauncherInfo::current()) {
                    Ok(info) => println!("{info}"),
                    Err(error) => {
                        eprintln!("varta runtime failed: {error}");
                        std::process::exit(1);
                    }
                }
                return;
            }
            _ => {}
        }
    }
    let json = values.iter().any(|v| v == "--json");
    let result = Args::parse(values).and_then(execute);
    match result {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({"status":"failed","error":error.to_string(),"code":error.code()})
                );
            }
            eprintln!("varta runtime failed: {error}");
            std::process::exit(1);
        }
    }
}
