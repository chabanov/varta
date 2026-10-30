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
use crate::CompilerError;
use varta_common::profile::{self, Probe};
use wasm_encoder::reencode::{Error, Reencode, utils};
use wasm_encoder::{Instruction as I, ValType};

pub(crate) fn instrument(module: &[u8], names: &[String]) -> Result<Vec<u8>, CompilerError> {
    let mut instrument = Instrument {
        imports: 0,
        probe_type: 0,
        next: 0,
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
    fn parse_function_body(
        &mut self,
        code: &mut wasm_encoder::CodeSection,
        func: wasmparser::FunctionBody<'_>,
    ) -> Result<(), Error<Self::Error>> {
        let mut function = self.new_function_with_parsed_locals(&func)?;
        let id = self.next;
        self.next += 1;
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
