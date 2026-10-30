//! The embedded v38 guest intrinsic library, the closure a program needs and
//! its placement in the module's intrinsic region.
//!
//! `intrinsics.wasm` is `varta-intrinsics-wasm` built by
//! `scripts/build-intrinsics.py` with wasm-ld relocations kept
//! (`--emit-relocs`) and one data segment per input section. The relocations
//! serve twice: the closure keeps exactly the functions and data segments
//! reachable from the exports a program imports, and [`Placement`] moves
//! those segments into the program's intrinsic region of memory 0 by
//! rewriting every memory address the code and data hold (tool-conventions
//! `Linking.md`), never by assuming the library's own addresses. The library
//! is parsed once per process.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;
use varta_common::abi;
use wasmparser::{
    BinaryReader, ConstExpr, DataKind, ExternalKind, FuncType, FunctionBody, GlobalType, Operator,
    Parser, Payload, TypeRef,
};

/// The checked-in library (see `intrinsics.json` for its provenance).
pub(super) const LIBRARY: &[u8] = include_bytes!("intrinsics.wasm");

/// Module that the library's linker-provided accessors are imported from.
pub(super) const LINK_MODULE: &str = "varta_link";
/// The library's shadow-stack pointer (wasm-ld's global symbol).
const STACK_POINTER: &str = "__stack_pointer";

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
const SUBSECTION_SEGMENT_INFO: u8 = 5;
const SUBSECTION_SYMBOL_TABLE: u8 = 8;

/// Memory-address relocation types the placement rewrites.
const R_MEMORY_ADDR_LEB: u8 = 3;
const R_MEMORY_ADDR_SLEB: u8 = 4;
const R_MEMORY_ADDR_I32: u8 = 5;
/// An address relative to `__memory_base`, as position-independent code
/// (the `wasm32-wasip2` standard library) forms it: `global.get
/// __memory_base` plus this constant. The library is linked with memory base
/// 0 (checked through the absolute `__memory_base` symbol), so the constant
/// is the absolute address and is rewritten like an SLEB address while the
/// base stays 0.
const R_MEMORY_ADDR_REL_SLEB: u8 = 11;
/// Other memory relocation types (64-bit, TLS and location-relative): a
/// function or segment carrying one is not linkable.
const UNSUPPORTED_MEMORY_RELOCATIONS: &[u8] = &[14, 15, 16, 17, 21, 23, 25];
/// wasm-ld's absolute symbol for the memory base of position-independent code.
const MEMORY_BASE: &str = "__memory_base";
/// Relocation types that carry an addend (`Linking.md`).
const ADDEND_RELOCATIONS: &[u8] = &[3, 4, 5, 8, 9, 11, 14, 15, 16, 17, 21, 22, 23, 25];
/// Bytes of an uncompressed relocatable LEB (wasm-ld keeps them padded with
/// `--emit-relocs`) and of an i32 address in data.
const PADDED_LEB_BYTES: usize = 5;
const I32_BYTES: usize = 4;
const LEB_PAYLOAD_BITS: u32 = 7;
const LEB_CONTINUATION: u8 = 0x80;
const LEB_PAYLOAD_MASK: u64 = 0x7f;

#[derive(Debug)]
pub(super) struct Import {
    pub name: &'static str,
    pub ty: u32,
}

/// Where a relocated address points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    /// Byte `offset` (symbol offset plus addend) of library data segment
    /// `segment`.
    Data { segment: u32, offset: i64 },
    /// A linker-synthesized absolute address (`__heap_base`, ...), which
    /// has no meaning once the library is placed: not linkable.
    Absolute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Leb,
    Sleb,
    I32,
}

/// One memory address in a function body or data segment, at byte `at` of
/// that body or segment's bytes.
#[derive(Debug, Clone, Copy)]
struct Relocation {
    encoding: Encoding,
    at: usize,
    target: Target,
}

