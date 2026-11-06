//! Module-level enums and namespaces: structure, hoisting and rewriting.
//!
//! The structure pass evaluates every enum and collects the exported
//! members of every namespace (merged over all of its declarations) before
//! any body is rewritten, since an exported member is visible unqualified
//! in every declaration of its namespace. The emission pass replaces each
//! enum with its type alias and each namespace with its hoisted members,
//! rewriting every body against the frames of its enclosing namespaces.
use super::rewrite::{self, Frame};
use super::{
    Enum, Exports, Member, Namespace, Symbol, declaration, enums, generated, statics, unsupported,
};
use crate::CompilerError;
use crate::source::Source;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use swc_common::{Span, Spanned};
use swc_ecma_ast as ast;

type Path = Vec<String>;

/// An exported namespace member while the structure is collected.
#[derive(Default)]
struct Entry {
    name: String,
    value: bool,
    enumeration: Option<Rc<Enum>>,
    namespace: bool,
    /// The declaration kind, for duplicate checks.
    kinds: Vec<Kind>,
    /// A static class field.
    field: Option<super::statics::Field>,
    /// Creation order among the namespace's members (declaration order).
    position: usize,
    /// How the namespace object's type names a constant member: `Some(None)`
    /// through `typeof` of its annotated declaration, `Some(Some(t))` as the
    /// keyword type of its literal initializer, `None` not at all.
    annotation: Option<Option<Box<ast::TsType>>>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Function,
    Class,
    Variable,
    /// A `let` or `var` member: a live binding, so no namespace object.
    Mutable,
    Interface,
    Alias,
    Enum,
    Namespace,
}

#[derive(Default)]
struct Structure {
    /// Exported members by namespace path; the empty path is the module.
    namespaces: BTreeMap<Path, BTreeMap<String, Entry>>,
    /// Module-level enums by name.
    enums: BTreeMap<String, Rc<Enum>>,
}

pub(super) fn lower(module: &mut ast::Module, source: &Source) -> Result<Exports, CompilerError> {
    let mut structure = Structure::default();
    collect_module(&module.body, source, &mut structure)?;
    let classes = statics::collect(&module.body, source)?;
    add_statics(&classes, &mut structure);
    let parents: BTreeMap<String, String> = classes
        .iter()
        .filter_map(|(name, c)| c.parent.clone().map(|p| (name.clone(), p)))
        .collect();
    let context = rewrite::Context { parents: &parents };
    let references = statics::references(&module.body);
    let frame = module_frame(&module.body, &structure);
    let mut body = Vec::new();
    let mut exports = Exports::default();
    // The last declaration of each module-level namespace, and the exported
    // namespaces (their objects are exported too).
    let mut last_namespace: BTreeMap<String, usize> = BTreeMap::new();
    let mut exported_namespaces: BTreeSet<String> = BTreeSet::new();
    for (position, item) in module.body.iter().enumerate() {
        if let Some((ast::Decl::TsModule(decl), exported)) = declaration(item)
            && let ast::TsModuleName::Ident(id) = &decl.id
        {
            last_namespace.insert(id.sym.to_string(), position);
            if exported {
                exported_namespaces.insert(id.sym.to_string());
            }
        }
    }
    for (position, item) in std::mem::take(&mut module.body).into_iter().enumerate() {
        let span = item.span();
        let Some((decl, exported)) = declaration(&item) else {
            body.push(rewrite::item(item, source, &[&frame])?);
            continue;
        };
        match decl {
            ast::Decl::TsEnum(decl) => {
                let name = decl.id.sym.to_string();
                if exported && let Some(symbol) = frame.get(&name) {
                    exports.symbols.insert(name.clone(), symbol.clone());
                }
                body.extend(enums::items(
                    &structure.enums[&name],
                    exported,
                    span,
                    source,
                )?);
            }
            ast::Decl::TsModule(decl) => {
                let (name, inner) = module_parts(decl, source)?;
                if exported && let Some(symbol) = frame.get(&name) {
                    exports.symbols.insert(name.clone(), symbol.clone());
                }
                let mut emitter = Emitter {
                    source,
                    structure: &structure,
                    items: &mut body,
                };
                emitter.namespace(vec![name.clone()], inner, exported, &[&frame])?;
                // After its last declaration, a namespace without a value of
                // its own gets its object (pruned when no code reads it).
                let pure = frame
                    .get(&name)
                    .is_some_and(|s| !s.value && s.enumeration.is_none());
                if pure && last_namespace.get(&name) == Some(&position) {
                    for decl in object_declarations(std::slice::from_ref(&name), &structure, span) {
                        body.push(item_of(decl, exported_namespaces.contains(&name), span));
                    }
                }
            }
            _ => {
                if exported
                    && let Some(name) = declared_name(decl)
                    && let Some(symbol) = frame.get(&name)
                {
                    exports.symbols.insert(name.clone(), symbol.clone());
                }
                if let ast::Decl::Class(class) = decl
                    && classes.contains_key(class.ident.sym.as_ref())
                {
                    let name = class.ident.sym.to_string();
                    let mut class = class.clone();
                    let variables = statics::lower_class(
                        &mut class.class,
                        (&name, exported),
                        &classes,
                        &references,
                        source,
                    )?;
                    let item = item_of(ast::Decl::Class(class), exported, span);
                    body.push(rewrite::item_with(item, source, &[&frame], &context, None)?);
                    for variable in variables {
                        let variable = rewrite::item_with(
                            item_of(variable, exported, span),
                            source,
                            &[&frame],
                            &context,
                            Some(&name),
                        )?;
                        body.push(variable);
                    }
                    continue;
                }
                body.push(rewrite::item_with(item, source, &[&frame], &context, None)?);
            }
        }
    }
    module.body = hoisted_statics(body, source, &mut exports)?;
    Ok(exports)
}

