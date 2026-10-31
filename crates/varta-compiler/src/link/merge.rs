//! Rewrite one emitted guest module with its linked intrinsics.
//!
//! Index plan, with `K` kept imports, `G` guest-defined functions and `L`
//! linked library functions:
//!
//! | Functions | Indices |
//! | --- | --- |
//! | Kept imports, in import order | `0..K` |
//! | Guest-defined functions, unchanged order | `K..K+G` |
//! | Linked library functions, ascending library index | `K+G..K+G+L` |
//! | Synthesized `varta_link` accessors | after the library functions |
//! | Synthesized stack-pointer check | last |
//!
//! Library types are deduplicated against the guest's and the library's
//! stack pointer follows the guest's globals. The module keeps its one
//! memory: the library's memory is memory 0, its reached data segments
//! follow the guest's in the module's intrinsic region
//! ([`super::library::Placement`]), its stack pointer starts at the top of
//! the region's stack, and every write of the stack pointer goes through
//! the check, which traps below the region.
use super::library::{Closure, LINK_MODULE, Library, Placement};
use std::collections::BTreeMap;
use varta_common::{HostOperation, HostScalar, abi};
use wasm_encoder::{
    self as encoder,
    reencode::{Error as ReencodeError, Reencode},
};
use wasmparser::{
    BinaryReader, BlockType, CustomSectionReader, ExternalKind, FuncType, FunctionBody, Operator,
    Parser, Payload, TypeRef, ValType,
};

/// Guest memory 0: the only memory of a linked module.
const GUEST_MEMORY: u32 = 0;
/// The parameters of `memory_read`/`memory_write`, passed through as the
/// destination, source and length operands of `memory.copy`.
const COPY_PARAMS: u32 = 3;

pub(super) struct Import<'a> {
    pub module: &'a str,
    pub name: &'a str,
    ty: TypeRef,
}

/// The facts about an emitted guest module the merge needs up front.
pub(super) struct Guest<'a> {
    pub imports: Vec<Import<'a>>,
    types: Vec<FuncType>,
    memories: u32,
    globals: u32,
    result_len: Option<u32>,
    resource: (Option<u32>, Option<u32>),
    /// The exported exception globals (version selector, pending, value).
    exceptions: (Option<u32>, Option<u32>, Option<u32>),
    /// The module's declared layout (`varta.static`, `varta.intrinsics`).
    pub region: abi::StaticRegion,
}

impl<'a> Guest<'a> {
    pub(super) fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        let mut guest = Self {
            imports: Vec::new(),
            types: Vec::new(),
            memories: 0,
            globals: 0,
            result_len: None,
            resource: (None, None),
            exceptions: (None, None, None),
            region: abi::StaticRegion::SINGLE_PAGE,
        };
        let mut intrinsic_section = None;
        for payload in Parser::new(0).parse_all(bytes) {
            match payload.map_err(|e| e.to_string())? {
                Payload::TypeSection(reader) => {
                    for ty in reader.into_iter_err_on_gc_types() {
                        guest.types.push(ty.map_err(|e| e.to_string())?);
                    }
                }
                Payload::ImportSection(reader) => {
                    for import in reader {
                        let import = import.map_err(|e| e.to_string())?;
                        if matches!(import.ty, TypeRef::Memory(_)) {
                            guest.memories += 1;
                        }
                        if matches!(import.ty, TypeRef::Global(_)) {
                            guest.globals += 1;
                        }
                        guest.imports.push(Import {
                            module: import.module,
                            name: import.name,
                            ty: import.ty,
                        });
                    }
                }
                Payload::MemorySection(reader) => guest.memories += reader.count(),
                Payload::GlobalSection(reader) => guest.globals += reader.count(),
                Payload::ExportSection(reader) => {
                    for export in reader {
                        let export = export.map_err(|e| e.to_string())?;
                        if export.kind == ExternalKind::Global
                            && export.name == varta_common::limits::RESOURCE_FAILURE
                        {
                            guest.resource.0 = Some(export.index);
                        }
                        if export.kind == ExternalKind::Global
                            && export.name == varta_common::limits::RESOURCE_REQUEST
                        {
                            guest.resource.1 = Some(export.index);
                        }
                        if export.kind == ExternalKind::Global && export.name == abi::RESULT_LEN {
                            guest.result_len = Some(export.index);
                        }
                        if export.kind == ExternalKind::Global {
                            use varta_common::exceptions as channel;
                            match export.name {
                                channel::HASHED_DICTIONARY_VERSION => {
                                    guest.exceptions.0 = Some(export.index);
                                }
                                channel::PENDING => guest.exceptions.1 = Some(export.index),
                                channel::VALUE => guest.exceptions.2 = Some(export.index),
                                _ => {}
                            }
                        }
                    }
                }
                Payload::CustomSection(section) if section.name() == abi::STATIC_SECTION => {
                    guest.region = abi::StaticRegion::read_section(section.data())
                        .map_err(|e| e.to_string())?;
                }
                Payload::CustomSection(section) if section.name() == abi::INTRINSIC_SECTION => {
                    intrinsic_section = Some(section.data());
                }
                _ => {}
            }
        }
        if guest.memories != 1 {
            return Err("linked intrinsics need exactly one guest memory".into());
        }
        if let Some(data) = intrinsic_section {
            guest.region = guest
                .region
                .read_intrinsic_section(data)
                .map_err(|e| e.to_string())?;
        }
        Ok(guest)
    }
    fn function_imports(&self) -> impl Iterator<Item = (u32, &Import<'a>)> {
        self.imports
            .iter()
            .filter(|import| matches!(import.ty, TypeRef::Func(_)))
            .enumerate()
            .map(|(index, import)| (index as u32, import))
    }
}

