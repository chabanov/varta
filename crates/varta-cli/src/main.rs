use clap::{Parser, Subcommand};
use std::fs;
use std::path::Path;
use varta_common::{CapabilityGrant, CapabilityValue, VartaConfig, VartaError};
use std::path::PathBuf;
use varta_runtime::VartaRuntime;
use wasmparser::{Parser as WasmParser, Payload};

#[derive(Parser, Debug)]
#[command(name = "varta", version, about = "Varta secure TS/JS runtime")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize a new Varta project.
    Init {
        name: String,
    },
    /// Check project types and capabilities.
    Check {
        #[arg(long, default_value = "varta.config.json")]
        config: String,
    },
    /// Run a development Wasm build.
    Dev,
    /// Build a production artifact.
    Build {
        #[arg(long, default_value = "varta.config.json")]
        config: String,
        /// Package the application into the varta executable itself.
        #[arg(long)]
        single: bool,
    },
    /// Run a Varta artifact.
    Run {
        path: String,
    },
    /// Inspect binary metadata.
    Inspect {
        path: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Validate {
        path: Option<String>,
    },
}

fn main() {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init { name }) => {
            if let Err(error) = init_project(&name) {
                eprintln!("varta init failed: {error}");
                std::process::exit(1);
            }
            println!("initialized varta project: {name}");
        }
        Some(Commands::Check { config }) => {
            match check_project(&config) {
                Ok(()) => println!("varta check: ok ({config})"),
                Err(error) => {
                    eprintln!("varta check failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Dev) => {
            if std::path::Path::new("varta.config.json").exists() {
                match load_and_compile("varta.config.json") {
                    Ok((bytes, capabilities)) => {
                        let runtime = VartaRuntime::new();
                        if let Err(error) = runtime.run_wasm_bytes(&bytes, &capabilities) {
                            eprintln!("varta dev failed: {error:?}");
                            std::process::exit(1);
                        }
                        println!("varta dev: completed");
                    }
                    Err(error) => {
                        eprintln!("varta dev failed: {error:?}");
                        std::process::exit(1);
                    }
                }
            } else {
                let runtime = VartaRuntime::new();
                let bytes = varta_compiler::minimal_module();
                match runtime.run_wasm_bytes(&bytes, &[]) {
                    Ok(()) => println!("varta dev: completed"),
                    Err(error) => eprintln!("varta dev failed: {error:?}"),
                }
            }
        }
        Some(Commands::Build { config, single }) => {
            match build_project(&config, single) {
                Ok(()) => {}
                Err(error) => {
                    eprintln!("varta build failed: {error}");
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Run { path }) => {
            match read_app_from_file(&path) {
                Ok((bytes, precompiled, capabilities)) => {
                    let runtime = VartaRuntime::new();
                    match runtime.run_app_bytes(&bytes, precompiled, &capabilities) {
                        Ok(()) => println!("varta run: completed {path}"),
                        Err(error) => eprintln!("varta run failed: {error}"),
                    }
                }
                Err(error) => eprintln!("varta run failed: {error}"),
            }
        }
        None => {
            run_self_app();
        }
        Some(Commands::Validate { path }) => {
            let target = path.unwrap_or_else(|| "dist/app".to_string());
            if let Err(error) = validate_app(&target) {
                eprintln!("varta validate failed: {error}");
                std::process::exit(1);
            }
            println!("varta validate: ok ({target})");
        }
        Some(Commands::Inspect { path, json }) => {
            if let Err(error) = inspect_app(&path.unwrap_or_else(|| "dist/app".to_string()), json) {
                eprintln!("varta inspect failed: {error}");
                std::process::exit(1);
            }
        }
    }
}

fn init_project(name: &str) -> Result<(), VartaError> {
    let root = Path::new(name);
    fs::create_dir_all(root.join("src")).map_err(io_error)?;
    fs::write(
        root.join("varta.config.json"),
        r#"{
  "entry": "src/main.ts",
  "target": "native",
  "policy": "strict",
  "out": "dist/app",
  "capabilities": {
    "time": {
      "now": true
    },
    "env": {
      "has": ["PATH"],
      "print": ["PATH"]
    },
    "fs": {
      "read": ["/data/**"]
    },
    "net": {
      "fetch": ["http://127.0.0.1/**"]
    }
  }
}
"#,
    )
    .map_err(io_error)?;
    fs::write(
        root.join("src").join("main.ts"),
        r#"export function main(): void {
  console.log("hello from varta");
  let answer = 40 + 2;
  console.log(answer);
  console.log(varta.time.now());
}
"#,
    )
    .map_err(io_error)?;
    fs::write(
        root.join("varta.d.ts"),
        r#"declare const varta: {
  time: { now(): number };
  env: { has(name: string): boolean; print(name: string): void; get(name: string): any };
  fs: { print(path: string): void; read(path: string): any };
  process: { exit(code: number): void };
  net: { fetch(url: string): number };
};
"#,
    )
    .map_err(io_error)?;
    Ok(())
}

