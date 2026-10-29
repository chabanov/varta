//! Assign global function slots, then resolve module-local bindings against
//! dependency exports. Function bodies are lowered with their own source/scope.
use super::*;
use crate::modules::{Export, Graph};
use std::path::{Path, PathBuf};

const MAX_FUNCTIONS: usize = varta_common::abi::MAX_USER_FUNCTIONS;
pub(super) type Symbols = BTreeMap<String, Signature>;
/// Names that no module binding or import may take.
const RESERVED_BINDINGS: [&str; 4] = ["varta", "console", "log", "JSON"];

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
    let mut type_exports: BTreeMap<PathBuf, named::Exports> = BTreeMap::new();
    for (module, path) in graph.order.iter().enumerate() {
        let unit = &graph.units[path];
        if profile.named_types() {
            named::enter(&mut types, module);
            let exports = named::link_module(graph, path, module, &type_exports, &mut types)?;
            type_exports.insert(path.clone(), exports);
        }
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
        for declaration in declarations::collect(&unit.ast, &unit.source, profile)? {
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
            if RESERVED_BINDINGS.contains(&name.as_str())
                || symbols.insert(name.clone(), signature).is_some()
            {
                return Err(unit.source.locate(
                    invalid(format!("duplicate or reserved function name: {name}")),
                    declaration.ident.span,
                ));
            }
            next += 1;
        }
        if profile.classes() {
            classes::layout_module(module, &symbols, &mut types, &mut next)?;
        }
        definitions.insert(path.clone(), symbols);
    }
    if profile.classes() {
        classes::dispatchers(&mut types, &mut next)?;
    }
    let entry = &graph.units[&graph.entry];
    let main = main.ok_or_else(|| {
        entry.source.locate(
            invalid("entry module must declare a top-level function named main()"),
            entry.ast.span(),
        )
    })?;
    let mut generated = BTreeMap::new();
    let mut call_effects = BTreeMap::new();
    let (scopes, constants) = link_values(
        graph,
        &definitions,
        &type_exports,
        (&mut types, main),
        (&mut generated, &mut next, &mut call_effects),
    )?;
    let mut functions = Vec::new();
    for (module, path) in graph.order.iter().enumerate() {
        let unit = &graph.units[path];
        named::enter(&mut types, module);
        let visible = constants.scopes.get(path).cloned().unwrap_or_default();
        for declaration in declarations::collect(&unit.ast, &unit.source, profile)? {
            let signature = &definitions[path][declaration.ident.sym.as_ref()];
            functions.push(body(
                declaration,
                (signature, &visible),
                &scopes[path],
                &unit.source,
                main,
                &mut types,
                (&mut generated, &mut next, &mut call_effects),
            )?);
        }
        if profile.classes() {
            classes::bodies(
                module,
                (&scopes[path], &visible),
                main,
                &mut types,
                &mut functions,
                (&mut generated, &mut next, &mut call_effects),
            )?;
        }
    }
    if profile.classes() {
        classes::dispatch_bodies(&types, &mut functions)?;
    }
    functions.extend(generated.into_values());
    // Initialize stored constants in dependency order before the entry body.
    functions[main].body.splice(
        0..0,
        constants.initializers.iter().map(|index| {
            S::Expr(Expr {
                span: entry.ast.span(),
                ty: Type::Void,
                kind: E::Call(*index, vec![]),
            })
        }),
    );
    let tools = super::tools::collect(graph, &scopes, &functions, main)?;
    let http = http::collect(graph, &scopes, &mut functions, &mut types, main)?;
    if constants.stored > 0 && (!tools.is_empty() || http.is_some()) {
        return Err(entry.source.locate(
            unsupported("module constants with tool/HTTP entry points; only main initializes them"),
            entry.ast.span(),
        ));
    }
    if profile.binary64() && (!tools.is_empty() || http.is_some()) {
        return Err(entry.source.locate(
            unsupported("tool/HTTP contracts in binary64 profile; adapters are not implemented"),
            entry.ast.span(),
        ));
    }
    let mut program = ir::Program {
        functions,
        main,
        tools,
        http,
        types,
        constants: constants.stored,
        initializers: constants.initializers,
        suspendable: BTreeMap::new(),
    };
    if profile.literal_types() {
        literal_erasure::erase_program(&mut program);
    }
    super::suspension::prepare(&mut program, graph)?;
    Ok(program)
}

/// One binding of the value namespace, shared by functions and (v32) module
/// constants: one module cannot export a function and a constant under one
/// name, and star re-exports compare both kinds as one namespace.
#[derive(Clone)]
enum Value {
    Function(Signature),
    Constant(module_constants::Constant),
}
#[derive(PartialEq, Eq)]
enum ValueIdentity {
    /// Function slots are unique across the graph.
    Function(usize),
    /// Constant declarations are unique across the graph.
    Constant(usize),
}