/// The linker-provided functions the library imports from [`LINK_MODULE`].
#[derive(Debug, Clone, Copy)]
enum Accessor {
    /// `(library dst, guest src, len)`: copy a guest value into a library
    /// buffer.
    MemoryRead,
    /// `(guest dst, library src, len)`: copy a library buffer into guest
    /// memory.
    MemoryWrite,
    /// `(len)`: set the guest's exported `env_result_len`.
    ResultLen,
    /// `() -> i32`: the guest heap's end, where the per-call arena continues
    /// once the reserved arena is full.
    ArenaBase,
    /// `() -> i32`: start of the reserved arena in the intrinsic region.
    ArenaReserved,
    /// `(code, request)`: store a `limits::Resource` failure before trapping.
    ResourceFailure,
    /// `() -> i32`: end of the reserved arena.
    ArenaReservedEnd,
    /// `(len) -> ptr`: the guest allocator (a managed heap's `Allocate`
    /// helper), which collects and reports exhausted memory itself.
    Allocate,
    /// `(handle) -> handle`: the guest retention helper, pushing a root.
    Retain,
    /// `() -> i32`: the root-stack top.
    Roots,
    /// `(i32)`: reset the root-stack top (dropping the roots above it).
    SetRoots,
    /// `() -> i32`: the exception channel version of a v38 module (its
    /// `exceptions::HASHED_DICTIONARY_VERSION` global), or -1 without one.
    ExceptionVersion,
    /// `() -> i32`: the channel's pending flag, or -1 without one.
    ExceptionPending,
    /// `() -> i64`: the channel's thrown value, or -1 without one.
    ExceptionValue,
    /// `(cell)`: throw `cell`: set the channel's value and pending flag.
    Raise,
    /// `() -> i64`: the static region's extension pages (low word) and
    /// intrinsic pages (high word).
    StaticPages,
}
/// What the accessors read from the guest module and its placement.
#[derive(Debug, Clone, Copy)]
struct AccessorTargets {
    result_len: Option<u32>,
    /// The exported `resource_failure` and `resource_request` globals.
    resource: (Option<u32>, Option<u32>),
    /// The guest allocator's heap-end global, when the module has a heap.
    heap: Option<u32>,
    heap_base: usize,
    arena: usize,
    arena_end: usize,
    /// The guest value helpers in merged function numbering.
    values: Option<super::GuestValues>,
    /// The exported exception globals: version selector, pending flag and
    /// value.
    exceptions: (Option<u32>, Option<u32>, Option<u32>),
    region: abi::StaticRegion,
}
impl Accessor {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "memory_read" => Self::MemoryRead,
            "memory_write" => Self::MemoryWrite,
            "result_len" => Self::ResultLen,
            "arena_base" => Self::ArenaBase,
            "arena_reserved" => Self::ArenaReserved,
            "arena_reserved_end" => Self::ArenaReservedEnd,
            "resource_failure" => Self::ResourceFailure,
            "allocate" => Self::Allocate,
            "retain" => Self::Retain,
            "roots" => Self::Roots,
            "set_roots" => Self::SetRoots,
            "exception_version" => Self::ExceptionVersion,
            "exception_pending" => Self::ExceptionPending,
            "exception_value" => Self::ExceptionValue,
            "raise" => Self::Raise,
            "static_pages" => Self::StaticPages,
            _ => return None,
        })
    }
    fn signature(self) -> (&'static [ValType], &'static [ValType]) {
        const I32: ValType = ValType::I32;
        const I64: ValType = ValType::I64;
        match self {
            Self::MemoryRead | Self::MemoryWrite => (&[I32, I32, I32], &[]),
            Self::ResultLen | Self::SetRoots => (&[I32], &[]),
            Self::ArenaBase
            | Self::ArenaReserved
            | Self::ArenaReservedEnd
            | Self::Roots
            | Self::ExceptionVersion
            | Self::ExceptionPending => (&[], &[I32]),
            Self::ResourceFailure => (&[I32, I32], &[]),
            Self::Allocate => (&[I32], &[I32]),
            Self::Retain => (&[I64], &[I64]),
            Self::ExceptionValue | Self::StaticPages => (&[], &[I64]),
            Self::Raise => (&[I64], &[]),
        }
    }
    fn body(self, targets: AccessorTargets) -> Result<encoder::Function, String> {
        use encoder::Instruction as I;
        let mut function = encoder::Function::new([]);
        // Every allocating intrinsic reaches the guest-block scratch path
        // of the library's allocator, which only linked JSON enters; in a
        // module without a managed heap (or exception channel) the value
        // accessors it reaches trap.
        match (self, targets.values) {
            (Self::Allocate | Self::Retain | Self::Roots | Self::SetRoots, None) => {
                function.instruction(&I::Unreachable);
            }
            (Self::Allocate | Self::Retain, Some(values)) => {
                function.instruction(&I::LocalGet(0)).instruction(&I::Call(
                    if matches!(self, Self::Allocate) {
                        values.allocate
                    } else {
                        values.retain
                    },
                ));
            }
            (Self::Roots, Some(values)) => {
                function.instruction(&I::GlobalGet(values.roots));
            }
            (Self::SetRoots, Some(values)) => {
                function
                    .instruction(&I::LocalGet(0))
                    .instruction(&I::GlobalSet(values.roots));
            }
            _ => {}
        }
        match self {
            Self::Allocate | Self::Retain | Self::Roots | Self::SetRoots => {}
            Self::ExceptionVersion | Self::ExceptionPending => {
                let global = if matches!(self, Self::ExceptionVersion) {
                    targets.exceptions.0
                } else {
                    targets.exceptions.1
                };
                match global {
                    Some(global) => function.instruction(&I::GlobalGet(global)),
                    None => function.instruction(&I::I32Const(-1)),
                };
            }
            Self::ExceptionValue => {
                match targets.exceptions.2 {
                    Some(global) => function.instruction(&I::GlobalGet(global)),
                    None => function.instruction(&I::I64Const(-1)),
                };
            }
            Self::Raise => match (targets.exceptions.1, targets.exceptions.2) {
                (Some(pending), Some(value)) => {
                    function
                        .instruction(&I::LocalGet(0))
                        .instruction(&I::GlobalSet(value))
                        .instruction(&I::I32Const(1))
                        .instruction(&I::GlobalSet(pending));
                }
                _ => {
                    function.instruction(&I::Unreachable);
                }
            },
            Self::StaticPages => {
                let pages = u64::from(targets.region.extension_pages())
                    | u64::from(targets.region.intrinsic_pages()) << u32::BITS;
                function.instruction(&I::I64Const(pages as i64));
            }
            Self::MemoryRead | Self::MemoryWrite => {
                for local in 0..COPY_PARAMS {
                    function.instruction(&I::LocalGet(local));
                }
                function.instruction(&I::MemoryCopy {
                    src_mem: GUEST_MEMORY,
                    dst_mem: GUEST_MEMORY,
                });
            }
            Self::ResourceFailure => {
                // A v38 module with an intrinsic region exports both
                // globals; the runtime reports the trap as typed.
                for (local, global) in [targets.resource.0, targets.resource.1]
                    .into_iter()
                    .enumerate()
                {
                    if let Some(global) = global {
                        function
                            .instruction(&I::LocalGet(local as u32))
                            .instruction(&I::GlobalSet(global));
                    }
                }
            }
            Self::ResultLen => {
                // Hosts set the global only when the module exports it.
                if let Some(global) = targets.result_len {
                    function
                        .instruction(&I::LocalGet(0))
                        .instruction(&I::GlobalSet(global));
                }
            }
            Self::ArenaBase => {
                match targets.heap {
                    Some(global) => function.instruction(&I::GlobalGet(global)),
                    None => function.instruction(&I::I32Const(
                        i32::try_from(targets.heap_base).map_err(|e| e.to_string())?,
                    )),
                };
            }
            Self::ArenaReserved | Self::ArenaReservedEnd => {
                let address = if matches!(self, Self::ArenaReserved) {
                    targets.arena
                } else {
                    targets.arena_end
                };
                function.instruction(&I::I32Const(
                    i32::try_from(address).map_err(|e| e.to_string())?,
                ));
            }
        }
        function.instruction(&I::End);
        Ok(function)
    }
}