fn check_project(config_path: &str) -> Result<(), VartaError> {
    let source = fs::read_to_string(config_path).map_err(io_error)?;
    let base = Path::new(config_path).parent().unwrap_or_else(|| Path::new("."));
    let config: VartaConfig = serde_json::from_str(&source)
        .map_err(|error| VartaError::InvalidManifest(error.to_string()))?;
    config.validate()?;

    println!("entry: {}", config.entry);
    println!("policy: {}", config.policy);
    println!("target: {}", config.target);
    println!("capabilities: {}", config.normalized_capabilities().len());

    if let Some(deno) = which_command("deno") {
        let mut check_command = std::process::Command::new(deno);
        check_command.arg("check").arg(base.join(&config.entry));
        let type_declarations = base.join("varta.d.ts");
        if type_declarations.exists() {
            check_command.arg(&type_declarations);
        }
        let status = check_command.status().map_err(io_error)?;
        if !status.success() {
            return Err(VartaError::InvalidManifest(format!(
                "typescript type check failed for {}",
                config.entry
            )));
        }
        println!("typescript: type checked with deno");
    } else {
        println!("typescript: skipping external type checker (deno not found)");
    }

    let entry_path = base.join(&config.entry);
    match fs::read_to_string(&entry_path) {
        Ok(source) => match varta_compiler::parse_typescript(&source) {
            Ok(items) => println!("typescript: parsed {items} top-level items"),
            Err(error) => {
                return Err(VartaError::InvalidManifest(format!(
                    "typescript parse failed: {error}"
                )));
            }
        },
        Err(error) => {
            return Err(VartaError::InvalidManifest(format!(
                "failed to read entry {}: {error}",
                entry_path.display()
            )));
        }
    }

    Ok(())
}

