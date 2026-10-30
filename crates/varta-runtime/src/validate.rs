use crate::RuntimeError;
use varta_common::abi;
use wasmtime::{Engine, Module};

pub(crate) fn module(
    engine: &Engine,
    bytes: &[u8],
) -> Result<(crate::PreparedModule, Option<varta_common::ToolCatalog>), RuntimeError> {
    let (language, function_catalog, static_region) = preflight(bytes)?;
    let version = language.profile.map_or(abi::VERSION, |p| p.host_abi());
    if bytes.len() > abi::MAX_WASM_BYTES || !bytes.starts_with(b"\0asm\x01\0\0\0") {
        return Err(wasmtime::Error::msg("expected a bounded portable Wasm module").into());
    }
    let module = Module::new(engine, bytes)?;
    for import in module.imports() {
        if import.module() != "varta" || varta_common::host_capability(import.name()).is_none() {
            return Err(wasmtime::Error::msg(format!(
                "unknown host import: {}.{}",
                import.module(),
                import.name()
            ))
            .into());
        }
        let wasmtime::ExternType::Func(ty) = import.ty() else {
            return Err(wasmtime::Error::msg("only function imports are supported").into());
        };
        let host = varta_common::HostOperation::from_name(import.name())
            .ok_or_else(|| error("unknown host operation"))?;
        let (params, results) = host.scalar_signature();
        let matches =
            |actual: wasmtime::ValType, expected: &varta_common::HostScalar| match expected {
                varta_common::HostScalar::I32 => matches!(actual, wasmtime::ValType::I32),
                varta_common::HostScalar::F64 => matches!(actual, wasmtime::ValType::F64),
                varta_common::HostScalar::I64 => matches!(actual, wasmtime::ValType::I64),
            };
        if language
            .profile
            .is_some_and(|p| p.host_operation(host) != host)
            || host.minimum_abi() > version
            || ty.params().len() != params.len()
            || ty.results().len() != results.len()
            || !ty.params().zip(params).all(|(a, b)| matches(a, b))
            || !ty.results().zip(results).all(|(a, b)| matches(a, b))
        {
            return Err(
                wasmtime::Error::msg(format!("invalid host signature: {}", import.name())).into(),
            );
        }
    }
    if let Some(export) = module.get_export("_start") {
        let wasmtime::ExternType::Func(ty) = export else {
            return Err(wasmtime::Error::msg("_start must be a function").into());
        };
        if ty.params().len() != 0 || ty.results().len() != 0 {
            return Err(wasmtime::Error::msg("_start must have signature () -> ()").into());
        }
    }
    crate::exceptions::validate(&module, language.profile.unwrap_or_default())?;
    check_exports(&module)?;
    let tools = crate::contracts::validate(&module, bytes)?;
    Ok((
        crate::PreparedModule {
            module,
            function_catalog,
            static_region,
            uncharged_initialization: language.profile.is_some_and(|p| p.program_limits()),
        },
        tools,
    ))
}

fn error(message: &str) -> RuntimeError {
    wasmtime::Error::msg(message.to_owned()).into()
}
pub(crate) fn value_version(bytes: &[u8]) -> Result<Option<u32>, RuntimeError> {
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let wasmparser::Payload::CustomSection(section) =
            payload.map_err(|e| error(&e.to_string()))?
            && section.name() == varta_common::value::VALUE_SECTION
        {
            return Ok(Some(u32::from_le_bytes(
                section
                    .data()
                    .try_into()
                    .map_err(|_| error("invalid structured value ABI"))?,
            )));
        }
    }
    Ok(None)
}

fn preflight(
    bytes: &[u8],
) -> Result<
    (
        varta_common::language::LanguageInspection,
        Option<std::sync::Arc<varta_common::function::VerifiedFunctionCatalog>>,
        abi::StaticRegion,
    ),
    RuntimeError,