#[derive(Debug)]
pub(super) struct Function {
    pub ty: u32,
    pub body: FunctionBody<'static>,
    calls: BTreeSet<u32>,
    globals: BTreeSet<u32>,
    relocations: Vec<Relocation>,
    /// A table, indirect call, passive segment or unsupported relocation.
    unlinkable: bool,
}
#[derive(Debug)]
pub(super) struct Global {
    pub ty: GlobalType,
    /// An immutable i32 global initialized to 0: the zero memory and table
    /// bases of position-independent code, linked unchanged.
    pub zero_base: bool,
}
#[derive(Debug)]
pub(super) struct Segment {
    /// Address of the segment in the library's own layout.
    address: i64,
    pub data: &'static [u8],
    /// Required alignment in bytes (`WASM_SEGMENT_INFO`).
    alignment: usize,
    relocations: Vec<Relocation>,
    unlinkable: bool,
}

/// The parsed library: one memory, function imports from [`LINK_MODULE`]
/// only, no table use, active data segments, complete relocation metadata.
#[derive(Debug)]
pub(super) struct Library {
    pub types: Vec<FuncType>,
    pub imports: Vec<Import>,
    pub functions: Vec<Function>,
    pub globals: Vec<Global>,
    pub exports: BTreeMap<&'static str, u32>,
    pub segments: Vec<Segment>,
    /// The global the library's code uses as its shadow-stack pointer.
    pub stack_pointer: u32,
}

/// What one program links: library function, global and segment indices.
#[derive(Debug, Default)]
pub(super) struct Closure {
    /// Absolute library function indices (imports first), ascending.
    pub functions: BTreeSet<u32>,
    pub globals: BTreeSet<u32>,
    pub segments: BTreeSet<u32>,
}

