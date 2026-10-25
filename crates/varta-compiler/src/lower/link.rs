//! Assign global function slots, then resolve module-local bindings against
//! dependency exports. Function bodies are lowered with their own source/scope.
use super::*;
use crate::modules::Graph;
use std::path::PathBuf;

const MAX_FUNCTIONS: usize = varta_common::abi::MAX_USER_FUNCTIONS;
type Symbols = BTreeMap<String, Signature>;

pub(crate) fn lower(graph: &Graph) -> Result<ir::Program> {
    let mut definitions: BTreeMap<PathBuf, Symbols> = BTreeMap::new();
    let mut next = 0;
    let mut main = None;
    for path in &graph.order {
        let unit = &graph.units[path];
        let mut symbols = BTreeMap::new();
        for declaration in declarations::collect(&unit.ast, &unit.source)? {
            if next >= MAX_FUNCTIONS {
                return Err(unit.source.locate(
                    invalid("too many functions across modules"),
                    declaration.ident.span,
                ));
            }
            let name = declaration.ident.sym.to_string();
            let signature = signature::check(declaration, next, &unit.source)
                .map_err(|e| unit.source.locate(e, declaration.ident.span))?;
            if path == &graph.entry && name == "main" {
                if !signature.params.is_empty() || signature.result != Type::Void {
                    return Err(unit.source.locate(
                        invalid("main must have signature () -> void"),
                        declaration.ident.span,
                    ));
                }
                main = Some(next);
            }
            if matches!(name.as_str(), "varta" | "console" | "log")
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
            )?);
        }
    }
    let tools = super::tools::collect(graph, &scopes, &functions, main)?;
    Ok(ir::Program {
        functions,
        main,
        tools,
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
                if matches!(binding.local.as_str(), "varta" | "console" | "log")
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
) -> Result<ir::Function> {
    let mut lower = Lower {
        source,
        functions,
        scopes: vec![BTreeMap::new()],
        next: 0,
        local_types: BTreeMap::new(),
        result: signature.result,
        entry,
        loops: 0,
        switches: 0,
        depth: 0,
    };
    for (param, ty) in declaration.function.params.iter().zip(&signature.params) {
        let ast::Pat::Ident(binding) = &param.pat else {
            return Err(source.locate(unsupported("parameter pattern"), param.span()));
        };
        lower
            .declare(binding.id.sym.as_ref(), Some(*ty), true, true)
            .map_err(|e| source.locate(e, binding.span()))?;
    }
    let body = declaration
        .function
        .body
        .as_ref()
        .ok_or_else(|| source.locate(invalid("missing function body"), declaration.span()))?;
    let statements = lower.block(&body.stmts, false)?;
    if signature.result != Type::Void && !returns(&statements) {
        return Err(source.locate(
            invalid("value-returning function must return on every path"),
            declaration.ident.span,
        ));
    }
    Ok(ir::Function {
        source: source.clone(),
        span: declaration.span(),
        params: signature.params.clone(),
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