/// `(i32) -> ()`: set the linked stack pointer, trapping below the region's
/// stack (`[base, base + abi::INTRINSIC_STACK_BYTES)`), where it would
/// overwrite the static extension or the root stack.
fn stack_check(base: usize, stack_pointer: u32) -> Result<encoder::Function, String> {
    use encoder::Instruction as I;
    let base = i32::try_from(base).map_err(|e| e.to_string())?;
    let mut function = encoder::Function::new([]);
    for instruction in [
        I::LocalGet(0),
        I::I32Const(base),
        I::I32LtU,
        I::If(encoder::BlockType::Empty),
        I::Unreachable,
        I::End,
        I::LocalGet(0),
        I::GlobalSet(stack_pointer),
        I::End,
    ] {
        function.instruction(&instruction);
    }
    Ok(function)
}

fn scalar(value: &HostScalar) -> ValType {
    match value {
        HostScalar::I32 => ValType::I32,
        HostScalar::I64 => ValType::I64,
        HostScalar::F64 => ValType::F64,
    }
}

/// Unchanged indices (guest types, memories and data).
struct Identity;
impl Reencode for Identity {
    type Error = String;
}

/// Index maps of guest code: removed imports point at their linked function.
struct GuestIndices {
    imports: Vec<u32>,
    shift: u32,
    kept: u32,
}
impl Reencode for GuestIndices {
    type Error = String;
    fn function_index(&mut self, func: u32) -> Result<u32, ReencodeError<String>> {
        match self.imports.get(func as usize) {
            Some(&index) => Ok(index),
            None => Ok(func - self.shift + self.kept),
        }
    }
}