fn build_project(config_path: &str, single: bool) -> Result<(), VartaError> {
    let source = fs::read_to_string(config_path).map_err(io_error)?;
    let base = Path::new(config_path).parent().unwrap_or_else(|| Path::new("."));
    let config: VartaConfig = serde_json::from_str(&source)
        .map_err(|error| VartaError::InvalidManifest(error.to_string()))?;
    config.validate()?;

    let entry_path = base.join(&config.entry);
    let entry_source = fs::read_to_string(&entry_path)
        .map_err(|error| VartaError::InvalidManifest(error.to_string()))?;
    varta_compiler::parse_typescript(&entry_source)
        .map_err(|error| VartaError::InvalidManifest(format!("typescript parse failed: {error}")))?;
    let compiled = varta_compiler::compile_typescript(&entry_source)
        .map_err(|error| VartaError::InvalidManifest(format!("typescript compile failed: {error}")))?;

    let artifact_path = format!("{}.wasm", config.out);
    if let Some(parent) = Path::new(&artifact_path).parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    fs::write(&artifact_path, &compiled).map_err(io_error)?;

    let manifest_capabilities = config
        .normalized_capabilities()
        .into_iter()
        .map(|grant| match grant.value {
            varta_common::CapabilityValue::PathGlobs(patterns) => serde_json::json!({
                "id": grant.id,
                "patterns": patterns,
            }),
            varta_common::CapabilityValue::Ports(ports) => serde_json::json!({
                "id": grant.id,
                "ports": ports,
            }),
            varta_common::CapabilityValue::UrlGlobs(urls) => serde_json::json!({
                "id": grant.id,
                "urls": urls,
            }),
            varta_common::CapabilityValue::Boolean(enabled) => serde_json::json!({
                "id": grant.id,
                "enabled": enabled,
            }),
        })
        .collect::<Vec<_>>();

    let manifest = serde_json::json!({
        "entry": config.entry,
        "policy": config.policy,
        "target": config.target,
        "capabilities": manifest_capabilities,
    });
    let manifest_path = format!("{}.manifest.json", config.out);
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).map_err(|error| {
            VartaError::InvalidManifest(error.to_string())
        })?,
    )
    .map_err(io_error)?;

    if single {
        let output = PathBuf::from(&config.out);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        std::fs::copy(std::env::current_exe().map_err(io_error)?, &output).map_err(io_error)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&output).map_err(io_error)?.permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&output, permissions).map_err(io_error)?;
        }

        let runtime = VartaRuntime::new();
        let (payload_magic, payload_bytes) = match runtime.precompile_module(&compiled) {
            Ok(bytes) => (b"VARTA2\0", bytes),
            Err(error) => {
                eprintln!("wasm precompile failed, embedding portable Wasm: {error}");
                (b"VARTA1\0", compiled.clone())
            }
        };

        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&output)
            .map_err(io_error)?;
        use std::io::Write;
        file.write_all(payload_magic).map_err(io_error)?;
        file.write_all(&(payload_bytes.len() as u64).to_le_bytes())
            .map_err(io_error)?;
        file.write_all(&payload_bytes).map_err(io_error)?;
        let manifest_bytes = serde_json::to_vec(&manifest)
            .map_err(|error| VartaError::InvalidManifest(error.to_string()))?;
        file.write_all(&(manifest_bytes.len() as u64).to_le_bytes())
            .map_err(io_error)?;
        file.write_all(&manifest_bytes).map_err(io_error)?;
        println!("wrote single binary: {}", output.display());
    }

    println!("wrote artifact: {artifact_path}");
    println!("wrote manifest: {manifest_path}");
    Ok(())
}

fn load_and_compile(config_path: &str) -> Result<(Vec<u8>, Vec<CapabilityGrant>), VartaError> {
    let source = fs::read_to_string(config_path).map_err(io_error)?;
    let base = Path::new(config_path).parent().unwrap_or_else(|| Path::new("."));
    let config: VartaConfig = serde_json::from_str(&source)
        .map_err(|error| VartaError::InvalidManifest(error.to_string()))?;
    config.validate()?;

    let entry_path = base.join(&config.entry);
    let entry_source = fs::read_to_string(&entry_path)
        .map_err(|error| VartaError::InvalidManifest(error.to_string()))?;
    varta_compiler::parse_typescript(&entry_source)
        .map_err(|error| VartaError::InvalidManifest(format!("typescript parse failed: {error}")))?;
    let compiled = varta_compiler::compile_typescript(&entry_source)
        .map_err(|error| VartaError::InvalidManifest(format!("typescript compile failed: {error}")))?;
    Ok((compiled, config.normalized_capabilities()))
}