/// A closure placed in an intrinsic region starting at `base`: the shadow
/// stack fills `[base, base + abi::INTRINSIC_STACK_BYTES)` and the closure's
/// data segments follow it in library order, each at the next offset of its
/// own alignment.
#[derive(Debug)]
pub(super) struct Placement {
    pub base: usize,
    /// Library segment -> its address in memory 0.
    pub addresses: BTreeMap<u32, usize>,
    /// Start of the reserved arena of `abi::INTRINSIC_ARENA_BYTES`.
    pub arena: usize,
    /// Bytes of the region the stack, data and reserved arena use.
    pub bytes: usize,
}
/// Alignment of the reserved arena's start: the largest a Rust allocation on
/// wasm32 requests from the bump arena in practice; the arena aligns each
/// allocation itself as well.
const ARENA_ALIGNMENT: usize = 16;
impl Placement {
    /// The initial (highest) stack pointer.
    pub(super) fn stack_top(&self) -> usize {
        self.base + abi::INTRINSIC_STACK_BYTES
    }
    /// The end of the reserved arena.
    pub(super) fn arena_end(&self) -> usize {
        self.arena + abi::INTRINSIC_ARENA_BYTES
    }
    /// Whole Wasm pages of the region.
    pub(super) fn pages(&self) -> Result<u32, String> {
        u32::try_from(self.bytes.div_ceil(abi::WASM_PAGE_BYTES))
            .map_err(|_| invalid("intrinsic region exceeds the address space"))
    }
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

/// Relocation and symbol metadata gathered while parsing.
#[derive(Default)]
struct Metadata {
    code_start: usize,
    data_start: usize,
    /// Section index (in file order) of the code and data sections.
    code_section: Option<u32>,
    data_section: Option<u32>,
    /// Data symbol index -> its target (segment and offset, or absolute).
    data_symbols: BTreeMap<u32, (Option<u32>, u64)>,
    /// Global index -> name of each named global symbol.
    global_names: BTreeMap<u32, &'static str>,
    /// Segment alignments (log2), in segment order.
    alignments: Vec<u32>,
    /// (section index, absolute file offset, type, symbol, addend).
    memory: Vec<(u32, usize, u8, u32, i64)>,
    seen: bool,
    /// Value of the absolute `__memory_base` symbol.
    memory_base: Option<u64>,
}

impl Library {
    fn parse(bytes: &'static [u8]) -> Result<Self, String> {
        let mut types = Vec::new();
        let mut imports = Vec::new();
        let mut declared = Vec::new();
        let mut bodies = Vec::new();
        let mut globals = Vec::new();
        let mut memories = 0;
        let mut exports = BTreeMap::new();
        let mut segments = Vec::new();
        let mut segment_ranges = Vec::new();
        let mut metadata = Metadata::default();
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
                Payload::MemorySection(reader) => memories += reader.count(),
                Payload::GlobalSection(reader) => {
                    for global in reader {
                        let global = global.map_err(invalid)?;
                        globals.push(Global {
                            ty: global.ty,
                            zero_base: zero_base(&global),
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
                    metadata.code_start = range.start;
                    metadata.code_section = Some(section);
                }
                Payload::CodeSectionEntry(body) => bodies.push(body),
                Payload::DataSection(reader) => {
                    metadata.data_start = reader.range().start;
                    metadata.data_section = Some(section);
                    for data in reader {
                        let data = data.map_err(invalid)?;
                        let DataKind::Active {
                            memory_index: 0,
                            offset_expr,
                        } = data.kind
                        else {
                            return Err(invalid("only active memory 0 segments are linkable"));
                        };
                        // The segment's bytes end its entry.
                        let start = data.range.end - data.data.len();
                        segment_ranges.push(start..data.range.end);
                        segments.push(Segment {
                            address: constant_address(&offset_expr)?,
                            data: data.data,
                            alignment: 1,
                            relocations: Vec::new(),
                            unlinkable: false,
                        });
                    }
                }
                Payload::CustomSection(custom) if custom.name() == "linking" => {
                    metadata.seen = true;
                    read_linking(custom.data(), &mut metadata)?;
                }
                Payload::CustomSection(custom) if custom.name().starts_with("reloc.") => {
                    read_relocations(custom.data(), custom.data_offset(), &mut metadata)?;
                }
                _ => {}
            }
            if counted {
                section += 1;
            }
        }
        if memories != 1 {
            return Err(invalid("expected exactly one memory"));
        }
        if !metadata.seen {
            return Err(invalid("relocation metadata is missing (`--emit-relocs`)"));
        }
        let relative = metadata
            .memory
            .iter()
            .any(|&(_, _, kind, _, _)| kind == R_MEMORY_ADDR_REL_SLEB);
        if relative && metadata.memory_base != Some(0) {
            return Err(invalid(
                "relative addresses need a library memory base of 0",
            ));
        }
        if declared.len() != bodies.len() {
            return Err(invalid("function and code sections disagree"));
        }
        if metadata.alignments.len() != segments.len() {
            return Err(invalid("segment info and data section disagree"));
        }
        for (segment, &alignment) in segments.iter_mut().zip(&metadata.alignments) {
            segment.alignment = 1_usize
                .checked_shl(alignment)
                .filter(|bytes| *bytes <= abi::WASM_PAGE_BYTES)
                .ok_or_else(|| invalid("segment alignment exceeds a page"))?;
        }
        let mut functions = Vec::with_capacity(bodies.len());
        for (ty, body) in declared.into_iter().zip(bodies) {
            functions.push(scan(ty, body)?);
        }
        let stack_pointer = metadata
            .global_names
            .iter()
            .find(|(_, name)| **name == STACK_POINTER)
            .map(|(index, _)| *index)
            .ok_or_else(|| invalid("no stack pointer symbol"))?;
        let mut library = Self {
            types,
            imports,
            functions,
            globals,
            exports,
            segments,
            stack_pointer,
        };
        library.attach(&metadata, &segment_ranges)?;
        Ok(library)
    }

    /// Attribute memory relocations to the function or segment they patch.
    fn attach(
        &mut self,
        metadata: &Metadata,
        segment_ranges: &[std::ops::Range<usize>],
    ) -> Result<(), String> {
        for &(section, at, kind, symbol, addend) in &metadata.memory {
            let encoding = match kind {
                R_MEMORY_ADDR_LEB => Some(Encoding::Leb),
                R_MEMORY_ADDR_SLEB | R_MEMORY_ADDR_REL_SLEB => Some(Encoding::Sleb),
                R_MEMORY_ADDR_I32 => Some(Encoding::I32),
                _ => None,
            };
            let target = match metadata.data_symbols.get(&symbol) {
                Some(&(Some(segment), offset)) => Target::Data {
                    segment,
                    offset: i64::try_from(offset).map_err(invalid)? + addend,
                },
                Some(&(None, _)) => Target::Absolute,
                None => return Err(invalid(format!("relocation of unknown symbol {symbol}"))),
            };
            if Some(section) == metadata.code_section {
                let at = metadata.code_start + at;
                let function = self
                    .functions
                    .iter_mut()
                    .find(|f| f.body.range().contains(&at))
                    .ok_or_else(|| invalid("code relocation outside every body"))?;
                match encoding {
                    Some(encoding) => function.relocations.push(Relocation {
                        encoding,
                        at: at - function.body.range().start,
                        target,
                    }),
                    None => function.unlinkable = true,
                }
            } else if Some(section) == metadata.data_section {
                let at = metadata.data_start + at;
                let index = segment_ranges
                    .iter()
                    .position(|r| r.contains(&at))
                    .ok_or_else(|| invalid("data relocation outside every segment"))?;
                let segment = &mut self.segments[index];
                match encoding {
                    Some(encoding) => segment.relocations.push(Relocation {
                        encoding,
                        at: at - segment_ranges[index].start,
                        target,
                    }),
                    None => segment.unlinkable = true,
                }
            }
        }
        Ok(())
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

    /// Every function, global and data segment reachable from `roots`. The
    /// only global linked code may use is its stack pointer, and no reached
    /// function or segment may address a linker-synthesized absolute symbol.
    pub(super) fn closure(&self, roots: &[u32]) -> Result<Closure, String> {
        let mut closure = Closure::default();
        let mut pending = roots.to_vec();
        let mut segments = Vec::new();
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
            if function.unlinkable {
                return Err(invalid(format!(
                    "function {index} uses a table, an indirect call or an unsupported relocation"
                )));
            }
            pending.extend(function.calls.iter().copied());
            closure.globals.extend(function.globals.iter().copied());
            segments.extend(targets(&function.relocations)?);
        }
        if closure.globals.iter().any(|&g| {
            g != self.stack_pointer && !self.globals.get(g as usize).is_some_and(|g| g.zero_base)
        }) {
            return Err(invalid(
                "linked code may use no global but its stack pointer and zero bases",
            ));
        }
        while let Some(segment) = segments.pop() {
            if !closure.segments.insert(segment) {
                continue;
            }
            let segment = self
                .segments
                .get(segment as usize)
                .ok_or_else(|| invalid(format!("unknown data segment {segment}")))?;
            if segment.unlinkable {
                return Err(invalid("a data segment holds an unsupported relocation"));
            }
            segments.extend(targets(&segment.relocations)?);
        }
        Ok(closure)
    }

    /// Place `closure` in an intrinsic region starting at `base` (a page
    /// boundary, so every segment alignment up to a page is kept).
    pub(super) fn place(&self, closure: &Closure, base: usize) -> Result<Placement, String> {
        if !base.is_multiple_of(abi::WASM_PAGE_BYTES) {
            return Err(invalid("intrinsic region must start on a page"));
        }
        let mut cursor = abi::INTRINSIC_STACK_BYTES;
        let mut addresses = BTreeMap::new();
        for &index in &closure.segments {
            let segment = &self.segments[index as usize];
            cursor = cursor
                .checked_next_multiple_of(segment.alignment)
                .and_then(|start| {
                    addresses.insert(index, base.checked_add(start)?);
                    start.checked_add(segment.data.len())
                })
                .ok_or_else(|| invalid("intrinsic region exceeds the address space"))?;
        }
        // The reserved arena follows the data, aligned for any allocation.
        let arena = cursor
            .checked_next_multiple_of(ARENA_ALIGNMENT)
            .ok_or_else(|| invalid("intrinsic region exceeds the address space"))?;
        Ok(Placement {
            base,
            addresses,
            arena: base + arena,
            bytes: arena + abi::INTRINSIC_ARENA_BYTES,
        })
    }

    /// The bytes of `function`'s body with every memory address moved to
    /// `placement`.
    pub(super) fn relocated_body(
        &self,
        function: &Function,
        placement: &Placement,
    ) -> Result<Vec<u8>, String> {
        let range = function.body.range();
        let mut bytes = LIBRARY
            .get(range)
            .ok_or_else(|| invalid("body outside the library"))?
            .to_vec();
        self.relocate(&mut bytes, &function.relocations, placement)?;
        Ok(bytes)
    }
    /// The bytes of `segment` with every stored address moved to `placement`.
    pub(super) fn relocated_segment(
        &self,
        segment: &Segment,
        placement: &Placement,
    ) -> Result<Vec<u8>, String> {
        let mut bytes = segment.data.to_vec();
        self.relocate(&mut bytes, &segment.relocations, placement)?;
        Ok(bytes)
    }

    /// Rewrite each relocated address from its library address to its
    /// placed one. The encoded value must be exactly the library address the
    /// relocation names, so a misread relocation fails instead of patching
    /// unrelated bytes.
    fn relocate(
        &self,
        bytes: &mut [u8],
        relocations: &[Relocation],
        placement: &Placement,
    ) -> Result<(), String> {
        for relocation in relocations {
            let Target::Data { segment, offset } = relocation.target else {
                return Err(invalid("an absolute address is not linkable"));
            };
            let original = self.segments[segment as usize].address + offset;
            let placed = placement
                .addresses
                .get(&segment)
                .and_then(|&address| i64::try_from(address).ok())
                .ok_or_else(|| invalid("relocation into an unplaced segment"))?
                + offset;
            let width = match relocation.encoding {
                Encoding::Leb | Encoding::Sleb => PADDED_LEB_BYTES,
                Encoding::I32 => I32_BYTES,
            };
            let field = relocation
                .at
                .checked_add(width)
                .and_then(|end| bytes.get_mut(relocation.at..end))
                .ok_or_else(|| invalid("relocation outside its bytes"))?;
            if read_field(field, relocation.encoding) != Some(original) {
                return Err(invalid("relocated value does not match its symbol"));
            }
            let placed = u32::try_from(placed)
                .ok()
                .filter(|value| relocation.encoding != Encoding::Sleb || *value <= i32::MAX as u32)
                .ok_or_else(|| invalid("placed address exceeds the memory32 range"))?;
            write_field(field, relocation.encoding, placed);
        }
        Ok(())
    }
}

/// The segments a list of relocations points into.
fn targets(relocations: &[Relocation]) -> Result<Vec<u32>, String> {
    relocations
        .iter()
        .map(|relocation| match relocation.target {
            Target::Data { segment, .. } => Ok(segment),
            Target::Absolute => Err(invalid("an absolute address is not linkable")),
        })
        .collect()
}

/// The value of a padded LEB/SLEB or little-endian i32 field.
fn read_field(field: &[u8], encoding: Encoding) -> Option<i64> {
    if encoding == Encoding::I32 {
        return Some(i64::from(u32::from_le_bytes(field.try_into().ok()?)));
    }
    let mut value = 0_u64;
    for (index, &byte) in field.iter().enumerate() {
        let last = index + 1 == field.len();
        if (byte & LEB_CONTINUATION != 0) == last {
            return None;
        }
        value |= (u64::from(byte) & LEB_PAYLOAD_MASK) << (LEB_PAYLOAD_BITS * index as u32);
    }
    let bits = LEB_PAYLOAD_BITS * field.len() as u32;
    Some(
        if encoding == Encoding::Sleb && value >> (bits - 1) & 1 == 1 {
            value as i64 - (1_i64 << bits)
        } else {
            value as i64
        },
    )
}

/// Encode `value` into a field of the same width and encoding.
fn write_field(field: &mut [u8], encoding: Encoding, value: u32) {
    if encoding == Encoding::I32 {
        field.copy_from_slice(&value.to_le_bytes());
        return;
    }
    let count = field.len();
    for (index, byte) in field.iter_mut().enumerate() {
        let payload = (u64::from(value) >> (LEB_PAYLOAD_BITS * index as u32)) & LEB_PAYLOAD_MASK;
        *byte = payload as u8
            | if index + 1 < count {
                LEB_CONTINUATION
            } else {
                0
            };
    }
}

/// Whether `global` is an immutable i32 global initialized to 0.
fn zero_base(global: &wasmparser::Global<'_>) -> bool {
    let mut reader = global.init_expr.get_operators_reader();
    !global.ty.mutable
        && global.ty.content_type == wasmparser::ValType::I32
        && matches!(
            (reader.read(), reader.read()),
            (Ok(Operator::I32Const { value: 0 }), Ok(Operator::End))
        )
        && reader.eof()
}

/// The address of a segment placed by an `i32.const` offset.
fn constant_address(offset: &ConstExpr<'_>) -> Result<i64, String> {
    let mut reader = offset.get_operators_reader();
    match (reader.read(), reader.read()) {
        (Ok(Operator::I32Const { value }), Ok(Operator::End)) if reader.eof() => {
            Ok(i64::from(value as u32))
        }
        _ => Err(invalid("segment offsets must be i32 constants")),
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
    let mut unlinkable = false;
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
            | Operator::DataDrop { .. } => unlinkable = true,
            _ => {}
        }
    }
    Ok(Function {
        ty,
        body,
        calls,
        globals,
        relocations: Vec::new(),
        unlinkable,
    })
}

fn string(reader: &mut BinaryReader<'static>) -> Result<&'static str, String> {
    reader.read_string().map_err(invalid)
}

/// Segment alignments and the data and global symbols of the `linking`
/// section.
fn read_linking(data: &'static [u8], metadata: &mut Metadata) -> Result<(), String> {
    let mut reader = BinaryReader::new(data, 0);
    if reader.read_var_u32().map_err(invalid)? != LINKING_VERSION {
        return Err(invalid("unsupported linking metadata version"));
    }
    while !reader.eof() {
        let kind = reader.read_u8().map_err(invalid)?;
        let size = reader.read_var_u32().map_err(invalid)? as usize;
        let payload = reader.read_bytes(size).map_err(invalid)?;
        let mut table = BinaryReader::new(payload, 0);
        match kind {
            SUBSECTION_SEGMENT_INFO => {
                for _ in 0..table.read_var_u32().map_err(invalid)? {
                    string(&mut table)?;
                    metadata
                        .alignments
                        .push(table.read_var_u32().map_err(invalid)?);
                    table.read_var_u32().map_err(invalid)?;
                }
            }
            SUBSECTION_SYMBOL_TABLE => read_symbols(&mut table, metadata)?,
            _ => {}
        }
    }
    Ok(())
}

fn read_symbols(table: &mut BinaryReader<'static>, metadata: &mut Metadata) -> Result<(), String> {
    let count = table.read_var_u32().map_err(invalid)?;
    for index in 0..count {
        let kind = table.read_u8().map_err(invalid)?;
        let flags = table.read_var_u32().map_err(invalid)?;
        let defined = flags & SYMBOL_UNDEFINED == 0;
        match kind {
            SYMBOL_FUNCTION | SYMBOL_GLOBAL | SYMBOL_TAG | SYMBOL_TABLE => {
                let item = table.read_var_u32().map_err(invalid)?;
                if defined || flags & SYMBOL_EXPLICIT_NAME != 0 {
                    let name = string(table)?;
                    if kind == SYMBOL_GLOBAL {
                        metadata.global_names.insert(item, name);
                    }
                }
            }
            SYMBOL_DATA => {
                let name = string(table)?;
                if defined {
                    let segment = table.read_var_u32().map_err(invalid)?;
                    let offset = table.read_var_u64().map_err(invalid)?;
                    table.read_var_u64().map_err(invalid)?;
                    let absolute = flags & SYMBOL_ABSOLUTE != 0;
                    if absolute && name == MEMORY_BASE {
                        metadata.memory_base = Some(offset);
                    }
                    let segment = (!absolute).then_some(segment);
                    metadata.data_symbols.insert(index, (segment, offset));
                }
            }
            SYMBOL_SECTION => {
                table.read_var_u32().map_err(invalid)?;
            }
            _ => return Err(invalid(format!("unknown symbol kind {kind}"))),
        }
    }
    Ok(())
}

/// Memory relocations of one `reloc.*` section, as absolute file offsets
/// relative to their target section's payload.
fn read_relocations(data: &[u8], offset: usize, metadata: &mut Metadata) -> Result<(), String> {
    let mut reader = BinaryReader::new(data, offset);
    let section = reader.read_var_u32().map_err(invalid)?;
    let count = reader.read_var_u32().map_err(invalid)?;
    for _ in 0..count {
        let kind = reader.read_u8().map_err(invalid)?;
        let at = reader.read_var_u32().map_err(invalid)? as usize;
        let symbol = reader.read_var_u32().map_err(invalid)?;
        let addend = if ADDEND_RELOCATIONS.contains(&kind) {
            reader.read_var_i64().map_err(invalid)?
        } else {
            0
        };
        if matches!(
            kind,
            R_MEMORY_ADDR_LEB | R_MEMORY_ADDR_SLEB | R_MEMORY_ADDR_REL_SLEB | R_MEMORY_ADDR_I32
        ) || UNSUPPORTED_MEMORY_RELOCATIONS.contains(&kind)
        {
            metadata.memory.push((section, at, kind, symbol, addend));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padded_fields_round_trip_and_reject_unpadded_encodings() {
        let mut leb = [0_u8; PADDED_LEB_BYTES];
        write_field(&mut leb, Encoding::Leb, 1_179_648);
        assert_eq!(read_field(&leb, Encoding::Leb), Some(1_179_648));
        assert!(leb[..4].iter().all(|b| b & LEB_CONTINUATION != 0) && leb[4] < 0x80);
        let mut sleb = leb;
        write_field(&mut sleb, Encoding::Sleb, i32::MAX as u32);
        assert_eq!(read_field(&sleb, Encoding::Sleb), Some(i64::from(i32::MAX)));
        // A negative SLEB is sign-extended; unpadded or unterminated
        // encodings are rejected.
        assert_eq!(
            read_field(&[0xff, 0xff, 0xff, 0xff, 0x7f], Encoding::Sleb),
            Some(-1)
        );
        assert_eq!(read_field(&[0xff; 5], Encoding::Leb), None);
        assert_eq!(read_field(&[0x01, 0, 0, 0, 0], Encoding::Leb), None);
        let mut word = [0_u8; I32_BYTES];
        write_field(&mut word, Encoding::I32, 0xdead_beef);
        assert_eq!(read_field(&word, Encoding::I32), Some(0xdead_beef));
    }

    #[test]
    fn placement_packs_reached_segments_after_the_stack_with_their_alignment() {
        let library = library().unwrap();
        let roots: Vec<u32> = ["number_to_fixed", "string_to_upper_case16"]
            .iter()
            .map(|name| library.export(name).unwrap())
            .collect();
        let closure = library.closure(&roots).unwrap();
        assert!(!closure.segments.is_empty());
        let base = abi::OWNED_STRING_BASE;
        let placement = library.place(&closure, base).unwrap();
        let mut end = placement.stack_top();
        for (&segment, &address) in &placement.addresses {
            let segment = &library.segments[segment as usize];
            assert!(address >= end, "segments overlap the stack or each other");
            assert!(address.is_multiple_of(segment.alignment));
            end = address + segment.data.len();
        }
        // The reserved arena follows the data and ends the region.
        assert!(placement.arena >= end && placement.arena.is_multiple_of(ARENA_ALIGNMENT));
        assert_eq!(placement.arena_end(), base + placement.bytes);
        // Every relocated body still parses, and its addresses moved.
        for &index in &closure.functions {
            if let Some(function) = library.defined(index) {
                let body = library.relocated_body(function, &placement).unwrap();
                FunctionBody::new(BinaryReader::new(&body, 0))
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .for_each(|op| {
                        op.unwrap();
                    });
            }
        }
        // A misread relocation fails instead of patching other bytes.
        let function = closure
            .functions
            .iter()
            .filter_map(|&index| library.defined(index))
            .find(|f| !f.relocations.is_empty())
            .unwrap();
        let mut bytes = LIBRARY[function.body.range()].to_vec();
        let at = function.relocations[0].at;
        bytes[at] ^= 1;
        assert!(
            library
                .relocate(&mut bytes, &function.relocations, &placement)
                .is_err()
        );
    }
}
