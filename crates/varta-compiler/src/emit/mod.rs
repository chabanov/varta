mod expr;
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
    temporary: u32,
    result_len: u32,
    result_status: u32,
}

pub(crate) fn emit(program: &ir::Program, source: &crate::source::Source) -> Result<Vec<u8>> {
    let mut hosts = BTreeSet::new();
    let mut strings = BTreeMap::new();
    for function in &program.functions {
        collect(&function.body, &mut hosts, &mut strings);
    }
    let mut offsets = BTreeMap::new();
    let mut offset = 0usize;
    for (string, span) in strings {
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
    let mut intern = |key: (usize, usize)| {
        *signatures.entry(key).or_insert_with(|| {
            let index = types.len();
            types
                .ty()
                .function(vec![ValType::I32; key.0], vec![ValType::I32; key.1]);
            index
        })
    };
    let host_types: BTreeMap<_, _> = hosts.keys().map(|h| (*h, intern(h.signature()))).collect();
    let function_types: Vec<_> = program
        .functions
        .iter()
        .map(|f| intern((f.params.len(), usize::from(f.result != Type::Void))))
        .collect();
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
    module.section(&globals);
    let functions_begin = hosts.len() as u32;
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
        let mut emitter = Emitter {
            code: Vec::new(),
            labels: Vec::new(),
            hosts: &hosts,
            strings: &offsets,
            functions_begin,
            temporary: function.locals,
            result_len,
            result_status,
        };
        emitter.statements(&function.body)?;
        let declared = function.locals - function.params.len() as u32 + 1;
        let mut body = wasm_encoder::Function::new([(declared, ValType::I32)]);
        for instruction in &emitter.code {
            body.instruction(instruction);
        }
        if function.result != Type::Void {
            body.instruction(&I::Unreachable);
        }
        body.instruction(&I::End);
        code.function(&body);
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

fn collect_expr(
    expr: &Expr,
    hosts: &mut BTreeSet<Host>,
    strings: &mut BTreeMap<String, swc_common::Span>,
) {
    match &expr.kind {
        E::String(s) => {
            strings.entry(s.clone()).or_insert(expr.span);
        }
        E::Host(host, args) => {
            hosts.insert(*host);
            for arg in args {
                collect_expr(arg, hosts, strings);
            }
        }
        E::Call(_, args) => {
            for arg in args {
                collect_expr(arg, hosts, strings);
            }
        }
        E::Binary(_, a, b) => {
            collect_expr(a, hosts, strings);
            collect_expr(b, hosts, strings);
        }
        E::Negative(a) | E::Not(a) => collect_expr(a, hosts, strings),
        _ => {}
    }
}
fn collect(
    statements: &[S],
    hosts: &mut BTreeSet<Host>,
    strings: &mut BTreeMap<String, swc_common::Span>,
) {
    for statement in statements {
        match statement {
            S::Set(_, e) | S::Expr(e) | S::Return(Some(e)) => collect_expr(e, hosts, strings),
            S::Block(b) => collect(b, hosts, strings),
            S::If(e, a, b) => {
                collect_expr(e, hosts, strings);
                collect(a, hosts, strings);
                collect(b, hosts, strings);
            }
            S::While(e, b) => {
                collect_expr(e, hosts, strings);
                collect(b, hosts, strings);
            }
            S::For(a, test, update, b) => {
                collect(a, hosts, strings);
                if let Some(e) = test {
                    collect_expr(e, hosts, strings);
                }
                collect(update, hosts, strings);
                collect(b, hosts, strings);
            }
            S::Switch { value, cases, .. } => {
                collect_expr(value, hosts, strings);
                for (e, b) in cases {
                    if let Some(e) = e {
                        collect_expr(e, hosts, strings);
                    }
                    collect(b, hosts, strings);
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
                    self.code.push(I::LocalSet(*slot));
                }
                S::Expr(value) => {
                    self.expr(value)?;
                    for _ in 0..match value.ty {
                        Type::Void => 0,
                        Type::String => 2,
                        _ => 1,
                    } {
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
                    self.code.push(I::LocalSet(*discriminant));
                    self.code.push(I::I32Const(-1));
                    self.code.push(I::LocalSet(*selected));
                    let mut default = cases.len();
                    for (index, (test, _)) in cases.iter().enumerate() {
                        let Some(test) = test else {
                            default = index;
                            continue;
                        };
                        self.code.push(I::LocalGet(*selected));
                        self.code.push(I::I32Const(-1));
                        self.code.push(I::I32Eq);
                        self.code.push(I::If(BlockType::Empty));
                        self.code.push(I::LocalGet(*discriminant));
                        self.expr(test)?;
                        self.code.push(I::I32Eq);
                        self.code.push(I::If(BlockType::Empty));
                        self.code.push(I::I32Const(index as i32));
                        self.code.push(I::LocalSet(*selected));
                        self.code.push(I::End);
                        self.code.push(I::End);
                    }
                    self.code.push(I::LocalGet(*selected));
                    self.code.push(I::I32Const(-1));
                    self.code.push(I::I32Eq);
                    self.code.push(I::If(BlockType::Empty));
                    self.code.push(I::I32Const(default as i32));
                    self.code.push(I::LocalSet(*selected));
                    self.code.push(I::End);
                    self.push_label(I::Block(BlockType::Empty), true, false);
                    for (index, (_, body)) in cases.iter().enumerate() {
                        self.code.push(I::LocalGet(*selected));
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