/// Whether a class has static members that `statics` lowers.
fn has_statics(class: &ast::Class) -> bool {
    class.body.iter().any(|member| match member {
        ast::ClassMember::ClassProp(p) => p.is_static,
        ast::ClassMember::AutoAccessor(a) => a.is_static,
        ast::ClassMember::StaticBlock(_) => true,
        ast::ClassMember::Method(m) => m.is_static,
        _ => false,
    })
}

/// Static members of classes declared in namespaces, which the namespace
/// pass hoisted under generated names: a second pass lowers them like those
/// of module-level classes, and `NS.C.x` (already `__vartaDecl$NS$C.x`)
/// resolves to the field variables. The exported namespace symbols gain the
/// class's fields.
fn hoisted_statics(
    body: Vec<ast::ModuleItem>,
    source: &Source,
    exports: &mut Exports,
) -> Result<Vec<ast::ModuleItem>, CompilerError> {
    let hoisted = |item: &ast::ModuleItem| match declaration(item) {
        Some((ast::Decl::Class(class), _)) => {
            class.ident.sym.starts_with(super::PREFIX) && has_statics(&class.class)
        }
        _ => false,
    };
    if !body.iter().any(hoisted) {
        return Ok(body);
    }
    let classes: statics::Classes = statics::collect(&body, source)?
        .into_iter()
        .filter(|(name, _)| name.starts_with(super::PREFIX))
        .collect();
    let mut frame = Frame::new();
    for (name, class) in &classes {
        if class.fields.is_empty() {
            continue;
        }
        let namespace = Rc::new(Namespace {
            object: None,
            members: class
                .fields
                .iter()
                .map(|(field, info, variable)| {
                    let mut symbol = Symbol::plain(variable.clone());
                    symbol.field = Some(info.clone());
                    (
                        field.clone(),
                        Member {
                            exported: true,
                            symbol,
                        },
                    )
                })
                .collect(),
        });
        frame.insert(
            name.clone(),
            Symbol {
                name: name.clone(),
                enumeration: None,
                namespace: Some(namespace.clone()),
                value: true,
                field: None,
            },
        );
        for symbol in exports.symbols.values_mut() {
            *symbol = with_namespace(symbol, name, &namespace);
        }
    }
    let parents: BTreeMap<String, String> = classes
        .iter()
        .filter_map(|(name, c)| c.parent.clone().map(|p| (name.clone(), p)))
        .collect();
    let context = rewrite::Context { parents: &parents };
    let references = statics::references(&body);
    let mut lowered = Vec::with_capacity(body.len());
    for item in body {
        let span = item.span();
        if let Some((ast::Decl::Class(class), exported)) = declaration(&item)
            && classes.contains_key(class.ident.sym.as_ref())
        {
            let name = class.ident.sym.to_string();
            let mut class = class.clone();
            let variables = statics::lower_class(
                &mut class.class,
                (&name, exported),
                &classes,
                &references,
                source,
            )?;
            let item = item_of(ast::Decl::Class(class), exported, span);
            lowered.push(rewrite::item_with(item, source, &[&frame], &context, None)?);
            for variable in variables {
                lowered.push(rewrite::item_with(
                    item_of(variable, exported, span),
                    source,
                    &[&frame],
                    &context,
                    Some(&name),
                )?);
            }
            continue;
        }
        lowered.push(rewrite::item_with(item, source, &[&frame], &context, None)?);
    }
    Ok(lowered)
}

