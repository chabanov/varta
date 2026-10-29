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
    if request.source.len() > MAX_SOURCE_BYTES {
        return Observation::Infrastructure {
            category: InfrastructureFailure::Protocol,
            detail: "source exceeds worker limit".into(),
        };
    }
    let bytes =
        match varta_compiler::compile_typescript_with_profile(&request.source, request.profile) {
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
    match varta_runtime::VartaRuntime::new().run_wasm_bytes_with_policy(&bytes, &grants, &grants) {
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
