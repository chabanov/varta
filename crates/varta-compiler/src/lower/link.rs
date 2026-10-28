//! Assign global function slots, then resolve module-local bindings against
//! dependency exports. Function bodies are lowered with their own source/scope.
use super::*;
use crate::modules::Graph;
use std::path::PathBuf;

const MAX_FUNCTIONS: usize = varta_common::abi::MAX_USER_FUNCTIONS;
type Symbols = BTreeMap<String, Signature>;

pub(crate) fn lower(
    graph: &Graph,
    profile: varta_common::language::LanguageProfile,
) -> Result<ir::Program> {
    let mut definitions: BTreeMap<PathBuf, Symbols> = BTreeMap::new();
    let mut types = ir::ValueTypes {
        profile,
        ..Default::default()
    };
    let mut next = 0;
    let mut main = None;
    for path in &graph.order {
        let unit = &graph.units[path];
        if profile.binary64()
            && let Some(item) = unit
                .ast
                .body
                .iter()
                .find(|item| tools::is_declaration(item) || http::is_declaration(item))
        {
            return Err(unit.source.locate(
                unsupported(
                    "tool/HTTP contracts in binary64 profile; adapters are not implemented",
                ),
                item.span(),
            ));
        }
        let mut symbols = BTreeMap::new();
        for declaration in declarations::collect(&unit.ast, &unit.source)? {
            if next >= MAX_FUNCTIONS {
                return Err(unit.source.locate(
                    invalid("too many functions across modules"),
                    declaration.ident.span,
                ));
            }
            let name = declaration.ident.sym.to_string();
            let signature = signature::check(declaration, next, &unit.source, &mut types)
                .map_err(|e| unit.source.locate(e, declaration.ident.span))?;
            if path == &graph.entry && name == "main" {
                if signature.receiver.is_some()
                    || !signature.params.is_empty()
                    || signature.result != Type::Void
                {
                    return Err(unit.source.locate(
                        invalid("main must have signature () -> void"),
                        declaration.ident.span,
                    ));
                }
                main = Some(next);
            }
            if matches!(name.as_str(), "varta" | "console" | "log" | "JSON")
                || symbols.insert(name.clone(), signature).is_some()
            {
                return Err(unit.source.locate(
                    invalid(format!("duplicate or reserved function name: {name}")),
                    declaration.ident.span,
                ));
            }
            next += 1;
        }
        definitions.insert(path.clone(), symbols);
    }
    let entry = &graph.units[&graph.entry];
    let main = main.ok_or_else(|| {
        entry.source.locate(
            invalid("entry module must declare a top-level function named main()"),
            entry.ast.span(),
        )
    })?;
    let scopes = link_bindings(graph, &definitions)?;
    let mut functions = Vec::new();
    let mut generated = BTreeMap::new();
    let mut call_effects = BTreeMap::new();
    for path in &graph.order {
        let unit = &graph.units[path];
        for declaration in declarations::collect(&unit.ast, &unit.source)? {
            let signature = &definitions[path][declaration.ident.sym.as_ref()];
            functions.push(body(
                declaration,
                signature,
                &scopes[path],
                &unit.source,
                main,
                &mut types,
                (&mut generated, &mut next, &mut call_effects),
            )?);
        }
    }
    functions.extend(generated.into_values());
    let tools = super::tools::collect(graph, &scopes, &functions, main)?;
    let http = http::collect(graph, &scopes, &mut functions, &mut types, main)?;
    if profile.binary64() && (!tools.is_empty() || http.is_some()) {
        return Err(entry.source.locate(
            unsupported("tool/HTTP contracts in binary64 profile; adapters are not implemented"),
            entry.ast.span(),
        ));
    }
    Ok(ir::Program {
        functions,
        main,
        tools,
        http,
        types,
    })
}