fn load_manifest_capabilities(artifact_path: &str) -> Vec<CapabilityGrant> {
    let manifest_path = artifact_path
        .strip_suffix(".wasm")
        .map(|stem| format!("{stem}.manifest.json"))
        .unwrap_or_else(|| format!("{artifact_path}.manifest.json"));

    let Ok(source) = fs::read_to_string(&manifest_path) else {
        return Vec::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&source) else {
        return Vec::new();
    };

    let mut capabilities = Vec::new();
    if let Some(capabilities_list) = json.get("capabilities").and_then(|value| value.as_array()) {
        for capability in capabilities_list {
            if let Some(id) = capability.as_str() {
                capabilities.push(CapabilityGrant {
                    id: Box::leak(id.to_string().into_boxed_str()),
                    value: CapabilityValue::Boolean(true),
                });
                continue;
            }

            let Some(id) = capability.get("id").and_then(|value| value.as_str()) else {
                continue;
            };

            let grant = if let Some(patterns) = capability.get("patterns").and_then(|value| value.as_array()) {
                CapabilityGrant {
                    id: Box::leak(id.to_string().into_boxed_str()),
                    value: CapabilityValue::PathGlobs(
                        patterns
                            .iter()
                            .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                            .collect(),
                    ),
                }
            } else if let Some(urls) = capability.get("urls").and_then(|value| value.as_array()) {
                CapabilityGrant {
                    id: Box::leak(id.to_string().into_boxed_str()),
                    value: CapabilityValue::UrlGlobs(
                        urls.iter()
                            .filter_map(|value| value.as_str().map(ToOwned::to_owned))
                            .collect(),
                    ),
                }
            } else if let Some(ports) = capability.get("ports").and_then(|value| value.as_array()) {
                CapabilityGrant {
                    id: Box::leak(id.to_string().into_boxed_str()),
                    value: CapabilityValue::Ports(
                        ports
                            .iter()
                            .filter_map(|value| value.as_u64().map(|port| port as u16))
                            .collect(),
                    ),
                }
            } else {
                CapabilityGrant {
                    id: Box::leak(id.to_string().into_boxed_str()),
                    value: CapabilityValue::Boolean(
                        capability
                            .get("enabled")
                            .and_then(|value| value.as_bool())
                            .unwrap_or(true),
                    ),
                }
            };
            capabilities.push(grant);
        }
    }
    capabilities
}

fn io_error(error: std::io::Error) -> VartaError {
    VartaError::InvalidManifest(error.to_string())
}

fn which_command(name: &str) -> Option<String> {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|path| !path.is_empty())
}