> {
    use wasmparser::{Parser, Payload};
    if bytes.len() > abi::MAX_WASM_BYTES {
        return Err(error("Wasm exceeds size limit"));
    }
    let language = crate::language::inspect(bytes)?;
    let mut values = 0;
    let mut function_catalog = None;
    let mut union_imports = false;
    let mut json_imports = std::collections::BTreeSet::new();
    let mut http = false;
    let mut tools = false;
    let mut value_version = None;
    let mut static_region = None;
    let mut memory_initial = None;
    let mut data_segments = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|e| error(&e.to_string()))? {
            Payload::StartSection { .. } if language.profile.is_some_and(|p| p.exceptions()) => {
                return Err(error("exception profile forbids a Wasm start function"));
            }
            Payload::TypeSection(reader) => {
                for (count, ty) in reader.into_iter_err_on_gc_types().enumerate() {
                    let ty = ty.map_err(|e| error(&e.to_string()))?;
                    if count >= abi::MAX_WASM_SIGNATURES
                        || ty.params().len() > abi::MAX_WASM_PARAMETERS
                        || ty.results().len() > 2
                    {
                        return Err(error("Wasm signature complexity exceeds limits"));
                    }
                }
            }
            Payload::ImportSection(reader) if reader.count() > 64 => {
                return Err(error("too many imports"));
            }
            Payload::ImportSection(reader) => {
                for import in reader {
                    let import = import.map_err(|e| error(&e.to_string()))?;
                    union_imports |= import.module == "varta"
                        && varta_common::HostOperation::from_name(import.name).is_some_and(|op| {
                            matches!(
                                op.minimum_abi(),
                                abi::UNION_VERSION
                                    | abi::REFERENCE_VERSION
                                    | abi::VECTOR_VERSION
                                    | abi::ERROR_VERSION
                                    | abi::ERROR_FAMILY_VERSION
                                    | abi::SPARSE_VERSION
                            )
                        });

                    if import.module == "varta"
                        && let Some(operation) = varta_common::HostOperation::from_name(import.name)
                        && matches!(
                            operation,
                            varta_common::HostOperation::JsonStringify
                                | varta_common::HostOperation::JsonParse
                                | varta_common::HostOperation::JsonStringifyV3
                                | varta_common::HostOperation::JsonParseV3
                                | varta_common::HostOperation::JsonStringifyV4
                                | varta_common::HostOperation::JsonParseV4
                                | varta_common::HostOperation::JsonParseV5
                                | varta_common::HostOperation::JsonStringifyV5
                                | varta_common::HostOperation::JsonParseV6
                                | varta_common::HostOperation::JsonStringifyV6
                                | varta_common::HostOperation::JsonStringifyV7
                                | varta_common::HostOperation::JsonParseV7
                                | varta_common::HostOperation::JsonStringifyV9
                                | varta_common::HostOperation::JsonStringifyV8
                                | varta_common::HostOperation::JsonParseV8
                                | varta_common::HostOperation::JsonParseV9
                                | varta_common::HostOperation::JsonParseV10
                                | varta_common::HostOperation::JsonParseV11
                                | varta_common::HostOperation::JsonParseV12
                                | varta_common::HostOperation::JsonParseV13
                                | varta_common::HostOperation::JsonParseV14
                                | varta_common::HostOperation::JsonStringifyV11
                                | varta_common::HostOperation::JsonStringifyV12
                                | varta_common::HostOperation::JsonStringifyV10
                                | varta_common::HostOperation::JsonStringifyV13
                                | varta_common::HostOperation::JsonParseV15
                                | varta_common::HostOperation::JsonStringifyV14
                                | varta_common::HostOperation::JsonParseV16
                                | varta_common::HostOperation::JsonStringifyV15
                                | varta_common::HostOperation::JsonParseV17
                                | varta_common::HostOperation::JsonStringifyV16
                                | varta_common::HostOperation::JsonStringifyV17
                                | varta_common::HostOperation::JsonParseV18
                        )
                    {
                        json_imports.insert(operation);
                    }
                }
            }
            Payload::FunctionSection(reader)
                if reader.count() as usize
                    > abi::MAX_USER_FUNCTIONS
                        + if language.profile.is_some_and(|p| p.array_mutations()) {
                            abi::MAX_INTERNAL_FUNCTIONS
                        } else if language.profile.is_some_and(|p| p.sparse_arrays()) {
                            abi::SPARSE_INTERNAL_FUNCTIONS
                        } else {
                            abi::LEGACY_INTERNAL_FUNCTIONS
                        }
                        + abi::MAX_HTTP_HANDLERS =>
            {
                return Err(error("too many functions"));
            }
            Payload::MemorySection(reader) => {
                if reader.count() > 1 {
                    return Err(error("only one memory is supported"));
                }
                for memory in reader {
                    let memory = memory.map_err(|e| error(&e.to_string()))?;
                    if memory.memory64
                        || memory.shared
                        || memory.initial > (abi::MAX_MEMORY_BYTES / abi::RESULT_BASE) as u64
                    {
                        return Err(error("unsupported memory type or initial size"));
                    }
                    memory_initial = Some(memory.initial);
                }
            }
            Payload::DataSection(reader)
                if language.profile.is_some_and(|p| p.program_limits()) =>
            {
                for segment in reader {
                    data_segments
                        .push(active_segment(segment.map_err(|e| error(&e.to_string()))?)?);
                }
            }
            Payload::CustomSection(section) if section.name() == abi::STATIC_SECTION => {
                if static_region.is_some() || language.profile.is_none_or(|p| !p.program_limits()) {
                    return Err(error("unsupported or duplicate static region"));
                }
                static_region = Some(
                    abi::StaticRegion::read_section(section.data())
                        .map_err(|e| error(&e.to_string()))?,
                );
            }
            Payload::TableSection(reader) => {
                if reader.count() > 1 {
                    return Err(error("only one table is supported"));
                }
                for table in reader {
                    let table = table.map_err(|e| error(&e.to_string()))?;
                    if table.ty.initial > 1024 {
                        return Err(error("table exceeds element budget"));
                    }
                }
            }
            Payload::GlobalSection(reader) if reader.count() > 128 => {
                return Err(error("too many globals"));
            }
            Payload::CodeSectionEntry(body) => {
                if body.range().len() > abi::MAX_WASM_FUNCTION_BODY_BYTES {
                    return Err(error("function body exceeds compilation budget"));
                }
            }
            Payload::CustomSection(section)
                if section.name() == varta_common::function::CATALOG_SECTION =>
            {
                if function_catalog.is_some()
                    || language
                        .profile
                        .is_none_or(|profile| !profile.property_deletion())
                {
                    return Err(error("unsupported or duplicate function capture catalog"));
                }
                // Version 2 frame catalogs are read only for resumable-frame profiles.
                let catalog = varta_common::function::admission::read_catalog(
                    section.data(),
                    language.profile.unwrap_or_default(),
                )?
                .into_verified()?;
                if Some(catalog.as_catalog().value_version)
                    != language.profile.map(|profile| profile.value_layout())
                {
                    return Err(error("function capture catalog value version mismatch"));
                }
                function_catalog = Some(catalog);
            }
            Payload::CustomSection(section)
                if section.name() == varta_common::value::VALUE_SECTION =>
            {
                values += 1;
                value_version = section.data().try_into().ok().map(u32::from_le_bytes);
                if values > 1
                    || !section.data().try_into().ok().is_some_and(|version| {
                        varta_common::value::supported_version(u32::from_le_bytes(version))
                    })
                {
                    return Err(error("unsupported or duplicate structured value ABI"));
                }
            }
            Payload::CustomSection(section) if section.name() == varta_common::http::SECTION => {
                http = true;
            }
            Payload::CustomSection(section) if section.name() == varta_common::TOOL_SECTION => {
                tools = true;
            }
            _ => {}
        }
    }
    let profile = language.profile.unwrap_or_default();
    let mismatch = value_version
        .is_some_and(|v| !varta_common::value::compatible_version(v, profile.value_abi()))
        || json_imports
            .iter()
            .any(|op| *op != profile.json_parse() && *op != profile.json_stringify())
        || profile.binary64() && (http || tools);
    if mismatch {
        let message = match profile {
            varta_common::language::LanguageProfile::CheckedF64Utf8V1 => {
                "binary64 requires value ABI v3 and cannot use legacy JSON/tool/HTTP contracts"
            }
            varta_common::language::LanguageProfile::CheckedF64Utf8V2 => {
                "undefined profile requires value ABI v4 and matching JSON imports; tool/HTTP adapters are unavailable"
            }
            _ => "structured value ABI or JSON imports do not match the language profile",
        };
        return Err(varta_common::language::LanguageError(message.into()).into());
    }
    if http && value_version != Some(varta_common::value::VALUE_VERSION) {
        return Err(error("HTTP requires structured value ABI v2"));
    }
    if (!json_imports.is_empty() || union_imports || profile.exceptions()) && values != 1 {
        return Err(error("JSON import requires structured value ABI metadata"));
    }
    let static_region = static_region.unwrap_or(abi::StaticRegion::SINGLE_PAGE);
    check_static_region(static_region, memory_initial, &data_segments)?;
    crate::frames::admit(
        bytes,
        profile,
        function_catalog.as_ref().map(|c| c.as_catalog()),
    )?;
    let function_catalog = function_catalog
        .map(|catalog| std::sync::Arc::new(catalog.with_static_region(static_region)));
    Ok((language, function_catalog, static_region))
}

