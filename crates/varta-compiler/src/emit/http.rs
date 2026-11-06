//! The emitted closed resume dispatcher can reach scheduled bodies beyond the
//! source handler's direct calls. Describe that actual graph (of the handler
//! and, for a streaming service, of `pull`); hosts verify it.
use super::*;
use varta_common::http::HttpContract;
use wasmparser::{ExternalKind, Operator, Parser, Payload, TypeRef};

pub(super) fn contract(bytes: &[u8], original: &HttpContract) -> Result<HttpContract> {
    let parsed = |e: wasmparser::BinaryReaderError| CompilerError::Internal(e.to_string());
    let mut effects = Vec::new();
    let mut calls = Vec::new();
    let mut entries = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(parsed)? {
            Payload::ImportSection(reader) => {
                for import in reader {
                    let import = import.map_err(parsed)?;
                    if matches!(import.ty, TypeRef::Func(_)) {
                        effects.push(Host::from_name(import.name).and_then(|op| op.capability()));
                        calls.push(Vec::new());
                    }
                }
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.map_err(parsed)?;
                    // A streaming service's pull runs guest code too.
                    if export.kind == ExternalKind::Func
                        && (export.name == original.export
                            || original.streaming && export.name == varta_common::http::PULL)
                    {
                        entries.push(export.index as usize);
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                let mut targets = Vec::new();
                let mut ops = body.get_operators_reader().map_err(parsed)?;
                while !ops.eof() {
                    match ops.read().map_err(parsed)? {
                        Operator::Call { function_index }
                        | Operator::ReturnCall { function_index } => {
                            targets.push(function_index as usize)
                        }
                        _ => {}
                    }
                }
                effects.push(None);
                calls.push(targets);
            }
            _ => {}
        }
    }
    if entries.len() != 1 + usize::from(original.streaming) {
        return Err(CompilerError::Internal("missing HTTP export".into()));
    }
    let mut pending = entries;
    let mut seen = BTreeSet::new();
    let mut required = BTreeSet::from([varta_common::http::LISTEN_CAPABILITY.to_owned()]);
    while let Some(index) = pending.pop() {
        if !seen.insert(index) {
            continue;
        }
        let next = calls
            .get(index)
            .ok_or_else(|| CompilerError::Internal("invalid HTTP call target".into()))?;
        if let Some(effect) = effects[index] {
            required.insert(effect.to_owned());
        }
        pending.extend(next);
    }
    let mut contract = original.clone();
    contract.required_effects = required.into_iter().collect();
    Ok(contract)
}
