mod expr;
mod strings;
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
}
struct Emitter<'a> {
    code: Vec<I<'static>>,
    labels: Vec<Label>,
    hosts: &'a BTreeMap<Host, u32>,
    strings: &'a BTreeMap<String, u32>,
    functions_begin: u32,
    local_slots: Vec<u32>,
    temporary: u32,
    string_temporary: u32,
    snapshot: Option<u32>,
    result_len: u32,
    result_status: u32,
}

pub(crate) fn emit(program: &ir::Program) -> Result<Vec<u8>> {
    let mut hosts = BTreeSet::new();
    let mut strings = BTreeMap::new();
    for function in &program.functions {
        collect(&function.body, &function.source, &mut hosts, &mut strings);
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
            let (params, results) = h.signature();
            (
                *h,
                intern((vec![ValType::I32; params], vec![ValType::I32; results])),
            )
        })
        .collect();
    let function_types: Vec<_> = program
        .functions
        .iter()
        .map(|f| {
            intern((
                f.params.iter().copied().flat_map(wasm_types).collect(),
                wasm_types(f.result),
            ))
        })
        .collect();
    let needs_snapshot = hosts
        .keys()
        .any(|h| matches!(h, Host::EnvGet | Host::FsRead | Host::NetGet));
    let snapshot_type = needs_snapshot.then(|| intern((vec![ValType::I32; 2], vec![ValType::I64])));
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
    if let Some(index) = snapshot_type {
        functions.function(index);
    }
    module.section(&functions);
    let mut memory = wasm_encoder::MemorySection::new();
    memory.memory(wasm_encoder::MemoryType {
        minimum: 1,
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
    let arena = globals.len();
    if needs_snapshot {
        globals.global(
            wasm_encoder::GlobalType {
                val_type: ValType::I32,
                mutable: true,
                shared: false,
            },
            &wasm_encoder::ConstExpr::i32_const(varta_common::abi::OWNED_STRING_BASE as i32),
        );
    }
    module.section(&globals);
    let functions_begin = hosts.len() as u32;
    let snapshot = needs_snapshot.then_some(functions_begin + program.functions.len() as u32);
    let mut exports = wasm_encoder::ExportSection::new();
    exports.export("memory", wasm_encoder::ExportKind::Memory, 0);
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
    module.section(&exports);
    let mut code = wasm_encoder::CodeSection::new();
    for function in &program.functions {
        let mut temporary = 0;
        let local_slots = function
            .locals
            .iter()
            .map(|ty| {
                let slot = temporary;
                temporary += ty.width();
                slot
            })
            .collect();
        let mut emitter = Emitter {
            code: Vec::new(),
            labels: Vec::new(),
            hosts: &hosts,
            strings: &offsets,
            functions_begin,
            local_slots,
            temporary,
            string_temporary: temporary + 1,
            snapshot,
            result_len,
            result_status,
        };
        emitter
            .statements(&function.body)
            .map_err(|e| function.source.locate(e, function.span))?;
        let locals = function.locals[function.params.len()..]
            .iter()
            .copied()
            .flat_map(wasm_types)
            .chain([ValType::I32, ValType::I64]);
        let mut body = wasm_encoder::Function::new_with_locals_types(locals);
        for instruction in &emitter.code {
            body.instruction(instruction);
        }
        if function.result != Type::Void {
            body.instruction(&I::Unreachable);
        }
        body.instruction(&I::End);
        code.function(&body);
    }
    if needs_snapshot {
        code.function(&strings::snapshot(arena));
    }
    module.section(&code);
    let mut data = wasm_encoder::DataSection::new();
    for (string, offset) in offsets {
        data.active(
            0,
            &wasm_encoder::ConstExpr::i32_const(offset as i32),
            string.into_bytes(),
        );
    }
    module.section(&data);
    module.section(&wasm_encoder::CustomSection {
        name: "varta.abi".into(),
        data: varta_common::abi::VERSION.to_le_bytes().to_vec().into(),
    });
    Ok(module.finish())
}

fn wasm_types(ty: Type) -> Vec<ValType> {
    if let Some(value) = ty.result_value() {
        vec![
            if value == Type::String {
                ValType::I64
            } else {
                ValType::I32
            },
            ValType::I32,
        ]
    } else {
        match ty {
            Type::String => vec![ValType::I64],
            Type::Void => vec![],
            _ => vec![ValType::I32],
        }
    }
}

fn collect_expr<'a>(
    expr: &Expr,
    source: &'a crate::source::Source,
    hosts: &mut BTreeSet<Host>,
    strings: &mut BTreeMap<String, (swc_common::Span, &'a crate::source::Source)>,
) {
    match &expr.kind {
        E::String(s) => {
            strings.entry(s.clone()).or_insert((expr.span, source));
        }
        E::Host(host, args) => {
            hosts.insert(*host);
            for arg in args {
                collect_expr(arg, source, hosts, strings);
            }
        }
        E::Call(_, args) => {
            for arg in args {
                collect_expr(arg, source, hosts, strings);
            }
        }
        E::Binary(_, a, b) => {
            collect_expr(a, source, hosts, strings);
            collect_expr(b, source, hosts, strings);
        }
        E::Negative(a) | E::Not(a) | E::Capture(a) | E::ResultField(_, a) => {
            collect_expr(a, source, hosts, strings)
        }
        _ => {}
    }
}
fn collect<'a>(
    statements: &[S],
    source: &'a crate::source::Source,
    hosts: &mut BTreeSet<Host>,
    strings: &mut BTreeMap<String, (swc_common::Span, &'a crate::source::Source)>,
) {
    for statement in statements {
        match statement {
            S::Set(_, e) | S::Expr(e) | S::Return(Some(e)) => {
                collect_expr(e, source, hosts, strings)
            }
            S::Block(b) => collect(b, source, hosts, strings),
            S::If(e, a, b) => {
                collect_expr(e, source, hosts, strings);
                collect(a, source, hosts, strings);
                collect(b, source, hosts, strings);
            }
            S::While(e, b) => {
                collect_expr(e, source, hosts, strings);
                collect(b, source, hosts, strings);
            }
            S::For(a, test, update, b) => {
                collect(a, source, hosts, strings);
                if let Some(e) = test {
                    collect_expr(e, source, hosts, strings);
                }
                collect(update, source, hosts, strings);
                collect(b, source, hosts, strings);
            }
            S::Switch { value, cases, .. } => {
                collect_expr(value, source, hosts, strings);
                for (e, b) in cases {
                    if let Some(e) = e {
                        collect_expr(e, source, hosts, strings);
                    }
                    collect(b, source, hosts, strings);
                }
            }
            _ => {}
        }
    }
}

impl Emitter<'_> {
    fn push_label(&mut self, instruction: I<'static>, breakable: bool, continuable: bool) -> usize {
        self.code.push(instruction);
        let index = self.labels.len();
        self.labels.push(Label {
            breakable,
            continuable,
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
                S::Set(slot, value) => {
                    self.expr(value)?;
                    for offset in (0..value.ty.width()).rev() {
                        self.code
                            .push(I::LocalSet(self.local_slots[*slot as usize] + offset));
                    }
                }
                S::Expr(value) => {
                    self.expr(value)?;
                    for _ in 0..value.ty.width() {
                        self.code.push(I::Drop);
                    }
                }
                S::Return(value) => {
                    if let Some(value) = value {
                        self.expr(value)?;
                    }
                    self.code.push(I::Return);
                }
                S::Block(body) => self.statements(body)?,
                S::If(test, yes, no) => {
                    self.expr(test)?;
                    self.push_label(I::If(BlockType::Empty), false, false);
                    self.statements(yes)?;
                    if !no.is_empty() {
                        self.code.push(I::Else);
                        self.statements(no)?;
                    }
                    self.end();
                }
                S::While(test, body) => {
                    self.push_label(I::Block(BlockType::Empty), true, false);
                    let again = self.push_label(I::Loop(BlockType::Empty), false, true);
                    self.expr(test)?;
                    self.code.push(I::I32Eqz);
                    self.code.push(I::BrIf(self.target(false)?));
                    self.statements(body)?;
                    self.code.push(I::Br(self.depth(again)));
                    self.end();
                    self.end();
                }
                S::For(init, test, update, body) => {
                    self.statements(init)?;
                    self.push_label(I::Block(BlockType::Empty), true, false);
                    let again = self.push_label(I::Loop(BlockType::Empty), false, false);
                    if let Some(test) = test {
                        self.expr(test)?;
                        self.code.push(I::I32Eqz);
                        self.code.push(I::BrIf(self.target(false)?));
                    }
                    self.push_label(I::Block(BlockType::Empty), false, true);
                    self.statements(body)?;
                    self.end();
                    self.statements(update)?;
                    self.code.push(I::Br(self.depth(again)));
                    self.end();
                    self.end();
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
                        self.code.push(I::I32Eq);
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
                S::Break => self.code.push(I::Br(self.target(false)?)),
                S::Continue => self.code.push(I::Br(self.target(true)?)),
            }
        }
        Ok(())
    }
}
