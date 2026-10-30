//! The embedded v38 guest intrinsic library and the closure a program needs.
//!
//! `intrinsics.wasm` is `varta-intrinsics-wasm` built by
//! `scripts/build-intrinsics.py` with wasm-ld relocations kept
//! (`--emit-relocs`) and one data segment per input section, so the closure
//! keeps exactly the functions and data segments reachable from the exports a
//! program imports. The library is parsed once per process.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;
use wasmparser::{
    BinaryReader, ConstExpr, DataKind, ExternalKind, FuncType, FunctionBody, GlobalType,
    MemoryType, Operator, Parser, Payload, TypeRef,
};

/// The checked-in library (see `intrinsics.json` for its provenance).
pub(super) const LIBRARY: &[u8] = include_bytes!("intrinsics.wasm");

/// Module that the library's linker-provided accessors are imported from.
pub(super) const LINK_MODULE: &str = "varta_link";

/// `linking` symbol kinds and flags (tool-conventions `Linking.md`).
const SYMBOL_FUNCTION: u8 = 0;
const SYMBOL_DATA: u8 = 1;
const SYMBOL_GLOBAL: u8 = 2;
const SYMBOL_SECTION: u8 = 3;
const SYMBOL_TAG: u8 = 4;
const SYMBOL_TABLE: u8 = 5;
const SYMBOL_UNDEFINED: u32 = 0x10;
const SYMBOL_EXPLICIT_NAME: u32 = 0x40;
const SYMBOL_ABSOLUTE: u32 = 0x200;
const LINKING_VERSION: u32 = 2;
const SUBSECTION_SYMBOL_TABLE: u8 = 8;

/// Relocation types that address linear memory through a data symbol.
const MEMORY_RELOCATIONS: &[u8] = &[3, 4, 5, 11, 14, 15, 16, 17, 21, 23, 25];
/// Relocation types that carry an addend (`Linking.md`).
const ADDEND_RELOCATIONS: &[u8] = &[3, 4, 5, 8, 9, 11, 14, 15, 16, 17, 21, 22, 23, 25];

#[derive(Debug)]
pub(super) struct Import {
    pub name: &'static str,
    pub ty: u32,
}
#[derive(Debug)]
pub(super) struct Function {
    pub ty: u32,
    pub body: FunctionBody<'static>,
    calls: BTreeSet<u32>,
    globals: BTreeSet<u32>,
    /// Data segments the body addresses; `None` without relocation data.
    segments: Option<BTreeSet<u32>>,
    indirect: bool,
}
#[derive(Debug)]
pub(super) struct Global {
    pub ty: GlobalType,
    pub init: ConstExpr<'static>,
}
#[derive(Debug)]
pub(super) struct Segment {
    pub offset: ConstExpr<'static>,
    pub data: &'static [u8],
    /// Segments this segment's bytes point into (relocated pointers).
    references: BTreeSet<u32>,
}

/// The parsed library: one memory, function imports from [`LINK_MODULE`]
/// only, no table use, active data segments.
#[derive(Debug)]
pub(super) struct Library {
    pub types: Vec<FuncType>,
    pub imports: Vec<Import>,
    pub functions: Vec<Function>,
    pub globals: Vec<Global>,
    pub memory: MemoryType,
    pub exports: BTreeMap<&'static str, u32>,
    pub segments: Vec<Segment>,
}

/// What one program links: library function, global and segment indices.
#[derive(Debug, Default)]
pub(super) struct Closure {
    /// Absolute library function indices (imports first), ascending.
    pub functions: BTreeSet<u32>,
    pub globals: BTreeSet<u32>,
    pub segments: BTreeSet<u32>,
}

fn invalid(message: impl std::fmt::Display) -> String {
    format!("intrinsic library: {message}")
}

/// The process-wide parsed library.
pub(super) fn library() -> Result<&'static Library, String> {
    static PARSED: OnceLock<Result<Library, String>> = OnceLock::new();
    PARSED
        .get_or_init(|| Library::parse(LIBRARY))
        .as_ref()
        .map_err(Clone::clone)
}

