//! Preflight admission of the resumable-frame surface (K4.4 step 1). The
//! shared rules live in `varta_common::function::admission`; this module only
//! extracts their input from portable Wasm. The runtime never calls
//! `varta_resume` yet: no scheduler exists, so an admitted module runs its
//! entry point exactly as before.
use crate::RuntimeError;
use varta_common::function::{
    FunctionCatalog,
    admission::{self, DispatcherCode, FrameSurface, FunctionSignature, ResumeExport, WasmScalar},
    frame,
};
use varta_common::language::LanguageProfile;
use wasmparser::{ExternalKind, Operator, Parser, Payload, TypeRef, ValType};

fn parsed<T>(value: Result<T, wasmparser::BinaryReaderError>) -> Result<T, RuntimeError> {
    value.map_err(|e| wasmtime::Error::msg(e.to_string()).into())
}
fn scalar(ty: ValType) -> WasmScalar {
    match ty {
        ValType::I32 => WasmScalar::I32,
        ValType::I64 => WasmScalar::I64,
        ValType::F32 => WasmScalar::F32,
        ValType::F64 => WasmScalar::F64,
        ValType::V128 | ValType::Ref(_) => WasmScalar::Other,
    }
}
fn signature(types: &[FunctionSignature], index: u32) -> Result<FunctionSignature, RuntimeError> {
    types
        .get(index as usize)
        .cloned()
        .ok_or_else(|| wasmtime::Error::msg("function type index out of range").into())
}

/// Collect the facts the shared admission rules check.
fn surface(bytes: &[u8]) -> Result<FrameSurface, RuntimeError> {
    let mut types = Vec::new();
    let mut surface = FrameSurface::default();
    let mut defined = 0usize;
    for payload in Parser::new(0).parse_all(bytes) {
        match parsed(payload)? {
            Payload::TypeSection(reader) => {
                for ty in reader.into_iter_err_on_gc_types() {
                    let ty = parsed(ty)?;
                    types.push(FunctionSignature {
                        params: ty.params().iter().copied().map(scalar).collect(),
                        results: ty.results().iter().copied().map(scalar).collect(),
                    });
                }
            }
            Payload::ImportSection(reader) => {
                for import in reader {
                    if let TypeRef::Func(index) = parsed(import)?.ty {
                        surface.functions.push(signature(&types, index)?);
                        surface.imported_functions += 1;
                    }
                }
            }
            Payload::FunctionSection(reader) => {
                for index in reader {
                    surface.functions.push(signature(&types, parsed(index)?)?);
                }
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = parsed(export)?;
                    let function = export.kind == ExternalKind::Func;
                    if function {
                        surface.exported_functions.insert(export.index);
                    }
                    if export.name == frame::RESUME_EXPORT {
                        surface.resume_export = Some(if function {
                            ResumeExport::Function(export.index)
                        } else {
                            ResumeExport::NotFunction
                        });
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                let index = surface.imported_functions + defined;
                defined += 1;
                if surface.resume_export == Some(ResumeExport::Function(index as u32)) {
                    surface.dispatcher = Some(dispatcher(body)?);
                }
            }
            _ => {}
        }
    }
    Ok(surface)
}
fn dispatcher(body: wasmparser::FunctionBody<'_>) -> Result<DispatcherCode, RuntimeError> {
    let mut code = DispatcherCode::default();
    let mut reader = parsed(body.get_operators_reader())?;
    while !reader.eof() {
        match parsed(reader.read())? {
            Operator::Call { function_index } | Operator::ReturnCall { function_index } => {
                code.calls.push(function_index);
            }
            Operator::CallIndirect { .. }
            | Operator::ReturnCallIndirect { .. }
            | Operator::CallRef { .. }
            | Operator::ReturnCallRef { .. }
            | Operator::TableGet { .. }
            | Operator::TableSet { .. }
            | Operator::TableGrow { .. }
            | Operator::TableSize { .. }
            | Operator::TableFill { .. }
            | Operator::TableCopy { .. }
            | Operator::TableInit { .. }
            | Operator::ElemDrop { .. } => code.indirect = true,
            _ => {}
        }
    }
    Ok(code)
}

/// Admit or reject the module's frame surface before compilation.
pub(crate) fn admit(
    bytes: &[u8],
    profile: LanguageProfile,
    catalog: Option<&FunctionCatalog>,
) -> Result<(), RuntimeError> {
    Ok(admission::admit(&surface(bytes)?, profile, catalog)?)
}
