//! Trusted browser-execution transform, separate from portable artifacts.
//! Native artifacts stay unchanged. A host must validate the original module
//! and its permissions before this transform; transformed bytes are private to
//! that host and must never be treated as a new portable ABI.
//!
//! The model follows locked Wasmtime 48's default Cranelift fuel: one entry
//! unit, buffered operator costs, a cached per-function counter, save/reload
//! around calls, loop checks and per-byte bulk-memory work. Instrumentation
//! itself is outside the modeled guest cost.
use crate::CompilerError;
use std::collections::VecDeque;
use varta_common::limits;
use wasm_encoder::reencode::{Error, Reencode, utils};
use wasm_encoder::{BlockType, EntityType, GlobalType, Instruction as I, SectionId, ValType};
use wasmparser::{Operator as O, Payload, ValidPayload};
mod flow;

/// Private import supplied by the trusted browser host, never by guest input.
pub use varta_common::browser::{
    METER_COUNTER as COUNTER, METER_MODULE as MODULE, METER_SECTION as SECTION,
    METER_VERSION as VERSION,
};
const COUNTER_GLOBAL: u32 = 0;
const CHEAP_BULK_UNITS: i64 = 128;

/// Derive a metered browser execution image. The import is a mutable i64 global
/// initialized to the negative remaining fuel. Hosts clamp an exhausted counter
/// to zero when reporting, and use the same global for fair host-work charges.
/// All existing function indices and portable input bytes remain unchanged.
pub fn instrument(bytes: &[u8]) -> Result<Vec<u8>, CompilerError> {
    instrument_with_memory_limit(bytes, varta_common::abi::MAX_MEMORY_BYTES)
}

/// Apply the already resolved host memory ceiling to the private image. Native
/// uses a Store limiter; JS requires explicit maxima on every memory/table.
/// This never changes portable bytes or raises a declared maximum.
pub fn instrument_with_memory_limit(
    bytes: &[u8],
    memory_bytes: usize,
) -> Result<Vec<u8>, CompilerError> {
    if memory_bytes == 0 || memory_bytes as u64 > varta_common::budget::MAX_MEMORY_BYTES {
        return Err(unsupported("invalid host memory ceiling"));
    }
    if bytes.len() > limits::WASM_MODULE_BYTES {
        return Err(unsupported("module exceeds the Wasm byte limit"));
    }
    wasmparser::Validator::new()
        .validate_all(bytes)
        .map_err(invalid)?;
    let mut validator = wasmparser::Validator::new();
    let mut types = Vec::new();
    let mut functions = Vec::new();
    let mut checks = VecDeque::new();
    let mut memories = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(invalid)?;
        let validated = validator.payload(&payload).map_err(invalid)?;
        match payload {
            Payload::TypeSection(section) => {
                for ty in section.into_iter_err_on_gc_types() {
                    types.push(ty.map_err(invalid)?);
                }
            }
            Payload::FunctionSection(section) => {
                for ty in section {
                    functions.push(ty.map_err(invalid)?);
                }
            }
            Payload::ImportSection(section) => {
                for import in section {
                    let import = import.map_err(invalid)?;
                    if import.module == MODULE || !matches!(import.ty, wasmparser::TypeRef::Func(_))
                    {
                        return Err(unsupported("reserved meter or non-function import"));
                    }
                }
            }
            Payload::MemorySection(section) => {
                for memory in section {
                    let memory = memory.map_err(invalid)?;
                    if memory.memory64 || memory.shared || memory.page_size_log2.is_some() {
                        return Err(unsupported("shared, 64-bit or custom-page memory"));
                    }
                    memories.push(memory.maximum.map_or(u64::from(u32::MAX) + 1, |pages| {
                        pages * varta_common::abi::WASM_PAGE_BYTES as u64
                    }));
                }
            }
            Payload::StartSection { .. } => {
                return Err(unsupported("Wasm start function instantiation charges"));
            }
            Payload::CustomSection(section) if section.name() == SECTION => {
                return Err(unsupported("already instrumented module"));
            }
            _ => {}
        }
        if let ValidPayload::Func(check, _) = validated {
            let check = check.into_validator(Default::default());
            checks.push_back(check);
        }
    }
    if types.len() > limits::WASM_TYPES / 2 {
        return Err(unsupported("metered image exceeds the Wasm type limit"));
    }
    let mut meter = Meter {
        types,
        functions,
        checks,
        memories,
        memory_bytes,
        next: 0,
        imports: false,
    };
    let mut output = wasm_encoder::Module::new();
    meter
        .parse_core_module(&mut output, wasmparser::Parser::new(0), bytes)
        .map_err(|error| invalid(error.to_string()))?;
    output.section(&wasm_encoder::CustomSection {
        name: SECTION.into(),
        data: VERSION.to_le_bytes().to_vec().into(),
    });
    let output = output.finish();
    if output.len() > limits::WASM_MODULE_BYTES {
        return Err(unsupported("metered image exceeds the Wasm byte limit"));
    }
    wasmparser::Validator::new()
        .validate_all(&output)
        .map_err(invalid)?;
    Ok(output)
}
fn invalid(error: impl std::fmt::Display) -> CompilerError {
    CompilerError::Invalid(format!("browser fuel: {error}"))
}
fn unsupported(detail: impl Into<String>) -> CompilerError {
    CompilerError::Unsupported {
        kind: "browser fuel transform".into(),
        detail: detail.into(),
    }
}