/// A `program_limits` (v38) data segment: active, in memory 0, at a constant
/// i32 offset. Earlier profiles keep their unchecked data sections.
fn active_segment(segment: wasmparser::Data<'_>) -> Result<(usize, usize), RuntimeError> {
    let wasmparser::DataKind::Active {
        memory_index: 0,
        offset_expr,
    } = segment.kind
    else {
        return Err(error("static data must be active in memory 0"));
    };
    let mut operators = offset_expr.get_operators_reader();
    match (operators.read(), operators.read()) {
        (Ok(wasmparser::Operator::I32Const { value }), Ok(wasmparser::Operator::End))
            if operators.eof() =>
        {
            Ok((value as u32 as usize, segment.data.len()))
        }
        _ => Err(error("static data offset must be an i32 constant")),
    }
}

/// The static region covers every data segment of a `program_limits` module
/// (for earlier profiles the list is empty), and a declared extension lies
/// within the module's initial memory, so the heap starts after it.
fn check_static_region(
    region: abi::StaticRegion,
    memory_initial: Option<u64>,
    data_segments: &[(usize, usize)],
) -> Result<(), RuntimeError> {
    if data_segments
        .iter()
        .any(|(offset, len)| !region.contains(*offset, *len))
    {
        return Err(error("data segment outside the static region"));
    }
    if region.extension_pages() > 0
        && memory_initial.is_none_or(|initial| initial < region.minimum_pages())
    {
        return Err(error("static region exceeds the initial memory"));
    }
    Ok(())
}

