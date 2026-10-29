//! Validate public metadata against portable code without instantiating it.
use crate::RuntimeError;
use std::collections::{BTreeMap, BTreeSet};
use varta_common::{ToolCatalog, ToolType, abi};
use wasmparser::{ExternalKind, Operator, Parser, Payload};
use wasmtime::{ExternType, Module, ValType};

fn invalid(message: impl Into<String>) -> RuntimeError {
    varta_common::ToolContractError(message.into()).into()
}
fn invalid_value(message: impl Into<String>) -> RuntimeError {
    varta_common::value::ValueAbiError(message.into()).into()
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
    let http = crate::http_contract::read(bytes)?;
    let marker = module
        .imports()
        .any(|i| i.name() == varta_common::HostOperation::HttpService.name());
    if http.is_some() != marker {
        return Err(varta_common::http::HttpError::Contract(
            "service metadata and HTTP feature import must occur together".into(),
        )
        .into());
    }
    let http_names = [
        varta_common::http::HANDLER,
        varta_common::http::COPY,
        varta_common::http::RETAIN,
    ];
    if module.exports().any(|e| {
        e.name().starts_with(varta_common::http::ABI_PREFIX)
            && (http.is_none() || !http_names.contains(&e.name()))
    }) {
        return Err(varta_common::http::HttpError::Contract(
            "unknown or unused HTTP ABI export".into(),
        )
        .into());
    }
    if http.is_some() {
        if abi_sections != 1 || has_start {
            return Err(varta_common::http::HttpError::Contract(
                "HTTP requires one host ABI section and no Wasm start".into(),
            )
            .into());
        }
        signature(module, http_names[0], &[ValType::I64], &[ValType::I64])?;
        signature(
            module,
            http_names[1],
            &[ValType::I32, ValType::I32],
            &[ValType::I64],
        )?;
        signature(module, http_names[2], &[ValType::I64], &[ValType::I64])?;
        bounded_memory(module, true)?;
    }
    let parses = module.imports().any(|i| {
        matches!(
            varta_common::HostOperation::from_name(i.name()),
            Some(
                varta_common::HostOperation::JsonParse
                    | varta_common::HostOperation::JsonParseV3
                    | varta_common::HostOperation::JsonParseV4
                    | varta_common::HostOperation::JsonParseV5
                    | varta_common::HostOperation::JsonParseV6
                    | varta_common::HostOperation::JsonParseV7
                    | varta_common::HostOperation::JsonParseV8
                    | varta_common::HostOperation::JsonParseV9
                    | varta_common::HostOperation::JsonParseV10
                    | varta_common::HostOperation::JsonParseV11
                    | varta_common::HostOperation::JsonParseV12
                    | varta_common::HostOperation::JsonParseV13
                    | varta_common::HostOperation::JsonParseV14
                    | varta_common::HostOperation::JsonParseV15
                    | varta_common::HostOperation::JsonParseV16
                    | varta_common::HostOperation::JsonParseV17
            )
        )
    });
    let value_exports = [
        varta_common::value::VALUE_ALLOCATE,
        varta_common::value::VALUE_RETAIN,
    ];
    if module.exports().any(|e| {
        e.name().starts_with(varta_common::value::VALUE_ABI_PREFIX)
            && (!parses || !value_exports.contains(&e.name()))
    }) {
        return Err(invalid_value("unknown or unused value ABI export"));
    }
    if parses {
        if abi_sections != 1 || has_start {
            return Err(invalid_value(
                "JSON parsing requires one host ABI section and no Wasm start function",
            ));
        }
        signature(module, value_exports[0], &[ValType::I32], &[ValType::I32])?;
        signature(module, value_exports[1], &[ValType::I64], &[ValType::I64])?;
    }
    let Some(catalog) = catalog else {
        if module.exports().any(|export| reserved(export.name())) {
            return Err(invalid("tool exports require a catalog"));
        }
        if parses || http.is_some() {
            bounded_memory(module, true)?;
            check_effects(module, bytes, None, http.as_ref())?;
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
    }
    if strings || parses {
        bounded_memory(module, parses)?;
    }

    if module
        .exports()
        .any(|export| reserved(export.name()) && !expected.contains(export.name()))
    {
        return Err(invalid("unlisted or unknown reserved tool export"));
    }
    check_effects(module, bytes, Some(&catalog), http.as_ref())?;
    Ok(Some(catalog))
}
fn bounded_memory(module: &Module, values: bool) -> Result<(), RuntimeError> {
    match module.get_export("memory") {
        Some(ExternType::Memory(memory))
            if !memory.is_64()
                && !memory.is_shared()
                && memory.minimum() >= (abi::OWNED_STRING_BASE / abi::WASM_PAGE_BYTES) as u64
                && memory.maximum().is_some_and(|max| {
                    max <= (abi::MAX_MEMORY_BYTES / abi::WASM_PAGE_BYTES) as u64
                }) =>
        {
            Ok(())
        }
        _ if values => Err(invalid_value(
            "JSON parsing requires bounded exported ABI memory",
        )),
        _ => Err(invalid("string tools require bounded exported ABI memory")),
    }
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
    let values = name.starts_with(varta_common::value::VALUE_ABI_PREFIX);
    let http = name.starts_with(varta_common::http::ABI_PREFIX);
    let subject = if http {
        "HTTP ABI"
    } else if values {
        "value ABI"
    } else {
        "tool"
    };
    let fail = |message| {
        if http {
            varta_common::http::HttpError::Contract(message).into()
        } else if values {
            invalid_value(message)
        } else {
            invalid(message)
        }
    };
    let Some(ExternType::Func(ty)) = module.get_export(name) else {
        return Err(fail(format!("missing {subject} function export: {name}")));
    };
    if ty.params().len() != params.len()
        || ty.results().len() != results.len()
        || !ty.params().zip(params).all(|(a, b)| ValType::eq(&a, b))
        || !ty.results().zip(results).all(|(a, b)| ValType::eq(&a, b))
    {
        return Err(fail(format!("{subject} signature mismatch: {name}")));
    }
    Ok(())
}

fn check_effects(
    module: &Module,
    bytes: &[u8],
    catalog: Option<&ToolCatalog>,
    http: Option<&varta_common::http::HttpContract>,
) -> Result<(), RuntimeError> {
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
                            return Err(if catalog.is_none() {
                                invalid_value("indirect calls are outside the managed value ABI")
                            } else {
                                invalid("indirect calls are outside tool invocation ABI v1")
                            });
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
        varta_common::value::VALUE_ALLOCATE,
        varta_common::value::VALUE_RETAIN,
        varta_common::http::COPY,
        varta_common::http::RETAIN,
    ] {
        if let Some(index) = exports.get(name) {
            let imported = module.imports().len();
            if reachable(*index, &calls)?
                .iter()
                .any(|index| *index < imported)
            {
                return Err(if name.starts_with(varta_common::http::ABI_PREFIX) {
                    varta_common::http::HttpError::Contract(
                        "HTTP string ABI helpers cannot call host imports".into(),
                    )
                    .into()
                } else if name.starts_with(varta_common::value::VALUE_ABI_PREFIX) {
                    invalid_value("value ABI helpers cannot call host imports")
                } else {
                    invalid("tool string ABI helpers cannot call host imports")
                });
            }
        }
    }
    if let Some(http) = http {
        let index = *exports
            .get(http.export.as_str())
            .ok_or_else(|| invalid_value("missing HTTP handler index"))?;
        if index < module.imports().len() {
            return Err(invalid_value("HTTP handlers must be guest-defined"));
        }
        let mut effects = BTreeSet::from(["net.listen".to_owned()]);
        for target in reachable(index, &calls)? {
            effects.extend(direct[target].iter().cloned());
        }
        if effects.into_iter().collect::<Vec<_>>() != http.required_effects {
            return Err(
                varta_common::http::HttpError::Contract("HTTP effect mismatch".into()).into(),
            );
        }
    }
    for tool in catalog.into_iter().flat_map(|c| &c.tools) {
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