#[derive(Default)]
struct Relocations {
    code_start: usize,
    data_start: usize,
    /// Section index (in file order) of the code and data sections.
    code_section: Option<u32>,
    data_section: Option<u32>,
    /// Data symbol index -> segment index (defined, non-absolute symbols).
    data_symbols: BTreeMap<u32, u32>,
    /// (section index, absolute file offset, symbol) of memory relocations.
    memory: Vec<(u32, usize, u32)>,
    seen: bool,
}

impl Library {
    fn parse(bytes: &'static [u8]) -> Result<Self, String> {
        let mut types = Vec::new();
        let mut imports = Vec::new();
        let mut declared = Vec::new();
        let mut bodies = Vec::new();
        let mut globals = Vec::new();
        let mut memories = Vec::new();
        let mut exports = BTreeMap::new();
        let mut segments = Vec::new();
        let mut segment_ranges = Vec::new();
        let mut relocations = Relocations::default();
        let mut section = 0_u32;
        for payload in Parser::new(0).parse_all(bytes) {
            let payload = payload.map_err(invalid)?;
            let counted = is_section(&payload);
            match payload {
                Payload::TypeSection(reader) => {
                    for ty in reader.into_iter_err_on_gc_types() {
                        types.push(ty.map_err(invalid)?);
                    }
                }
                Payload::ImportSection(reader) => {
                    for import in reader {
                        let import = import.map_err(invalid)?;
                        let TypeRef::Func(ty) = import.ty else {
                            return Err(invalid("only function imports are linkable"));
                        };
                        if import.module != LINK_MODULE {
                            return Err(invalid(format!(
                                "unknown import module {}",
                                import.module
                            )));
                        }
                        imports.push(Import {
                            name: import.name,
                            ty,
                        });
                    }
                }
                Payload::FunctionSection(reader) => {
                    for ty in reader {
                        declared.push(ty.map_err(invalid)?);
                    }
                }
                Payload::TableSection(reader) => {
                    // Rust reserves a one-slot table for null function pointers;
                    // element segments (real targets) are rejected below.
                    for table in reader {
                        table.map_err(invalid)?;
                    }
                }
                Payload::ElementSection(reader) if reader.count() > 0 => {
                    return Err(invalid("function tables are not linkable"));
                }
                Payload::MemorySection(reader) => {
                    for memory in reader {
                        memories.push(memory.map_err(invalid)?);
                    }
                }
                Payload::GlobalSection(reader) => {
                    for global in reader {
                        let global = global.map_err(invalid)?;
                        globals.push(Global {
                            ty: global.ty,
                            init: global.init_expr,
                        });
                    }
                }
                Payload::ExportSection(reader) => {
                    for export in reader {
                        let export = export.map_err(invalid)?;
                        if export.kind == ExternalKind::Func {
                            exports.insert(export.name, export.index);
                        }
                    }
                }
                Payload::StartSection { .. } => {
                    return Err(invalid("a start function is not linkable"));
                }
                Payload::CodeSectionStart { range, .. } => {
                    relocations.code_start = range.start;
                    relocations.code_section = Some(section);
                }
                Payload::CodeSectionEntry(body) => bodies.push(body),
                Payload::DataSection(reader) => {
                    relocations.data_start = reader.range().start;
                    relocations.data_section = Some(section);
                    for data in reader {
                        let data = data.map_err(invalid)?;
                        let DataKind::Active {
                            memory_index: 0,
                            offset_expr,
                        } = data.kind
                        else {
                            return Err(invalid("only active memory 0 segments are linkable"));
                        };
                        segment_ranges.push(data.range.clone());
                        segments.push(Segment {
                            offset: offset_expr,
                            data: data.data,
                            references: BTreeSet::new(),
                        });
                    }
                }
                Payload::CustomSection(custom) if custom.name() == "linking" => {
                    relocations.seen = true;
                    read_symbols(custom.data(), &mut relocations.data_symbols)?;
                }
                Payload::CustomSection(custom) if custom.name().starts_with("reloc.") => {
                    read_relocations(custom.data(), custom.data_offset(), &mut relocations)?;
                }
                _ => {}
            }
            if counted {
                section += 1;
            }
        }
        let [memory] = memories[..] else {
            return Err(invalid("expected exactly one memory"));
        };
        if declared.len() != bodies.len() {
            return Err(invalid("function and code sections disagree"));
        }
        let mut functions = Vec::with_capacity(bodies.len());
        for (ty, body) in declared.into_iter().zip(bodies) {
            functions.push(scan(ty, body)?);
        }
        let mut library = Self {
            types,
            imports,
            functions,
            globals,
            memory,
            exports,
            segments,
        };
        if relocations.seen {
            library.attach(&relocations, &segment_ranges);
        }
        Ok(library)
    }

