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
        /// Type parameters of a v32 generic declaration.
        params: Option<Box<ast::TsTypeParamDecl>>,
    },
    Imported(Key),
    /// A v32 class: its instance type is nominal and needs no lazy resolution.
    Class(u32),
    /// A v32 interface with `extends` clauses or merged declarations.
    Interface(Box<interfaces::Interface>),
}
type Key = (usize, String);

#[derive(Default)]
pub(crate) struct NamedTypes {
    /// Index of the module whose annotations are being lowered.
    current: usize,
    declarations: BTreeMap<Key, Declaration>,
    /// Resolved declarations by type arguments (empty for non-generic ones).
    resolved: BTreeMap<(Key, Vec<Type>), Type>,
    resolving: Vec<Key>,
    /// Reserved types of declarations that referred to themselves while being
    /// resolved (v34 `recursive_types`), filled when their resolution ends.
    placeholders: BTreeMap<Key, Type>,
}

/// Exported type names of one module, pointing at the exporting module's entry
/// (or, for a re-export, at the source module's exported entry).
pub(super) type Exports = exports::Table<Key>;

pub(super) fn enter(types: &mut ir::ValueTypes, module: usize) {
    types.named.current = module;
}
pub(super) fn current(types: &ir::ValueTypes) -> usize {
    types.named.current
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
    let mut merged = interfaces::Merged::new();
    for item in &unit.ast.body {
        // Exported classes are scanned as module exports and linked below.
        if let Some((name, id)) = classes::register(item, module, source, types)? {
            let key = (module, name.clone());
            if types
                .named
                .declarations
                .insert(key.clone(), Declaration::Class(id))
                .is_some()
            {
                return Err(source.locate(
                    invalid(format!("duplicate type declaration: {name}")),
                    types.classes.span(id),
                ));
            }
            continue;
        }
        let declaration = match item {
            ast::ModuleItem::Stmt(ast::Stmt::Decl(decl)) => decl,
            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) => &export.decl,
            _ => continue,
        };
        let (name, span, ty, params) =
            match declaration {
                ast::Decl::TsInterface(interface) => {
                    if interface.type_params.is_some() && !types.profile.generics() {
                        return Err(source
                            .locate(unsupported("generic type declaration"), interface.id.span));
                    }
                    if types.profile.interface_extends() {
                        let exported = matches!(item, ast::ModuleItem::ModuleDecl(_));
                        interfaces::collect(&mut merged, interface, exported, source)?;
                        continue;
                    }
                    if !interface.extends.is_empty() {
                        return Err(source
                            .locate(unsupported("interface extends"), interface.extends[0].span));
                    }
                    let literal = ast::TsType::TsTypeLit(ast::TsTypeLit {
                        span: interface.body.span,
                        members: interface.body.body.clone(),
                    });
                    (
                        &interface.id,
                        interface.span,
                        literal,
                        interface.type_params.clone(),
                    )
                }
                ast::Decl::TsTypeAlias(alias) => {
                    if alias.type_params.is_some() && !types.profile.generics() {
                        return Err(
                            source.locate(unsupported("generic type declaration"), alias.id.span)
                        );
                    }
                    (
                        &alias.id,
                        alias.span,
                        (*alias.type_ann).clone(),
                        alias.type_params.clone(),
                    )
                }
                _ => continue,
            };
        let declaration = Declaration::Local {
            ty: Box::new(ty),
            source: source.clone(),
            span,
            params,
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
    for (name, interface) in merged {
        declare_interface(module, name, interface, types)?;
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

/// Register the merged declarations of one interface name (v32).
fn declare_interface(
    module: usize,
    name: String,
    interface: interfaces::Interface,
    types: &mut ir::ValueTypes,
) -> Result<()> {
    let source = interface.source().clone();
    let span = interface.span();
    let declaration = match interface.plain() {
        Some(ty) => Declaration::Local {
            ty: Box::new(ty),
            source: source.clone(),
            span,
            params: interface.params().cloned().map(Box::new),
        },
        None => Declaration::Interface(Box::new(interface)),
    };
    let message = match types.named.declarations.get(&(module, name.clone())) {
        None => {
            types.named.declarations.insert((module, name), declaration);
            return Ok(());
        }
        Some(Declaration::Class(_)) => unsupported(&format!(
            "declaration merging of interface {name} with class {name} \
             (merged members would be class fields its constructor never initializes)"
        )),
        Some(_) => invalid(format!("duplicate type declaration: {name}")),
    };
    Err(source.locate(message, span))
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

/// The class named `name` in the current module's type namespace, if any.
pub(super) fn class(types: &ir::ValueTypes, name: &str) -> Option<u32> {
    let mut key = (types.named.current, name.to_owned());
    while let Some(Declaration::Imported(target)) = types.named.declarations.get(&key) {
        key = target.clone();
    }
    match types.named.declarations.get(&key) {
        Some(Declaration::Class(id)) => Some(*id),
        _ => None,
    }
}

/// The defining module, type parameters and body patterns of the generic
/// declaration `name` visible in the current module (v32 inference through
/// named types). An interface with bases yields its own members and each base
/// reference; inferred arguments are checked against the instantiated type.
pub(super) fn generic_declaration(
    types: &ir::ValueTypes,
    name: &str,
) -> Option<(usize, Box<ast::TsTypeParamDecl>, Vec<ast::TsType>)> {
    let mut key = (types.named.current, name.to_owned());
    while let Some(Declaration::Imported(target)) = types.named.declarations.get(&key) {
        key = target.clone();
    }
    match types.named.declarations.get(&key)? {
        Declaration::Local {
            ty,
            params: Some(params),
            ..
        } => Some((key.0, params.clone(), vec![(**ty).clone()])),
        Declaration::Interface(interface) => interface
            .generic_patterns()
            .map(|(params, bodies)| (key.0, params, bodies)),
        _ => None,
    }
}

/// Resolve a named reference without type arguments in the current module.
pub(super) fn resolve(name: &str, types: &mut ir::ValueTypes) -> Result<Type> {
    instantiate(name, None, types)
}

/// Resolve a named reference in the current module; v32 type arguments are
/// lowered here, in the referencing module, and bound in the defining one.
pub(super) fn instantiate(
    name: &str,
    args: Option<&ast::TsTypeParamInstantiation>,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let mut key = (types.named.current, name.to_owned());
    // Follow import bindings to the defining declaration; exports point at
    // already-linked modules, so the chain is acyclic and finite.
    while let Some(Declaration::Imported(target)) = types.named.declarations.get(&key) {
        key = target.clone();
    }
    if let Some(Declaration::Class(id)) = types.named.declarations.get(&key) {
        let id = *id;
        // A generic class (v37) is instantiated per argument list.
        if types.classes.get(id).is_template() {
            let lowered = match args {
                Some(args) => generics::arguments(args, types)?
                    .into_iter()
                    .map(Some)
                    .collect(),
                None => Vec::new(),
            };
            return classes::instantiate_class(id, &lowered, generics::Missing::Reference, types)
                .map(Type::Class);
        }
        if args.is_some() {
            return Err(unsupported("generic class type reference"));
        }
        return Ok(Type::Class(id));
    }
    let declaration = types.named.declarations.get(&key).cloned();
    let (source, span, params) = match &declaration {
        Some(Declaration::Local {
            source,
            span,
            params,
            ..
        }) => (source.clone(), *span, params.as_deref().cloned()),
        Some(Declaration::Interface(interface)) => (
            interface.source().clone(),
            interface.span(),
            interface.params().cloned(),
        ),
        _ => return Err(unsupported(&format!("type reference {name}"))),
    };
    if args.is_some() && params.is_none() {
        return Err(type_error(format!(
            "type {name} is not generic and takes no type arguments"
        )));
    }
    let arguments = match args {
        Some(args) => generics::arguments(args, types)?,
        None => Vec::new(),
    };
    let memo = (key.clone(), arguments);
    if let Some(ty) = types.named.resolved.get(&memo) {
        return Ok(*ty);
    }
    if types.named.resolving.contains(&key) {
        if types.profile.recursive_types() && memo.1.is_empty() && params.is_none() {
            return recursive::placeholder(&key, declaration.as_ref(), types)
                .map_err(|e| source.locate(e, span));
        }
        return Err(source.locate(unsupported("recursive type declaration"), span));
    }
    let caller = types.named.current;
    types.named.resolving.push(key.clone());
    types.named.current = key.0;
    let result = match &declaration {
        Some(Declaration::Interface(interface)) => {
            resolve_parameters((params.as_ref(), &memo.1), name, &source, types).and_then(|frame| {
                generics::with_frame(types, frame, |t| interface.resolve(t))
                    .map_err(|e| source.locate(e, span))
            })
        }
        Some(Declaration::Local { ty, .. }) => {
            resolve_local((params.as_ref(), &memo.1), name, ty, (&source, span), types)
        }
        _ => Err(unsupported(&format!("type reference {name}"))),
    };
    types.named.current = caller;
    types.named.resolving.pop();
    let placeholder = types.named.placeholders.remove(&key);
    let ty = result?;
    let ty = match placeholder {
        Some(placeholder) => {
            recursive::fill(placeholder, ty, types).map_err(|e| source.locate(e, span))?
        }
        None => ty,
    };
    types.named.resolved.insert(memo, ty);
    Ok(ty)
}

/// Bind type parameters (errors stay at the reference), then lower the body
/// in the defining module with only those parameters visible.
fn resolve_local(
    (params, args): (Option<&ast::TsTypeParamDecl>, &[Type]),
    name: &str,
    body: &ast::TsType,
    (source, span): (&Source, swc_common::Span),
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let frame = resolve_parameters((params, args), name, source, types)?;
    generics::declaration_body(frame, body, types).map_err(|e| source.locate(e, span))
}

/// Bind the type parameters of a named declaration (none for a non-generic one).
fn resolve_parameters(
    (params, args): (Option<&ast::TsTypeParamDecl>, &[Type]),
    name: &str,
    source: &Source,
    types: &mut ir::ValueTypes,
) -> Result<generics::Frame> {
    Ok(match params {
        Some(params) => {
            let args: Vec<_> = args.iter().copied().map(Some).collect();
            let what = format!("type {name}");
            generics::bind(
                params,
                &args,
                &what,
                generics::Missing::Reference,
                source,
                types,
            )?
        }
        None => generics::Frame::new(),
    })
}

/// Recursive named types (v34 `recursive_types`): a declaration that refers to
/// itself gets a reserved type of the kind its body declares (a record for an
/// interface or object type, a dictionary for an index signature, a reference
/// union for a union); references resolve to it, and the finished body fills
/// it. A cycle must pass through a record field, array element or dictionary
/// value; `type A = B; type B = A`, `type A = A | string` and recursive generic
/// declarations stay rejected.
mod recursive {
    use super::*;
    use crate::ir::{DictionaryType, RecordField, RecordShape};

    /// Field name of a reserved record shape; no source key can equal it, so
    /// structural interning never matches the reservation.
    const RESERVED: &str = "\u{0}recursive";
    fn rejected() -> CompilerError {
        unsupported(
            "recursive type declaration that is not productive through a record field, array element or dictionary value",
        )
    }
    enum Kind {
        Record,
        Dictionary,
        Union,
    }
    fn kind(ty: &ast::TsType) -> Option<Kind> {
        match ty {
            ast::TsType::TsParenthesizedType(inner) => kind(&inner.type_ann),
            ast::TsType::TsTypeLit(literal)
                if literal
                    .members
                    .iter()
                    .any(|m| matches!(m, ast::TsTypeElement::TsIndexSignature(_))) =>
            {
                Some(Kind::Dictionary)
            }
            ast::TsType::TsTypeLit(_) => Some(Kind::Record),
            ast::TsType::TsUnionOrIntersectionType(
                ast::TsUnionOrIntersectionType::TsUnionType(_),
            ) => Some(Kind::Union),
            _ => None,
        }
    }
    pub(super) fn placeholder(
        key: &Key,
        declaration: Option<&Declaration>,
        types: &mut ir::ValueTypes,
    ) -> Result<Type> {
        if let Some(ty) = types.named.placeholders.get(key) {
            return Ok(*ty);
        }
        let kind = match declaration {
            Some(Declaration::Interface(interface)) if interface.declares_index() => {
                Kind::Dictionary
            }
            Some(Declaration::Interface(_)) => Kind::Record,
            Some(Declaration::Local { ty, .. }) => kind(ty).ok_or_else(rejected)?,
            _ => return Err(rejected()),
        };
        let ty = match kind {
            Kind::Record => {
                if types.records.len() >= varta_common::value::MAX_RECORD_SHAPES {
                    return Err(invalid("object shape count exceeds limit"));
                }
                types.records.push(RecordShape {
                    fields: vec![RecordField {
                        name: RESERVED.into(),
                        ty: Type::Null,
                        optional: false,
                    }],
                    class: None,
                });
                Type::Record(types.records.len() as u32 - 1)
            }
            Kind::Dictionary => {
                types.dictionaries.push(DictionaryType {
                    value: Type::Void,
                    keys: Type::Void,
                    values: Type::Void,
                    storage: 0,
                    declared: None,
                });
                Type::Dictionary(types.dictionaries.len() as u32 - 1)
            }
            Kind::Union => {
                if types.unions.len() >= varta_common::value::MAX_UNION_TYPES {
                    return Err(invalid("union type count exceeds limit"));
                }
                // An empty member list marks the reservation (see unions::intern).
                types.unions.push(Vec::new());
                Type::ReferenceUnion(types.unions.len() as u32 - 1)
            }
        };
        types.named.placeholders.insert(key.clone(), ty);
        Ok(ty)
    }
    /// Copy the resolved body into the reservation; a just-interned duplicate
    /// that nothing else refers to is dropped.
    pub(super) fn fill(
        placeholder: Type,
        resolved: Type,
        types: &mut ir::ValueTypes,
    ) -> Result<Type> {
        match (placeholder, resolved) {
            (Type::Record(p), Type::Record(r)) if p != r => {
                types.records[p as usize] = types.records[r as usize].clone();
                if r as usize + 1 == types.records.len() {
                    types.records.pop();
                }
            }
            (Type::Dictionary(p), Type::Dictionary(d)) if p != d => {
                types.dictionaries[p as usize] = types.dictionaries[d as usize];
                if d as usize + 1 == types.dictionaries.len() {
                    types.dictionaries.pop();
                }
            }
            (Type::ReferenceUnion(p), Type::ReferenceUnion(u)) if p != u => {
                types.unions[p as usize] = types.unions[u as usize].clone();
                let literal = literals::copy_erased_union(u, p, types);
                if u as usize + 1 == types.unions.len() && !literal {
                    types.unions.pop();
                }
            }
            _ => return Err(rejected()),
        }
        Ok(placeholder)
    }
}
