mod array_mutation;
mod arrays;
mod bitwise;
mod closures;
mod collect;
mod errors;
mod exceptions;
mod expr;
mod finally;
mod functions;
mod labels;
mod math;
mod missing;
mod numbers;
mod object_enumeration;
mod patterns;
mod records;
mod repeat;
mod roots;
mod schemas;
mod strings;
mod unions;
use crate::{
    CompilerError,
    ir::{self, Expr, Expression as E, Host, Statement as S, Type},
};
use std::collections::{BTreeMap, BTreeSet};
use swc_ecma_ast::BinaryOp;
use wasm_encoder::{BlockType, Instruction as I, ValType};

type Result<T> = std::result::Result<T, CompilerError>;
#[derive(Clone, Copy)]
struct Label {
    breakable: bool,
    continuable: bool,
    scope_depth: usize,
}
struct Emitter<'a> {
    binary64: bool,
    profile: varta_common::language::LanguageProfile,
    records: &'a [ir::RecordShape],
    arrays: &'a [Type],
    layouts: &'a BTreeMap<records::LayoutKey, u32>,
    schemas: &'a BTreeMap<Type, (u32, u32)>,
    code: Vec<I<'static>>,
    labels: Vec<Label>,
    named_breaks: BTreeMap<u32, usize>,
    named_continues: BTreeMap<u32, usize>,
    pending_labels: Vec<u32>,
    hosts: &'a BTreeMap<Host, u32>,
    strings: &'a BTreeMap<Vec<u8>, u32>,
    functions_begin: u32,
    local_slots: Vec<u32>,
    temporary: u32,
    string_temporary: u32,
    float_temporary: u32,
    helpers: &'a BTreeMap<strings::Helper, u32>,
    result_len: u32,
    result_status: u32,
    memory: Option<strings::Globals>,
    root_slots: Vec<Option<u32>>,
    frame_base: u32,
    frame_end: u32,
    root_mark_base: u32,
    expr_depth: u32,
    temporary_floor: Option<u32>,
    max_expr_depth: u32,
    scopes: Vec<Vec<u32>>,
    result: Type,
    exceptions: Option<exceptions::Globals>,
    handlers: Vec<exceptions::Handler>,
    exception_jumps: BTreeMap<usize, usize>,
    finalizers: Vec<finally::Finalizer>,
    canonical_functions: &'a BTreeMap<usize, u32>,
    function_targets: &'a BTreeMap<u32, Vec<usize>>,
    function_types: &'a [ir::FunctionType],
    reserved_roots: usize,
    cell_slots: BTreeMap<u32, u32>,
    cell_types: BTreeMap<u32, Type>,
    binding_types: &'a BTreeMap<Type, u32>,
    environment_functions: &'a BTreeSet<usize>,
    self_functions: &'a BTreeSet<usize>,
}