fn run_self_app() {
    let Ok(path) = std::env::current_exe() else {
        eprintln!("varta: could not determine current executable");
        std::process::exit(1);
    };

    match read_app_from_file(path.to_string_lossy().as_ref()) {
        Ok((bytes, precompiled, capabilities)) => {
            let runtime = VartaRuntime::new();
            if let Err(error) = runtime.run_app_bytes(&bytes, precompiled, &capabilities) {
                eprintln!("varta run failed: {error}");
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("varta run failed: {error}");
            std::process::exit(1);
        }
    }
}

fn read_app_from_file(path: &str) -> Result<(Vec<u8>, bool, Vec<CapabilityGrant>), VartaError> {
    let data = fs::read(path).map_err(io_error)?;
    if let Some((wasm, precompiled, manifest)) = extract_app_payload(&data) {
        return Ok((wasm, precompiled, parse_manifest_capabilities_bytes(&manifest)));
    }
    Ok((data, false, load_manifest_capabilities(path)))
}

fn extract_app_payload(data: &[u8]) -> Option<(Vec<u8>, bool, Vec<u8>)> {
    let raw_pos = find_last_magic(data, b"VARTA1\0");
    let compiled_pos = find_last_magic(data, b"VARTA2\0");
    let (magic_pos, precompiled) = match (raw_pos, compiled_pos) {
        (Some(raw), Some(compiled)) => {
            if compiled > raw {
                (compiled, true)
            } else {
                (raw, false)
            }
        }
        (Some(raw), None) => (raw, false),
        (None, Some(compiled)) => (compiled, true),
        (None, None) => return None,
    };
    let magic = if precompiled { b"VARTA2\0" } else { b"VARTA1\0" };

    let mut offset = magic_pos + magic.len();
    if offset + 8 > data.len() {
        return None;
    }
    let wasm_len = usize::from_le_bytes(data[offset..offset + 8].try_into().ok()?);
    offset += 8;
    if wasm_len == 0 || offset + wasm_len + 8 > data.len() {
        return None;
    }
    let wasm_bytes = data[offset..offset + wasm_len].to_vec();
    offset += wasm_len;
    let manifest_len = usize::from_le_bytes(data[offset..offset + 8].try_into().ok()?);
    offset += 8;
    if manifest_len == 0 || offset + manifest_len != data.len() {
        return None;
    }
    let manifest_bytes = data[offset..offset + manifest_len].to_vec();
    Some((wasm_bytes, precompiled, manifest_bytes))
}

fn find_last_magic(data: &[u8], magic: &[u8]) -> Option<usize> {
    (0..=data.len().saturating_sub(magic.len())).rev().find(|&i| data[i..].starts_with(magic))
}

fn parse_manifest_capabilities_bytes(bytes: &[u8]) -> Vec<CapabilityGrant> {
    let Ok(source) = std::str::from_utf8(bytes) else { return Vec::new(); };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(source) else { return Vec::new(); };

    let mut capabilities = Vec::new();
    if let Some(capabilities_list) = json.get("capabilities").and_then(|value| value.as_array()) {
        for capability in capabilities_list {
            if let Some(id) = capability.as_str() {
                capabilities.push(CapabilityGrant {
                    id: Box::leak(id.to_string().into_boxed_str()),
                    value: CapabilityValue::Boolean(true),
                });
                continue;
            }

            let Some(id) = capability.get("id").and_then(|value| value.as_str()) else {
                continue;
            };

            let grant = if let Some(patterns) = capability.get("patterns").and_then(|value| value.as_array()) {
                CapabilityGrant {
                    id: Box::leak(id.to_string().into_boxed_str()),
                    value: CapabilityValue::PathGlobs(
                        patterns.iter().filter_map(|value| value.as_str().map(ToOwned::to_owned)).collect(),
                    ),
                }
            } else if let Some(urls) = capability.get("urls").and_then(|value| value.as_array()) {
                CapabilityGrant {
                    id: Box::leak(id.to_string().into_boxed_str()),
                    value: CapabilityValue::UrlGlobs(
                        urls.iter().filter_map(|value| value.as_str().map(ToOwned::to_owned)).collect(),
                    ),
                }
            } else if let Some(ports) = capability.get("ports").and_then(|value| value.as_array()) {
                CapabilityGrant {
                    id: Box::leak(id.to_string().into_boxed_str()),
                    value: CapabilityValue::Ports(ports.iter().filter_map(|value| value.as_u64().map(|port| port as u16)).collect()),
                }
            } else {
                CapabilityGrant {
                    id: Box::leak(id.to_string().into_boxed_str()),
                    value: CapabilityValue::Boolean(capability.get("enabled").and_then(|value| value.as_bool()).unwrap_or(true)),
                }
            };
            capabilities.push(grant);
        }
    }
    capabilities
}

fn inspect_app(path: &str, json: bool) -> Result<(), VartaError> {
    let (bytes, precompiled, capabilities) = read_app_from_file(path)?;
    let capabilities = if capabilities.is_empty() {
        load_manifest_capabilities(path)
    } else {
        capabilities
    };

    let capability_ids: Vec<&str> = capabilities.iter().map(|grant| grant.id).collect();
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    let mut memories = 0u32;

    if !precompiled {
        let parser = WasmParser::new(0);
        for payload in parser.parse_all(&bytes) {
            let payload = match payload {
                Ok(payload) => payload,
                Err(error) => return Err(VartaError::InvalidManifest(error.to_string())),
            };
            match payload {
                Payload::ImportSection(reader) => {
                    for result in reader {
                        let import = result.map_err(|error| VartaError::InvalidManifest(error.to_string()))?;
                        imports.push(format!("{}.{} ({:?})", import.module, import.name, import.ty));
                    }
                }
                Payload::ExportSection(reader) => {
                    for result in reader {
                        let export = result.map_err(|error| VartaError::InvalidManifest(error.to_string()))?;
                        exports.push(format!("{}", export.name));
                    }
                }
                Payload::MemorySection(reader) => {
                    for result in reader {
                        result.map_err(|error| VartaError::InvalidManifest(error.to_string()))?;
                        memories += 1;
                    }
                }
                _ => {}
            }
        }
    }

    if json {
        println!("{}", serde_json::json!({
            "artifact": path,
            "size": bytes.len(),
            "precompiled": precompiled,
            "capabilities": capability_ids,
            "imports": imports,
            "exports": exports,
            "memories": memories,
        }));
    } else {
        println!("artifact: {path}");
        println!("size: {} bytes", bytes.len());
        println!("precompiled: {precompiled}");
        println!("capabilities:");
        if capability_ids.is_empty() {
            println!("  none");
        } else {
            for capability in capability_ids {
                println!("  {capability}");
            }
        }
        if imports.is_empty() {
            println!("imports: none");
        } else {
            println!("imports:");
            for import in imports {
                println!("  {import}");
            }
        }
        if exports.is_empty() {
            println!("exports: none");
        } else {
            println!("exports:");
            for export in exports {
                println!("  {export}");
            }
        }
        println!("memories: {memories}");
    }
    Ok(())
}

fn validate_app(path: &str) -> Result<(), VartaError> {
    let (bytes, precompiled, capabilities) = read_app_from_file(path)?;
    let capabilities = if capabilities.is_empty() {
        load_manifest_capabilities(path)
    } else {
        capabilities
    };

    for capability in &capabilities {
        if capability.id.trim().is_empty() {
            return Err(VartaError::InvalidManifest("capability id is empty".into()));
        }

        match capability.id.split('.').next().unwrap_or("") {
            "log" | "time" | "env" | "fs" | "process" | "net" => {}
            other => {
                return Err(VartaError::InvalidManifest(format!(
                    "unknown capability namespace: {other}"
                )));
            }
        }

        match &capability.value {
            CapabilityValue::PathGlobs(patterns) => {
                if patterns.is_empty() {
                    return Err(VartaError::InvalidManifest(format!(
                        "{} has no patterns",
                        capability.id
                    )));
                }
                for pattern in patterns {
                    if pattern.trim().is_empty() {
                        return Err(VartaError::InvalidManifest(format!(
                            "{} contains an empty pattern",
                            capability.id
                        )));
                    }
                    if pattern == "*" || pattern == "**" {
                        return Err(VartaError::InvalidManifest(format!(
                            "{} has an overly broad pattern: {pattern}",
                            capability.id
                        )));
                    }
                }
            }
            CapabilityValue::UrlGlobs(patterns) => {
                if patterns.is_empty() {
                    return Err(VartaError::InvalidManifest(format!(
                        "{} has no URL patterns",
                        capability.id
                    )));
                }
                for pattern in patterns {
                    if pattern.trim().is_empty() || !pattern.contains("://") {
                        return Err(VartaError::InvalidManifest(format!(
                            "{} contains an invalid URL pattern: {pattern}",
                            capability.id
                        )));
                    }
                }
            }
            CapabilityValue::Ports(ports) => {
                if ports.is_empty() || ports.iter().any(|port| *port == 0) {
                    return Err(VartaError::InvalidManifest(format!(
                        "{} has an invalid port list",
                        capability.id
                    )));
                }
            }
            CapabilityValue::Boolean(_) => {}
        }
    }

    if !precompiled {
        let mut required = std::collections::BTreeSet::new();
        let parser = WasmParser::new(0);
        for payload in parser.parse_all(&bytes) {
            let payload = payload.map_err(|error| VartaError::InvalidManifest(error.to_string()))?;
            if let Payload::ImportSection(reader) = payload {
                for result in reader {
                    let import = result.map_err(|error| VartaError::InvalidManifest(error.to_string()))?;
                    if import.module != "varta" {
                        continue;
                    }
                    let namespace = match import.name {
                        "log_i32" | "log_string" => continue,
                        "time_now" => "time",
                        "env_has" | "env_print" | "env_get" => "env",
                        "fs_print" | "fs_read" => "fs",
                        "process_exit" => "process",
                        "net_fetch" => "net",
                        other => {
                            return Err(VartaError::InvalidManifest(format!(
                                "unknown host import: {other}"
                            )));
                        }
                    };
                    required.insert(namespace.to_string());
                }
            }
        }

        for namespace in required {
            if !capabilities
                .iter()
                .any(|capability| capability.id == namespace || capability.id.starts_with(&format!("{namespace}.")))
            {
                return Err(VartaError::InvalidManifest(format!(
                    "imported host API requires capability namespace: {namespace}"
                )));
            }
        }
    }

    Ok(())
}
