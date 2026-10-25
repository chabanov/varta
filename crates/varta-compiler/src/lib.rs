use swc_common::{sync::Lrc, FileName, SourceMap};
use swc_ecma_ast::{Expr, Lit, ModuleItem, Stmt};
use swc_ecma_parser::{lexer::Lexer, Parser, StringInput, Syntax};
use wasm_encoder::{
    BlockType, CodeSection, ConstExpr, DataSection, ExportKind, ExportSection, Function,
    FunctionSection, ImportSection, Instruction, MemorySection, MemoryType, Module, TypeSection,
    ValType,
};

/// The smallest valid Wasm module.
pub fn minimal_module() -> Vec<u8> {
    vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
}

#[derive(Clone, Copy)]
struct LabelFrame {
    breakable: bool,
    continuable: bool,
}

/// Number of top-level items in a parsed TypeScript module.
pub fn parse_typescript(source: &str) -> Result<usize, String> {
    let module = parse_module(source)?;
    Ok(module.body.len())
}

/// Compile a deliberately small TypeScript subset into Wasm.
///
/// Supported currently:
/// - a top-level `function main() { ... }`;
/// - expression statements calling `log(number)` or `console.log(number)`;
/// - implicit integer constants.
pub fn compile_typescript(source: &str) -> Result<Vec<u8>, String> {
    let module = parse_module(source)?;
    let main = find_main(&module)?;

    let body = main
        .function
        .body
        .as_ref()
        .ok_or_else(|| "main() must have a body".to_string())?;

    let variable_names = collect_variables(&body.stmts);
    let mut variables = std::collections::BTreeMap::new();
    for (index, name) in variable_names.iter().enumerate() {
        variables.insert(name.clone(), index as u32);
    }

    let mut string_literals = Vec::new();
    collect_strings(&body.stmts, &mut string_literals);
    let mut string_offsets = std::collections::BTreeMap::new();
    let mut offset = 0u32;
    for value in &string_literals {
        if !string_offsets.contains_key(value) {
            string_offsets.insert(value.clone(), offset);
            offset += value.as_bytes().len() as u32;
        }
    }
    if offset > 65_536 {
        return Err("static string data exceeds one Wasm page".into());
    }

    let mut instructions = Vec::new();
    let mut labels = Vec::new();
    compile_block(
        &body.stmts,
        &mut instructions,
        &variables,
        &string_offsets,
        &mut labels,
    )?;

    let locals = vec![ValType::I32; variables.len() + 3];

    let mut wasm = Module::new();

    // Type 0: (i32) -> ()
    // Type 1: (i32, i32) -> ()
    // Type 2: () -> i32
    // Type 3: (i32, i32) -> i32
    // Type 4: () -> ()
    let mut types = TypeSection::new();
    types.ty().function([ValType::I32], []);
    types.ty().function([ValType::I32, ValType::I32], []);
    types.ty().function([], [ValType::I32]);
    types.ty().function([ValType::I32, ValType::I32], [ValType::I32]);
    types.ty().function([], []);
    wasm.section(&types);

    // Imports:
    // 0: varta.log_i32
    // 1: varta.log_string
    // 2: varta.time_now
    // 3: varta.env_has
    // 4: varta.env_print
    let mut imports = ImportSection::new();
    imports.import("varta", "log_i32", wasm_encoder::EntityType::Function(0));
    imports.import("varta", "log_string", wasm_encoder::EntityType::Function(1));
    imports.import("varta", "time_now", wasm_encoder::EntityType::Function(2));
    imports.import("varta", "env_has", wasm_encoder::EntityType::Function(3));
    imports.import("varta", "env_print", wasm_encoder::EntityType::Function(1));
    imports.import("varta", "fs_print", wasm_encoder::EntityType::Function(1));
    imports.import("varta", "process_exit", wasm_encoder::EntityType::Function(0));
    imports.import("varta", "net_fetch", wasm_encoder::EntityType::Function(3));
    imports.import("varta", "env_get", wasm_encoder::EntityType::Function(3));
    imports.import("varta", "fs_read", wasm_encoder::EntityType::Function(3));
    wasm.section(&imports);

    // Function 4 is `main`, exported as `_start`.
    let mut functions = FunctionSection::new();
    functions.function(4);
    wasm.section(&functions);

    // Memory 0.
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: 1,
        maximum: None,
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    wasm.section(&memories);

    // Module globals.
    // 0: heap pointer; 1: env_result_len.
    let mut globals = wasm_encoder::GlobalSection::new();
    globals.global(
        wasm_encoder::GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(offset as i32),
    );
    globals.global(
        wasm_encoder::GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        },
        &ConstExpr::i32_const(0),
    );
    wasm.section(&globals);

    // Exports: memory, globals, and _start.
    let mut exports = ExportSection::new();
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("heap", ExportKind::Global, 0);
    exports.export("env_result_len", ExportKind::Global, 1);
    exports.export("_start", ExportKind::Func, 10);
    wasm.section(&exports);

    let mut code = CodeSection::new();
    let mut function = Function::new_with_locals_types(locals);
    for instruction in instructions {
        function.instruction(&instruction);
    }
    function.instruction(&Instruction::End);
    code.function(&function);
    wasm.section(&code);

    // Static strings.
    let mut data = DataSection::new();
    for (value, string_offset) in &string_offsets {
        let bytes = value.as_bytes().to_vec();
        data.active(0, &ConstExpr::i32_const(*string_offset as i32), bytes);
    }
    wasm.section(&data);

    Ok(wasm.finish())
}