fn link_bindings(
    graph: &Graph,
    definitions: &BTreeMap<PathBuf, Symbols>,
) -> Result<BTreeMap<PathBuf, Symbols>> {
    let mut scopes = BTreeMap::new();
    let mut exports: BTreeMap<PathBuf, Symbols> = BTreeMap::new();
    for path in &graph.order {
        let unit = &graph.units[path];
        let mut scope = definitions[path].clone();
        for import in &unit.imports {
            for binding in &import.bindings {
                let signature = exports
                    .get(&import.target)
                    .and_then(|e| e.get(&binding.imported))
                    .ok_or_else(|| {
                        unit.source.locate(
                            CompilerError::Module(format!(
                                "module {} does not export function {}",
                                import.target.display(),
                                binding.imported
                            )),
                            binding.span,
                        )
                    })?;
                if matches!(binding.local.as_str(), "varta" | "console" | "log" | "JSON")
                    || scope
                        .insert(binding.local.clone(), signature.clone())
                        .is_some()
                {
                    return Err(unit.source.locate(
                        CompilerError::Module(format!(
                            "duplicate or reserved import binding: {}",
                            binding.local
                        )),
                        binding.span,
                    ));
                }
            }
        }
        let mut visible = BTreeMap::new();
        for export in &unit.exports {
            let signature = scope.get(&export.local).ok_or_else(|| {
                unit.source.locate(
                    CompilerError::Module(format!(
                        "export {} is not a function binding",
                        export.local
                    )),
                    export.span,
                )
            })?;
            if visible
                .insert(export.exported.clone(), signature.clone())
                .is_some()
            {
                return Err(unit.source.locate(
                    CompilerError::Module(format!("duplicate export: {}", export.exported)),
                    export.span,
                ));
            }
        }
        exports.insert(path.clone(), visible);
        scopes.insert(path.clone(), scope);
    }
    Ok(scopes)
}

fn body(
    declaration: &ast::FnDecl,
    signature: &Signature,
    functions: &Symbols,
    source: &Source,
    entry: usize,
    types: &mut ir::ValueTypes,
    synthesized: (
        &mut BTreeMap<usize, ir::Function>,
        &mut usize,
        &mut BTreeMap<usize, flow_calls::Impact>,
    ),
) -> Result<ir::Function> {
    let (generated, next_function, call_effects) = synthesized;
    let mut lower = Lower {
        source,
        types,
        functions,
        scopes: vec![BTreeMap::new()],
        next: 0,
        local_types: BTreeMap::new(),
        refinements: BTreeMap::new(),
        writes: Default::default(),
        call_writes: Default::default(),
        call_unknown: false,
        direct_writes: Default::default(),
        call_parameters: Default::default(),
        parameter_values: Default::default(),
        parameter_roots: Default::default(),
        known_calls: Default::default(),
        call_effects,
        reachable: true,
        control: Default::default(),
        guard_blocked: Default::default(),
        outer_narrows: Default::default(),
        result: signature.result,
        infer_returns: false,
        entry,
        loops: 0,
        labels: Vec::new(),
        next_label: 0,
        switches: 0,
        depth: 0,
        repeat_slots: None,
        generated,
        next_function,
        index: signature.index,
        outer: Default::default(),
        captures: Default::default(),
        cells: Default::default(),
        guarded: Default::default(),
    };
    if let Some(ty) = signature.receiver {
        lower.declare("this", Some(ty), true, false)?;
        lower.flow_parameter_binding(&lower.binding("this")?, 0);
    }
    let patterns: Vec<_> = declaration
        .function
        .params
        .iter()
        .map(|p| p.pat.clone())
        .collect();
    let plans = parameters::analyze(
        &patterns,
        Some(&signature.params),
        Some(&signature.shape),
        lower.types,
        Some(lower.source),
    )?;
    lower.reserve_parameters(&plans)?;
    let mut prologue = lower.initialize_parameters(&plans, signature.receiver.is_some())?;
    let body = declaration
        .function
        .body
        .as_ref()
        .ok_or_else(|| source.locate(invalid("missing function body"), declaration.span()))?;
    prologue.extend(lower.block(&body.stmts, false)?);
    let mut statements = prologue;
    finish_body(&mut lower, &mut statements, declaration.span())?;

    Ok(ir::Function {
        shape: signature.shape.clone(),
        self_slot: None,
        receiver: signature.receiver,
        cells: lower.cells,
        captures: vec![],
        environment: false,
        source: source.clone(),
        span: declaration.span(),
        params: signature
            .receiver
            .into_iter()
            .chain(signature.params.iter().copied())
            .collect(),
        result: signature.result,
        locals: (0..lower.next)
            .map(|slot| {
                lower
                    .local_types
                    .get(&slot)
                    .copied()
                    .unwrap_or(Type::Number)
            })
            .collect(),
        body: statements,
    })
}

pub(super) fn finish_body(
    lower: &mut Lower<'_>,
    statements: &mut Vec<S>,
    span: swc_common::Span,
) -> Result<()> {
    if !returns(statements)
        && (lower.result == Type::Undefined
            || lower.types.profile.missing()
                && unions::members(lower.result, lower.types).contains(&Type::Undefined))
    {
        let value = unions::widen(
            Expr {
                span,
                ty: Type::Undefined,
                kind: E::Integer(0),
            },
            Some(lower.result),
            lower.types,
        )?;
        statements.push(S::Return(Some(value)));
    }
    if lower.result != Type::Void && !returns(statements) {
        return Err(lower.source.locate(
            invalid("value-returning function must return on every path"),
            span,
        ));
    }
    Ok(())
}