    /// Attribute memory relocations to the function or segment they patch.
    fn attach(&mut self, relocations: &Relocations, segment_ranges: &[std::ops::Range<usize>]) {
        for function in &mut self.functions {
            function.segments = Some(BTreeSet::new());
        }
        for &(section, at, symbol) in &relocations.memory {
            let Some(&target) = relocations.data_symbols.get(&symbol) else {
                continue;
            };
            if Some(section) == relocations.code_section {
                let at = relocations.code_start + at;
                if let Some(function) = self
                    .functions
                    .iter_mut()
                    .find(|f| f.body.range().contains(&at))
                {
                    function.segments.get_or_insert_default().insert(target);
                }
            } else if Some(section) == relocations.data_section {
                let at = relocations.data_start + at;
                if let Some(index) = segment_ranges.iter().position(|r| r.contains(&at)) {
                    self.segments[index].references.insert(target);
                }
            }
        }
    }

    /// Absolute index of the function exported as `name`.
    pub(super) fn export(&self, name: &str) -> Option<u32> {
        self.exports.get(name).copied()
    }
    /// Signature of absolute function `index`.
    pub(super) fn signature(&self, index: u32) -> Option<&FuncType> {
        let ty = match self.defined(index) {
            Some(function) => function.ty,
            None => self.imports.get(index as usize)?.ty,
        };
        self.types.get(ty as usize)
    }
    pub(super) fn defined(&self, index: u32) -> Option<&Function> {
        self.functions
            .get((index as usize).checked_sub(self.imports.len())?)
    }

    /// Every function, global and data segment reachable from `roots`.
    pub(super) fn closure(&self, roots: &[u32]) -> Result<Closure, String> {
        let mut closure = Closure::default();
        let mut pending = roots.to_vec();
        let mut all_data = false;
        while let Some(index) = pending.pop() {
            if !closure.functions.insert(index) {
                continue;
            }
            let Some(function) = self.defined(index) else {
                if (index as usize) >= self.imports.len() {
                    return Err(invalid(format!("unknown function {index}")));
                }
                continue;
            };
            if function.indirect {
                return Err(invalid(format!(
                    "function {index} uses a table or an indirect call"
                )));
            }
            pending.extend(function.calls.iter().copied());
            closure.globals.extend(function.globals.iter().copied());
            match &function.segments {
                Some(segments) => closure.segments.extend(segments.iter().copied()),
                None => all_data = true,
            }
        }
        if all_data {
            closure.segments = (0..self.segments.len() as u32).collect();
        }
        let mut pending: Vec<u32> = closure.segments.iter().copied().collect();
        while let Some(segment) = pending.pop() {
            let references = &self
                .segments
                .get(segment as usize)
                .ok_or_else(|| invalid(format!("unknown data segment {segment}")))?
                .references;
            for &target in references {
                if closure.segments.insert(target) {
                    pending.push(target);
                }
            }
        }
        Ok(closure)
    }
}

/// Whether a payload is a section (counted by relocation section indices).
fn is_section(payload: &Payload<'_>) -> bool {
    matches!(
        payload,
        Payload::TypeSection(_)
            | Payload::ImportSection(_)
            | Payload::FunctionSection(_)
            | Payload::TableSection(_)
            | Payload::MemorySection(_)
            | Payload::TagSection(_)
            | Payload::GlobalSection(_)
            | Payload::ExportSection(_)
            | Payload::StartSection { .. }
            | Payload::ElementSection(_)
            | Payload::DataCountSection { .. }
            | Payload::CodeSectionStart { .. }
            | Payload::DataSection(_)
            | Payload::CustomSection(_)
    )
}