/// Index maps of library code.
struct LibraryIndices {
    functions: BTreeMap<u32, u32>,
    types: BTreeMap<u32, u32>,
    globals: BTreeMap<u32, u32>,
    /// The library's stack-pointer global and the merged index of its
    /// checked setter, which replaces every `global.set` of it.
    stack: Option<(u32, u32)>,
}
fn missing(what: &str, index: u32) -> ReencodeError<String> {
    ReencodeError::UserError(format!("intrinsic library: unmapped {what} {index}"))
}
impl Reencode for LibraryIndices {
    type Error = String;
    fn function_index(&mut self, func: u32) -> Result<u32, ReencodeError<String>> {
        self.functions
            .get(&func)
            .copied()
            .ok_or_else(|| missing("function", func))
    }
    fn type_index(&mut self, ty: u32) -> Result<u32, ReencodeError<String>> {
        self.types
            .get(&ty)
            .copied()
            .ok_or_else(|| missing("type", ty))
    }
    fn global_index(&mut self, global: u32) -> Result<u32, ReencodeError<String>> {
        self.globals
            .get(&global)
            .copied()
            .ok_or_else(|| missing("global", global))
    }
    fn memory_index(&mut self, memory: u32) -> Result<u32, ReencodeError<String>> {
        if memory == 0 {
            Ok(GUEST_MEMORY)
        } else {
            Err(missing("memory", memory))
        }
    }
    fn instruction<'a>(
        &mut self,
        operator: Operator<'a>,
    ) -> Result<encoder::Instruction<'a>, ReencodeError<String>> {
        match (operator, self.stack) {
            (Operator::GlobalSet { global_index }, Some((stack_pointer, check)))
                if global_index == stack_pointer =>
            {
                Ok(encoder::Instruction::Call(check))
            }
            (operator, _) => encoder::reencode::utils::instruction(self, operator),
        }
    }
    fn table_index(&mut self, table: u32) -> Result<u32, ReencodeError<String>> {
        Err(missing("table", table))
    }
    fn data_index(&mut self, data: u32) -> Result<u32, ReencodeError<String>> {
        Err(missing("passive data segment", data))
    }
}

fn reencode<T>(result: Result<T, ReencodeError<String>>) -> Result<T, String> {
    result.map_err(|error| match error {
        ReencodeError::UserError(message) => message,
        other => other.to_string(),
    })
}

/// The canonical section order; custom sections stay after the section
/// they followed in the guest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    /// Before the first section.
    Header,
    Type,
    Import,
    Function,
    Table,
    Memory,
    Tag,
    Global,
    Export,
    StartFunction,
    Element,
    DataCount,
    Code,
    Data,
}
const ORDER: &[Kind] = &[
    Kind::Header,
    Kind::Type,
    Kind::Import,
    Kind::Function,
    Kind::Table,
    Kind::Memory,
    Kind::Tag,
    Kind::Global,
    Kind::Export,
    Kind::StartFunction,
    Kind::Element,
    Kind::DataCount,
    Kind::Code,
    Kind::Data,
];

#[derive(Default)]
struct Sections<'a> {
    functions: Vec<u32>,
    tables: Option<wasmparser::TableSectionReader<'a>>,
    memories: Option<wasmparser::MemorySectionReader<'a>>,
    globals: Option<wasmparser::GlobalSectionReader<'a>>,
    exports: Option<wasmparser::ExportSectionReader<'a>>,
    start: Option<u32>,
    elements: Option<wasmparser::ElementSectionReader<'a>>,
    data_count: Option<u32>,
    code: Vec<FunctionBody<'a>>,
    data: Option<wasmparser::DataSectionReader<'a>>,
    customs: Vec<(Kind, CustomSectionReader<'a>)>,
}