pub(crate) fn emit(program: &ir::Program) -> Result<Vec<u8>> {
    let collected = collect::Collected::gather(program);
    let reserved_roots = if program.types.profile.exceptions() {
        varta_common::exceptions::RESERVED_ROOT_BYTES
    } else {
        0
    } + collected.canonical_functions.len()
        * varta_common::abi::STRING_ROOT_SLOT_BYTES;
    let canonical_functions: BTreeMap<_, _> = collected
        .canonical_functions
        .iter()
        .enumerate()
        .map(|(index, function)| {
            (
                *function,
                (varta_common::abi::STRING_ROOT_BASE
                    + varta_common::exceptions::RESERVED_ROOT_BYTES
                    + index * varta_common::abi::STRING_ROOT_SLOT_BYTES) as u32,
            )
        })
        .collect();
    let function_targets: BTreeMap<_, _> = program
        .types
        .functions
        .iter()
        .enumerate()
        .map(|(id, signature)| {
            (
                id as u32,
                program
                    .functions
                    .iter()
                    .enumerate()
                    .filter(|(index, function)| {
                        *index != program.main
                            && function.receiver == signature.receiver
                            && function.shape == signature.shape
                            && function.call_params() == signature.physical_params()
                            && function.result == signature.result
                    })
                    .map(|(index, _)| index)
                    .collect(),
            )
        })
        .collect();
    let binding_types: BTreeMap<_, _> = program
        .functions
        .iter()
        .flat_map(|function| {
            function
                .cells
                .iter()
                .map(|slot| function.locals[*slot as usize])
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .enumerate()
        .map(|(id, ty)| (ty, id as u32))
        .collect();
    let self_functions: BTreeSet<_> = program
        .functions
        .iter()
        .enumerate()
        .filter(|(_, f)| f.self_slot.is_some())
        .map(|(index, _)| index)
        .collect();
    let environment_functions: BTreeSet<_> = program
        .functions
        .iter()
        .enumerate()
        .filter(|(_, f)| f.environment)
        .map(|(index, _)| index)
        .collect();
    let mut hosts = collected.hosts;
    if program.http.is_some() {
        hosts.insert(Host::HttpService);
    }
    let strings = collected.strings;
    let mut helpers = collected.helpers;
    if program.types.profile.exceptions() {
        helpers.insert(strings::Helper::Allocate);
    }
    let layouts = collected.layouts;
    let schemas = collected.schemas;
    let has_parser = hosts.contains(&program.types.profile.json_parse());
    let trace_values = !layouts.is_empty()
        || collected.arrays
        || collected.unions
        || program.types.profile.exceptions();
    let tool_strings = program.tools.iter().any(|(_, tool)| tool.uses_strings());
    if tool_strings || program.http.is_some() {
        helpers.insert(strings::Helper::Snapshot);
    }
    if program.types.profile.sparse_arrays() {
        helpers.extend([
            strings::Helper::ArrayPresent,
            strings::Helper::ArraySetPresent,
        ]);
    }
    if program.types.profile.array_mutations() {
        helpers.insert(strings::Helper::ArrayResize);
    }
    let needs_arena = helpers.iter().any(|helper| helper.needs_arena());
    if needs_arena {
        helpers.extend([
            strings::Helper::Allocate,
            strings::Helper::Collect,
            strings::Helper::RootPush,
        ]);
    }
    let mut offsets = BTreeMap::new();
    let mut offset = 0usize;
    for (string, (span, source)) in strings {
        offsets.insert(string.clone(), offset as u32);
        offset = offset.checked_add(string.len()).ok_or_else(|| {
            source.locate(CompilerError::Invalid("static data overflow".into()), span)
        })?;
        if offset > varta_common::abi::RESULT_BASE {
            return Err(source.locate(
                CompilerError::Invalid("static string data exceeds one Wasm page".into()),
                span,
            ));
        }
    }
    let mut layout_offsets = BTreeMap::new();
    let mut layout_data = Vec::new();
    for (key, (span, source)) in layouts {
        let bytes = records::descriptor(
            &key,
            &program.types.records,
            &offsets,
            program.types.value_version(),
        )?;
        let start = offset;
        offset += bytes.len();
        if offset > varta_common::abi::RESULT_BASE {
            return Err(source.locate(
                CompilerError::Invalid("object metadata exceeds static memory budget".into()),
                span,
            ));
        }
        layout_offsets.insert(key, start as u32);
        layout_data.push((start as u32, bytes));
    }
    let mut schema_offsets = BTreeMap::new();
    for (ty, (span, source)) in schemas {
        let bytes = schemas::build(ty, &program.types, &layout_offsets)
            .map_err(|e| source.locate(e, span))?;
        let start = offset;
        offset += bytes.len();
        if offset > varta_common::abi::RESULT_BASE {
            return Err(source.locate(
                CompilerError::Invalid("JSON schema exceeds static memory budget".into()),
                span,
            ));
        }
        schema_offsets.insert(ty, (start as u32, bytes.len() as u32));
        layout_data.push((start as u32, bytes));
    }
    let hosts: BTreeMap<_, _> = hosts
        .into_iter()
        .enumerate()
        .map(|(i, h)| (h, i as u32))
        .collect();
    let mut types = wasm_encoder::TypeSection::new();
    let mut signatures = BTreeMap::new();
    let mut intern = |key: (Vec<ValType>, Vec<ValType>)| {
        *signatures.entry(key.clone()).or_insert_with(|| {
            let index = types.len();
            types.ty().function(key.0, key.1);
            index
        })
    };
    let host_types: BTreeMap<_, _> = hosts
        .keys()
        .map(|h| {
            let (params, results) = h.scalar_signature();
            (
                *h,
                intern((
                    params.iter().copied().map(host_type).collect(),
                    results.iter().copied().map(host_type).collect(),
                )),
            )
        })
        .collect();
    let function_types: Vec<_> = program
        .functions
        .iter()
        .map(|f| {
            intern((
                f.params
                    .iter()
                    .copied()
                    .flat_map(wasm_types)
                    .chain(f.environment.then_some(ValType::I64))
                    .collect(),
                wasm_types(f.result),
            ))
        })
        .collect();
    let helper_types: BTreeMap<_, _> = helpers
        .iter()
        .map(|helper| (*helper, intern(helper.signature())))
        .collect();
    if types.len() as usize > varta_common::abi::MAX_WASM_SIGNATURES {
        return Err(CompilerError::Invalid(
            "Wasm signature complexity exceeds limits".into(),
        ));
    }
    let mut module = wasm_encoder::Module::new();
    module.section(&types);
    let mut imports = wasm_encoder::ImportSection::new();
    for host in hosts.keys() {
        imports.import(
            "varta",
            host.name(),
            wasm_encoder::EntityType::Function(host_types[host]),
        );
    }
    module.section(&imports);
    let mut functions = wasm_encoder::FunctionSection::new();
    for index in function_types {
        functions.function(index);
    }
    for index in helper_types.values() {
        functions.function(*index);
    }
    module.section(&functions);
    let mut memory = wasm_encoder::MemorySection::new();
    memory.memory(wasm_encoder::MemoryType {
        minimum: if needs_arena {
            (varta_common::abi::OWNED_STRING_BASE / varta_common::abi::WASM_PAGE_BYTES) as u64
        } else {
            1
        },
        maximum: Some(
            (varta_common::abi::MAX_MEMORY_BYTES / varta_common::abi::RESULT_BASE) as u64,
        ),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memory);
    let mut globals = wasm_encoder::GlobalSection::new();
    let result_len = globals.len();
    globals.global(
        wasm_encoder::GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &wasm_encoder::ConstExpr::i32_const(0),
    );
    let result_status = globals.len();
    globals.global(
        wasm_encoder::GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &wasm_encoder::ConstExpr::i32_const(0),
    );
    let memory_globals = strings::Globals {
        heap: globals.len(),
        free: globals.len() + 1,
        roots: globals.len() + 2,
    };
    if needs_arena {
        for initial in [
            varta_common::abi::OWNED_STRING_BASE,
            0,
            varta_common::abi::STRING_ROOT_BASE + reserved_roots,
        ] {
            globals.global(
                wasm_encoder::GlobalType {
                    val_type: ValType::I32,
                    mutable: true,
                    shared: false,
                },
                &wasm_encoder::ConstExpr::i32_const(initial as i32),
            );
        }
    }
    let exception_globals = if program.types.profile.exceptions() {
        let pending = globals.len();
        globals.global(
            wasm_encoder::GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &wasm_encoder::ConstExpr::i32_const(0),
        );
        let value = globals.len();
        globals.global(
            wasm_encoder::GlobalType {
                val_type: ValType::I64,
                mutable: true,
                shared: false,
            },
            &wasm_encoder::ConstExpr::i64_const(0),
        );
        Some(exceptions::Globals { pending, value })
    } else {
        None
    };
    let error_channel_version = if program.types.profile.errors() {
        let index = globals.len();
        globals.global(
            wasm_encoder::GlobalType {
                val_type: ValType::I32,
                mutable: false,
                shared: false,
            },
            &wasm_encoder::ConstExpr::i32_const(if program.types.profile.error_families() {
                varta_common::exceptions::FAMILY_CHANNEL_VERSION
            } else {
                varta_common::exceptions::ERROR_CHANNEL_VERSION
            }),
        );
        Some(index)
    } else {
        None
    };
    module.section(&globals);
    let functions_begin = hosts.len() as u32;
    let helpers: BTreeMap<_, _> = helpers
        .into_iter()
        .enumerate()
        .map(|(index, helper)| {
            (
                helper,
                functions_begin + program.functions.len() as u32 + index as u32,
            )
        })
        .collect();
    let mut exports = wasm_encoder::ExportSection::new();
    if let Some(index) = error_channel_version {
        exports.export(
            if program.types.profile.error_families() {
                varta_common::exceptions::FAMILY_VERSION
            } else {
                varta_common::exceptions::VERSION
            },
            wasm_encoder::ExportKind::Global,
            index,
        );
    }
    exports.export("memory", wasm_encoder::ExportKind::Memory, 0);
    if let Some(globals) = exception_globals {
        exports.export(
            varta_common::exceptions::PENDING,
            wasm_encoder::ExportKind::Global,
            globals.pending,
        );
        exports.export(
            varta_common::exceptions::VALUE,
            wasm_encoder::ExportKind::Global,
            globals.value,
        );
    }
    exports.export(
        varta_common::abi::RESULT_LEN,
        wasm_encoder::ExportKind::Global,
        result_len,
    );
    exports.export(
        varta_common::abi::RESULT_STATUS,
        wasm_encoder::ExportKind::Global,
        result_status,
    );
    exports.export(
        "_start",
        wasm_encoder::ExportKind::Func,
        functions_begin + program.main as u32,
    );
    for (index, tool) in &program.tools {
        exports.export(
            &tool.export,
            wasm_encoder::ExportKind::Func,
            functions_begin + *index as u32,
        );
    }
    if let Some((index, contract)) = &program.http {
        exports.export(
            &contract.export,
            wasm_encoder::ExportKind::Func,
            functions_begin + *index as u32,
        );
        for (name, helper) in [
            (varta_common::http::COPY, strings::Helper::Snapshot),
            (varta_common::http::RETAIN, strings::Helper::RootPush),
        ] {
            exports.export(name, wasm_encoder::ExportKind::Func, helpers[&helper]);
        }
    }
    if tool_strings {
        for (name, helper) in [
            (varta_common::TOOL_COPY_STRING, strings::Helper::Snapshot),
            (varta_common::TOOL_RETAIN_STRING, strings::Helper::RootPush),
        ] {
            exports.export(name, wasm_encoder::ExportKind::Func, helpers[&helper]);
        }
    }
    if has_parser {
        for (name, helper) in [
            (
                varta_common::value::VALUE_ALLOCATE,
                strings::Helper::Allocate,
            ),
            (varta_common::value::VALUE_RETAIN, strings::Helper::RootPush),
        ] {
            exports.export(name, wasm_encoder::ExportKind::Func, helpers[&helper]);
        }
    }
    module.section(&exports);
    let mut code = wasm_encoder::CodeSection::new();
    for (index, function) in program.functions.iter().enumerate() {
        let mut temporary = 0;
        let environment_slot = function.environment.then(|| {
            function
                .params
                .iter()
                .copied()
                .map(Type::width)
                .sum::<u32>()
        });
        let local_slots = function
            .locals
            .iter()
            .enumerate()
            .map(|(index, ty)| {
                if index == function.params.len() && function.environment {
                    temporary += 1;
                }
                let slot = temporary;
                temporary += ty.width();
                slot
            })
            .collect();
        if function.locals.len() == function.params.len() && function.environment {
            temporary += 1;
        }
        let cell_slots = function
            .cells
            .iter()
            .map(|slot| {
                let physical = temporary;
                temporary += 1;
                (*slot, physical)
            })
            .collect();
        let mut emitter = Emitter {
            binary64: program.types.profile.binary64(),
            profile: program.types.profile,
            records: &program.types.records,
            arrays: &program.types.arrays,
            layouts: &layout_offsets,
            schemas: &schema_offsets,
            code: Vec::new(),
            labels: Vec::new(),
            named_breaks: BTreeMap::new(),
            named_continues: BTreeMap::new(),
            pending_labels: Vec::new(),
            hosts: &hosts,
            strings: &offsets,
            functions_begin,
            local_slots,
            temporary,
            string_temporary: temporary + 1,
            float_temporary: temporary + 2,
            helpers: &helpers,
            result_len,
            result_status,
            memory: needs_arena.then_some(memory_globals),
            root_slots: roots::slots(&function.locals, &function.cells),
            frame_base: temporary + 3,
            frame_end: temporary + 4,
            root_mark_base: temporary + 5,
            expr_depth: 0,
            temporary_floor: None,
            max_expr_depth: 0,
            scopes: Vec::new(),
            result: function.result,
            exceptions: exception_globals,
            handlers: Vec::new(),
            exception_jumps: BTreeMap::new(),
            finalizers: Vec::new(),
            canonical_functions: &canonical_functions,
            function_targets: &function_targets,
            function_types: &program.types.functions,
            reserved_roots,
            cell_slots,
            cell_types: function
                .cells
                .iter()
                .map(|slot| (*slot, function.locals[*slot as usize]))
                .collect(),
            binding_types: &binding_types,
            environment_functions: &environment_functions,
            self_functions: &self_functions,
        };
        emitter.begin_frame(function)?;
        emitter.begin_closure(function, environment_slot, index)?;
        emitter
            .statements(&function.body)
            .map_err(|e| function.source.locate(e, function.span))?;
        emitter.end_frame();
        emitter.resolve_exception_jumps()?;
        let locals = function.locals[function.params.len()..]
            .iter()
            .copied()
            .flat_map(wasm_types)
            .chain(std::iter::repeat_n(ValType::I64, function.cells.len()))
            .chain([ValType::I32, ValType::I64, ValType::F64])
            .chain(std::iter::repeat_n(
                ValType::I32,
                if needs_arena {
                    2 + emitter.max_expr_depth as usize
                } else {
                    0
                },
            ));
        let mut body = wasm_encoder::Function::new_with_locals_types(locals);
        for instruction in &emitter.code {
            body.instruction(instruction);
        }
        if function.result != Type::Void {
            body.instruction(&I::Unreachable);
        }
        body.instruction(&I::End);
        if body.byte_len() > varta_common::abi::MAX_WASM_FUNCTION_BODY_BYTES {
            return Err(function.source.locate(
                CompilerError::Invalid("function body exceeds compilation budget".into()),
                function.span,
            ));
        }
        code.function(&body);
    }
    for helper in helpers.keys() {
        code.function(&helper.body(
            memory_globals,
            &helpers,
            trace_values,
            program.types.profile,
        ));
    }
    module.section(&code);
    let mut data = wasm_encoder::DataSection::new();
    for (string, offset) in offsets {
        data.active(
            0,
            &wasm_encoder::ConstExpr::i32_const(offset as i32),
            string,
        );
    }
    for (offset, bytes) in layout_data {
        data.active(0, &wasm_encoder::ConstExpr::i32_const(offset as i32), bytes);
    }
    module.section(&data);
    module.section(&wasm_encoder::CustomSection {
        name: varta_common::abi::SECTION.into(),
        data: program
            .types
            .profile
            .host_abi()
            .to_le_bytes()
            .to_vec()
            .into(),
    });
    module.section(&wasm_encoder::CustomSection {
        name: varta_common::language::SECTION.into(),
        data: varta_common::language::LanguageContract {
            schema_version: varta_common::language::SCHEMA_VERSION,
            profile: program.types.profile,
        }
        .to_bytes()
        .map_err(|e| CompilerError::Invalid(e.to_string()))?
        .into(),
    });
    if !program.tools.is_empty() {
        let catalog = varta_common::ToolCatalog::new(
            program.tools.iter().map(|(_, tool)| tool.clone()).collect(),
        );
        module.section(&wasm_encoder::CustomSection {
            name: varta_common::TOOL_SECTION.into(),
            data: catalog
                .to_bytes()
                .map_err(|e| CompilerError::Invalid(e.to_string()))?
                .into(),
        });
    }
    if collected.unions
        || trace_values
        || has_parser
        || hosts.contains_key(&Host::JsonStringify)
        || hosts.contains_key(&Host::JsonStringifyV3)
        || hosts.contains_key(&Host::JsonStringifyV4)
        || hosts.contains_key(&Host::JsonStringifyV5)
        || hosts.contains_key(&Host::JsonStringifyV6)
        || hosts.contains_key(&Host::JsonStringifyV7)
        || hosts.contains_key(&Host::JsonStringifyV8)
        || hosts.contains_key(&Host::JsonStringifyV9)
        || hosts.contains_key(&Host::JsonStringifyV10)
        || hosts.contains_key(&Host::JsonStringifyV11)
    {
        module.section(&wasm_encoder::CustomSection {
            name: varta_common::value::VALUE_SECTION.into(),
            data: program.types.value_version().to_le_bytes().to_vec().into(),
        });
    }
    if let Some((_, contract)) = &program.http {
        module.section(&wasm_encoder::CustomSection {
            name: varta_common::http::SECTION.into(),
            data: contract
                .to_bytes()
                .map_err(|e| CompilerError::Invalid(e.to_string()))?
                .into(),
        });
    }
    Ok(module.finish())
}

fn wasm_types(ty: Type) -> Vec<ValType> {
    if let Some(value) = ty.result_value() {
        vec![
            if value == Type::Float64 {
                ValType::F64
            } else if value == Type::String {
                ValType::I64
            } else {
                ValType::I32
            },
            ValType::I32,
        ]
    } else {
        match ty {
            Type::Float64 => vec![ValType::F64],
            Type::Function(_)
            | Type::Error
            | Type::NativeError(_)
            | Type::String
            | Type::Record(_)
            | Type::Array(_)
            | Type::Union(_)
            | Type::ReferenceUnion(_) => vec![ValType::I64],
            Type::Void => vec![],
            _ => vec![ValType::I32],
        }
    }
}

fn host_type(ty: varta_common::HostScalar) -> ValType {
    match ty {
        varta_common::HostScalar::I32 => ValType::I32,
        varta_common::HostScalar::I64 => ValType::I64,
        varta_common::HostScalar::F64 => ValType::F64,
    }
}

impl Emitter<'_> {
    fn push_label(&mut self, instruction: I<'static>, breakable: bool, continuable: bool) -> usize {
        self.code.push(instruction);
        let index = self.labels.len();
        self.labels.push(Label {
            breakable,
            continuable,
            scope_depth: self.scopes.len(),
        });
        index
    }
    fn end(&mut self) {
        self.code.push(I::End);
        self.labels.pop();
    }
    fn depth(&self, index: usize) -> u32 {
        (self.labels.len() - 1 - index) as u32
    }
    fn target(&self, continuing: bool) -> Result<u32> {
        self.labels
            .iter()
            .rposition(|l| {
                if continuing {
                    l.continuable
                } else {
                    l.breakable
                }
            })
            .map(|index| self.depth(index))
            .ok_or_else(|| CompilerError::Internal("missing branch target".into()))
    }
    fn statements(&mut self, statements: &[S]) -> Result<()> {
        for statement in statements {
            match statement {
                S::Trap => self.code.push(I::Unreachable),
                S::RecordEnumerate {
                    source,
                    target,
                    shape,
                    slots,
                    values,
                } => {
                    self.record_enumerate(*source, *target, *shape, *slots, values)?;
                    self.end_temporaries();
                }
                S::RecordRest {
                    source,
                    target,
                    from,
                    to,
                    mapping,
                    index,
                    cursor,
                } => self.record_rest((*source, *target), (*from, *to), mapping, (*index, *cursor)),
                S::Throw(value) => self.throw_value(value)?,
                S::TryCatch {
                    body,
                    binding,
                    handler,
                    marker,
                } => self.try_catch(body, *binding, handler, *marker)?,
                S::TryFinally {
                    body,
                    finalizer,
                    marker,
                    completion,
                    exception,
                    result,
                } => {
                    self.try_finally(
                        body,
                        finalizer,
                        finally::Slots {
                            marker: *marker,
                            completion: *completion,
                            exception: *exception,
                            result: *result,
                        },
                    )?;
                }
                S::ArrayLength {
                    array,
                    length,
                    error,
                } => {
                    self.array_length(array, length, error)?;
                    self.end_temporaries();
                }
                S::SetIndex {
                    array,
                    index,
                    value,
                    slots,
                } => {
                    if self.profile.array_mutations() {
                        self.array_set_index(array, index, value, *slots)?;
                        self.end_temporaries();
                        continue;
                    }
                    self.expr(array)?;
                    self.index(index)?;
                    self.expr(value)?;
                    self.pack_slot(value.ty);
                    self.code.push(I::LocalSet(self.string_temporary));
                    if let Some((base, index)) = slots {
                        self.scratch_set(*index, Type::Number);
                        self.scratch_set(*base, array.ty);
                        self.scratch_get(*base, array.ty);
                        self.scratch_get(*index, Type::Number);
                    }
                    self.string_helper(strings::Helper::ArrayIndex)?;
                    self.code.extend([
                        I::LocalGet(self.string_temporary),
                        I::I64Store(strings::ROOT),
                    ]);
                    if let Some((base, index)) = slots {
                        self.scratch_get(*base, array.ty);
                        self.scratch_get(*index, Type::Number);
                        self.code.push(I::I32Const(1));
                        self.string_helper(strings::Helper::ArraySetPresent)?;
                    }
                    self.end_temporaries();
                }
                S::SetField(object, index, value) => {
                    self.expr(object)?;
                    self.expr(value)?;
                    self.pack_slot(value.ty);
                    self.code.extend([
                        I::LocalSet(self.string_temporary),
                        I::I32WrapI64,
                        I::LocalTee(self.temporary),
                        I::LocalGet(self.string_temporary),
                        I::I64Store(records::field(*index)),
                    ]);
                    if self.profile.missing() {
                        self.mark_present(object.ty, *index);
                    }
                    self.end_temporaries();
                }
                S::Init(slot, value) | S::Set(slot, value) => {
                    self.expr(value)?;
                    self.write_binding(*slot, value.ty, matches!(statement, S::Init(..)))?;
                    self.end_temporaries();
                }
                S::Rebind(slots) => {
                    for slot in slots {
                        if let Some(ty) = self.cell_types.get(slot).copied() {
                            self.read_binding(*slot, ty);
                            if ty.needs_root() {
                                if ty == Type::ResultString {
                                    self.code.push(I::LocalSet(self.temporary));
                                }
                                self.string_helper(strings::Helper::RootPush)?;
                                if ty == Type::ResultString {
                                    self.code.push(I::LocalGet(self.temporary));
                                }
                            }
                            self.write_binding(*slot, ty, true)?;
                            self.end_temporaries();
                        }
                    }
                }
                S::Expr(value) => {
                    self.expr(value)?;
                    for _ in 0..value.ty.width() {
                        self.code.push(I::Drop);
                    }
                    self.end_temporaries();
                }
                S::Return(value) => {
                    if let Some(value) = value {
                        self.expr(value)?;
                    }
                    self.complete_return()?;
                }
                S::Block(body) => self.statements(body)?,
                S::Scope(slots, body) => {
                    self.scopes.push(slots.clone());
                    self.statements(body)?;
                    self.clear_scopes(self.scopes.len() - 1);
                    self.scopes.pop();
                }
                S::If(test, yes, no) => {
                    self.expr(test)?;
                    self.truthy(test.ty);
                    self.push_label(I::If(BlockType::Empty), false, false);
                    self.statements(yes)?;
                    if !no.is_empty() {
                        self.code.push(I::Else);
                        self.statements(no)?;
                    }
                    self.end();
                }
                S::DoWhile(test, body) => {
                    let names = std::mem::take(&mut self.pending_labels);
                    self.push_label(I::Block(BlockType::Empty), true, false);
                    let again = self.push_label(I::Loop(BlockType::Empty), false, false);
                    // Continue leaves the body block and reaches the condition;
                    // break leaves the outer block without evaluating it.
                    let continuing = self.push_label(I::Block(BlockType::Empty), false, true);
                    self.bind_continues(&names, continuing);
                    self.statements(body)?;
                    self.end();
                    self.expr(test)?;
                    self.truthy(test.ty);
                    self.end_temporaries();
                    self.code.push(I::BrIf(self.depth(again)));
                    self.end();
                    self.end();
                    self.unbind_continues(&names);
                }
                S::While(test, body) => {
                    let names = std::mem::take(&mut self.pending_labels);
                    self.push_label(I::Block(BlockType::Empty), true, false);
                    let again = self.push_label(I::Loop(BlockType::Empty), false, true);
                    self.bind_continues(&names, again);
                    self.expr(test)?;
                    self.truthy(test.ty);
                    self.code.push(I::I32Eqz);
                    self.code.push(I::BrIf(self.target(false)?));
                    self.statements(body)?;
                    self.code.push(I::Br(self.depth(again)));
                    self.end();
                    self.end();
                    self.unbind_continues(&names);
                }
                S::For(init, test, update, body) => {
                    let names = std::mem::take(&mut self.pending_labels);
                    self.statements(init)?;
                    self.push_label(I::Block(BlockType::Empty), true, false);
                    let again = self.push_label(I::Loop(BlockType::Empty), false, false);
                    if let Some(test) = test {
                        self.expr(test)?;
                        self.truthy(test.ty);
                        self.code.push(I::I32Eqz);
                        self.code.push(I::BrIf(self.target(false)?));
                    }
                    let continuing = self.push_label(I::Block(BlockType::Empty), false, true);
                    self.bind_continues(&names, continuing);
                    self.statements(body)?;
                    self.end();
                    self.statements(update)?;
                    self.code.push(I::Br(self.depth(again)));
                    self.end();
                    self.end();
                    self.unbind_continues(&names);
                }
                S::Switch {
                    value,
                    discriminant,
                    selected,
                    cases,
                } => {
                    self.expr(value)?;
                    self.code
                        .push(I::LocalSet(self.local_slots[*discriminant as usize]));
                    self.code.push(I::I32Const(-1));
                    self.code
                        .push(I::LocalSet(self.local_slots[*selected as usize]));
                    let mut default = cases.len();
                    for (index, (test, _)) in cases.iter().enumerate() {
                        let Some(test) = test else {
                            default = index;
                            continue;
                        };
                        self.code
                            .push(I::LocalGet(self.local_slots[*selected as usize]));
                        self.code.push(I::I32Const(-1));
                        self.code.push(I::I32Eq);
                        self.code.push(I::If(BlockType::Empty));
                        self.code
                            .push(I::LocalGet(self.local_slots[*discriminant as usize]));
                        self.expr(test)?;
                        self.code.push(if test.ty == Type::Float64 {
                            I::F64Eq
                        } else {
                            I::I32Eq
                        });
                        self.code.push(I::If(BlockType::Empty));
                        self.code.push(I::I32Const(index as i32));
                        self.code
                            .push(I::LocalSet(self.local_slots[*selected as usize]));
                        self.code.push(I::End);
                        self.code.push(I::End);
                    }
                    self.code
                        .push(I::LocalGet(self.local_slots[*selected as usize]));
                    self.code.push(I::I32Const(-1));
                    self.code.push(I::I32Eq);
                    self.code.push(I::If(BlockType::Empty));
                    self.code.push(I::I32Const(default as i32));
                    self.code
                        .push(I::LocalSet(self.local_slots[*selected as usize]));
                    self.code.push(I::End);
                    self.push_label(I::Block(BlockType::Empty), true, false);
                    for (index, (_, body)) in cases.iter().enumerate() {
                        self.code
                            .push(I::LocalGet(self.local_slots[*selected as usize]));
                        self.code.push(I::I32Const(index as i32));
                        self.code.push(I::I32LeS);
                        self.push_label(I::If(BlockType::Empty), false, false);
                        self.statements(body)?;
                        self.end();
                    }
                    self.end();
                }
                S::Label {
                    id,
                    iteration,
                    body,
                } => self.named_label(*id, *iteration, body)?,
                S::BreakLabel(id) => self.named_branch(*id, false)?,
                S::ContinueLabel(id) => self.named_branch(*id, true)?,
                S::Break => self.branch(false)?,
                S::Continue => self.branch(true)?,
            }
        }
        Ok(())
    }
}
