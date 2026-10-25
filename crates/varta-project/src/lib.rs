#![forbid(unsafe_code)]
//! Shared checked project pipeline, portable artifacts and reviewable policy metadata.
mod artifact;
mod modules;
mod policy;
pub use artifact::Artifact;
pub use policy::{HostPolicy, authorization, run_with_policy};
use std::{
    fs,
    path::{Path, PathBuf},
};
use varta_common::{Manifest, VartaConfig};
use varta_runtime::{RunOutput, VartaRuntime};

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Config(#[from] varta_common::VartaError),
    #[error(transparent)]
    Compile(#[from] varta_compiler::CompilerError),
    #[error(transparent)]
    Runtime(#[from] varta_runtime::RuntimeError),
    #[error("invalid artifact: {0}")]
    Artifact(String),
    #[error("{0}")]
    Usage(String),
}
impl ProjectError {
    #[must_use]
    pub fn diagnostic(&self) -> Option<&varta_common::Diagnostic> {
        match self {
            Self::Compile(error) => error.diagnostic(),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, ProjectError>;

pub struct CheckedProject {
    pub config: VartaConfig,
    pub artifact: Artifact,
    pub output: PathBuf,
    pub modules: Vec<varta_common::ModuleInfo>,
}

impl CheckedProject {
    /// The same semantic/type and Wasm validation stages serve check, dev and build.
    pub fn load(config_path: &Path) -> Result<Self> {
        let source = artifact::read_bounded(config_path, artifact::MAX_MANIFEST_BYTES)?;
        let config: VartaConfig = serde_json::from_slice(&source)
            .map_err(|e| varta_common::VartaError::InvalidManifest(e.to_string()))?;
        config.validate()?;
        let base = config_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let entry_path = base.join(&config.entry).canonicalize()?;
        let loader = modules::SourceLoader::new(base)?;
        let compilation = varta_compiler::compile_modules(loader.entry(&entry_path)?, &loader)?;
        let wasm = compilation.wasm;
        VartaRuntime::new().validate_module(&wasm)?;
        let mut manifest = Manifest::new(&config.normalized_capabilities());
        manifest.entry = Some(config.entry.clone());
        manifest.target = Some(config.target.clone());
        let output = base.join(&config.out);
        Ok(Self {
            config,
            artifact: Artifact { wasm, manifest },
            output,
            modules: compilation.modules,
        })
    }

    pub fn build(&self, single: bool, launcher: &Path) -> Result<Vec<PathBuf>> {
        let wasm = PathBuf::from(format!("{}.wasm", self.output.display()));
        self.artifact.write_wasm(&wasm)?;
        let mut paths = vec![wasm];
        if single {
            self.artifact.write_executable(&self.output, launcher)?;
            paths.push(self.output.clone());
        }
        Ok(paths)
    }

    pub fn run(&self) -> Result<RunOutput> {
        run(&self.artifact)
    }

    pub fn run_with_policy(&self, policy: &HostPolicy) -> Result<RunOutput> {
        run_with_policy(&self.artifact, policy)
    }
}

pub fn run(artifact: &Artifact) -> Result<RunOutput> {
    Ok(VartaRuntime::new().run_wasm_bytes(&artifact.wasm, &artifact.manifest.grants()?)?)
}

/// Policy review additionally requires a declaration for every privileged import.
/// An omitted grant is still safe to execute: the runtime denies the operation.
pub fn validate(artifact: &Artifact) -> Result<()> {
    let grants = artifact.manifest.grants()?;
    let module = VartaRuntime::new().validate_module(&artifact.wasm)?;
    for import in module.imports() {
        if let Some(Some(id)) = varta_common::host_capability(import.name())
            && !grants.iter().any(|grant| grant.id == id)
        {
            return Err(ProjectError::Artifact(format!(
                "imported host API requires capability namespace: {id}"
            )));
        }
    }
    Ok(())
}

pub fn inspect(artifact: &Artifact, path: &Path) -> Result<serde_json::Value> {
    inspect_with_policy(artifact, path, None)
}

pub fn inspect_with_policy(
    artifact: &Artifact,
    path: &Path,
    policy: Option<&HostPolicy>,
) -> Result<serde_json::Value> {
    let grants = artifact.manifest.grants()?;
    let (module, tools) = VartaRuntime::new().validate_module_and_tools(&artifact.wasm)?;
    let imports: Vec<_> = module
        .imports()
        .map(|i| format!("{}.{}", i.module(), i.name()))
        .collect();
    let exports: Vec<_> = module.exports().map(|e| e.name().to_owned()).collect();
    let memories = module
        .exports()
        .filter(|e| matches!(e.ty(), wasmtime::ExternType::Memory(_)))
        .count();
    Ok(
        serde_json::json!({ "artifact": path, "size": artifact.wasm.len(), "precompiled": false,
        "schemaVersion": artifact.manifest.schema_version, "abiVersion": artifact.manifest.abi_version,
        "policy": artifact.manifest.policy,
        "capabilities": grants.iter().map(|g| &g.id).collect::<Vec<_>>(),
        "grants": artifact.manifest.capabilities, "imports": imports, "exports": exports, "memories": memories,
        "toolCatalog": tools.map(|catalog| catalog.discovery()),
        "authorization": authorization(&artifact.manifest, policy)? }),
    )
}

pub fn init(path: &Path) -> Result<()> {
    // Never overwrite an existing project or configuration.
    fs::create_dir(path)?;
    fs::create_dir(path.join("src"))?;
    fs::write(
        path.join("varta.config.json"),
        r#"{
  "entry": "src/main.ts",
  "target": "native",
  "policy": "strict",
  "out": "dist/app",
  "capabilities": { "time": { "now": true } }
}
"#,
    )?;
    fs::write(
        path.join("src/main.ts"),
        r#"function add(a: number, b: number): number {
  return a + b;
}
export function main(): void {
  console.log("hello from varta");
  console.log(40 + 2);
  console.log(add(3, 4));
  console.log(varta.time.now());
}
"#,
    )?;
    fs::write(path.join("varta.d.ts"), varta_common::HOST_DECLARATIONS)?;
    Ok(())
}
