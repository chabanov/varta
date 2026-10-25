//! Validate public metadata against portable code without instantiating it.
use crate::RuntimeError;
use std::collections::{BTreeMap, BTreeSet};
use varta_common::{ToolCatalog, ToolType, abi};
use wasmparser::{ExternalKind, Operator, Parser, Payload};
use wasmtime::{ExternType, Module, ValType};

fn invalid(message: impl Into<String>) -> RuntimeError {
    varta_common::ToolContractError(message.into()).into()
}
fn parsed<T>(value: Result<T, wasmparser::BinaryReaderError>) -> Result<T, RuntimeError> {
    value.map_err(|e| invalid(e.to_string()))
}

pub(super) fn validate(module: &Module, bytes: &[u8]) -> Result<Option<ToolCatalog>, RuntimeError> {
    let mut catalog = None;
    let mut abi_sections = 0;
    let mut has_start = false;
    for payload in Parser::new(0).parse_all(bytes) {
        match parsed(payload)? {
            Payload::CustomSection(section) if section.name() == varta_common::TOOL_SECTION => {
                if catalog.is_some() {
                    return Err(invalid("duplicate tool catalog section"));
                }
                catalog = Some(ToolCatalog::parse(section.data())?);
            }
            Payload::CustomSection(section) if section.name() == abi::SECTION => abi_sections += 1,
            Payload::StartSection { .. } => has_start = true,
            _ => {}
        }
    }
    let Some(catalog) = catalog else {
        if module.exports().any(|export| reserved(export.name())) {
            return Err(invalid("tool exports require a catalog"));
        }
        return Ok(None);
    };
    if abi_sections != 1 || has_start {
        return Err(invalid(
            "tool modules require one host ABI section and no Wasm start function",
        ));
    }
    let strings = catalog.tools.iter().any(|tool| tool.uses_strings());
    let mut expected: BTreeSet<&str> = catalog
        .tools
        .iter()
        .map(|tool| tool.export.as_str())
        .collect();
    for tool in &catalog.tools {
        let params: Vec<_> = tool
            .parameters
            .iter()
            .map(|p| wasm_type(p.value_type))
            .collect();
        let results = if tool.result == ToolType::Void {
            vec![]
        } else {
            vec![wasm_type(tool.result)]
        };
        signature(module, &tool.export, &params, &results)?;
    }
    if strings {
        signature(
            module,
            varta_common::TOOL_COPY_STRING,
            &[ValType::I32, ValType::I32],
            &[ValType::I64],
        )?;
        signature(
            module,
            varta_common::TOOL_RETAIN_STRING,
            &[ValType::I64],
            &[ValType::I64],
        )?;
        expected.extend([
            varta_common::TOOL_COPY_STRING,
            varta_common::TOOL_RETAIN_STRING,
        ]);
        match module.get_export("memory") {
            Some(ExternType::Memory(memory))
                if !memory.is_64()
                    && !memory.is_shared()
                    && memory.minimum()
                        >= (abi::OWNED_STRING_BASE / abi::WASM_PAGE_BYTES) as u64
                    && memory.maximum().is_some_and(|max| {
                        max <= (abi::MAX_MEMORY_BYTES / abi::WASM_PAGE_BYTES) as u64
                    }) => {}
            _ => return Err(invalid("string tools require bounded exported ABI memory")),
        }
    }
    if module
        .exports()
        .any(|export| reserved(export.name()) && !expected.contains(export.name()))
    {
        return Err(invalid("unlisted or unknown reserved tool export"));
    }
    check_effects(module, bytes, &catalog)?;
    Ok(Some(catalog))
}
fn reserved(name: &str) -> bool {
    name.starts_with(varta_common::TOOL_EXPORT_PREFIX)
        || name.starts_with(varta_common::TOOL_ABI_EXPORT_PREFIX)
}
fn wasm_type(ty: ToolType) -> ValType {
    if ty == ToolType::String {
        ValType::I64
    } else {
        ValType::I32
    }
}
fn signature(
    module: &Module,
    name: &str,
    params: &[ValType],
    results: &[ValType],
) -> Result<(), RuntimeError> {
    let Some(ExternType::Func(ty)) = module.get_export(name) else {
        return Err(invalid(format!("missing tool function export: {name}")));
    };
    if ty.params().len() != params.len()
        || ty.results().len() != results.len()
        || !ty.params().zip(params).all(|(a, b)| ValType::eq(&a, b))
        || !ty.results().zip(results).all(|(a, b)| ValType::eq(&a, b))
    {
        return Err(invalid(format!("tool signature mismatch: {name}")));
    }
    Ok(())
}

fn check_effects(module: &Module, bytes: &[u8], catalog: &ToolCatalog) -> Result<(), RuntimeError> {
    let mut direct: Vec<BTreeSet<String>> = module
        .imports()
        .map(|import| {
            varta_common::HostOperation::from_name(import.name())
                .and_then(|op| op.capability())
                .map(str::to_owned)
                .into_iter()
                .collect()
        })
        .collect();
    let mut calls = vec![BTreeSet::new(); direct.len()];
    let mut exports = BTreeMap::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match parsed(payload)? {
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = parsed(export)?;
                    if export.kind == ExternalKind::Func {
                        exports.insert(export.name, export.index as usize);
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                let mut targets = BTreeSet::new();
                let mut reader = parsed(body.get_operators_reader())?;
                while !reader.eof() {
                    match parsed(reader.read())? {
                        Operator::Call { function_index }
                        | Operator::ReturnCall { function_index } => {
                            targets.insert(function_index as usize);
                        }
                        Operator::CallIndirect { .. }
                        | Operator::ReturnCallIndirect { .. }
                        | Operator::CallRef { .. }
                        | Operator::ReturnCallRef { .. } => {
                            return Err(invalid(
                                "indirect calls are outside tool invocation ABI v1",
                            ));
                        }
                        _ => {}
                    }
                }
                direct.push(BTreeSet::new());
                calls.push(targets);
            }
            _ => {}
        }
    }
    // Input transfer executes before a handler. It must not hide host effects
    // outside the handler's declared call graph (including unprivileged logging).
    for name in [
        varta_common::TOOL_COPY_STRING,
        varta_common::TOOL_RETAIN_STRING,
    ] {
        if let Some(index) = exports.get(name) {
            let imported = module.imports().len();
            if reachable(*index, &calls)?
                .iter()
                .any(|index| *index < imported)
            {
                return Err(invalid("tool string ABI helpers cannot call host imports"));
            }
        }
    }
    for tool in &catalog.tools {
        let index = *exports
            .get(tool.export.as_str())
            .ok_or_else(|| invalid("missing tool export index"))?;
        if index < module.imports().len() {
            return Err(invalid("tool handlers must be guest-defined functions"));
        }
        let mut effects = BTreeSet::new();
        for index in reachable(index, &calls)? {
            effects.extend(direct[index].iter().cloned());
        }
        if effects.into_iter().collect::<Vec<_>>() != tool.required_effects {
            return Err(invalid(format!("tool effect mismatch: {}", tool.name)));
        }
    }
    Ok(())
}

fn reachable(index: usize, calls: &[BTreeSet<usize>]) -> Result<BTreeSet<usize>, RuntimeError> {
    let mut pending = vec![index];
    let mut visited = BTreeSet::new();
    while let Some(index) = pending.pop() {
        if visited.insert(index) {
            let targets = calls
                .get(index)
                .ok_or_else(|| invalid("invalid tool call target"))?;
            pending.extend(
                targets
                    .iter()
                    .filter(|target| !visited.contains(target))
                    .copied(),
            );
        }
    }
    Ok(visited)
}