impl<'a> Sections<'a> {
    fn read(bytes: &'a [u8]) -> Result<Self, String> {
        let mut sections = Self::default();
        let mut last = Kind::Header;
        for payload in Parser::new(0).parse_all(bytes) {
            let payload = payload.map_err(|e| e.to_string())?;
            last = match payload {
                Payload::TypeSection(_) => Kind::Type,
                Payload::ImportSection(_) => Kind::Import,
                Payload::FunctionSection(reader) => {
                    for ty in reader {
                        sections.functions.push(ty.map_err(|e| e.to_string())?);
                    }
                    Kind::Function
                }
                Payload::TableSection(reader) => {
                    sections.tables = Some(reader);
                    Kind::Table
                }
                Payload::MemorySection(reader) => {
                    sections.memories = Some(reader);
                    Kind::Memory
                }
                Payload::TagSection(_) => return Err("guest tags are not linkable".into()),
                Payload::GlobalSection(reader) => {
                    sections.globals = Some(reader);
                    Kind::Global
                }
                Payload::ExportSection(reader) => {
                    sections.exports = Some(reader);
                    Kind::Export
                }
                Payload::StartSection { func, .. } => {
                    sections.start = Some(func);
                    Kind::StartFunction
                }
                Payload::ElementSection(reader) => {
                    sections.elements = Some(reader);
                    Kind::Element
                }
                Payload::DataCountSection { count, .. } => {
                    sections.data_count = Some(count);
                    Kind::DataCount
                }
                Payload::CodeSectionStart { .. } => Kind::Code,
                Payload::CodeSectionEntry(body) => {
                    sections.code.push(body);
                    Kind::Code
                }
                Payload::DataSection(reader) => {
                    sections.data = Some(reader);
                    Kind::Data
                }
                Payload::CustomSection(custom) => {
                    sections.customs.push((last, custom));
                    last
                }
                Payload::Version { .. } | Payload::End(_) => last,
                _ => return Err("unexpected guest module section".into()),
            };
        }
        Ok(sections)
    }
}

/// Each linked import's library export, checked against the registry
/// signature and the guest's declared import type.
fn roots(
    guest: &Guest<'_>,
    library: &Library,
    linked: &[(u32, HostOperation)],
) -> Result<BTreeMap<u32, u32>, String> {
    let mut roots = BTreeMap::new();
    for &(import, operation) in linked {
        let export = library.export(operation.name()).ok_or_else(|| {
            format!(
                "intrinsic library lacks linked operation {}",
                operation.name()
            )
        })?;
        let expected = registry_signature(operation);
        let declared = match guest.imports[import as usize].ty {
            TypeRef::Func(ty) => guest.types.get(ty as usize),
            _ => None,
        };
        if library.signature(export) != Some(&export_signature(operation))
            || declared != Some(&expected)
        {
            return Err(format!(
                "linked operation {} has a mismatched signature",
                operation.name()
            ));
        }
        roots.insert(import, export);
    }
    Ok(roots)
}

/// The registry signature of `operation`, as the guest imports it.
fn registry_signature(operation: HostOperation) -> FuncType {
    let (params, results) = operation.scalar_signature();
    FuncType::new(params.iter().map(scalar), results.iter().map(scalar))
}
/// Whether `operation` returns two i32 values, which its library export
/// packs into one i64 (stable Rust has no multi-value C ABI).
pub(super) fn packed_pair(operation: HostOperation) -> bool {
    operation.scalar_signature().1 == [HostScalar::I32, HostScalar::I32]
}
/// The signature of `operation`'s library export: the registry signature,
/// with a packed i64 for a pair of i32 results.
pub(super) fn export_signature(operation: HostOperation) -> FuncType {
    let (params, _) = operation.scalar_signature();
    if packed_pair(operation) {
        FuncType::new(params.iter().map(scalar), [ValType::I64])
    } else {
        registry_signature(operation)
    }
}

/// A synthesized function of an operation's registry signature that calls
/// its library export, which returns the two i32 results packed in one i64
/// (low word first), and unpacks them.
struct Adapter {
    /// The guest function import it replaces.
    import: u32,
    /// Its merged function index.
    index: u32,
    operation: HostOperation,
}
impl Adapter {
    fn body(&self, export: u32) -> encoder::Function {
        use encoder::Instruction as I;
        let params = self.operation.scalar_signature().0.len() as u32;
        // One i64 local after the parameters holds the packed pair.
        let packed = params;
        let mut function = encoder::Function::new([(1, encoder::ValType::I64)]);
        for local in 0..params {
            function.instruction(&I::LocalGet(local));
        }
        for instruction in [
            I::Call(export),
            I::LocalTee(packed),
            I::I32WrapI64,
            I::LocalGet(packed),
            I::I64Const(i64::from(u32::BITS)),
            I::I64ShrU,
            I::I32WrapI64,
            I::End,
        ] {
            function.instruction(&instruction);
        }
        function
    }
}