/// Calls, globals and table use of one body.
fn scan(ty: u32, body: FunctionBody<'static>) -> Result<Function, String> {
    let mut calls = BTreeSet::new();
    let mut globals = BTreeSet::new();
    let mut indirect = false;
    let mut reader = body.get_operators_reader().map_err(invalid)?;
    while !reader.eof() {
        match reader.read().map_err(invalid)? {
            Operator::Call { function_index } | Operator::ReturnCall { function_index } => {
                calls.insert(function_index);
            }
            Operator::GlobalGet { global_index } | Operator::GlobalSet { global_index } => {
                globals.insert(global_index);
            }
            Operator::CallIndirect { .. }
            | Operator::ReturnCallIndirect { .. }
            | Operator::RefFunc { .. }
            | Operator::TableGet { .. }
            | Operator::TableSet { .. }
            | Operator::TableGrow { .. }
            | Operator::TableSize { .. }
            | Operator::TableFill { .. }
            | Operator::TableCopy { .. }
            | Operator::TableInit { .. }
            | Operator::ElemDrop { .. }
            | Operator::MemoryInit { .. }
            | Operator::DataDrop { .. } => indirect = true,
            _ => {}
        }
    }
    Ok(Function {
        ty,
        body,
        calls,
        globals,
        segments: None,
        indirect,
    })
}

fn string<'a>(reader: &mut BinaryReader<'a>) -> Result<&'a str, String> {
    reader.read_string().map_err(invalid)
}

/// Defined, segment-backed data symbols of the `linking` symbol table.
fn read_symbols(data: &[u8], symbols: &mut BTreeMap<u32, u32>) -> Result<(), String> {
    let mut reader = BinaryReader::new(data, 0);
    if reader.read_var_u32().map_err(invalid)? != LINKING_VERSION {
        return Err(invalid("unsupported linking metadata version"));
    }
    while !reader.eof() {
        let kind = reader.read_u8().map_err(invalid)?;
        let size = reader.read_var_u32().map_err(invalid)? as usize;
        let payload = reader.read_bytes(size).map_err(invalid)?;
        if kind != SUBSECTION_SYMBOL_TABLE {
            continue;
        }
        let mut table = BinaryReader::new(payload, 0);
        let count = table.read_var_u32().map_err(invalid)?;
        for index in 0..count {
            let kind = table.read_u8().map_err(invalid)?;
            let flags = table.read_var_u32().map_err(invalid)?;
            let defined = flags & SYMBOL_UNDEFINED == 0;
            match kind {
                SYMBOL_FUNCTION | SYMBOL_GLOBAL | SYMBOL_TAG | SYMBOL_TABLE => {
                    table.read_var_u32().map_err(invalid)?;
                    if defined || flags & SYMBOL_EXPLICIT_NAME != 0 {
                        string(&mut table)?;
                    }
                }
                SYMBOL_DATA => {
                    string(&mut table)?;
                    if defined {
                        let segment = table.read_var_u32().map_err(invalid)?;
                        table.read_var_u64().map_err(invalid)?;
                        table.read_var_u64().map_err(invalid)?;
                        if flags & SYMBOL_ABSOLUTE == 0 {
                            symbols.insert(index, segment);
                        }
                    }
                }
                SYMBOL_SECTION => {
                    table.read_var_u32().map_err(invalid)?;
                }
                _ => return Err(invalid(format!("unknown symbol kind {kind}"))),
            }
        }
    }
    Ok(())
}

/// Memory relocations of one `reloc.*` section, as absolute file offsets
/// relative to their target section's payload.
fn read_relocations(
    data: &[u8],
    offset: usize,
    relocations: &mut Relocations,
) -> Result<(), String> {
    let mut reader = BinaryReader::new(data, offset);
    let section = reader.read_var_u32().map_err(invalid)?;
    let count = reader.read_var_u32().map_err(invalid)?;
    for _ in 0..count {
        let kind = reader.read_u8().map_err(invalid)?;
        let at = reader.read_var_u32().map_err(invalid)? as usize;
        let symbol = reader.read_var_u32().map_err(invalid)?;
        if ADDEND_RELOCATIONS.contains(&kind) {
            reader.read_var_i64().map_err(invalid)?;
        }
        if MEMORY_RELOCATIONS.contains(&kind) {
            relocations.memory.push((section, at, symbol));
        }
    }
    Ok(())
}
