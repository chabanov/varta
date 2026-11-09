//! A closed in-memory module loader. No source configuration, I/O or execution.
use super::contract::*;
use base64::Engine;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use varta_compiler::{CompilerError, ModuleLoader, ModuleSource};

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WorkerReply {
    pub result: CompilationResult,
    pub files: BTreeMap<String, String>,
}
struct Loader {
    files: BTreeMap<PathBuf, String>,
}
impl Loader {
    fn resolve_local(
        &self,
        importer: &Path,
        specifier: &str,
        text: bool,
    ) -> std::result::Result<PathBuf, CompilerError> {
        let fail = || {
            CompilerError::Module("local-only toolchain requires confined relative imports and explicit .ts/.tsx code extensions".into())
        };
        if !(specifier.starts_with("./") || specifier.starts_with("../"))
            || specifier.len() > MAX_PATH_BYTES
            || specifier
                .chars()
                .any(|c| c.is_control() || "\\:%?#".contains(c))
            || (!text && !(specifier.ends_with(".ts") || specifier.ends_with(".tsx")))
        {
            return Err(fail());
        }
        let mut components: Vec<_> = importer
            .parent()
            .unwrap_or(Path::new(""))
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        for part in specifier.split('/') {
            match part {
                "." => {}
                ".." => {
                    if components.pop().is_none() {
                        return Err(fail());
                    }
                }
                "" | "node_modules" => return Err(fail()),
                part => components.push(part.to_owned()),
            }
        }
        let path = PathBuf::from(components.join("/"));
        if !self.files.contains_key(&path) {
            return Err(CompilerError::Module(format!(
                "bundle module not found: {}",
                path.display()
            )));
        }
        Ok(path)
    }
}
impl ModuleLoader for Loader {
    fn resolve(
        &self,
        importer: &Path,
        specifier: &str,
    ) -> std::result::Result<PathBuf, CompilerError> {
        self.resolve_local(importer, specifier, false)
    }
    fn resolve_text(
        &self,
        importer: &Path,
        specifier: &str,
    ) -> std::result::Result<PathBuf, CompilerError> {
        self.resolve_local(importer, specifier, true)
    }
    fn load(&self, path: &Path) -> std::result::Result<String, CompilerError> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| CompilerError::Module("bundle module not found".into()))
    }
}
fn diagnostic(error: &crate::ProjectError, entry: &str) -> BuildDiagnostic {
    let (code, path, line, column, message) = match error.diagnostic() {
        Some(d) => (
            serde_json::to_value(d.code)
                .ok()
                .and_then(|s| s.as_str().map(str::to_owned))
                .unwrap_or_else(|| "invalid_program".into()),
            d.file.clone(),
            d.range.start.line,
            d.range.start.column,
            d.message.clone(),
        ),
        None => (
            "invalid_program".into(),
            entry.to_owned(),
            1,
            1,
            error.to_string(),
        ),
    };
    let mut message = message;
    if message.len() > MAX_DIAGNOSTIC_BYTES {
        let mut end = MAX_DIAGNOSTIC_BYTES;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
    }
    BuildDiagnostic {
        severity: "error".into(),
        code,
        path,
        line,
        column,
        message,
    }
}
pub(super) fn compile(request: &CompileRequest) -> Result<WorkerReply> {
    request.validate()?;
    let run = || -> crate::Result<BTreeMap<String, Vec<u8>>> {
        let loader = Loader {
            files: request
                .files
                .iter()
                .map(|f| (PathBuf::from(&f.path), f.content.clone()))
                .collect(),
        };
        let compilation = varta_compiler::compile_modules_with_profile(
            ModuleSource {
                path: PathBuf::from(&request.entry),
                text: loader.load(Path::new(&request.entry))?,
            },
            &loader,
            request.language_profile,
        )?;
        let mut manifest = varta_common::Manifest::new(
            &request
                .application
                .as_ref()
                .map_or_else(Vec::new, |app| app.grants()),
        );
        manifest.abi_version = request.language_profile.host_abi();
        manifest.entry = Some(request.entry.clone());
        manifest.target = Some("wasm".into());
        let artifact = crate::Artifact {
            wasm: compilation.wasm,
            manifest,
        };
        let prepared = crate::browser::BrowserArtifact::prepare(&artifact, None)?;
        // Check and build have identical browser admission; neither runs main.
        let bundle = prepared.bundle()?;
        Ok(if request.operation == Operation::Build {
            bundle.files().clone()
        } else {
            BTreeMap::new()
        })
    };
    match run() {
        Ok(files) => {
            if files.values().map(Vec::len).sum::<usize>() > MAX_ARTIFACT_BYTES {
                return Err(ToolchainError::Invalid("artifact byte limit"));
            }
            Ok(WorkerReply {
                result: CompilationResult {
                    ok: true,
                    diagnostics: Vec::new(),
                    artifact_sha256: String::new(),
                },
                files: files
                    .into_iter()
                    .map(|(p, b)| (p, base64::engine::general_purpose::STANDARD.encode(b)))
                    .collect(),
            })
        }
        Err(error) => Ok(WorkerReply {
            result: CompilationResult {
                ok: false,
                diagnostics: vec![diagnostic(&error, &request.entry)],
                artifact_sha256: String::new(),
            },
            files: BTreeMap::new(),
        }),
    }
}
