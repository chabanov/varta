//! Artifact-level admission of the resumable-frame surface (K4.4 step 1).
//! The rules are the shared `varta_common::function::admission` registry that
//! runtime preflight also applies; this module only extracts their input, so
//! an artifact whose frame surface its declared profile does not admit is
//! refused before packaging, inspection or execution.
use crate::invalid;
use varta_common::function::{
    CATALOG_SECTION,
    admission::{self, DispatcherCode, FrameSurface, FunctionSignature, ResumeExport, WasmScalar},
    frame,
};
use varta_common::language::{self, LanguageContract, LanguageProfile};
use varta_common::{Result, VartaError, abi};
use wasmparser::{ExternalKind, Operator, Parser, Payload, TypeRef, ValType};

fn parsed<T>(value: std::result::Result<T, wasmparser::BinaryReaderError>) -> Result<T> {
    value.map_err(|e| invalid(&e.to_string()))
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
fn signature(types: &[FunctionSignature], index: u32) -> Result<FunctionSignature> {
    types
        .get(index as usize)
        .cloned()
        .ok_or_else(|| invalid("function type index out of range"))
}
fn rejected(error: admission::FrameAdmissionError) -> VartaError {
    invalid(&error.to_string())
}

/// Declared metadata and the frame surface of one portable payload.
#[derive(Default)]
struct Extracted<'a> {
    host: Option<u32>,
    contract: Option<LanguageContract>,
    catalog: Option<&'a [u8]>,
    surface: FrameSurface,
}

fn extract(wasm: &[u8]) -> Result<Extracted<'_>> {
    let mut types = Vec::new();
    let mut found = Extracted::default();
    let surface = &mut found.surface;
    let mut defined = 0usize;
    for payload in Parser::new(0).parse_all(wasm) {
        match parsed(payload)? {
            Payload::CustomSection(section) if section.name() == abi::SECTION => {
                found.host = section.data().try_into().ok().map(u32::from_le_bytes);
            }
            Payload::CustomSection(section) if section.name() == language::SECTION => {
                if found.contract.is_some() {
                    return Err(invalid("duplicate language section"));
                }
                found.contract = Some(
                    LanguageContract::parse(section.data()).map_err(|e| invalid(&e.to_string()))?,
                );
            }
            Payload::CustomSection(section) if section.name() == CATALOG_SECTION => {
                if found.catalog.replace(section.data()).is_some() {
                    return Err(invalid("duplicate function capture catalog"));
                }
            }
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
    Ok(found)
}
fn dispatcher(body: wasmparser::FunctionBody<'_>) -> Result<DispatcherCode> {
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

/// Check the declared profile against its host ABI and admit the frame
/// surface with the shared rules. Payloads without a declared profile admit
/// no frame surface.
pub(crate) fn validate(wasm: &[u8]) -> Result<()> {
    let found = extract(wasm)?;
    let profile = match found.contract {
        Some(contract) if found.host == Some(contract.profile.host_abi()) => contract.profile,
        Some(_) => {
            return Err(invalid(
                "declared profile requires its matching host ABI section",
            ));
        }
        None => LanguageProfile::default(),
    };
    // The payload within its declared profile's module-size limit (v38: the
    // Wasm limit; earlier profiles: the frozen 4 MiB).
    if wasm.len() > profile.limits().wasm_bytes {
        return Err(invalid("Wasm exceeds its profile's size limit"));
    }
    let catalog = found
        .catalog
        .map(|bytes| admission::read_catalog(bytes, profile))
        .transpose()
        .map_err(rejected)?;
    admission::admit(&found.surface, profile, catalog.as_ref()).map_err(rejected)
}
