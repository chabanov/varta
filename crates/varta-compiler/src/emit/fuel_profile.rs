//! Fuel-profile instrumentation of an emitted module (`varta dev
//! --profile-fuel`, [`varta_common::profile`]).
//!
//! The probe imports follow the host imports, so host import indices are
//! unchanged and every defined function index moves up by the probe count.
//! Each function body starts with `i32.const id; call enter` and runs
//! `i32.const id; call exit` before every `return` and before its final
//! `end`, where `id` is the function's code-section position. Probes leave
//! the value stack and every local unchanged; a trap simply leaves the
//! activation open. Only profile builds are instrumented.
//!
//! The frame dispatcher (`varta_resume`) stays uninstrumented: frame
//! admission requires that it calls only defined functions, and the host
//! calls it between guest activations, so its fuel counts as outside guest
//! functions.
//!
//! The host-free ABI exports (`varta_common::HOST_FREE_EXPORTS`, the value,
//! tool and HTTP copy and retain helpers) and every function they reach
//! through direct calls stay uninstrumented too: validation rejects any host
//! import, probes included, in that call graph. Their fuel counts toward the
//! innermost open activation, the caller of the allocator or of the host
//! operation that called the export, so the profile still sums to the
//! consumed fuel.
use crate::CompilerError;
use std::collections::BTreeSet;
use varta_common::profile::{self, Probe};
use wasm_encoder::reencode::{Error, Reencode, utils};
use wasm_encoder::{Instruction as I, ValType};

pub(crate) fn instrument(module: &[u8], names: &[String]) -> Result<Vec<u8>, CompilerError> {
    let mut instrument = Instrument {
        imports: 0,
        probe_type: 0,
        next: 0,
        dispatcher: None,
        host_free: host_free(module)?,
    };
    let mut output = wasm_encoder::Module::new();
    instrument
        .parse_core_module(&mut output, wasmparser::Parser::new(0), module)
        .map_err(|e| CompilerError::Internal(format!("fuel profile instrumentation: {e}")))?;
    if instrument.next as usize != names.len() {
        return Err(CompilerError::Internal(
            "fuel profile names do not match the code section".into(),
        ));
    }
    output.section(&wasm_encoder::CustomSection {
        name: profile::NAMES_SECTION.into(),
        data: profile::encode_names(names).into(),
    });
    Ok(output.finish())
}

struct Instrument {
    /// Host imports of the original module.
    imports: u32,
    probe_type: u32,
    /// Code-section position of the next body.
    next: u32,
    /// Code-section position of the frame dispatcher, if exported.
    dispatcher: Option<u32>,
    /// Code-section positions reachable from the host-free ABI exports.
    host_free: BTreeSet<u32>,
}

/// Code-section positions of the functions the host-free ABI exports reach
/// through direct calls (the call graph runtime validation checks).
fn host_free(module: &[u8]) -> Result<BTreeSet<u32>, CompilerError> {
    let malformed = |e: wasmparser::BinaryReaderError| {
        CompilerError::Internal(format!("fuel profile instrumentation: {e}"))
    };
    let mut imports = 0;
    let mut roots = Vec::new();
    let mut calls: Vec<Vec<u32>> = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(module) {
        match payload.map_err(malformed)? {
            wasmparser::Payload::ImportSection(section) => imports = section.count(),
            wasmparser::Payload::ExportSection(section) => {
                for export in section {
                    let export = export.map_err(malformed)?;
                    if export.kind == wasmparser::ExternalKind::Func
                        && varta_common::HOST_FREE_EXPORTS.contains(&export.name)
                    {
                        roots.push(export.index);
                    }
                }
            }
            wasmparser::Payload::CodeSectionEntry(body) => {
                let mut targets = Vec::new();
                let mut reader = body.get_operators_reader().map_err(malformed)?;
                while !reader.eof() {
                    if let wasmparser::Operator::Call { function_index }
                    | wasmparser::Operator::ReturnCall { function_index } =
                        reader.read().map_err(malformed)?
                    {
                        targets.push(function_index);
                    }
                }
                calls.push(targets);
            }
            _ => {}
        }
    }
    let mut reached = BTreeSet::new();
    let mut pending: Vec<u32> = roots
        .into_iter()
        .filter_map(|index| index.checked_sub(imports))
        .collect();
    while let Some(position) = pending.pop() {
        if !reached.insert(position) {
            continue;
        }
        let targets = calls.get(position as usize).ok_or_else(|| {
            CompilerError::Internal("fuel profile instrumentation: export outside the code".into())
        })?;
        pending.extend(
            targets
                .iter()
                .filter_map(|index| index.checked_sub(imports))
                .filter(|position| !reached.contains(position)),
        );
    }
    Ok(reached)
}
impl Instrument {
    fn probe(&self, function: &mut wasm_encoder::Function, id: u32, probe: Probe) {
        function.instruction(&I::I32Const(id as i32));
        function.instruction(&I::Call(self.imports + probe.offset()));
    }
}
impl Reencode for Instrument {
    type Error = std::convert::Infallible;