/// Everything the merged module is written from (see the module index plan).
struct Plan<'a> {
    guest: &'a Guest<'a>,
    library: &'a Library,
    sections: Sections<'a>,
    /// Guest function import index -> linked library export.
    roots: BTreeMap<u32, u32>,
    closure: Closure,
    placement: Placement,
    adapters: Vec<Adapter>,
    targets: AccessorTargets,
    /// Linked library functions and accessors, in output order.
    defined: Vec<u32>,
    accessors: Vec<u32>,
    function_types: Vec<u32>,
    appended_types: Vec<FuncType>,
    guest_indices: GuestIndices,
    library_indices: LibraryIndices,
}

/// The closure `roots` reach, placed at the intrinsic region of `region`
/// (whose declared pages must hold it).
pub(super) fn placed(
    library: &Library,
    roots: &[u32],
    region: abi::StaticRegion,
) -> Result<(Closure, Placement), String> {
    let closure = library.closure(roots)?;
    let placement = library.place(&closure, region.intrinsic_base())?;
    if placement.bytes > region.intrinsic_bytes() {
        return Err("intrinsic region is smaller than the linked library".into());
    }
    Ok((closure, placement))
}

impl<'a> Plan<'a> {
    fn new(
        bytes: &'a [u8],
        guest: &'a Guest<'a>,
        library: &'a Library,
        linked: &[(u32, HostOperation)],
        heap: Option<u32>,
        values: Option<super::GuestValues>,
    ) -> Result<Self, String> {
        let sections = Sections::read(bytes)?;
        let roots = roots(guest, library, linked)?;
        let (closure, placement) = placed(
            library,
            &roots.values().copied().collect::<Vec<_>>(),
            guest.region,
        )?;
        let library_imports = library.imports.len() as u32;
        let (accessors, defined): (Vec<u32>, Vec<u32>) = closure
            .functions
            .iter()
            .partition(|&&f| f < library_imports);

        let imported = guest.function_imports().count() as u32;
        let kept = imported - roots.len() as u32;
        let base = kept + sections.functions.len() as u32;
        let functions: BTreeMap<u32, u32> = defined
            .iter()
            .chain(&accessors)
            .enumerate()
            .map(|(offset, &index)| (index, base + offset as u32))
            .collect();
        // The checked stack-pointer setter follows the accessors, and the
        // pair adapters follow it.
        let check = base + functions.len() as u32;
        let linked_stack = closure.globals.contains(&library.stack_pointer);
        let adapters: Vec<Adapter> = linked
            .iter()
            .filter(|(_, operation)| packed_pair(*operation))
            .enumerate()
            .map(|(offset, &(import, operation))| Adapter {
                import,
                index: check + u32::from(linked_stack) + offset as u32,
                operation,
            })
            .collect();
        let mut next = 0;
        let imports = guest
            .function_imports()
            .map(|(index, _)| {
                match (
                    roots.get(&index),
                    adapters.iter().find(|a| a.import == index),
                ) {
                    (_, Some(adapter)) => adapter.index,
                    (Some(export), None) => functions[export],
                    (None, None) => {
                        next += 1;
                        next - 1
                    }
                }
            })
            .collect();
        // Only the stack pointer and the zero bases are linked
        // (`Library::closure`).
        let globals: BTreeMap<u32, u32> = closure
            .globals
            .iter()
            .enumerate()
            .map(|(offset, &index)| (index, guest.globals + offset as u32))
            .collect();
        let stack = linked_stack.then_some((library.stack_pointer, check));
        let targets = AccessorTargets {
            result_len: guest.result_len,
            resource: guest.resource,
            heap,
            heap_base: guest.region.heap_base(),
            arena: placement.arena,
            arena_end: placement.arena_end(),
            // Defined guest functions move down past the removed imports.
            values: values.map(|values| super::GuestValues {
                allocate: values.allocate - imported + kept,
                retain: values.retain - imported + kept,
                ..values
            }),
            exceptions: guest.exceptions,
            region: guest.region,
        };
        let mut plan = Self {
            guest,
            library,
            sections,
            roots,
            closure,
            placement,
            adapters,
            targets,
            defined,
            accessors,
            function_types: Vec::new(),
            appended_types: Vec::new(),
            guest_indices: GuestIndices {
                imports,
                shift: imported,
                kept,
            },
            library_indices: LibraryIndices {
                functions,
                types: BTreeMap::new(),
                globals,
                stack,
            },
        };
        plan.plan_types()?;
        Ok(plan)
    }