fn parse_module(source: &str) -> Result<swc_ecma_ast::Module, String> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(FileName::Custom("input.ts".into()).into(), source.to_string());

    let lexer = Lexer::new(
        Syntax::Typescript(Default::default()),
        Default::default(),
        StringInput::from(&*fm),
        None,
    );

    let mut parser = Parser::new_from(lexer);
    for error in parser.take_errors() {
        return Err(format!("{error:?}"));
    }

    parser
        .parse_typescript_module()
        .map_err(|error| format!("{error:?}"))
}

fn find_main(module: &swc_ecma_ast::Module) -> Result<&swc_ecma_ast::FnDecl, String> {
    let mut main = None;
    for item in &module.body {
        let function = match item {
            ModuleItem::Stmt(Stmt::Decl(swc_ecma_ast::Decl::Fn(function))) => Some(function),
            ModuleItem::ModuleDecl(swc_ecma_ast::ModuleDecl::ExportDecl(export)) => {
                if let swc_ecma_ast::Decl::Fn(function) = &export.decl {
                    Some(function)
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some(function) = function {
            if function.ident.sym == "main" {
                if function.function.params.is_empty() {
                    main = Some(function);
                } else {
                    return Err("main() must not take parameters in this MVP".into());
                }
            }
        }
    }

    main.ok_or_else(|| "expected a top-level function named main()".into())
}

fn compile_block(
    statements: &[Stmt],
    output: &mut Vec<Instruction>,
    variables: &std::collections::BTreeMap<String, u32>,
    string_offsets: &std::collections::BTreeMap<String, u32>,
    labels: &mut Vec<LabelFrame>,
) -> Result<(), String> {
    for statement in statements {
        match statement {
            Stmt::Decl(swc_ecma_ast::Decl::Var(var_decl)) => {
                for declarator in &var_decl.decls {
                    let Some(name) = binding_name(&declarator.name) else {
                        return Err("only simple variable names are supported".into());
                    };
                    let index = *variables
                        .get(&name)
                        .ok_or_else(|| format!("unknown variable: {name}"))?;
                    if let Some(init) = &declarator.init {
                        compile_expr(init, output, variables, string_offsets)?;
                        output.push(Instruction::LocalSet(index));
                    } else {
                        output.push(Instruction::I32Const(0));
                        output.push(Instruction::LocalSet(index));
                    }
                }
            }
            Stmt::Expr(expr_stmt) => {
                if let Expr::Assign(assign) = &*expr_stmt.expr {
                    if assign.op != swc_ecma_ast::AssignOp::Assign {
                        return Err("only simple assignment is supported".into());
                    }
                    let Some(name) = (match &assign.left {
                        swc_ecma_ast::AssignTarget::Simple(
                            swc_ecma_ast::SimpleAssignTarget::Ident(binding),
                        ) => Some(binding.id.sym.to_string()),
                        _ => None,
                    }) else {
                        return Err("only simple variable assignment is supported".into());
                    };
                    let index = *variables
                        .get(&name)
                        .ok_or_else(|| format!("unknown variable: {name}"))?;
                    compile_expr(&assign.right, output, variables, string_offsets)?;
                    output.push(Instruction::LocalSet(index));
                    continue;
                }

                if let Expr::Call(call) = &*expr_stmt.expr {
                    if is_supported_log_call(call) && call.args.len() == 1 {
                        if let Expr::Call(inner) = &*call.args[0].expr {
                            if is_env_get_call(inner) && inner.args.len() == 1 {
                                if let Expr::Lit(Lit::Str(value)) = &*inner.args[0].expr {
                                    let text = value
                                        .value
                                        .as_str()
                                        .ok_or_else(|| "only UTF-8 string literals are supported".to_string())?;
                                    let pointer = *string_offsets.get(text).ok_or_else(|| {
                                        "internal error: missing string offset".to_string()
                                    })?;
                                    output.push(Instruction::I32Const(pointer as i32));
                                    output.push(Instruction::I32Const(text.len() as i32));
                                    output.push(Instruction::Call(8));
                                    output.push(Instruction::GlobalGet(1));
                                    output.push(Instruction::Call(1));
                                    continue;
                                }
                            }
                        }
                        if let Expr::Call(inner) = &*call.args[0].expr {
                            if is_fs_read_call(inner) && inner.args.len() == 1 {
                                if let Expr::Lit(Lit::Str(value)) = &*inner.args[0].expr {
                                    let text = value
                                        .value
                                        .as_str()
                                        .ok_or_else(|| "only UTF-8 string literals are supported".to_string())?;
                                    let pointer = *string_offsets.get(text).ok_or_else(|| {
                                        "internal error: missing string offset".to_string()
                                    })?;
                                    output.push(Instruction::I32Const(pointer as i32));
                                    output.push(Instruction::I32Const(text.len() as i32));
                                    output.push(Instruction::Call(9));
                                    output.push(Instruction::GlobalGet(1));
                                    output.push(Instruction::Call(1));
                                    continue;
                                }
                            }
                        }
                        match &*call.args[0].expr {
                            Expr::Lit(Lit::Str(value)) => {
                                let text = value
                                    .value
                                    .as_str()
                                    .ok_or_else(|| "only UTF-8 string literals are supported".to_string())?;
                                let pointer = *string_offsets.get(text).ok_or_else(|| {
                                    "internal error: missing string offset".to_string()
                                })?;
                                output.push(Instruction::I32Const(pointer as i32));
                                output.push(Instruction::I32Const(text.len() as i32));
                                output.push(Instruction::Call(1));
                            }
                            _ => {
                                compile_expr(&call.args[0].expr, output, variables, string_offsets)?;
                                output.push(Instruction::Call(0));
                            }
                        }
                        continue;
                    }
                }
                let produces_value = compile_expr(&expr_stmt.expr, output, variables, string_offsets)?;
                if produces_value {
                    output.push(Instruction::Drop);
                }
            }
            Stmt::Return(ret) => {
                if let Some(expr) = &ret.arg {
                    compile_expr(expr, output, variables, string_offsets)?;
                    output.push(Instruction::Drop);
                }
            }
            Stmt::Empty(_) => {}
            Stmt::Block(block) => {
                compile_block(&block.stmts, output, variables, string_offsets, labels)?;
            }
            Stmt::Break(_) => {
                let position = labels
                    .iter()
                    .rposition(|frame| frame.breakable)
                    .ok_or_else(|| "break is only supported inside a loop".to_string())?;
                let index = labels.len() - 1 - position;
                output.push(Instruction::Br(index as u32));
            }
            Stmt::Continue(_) => {
                let position = labels
                    .iter()
                    .rposition(|frame| frame.continuable)
                    .ok_or_else(|| "continue is only supported inside a loop".to_string())?;
                let index = labels.len() - 1 - position;
                output.push(Instruction::Br(index as u32));
            }
            Stmt::If(if_stmt) => {
                compile_expr(&if_stmt.test, output, variables, string_offsets)?;
                output.push(Instruction::If(BlockType::Empty));
                labels.push(LabelFrame {
                    breakable: false,
                    continuable: false,
                });
                compile_block(std::slice::from_ref(&*if_stmt.cons), output, variables, string_offsets, labels)?;
                if let Some(alternate) = &if_stmt.alt {
                    output.push(Instruction::Else);
                    compile_block(std::slice::from_ref(&**alternate), output, variables, string_offsets, labels)?;
                }
                labels.pop();
                output.push(Instruction::End);
            }
            Stmt::While(while_stmt) => {
                labels.push(LabelFrame {
                    breakable: true,
                    continuable: false,
                });
                labels.push(LabelFrame {
                    breakable: false,
                    continuable: true,
                });
                output.push(Instruction::Block(BlockType::Empty));
                output.push(Instruction::Loop(BlockType::Empty));
                compile_expr(&while_stmt.test, output, variables, string_offsets)?;
                output.push(Instruction::I32Eqz);
                output.push(Instruction::BrIf(1));
                compile_block(std::slice::from_ref(&*while_stmt.body), output, variables, string_offsets, labels)?;
                output.push(Instruction::Br(0));
                output.push(Instruction::End);
                output.push(Instruction::End);
                labels.pop();
                labels.pop();
            }
            Stmt::For(for_stmt) => {
                if let Some(init) = &for_stmt.init {
                    match init {
                        swc_ecma_ast::VarDeclOrExpr::VarDecl(var_decl) => {
                            for declarator in &var_decl.decls {
                                let Some(name) = binding_name(&declarator.name) else {
                                    return Err("only simple variable names are supported".into());
                                };
                                let index = *variables
                                    .get(&name)
                                    .ok_or_else(|| format!("unknown variable: {name}"))?;
                                if let Some(init_expr) = &declarator.init {
                                    compile_expr(init_expr, output, variables, string_offsets)?;
                                    output.push(Instruction::LocalSet(index));
                                } else {
                                    output.push(Instruction::I32Const(0));
                                    output.push(Instruction::LocalSet(index));
                                }
                            }
                        }
                        swc_ecma_ast::VarDeclOrExpr::Expr(expr) => {
                            let produces_value = compile_expr(expr, output, variables, string_offsets)?;
                            if produces_value {
                                output.push(Instruction::Drop);
                            }
                        }
                    }
                }

                labels.push(LabelFrame {
                    breakable: true,
                    continuable: false,
                });
                labels.push(LabelFrame {
                    breakable: false,
                    continuable: true,
                });
                output.push(Instruction::Block(BlockType::Empty));
                output.push(Instruction::Loop(BlockType::Empty));
                if let Some(test) = &for_stmt.test {
                    compile_expr(test, output, variables, string_offsets)?;
                    output.push(Instruction::I32Eqz);
                    output.push(Instruction::BrIf(1));
                }
                labels.push(LabelFrame {
                    breakable: false,
                    continuable: true,
                });
                output.push(Instruction::Block(BlockType::Empty));
                compile_block(std::slice::from_ref(&*for_stmt.body), output, variables, string_offsets, labels)?;
                output.push(Instruction::End);
                labels.pop();
                if let Some(update) = &for_stmt.update {
                    if let Expr::Assign(assign) = &**update {
                        if assign.op != swc_ecma_ast::AssignOp::Assign {
                            return Err("only simple assignment is supported".into());
                        }
                        let Some(name) = (match &assign.left {
                            swc_ecma_ast::AssignTarget::Simple(
                                swc_ecma_ast::SimpleAssignTarget::Ident(binding),
                            ) => Some(binding.id.sym.to_string()),
                            _ => None,
                        }) else {
                            return Err("only simple variable assignment is supported".into());
                        };
                        let index = *variables
                            .get(&name)
                            .ok_or_else(|| format!("unknown variable: {name}"))?;
                        compile_expr(&assign.right, output, variables, string_offsets)?;
                        output.push(Instruction::LocalSet(index));
                    } else {
                        let produces_value = compile_expr(update, output, variables, string_offsets)?;
                        if produces_value {
                            output.push(Instruction::Drop);
                        }
                    }
                }
                output.push(Instruction::Br(0));
                output.push(Instruction::End);
                output.push(Instruction::End);
                labels.pop();
                labels.pop();
            }
            Stmt::Switch(switch_stmt) => {
                let discriminant = variables.len() as u32;
                let matched = variables.len() as u32 + 1;
                compile_expr(&switch_stmt.discriminant, output, variables, string_offsets)?;
                output.push(Instruction::LocalSet(discriminant));
                output.push(Instruction::I32Const(0));
                output.push(Instruction::LocalSet(matched));

                labels.push(LabelFrame {
                    breakable: true,
                    continuable: false,
                });
                output.push(Instruction::Block(BlockType::Empty));
                compile_switch_cases(
                    &switch_stmt.cases,
                    output,
                    variables,
                    string_offsets,
                    labels,
                    discriminant,
                    matched,
                )?;
                output.push(Instruction::End);
                labels.pop();
            }
            other => {
                return Err(format!("unsupported statement in main(): {other:?}"));
            }
        }
    }
    Ok(())
}

fn compile_switch_cases(
    cases: &[swc_ecma_ast::SwitchCase],
    output: &mut Vec<Instruction>,
    variables: &std::collections::BTreeMap<String, u32>,
    string_offsets: &std::collections::BTreeMap<String, u32>,
    labels: &mut Vec<LabelFrame>,
    discriminant: u32,
    matched: u32,
) -> Result<(), String> {
    if cases.is_empty() {
        return Ok(());
    }

    let case = &cases[0];
    let rest = &cases[1..];

    let Some(test) = &case.test else {
        output.push(Instruction::LocalGet(matched));
        output.push(Instruction::I32Eqz);
        output.push(Instruction::If(BlockType::Empty));
        compile_block(&case.cons, output, variables, string_offsets, labels)?;
        output.push(Instruction::End);
        return Ok(());
    };

    output.push(Instruction::LocalGet(matched));
    output.push(Instruction::LocalGet(discriminant));
    compile_expr(test, output, variables, string_offsets)?;
    output.push(Instruction::I32Eq);
    output.push(Instruction::I32Or);
    output.push(Instruction::LocalTee(matched));
    output.push(Instruction::If(BlockType::Empty));
    compile_block(&case.cons, output, variables, string_offsets, labels)?;
    output.push(Instruction::End);
    compile_switch_cases(rest, output, variables, string_offsets, labels, discriminant, matched)?;
    Ok(())
}


fn compile_expr(
    expr: &Expr,
    output: &mut Vec<Instruction>,
    variables: &std::collections::BTreeMap<String, u32>,
    string_offsets: &std::collections::BTreeMap<String, u32>,
) -> Result<bool, String> {
    match expr {
        Expr::Lit(Lit::Num(number)) => {
            if number.value.fract() != 0.0 {
                return Err("only integer literals are supported".into());
            }
            if number.value < i32::MIN as f64 || number.value > i32::MAX as f64 {
                return Err("integer literal is outside i32 range".into());
            }
            output.push(Instruction::I32Const(number.value as i32));
            Ok(true)
        }
        Expr::Ident(ident) => {
            if let Some(index) = variables.get(ident.sym.as_str()) {
                output.push(Instruction::LocalGet(*index));
                Ok(true)
            } else {
                Err(format!("unknown identifier: {}", ident.sym))
            }
        }
        Expr::Bin(bin) => {
            match bin.op {
                swc_ecma_ast::BinaryOp::LogicalAnd => {
                    compile_expr(&bin.left, output, variables, string_offsets)?;
                    let temp = variables.len() as u32 + 2;
                    output.push(Instruction::LocalTee(temp));
                    output.push(Instruction::If(BlockType::Result(ValType::I32)));
                    compile_expr(&bin.right, output, variables, string_offsets)?;
                    output.push(Instruction::Else);
                    output.push(Instruction::LocalGet(temp));
                    output.push(Instruction::End);
                    Ok(true)
                }
                swc_ecma_ast::BinaryOp::LogicalOr => {
                    compile_expr(&bin.left, output, variables, string_offsets)?;
                    let temp = variables.len() as u32 + 2;
                    output.push(Instruction::LocalTee(temp));
                    output.push(Instruction::If(BlockType::Result(ValType::I32)));
                    output.push(Instruction::LocalGet(temp));
                    output.push(Instruction::Else);
                    compile_expr(&bin.right, output, variables, string_offsets)?;
                    output.push(Instruction::End);
                    Ok(true)
                }
                _ => {
                    compile_expr(&bin.left, output, variables, string_offsets)?;
                    compile_expr(&bin.right, output, variables, string_offsets)?;
                    match bin.op {
                        swc_ecma_ast::BinaryOp::Add => output.push(Instruction::I32Add),
                        swc_ecma_ast::BinaryOp::Sub => output.push(Instruction::I32Sub),
                        swc_ecma_ast::BinaryOp::Mul => output.push(Instruction::I32Mul),
                        swc_ecma_ast::BinaryOp::Div => output.push(Instruction::I32DivS),
                        swc_ecma_ast::BinaryOp::Mod => output.push(Instruction::I32RemS),
                        swc_ecma_ast::BinaryOp::Lt => output.push(Instruction::I32LtS),
                        swc_ecma_ast::BinaryOp::LtEq => output.push(Instruction::I32LeS),
                        swc_ecma_ast::BinaryOp::Gt => output.push(Instruction::I32GtS),
                        swc_ecma_ast::BinaryOp::GtEq => output.push(Instruction::I32GeS),
                        swc_ecma_ast::BinaryOp::EqEq => output.push(Instruction::I32Eq),
                        swc_ecma_ast::BinaryOp::EqEqEq => output.push(Instruction::I32Eq),
                        swc_ecma_ast::BinaryOp::NotEq => output.push(Instruction::I32Ne),
                        swc_ecma_ast::BinaryOp::NotEqEq => output.push(Instruction::I32Ne),
                        _ => return Err("unsupported binary operator".into()),
                    };
                    Ok(true)
                }
            }
        }
        Expr::Unary(unary) if matches!(unary.op, swc_ecma_ast::UnaryOp::Minus) => {
            output.push(Instruction::I32Const(0));
            compile_expr(&unary.arg, output, variables, string_offsets)?;
            output.push(Instruction::I32Sub);
            Ok(true)
        }
        Expr::Call(call) => {
            if is_time_now_call(call) {
                if !call.args.is_empty() {
                    return Err("varta.time.now() does not accept arguments".into());
                }
                output.push(Instruction::Call(2));
                Ok(true)
            } else if is_env_has_call(call) {
                if call.args.len() != 1 {
                    return Err("varta.env.has() expects one string argument".into());
                }
                match &*call.args[0].expr {
                    Expr::Lit(Lit::Str(value)) => {
                        let text = value
                            .value
                            .as_str()
                            .ok_or_else(|| "only UTF-8 string literals are supported".to_string())?;
                        let pointer = *string_offsets.get(text).ok_or_else(|| {
                            "internal error: missing string offset".to_string()
                        })?;
                        output.push(Instruction::I32Const(pointer as i32));
                        output.push(Instruction::I32Const(text.len() as i32));
                        output.push(Instruction::Call(3));
                        Ok(true)
                    }
                    _ => Err("varta.env.has() requires a string literal".into()),
                }
            } else if is_env_print_call(call) {
                if call.args.len() != 1 {
                    return Err("varta.env.print() expects one string argument".into());
                }
                match &*call.args[0].expr {
                    Expr::Lit(Lit::Str(value)) => {
                        let text = value
                            .value
                            .as_str()
                            .ok_or_else(|| "only UTF-8 string literals are supported".to_string())?;
                        let pointer = *string_offsets.get(text).ok_or_else(|| {
                            "internal error: missing string offset".to_string()
                        })?;
                        output.push(Instruction::I32Const(pointer as i32));
                        output.push(Instruction::I32Const(text.len() as i32));
                        output.push(Instruction::Call(4));
                        Ok(false)
                    }
                    _ => Err("varta.env.print() requires a string literal".into()),
                }
            } else if is_fs_print_call(call) {
                if call.args.len() != 1 {
                    return Err("varta.fs.print() expects one string argument".into());
                }
                match &*call.args[0].expr {
                    Expr::Lit(Lit::Str(value)) => {
                        let text = value
                            .value
                            .as_str()
                            .ok_or_else(|| "only UTF-8 string literals are supported".to_string())?;
                        let pointer = *string_offsets.get(text).ok_or_else(|| {
                            "internal error: missing string offset".to_string()
                        })?;
                        output.push(Instruction::I32Const(pointer as i32));
                        output.push(Instruction::I32Const(text.len() as i32));
                        output.push(Instruction::Call(5));
                        Ok(false)
                    }
                    _ => Err("varta.fs.print() requires a string literal".into()),
                }
            } else if is_process_exit_call(call) {
                if call.args.len() != 1 {
                    return Err("varta.process.exit() expects one number argument".into());
                }
                compile_expr(&call.args[0].expr, output, variables, string_offsets)?;
                output.push(Instruction::Call(6));
                Ok(false)
            } else if is_net_fetch_call(call) {
                if call.args.len() != 1 {
                    return Err("varta.net.fetch() expects one string argument".into());
                }
                match &*call.args[0].expr {
                    Expr::Lit(Lit::Str(value)) => {
                        let text = value
                            .value
                            .as_str()
                            .ok_or_else(|| "only UTF-8 string literals are supported".to_string())?;
                        let pointer = *string_offsets.get(text).ok_or_else(|| {
                            "internal error: missing string offset".to_string()
                        })?;
                        output.push(Instruction::I32Const(pointer as i32));
                        output.push(Instruction::I32Const(text.len() as i32));
                        output.push(Instruction::Call(8));
                        Ok(true)
                    }
                    _ => Err("varta.net.fetch() requires a string literal".into()),
                }
            } else {
                Err("unsupported call expression".into())
            }
        }
        Expr::Paren(paren) => compile_expr(&paren.expr, output, variables, string_offsets),
        _ => Err("unsupported expression".into()),
    }
}

fn collect_strings(statements: &[Stmt], output: &mut Vec<String>) {
    for statement in statements {
        match statement {
            Stmt::Decl(swc_ecma_ast::Decl::Var(var_decl)) => {
                for declarator in &var_decl.decls {
                    if let Some(init) = &declarator.init {
                        collect_expr_strings(init, output);
                    }
                }
            }
            Stmt::Expr(expr_stmt) => collect_expr_strings(&expr_stmt.expr, output),
            Stmt::Return(ret) => {
                if let Some(expr) = &ret.arg {
                    collect_expr_strings(expr, output);
                }
            }
            Stmt::Block(block) => collect_strings(&block.stmts, output),
            Stmt::If(if_stmt) => {
                collect_expr_strings(&if_stmt.test, output);
                collect_strings(std::slice::from_ref(&*if_stmt.cons), output);
                if let Some(alternate) = &if_stmt.alt {
                    collect_strings(std::slice::from_ref(&**alternate), output);
                }
            }
            Stmt::While(while_stmt) => {
                collect_expr_strings(&while_stmt.test, output);
                collect_strings(std::slice::from_ref(&*while_stmt.body), output);
            }
            Stmt::For(for_stmt) => {
                if let Some(swc_ecma_ast::VarDeclOrExpr::VarDecl(var_decl)) = &for_stmt.init {
                    for declarator in &var_decl.decls {
                        if let Some(init) = &declarator.init {
                            collect_expr_strings(init, output);
                        }
                    }
                } else if let Some(swc_ecma_ast::VarDeclOrExpr::Expr(expr)) = &for_stmt.init {
                    collect_expr_strings(expr, output);
                }
                if let Some(test) = &for_stmt.test {
                    collect_expr_strings(test, output);
                }
                if let Some(update) = &for_stmt.update {
                    collect_expr_strings(update, output);
                }
                collect_strings(std::slice::from_ref(&*for_stmt.body), output);
            }
            Stmt::Switch(switch_stmt) => {
                collect_expr_strings(&switch_stmt.discriminant, output);
                for case in &switch_stmt.cases {
                    collect_strings(&case.cons, output);
                }
            }
            _ => {}
        }
    }
}

fn collect_expr_strings(expr: &Expr, output: &mut Vec<String>) {
    match expr {
        Expr::Lit(Lit::Str(value)) => {
            if let Some(text) = value.value.as_str() {
                if !output.iter().any(|existing| existing == text) {
                    output.push(text.to_string());
                }
            }
        }
        Expr::Call(call) => {
            for arg in &call.args {
                collect_expr_strings(&arg.expr, output);
            }
        }
        Expr::Bin(bin) => {
            collect_expr_strings(&bin.left, output);
            collect_expr_strings(&bin.right, output);
        }
        Expr::Unary(unary) => collect_expr_strings(&unary.arg, output),
        Expr::Paren(paren) => collect_expr_strings(&paren.expr, output),
        _ => {}
    }
}

fn collect_variables(statements: &[Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    for statement in statements {
        match statement {
            Stmt::Decl(swc_ecma_ast::Decl::Var(var_decl)) => {
                for declarator in &var_decl.decls {
                    if let Some(name) = binding_name(&declarator.name) {
                        if !names.contains(&name) {
                            names.push(name);
                        }
                    }
                }
            }
            Stmt::Block(block) => {
                for name in collect_variables(&block.stmts) {
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
            Stmt::If(if_stmt) => {
                for name in collect_variables(std::slice::from_ref(&*if_stmt.cons)) {
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
                if let Some(alternate) = &if_stmt.alt {
                    for name in collect_variables(std::slice::from_ref(&**alternate)) {
                        if !names.contains(&name) {
                            names.push(name);
                        }
                    }
                }
            }
            Stmt::While(while_stmt) => {
                for name in collect_variables(std::slice::from_ref(&*while_stmt.body)) {
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
            Stmt::For(for_stmt) => {
                if let Some(swc_ecma_ast::VarDeclOrExpr::VarDecl(var_decl)) = &for_stmt.init {
                    for declarator in &var_decl.decls {
                        if let Some(name) = binding_name(&declarator.name) {
                            if !names.contains(&name) {
                                names.push(name);
                            }
                        }
                    }
                }
                for name in collect_variables(std::slice::from_ref(&*for_stmt.body)) {
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
            Stmt::Switch(switch_stmt) => {
                for case in &switch_stmt.cases {
                    for name in collect_variables(&case.cons) {
                        if !names.contains(&name) {
                            names.push(name);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    names
}


fn is_env_print_call(call: &swc_ecma_ast::CallExpr) -> bool {
    let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
        return false;
    };
    let Expr::Member(member) = &**callee else {
        return false;
    };
    let property_ok = matches!(
        member.prop,
        swc_ecma_ast::MemberProp::Ident(ref ident) if ident.sym == "print"
    );
    let object_ok = matches!(
        &*member.obj,
        Expr::Member(env_member)
            if matches!(&*env_member.obj, Expr::Ident(ident) if ident.sym == "varta")
                && matches!(
                    env_member.prop,
                    swc_ecma_ast::MemberProp::Ident(ref ident) if ident.sym == "env"
                )
    );
    property_ok && object_ok
}

fn is_fs_print_call(call: &swc_ecma_ast::CallExpr) -> bool {
    let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
        return false;
    };
    let Expr::Member(member) = &**callee else {
        return false;
    };
    let property_ok = matches!(
        member.prop,
        swc_ecma_ast::MemberProp::Ident(ref ident) if ident.sym == "print"
    );
    let object_ok = matches!(
        &*member.obj,
        Expr::Member(fs_member)
            if matches!(&*fs_member.obj, Expr::Ident(ident) if ident.sym == "varta")
                && matches!(
                    fs_member.prop,
                    swc_ecma_ast::MemberProp::Ident(ref ident) if ident.sym == "fs"
                )
    );
    property_ok && object_ok
}

fn is_process_exit_call(call: &swc_ecma_ast::CallExpr) -> bool {
    let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
        return false;
    };
    let Expr::Member(member) = &**callee else {
        return false;
    };
    let property_ok = matches!(
        member.prop,
        swc_ecma_ast::MemberProp::Ident(ref ident) if ident.sym == "exit"
    );
    let object_ok = matches!(
        &*member.obj,
        Expr::Member(process_member)
            if matches!(&*process_member.obj, Expr::Ident(ident) if ident.sym == "varta")
                && matches!(
                    process_member.prop,
                    swc_ecma_ast::MemberProp::Ident(ref ident) if ident.sym == "process"
                )
    );
    property_ok && object_ok
}

fn is_net_fetch_call(call: &swc_ecma_ast::CallExpr) -> bool {
    let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
        return false;
    };
    let Expr::Member(member) = &**callee else {
        return false;
    };
    let property_ok = matches!(
        member.prop,
        swc_ecma_ast::MemberProp::Ident(ref ident) if ident.sym == "fetch"
    );
    let object_ok = matches!(
        &*member.obj,
        Expr::Member(net_member)
            if matches!(&*net_member.obj, Expr::Ident(ident) if ident.sym == "varta")
                && matches!(
                    net_member.prop,
                    swc_ecma_ast::MemberProp::Ident(ref ident) if ident.sym == "net"
                )
    );
    property_ok && object_ok
}

fn is_env_get_call(call: &swc_ecma_ast::CallExpr) -> bool {
    let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
        return false;
    };
    let Expr::Member(member) = &**callee else {
        return false;
    };
    let property_ok = matches!(
        member.prop,
        swc_ecma_ast::MemberProp::Ident(ref ident) if ident.sym == "get"
    );
    let object_ok = matches!(
        &*member.obj,
        Expr::Member(env_member)
            if matches!(&*env_member.obj, Expr::Ident(ident) if ident.sym == "varta")
                && matches!(
                    env_member.prop,
                    swc_ecma_ast::MemberProp::Ident(ref ident) if ident.sym == "env"
                )
    );
    property_ok && object_ok
}

fn is_fs_read_call(call: &swc_ecma_ast::CallExpr) -> bool {
    let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
        return false;
    };
    let Expr::Member(member) = &**callee else {
        return false;
    };
    let property_ok = matches!(
        member.prop,
        swc_ecma_ast::MemberProp::Ident(ref ident) if ident.sym == "read"
    );
    let object_ok = matches!(
        &*member.obj,
        Expr::Member(fs_member)
            if matches!(&*fs_member.obj, Expr::Ident(ident) if ident.sym == "varta")
                && matches!(
                    fs_member.prop,
                    swc_ecma_ast::MemberProp::Ident(ref ident) if ident.sym == "fs"
                )
    );
    property_ok && object_ok
}

fn binding_name(pat: &swc_ecma_ast::Pat) -> Option<String> {
    match pat {
        swc_ecma_ast::Pat::Ident(binding) => Some(binding.id.sym.to_string()),
        _ => None,
    }
}


fn is_supported_log_call(call: &swc_ecma_ast::CallExpr) -> bool {
    match &call.callee {
        swc_ecma_ast::Callee::Expr(expr) => match &**expr {
            Expr::Ident(ident) => ident.sym == "log",
            Expr::Member(member) => {
                let property_matches = match &member.prop {
                    swc_ecma_ast::MemberProp::Ident(ident) => ident.sym == "log",
                    _ => false,
                };
                let object_matches = matches!(&*member.obj, Expr::Ident(object) if object.sym == "console");
                property_matches && object_matches
            }
            _ => false,
        },
        _ => false,
    }
}

fn is_time_now_call(call: &swc_ecma_ast::CallExpr) -> bool {
    let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
        return false;
    };
    let Expr::Member(member) = &**callee else {
        return false;
    };
    let property_ok = matches!(
        member.prop,
        swc_ecma_ast::MemberProp::Ident(ref ident) if ident.sym == "now"
    );
    let object_ok = matches!(
        &*member.obj,
        Expr::Member(time_member)
            if matches!(&*time_member.obj, Expr::Ident(ident) if ident.sym == "varta")
                && matches!(
                    time_member.prop,
                    swc_ecma_ast::MemberProp::Ident(ref ident) if ident.sym == "time"
                )
    );
    property_ok && object_ok
}

fn is_env_has_call(call: &swc_ecma_ast::CallExpr) -> bool {
    let swc_ecma_ast::Callee::Expr(callee) = &call.callee else {
        return false;
    };
    let Expr::Member(member) = &**callee else {
        return false;
    };
    let property_ok = matches!(
        member.prop,
        swc_ecma_ast::MemberProp::Ident(ref ident) if ident.sym == "has"
    );
    let object_ok = matches!(
        &*member.obj,
        Expr::Member(env_member)
            if matches!(&*env_member.obj, Expr::Ident(ident) if ident.sym == "varta")
                && matches!(
                    env_member.prop,
                    swc_ecma_ast::MemberProp::Ident(ref ident) if ident.sym == "env"
                )
    );
    property_ok && object_ok
}