/// `symbol` with the member named `name` (at any depth) given `namespace`.
fn with_namespace(symbol: &Symbol, name: &str, namespace: &Rc<Namespace>) -> Symbol {
    let mut symbol = symbol.clone();
    if symbol.name == name {
        symbol.namespace = Some(namespace.clone());
    } else if let Some(inner) = &symbol.namespace {
        symbol.namespace = Some(Rc::new(Namespace {
            object: inner.object.clone(),
            members: inner
                .members
                .iter()
                .map(|(key, member)| {
                    (
                        key.clone(),
                        Member {
                            exported: member.exported,
                            symbol: with_namespace(&member.symbol, name, namespace),
                        },
                    )
                })
                .collect(),
        }));
    }
    symbol
}

/// Static fields are members of their class's symbol, like the members of a
/// namespace merged with it.
fn add_statics(classes: &statics::Classes, structure: &mut Structure) {
    for (class, statics) in classes {
        if statics.fields.is_empty() {
            continue;
        }
        let module = structure.namespaces.entry(Vec::new()).or_default();
        let entry = module.entry(class.clone()).or_default();
        entry.name = class.clone();
        entry.namespace = true;
        entry.value = true;
        let members = structure.namespaces.entry(vec![class.clone()]).or_default();
        for (field, info, variable) in &statics.fields {
            let entry = members.entry(field.clone()).or_default();
            entry.name = variable.clone();
            entry.value = true;
            entry.kinds = vec![Kind::Variable];
            entry.field = Some(info.clone());
        }
    }
}

/// The name and body of a namespace declaration.
fn module_parts<'a>(
    decl: &'a ast::TsModuleDecl,
    source: &Source,
) -> Result<(String, &'a ast::TsNamespaceBody), CompilerError> {
    if decl.declare || decl.global {
        return Err(source.locate(
            unsupported("ambient namespace or module declaration"),
            decl.span,
        ));
    }
    let ast::TsModuleName::Ident(id) = &decl.id else {
        return Err(source.locate(unsupported("ambient module declaration"), decl.span));
    };
    let body = decl
        .body
        .as_ref()
        .ok_or_else(|| source.locate(unsupported("namespace without a body"), decl.span))?;
    Ok((id.sym.to_string(), body))
}

/// The statements of a namespace body; a dotted `namespace A.B {}` nests.
fn block_or_nested(
    body: &ast::TsNamespaceBody,
) -> Result<&[ast::ModuleItem], (&ast::TsNamespaceDecl, ())> {
    match body {
        ast::TsNamespaceBody::TsModuleBlock(block) => Ok(&block.body),
        ast::TsNamespaceBody::TsNamespaceDecl(nested) => Err((nested, ())),
    }
}

fn declared_name(decl: &ast::Decl) -> Option<String> {
    match decl {
        ast::Decl::Fn(f) => Some(f.ident.sym.to_string()),
        ast::Decl::Class(c) => Some(c.ident.sym.to_string()),
        ast::Decl::TsEnum(e) => Some(e.id.sym.to_string()),
        _ => None,
    }
}