    /// Library type `ty`, as an index of the merged type section.
    fn merged_type(&mut self, ty: u32) -> Result<u32, String> {
        if let Some(&index) = self.library_indices.types.get(&ty) {
            return Ok(index);
        }
        let func = self
            .library
            .types
            .get(ty as usize)
            .ok_or_else(|| format!("intrinsic library: unknown type {ty}"))?
            .clone();
        let index = self.intern(func);
        self.library_indices.types.insert(ty, index);
        Ok(index)
    }
    /// `func` as an index of the merged type section.
    fn intern(&mut self, func: FuncType) -> u32 {
        let existing = self.guest.types.iter().chain(&self.appended_types);
        (match existing.clone().position(|t| *t == func) {
            Some(index) => index,
            None => {
                let index = existing.count();
                self.appended_types.push(func);
                index
            }
        }) as u32
    }

    /// Function types of the linked functions and accessors, then the types
    /// their block signatures name.
    fn plan_types(&mut self) -> Result<(), String> {
        for index in self
            .defined
            .clone()
            .into_iter()
            .chain(self.accessors.clone())
        {
            let ty = match self.library.defined(index) {
                Some(function) => function.ty,
                None => self.library.imports[index as usize].ty,
            };
            let merged = self.merged_type(ty)?;
            self.function_types.push(merged);
        }
        if self.library_indices.stack.is_some() {
            let setter = self.intern(FuncType::new([ValType::I32], []));
            self.function_types.push(setter);
        }
        for operation in self
            .adapters
            .iter()
            .map(|a| a.operation)
            .collect::<Vec<_>>()
        {
            let adapter = self.intern(registry_signature(operation));
            self.function_types.push(adapter);
        }
        for index in self.defined.clone() {
            let body = &self.body(index)?;
            let mut reader = body.get_operators_reader().map_err(|e| e.to_string())?;
            while !reader.eof() {
                if let Operator::Block { blockty }
                | Operator::Loop { blockty }
                | Operator::If { blockty } = reader.read().map_err(|e| e.to_string())?
                    && let BlockType::FuncType(ty) = blockty
                {
                    self.merged_type(ty)?;
                }
            }
        }
        Ok(())
    }