    fn function_index(&mut self, func: u32) -> Result<u32, Error<Self::Error>> {
        Ok(if func >= self.imports {
            func + Probe::ALL.len() as u32
        } else {
            func
        })
    }
    fn parse_type_section(
        &mut self,
        types: &mut wasm_encoder::TypeSection,
        section: wasmparser::TypeSectionReader<'_>,
    ) -> Result<(), Error<Self::Error>> {
        utils::parse_type_section(self, types, section)?;
        self.probe_type = types.len();
        types.ty().function([ValType::I32], []);
        Ok(())
    }
    fn parse_import_section(
        &mut self,
        imports: &mut wasm_encoder::ImportSection,
        section: wasmparser::ImportSectionReader<'_>,
    ) -> Result<(), Error<Self::Error>> {
        self.imports = section.count();
        utils::parse_import_section(self, imports, section)?;
        for probe in Probe::ALL {
            imports.import(
                profile::MODULE,
                probe.name(),
                wasm_encoder::EntityType::Function(self.probe_type),
            );
        }
        Ok(())
    }
    fn parse_export_section(
        &mut self,
        exports: &mut wasm_encoder::ExportSection,
        section: wasmparser::ExportSectionReader<'_>,
    ) -> Result<(), Error<Self::Error>> {
        for export in section.clone() {
            let export = export?;
            if export.name == varta_common::function::frame::RESUME_EXPORT
                && export.kind == wasmparser::ExternalKind::Func
            {
                self.dispatcher = export.index.checked_sub(self.imports);
            }
        }
        utils::parse_export_section(self, exports, section)
    }
    fn parse_function_body(
        &mut self,
        code: &mut wasm_encoder::CodeSection,
        func: wasmparser::FunctionBody<'_>,
    ) -> Result<(), Error<Self::Error>> {
        let id = self.next;
        self.next += 1;
        if self.dispatcher == Some(id) || self.host_free.contains(&id) {
            let mut function = self.new_function_with_parsed_locals(&func)?;
            let mut reader = func.get_operators_reader()?;
            while !reader.eof() {
                function.instruction(&self.instruction(reader.read()?)?);
            }
            code.function(&function);
            return Ok(());
        }
        let mut function = self.new_function_with_parsed_locals(&func)?;
        self.probe(&mut function, id, Probe::Enter);
        let mut reader = func.get_operators_reader()?;
        let mut depth = 0u32;
        while !reader.eof() {
            let operator = reader.read()?;
            match operator {
                wasmparser::Operator::Block { .. }
                | wasmparser::Operator::Loop { .. }
                | wasmparser::Operator::If { .. } => depth += 1,
                wasmparser::Operator::End if depth == 0 => {
                    self.probe(&mut function, id, Probe::Exit);
                }
                wasmparser::Operator::End => depth -= 1,
                wasmparser::Operator::Return => self.probe(&mut function, id, Probe::Exit),
                _ => {}
            }
            function.instruction(&self.instruction(operator)?);
        }
        code.function(&function);
        Ok(())
    }
}
