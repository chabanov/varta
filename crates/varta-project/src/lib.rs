#![forbid(unsafe_code)]
//! Shared checked project pipeline, portable artifacts and reviewable policy metadata.
mod artifact;
pub mod browser;
mod config;
mod invoke;
mod launcher;
pub use launcher::companion as runtime_launcher;
mod modules;
mod policy;
mod roots;
pub use artifact::Artifact;
pub use config::{CapabilitySpec, TailwindConfig, VartaConfig};
pub use invoke::{invoke_tool, invoke_tool_with_policy};
pub use policy::{HostPolicy, authorization, run_with_policy};
use std::{
    fs,
    path::{Path, PathBuf},
};
pub use varta_common::host_inputs::HostInputs;
pub use varta_common::permission::DecisionFile;
pub use varta_common::profile::{FuelProfile, FunctionFuel};
pub use varta_runtime::system_host_inputs;
use varta_runtime::{ApplicationOptions, RunOptions, VartaRuntime};
pub use varta_runtime::{
    FileRoots, HttpReport, HttpRequestOutcome, HttpRequestReport, NetworkLog, NetworkLogError,
    NetworkMode, RunOutput, RunReport, RuntimeEvent, RuntimeObserver, ViewGeometryRequest,
    ViewReport, ViewScript, ViewSnapshotError, ViewSnapshotFormat, encode_view_snapshot,
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
    #[error(transparent)]
    ViewSnapshot(#[from] varta_runtime::ViewSnapshotError),
    /// An explicit `--view-font` face the text shaper refuses.
    #[error("view font: {0}")]
    ViewFont(#[from] varta_runtime::ViewFontError),
    /// A recorded network log that is not one (`varta dev --network-replay`).
    #[error(transparent)]
    NetworkLog(#[from] varta_runtime::NetworkLogError),
    /// The `--view-font` files together exceed the browser resource total.
    #[error("view fonts exceed their {limit}-byte total")]
    ViewFontsTooLarge { limit: usize },
    /// One `--view-font` file exceeds the browser font-file bound.
    #[error("view font {} exceeds its {limit}-byte bound", path.display())]
    ViewFontTooLarge { path: PathBuf, limit: usize },
    /// A `--view-font` path that is not a regular file.
    #[error("view font {} is not a regular file", .0.display())]
    ViewFontNotFile(PathBuf),
    /// The project's Tailwind entry stylesheet failed to compile (K8).
    #[error("{}: {source}", path.display())]
    Tailwind {
        path: PathBuf,
        source: varta_tailwind::TailwindError,
    },
    /// `tailwind.entry` is not imported by the program as a text resource.
    #[error(
        "{}: the Tailwind entry is not imported by the program; import it as text (`import css from \"./app.css\" with {{ type: \"text\" }}`) and apply it with `View.stylesheet`",
        .0.display()
    )]
    TailwindUnused(PathBuf),
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
    /// The artifact's resource section: the configuration's validated font
    /// faces and images in canonical order (empty without either).
    pub resources: varta_artifact::Resources,
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
        let resources = artifact::resources(&config, &base)?;
        Ok(Self {
            config,
            artifact: Artifact { wasm, manifest },
            output,
            modules: compilation.modules,
            roots,
            resources,
        })
    }

    /// Write `<out>.wasm` with its sidecar manifest and, when fonts or images
    /// are declared, its sidecar resource section `<out>.resources`; with
    /// `single`, also the container `<out>` around the verified runtime.
    pub fn build(&self, single: bool, launcher: &Path) -> Result<Vec<PathBuf>> {
        let launcher = single
            .then(|| launcher::VerifiedLauncher::prepare(launcher, &self.artifact, &self.resources))
            .transpose()?;
        let wasm = PathBuf::from(format!("{}.wasm", self.output.display()));
        self.artifact.write_wasm_with(&wasm, &self.resources)?;
        let mut paths = vec![wasm.clone()];
        if !self.resources.is_empty() {
            paths.push(wasm.with_extension(artifact::RESOURCE_SIDECAR_EXTENSION));
        }
        if let Some(launcher) = launcher {
            self.artifact
                .write_executable_with(&self.output, launcher.path(), &self.resources)?;
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
    let (config, base, compilation, _) = compile_inputs(config_path, fuel_profile, false)?;
    Ok((config, base, compilation))
}

fn compile_inputs(
    config_path: &Path,
    fuel_profile: bool,
    observe: bool,
) -> Result<(
    VartaConfig,
    PathBuf,
    varta_compiler::Compilation,
    modules::SourceLoader,
)> {
    let (result, loader) = compile_attempt(config_path, fuel_profile, observe);
    let (config, base, compilation) = result?;
    Ok((
        config,
        base,
        compilation,
        loader.expect("successful compilation has a source loader"),
    ))
}

fn compile_attempt(
    config_path: &Path,
    fuel_profile: bool,
    observe: bool,
) -> (
    Result<(VartaConfig, PathBuf, varta_compiler::Compilation)>,
    Option<modules::SourceLoader>,
) {
    let mut observed = None;
    let result = (|| {
        let source = artifact::read_bounded(config_path, artifact::MAX_MANIFEST_BYTES)?;
        let config: VartaConfig = serde_json::from_slice(&source)
            .map_err(|e| varta_common::VartaError::InvalidManifest(e.to_string()))?;
        config.validate()?;
        let base = config_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_owned();
        let mut loader = modules::SourceLoader::new(&base, config.language_profile)?;
        if observe {
            loader.observe_inputs();
            loader.observe_config(config_path, source)?;
        }
        observed = Some(loader);
        let loader = observed.as_ref().expect("source loader was initialized");
        let entry_path = base.join(&config.entry);
        if observe {
            loader.read_external(&entry_path, modules::MAX_SOURCE_BYTES)?;
        }
        let entry_path = entry_path.canonicalize()?;
        let entry = loader.entry(&entry_path)?;
        if let Some(tailwind) = &config.tailwind {
            tailwind_stylesheet(loader, &tailwind.entry, &entry, config.language_profile)?;
        }
        let settings = config.compiler_settings();
        let compilation = if fuel_profile {
            varta_compiler::compile_modules_for_fuel_profile_with_settings(
                entry,
                loader,
                config.language_profile,
                &settings,
            )?
        } else {
            varta_compiler::compile_modules_with_settings(
                entry,
                loader,
                config.language_profile,
                &settings,
            )?
        };
        Ok((config, base, compilation))
    })();
    (result, observed)
}

/// Generate the project's Tailwind stylesheet (K8 T1) and serve it as the
/// text of the entry stylesheet's resource: scan the module graph's class
/// tokens with the resource as an empty sheet, compile the entry (its
/// relative `@import`s read through the same loader) with them
/// (`varta-tailwind`, canonical form, `@supports` resolved against the L0
/// grammar), and serve the result for the compilation that follows. The same
/// sources always give the same stylesheet bytes.
fn tailwind_stylesheet(
    loader: &modules::SourceLoader,
    entry_css: &str,
    entry: &varta_compiler::ModuleSource,
    profile: varta_common::language::LanguageProfile,
) -> Result<()> {
    let path = loader.project_file(entry_css)?;
    let raw = varta_compiler::ModuleLoader::load(loader, &path)?;
    loader.set_generated(path.clone(), String::new());
    let scan = varta_compiler::scan_classes_with_profile(entry.clone(), loader, profile)?;
    if !scan.text_resources.contains(&path) {
        return Err(ProjectError::TailwindUnused(path));
    }
    if let Some(composition) = scan.compositions.into_iter().next() {
        return Err(composition.into());
    }
    let tailwind = |source| ProjectError::Tailwind {
        path: path.clone(),
        source,
    };
    let mut compiler =
        varta_tailwind::compile_with_loader(&raw, &path, loader).map_err(tailwind)?;
    let build = compiler
        .build(
            scan.tokens.iter().map(String::as_str),
            &varta_tailwind::VartaSupports,
        )
        .map_err(tailwind)?;
    loader.set_generated(path, build.css);
    Ok(())
}

/// Run a packaged application with the system's zone and locale.
pub fn run(artifact: &Artifact) -> Result<RunOutput> {
    let options = RunOptions {
        requested: artifact.grants()?,
        host_inputs: system_host_inputs(),
        ..RunOptions::default()
    };
    Ok(VartaRuntime::new()
        .run_with(&artifact.wasm, options)
        .into_result()?)
}

/// Stream runtime events while preserving the same validated artifact/policy
/// path, with the file roots the host policy maps.
pub fn run_observed(
    artifact: &Artifact,
    host: Option<&HostPolicy>,
    observer: RuntimeObserver,
) -> Result<RunOutput> {
    artifact.grants()?;
    let settings = RunSettings {
        host_inputs: system_host_inputs(),
        ..RunSettings::default()
    };
    let mut options = options(&artifact.manifest, host, &settings)?;
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
    /// How an application instance's requests reach a transport: real
    /// transfers, recorded ones, or a recorded log replayed instead.
    /// Recording and replay run application instances only.
    pub network: NetworkMode,
    /// The zone and locale of v38 local time, recorded host inputs (UTC
    /// and `en-US` by default; the CLI passes the system's or its flags).
    pub host_inputs: HostInputs,
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
    options.host_inputs = settings.host_inputs.clone();
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
    if !settings.application && !matches!(settings.network, NetworkMode::Live) {
        return Err(ProjectError::Usage(
            "network recording and replay run application instances".into(),
        ));
    }
    Ok(if settings.application {
        runtime.run_application(
            wasm,
            ApplicationOptions {
                run: options,
                network: settings.network,
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
    run_view_with_geometry(artifact, host, settings, script, None)
}

/// [`run_view`] whose final snapshot also carries the opt-in `geometry`
/// section when `geometry` is given (`varta dev --view-geometry`); see
/// [`varta_runtime::VartaRuntime::run_view_with_geometry`].
pub fn run_view_with_geometry(
    artifact: &Artifact,
    host: Option<&HostPolicy>,
    settings: RunSettings,
    script: &ViewScript,
    geometry: Option<&mut ViewGeometryRequest>,
) -> Result<ViewReport> {
    let mut options = options(&artifact.manifest, host, &settings)?;
    options.decisions = settings.decisions;
    options.observer = settings.observer;
    Ok(VartaRuntime::new().run_view_with_geometry(
        &artifact.wasm,
        ApplicationOptions {
            run: options,
            network: settings.network,
            ..ApplicationOptions::default()
        },
        script,
        geometry,
    ))
}

/// Read and validate a bounded recorded network log
/// (`varta dev --network-replay`).
pub fn read_network_log(path: &Path) -> Result<std::sync::Arc<NetworkLog>> {
    let bytes = artifact::read_bounded(path, varta_common::net::replay::MAX_LOG_BYTES)?;
    Ok(std::sync::Arc::new(NetworkLog::parse(&bytes)?))
}

/// Write a recorded network log (`varta dev --network-record`) to a new
/// file; an existing file is never overwritten.
pub fn write_network_log(path: &Path, log: &NetworkLog) -> Result<()> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(&log.to_json())?;
    Ok(())
}

/// Read and parse a bounded headless view script.
pub fn read_view_script(path: &Path) -> Result<ViewScript> {
    let bytes = artifact::read_bounded(path, ViewScript::MAX_BYTES)?;
    Ok(ViewScript::parse(&bytes)?)
}

/// Read explicit headless layout fonts (`varta dev --view-font PATH FAMILY`)
/// into a geometry request. Each read is bounded by the browser font-file
/// bound and by what remains of the browser resource total, as browser
/// resources are, so no byte past either bound is kept and at most one more
/// is read. The text shaper then validates the faces (no OS font lookup); the
/// first family is the default when CSS names none.
pub fn read_view_fonts(fonts: &[(PathBuf, String)]) -> Result<ViewGeometryRequest> {
    use browser::resources::{MAX_RESOURCE_BYTES, MAX_TOTAL_BYTES};
    let mut faces = Vec::with_capacity(fonts.len());
    let mut total = 0usize;
    for (path, family) in fonts {
        let bound = MAX_RESOURCE_BYTES.min(MAX_TOTAL_BYTES - total);
        let file = fs::File::open(path)?;
        if !file.metadata()?.is_file() {
            return Err(ProjectError::ViewFontNotFile(path.clone()));
        }
        let Some(data) = read_within(file, bound)? else {
            // The smaller bound refused the file: what remains of the total,
            // or the per-file bound while a whole file's worth remains.
            return Err(if bound < MAX_RESOURCE_BYTES {
                ProjectError::ViewFontsTooLarge {
                    limit: MAX_TOTAL_BYTES,
                }
            } else {
                ProjectError::ViewFontTooLarge {
                    path: path.clone(),
                    limit: MAX_RESOURCE_BYTES,
                }
            });
        };
        total += data.len();
        faces.push(varta_runtime::ViewFont {
            data,
            family: family.clone(),
            aliases: Vec::new(),
        });
    }
    let measurer = varta_runtime::ViewTextMeasurer::new_with_fonts(faces)?;
    Ok(ViewGeometryRequest::new(measurer))
}

/// Read at most `bound + 1` bytes; `None` when the input is longer than `bound`.
fn read_within(reader: impl std::io::Read, bound: usize) -> std::io::Result<Option<Vec<u8>>> {
    use std::io::Read as _;
    let limit = u64::try_from(bound).unwrap_or(u64::MAX).saturating_add(1);
    let mut data = Vec::new();
    reader.take(limit).read_to_end(&mut data)?;
    Ok((data.len() <= bound).then_some(data))
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

#[cfg(test)]
mod tests {
    use super::*;

    const FONT: &[u8] = include_bytes!("../../varta-view/tests/fixtures/structure-400.ttf");
    const FAMILY: &str = "Varta Structure Fixture";

    /// A fresh directory for one test; removed when dropped.
    struct Scratch(PathBuf);
    impl Scratch {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "varta-project-{name}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn view_fonts_are_read_within_the_browser_font_bounds() {
        let scratch = Scratch::new("view-fonts");
        let font = scratch.0.join("font.ttf");
        fs::write(&font, FONT).unwrap();
        assert!(read_view_fonts(&[(font.clone(), FAMILY.into())]).is_ok());
        // One file over the per-font bound is refused while it is read.
        let large = scratch.0.join("large.ttf");
        fs::write(&large, vec![0; browser::resources::MAX_RESOURCE_BYTES + 1]).unwrap();
        assert!(matches!(
            read_view_fonts(&[(large.clone(), FAMILY.into())]),
            Err(ProjectError::ViewFontTooLarge { path, limit })
                if path == large && limit == browser::resources::MAX_RESOURCE_BYTES
        ));
        // A directory is not a face.
        assert!(matches!(
            read_view_fonts(&[(scratch.0.clone(), FAMILY.into())]),
            Err(ProjectError::ViewFontNotFile(path)) if path == scratch.0
        ));
        // Files each within the bound but together over the total are refused
        // before any face is parsed: after whole files, and when a file is
        // within its own bound but over what remains of the total.
        let full = scratch.0.join("full.ttf");
        fs::write(&full, vec![0; browser::resources::MAX_RESOURCE_BYTES]).unwrap();
        let faces = browser::resources::MAX_TOTAL_BYTES / browser::resources::MAX_RESOURCE_BYTES;
        let entry = |path: &PathBuf| (path.clone(), FAMILY.to_owned());
        let after_whole: Vec<_> = std::iter::repeat_n(entry(&full), faces)
            .chain([entry(&font)])
            .collect();
        let over_remaining: Vec<_> = std::iter::repeat_n(entry(&full), faces - 1)
            .chain([entry(&font), entry(&full)])
            .collect();
        for fonts in [after_whole, over_remaining] {
            assert!(matches!(
                read_view_fonts(&fonts),
                Err(ProjectError::ViewFontsTooLarge { limit })
                    if limit == browser::resources::MAX_TOTAL_BYTES
            ));
        }
        // A missing file and bytes that are not a font fail too.
        assert!(matches!(
            read_view_fonts(&[(scratch.0.join("absent.ttf"), FAMILY.into())]),
            Err(ProjectError::Io(_))
        ));
        let bad = scratch.0.join("bad.ttf");
        fs::write(&bad, b"not a font").unwrap();
        assert!(matches!(
            read_view_fonts(&[(bad, FAMILY.into())]),
            Err(ProjectError::ViewFont(_))
        ));
    }

    #[test]
    fn a_view_font_read_stops_one_byte_past_the_remaining_total() {
        use browser::resources::{MAX_RESOURCE_BYTES, MAX_TOTAL_BYTES};
        // Input that fits its bound exactly is kept whole.
        assert_eq!(
            read_within(FONT, FONT.len()).unwrap().as_deref(),
            Some(FONT)
        );
        // With less than one file's bound left of the total, an endless input
        // is read one byte past what remains, not up to the per-file bound.
        let total = MAX_TOTAL_BYTES - FONT.len();
        let bound = MAX_RESOURCE_BYTES.min(MAX_TOTAL_BYTES - total);
        assert!(bound < MAX_RESOURCE_BYTES);
        let mut endless = std::io::Read::take(std::io::repeat(0), u64::MAX);
        assert!(read_within(&mut endless, bound).unwrap().is_none());
        assert_eq!(
            u64::MAX - endless.limit(),
            u64::try_from(bound + 1).unwrap()
        );
    }
}
