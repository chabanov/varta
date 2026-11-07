//! Compile the exact snapshot and run it with fresh default limits and only
//! the case's reviewed grants (none unless the inventory lists them).
use crate::{inventory::MAX_SOURCE_BYTES, model::*};
use varta_common::DiagnosticCode;
use varta_compiler::CompilerError;
pub(crate) const MAX_DETAIL: usize = 2048;
pub(crate) fn detail(text: impl ToString) -> String {
    text.to_string().chars().take(MAX_DETAIL).collect()
}
fn code(error: &CompilerError) -> CompileCode {
    if let Some(d) = error.diagnostic() {
        return match d.code {
            DiagnosticCode::SyntaxError => CompileCode::SyntaxError,
            DiagnosticCode::TypeError => CompileCode::TypeError,
            DiagnosticCode::ModuleError => CompileCode::ModuleError,
            DiagnosticCode::UnsupportedSyntax => CompileCode::UnsupportedSyntax,
            DiagnosticCode::InvalidProgram => CompileCode::InvalidProgram,
            DiagnosticCode::InternalError => CompileCode::InternalError,
        };
    }
    match error {
        CompilerError::Parse(_) => CompileCode::SyntaxError,
        CompilerError::Unsupported { .. } => CompileCode::UnsupportedSyntax,
        CompilerError::Invalid(_) => CompileCode::InvalidProgram,
        CompilerError::Type(_) => CompileCode::TypeError,
        CompilerError::Module(_) => CompileCode::ModuleError,
        CompilerError::Located { source, .. } => code(source),
        CompilerError::Internal(_) => CompileCode::InternalError,
    }
}
pub(crate) fn execute(request: Request) -> Observation {
    if request.modules.len() > MAX_CASE_MODULES
        || request.source.len().saturating_add(
            request
                .modules
                .iter()
                .map(|m| m.source.len())
                .sum::<usize>(),
        ) > MAX_SOURCE_BYTES
    {
        return Observation::Infrastructure {
            category: InfrastructureFailure::Protocol,
            detail: "source exceeds worker limit".into(),
        };
    }
    let bytes = match compile(&request) {
        Ok(bytes) => bytes,
        Err(error) => {
            let code = code(&error);
            return if code == CompileCode::InternalError {
                Observation::Infrastructure {
                    category: InfrastructureFailure::InternalCompiler,
                    detail: detail(error),
                }
            } else {
                Observation::CompileRejected {
                    code,
                    detail: detail(error),
                }
            };
        }
    };
    // The reviewed case grants are both the package request and the host
    // ceiling; every other capability stays denied.
    let grants: Vec<_> = request
        .grants
        .iter()
        .filter(|id| CASE_GRANTS.contains(&id.as_str()))
        .map(|id| varta_common::CapabilityGrant {
            id: id.clone(),
            value: varta_common::CapabilityValue::Boolean(true),
        })
        .collect();
    let options = varta_runtime::RunOptions {
        requested: grants.clone(),
        ceiling: Some(grants),
        host_inputs: request.host_inputs.unwrap_or_default(),
        ..varta_runtime::RunOptions::default()
    };
    match varta_runtime::VartaRuntime::new()
        .run_with(&bytes, options)
        .into_result()
    {
        Ok(output) => Observation::Executed {
            lines: output.lines,
            exit_code: output.exit_code,
        },
        Err(error) => {
            let category = match &error {
                varta_runtime::RuntimeError::Uncaught(_) => RuntimeFailure::UncaughtException,
                varta_runtime::RuntimeError::Execution(_) => RuntimeFailure::Execution,
                _ => RuntimeFailure::Validation,
            };
            Observation::RuntimeFailed {
                category,
                detail: detail(error),
            }
        }
    }
}

/// Pure in-memory loader. No file or network lookup is available to the worker.
struct Snapshot(std::collections::BTreeMap<std::path::PathBuf, String>);
fn module_error(message: &str) -> CompilerError {
    CompilerError::Module(message.into())
}
impl varta_compiler::ModuleLoader for Snapshot {
    fn resolve(
        &self,
        importer: &std::path::Path,
        specifier: &str,
    ) -> Result<std::path::PathBuf, CompilerError> {
        if !specifier.starts_with("./") && !specifier.starts_with("../") {
            return Err(module_error("case imports must be relative"));
        }
        let mut path = importer
            .parent()
            .unwrap_or_else(|| std::path::Path::new(""))
            .to_path_buf();
        for part in std::path::Path::new(specifier).components() {
            match part {
                std::path::Component::CurDir => {}
                std::path::Component::Normal(p) => path.push(p),
                std::path::Component::ParentDir if path.components().count() > 1 => {
                    path.pop();
                }
                _ => return Err(module_error("case import escaped snapshot")),
            }
        }
        if self.0.contains_key(&path) {
            Ok(path)
        } else {
            Err(module_error("case import is not in the pinned snapshot"))
        }
    }
    fn load(&self, path: &std::path::Path) -> Result<String, CompilerError> {
        self.0
            .get(path)
            .cloned()
            .ok_or_else(|| module_error("case module is not in the pinned snapshot"))
    }
}
fn snapshot_path(path: &str) -> bool {
    path.starts_with("cases/")
        && path.ends_with(".ts")
        && path.len() <= 240
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
        && path
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_/ .".contains(&b))
}
fn compile(request: &Request) -> Result<Vec<u8>, CompilerError> {
    if request.modules.is_empty() {
        return varta_compiler::compile_typescript_with_profile(&request.source, request.profile);
    }
    let entry = request
        .source_path
        .as_deref()
        .filter(|p| snapshot_path(p))
        .ok_or_else(|| module_error("invalid snapshot entry path"))?;
    let mut loader = Snapshot(Default::default());
    loader.0.insert(entry.into(), request.source.clone());
    for module in &request.modules {
        if !snapshot_path(&module.path)
            || loader
                .0
                .insert(module.path.clone().into(), module.source.clone())
                .is_some()
        {
            return Err(module_error("invalid or duplicate snapshot module path"));
        }
    }
    varta_compiler::compile_modules_with_profile(
        varta_compiler::ModuleSource {
            path: entry.into(),
            text: request.source.clone(),
        },
        &loader,
        request.profile,
    )
    .map(|c| c.wasm)
}