struct Meter {
    types: Vec<wasmparser::FuncType>,
    functions: Vec<u32>,
    checks: VecDeque<wasmparser::FuncValidator<wasmparser::ValidatorResources>>,
    memories: Vec<u64>,
    memory_bytes: usize,
    next: usize,
    imports: bool,
}
impl Meter {
    fn import(&mut self, imports: &mut wasm_encoder::ImportSection) {
        imports.import(
            MODULE,
            COUNTER,
            EntityType::Global(GlobalType {
                val_type: ValType::I64,
                mutable: true,
                shared: false,
            }),
        );
        self.imports = true;
    }
}
struct Counter {
    local: u32,
    units: u32,
    pending: i64,
}
impl Counter {
    fn flush(&mut self, f: &mut wasm_encoder::Function) {
        if self.pending != 0 {
            f.instruction(&I::LocalGet(self.local));
            f.instruction(&I::I64Const(self.pending));
            f.instruction(&I::I64Add);
            f.instruction(&I::LocalSet(self.local));
            self.pending = 0;
        }
    }
    fn save(&self, f: &mut wasm_encoder::Function) {
        f.instruction(&I::LocalGet(self.local));
        f.instruction(&I::GlobalSet(COUNTER_GLOBAL));
    }
    fn load(&self, f: &mut wasm_encoder::Function) {
        f.instruction(&I::GlobalGet(COUNTER_GLOBAL));
        f.instruction(&I::LocalSet(self.local));
    }
    fn check(&mut self, f: &mut wasm_encoder::Function) {
        self.flush(f);
        f.instruction(&I::LocalGet(self.local));
        f.instruction(&I::I64Const(0));
        f.instruction(&I::I64GeS);
        f.instruction(&I::If(BlockType::Empty));
        self.save(f);
        f.instruction(&I::Unreachable);
        f.instruction(&I::End);
    }
    fn bulk(&mut self, f: &mut wasm_encoder::Function, constant: Option<u32>, rate: i64) {
        if let Some(units) = constant {
            self.pending += i64::from(units) * rate;
            if i64::from(units) * rate > CHEAP_BULK_UNITS {
                self.check(f);
            }
        } else {
            // Keep the size operand on the value stack for the original op.
            f.instruction(&I::LocalTee(self.units));
            self.flush(f);
            if rate != 0 {
                f.instruction(&I::LocalGet(self.local));
                f.instruction(&I::LocalGet(self.units));
                f.instruction(&I::I64ExtendI32U);
                f.instruction(&I::I64Add);
                f.instruction(&I::LocalSet(self.local));
            }
            self.check(f);
        }
    }
}
impl Reencode for Meter {
    type Error = CompilerError;
    fn memory_type(
        &mut self,
        ty: wasmparser::MemoryType,
    ) -> Result<wasm_encoder::MemoryType, Error<Self::Error>> {
        let mut memory = utils::memory_type(self, ty);
        let cap = self.memory_bytes as u64 / varta_common::abi::WASM_PAGE_BYTES as u64;
        if memory.minimum > cap {
            return Err(Error::UserError(unsupported(
                "initial memory exceeds the host ceiling",
            )));
        }
        memory.maximum = Some(memory.maximum.unwrap_or(cap).min(cap));
        Ok(memory)
    }
    fn table_type(
        &mut self,
        ty: wasmparser::TableType,
    ) -> Result<wasm_encoder::TableType, Error<Self::Error>> {
        if ty.table64 || ty.shared {
            return Err(Error::UserError(unsupported("shared or 64-bit table")));
        }
        let mut table = utils::table_type(self, ty)?;
        let cap = varta_common::abi::MAX_TABLE_ELEMENTS as u64;
        if table.minimum > cap {
            return Err(Error::UserError(unsupported(
                "initial table exceeds the host ceiling",
            )));
        }
        table.maximum = Some(table.maximum.unwrap_or(cap).min(cap));
        Ok(table)
    }
    fn global_index(&mut self, index: u32) -> Result<u32, Error<Self::Error>> {
        index
            .checked_add(1)
            .ok_or_else(|| Error::UserError(invalid("global index overflow")))
    }
    fn parse_type_section(
        &mut self,
        types: &mut wasm_encoder::TypeSection,
        section: wasmparser::TypeSectionReader<'_>,
    ) -> Result<(), Error<Self::Error>> {
        utils::parse_type_section(self, types, section)?;
        // The enclosing epilogue block replaces the original implicit function
        // label, so br/br_if/br_table to it also save the cached counter.
        for ty in self.types.clone() {
            let results = ty
                .results()
                .iter()
                .map(|ty| self.val_type(*ty))
                .collect::<Result<Vec<_>, _>>()?;
            types.ty().function([], results);
        }
        Ok(())
    }
    fn parse_import_section(
        &mut self,
        imports: &mut wasm_encoder::ImportSection,
        section: wasmparser::ImportSectionReader<'_>,
    ) -> Result<(), Error<Self::Error>> {
        utils::parse_import_section(self, imports, section)?;
        self.import(imports);
        Ok(())
    }
    fn intersperse_section_hook(
        &mut self,
        module: &mut wasm_encoder::Module,
        _after: Option<SectionId>,
        before: Option<SectionId>,
    ) -> Result<(), Error<Self::Error>> {
        if !self.imports && before.is_none_or(|id| id as u8 > SectionId::Import as u8) {
            let mut imports = wasm_encoder::ImportSection::new();
            self.import(&mut imports);
            module.section(&imports);
        }
        Ok(())
    }
    fn parse_function_body(
        &mut self,
        code: &mut wasm_encoder::CodeSection,
        body: wasmparser::FunctionBody<'_>,
    ) -> Result<(), Error<Self::Error>> {
        if body.range().len() > limits::WASM_FUNCTION_BODY_BYTES {
            return Err(Error::UserError(unsupported(
                "function exceeds the Wasm byte limit",
            )));
        }
        let type_index = self.functions[self.next];
        self.next += 1;
        let params = self.types[type_index as usize].params().len() as u32;
        let locals = body
            .get_locals_reader()?
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let count = locals
            .iter()
            .try_fold(params, |n, (count, _)| n.checked_add(*count))
            .ok_or_else(|| Error::UserError(invalid("local index overflow")))?;
        let mut declarations = locals
            .iter()
            .map(|(n, ty)| Ok((*n, self.val_type(*ty)?)))
            .collect::<Result<Vec<_>, Error<Self::Error>>>()?;
        declarations.extend([(1, ValType::I64), (1, ValType::I32)]);
        let mut f = wasm_encoder::Function::new(declarations);
        let mut counter = Counter {
            local: count,
            units: count + 1,
            pending: 1,
        };
        counter.load(&mut f);
        counter.check(&mut f);
        f.instruction(&I::Block(BlockType::FunctionType(
            self.types.len() as u32 + type_index,
        )));
        let mut check = self
            .checks
            .pop_front()
            .ok_or_else(|| Error::UserError(invalid("missing function validation")))?;
        check.read_locals(&mut body.get_binary_reader())?;
        let mut reader = body.get_operators_reader()?;
        let mut flow = flow::Flow::new(
            params as usize,
            &locals,
            self.types[type_index as usize].results().len(),
        );
        while !reader.eof() {
            let offset = reader.original_position();
            let op = reader.read()?;
            if !supported(&op) {
                return Err(Error::UserError(unsupported(
                    "operator outside the implemented fuel model",
                )));
            }
            flow.sync(check.operand_stack_height() as usize);
            let reachable = flow.reachable();
            let unconditional_trap = memory_access(&op).is_some_and(|(arg, size)| {
                arg.offset + u64::from(size) > self.memories[arg.memory as usize]
            });
            if reachable {
                counter.pending += cost(&op);
                if unconditional_trap {
                    counter.flush(&mut f);
                    counter.save(&mut f);
                }
                match op {
                    O::MemoryCopy { .. } | O::MemoryFill { .. } | O::MemoryInit { .. } => {
                        counter.bulk(&mut f, flow.top(), 1)
                    }
                    O::MemoryGrow { .. } => counter.bulk(&mut f, flow.top(), 0),
                    O::Call { .. }
                    | O::CallIndirect { .. }
                    | O::ReturnCall { .. }
                    | O::ReturnCallIndirect { .. }
                    | O::Return
                    | O::Unreachable => {
                        counter.flush(&mut f);
                        counter.save(&mut f);
                    }
                    O::Loop { .. }
                    | O::If { .. }
                    | O::Br { .. }
                    | O::BrIf { .. }
                    | O::BrTable { .. }
                    | O::Else
                    | O::End => counter.flush(&mut f),
                    _ => {}
                }
            }
            f.instruction(&self.instruction(op.clone())?);
            if reachable {
                match op {
                    O::Loop { .. } => counter.check(&mut f),
                    O::Call { .. } | O::CallIndirect { .. } => counter.load(&mut f),
                    _ => {}
                }
            }
            flow.op(&op, &check, &self.types);
            if unconditional_trap {
                flow.trap();
            }
            check.op(offset, &op)?;
        }
        counter.save(&mut f);
        f.instruction(&I::End);
        if f.byte_len() > limits::WASM_FUNCTION_BODY_BYTES {
            return Err(Error::UserError(unsupported(
                "metered function exceeds the Wasm byte limit",
            )));
        }
        code.function(&f);
        Ok(())
    }
}
// These widths are the actual bytes touched, including SIMD widening/lane
// operations. A statically impossible offset makes Cranelift end the block
// before the access, publishing buffered fuel even though ordinary traps do not.
fn memory_access(op: &O<'_>) -> Option<(wasmparser::MemArg, u8)> {
    let (arg, size) = match op {
        O::I32Load { memarg }
        | O::F32Load { memarg }
        | O::I32Store { memarg }
        | O::F32Store { memarg } => (*memarg, 4),
        O::I64Load { memarg }
        | O::F64Load { memarg }
        | O::I64Store { memarg }
        | O::F64Store { memarg } => (*memarg, 8),
        O::I32Load8S { memarg }
        | O::I32Load8U { memarg }
        | O::I64Load8S { memarg }
        | O::I64Load8U { memarg }
        | O::I32Store8 { memarg }
        | O::I64Store8 { memarg } => (*memarg, 1),
        O::I32Load16S { memarg }
        | O::I32Load16U { memarg }
        | O::I64Load16S { memarg }
        | O::I64Load16U { memarg }
        | O::I32Store16 { memarg }
        | O::I64Store16 { memarg } => (*memarg, 2),
        O::I64Load32S { memarg } | O::I64Load32U { memarg } | O::I64Store32 { memarg } => {
            (*memarg, 4)
        }
        O::V128Load { memarg } | O::V128Store { memarg } => (*memarg, 16),
        O::V128Load8x8S { memarg }
        | O::V128Load8x8U { memarg }
        | O::V128Load16x4S { memarg }
        | O::V128Load16x4U { memarg }
        | O::V128Load32x2S { memarg }
        | O::V128Load32x2U { memarg }
        | O::V128Load64Splat { memarg }
        | O::V128Load64Zero { memarg }
        | O::V128Load64Lane { memarg, .. }
        | O::V128Store64Lane { memarg, .. } => (*memarg, 8),
        O::V128Load32Splat { memarg }
        | O::V128Load32Zero { memarg }
        | O::V128Load32Lane { memarg, .. }
        | O::V128Store32Lane { memarg, .. } => (*memarg, 4),
        O::V128Load16Splat { memarg }
        | O::V128Load16Lane { memarg, .. }
        | O::V128Store16Lane { memarg, .. } => (*memarg, 2),
        O::V128Load8Splat { memarg }
        | O::V128Load8Lane { memarg, .. }
        | O::V128Store8Lane { memarg, .. } => (*memarg, 1),
        _ => return None,
    };
    Some((arg, size))
}
fn cost(op: &O<'_>) -> i64 {
    match op {
        O::Nop
        | O::Drop
        | O::Block { .. }
        | O::Loop { .. }
        | O::Unreachable
        | O::Return
        | O::Else
        | O::End => 0,
        _ => 1,
    }
}
fn supported(op: &O<'_>) -> bool {
    // Reject proposals with additional variable work or exception/control
    // rules until their native cost model is implemented, never leave them free.
    if matches!(
        op,
        O::TableCopy { .. } | O::TableFill { .. } | O::TableInit { .. } | O::TableGrow { .. }
    ) {
        return false;
    }
    macro_rules! proposal {
        ($(@$proposal:ident $name:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*) )*) => {
            match op { $(O::$name $({ $($arg: _),* })? => matches!(stringify!($proposal),"mvp"|"sign_extension"|"saturating_float_to_int"|"bulk_memory"|"multi_value"|"simd"|"relaxed_simd"|"tail_call"),)* _=>false }
        }
    }
    wasmparser::for_each_operator!(proposal)
}

#[cfg(test)]
mod tests;
