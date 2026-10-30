mod array_mutation;
mod arrays;
mod bitwise;
mod classes;
mod closures;
mod collect;
mod errors;
mod exceptions;
mod expr;
mod finally;
mod frame_slots;
mod frames;
mod function_catalog;
mod functions;
mod labels;
mod math;
mod math_f16;
mod missing;
mod number_predicates;
mod numbers;
mod object_enumeration;
mod patterns;
mod primitive_builtins;
mod records;
mod repeat;
mod roots;
mod schemas;
mod shared_members;
mod static_data;
mod strings;
mod unions;
use crate::{
    CompilerError,
    ir::{self, Expr, Expression as E, Host, Statement as S, Type},
};
use shared_members::{SharedMembers, shared_members};
use std::collections::{BTreeMap, BTreeSet};
use swc_ecma_ast::BinaryOp;
use wasm_encoder::{BlockType, Instruction as I, ValType};

type Result<T> = std::result::Result<T, CompilerError>;
#[cfg(any(test, feature = "suspension-test-hook"))]
pub(crate) use frames::{TEST_COLLECT, TEST_CREATE_PREFIX};
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
    dictionaries: &'a [ir::DictionaryType],
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
    /// Address of the first permanent module-constant root slot.
    constant_roots: u32,
    cell_slots: BTreeMap<u32, u32>,
    cell_types: BTreeMap<u32, Type>,
    binding_types: &'a BTreeMap<Type, u32>,
    environment_functions: &'a BTreeSet<usize>,
    self_functions: &'a BTreeSet<usize>,
    /// The Wasm local holding this function's own closure environment, which
    /// an async closure forwards to its scheduled body (v35).
    environment_param: Option<u32>,
    /// Present only while emitting a suspendable function's resume body.
    suspension: Option<frames::Resume<'a>>,
    /// Step function of each generator type id (v33).
    generator_steps: &'a [u32],
    /// The exported dispatcher, present when a frame layout exists.
    dispatcher: Option<u32>,
    /// Functions whose own index is a frame creator.
    frame_functions: &'a BTreeMap<usize, ir::Suspendable>,
    frame_resources: Option<frames::Resources>,
    /// `Promise<T>` types (v34) by id: value type and private record shape.
    promises: &'a [ir::PromiseType],
    /// V37 (`language_core`): unions with several record-tag or array
    /// members, and the descriptor shapes that identify each record or class
    /// member (`shared_members`).
    shared: &'a SharedMembers,
}