fn check_exports(module: &Module) -> Result<(), RuntimeError> {
    use wasmtime::{ExternType, Mutability, ValType};
    let mut needs_memory = false;
    let mut needs_result = false;
    for import in module.imports() {
        needs_memory |= !matches!(
            import.name(),
            "log_i32"
                | "log_f64"
                | "log_f64_v2"
                | "log_undefined"
                | "number_remainder"
                | "process_exit_f64"
                | "process_exit"
                | "time_now"
        );
        needs_result |= varta_common::HostOperation::from_name(import.name())
            .is_some_and(|host| host.needs_result_globals());
    }
    if needs_memory {
        match module.get_export("memory") {
            Some(ExternType::Memory(memory)) if !memory.is_64() && !memory.is_shared() => {}
            _ => {
                return Err(error(
                    "host pointer imports require an exported 32-bit memory",
                ));
            }
        }
    }
    for name in [abi::RESULT_LEN, abi::RESULT_STATUS] {
        match module.get_export(name) {
            Some(ExternType::Global(global))
                if matches!(global.content(), ValType::I32)
                    && global.mutability() == Mutability::Var => {}
            None if !needs_result => {}
            _ => {
                return Err(error(
                    "ABI result globals must be exported mutable i32 values",
                ));
            }
        }
    }
    Ok(())
}
