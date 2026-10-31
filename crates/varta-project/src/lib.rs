#![forbid(unsafe_code)]
//! Shared checked project pipeline, portable artifacts and reviewable policy metadata.
mod artifact;
mod config;
mod invoke;
mod launcher;
pub use launcher::companion as runtime_launcher;
mod modules;
mod policy;
mod roots;
pub use artifact::Artifact;
pub use config::{CapabilitySpec, VartaConfig};
pub use invoke::{invoke_tool, invoke_tool_with_policy};
pub use policy::{HostPolicy, authorization, run_with_policy};
use std::{
    fs,
    path::{Path, PathBuf},
};
pub use varta_common::permission::DecisionFile;
pub use varta_common::profile::{FuelProfile, FunctionFuel};
use varta_runtime::{ApplicationOptions, RunOptions, VartaRuntime};
pub use varta_runtime::{
    FileRoots, RunOutput, RunReport, RuntimeEvent, RuntimeObserver, ViewReport, ViewScript,
};

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error(transparent)]
    Invocation(#[from] varta_common::ToolInvocationError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Config(#[from] varta_common::VartaError),
    #[error(transparent)]
    Compile(#[from] varta_compiler::CompilerError),
    #[error(transparent)]
    Runtime(#[from] varta_runtime::RuntimeError),
    #[error(transparent)]
    Permission(#[from] varta_common::permission::PermissionError),
    #[error(transparent)]
    FileRoot(#[from] varta_common::files::FileRootError),
    #[error(transparent)]
    ViewScript(#[from] varta_runtime::ViewScriptError),
    #[error("invalid artifact: {0}")]
    Artifact(String),
    #[error("{0}")]
    Usage(String),
}
impl ProjectError {
    #[must_use]
    pub fn invocation_code(&self) -> Option<varta_common::ToolInvocationCode> {
        match self {
            Self::Invocation(error) => Some(error.code()),
            Self::Config(varta_common::VartaError::InvalidArtifact(_)) | Self::Artifact(_) => {
                Some(varta_common::ToolInvocationCode::InvalidArtifact)
            }
            _ => None,
        }
    }
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
    /// The configuration's v38 file roots, pinned when the project loads
    /// (`file_roots`); `varta dev` reads under them.
    pub roots: FileRoots,
}

impl CheckedProject {
    /// The same semantic/type and Wasm validation stages serve check, dev and build.
    pub fn load(config_path: &Path) -> Result<Self> {
        let (config, base, compilation) = compile(config_path, false)?;
        let wasm = compilation.wasm;
        VartaRuntime::new().validate_module(&wasm)?;
        let manifest = config.manifest();
        let output = base.join(&config.out);
        let roots = roots::pin(&base, &config)?;
        Ok(Self {
            config,
            artifact: Artifact { wasm, manifest },
            output,
            modules: compilation.modules,
            roots,
        })
    }

    pub fn build(&self, single: bool, launcher: &Path) -> Result<Vec<PathBuf>> {
        let launcher = single
            .then(|| launcher::VerifiedLauncher::prepare(launcher, &self.artifact))
            .transpose()?;
        let wasm = PathBuf::from(format!("{}.wasm", self.output.display()));
        self.artifact.write_wasm(&wasm)?;
        let mut paths = vec![wasm];
        if let Some(launcher) = launcher {
            self.artifact
                .write_executable(&self.output, launcher.path())?;
            paths.push(self.output.clone());
        }
        Ok(paths)
    }

    pub fn run(&self) -> Result<RunOutput> {
        run(&self.artifact)
    }

    /// How `varta dev` runs this project: `settings` with the project's
    /// pinned file roots.
    #[must_use]
    pub fn dev_settings(&self, mut settings: RunSettings) -> RunSettings {
        settings.file_roots = Some(self.roots.clone());
        settings
    }

    pub fn run_with_policy(&self, policy: &HostPolicy) -> Result<RunOutput> {
        run_with_policy(&self.artifact, policy)
    }

    /// `varta dev --profile-fuel`: compile the project again as a fuel-profile
    /// build (every guest function instrumented, [`FuelProfile`]) and run it
    /// exactly as [`run_report`] runs this project: its grants and budget
    /// requests, the optional host ceiling, stored decisions and, with
    /// `settings.application`, as an application instance. The probes refund
    /// their own fuel, so the profiled run consumes what the ordinary build
    /// does; the report carries the profile across all turns
    /// ([`RunReport::fuel_profile`]) next to the per-turn budget report. A
    /// profile build is never written or packaged; ordinary builds carry no
    /// instrumentation.
    pub fn profile_fuel(
        &self,
        config_path: &Path,
        policy: Option<&HostPolicy>,
        settings: RunSettings,
    ) -> Result<RunReport> {
        let (_, _, compilation) = compile(config_path, true)?;
        execute_report(
            &compilation.wasm,
            &self.artifact.manifest,
            policy,
            settings,
            true,
        )
    }
}

/// Read and validate the configuration, then compile its module graph (as a
/// fuel-profile build when `fuel_profile` is set).
fn compile(
    config_path: &Path,
    fuel_profile: bool,
) -> Result<(VartaConfig, PathBuf, varta_compiler::Compilation)> {
    let source = artifact::read_bounded(config_path, artifact::MAX_MANIFEST_BYTES)?;
    let config: VartaConfig = serde_json::from_slice(&source)
        .map_err(|e| varta_common::VartaError::InvalidManifest(e.to_string()))?;
    config.validate()?;
    let base = config_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_owned();
    let entry_path = base.join(&config.entry).canonicalize()?;
    let loader = modules::SourceLoader::new(&base, config.language_profile)?;
    let entry = loader.entry(&entry_path)?;
    let compilation = if fuel_profile {
        varta_compiler::compile_modules_for_fuel_profile(entry, &loader, config.language_profile)?
    } else {
        varta_compiler::compile_modules_with_profile(entry, &loader, config.language_profile)?
    };
    Ok((config, base, compilation))
}

pub fn run(artifact: &Artifact) -> Result<RunOutput> {
    Ok(VartaRuntime::new().run_wasm_bytes(&artifact.wasm, &artifact.grants()?)?)
}

/// Stream runtime events while preserving the same validated artifact/policy
/// path, with the file roots the host policy maps.
pub fn run_observed(
    artifact: &Artifact,
    host: Option<&HostPolicy>,
    observer: RuntimeObserver,
) -> Result<RunOutput> {
    artifact.grants()?;
    let mut options = options(&artifact.manifest, host, &RunSettings::default())?;
    options.observer = Some(observer);
    Ok(VartaRuntime::new()
        .run_with(&artifact.wasm, options)
        .into_result()?)
}

/// How `run`, `dev` and a packaged application execute.
#[derive(Default)]
pub struct RunSettings {
    /// Run a v38 module as a long-lived application instance: the deadline
    /// renews per turn and output streams through the observer per turn.
    pub application: bool,
    /// Stored permission decisions (the headless policy file).
    pub decisions: Option<DecisionFile>,
    pub observer: Option<RuntimeObserver>,
    /// Development file roots (the project configuration's, pinned at load)
    /// for a v38 module; a host policy that maps `fsRoots` replaces them.
    /// Without them, only the host policy maps roots.
    pub file_roots: Option<FileRoots>,
}

/// Read a bounded headless decision file.
pub fn read_decisions(path: &Path) -> Result<DecisionFile> {
    Ok(DecisionFile::parse(&artifact::read_bounded(
        path,
        artifact::MAX_MANIFEST_BYTES,
    )?)?)
}

/// Run an artifact and report everything it produced, including the output
/// captured before a failure and the per-turn budget report.
pub fn run_report(
    artifact: &Artifact,
    host: Option<&HostPolicy>,
    settings: RunSettings,
) -> Result<RunReport> {
    execute_report(&artifact.wasm, &artifact.manifest, host, settings, false)
}

/// The run options of a package under an optional host policy. The host
/// policy's `fsRoots` mapping decides the file roots; without one, the
/// development roots of `settings` apply.
fn options(
    manifest: &varta_common::Manifest,
    host: Option<&HostPolicy>,
    settings: &RunSettings,
) -> Result<RunOptions> {
    let host_manifest = host.map(HostPolicy::manifest);
    let mut options = RunOptions::from_manifests(manifest, host_manifest)?;
    let host_maps = host_manifest.is_some_and(|host| host.fs_roots.is_some());
    if let (Some(roots), false) = (&settings.file_roots, host_maps) {
        options.file_roots = roots.clone();
    }
    Ok(options)
}

/// The one run path of `run`, `dev` and `dev --profile-fuel`.
fn execute_report(
    wasm: &[u8],
    manifest: &varta_common::Manifest,
    host: Option<&HostPolicy>,
    settings: RunSettings,
    fuel_profile: bool,
) -> Result<RunReport> {
    let mut options = options(manifest, host, &settings)?;
    options.decisions = settings.decisions;
    options.observer = settings.observer;
    options.fuel_profile = fuel_profile;
    let runtime = VartaRuntime::new();
    Ok(if settings.application {
        runtime.run_application(
            wasm,
            ApplicationOptions {
                run: options,
                ..ApplicationOptions::default()
            },
        )
    } else {
        runtime.run_with(wasm, options)
    })
}

/// Run a view application headlessly (v38) as an application instance with
/// a scripted session of platform inputs, exactly as [`run_report`] runs an
/// artifact (grants, budget requests, the host policy, stored decisions);
/// reports what the instance produced and the final tree and accessibility
/// snapshot.
pub fn run_view(
    artifact: &Artifact,
    host: Option<&HostPolicy>,
    settings: RunSettings,
    script: &ViewScript,
) -> Result<ViewReport> {
    let mut options = options(&artifact.manifest, host, &settings)?;
    options.decisions = settings.decisions;
    options.observer = settings.observer;
    Ok(VartaRuntime::new().run_view(
        &artifact.wasm,
        ApplicationOptions {
            run: options,
            ..ApplicationOptions::default()
        },
        script,
    ))
}

/// Read and parse a bounded headless view script.
pub fn read_view_script(path: &Path) -> Result<ViewScript> {
    let bytes = artifact::read_bounded(path, ViewScript::MAX_BYTES)?;
    Ok(ViewScript::parse(&bytes)?)
}

/// Policy review additionally requires a declaration for every privileged import.
/// An omitted grant is still safe to execute: the runtime denies the operation.
pub fn validate(artifact: &Artifact) -> Result<()> {
    let grants = artifact.grants()?;
    let module = VartaRuntime::new().inspect_module(&artifact.wasm)?;
    for import in &module.imports {
        if let Some(Some(id)) = varta_common::host_capability(&import.name)
            && !grants.iter().any(|grant| grant.id == id)
        {
            return Err(ProjectError::Artifact(format!(
                "imported host API requires capability namespace: {id}"
            )));
        }
    }
    if module.http_service.is_some()
        && !grants
            .iter()
            .any(|g| g.id == varta_common::http::LISTEN_CAPABILITY)
    {
        return Err(ProjectError::Artifact(
            "HTTP service requires a net.listen declaration".into(),
        ));
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
    let grants = artifact.grants()?;
    let module = VartaRuntime::new().inspect_module(&artifact.wasm)?;
    let imports: Vec<_> = module
        .imports
        .iter()
        .map(|i| format!("{}.{}", i.module, i.name))
        .collect();
    Ok(
        serde_json::json!({ "artifact": path, "size": artifact.wasm.len(), "precompiled": false,
        "schemaVersion": artifact.manifest.schema_version, "abiVersion": artifact.manifest.abi_version, "language":module.language, "valueAbiVersion":module.value_abi_version,
        "policy": artifact.manifest.policy,
        "capabilities": grants.iter().map(|g| &g.id).collect::<Vec<_>>(),
        "grants": artifact.manifest.capabilities, "budgets": artifact.manifest.budgets, "fsRoots": artifact.manifest.fs_roots, "identity": varta_runtime::artifact_identity(&artifact.wasm), "imports": imports, "exports": module.exports, "memories": module.memories,
        "toolCatalog": module.tool_catalog.map(|catalog| catalog.discovery()), "httpService": module.http_service,
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