/// Value bindings, module by module in dependency order: resolve value imports
/// against dependency export tables, link the module's constants, then compute
/// its value export table. Type bindings were linked by `named::link_module`.
fn link_values(
    graph: &Graph,
    definitions: &BTreeMap<PathBuf, Symbols>,
    type_exports: &BTreeMap<PathBuf, named::Exports>,
    (types, entry): (&mut ir::ValueTypes, usize),
    synthesized: (
        &mut BTreeMap<usize, ir::Function>,
        &mut usize,
        &mut BTreeMap<usize, flow_calls::Impact>,
    ),
) -> Result<(BTreeMap<PathBuf, Symbols>, module_constants::Linked)> {
    let (generated, next, call_effects) = synthesized;
    let mut scopes = BTreeMap::new();
    let mut constants = module_constants::Linked::default();
    let mut tables: BTreeMap<PathBuf, exports::Table<Value>> = BTreeMap::new();
    for (module, path) in graph.order.iter().enumerate() {
        let unit = &graph.units[path];
        named::enter(types, module);
        let mut scope = definitions[path].clone();
        let mut imported = module_constants::Scope::new();
        let mut function_imports = Vec::new();
        for import in &unit.imports {
            for binding in import.bindings.iter().filter(|b| !b.type_only) {
                let found = tables
                    .get(&import.target)
                    .map_or(exports::Lookup::Missing, |t| t.lookup(&binding.imported));
                let fresh = match found {
                    exports::Lookup::Bound(Value::Function(signature)) => {
                        function_imports.push(binding);
                        scope
                            .insert(binding.local.clone(), signature.clone())
                            .is_none()
                            && !imported.contains_key(&binding.local)
                    }
                    exports::Lookup::Bound(Value::Constant(constant)) => {
                        !scope.contains_key(&binding.local)
                            && imported
                                .insert(binding.local.clone(), constant.clone())
                                .is_none()
                    }
                    exports::Lookup::Ambiguous => {
                        return Err(unit.source.locate(
                            exports::ambiguous(&import.target, &binding.imported),
                            binding.span,
                        ));
                    }
                    exports::Lookup::Missing => {
                        other_namespace(type_exports, &import.target, &binding.imported, types)
                            .map_err(|e| unit.source.locate(e, binding.span))?;
                        continue;
                    }
                };
                if !fresh || RESERVED_BINDINGS.contains(&binding.local.as_str()) {
                    return Err(unit
                        .source
                        .locate(duplicate_import(&binding.local), binding.span));
                }
            }
        }
        module_constants::link_module(
            (unit, path),
            &definitions[path],
            imported,
            (types, entry),
            (generated, next, call_effects),
            &mut constants,
        )?;
        let empty = module_constants::Scope::new();
        let visible = constants.scopes.get(path).unwrap_or(&empty);
        // A declared constant may not take the name of an imported function.
        if let Some(binding) = function_imports
            .iter()
            .find(|binding| visible.contains_key(&binding.local))
        {
            return Err(unit
                .source
                .locate(duplicate_import(&binding.local), binding.span));
        }
        let values = Values {
            functions: &scope,
            constants: visible,
            module,
            types,
            type_exports,
        };
        let table = exports::link(unit, &values, &tables)?;
        tables.insert(path.clone(), table);
        scopes.insert(path.clone(), scope);
    }
    Ok((scopes, constants))
}

fn duplicate_import(name: &str) -> CompilerError {
    CompilerError::Module(format!("duplicate or reserved import binding: {name}"))
}

/// A name missing from a dependency's value exports must be one of its types.
fn other_namespace(
    type_exports: &BTreeMap<PathBuf, named::Exports>,
    target: &Path,
    name: &str,
    types: &ir::ValueTypes,
) -> Result<()> {
    match type_exports.get(target).map(|e| e.lookup(name)) {
        Some(exports::Lookup::Bound(_)) => Ok(()),
        Some(exports::Lookup::Ambiguous) => Err(exports::ambiguous(target, name)),
        Some(exports::Lookup::Missing) | None => Err(CompilerError::Module(format!(
            "module {} does not export function {}{name}",
            target.display(),
            if types.profile.module_constants() {
                "or constant "
            } else {
                ""
            },
        ))),
    }
}

/// The value namespace of one module for [`exports::link`].
struct Values<'a> {
    functions: &'a Symbols,
    constants: &'a module_constants::Scope,
    module: usize,
    types: &'a ir::ValueTypes,
    type_exports: &'a BTreeMap<PathBuf, named::Exports>,
}
impl exports::Namespace for Values<'_> {
    type Binding = Value;
    type Identity = ValueIdentity;
    fn local(&self, export: &Export) -> Result<Option<Value>> {
        if export.type_only {
            return Ok(None);
        }
        if let Some(signature) = self.functions.get(&export.local) {
            return Ok(Some(Value::Function(signature.clone())));
        }
        if let Some(constant) = self.constants.get(&export.local) {
            return Ok(Some(Value::Constant(constant.clone())));
        }
        if named::is_type(self.types, self.module, &export.local) {
            return Ok(None);
        }
        Err(CompilerError::Module(format!(
            "export {} is not a function binding",
            export.local
        )))
    }
    fn admits(&self, export: &Export) -> bool {
        !export.type_only
    }
    fn missing(&self, export: &Export, target: &Path) -> Result<()> {
        other_namespace(self.type_exports, target, &export.local, self.types)
    }
    fn identity(&self, value: &Value) -> ValueIdentity {
        match value {
            Value::Function(signature) => ValueIdentity::Function(signature.index),
            Value::Constant(constant) => ValueIdentity::Constant(constant.id()),
        }
    }
}

pub(super) fn body(
    declaration: &ast::FnDecl,
    (signature, constants): (&Signature, &module_constants::Scope),
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
    let mut prologue = lower.module_constants(constants, &declaration.function)?;
    prologue.extend(lower.initialize_parameters(&plans, signature.receiver.is_some())?);
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
