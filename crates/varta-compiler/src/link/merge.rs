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
//!
//! Library types are deduplicated against the guest's, library globals and
//! data segments follow the guest's, and the library's memory becomes the
//! module's last memory (private: no export, no import).
use super::library::{Closure, LINK_MODULE, Library};
use std::collections::BTreeMap;
use varta_common::{HostOperation, HostScalar, abi};
use wasm_encoder::{
    self as encoder,
    reencode::{Error as ReencodeError, Reencode},
};
use wasmparser::{
    BlockType, CustomSectionReader, ExternalKind, FuncType, FunctionBody, Operator, Parser,
    Payload, TypeRef, ValType,
};

/// Guest memory 0: the exported ABI memory every accessor addresses.
const GUEST_MEMORY: u32 = 0;
/// `memory.size` counts pages; the size accessor reports bytes.
const PAGE_SHIFT: i32 = abi::WASM_PAGE_BYTES.trailing_zeros() as i32;

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
}

impl<'a> Guest<'a> {
    pub(super) fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        let mut guest = Self {
            imports: Vec::new(),
            types: Vec::new(),
            memories: 0,
            globals: 0,
            result_len: None,
        };
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
                        if export.kind == ExternalKind::Global && export.name == abi::RESULT_LEN {
                            guest.result_len = Some(export.index);
                        }
                    }
                }
                _ => {}
            }
        }
        if guest.memories != 1 {
            return Err("linked intrinsics need exactly one guest memory".into());
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
    /// `() -> i32`: byte size of guest memory 0.
    MemorySize,
    /// `(pages) -> i32`: `memory.grow` of guest memory 0.
    MemoryGrow,
    /// `(private dst, guest src, len)`: copy out of guest memory 0.
    MemoryRead,
    /// `(guest dst, private src, len)`: copy into guest memory 0.
    MemoryWrite,
    /// `(len)`: set the guest's exported `env_result_len`.
    ResultLen,
}
impl Accessor {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "memory_size" => Self::MemorySize,
            "memory_grow" => Self::MemoryGrow,
            "memory_read" => Self::MemoryRead,
            "memory_write" => Self::MemoryWrite,
            "result_len" => Self::ResultLen,
            _ => return None,
        })
    }
    fn signature(self) -> (&'static [ValType], &'static [ValType]) {
        const I32: ValType = ValType::I32;
        match self {
            Self::MemorySize => (&[], &[I32]),
            Self::MemoryGrow => (&[I32], &[I32]),
            Self::MemoryRead | Self::MemoryWrite => (&[I32, I32, I32], &[]),
            Self::ResultLen => (&[I32], &[]),
        }
    }
    fn body(self, private: u32, result_len: Option<u32>) -> encoder::Function {
        use encoder::Instruction as I;
        let mut function = encoder::Function::new([]);
        let copy = |function: &mut encoder::Function, src_mem, dst_mem| {
            for local in 0..3 {
                function.instruction(&I::LocalGet(local));
            }
            function.instruction(&I::MemoryCopy { src_mem, dst_mem });
        };
        match self {
            Self::MemorySize => {
                function
                    .instruction(&I::MemorySize(GUEST_MEMORY))
                    .instruction(&I::I32Const(PAGE_SHIFT))
                    .instruction(&I::I32Shl);
            }
            Self::MemoryGrow => {
                function
                    .instruction(&I::LocalGet(0))
                    .instruction(&I::MemoryGrow(GUEST_MEMORY));
            }
            Self::MemoryRead => copy(&mut function, GUEST_MEMORY, private),
            Self::MemoryWrite => copy(&mut function, private, GUEST_MEMORY),
            Self::ResultLen => {
                // Hosts set the global only when the module exports it.
                if let Some(global) = result_len {
                    function
                        .instruction(&I::LocalGet(0))
                        .instruction(&I::GlobalSet(global));
                }
            }
        }
        function.instruction(&I::End);
        function
    }
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
    memory: u32,
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
            Ok(self.memory)
        } else {
            Err(missing("memory", memory))
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
        let (params, results) = operation.scalar_signature();
        let expected = FuncType::new(params.iter().map(scalar), results.iter().map(scalar));
        let declared = match guest.imports[import as usize].ty {
            TypeRef::Func(ty) => guest.types.get(ty as usize),
            _ => None,
        };
        if library.signature(export) != Some(&expected) || declared != Some(&expected) {
            return Err(format!(
                "linked operation {} has a mismatched signature",
                operation.name()
            ));
        }
        roots.insert(import, export);
    }
    Ok(roots)
}

/// Everything the merged module is written from (see the module index plan).
struct Plan<'a> {
    guest: &'a Guest<'a>,
    library: &'a Library,
    sections: Sections<'a>,
    /// Guest function import index -> linked library export.
    roots: BTreeMap<u32, u32>,
    closure: Closure,
    /// Linked library functions and accessors, in output order.
    defined: Vec<u32>,
    accessors: Vec<u32>,
    function_types: Vec<u32>,
    appended_types: Vec<FuncType>,
    guest_indices: GuestIndices,
    library_indices: LibraryIndices,
}

impl<'a> Plan<'a> {
    fn new(
        bytes: &'a [u8],
        guest: &'a Guest<'a>,
        library: &'a Library,
        linked: &[(u32, HostOperation)],
    ) -> Result<Self, String> {
        let sections = Sections::read(bytes)?;
        let roots = roots(guest, library, linked)?;
        let closure = library.closure(&roots.values().copied().collect::<Vec<_>>())?;
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
        let mut next = 0;
        let imports = guest
            .function_imports()
            .map(|(index, _)| match roots.get(&index) {
                Some(export) => functions[export],
                None => {
                    next += 1;
                    next - 1
                }
            })
            .collect();
        let globals = closure
            .globals
            .iter()
            .enumerate()
            .map(|(offset, &index)| (index, guest.globals + offset as u32))
            .collect();
        let mut plan = Self {
            guest,
            library,
            sections,
            roots,
            closure,
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
                memory: guest.memories,
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
            .ok_or_else(|| format!("intrinsic library: unknown type {ty}"))?;
        let existing = self.guest.types.iter().chain(&self.appended_types);
        let index = match existing.clone().position(|t| t == func) {
            Some(index) => index,
            None => {
                let index = existing.count();
                self.appended_types.push(func.clone());
                index
            }
        } as u32;
        self.library_indices.types.insert(ty, index);
        Ok(index)
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
                section.memory(reencode(identity.memory_type(self.library.memory))?);
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
            let init = reencode(self.library_indices.const_expr(global.init.clone()))?;
            section.global(ty, &init);
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
            let body = self.body(index)?;
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
            section.function(&accessor.body(self.library_indices.memory, self.guest.result_len));
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
            let offset = reencode(self.library_indices.const_expr(segment.offset.clone()))?;
            section.active(
                self.library_indices.memory,
                &offset,
                segment.data.iter().copied(),
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
) -> Result<Vec<u8>, String> {
    Plan::new(bytes, guest, library, linked)?.emit()
}