pub(crate) fn emit(program: &ir::Program) -> Result<Vec<u8>> {
    let limits = program.types.profile.limits();
    let collected = collect::Collected::gather(program);
    let reserved_roots = if program.types.profile.exceptions() {
        varta_common::exceptions::RESERVED_ROOT_BYTES
    } else {
        0
    } + collected.canonical_functions.len()
        * varta_common::abi::STRING_ROOT_SLOT_BYTES;
    // Stored module constants follow the canonical function roots and stay
    // rooted for the instance lifetime.
    let constant_roots = (varta_common::abi::STRING_ROOT_BASE + reserved_roots) as u32;
    let reserved_roots =
        program.constants as usize * varta_common::abi::STRING_ROOT_SLOT_BYTES + reserved_roots;
    // Live continuation frames are retained by a bounded permanent root table.
    let frame_table = (varta_common::abi::STRING_ROOT_BASE + reserved_roots) as u32;
    let reserved_roots = reserved_roots
        + if program.suspendable.is_empty() {
            0
        } else {
            varta_common::function::frame::MAX_LIVE_FRAMES
                * varta_common::abi::STRING_ROOT_SLOT_BYTES
        };
    if reserved_roots > varta_common::abi::STRING_ROOT_BYTES {
        return Err(CompilerError::Invalid(
            "module constants exceed root stack capacity".into(),
        ));
    }
    let shared = shared_members(&program.types);
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
                            && !program.initializers.contains(index)
                            && !program.suspendable.contains_key(index)
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
    let frame_layouts: Vec<_> = program
        .suspendable
        .iter()
        .enumerate()
        .map(|(id, (&index, machine))| {
            frames::Layout::new(id as u32, index, &program.functions[index], machine)
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
        .chain(frame_layouts.iter().flat_map(|layout| {
            layout.binding_types(
                &program.suspendable[&layout.function],
                program.functions[layout.function].result,
            )
        }))
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
    if program.types.profile.exceptions() || program.constants > 0 {
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
    let mut placement = static_data::Placement::new(program.types.profile);
    let mut offsets = BTreeMap::new();
    for (string, (span, source)) in strings {
        let at = placement.push(&string).ok_or_else(|| {
            source.locate(
                CompilerError::Invalid("static string data exceeds one Wasm page".into()),
                span,
            )
        })?;
        offsets.insert(string.clone(), at);
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
        let start = placement.push(&bytes).ok_or_else(|| {
            source.locate(
                CompilerError::Invalid("object metadata exceeds static memory budget".into()),
                span,
            )
        })?;
        layout_offsets.insert(key, start);
        layout_data.push((start, bytes));
    }
    let mut schema_offsets = BTreeMap::new();
    for (ty, (span, source)) in schemas {
        let bytes = schemas::build(ty, &program.types, &layout_offsets)
            .map_err(|e| source.locate(e, span))?;
        let start = placement.push(&bytes).ok_or_else(|| {
            source.locate(
                CompilerError::Invalid("JSON schema exceeds static memory budget".into()),
                span,
            )
        })?;
        schema_offsets.insert(ty, (start, bytes.len() as u32));
        layout_data.push((start, bytes));
    }
    let static_region = placement.region()?;
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
        .enumerate()
        .map(|(index, f)| {
            intern((
                f.params
                    .iter()
                    .copied()
                    .flat_map(wasm_types)
                    .chain(f.environment.then_some(ValType::I64))
                    .collect(),
                // A suspendable function's own index creates its frame.
                if program.suspendable.contains_key(&index) {
                    vec![ValType::I64]
                } else {
                    wasm_types(f.result)
                },
            ))
        })
        .collect();
    let generator_types = program.types.generators.len();
    let resume_type = (!frame_layouts.is_empty() || generator_types > 0).then(|| {
        (
            intern((vec![ValType::I64, ValType::I32, ValType::I64], vec![])),
            intern((
                vec![ValType::I64, ValType::I32, ValType::I64],
                vec![ValType::I32],
            )),
        )
    });
    let helper_types: BTreeMap<_, _> = helpers
        .iter()
        .map(|helper| (*helper, intern(helper.signature())))
        .collect();
    if types.len() as usize > limits.wasm_signatures {
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
    if let Some((body, dispatcher)) = resume_type {
        for _ in &frame_layouts {
            functions.function(body);
        }
        if !frame_layouts.is_empty() {
            functions.function(dispatcher);
        }
        // Generator step functions share the dispatcher signature.
        for _ in 0..generator_types {
            functions.function(dispatcher);
        }
    }
    // V38: the function index space (imports included) within the Wasm
    // implementation limit.
    if program.types.profile.program_limits()
        && hosts.len() + functions.len() as usize > limits.wasm_functions
    {
        return Err(CompilerError::Invalid(format!(
            "Wasm function count exceeds the limit of {}",
            limits.wasm_functions
        )));
    }
    module.section(&functions);
    let mut memory = wasm_encoder::MemorySection::new();
    memory.memory(wasm_encoder::MemoryType {
        // A static extension (v38) lies after the root stack, so its pages
        // are committed even without a managed heap.
        minimum: if needs_arena || static_region.extension_pages() > 0 {
            static_region.minimum_pages()
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
        base: static_region.heap_base() as u32,
    };
    if needs_arena {
        for initial in [
            static_region.heap_base(),
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
            &wasm_encoder::ConstExpr::i32_const(program.types.profile.exception_channel().1),
        );
        Some(index)
    } else {
        None
    };
    let frame_resources = (!frame_layouts.is_empty()).then(|| {
        let generation = globals.len();
        globals.global(
            wasm_encoder::GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &wasm_encoder::ConstExpr::i32_const(0),
        );
        frames::Resources {
            table: frame_table,
            generation,
        }
    });
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
    // Resume bodies, then the dispatcher, follow the helpers.
    let frames_begin = functions_begin + program.functions.len() as u32 + helpers.len() as u32;
    let resume_bodies: Vec<u32> = (0..frame_layouts.len() as u32)
        .map(|id| frames_begin + id)
        .collect();
    let dispatcher = frames_begin + frame_layouts.len() as u32;
    let steps_begin = dispatcher + u32::from(!frame_layouts.is_empty());
    let generator_steps: Vec<u32> = (0..generator_types as u32)
        .map(|id| steps_begin + id)
        .collect();
    let mut exports = wasm_encoder::ExportSection::new();
    if !frame_layouts.is_empty() {
        exports.export(
            varta_common::function::frame::RESUME_EXPORT,
            wasm_encoder::ExportKind::Func,
            dispatcher,
        );
        // Frame creators and the collector are exported only for tests: always
        // in compiler unit tests, and for feature builds only while the hook
        // is enabled on this thread.
        #[cfg(any(test, feature = "suspension-test-hook"))]
        if cfg!(test) || crate::lower::suspension::hook::enabled() {
            frames::test_exports(&mut exports, &frame_layouts, functions_begin, &helpers);
        }
    }
    if let Some(index) = error_channel_version {
        exports.export(
            program.types.profile.exception_channel().0,
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
    let mut resume_code = Vec::new();
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
            dictionaries: &program.types.dictionaries,
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
            constant_roots,
            cell_slots,
            cell_types: function
                .cells
                .iter()
                .map(|slot| (*slot, function.locals[*slot as usize]))
                .collect(),
            binding_types: &binding_types,
            environment_functions: &environment_functions,
            self_functions: &self_functions,
            environment_param: environment_slot,
            suspension: None,
            generator_steps: &generator_steps,
            dispatcher: (!frame_layouts.is_empty()).then_some(dispatcher),
            frame_functions: &program.suspendable,
            frame_resources,
            promises: &program.types.promises,
            shared: &shared,
        };
        let frame_layout = frame_layouts.iter().find(|layout| layout.function == index);
        if let (Some(layout), Some(resources)) = (frame_layout, frame_resources) {
            emitter.frame_creator(function, layout, resources, (environment_slot, index))?;
        } else {
            emitter.begin_frame(function)?;
            emitter.begin_closure(function, environment_slot, index)?;
            emitter
                .statements(&function.body)
                .map_err(|e| function.source.locate(e, function.span))?;
            emitter.end_frame();
        }
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
        if body.byte_len() > limits.function_body_bytes {
            return Err(function.source.locate(
                CompilerError::Invalid("function body exceeds compilation budget".into()),
                function.span,
            ));
        }
        code.function(&body);
        if let Some(layout) = frame_layout {
            let resume = frames::Resume {
                layout,
                machine: &program.suspendable[&index],
            };
            resume_code.push(frames::resume_body(emitter, function, resume, needs_arena)?);
        }
    }
    for helper in helpers.keys() {
        code.function(&helper.body(
            memory_globals,
            &helpers,
            trace_values,
            program.types.profile,
            frame_resources.is_some(),
        ));
    }
    for body in &resume_code {
        code.function(body);
    }
    if let (Some(resources), Some(exceptions)) = (frame_resources, exception_globals) {
        code.function(&frames::dispatcher(&frames::Dispatch {
            layouts: &frame_layouts,
            machines: &program.suspendable,
            bodies: &resume_bodies,
            resources,
            exceptions,
            allocate: helpers[&strings::Helper::Allocate],
            bindings: &binding_types,
        })?);
    }
    if let Some(exceptions) = exception_globals {
        for id in 0..generator_types as u32 {
            code.function(&frames::step_function(
                id,
                &frame_layouts,
                &resume_bodies,
                exceptions.pending,
                program.types.generators[id as usize].body,
            ));
        }
    } else if generator_types > 0 {
        return Err(CompilerError::Internal(
            "generators without an exception channel".into(),
        ));
    }
    module.section(&code);
    let mut data = wasm_encoder::DataSection::new();
    if program.types.profile.program_limits() {
        // One segment per region (v38): the static image, not one segment
        // per literal, so the Wasm data-segment limit never binds.
        for (offset, bytes) in placement.images() {
            data.active(
                0,
                &wasm_encoder::ConstExpr::i32_const(offset as i32),
                bytes.iter().copied(),
            );
        }
    } else {
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
    }
    module.section(&data);
    if static_region.extension_pages() > 0 {
        module.section(&wasm_encoder::CustomSection {
            name: varta_common::abi::STATIC_SECTION.into(),
            data: static_region.section_bytes().to_vec().into(),
        });
    }
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
    if !frame_layouts.is_empty()
        || program.types.profile.property_deletion() && !program.types.functions.is_empty()
    {
        let frames = frame_layouts
            .iter()
            .map(|layout| {
                layout.catalog_row(
                    &program.suspendable[&layout.function],
                    program.functions[layout.function].result,
                    &binding_types,
                )
            })
            .collect();
        module.section(&wasm_encoder::CustomSection {
            name: varta_common::function::CATALOG_SECTION.into(),
            data: function_catalog::build(program, &binding_types, &function_targets, frames)?
                .into(),
        });
    }
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
            data: program
                .types
                .profile
                .value_abi()
                .to_le_bytes()
                .to_vec()
                .into(),
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
    let bytes = module.finish();
    // V38: the whole module within the Wasm module-size limit (earlier
    // profiles keep their runtime-side size check and diagnostics).
    if program.types.profile.program_limits() && bytes.len() > limits.wasm_bytes {
        return Err(CompilerError::Invalid(format!(
            "Wasm module of {} bytes exceeds the module limit of {} bytes",
            bytes.len(),
            limits.wasm_bytes
        )));
    }
    Ok(bytes)
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
            Type::BigInt
            | Type::Function(_)
            | Type::Error
            | Type::NativeError(_)
            | Type::String
            | Type::Record(_)
            | Type::Dictionary(_)
            | Type::Class(_)
            | Type::Generator(_)
            | Type::Promise(_)
            | Type::Array(_)
            | Type::Union(_)
            | Type::ReferenceUnion(_)
            | Type::Unknown => vec![ValType::I64],
            Type::Void | Type::Never => vec![],
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
    /// First managed-heap address of this module (v38: after its static
    /// extension).
    fn heap_base(&self) -> u32 {
        self.memory
            .map_or(varta_common::abi::OWNED_STRING_BASE as u32, |globals| {
                globals.base
            })
    }
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
                S::InitModuleConstant(index, value) => {
                    self.code.push(I::I32Const(self.constant_root(*index)));
                    self.expr(value)?;
                    self.code.push(I::I64Store(strings::ROOT));
                    self.end_temporaries();
                }
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
