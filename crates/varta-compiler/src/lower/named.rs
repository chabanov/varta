//! Named interface/alias annotations (v31). Declarations are resolved lazily in
//! their defining module and memoized; a cycle is rejected rather than unrolled.
use super::*;
use crate::modules::{Export, Graph};
use std::path::{Path, PathBuf};

/// A type namespace entry: a local declaration or a binding to another module's.
#[derive(Clone)]
enum Declaration {
    Local {
        ty: Box<ast::TsType>,
        source: Source,
        span: swc_common::Span,
    },
    Imported(Key),
}
type Key = (usize, String);

#[derive(Default)]
pub(crate) struct NamedTypes {
    /// Index of the module whose annotations are being lowered.
    current: usize,
    declarations: BTreeMap<Key, Declaration>,
    resolved: BTreeMap<Key, Type>,
    resolving: Vec<Key>,
}

/// Exported type names of one module, pointing at the exporting module's entry
/// (or, for a re-export, at the source module's exported entry).
pub(super) type Exports = exports::Table<Key>;

pub(super) fn enter(types: &mut ir::ValueTypes, module: usize) {
    types.named.current = module;
}

/// Record local declarations and type imports of `path`, then return its type
/// exports. Dependencies precede dependents in `graph.order`.
pub(super) fn link_module(
    graph: &Graph,
    path: &PathBuf,
    module: usize,
    exports: &BTreeMap<PathBuf, Exports>,
    types: &mut ir::ValueTypes,
) -> Result<Exports> {
    let unit = &graph.units[path];
    let source = &unit.source;
    for item in &unit.ast.body {
        let declaration = match item {
            ast::ModuleItem::Stmt(ast::Stmt::Decl(decl)) => decl,
            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) => &export.decl,
            _ => continue,
        };
        let (name, span, ty) =
            match declaration {
                ast::Decl::TsInterface(interface) => {
                    if interface.type_params.is_some() {
                        return Err(source
                            .locate(unsupported("generic type declaration"), interface.id.span));
                    }
                    if !interface.extends.is_empty() {
                        return Err(source
                            .locate(unsupported("interface extends"), interface.extends[0].span));
                    }
                    let literal = ast::TsType::TsTypeLit(ast::TsTypeLit {
                        span: interface.body.span,
                        members: interface.body.body.clone(),
                    });
                    (&interface.id, interface.span, literal)
                }
                ast::Decl::TsTypeAlias(alias) => {
                    if alias.type_params.is_some() {
                        return Err(
                            source.locate(unsupported("generic type declaration"), alias.id.span)
                        );
                    }
                    (&alias.id, alias.span, (*alias.type_ann).clone())
                }
                _ => continue,
            };
        let declaration = Declaration::Local {
            ty: Box::new(ty),
            source: source.clone(),
            span,
        };
        if types
            .named
            .declarations
            .insert((module, name.sym.to_string()), declaration)
            .is_some()
        {
            return Err(source.locate(
                invalid(format!("duplicate type declaration: {}", name.sym)),
                name.span,
            ));
        }
    }
    for import in &unit.imports {
        let dependency = &exports[&import.target];
        for binding in &import.bindings {
            let key = match dependency.lookup(&binding.imported) {
                exports::Lookup::Bound(key) => key,
                exports::Lookup::Ambiguous => {
                    return Err(source.locate(
                        exports::ambiguous(&import.target, &binding.imported),
                        binding.span,
                    ));
                }
                exports::Lookup::Missing if binding.type_only => {
                    return Err(source.locate(
                        missing_type(&import.target, &binding.imported),
                        binding.span,
                    ));
                }
                exports::Lookup::Missing => continue,
            };
            let local = (module, binding.local.clone());
            if types
                .named
                .declarations
                .insert(local, Declaration::Imported(key.clone()))
                .is_some()
            {
                return Err(source.locate(
                    CompilerError::Module(format!("duplicate type binding: {}", binding.local)),
                    binding.span,
                ));
            }
        }
    }
    exports::link(unit, &Types { module, types }, exports)
}

fn missing_type(target: &Path, name: &str) -> CompilerError {
    CompilerError::Module(format!(
        "module {} does not export type {name}",
        target.display()
    ))
}

/// The type namespace of one module for [`exports::link`].
struct Types<'a> {
    module: usize,
    types: &'a ir::ValueTypes,
}
impl exports::Namespace for Types<'_> {
    type Binding = Key;
    /// The defining declaration after following import bindings.
    type Identity = Key;
    fn local(&self, export: &Export) -> Result<Option<Key>> {
        let key = (self.module, export.local.clone());
        if self.types.named.declarations.contains_key(&key) {
            Ok(Some(key))
        } else if export.type_only {
            Err(CompilerError::Module(format!(
                "export {} is not a type binding",
                export.local
            )))
        } else {
            Ok(None)
        }
    }
    fn admits(&self, _: &Export) -> bool {
        true
    }
    fn missing(&self, export: &Export, target: &Path) -> Result<()> {
        // A value re-export may name a function; function linking checks it.
        if export.type_only {
            return Err(missing_type(target, &export.local));
        }
        Ok(())
    }
    fn identity(&self, key: &Key) -> Key {
        let mut key = key;
        while let Some(Declaration::Imported(target)) = self.types.named.declarations.get(key) {
            key = target;
        }
        key.clone()
    }
}

/// Whether `name` is a type visible in the current module (used to accept type
/// exports and imports that are not function bindings).
pub(super) fn is_type(types: &ir::ValueTypes, module: usize, name: &str) -> bool {
    types
        .named
        .declarations
        .contains_key(&(module, name.to_owned()))
}

/// Whether the current module declares or imports `name`. Like TypeScript, a
/// module-local type shadows a global built-in type name (not its value).
pub(super) fn declared(types: &ir::ValueTypes, name: &str) -> bool {
    is_type(types, types.named.current, name)
}

/// Resolve a non-generic named reference in the current module.
pub(super) fn resolve(name: &str, types: &mut ir::ValueTypes) -> Result<Type> {
    let mut key = (types.named.current, name.to_owned());
    // Follow import bindings to the defining declaration; exports point at
    // already-linked modules, so the chain is acyclic and finite.
    while let Some(Declaration::Imported(target)) = types.named.declarations.get(&key) {
        key = target.clone();
    }
    if let Some(ty) = types.named.resolved.get(&key) {
        return Ok(*ty);
    }
    let Some(Declaration::Local { ty, source, span }) = types.named.declarations.get(&key).cloned()
    else {
        return Err(unsupported(&format!("type reference {name}")));
    };
    if types.named.resolving.contains(&key) {
        return Err(source.locate(unsupported("recursive type declaration"), span));
    }
    let caller = types.named.current;
    types.named.resolving.push(key.clone());
    types.named.current = key.0;
    let result = keyword_type(&ty, types).map_err(|e| source.locate(e, span));
    types.named.current = caller;
    types.named.resolving.pop();
    let ty = result?;
    types.named.resolved.insert(key, ty);
    Ok(ty)
}