    fn body(&self, index: u32) -> Result<FunctionBody<'static>, String> {
        self.library
            .defined(index)
            .map(|function| function.body.clone())
            .ok_or_else(|| "intrinsic library: missing body".into())
    }

    fn emit(mut self) -> Result<Vec<u8>, String> {
        let mut module = encoder::Module::new();
        for &kind in ORDER {
            self.section(&mut module, kind)?;
            for (anchor, custom) in &self.sections.customs {
                if *anchor == kind {
                    module.section(&encoder::CustomSection {
                        name: custom.name().into(),
                        data: custom.data().into(),
                    });
                }
            }
        }
        Ok(module.finish())
    }

    fn section(&mut self, module: &mut encoder::Module, kind: Kind) -> Result<(), String> {
        let mut identity = Identity;
        match kind {
            Kind::Header | Kind::Tag => {}
            Kind::Type => {
                let mut section = encoder::TypeSection::new();
                for ty in self.guest.types.iter().chain(&self.appended_types) {
                    let values = |values: &[ValType]| -> Result<Vec<_>, String> {
                        values
                            .iter()
                            .map(|&v| reencode(Identity.val_type(v)))
                            .collect()
                    };
                    section
                        .ty()
                        .function(values(ty.params())?, values(ty.results())?);
                }
                module.section(&section);
            }
            Kind::Import => {
                let mut section = encoder::ImportSection::new();
                let mut function = 0_u32;
                for import in &self.guest.imports {
                    if matches!(import.ty, TypeRef::Func(_)) {
                        function += 1;
                        if self.roots.contains_key(&(function - 1)) {
                            continue;
                        }
                    }
                    let entity = reencode(identity.entity_type(import.ty))?;
                    section.import(import.module, import.name, entity);
                }
                module.section(&section);
            }
            Kind::Function => {
                let mut section = encoder::FunctionSection::new();
                for &ty in self.sections.functions.iter().chain(&self.function_types) {
                    section.function(ty);
                }
                module.section(&section);
            }
            Kind::Table => {
                if let Some(reader) = self.sections.tables.clone() {
                    let mut section = encoder::TableSection::new();
                    reencode(self.guest_indices.parse_table_section(&mut section, reader))?;
                    module.section(&section);
                }
            }
            Kind::Memory => {
                let mut section = encoder::MemorySection::new();
                if let Some(reader) = self.sections.memories.clone() {
                    reencode(identity.parse_memory_section(&mut section, reader))?;
                }
                module.section(&section);
            }
            Kind::Global => self.globals(module)?,
            Kind::Export => {
                if let Some(reader) = self.sections.exports.clone() {
                    let mut section = encoder::ExportSection::new();
                    reencode(
                        self.guest_indices
                            .parse_export_section(&mut section, reader),
                    )?;
                    module.section(&section);
                }
            }
            Kind::StartFunction => {
                if let Some(func) = self.sections.start {
                    let function_index = reencode(self.guest_indices.function_index(func))?;
                    module.section(&encoder::StartSection { function_index });
                }
            }
            Kind::Element => {
                if let Some(reader) = self.sections.elements.clone() {
                    let mut section = encoder::ElementSection::new();
                    reencode(
                        self.guest_indices
                            .parse_element_section(&mut section, reader),
                    )?;
                    module.section(&section);
                }
            }
            Kind::DataCount => {
                if let Some(count) = self.sections.data_count {
                    module.section(&encoder::DataCountSection {
                        count: count + self.closure.segments.len() as u32,
                    });
                }
            }
            Kind::Code => self.code(module)?,
            Kind::Data => self.data(module)?,
        }
        Ok(())
    }

    fn globals(&mut self, module: &mut encoder::Module) -> Result<(), String> {
        let mut section = encoder::GlobalSection::new();
        if let Some(reader) = self.sections.globals.clone() {
            reencode(
                self.guest_indices
                    .parse_global_section(&mut section, reader),
            )?;
        }
        for &index in &self.closure.globals {
            let global = self
                .library
                .globals
                .get(index as usize)
                .ok_or("intrinsic library: missing global")?;
            let ty = reencode(Identity.global_type(global.ty))?;
            if global.zero_base {
                // Relocated relative addresses are absolute: the base stays 0.
                section.global(ty, &encoder::ConstExpr::i32_const(0));
                continue;
            }
            if index != self.library.stack_pointer
                || global.ty.content_type != ValType::I32
                || !global.ty.mutable
            {
                return Err(
                    "intrinsic library: linked globals must be its i32 stack pointer".into(),
                );
            }
            // The stack starts at the top of the region's stack.
            let top = i32::try_from(self.placement.stack_top()).map_err(|e| e.to_string())?;
            section.global(ty, &encoder::ConstExpr::i32_const(top));
        }
        if !section.is_empty() {
            module.section(&section);
        }
        Ok(())
    }

    fn code(&mut self, module: &mut encoder::Module) -> Result<(), String> {
        let mut section = encoder::CodeSection::new();
        for body in &self.sections.code {
            reencode(
                self.guest_indices
                    .parse_function_body(&mut section, body.clone()),
            )?;
        }
        for index in self.defined.clone() {
            let function = self
                .library
                .defined(index)
                .ok_or("intrinsic library: missing body")?;
            let bytes = self.library.relocated_body(function, &self.placement)?;
            let body = FunctionBody::new(BinaryReader::new(&bytes, 0));
            reencode(self.library_indices.parse_function_body(&mut section, body))?;
        }
        for &index in &self.accessors {
            let import = &self.library.imports[index as usize];
            let accessor = Accessor::from_name(import.name).ok_or_else(|| {
                format!(
                    "intrinsic library: unknown {LINK_MODULE} import {}",
                    import.name
                )
            })?;
            let (params, results) = accessor.signature();
            let expected = FuncType::new(params.iter().copied(), results.iter().copied());
            if self.library.types.get(import.ty as usize) != Some(&expected) {
                return Err(format!(
                    "intrinsic library: {LINK_MODULE}.{} has a mismatched signature",
                    import.name
                ));
            }
            section.function(&accessor.body(self.targets)?);
        }
        if let Some((stack_pointer, _)) = self.library_indices.stack {
            let merged = self.library_indices.globals[&stack_pointer];
            section.function(&stack_check(self.placement.base, merged)?);
        }
        for adapter in &self.adapters {
            let export = self
                .roots
                .get(&adapter.import)
                .and_then(|export| self.library_indices.functions.get(export))
                .ok_or("intrinsic library: unmapped pair adapter")?;
            section.function(&adapter.body(*export));
        }
        module.section(&section);
        Ok(())
    }

    fn data(&mut self, module: &mut encoder::Module) -> Result<(), String> {
        let mut section = encoder::DataSection::new();
        if let Some(reader) = self.sections.data.clone() {
            reencode(Identity.parse_data_section(&mut section, reader))?;
        }
        for &index in &self.closure.segments {
            let segment = self
                .library
                .segments
                .get(index as usize)
                .ok_or("intrinsic library: missing data segment")?;
            let address =
                i32::try_from(self.placement.addresses[&index]).map_err(|e| e.to_string())?;
            section.active(
                GUEST_MEMORY,
                &encoder::ConstExpr::i32_const(address),
                self.library.relocated_segment(segment, &self.placement)?,
            );
        }
        if !section.is_empty() {
            module.section(&section);
        }
        Ok(())
    }
}

pub(super) fn merge(
    bytes: &[u8],
    guest: &Guest<'_>,
    library: &Library,
    linked: &[(u32, HostOperation)],
    heap: Option<u32>,
    values: Option<super::GuestValues>,
) -> Result<Vec<u8>, String> {
    Plan::new(bytes, guest, library, linked, heap, values)?.emit()
}