/// Structure pass over the module's items.
fn collect_module(
    items: &[ast::ModuleItem],
    source: &Source,
    structure: &mut Structure,
) -> Result<(), CompilerError> {
    structure.namespaces.entry(Vec::new()).or_default();
    for item in items {
        let Some((decl, _)) = declaration(item) else {
            continue;
        };
        match decl {
            ast::Decl::TsEnum(decl) => {
                let name = decl.id.sym.to_string();
                if structure.enums.contains_key(&name) {
                    return Err(
                        source.locate(unsupported("merged enum declarations"), decl.id.span)
                    );
                }
                let enumeration = evaluate(decl, &name, &[], structure, source)?;
                structure.enums.insert(name, enumeration);
            }
            ast::Decl::TsModule(decl) => {
                let (name, body) = module_parts(decl, source)?;
                collect_namespace(vec![name], body, source, structure)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn evaluate(
    decl: &ast::TsEnumDecl,
    name: &str,
    path: &[String],
    structure: &Structure,
    source: &Source,
) -> Result<Rc<Enum>, CompilerError> {
    let segments: Vec<&str> = path
        .iter()
        .map(String::as_str)
        .chain([decl.id.sym.as_ref(), "", "name"])
        .collect();
    let lookup = |other: &str| -> Option<Symbol> {
        // Enums of the enclosing namespaces, innermost first, then the module.
        for depth in (0..=path.len()).rev() {
            let scope = &path[..depth];
            let mut hidden = scope.to_vec();
            hidden.push(String::from(HIDDEN));
            for key in [scope.to_vec(), hidden] {
                if let Some(entry) = structure
                    .namespaces
                    .get(&key)
                    .and_then(|members| members.get(other))
                    && let Some(enumeration) = &entry.enumeration
                {
                    return Some(enum_symbol(entry.name.clone(), enumeration.clone()));
                }
            }
        }
        structure
            .enums
            .get(other)
            .map(|e| enum_symbol(other.to_owned(), e.clone()))
    };
    let mut object = segments.clone();
    if let Some(last) = object.last_mut() {
        *last = "object";
    }
    enums::evaluate(
        decl,
        name,
        (generated(&segments), generated(&object)),
        source,
        &lookup,
    )
    .map(Rc::new)
}

fn enum_symbol(name: String, enumeration: Rc<Enum>) -> Symbol {
    Symbol {
        name,
        enumeration: Some(enumeration),
        namespace: None,
        value: false,
        field: None,
    }
}

/// Structure pass over one namespace declaration at `path`.
fn collect_namespace(
    path: Path,
    body: &ast::TsNamespaceBody,
    source: &Source,
    structure: &mut Structure,
) -> Result<(), CompilerError> {
    structure.namespaces.entry(path.clone()).or_default();
    let parent = path[..path.len() - 1].to_vec();
    add(
        &parent,
        &path,
        Kind::Namespace,
        structure,
        source,
        body.span(),
    )?;
    let items = match block_or_nested(body) {
        Ok(items) => items,
        Err((nested, ())) => {
            let mut inner = path.clone();
            inner.push(nested.id.sym.to_string());
            return collect_namespace(inner, &nested.body, source, structure);
        }
    };
    for item in items {
        let Some((decl, exported)) = declaration(item) else {
            continue;
        };
        if let ast::Decl::TsModule(decl) = decl {
            let (name, inner_body) = module_parts(decl, source)?;
            let mut inner = path.clone();
            inner.push(name);
            // An unexported nested namespace is still collected so its own
            // exported members resolve inside it, but it is no member.
            collect_namespace(inner.clone(), inner_body, source, structure)?;
            if !exported {
                remove(&path, &inner, structure);
            }
            continue;
        }
        let Some((name, kind)) = member(decl, source)? else {
            continue;
        };
        if let ast::Decl::TsEnum(decl) = decl {
            let segments: Vec<&str> = path
                .iter()
                .map(String::as_str)
                .chain([name.as_str()])
                .collect();
            let mangled = if exported {
                generated(&segments)
            } else {
                hidden(&path, &name)
            };
            let enumeration = evaluate(decl, &mangled, &path, structure, source)?;
            if exported {
                let entry = entry(&path, &name, structure);
                entry.enumeration = Some(enumeration);
                check_kind(entry, Kind::Enum, &name, source, decl.span)?;
            } else {
                hidden_enums(structure, &path, &name, enumeration);
            }
            continue;
        }
        if exported {
            let entry = entry(&path, &name, structure);
            entry.value |= matches!(
                kind,
                Kind::Function | Kind::Class | Kind::Variable | Kind::Mutable
            );
            entry.annotation = variable_type(decl);
            check_kind(entry, kind, &name, source, decl.span())?;
        }
    }
    Ok(())
}

/// Unexported enums of a namespace body, kept under a reserved key so that
/// later enum initializers of the body can reference them.
fn hidden_enums(structure: &mut Structure, path: &[String], name: &str, enumeration: Rc<Enum>) {
    let mut key = path.to_vec();
    key.push(String::from(HIDDEN));
    let members = structure.namespaces.entry(key).or_default();
    let entry = members.entry(name.to_owned()).or_default();
    entry.name = enumeration.name.clone();
    entry.enumeration = Some(enumeration);
}
/// A path segment no namespace name can take.
const HIDDEN: &str = "#hidden";

fn remove(path: &[String], inner: &[String], structure: &mut Structure) {
    if let Some(members) = structure.namespaces.get_mut(path)
        && let Some(name) = inner.last()
        && let Some(entry) = members.get(name)
        && entry.kinds == [Kind::Namespace]
    {
        members.remove(name);
    }
}

fn entry<'s>(path: &[String], name: &str, structure: &'s mut Structure) -> &'s mut Entry {
    let members = structure.namespaces.entry(path.to_vec()).or_default();
    let position = members.len();
    let entry = members.entry(name.to_owned()).or_insert_with(|| Entry {
        position,
        ..Entry::default()
    });
    if entry.name.is_empty() {
        let segments: Vec<&str> = path.iter().map(String::as_str).chain([name]).collect();
        entry.name = generated(&segments);
    }
    entry
}

/// Register namespace `inner` as a member of `parent`.
fn add(
    parent: &[String],
    inner: &[String],
    kind: Kind,
    structure: &mut Structure,
    source: &Source,
    span: Span,
) -> Result<(), CompilerError> {
    let Some(name) = inner.last() else {
        return Ok(());
    };
    if parent.is_empty() {
        // A module-level namespace keeps its own name.
        let members = structure.namespaces.entry(Vec::new()).or_default();
        let position = members.len();
        let entry = members.entry(name.clone()).or_insert_with(|| Entry {
            position,
            ..Entry::default()
        });
        entry.name = name.clone();
        entry.namespace = true;
        if !entry.kinds.contains(&kind) {
            entry.kinds.push(kind);
        }
        return Ok(());
    }
    let entry = entry(parent, name, structure);
    entry.namespace = true;
    if !entry.kinds.contains(&kind) {
        check_kind(entry, kind, name, source, span)?;
    }
    Ok(())
}

/// Declarations merge as TypeScript merges them: namespaces with each
/// other and with a function, class or enum; interfaces with interfaces;
/// function overload signatures with their implementation.
fn check_kind(
    entry: &mut Entry,
    kind: Kind,
    name: &str,
    source: &Source,
    span: Span,
) -> Result<(), CompilerError> {
    let merges = entry.kinds.iter().all(|existing| {
        matches!(
            (existing, kind),
            (
                Kind::Namespace,
                Kind::Namespace | Kind::Function | Kind::Class | Kind::Enum
            ) | (Kind::Function | Kind::Class | Kind::Enum, Kind::Namespace)
                | (Kind::Interface, Kind::Interface)
                | (Kind::Function, Kind::Function)
        )
    });
    if !merges {
        return Err(source.locate(
            CompilerError::Invalid(format!("duplicate namespace member: {name}")),
            span,
        ));
    }
    entry.kinds.push(kind);
    Ok(())
}

/// The name and kind of a namespace body declaration.
fn member(decl: &ast::Decl, source: &Source) -> Result<Option<(String, Kind)>, CompilerError> {
    Ok(Some(match decl {
        ast::Decl::Fn(f) if !f.declare => (f.ident.sym.to_string(), Kind::Function),
        ast::Decl::Class(c) if !c.declare => (c.ident.sym.to_string(), Kind::Class),
        ast::Decl::TsInterface(i) if !i.declare => (i.id.sym.to_string(), Kind::Interface),
        ast::Decl::TsTypeAlias(a) if !a.declare => (a.id.sym.to_string(), Kind::Alias),
        ast::Decl::TsEnum(e) => (e.id.sym.to_string(), Kind::Enum),
        ast::Decl::Var(var) if !var.declare => {
            let [declarator] = var.decls.as_slice() else {
                return Err(source.locate(
                    unsupported("namespace variable statement with several declarators"),
                    var.span,
                ));
            };
            let ast::Pat::Ident(binding) = &declarator.name else {
                return Err(source.locate(
                    unsupported("destructuring declaration in a namespace"),
                    declarator.span,
                ));
            };
            let kind = if var.kind == ast::VarDeclKind::Const {
                Kind::Variable
            } else {
                Kind::Mutable
            };
            (binding.id.sym.to_string(), kind)
        }
        ast::Decl::TsModule(_) => return Ok(None),
        other => {
            return Err(source.locate(
                unsupported("ambient declaration in a namespace"),
                other.span(),
            ));
        }
    }))
}

/// The generated name of an unexported member: it is reachable only
/// unqualified inside its own declaration.
fn hidden(path: &[String], name: &str) -> String {
    let segments: Vec<&str> = path
        .iter()
        .map(String::as_str)
        .chain(["local", name])
        .collect();
    generated(&segments)
}

/// The symbol of the namespace at `path`, with its exported members.
/// How the type of a constant declaration is named (see `Entry::annotation`).
fn variable_type(decl: &ast::Decl) -> Option<Option<Box<ast::TsType>>> {
    let ast::Decl::Var(var) = decl else {
        return None;
    };
    let declarator = var.decls.first()?;
    if let ast::Pat::Ident(binding) = &declarator.name
        && binding.type_ann.is_some()
    {
        return Some(None);
    }
    let kind = match declarator.init.as_deref()? {
        ast::Expr::Lit(ast::Lit::Num(_)) => ast::TsKeywordTypeKind::TsNumberKeyword,
        ast::Expr::Lit(ast::Lit::Str(_)) | ast::Expr::Tpl(_) => {
            ast::TsKeywordTypeKind::TsStringKeyword
        }
        ast::Expr::Lit(ast::Lit::Bool(_)) => ast::TsKeywordTypeKind::TsBooleanKeyword,
        _ => return None,
    };
    Some(Some(Box::new(ast::TsType::TsKeywordType(
        ast::TsKeywordType {
            span: var.span,
            kind,
        },
    ))))
}

/// The generated name of the object of the namespace at `path`.
fn object_name(path: &[String]) -> String {
    let segments: Vec<&str> = path
        .iter()
        .map(String::as_str)
        .chain(["", "object"])
        .collect();
    generated(&segments)
}

/// A namespace object property: its key, the identifier of its value and
/// the type that names it (`typeof` of the identifier when absent).
type Property = (String, String, Option<Box<ast::TsType>>);

/// The properties of the namespace object at `path` (key and the value's
/// identifier), if every exported value member has a value of its own.
fn object_members(path: &[String], structure: &Structure) -> Option<Vec<Property>> {
    let members = structure.namespaces.get(path)?;
    let mut properties = Vec::new();
    // Declaration order, as the namespace's properties are assigned.
    let mut ordered: Vec<(&String, &Entry)> = members.iter().collect();
    ordered.sort_by_key(|(_, entry)| entry.position);
    for (name, entry) in ordered {
        let value_kinds = entry
            .kinds
            .iter()
            .filter(|k| !matches!(k, Kind::Interface | Kind::Alias))
            .count();
        if value_kinds == 0 {
            continue;
        }
        let value = match entry.kinds.as_slice() {
            [Kind::Function, ..] if entry.kinds.iter().all(|k| *k == Kind::Function) => {
                entry.name.clone()
            }
            [Kind::Variable] => entry.name.clone(),
            [Kind::Enum] => entry.enumeration.as_ref()?.object.clone()?,
            [Kind::Namespace] => {
                let mut inner = path.to_vec();
                inner.push(name.clone());
                object_members(&inner, structure)?;
                object_name(&inner)
            }
            _ => return None,
        };
        let ty = if entry.kinds == [Kind::Variable] {
            entry.annotation.clone()?
        } else {
            None
        };
        properties.push((name.clone(), value, ty));
    }
    Some(properties)
}

/// `const NS$object = { a: NS$a, f: NS$f, Inner: NS$Inner$object }` for the
/// namespace at `path` and its nested namespaces, innermost first.
fn object_declarations(path: &[String], structure: &Structure, span: Span) -> Vec<ast::Decl> {
    let Some(properties) = object_members(path, structure) else {
        return Vec::new();
    };
    let mut declarations = Vec::new();
    if let Some(members) = structure.namespaces.get(path) {
        for (name, entry) in members {
            if entry.kinds == [Kind::Namespace] {
                let mut inner = path.to_vec();
                inner.push(name.clone());
                declarations.extend(object_declarations(&inner, structure, span));
            }
        }
    }
    // `typeof NS` names the object's type member by member.
    let annotation = ast::TsType::TsTypeLit(ast::TsTypeLit {
        span,
        members: properties
            .iter()
            .map(|(key, value, ty)| {
                ast::TsTypeElement::TsPropertySignature(ast::TsPropertySignature {
                    span,
                    readonly: true,
                    key: Box::new(ast::Expr::Lit(ast::Lit::Str(enums::string(key, span)))),
                    computed: false,
                    optional: false,
                    type_ann: Some(Box::new(ast::TsTypeAnn {
                        span,
                        type_ann: ty.clone().unwrap_or_else(|| {
                            Box::new(ast::TsType::TsTypeQuery(ast::TsTypeQuery {
                                span,
                                expr_name: ast::TsTypeQueryExpr::TsEntityName(
                                    ast::TsEntityName::Ident(ast::Ident::new_no_ctxt(
                                        value.as_str().into(),
                                        span,
                                    )),
                                ),
                                type_args: None,
                            }))
                        }),
                    })),
                })
            })
            .collect(),
    });
    let props = properties
        .into_iter()
        .map(|(key, value, _)| {
            ast::PropOrSpread::Prop(Box::new(ast::Prop::KeyValue(ast::KeyValueProp {
                key: ast::PropName::Str(enums::string(&key, span)),
                value: Box::new(ast::Expr::Ident(ast::Ident::new_no_ctxt(
                    value.into(),
                    span,
                ))),
            })))
        })
        .collect();
    declarations.push(ast::Decl::Var(Box::new(ast::VarDecl {
        span,
        ctxt: Default::default(),
        kind: ast::VarDeclKind::Const,
        declare: false,
        decls: vec![ast::VarDeclarator {
            span,
            name: ast::Pat::Ident(ast::BindingIdent {
                id: ast::Ident::new_no_ctxt(object_name(path).into(), span),
                type_ann: Some(Box::new(ast::TsTypeAnn {
                    span,
                    type_ann: Box::new(annotation),
                })),
            }),
            init: Some(Box::new(ast::Expr::Object(ast::ObjectLit { span, props }))),
            definite: false,
        }],
    })));
    declarations
}

fn namespace_symbol(path: &[String], structure: &Structure) -> Rc<Namespace> {
    let mut namespace = Namespace {
        object: object_members(path, structure).map(|_| object_name(path)),
        ..Namespace::default()
    };
    if let Some(members) = structure.namespaces.get(path) {
        for (name, entry) in members {
            let mut inner = path.to_vec();
            inner.push(name.clone());
            namespace.members.insert(
                name.clone(),
                Member {
                    exported: true,
                    symbol: Symbol {
                        name: entry.name.clone(),
                        enumeration: entry.enumeration.clone(),
                        namespace: entry.namespace.then(|| namespace_symbol(&inner, structure)),
                        value: entry.value,
                        field: entry.field.clone(),
                    },
                },
            );
        }
    }
    Rc::new(namespace)
}

/// Module-level enums and namespaces, merged with same-named functions,
/// classes and enums.
fn module_frame(items: &[ast::ModuleItem], structure: &Structure) -> Frame {
    let mut frame = Frame::new();
    for (name, enumeration) in &structure.enums {
        frame.insert(name.clone(), enum_symbol(name.clone(), enumeration.clone()));
    }
    if let Some(members) = structure.namespaces.get(&Vec::new()) {
        for name in members.keys() {
            let symbol = frame.entry(name.clone()).or_insert_with(|| Symbol {
                name: name.clone(),
                enumeration: None,
                namespace: None,
                value: false,
                field: None,
            });
            symbol.namespace = Some(namespace_symbol(std::slice::from_ref(name), structure));
        }
    }
    for item in items {
        if let Some((decl, _)) = declaration(item)
            && let Some(name) = declared_name(decl)
            && !matches!(decl, ast::Decl::TsEnum(_))
            && let Some(symbol) = frame.get_mut(&name)
        {
            symbol.value = true;
        }
    }
    frame
}

/// Emission pass: hoisted namespace members, rewritten in their frames.
struct Emitter<'a> {
    source: &'a Source,
    structure: &'a Structure,
    items: &'a mut Vec<ast::ModuleItem>,
}
impl Emitter<'_> {
    fn namespace(
        &mut self,
        path: Path,
        body: &ast::TsNamespaceBody,
        exported: bool,
        frames: &[&Frame],
    ) -> Result<(), CompilerError> {
        let items = match block_or_nested(body) {
            Ok(items) => items,
            Err((nested, ())) => {
                let frame = self.frame(&path, &[]);
                let mut chain = frames.to_vec();
                chain.push(&frame);
                let mut inner = path.clone();
                inner.push(nested.id.sym.to_string());
                return self.namespace(inner, &nested.body, exported, &chain);
            }
        };
        let frame = self.frame(&path, items);
        let mut chain = frames.to_vec();
        chain.push(&frame);
        for item in items {
            let Some((decl, member_exported)) = declaration(item) else {
                return Err(self.source.locate(
                    unsupported("statement in a namespace body (only declarations are admitted)"),
                    item.span(),
                ));
            };
            let visible = exported && member_exported;
            let span = item.span();
            match decl {
                ast::Decl::TsModule(decl) => {
                    let (name, inner_body) = module_parts(decl, self.source)?;
                    let mut inner = path.clone();
                    inner.push(name);
                    self.namespace(inner, inner_body, visible, &chain)?;
                }
                ast::Decl::TsEnum(decl) => {
                    let name = decl.id.sym.to_string();
                    let enumeration = self.enumeration(&path, &name, member_exported)?;
                    self.items
                        .extend(enums::items(&enumeration, visible, span, self.source)?);
                }
                _ => {
                    let Some((name, _)) = member(decl, self.source)? else {
                        continue;
                    };
                    let mangled = frame
                        .get(&name)
                        .map(|symbol| symbol.name.clone())
                        .ok_or_else(|| CompilerError::Internal("namespace member".into()))?;
                    let decl = rewrite::declaration(decl.clone(), self.source, &chain)?;
                    self.items
                        .push(item_of(rename(decl, &mangled), visible, span));
                }
            }
        }
        Ok(())
    }
    fn enumeration(
        &self,
        path: &[String],
        name: &str,
        exported: bool,
    ) -> Result<Rc<Enum>, CompilerError> {
        let mut key = path.to_vec();
        if !exported {
            key.push(String::from(HIDDEN));
        }
        self.structure
            .namespaces
            .get(&key)
            .and_then(|members| members.get(name))
            .and_then(|entry| entry.enumeration.clone())
            .ok_or_else(|| CompilerError::Internal("namespace enum".into()))
    }
    /// The names visible unqualified in one declaration of the namespace at
    /// `path`: every exported member and the declaration's own members.
    fn frame(&self, path: &[String], items: &[ast::ModuleItem]) -> Frame {
        let mut frame = Frame::new();
        let namespace = namespace_symbol(path, self.structure);
        for (name, member) in &namespace.members {
            frame.insert(name.clone(), member.symbol.clone());
        }
        for item in items {
            let Some((decl, false)) = declaration(item) else {
                continue;
            };
            match decl {
                ast::Decl::TsEnum(e) => {
                    let name = e.id.sym.to_string();
                    if let Ok(enumeration) = self.enumeration(path, &name, false) {
                        frame.insert(name, enum_symbol(enumeration.name.clone(), enumeration));
                    }
                }
                ast::Decl::TsModule(m) => {
                    if let ast::TsModuleName::Ident(id) = &m.id {
                        let mut inner = path.to_vec();
                        inner.push(id.sym.to_string());
                        frame.entry(id.sym.to_string()).or_insert_with(|| Symbol {
                            name: id.sym.to_string(),
                            enumeration: None,
                            namespace: Some(namespace_symbol(&inner, self.structure)),
                            value: false,
                            field: None,
                        });
                    }
                }
                decl => {
                    if let Ok(Some((name, kind))) = member(decl, self.source) {
                        let symbol = frame
                            .entry(name.clone())
                            .or_insert_with(|| Symbol::plain(hidden(path, &name)));
                        symbol.value |= matches!(
                            kind,
                            Kind::Function | Kind::Class | Kind::Variable | Kind::Mutable
                        );
                    }
                }
            }
        }
        frame
    }
}

/// Rename the declared identifier of a hoisted member.
fn rename(decl: ast::Decl, name: &str) -> ast::Decl {
    let rename = |id: &mut ast::Ident| id.sym = name.into();
    let mut decl = decl;
    match &mut decl {
        ast::Decl::Fn(f) => rename(&mut f.ident),
        ast::Decl::Class(c) => rename(&mut c.ident),
        ast::Decl::TsInterface(i) => rename(&mut i.id),
        ast::Decl::TsTypeAlias(a) => rename(&mut a.id),
        ast::Decl::Var(var) => {
            for declarator in &mut var.decls {
                if let ast::Pat::Ident(binding) = &mut declarator.name {
                    rename(&mut binding.id);
                }
            }
        }
        _ => {}
    }
    decl
}

fn item_of(decl: ast::Decl, exported: bool, span: Span) -> ast::ModuleItem {
    item(decl, exported, span)
}

/// A module item declaring `decl`, exported or not.
pub(super) fn item(decl: ast::Decl, exported: bool, span: Span) -> ast::ModuleItem {
    if exported {
        ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(ast::ExportDecl { span, decl }))
    } else {
        ast::ModuleItem::Stmt(ast::Stmt::Decl(decl))
    }
}
