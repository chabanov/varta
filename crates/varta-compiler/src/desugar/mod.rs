//! Declaration forms of profile v38 (`declaration_forms()`, K9 B), lowered
//! to the checked subset before module scanning, as the TSX templates are.
//!
//! - An `enum` becomes the literal union type of its member values (`type E
//!   = 0 | 1`), its member reads `E.X` become those literal values, and the
//!   reverse mapping `E[n]` of a numeric member becomes the member name, or
//!   for a key that is not a constant a call of a generated function typed
//!   `(value: number) => string | undefined`. Members are constant
//!   expressions evaluated here with ECMAScript number semantics, as
//!   TypeScript evaluates them; a `const enum` has no reverse mapping.
//! - A `namespace` is hoisted: every member becomes a module declaration
//!   under a generated name (`__vartaDecl$NS$member`), and every reference
//!   to a member, unqualified inside the namespace or qualified (`NS.x`,
//!   `NS.Inner.T`) anywhere, becomes that name. Namespaces merge with each
//!   other and with a function, class or enum of the same name.
//!
//! Rewriting is scope-aware: a local binding that shadows an enum,
//! namespace or member name is never rewritten. Imported enums and
//! namespaces are rewritten once the module's dependencies are loaded
//! ([`imports`]), through the symbols each module exports ([`Exports`]).
//! The generated names start with [`PREFIX`], which program identifiers may
//! not use.
mod class_expressions;
mod constants;
mod enums;
mod expandos;
mod namespaces;
mod overloads;
mod rewrite;
mod scope;
mod statics;

pub(crate) use expandos::target as expando_target;

use crate::CompilerError;
use crate::modules::{Dependency, Export};
use crate::source::Source;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use swc_common::Span;
use swc_ecma_ast as ast;
use swc_ecma_visit::{Visit, VisitMut, VisitMutWith, VisitWith};

/// Generated names start with this prefix, which program code may not use.
pub(crate) const PREFIX: &str = "__vartaDecl";
/// The generated type reference `__vartaDeclEnum<T, "group">` of an enum
/// type: `T`'s numeric literals accept any `number` and its string literals
/// are nominal to the enum (`lower::literals::enum_type`).
pub(crate) const ENUM_TYPE: &str = "__vartaDeclEnum";
/// Separates the segments of a generated name.
const SEPARATOR: char = '$';

/// The name a class or function declared under a generated name shows at
/// run time (`new.target.name`, an Error subclass's `name`): its own
/// declared name, the last segment.
pub(crate) fn display_name(name: &str) -> &str {
    match name.strip_prefix(PREFIX) {
        Some(rest) => rest.rsplit(SEPARATOR).next().unwrap_or(rest),
        None => name,
    }
}

/// A generated name from its segments.
fn generated(segments: &[&str]) -> String {
    let mut name = PREFIX.to_owned();
    for segment in segments {
        name.push(SEPARATOR);
        name.push_str(segment);
    }
    name
}

fn unsupported(kind: &str) -> CompilerError {
    CompilerError::Unsupported {
        kind: kind.into(),
        detail: "outside the supported TypeScript subset".into(),
    }
}
fn type_error(message: impl Into<String>) -> CompilerError {
    CompilerError::Type(message.into())
}

/// A constant enum member value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Value {
    Number(f64),
    String(String),
}

/// An evaluated enum.
pub(crate) struct Enum {
    pub name: String,
    pub members: Vec<(String, Value)>,
    pub is_const: bool,
    /// The generated reverse-mapping function of a non-const enum with
    /// numeric members.
    pub reverse: Option<String>,
    /// The generated module constant that is the enum object of a non-const
    /// enum (its value when used as one: `Object.keys(E)`, `f(E)`).
    pub object: Option<String>,
    /// The identity of the enum's nominal string type, unique in the graph.
    pub group: String,
}

/// The exported and unexported members of a namespace, merged over all of
/// its declarations.
#[derive(Default)]
pub(crate) struct Namespace {
    pub members: BTreeMap<String, Member>,
    /// The generated module constant that is the namespace object, when
    /// every exported value member has a value of its own (functions,
    /// constants, enum and nested namespace objects; a class or a `let`
    /// member has none).
    pub object: Option<String>,
}
#[derive(Clone)]
pub(crate) struct Member {
    pub exported: bool,
    pub symbol: Symbol,
}

/// An enum, a namespace or a member of a namespace.
#[derive(Clone)]
pub(crate) struct Symbol {
    /// The identifier the declaration's value and type are bound to.
    pub name: String,
    pub enumeration: Option<Rc<Enum>>,
    pub namespace: Option<Rc<Namespace>>,
    /// Bound to a run-time value (a function, class or variable) under `name`.
    pub value: bool,
    /// A static field: its access restriction, declaring class and whether it
    /// is reached through a subclass or is readonly.
    pub field: Option<statics::Field>,
}
impl Symbol {
    fn plain(name: String) -> Self {
        Self {
            name,
            enumeration: None,
            namespace: None,
            value: true,
            field: None,
        }
    }
    /// Whether this symbol has a part the rewriter resolves (otherwise it is
    /// a plain declaration that keeps its name).
    fn structured(&self) -> bool {
        self.enumeration.is_some() || self.namespace.is_some()
    }
}

/// The enums and namespaces a module exports, by exported name.
#[derive(Default, Clone)]
pub(crate) struct Exports {
    symbols: BTreeMap<String, Symbol>,
}
impl Exports {
    pub(crate) fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }
}

/// Reject program identifiers that use the generated-name prefix.
struct Reserved(Option<Span>);
impl Visit for Reserved {
    fn visit_ident(&mut self, id: &ast::Ident) {
        if self.0.is_none() && id.sym.starts_with(PREFIX) {
            self.0 = Some(id.span);
        }
    }
    fn visit_binding_ident(&mut self, id: &ast::BindingIdent) {
        self.visit_ident(&id.id);
    }
}

/// Lower the module's own enums and namespaces (before scanning); return
/// the ones it exports.
pub(crate) fn declarations(
    module: &mut ast::Module,
    source: &Source,
) -> Result<Exports, CompilerError> {
    let mut reserved = Reserved(None);
    module.visit_with(&mut reserved);
    if let Some(span) = reserved.0 {
        return Err(source.locate(
            CompilerError::Invalid(format!(
                "identifiers starting with {PREFIX} are reserved for generated declarations"
            )),
            span,
        ));
    }
    class_expressions::hoist(module, source)?;
    let exports = if module.body.iter().any(declares) {
        namespaces::lower(module, source)?
    } else {
        Exports::default()
    };
    overloads::mark(module, source)?;
    expandos::lower(module);
    Ok(tag(module, exports, source))
}

/// Make the module's generated names unique in the graph: `__vartaDecl$x`
/// becomes `__vartaDecl$m<hash>$x`, with the hash of the module path, so
/// imports and re-exports of two modules' declarations never collide. The
/// last segment, the declared name, is kept.
fn tag(module: &mut ast::Module, exports: Exports, source: &Source) -> Exports {
    struct Tag(String);
    impl Tag {
        fn name(&self, name: &str) -> String {
            match name.strip_prefix(PREFIX) {
                Some(rest) if rest.starts_with(SEPARATOR) && !rest.starts_with(&self.0) => {
                    format!("{PREFIX}{}{rest}", self.0)
                }
                _ => name.to_owned(),
            }
        }
    }
    impl VisitMut for Tag {
        fn visit_mut_ident(&mut self, id: &mut ast::Ident) {
            if id.sym.starts_with(PREFIX) {
                id.sym = self.name(&id.sym).into();
            }
        }
    }
    let mut tag = Tag(format!("{SEPARATOR}m{}", module_hash(source)));
    module.visit_mut_with(&mut tag);
    let symbols = exports
        .symbols
        .iter()
        .map(|(name, symbol)| (name.clone(), mapped(symbol, &|n| tag.name(n))))
        .collect();
    Exports { symbols }
}

/// FNV-1a of the module path: stable across runs and platforms.
fn module_hash(source: &Source) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in source.file.name.to_string().bytes() {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Whether a module item declares an enum, a namespace or a class with
/// static members.
fn declares(item: &ast::ModuleItem) -> bool {
    match declaration(item) {
        Some((ast::Decl::TsEnum(_) | ast::Decl::TsModule(_), _)) => true,
        Some((ast::Decl::Class(class), _)) => class.class.body.iter().any(|member| {
            matches!(member, ast::ClassMember::ClassProp(p) if p.is_static)
                || matches!(member, ast::ClassMember::AutoAccessor(a) if a.is_static)
                || matches!(member, ast::ClassMember::Method(m) if m.is_static)
                || matches!(member, ast::ClassMember::StaticBlock(_))
        }),
        _ => false,
    }
}

/// The declaration of a module item and whether it is exported.
fn declaration(item: &ast::ModuleItem) -> Option<(&ast::Decl, bool)> {
    match item {
        ast::ModuleItem::Stmt(ast::Stmt::Decl(decl)) => Some((decl, false)),
        ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) => {
            Some((&export.decl, true))
        }
        _ => None,
    }
}

/// `symbol` with every generated name mapped through `map`.
fn mapped(symbol: &Symbol, map: &dyn Fn(&str) -> String) -> Symbol {
    let mut symbol = symbol.clone();
    if symbol.name.starts_with(PREFIX) {
        symbol.name = map(&symbol.name);
    }
    if let Some(field) = &mut symbol.field {
        field.setter = field.setter.as_deref().map(map);
    }
    if let Some(enumeration) = &symbol.enumeration
        && enumeration.reverse.is_some()
    {
        symbol.enumeration = Some(Rc::new(Enum {
            name: enumeration.name.clone(),
            members: enumeration.members.clone(),
            is_const: enumeration.is_const,
            reverse: enumeration.reverse.as_deref().map(map),
            object: enumeration.object.as_deref().map(map),
            group: enumeration.group.clone(),
        }));
    }
    if let Some(namespace) = &symbol.namespace {
        symbol.namespace = Some(Rc::new(Namespace {
            object: namespace.object.as_deref().map(map),
            members: namespace
                .members
                .iter()
                .map(|(name, member)| {
                    (
                        name.clone(),
                        Member {
                            exported: member.exported,
                            symbol: mapped(&member.symbol, map),
                        },
                    )
                })
                .collect(),
        }));
    }
    symbol
}

/// Rewrite the module's uses of imported enums and namespaces, once its
/// dependencies are loaded, and import the generated names they need.
/// Re-exports (`export { E } from`, `export *`, `export { E }` of an import)
/// forward the symbols and their generated names, which `declared`
/// receives.
pub(crate) fn imports(
    module: &mut ast::Module,
    source: &Source,
    (dependencies, module_exports, stars): (&mut [Dependency], &mut Vec<Export>, &[PathBuf]),
    declared: &mut Exports,
    exports: &dyn Fn(&Path) -> Option<Exports>,
) -> Result<(), CompilerError> {
    let mut frame = BTreeMap::new();
    // Local generated names: the providing module and the exported name.
    let mut providers: Providers = BTreeMap::new();
    let mut namespaces: BTreeSet<(PathBuf, String)> = BTreeSet::new();
    for dependency in dependencies.iter() {
        let Some(exported) = exports(&dependency.target) else {
            continue;
        };
        for binding in &dependency.bindings {
            let Some(symbol) = exported.symbols.get(&binding.imported) else {
                continue;
            };
            // A type-only import binds no value: a class or enum keeps its
            // type, and a namespace (whose types are generated
            // declarations) is refused.
            if binding.type_only {
                if symbol.namespace.is_some() && !symbol.value && symbol.enumeration.is_none() {
                    return Err(
                        source.locate(unsupported("type-only import of a namespace"), binding.span)
                    );
                }
                continue;
            }
            for name in generated_names(symbol) {
                providers.insert(name.clone(), (dependency.target.clone(), name));
            }
            if !symbol.value && symbol.enumeration.is_none() {
                namespaces.insert((dependency.target.clone(), binding.local.clone()));
            }
            let mut symbol = symbol.clone();
            symbol.name = binding.local.clone();
            frame.insert(binding.local.clone(), symbol);
        }
    }
    let mut used = if frame.is_empty() {
        BTreeSet::new()
    } else {
        rewrite::module(module, source, &frame)?
    };
    forward(
        (module_exports, stars),
        declared,
        exports,
        (&frame, &providers, &mut used),
    );
    // Each generated name is bound once, by the first import of its module.
    let mut bound: BTreeSet<String> = BTreeSet::new();
    for dependency in dependencies.iter_mut() {
        // A namespace without a value of its own has no export binding.
        dependency
            .bindings
            .retain(|b| !namespaces.contains(&(dependency.target.clone(), b.local.clone())));
        for name in &used {
            if let Some((target, imported)) = providers.get(name)
                && *target == dependency.target
                && !dependency.bindings.iter().any(|b| &b.local == name)
                && bound.insert(name.clone())
            {
                dependency.bindings.push(crate::modules::ImportBinding {
                    imported: imported.clone(),
                    local: name.clone(),
                    span: dependency.span,
                    type_only: false,
                });
            }
        }
    }
    Ok(())
}

/// Generated names by the module that exports them, with the name it
/// exports them under.
type Providers = BTreeMap<String, (PathBuf, String)>;

/// Forward re-exported enums, namespaces and classes with statics.
fn forward(
    (module_exports, stars): (&mut Vec<Export>, &[PathBuf]),
    declared: &mut Exports,
    exports: &dyn Fn(&Path) -> Option<Exports>,
    (frame, providers, used): (&BTreeMap<String, Symbol>, &Providers, &mut BTreeSet<String>),
) {
    let mut added = Vec::new();
    let mut removed = BTreeSet::new();
    for (position, export) in module_exports.iter().enumerate() {
        let symbol = match &export.from {
            // `export { E } from './a'`: the target's own symbol and names.
            Some(target) => {
                match exports(target).and_then(|e| e.symbols.get(&export.local).cloned()) {
                    Some(symbol) => {
                        for name in generated_names(&symbol) {
                            added.push(Export {
                                local: name.clone(),
                                exported: name,
                                span: export.span,
                                type_only: false,
                                from: Some(target.clone()),
                            });
                        }
                        symbol
                    }
                    None => continue,
                }
            }
            // `export { E }` of an imported `E`: re-export the local
            // bindings of its generated names under their original names.
            None => match frame.get(&export.local) {
                Some(symbol) => {
                    for (local, (_, original)) in providers {
                        if generated_names(symbol).contains(local) {
                            used.insert(local.clone());
                            added.push(Export {
                                local: local.clone(),
                                exported: original.clone(),
                                span: export.span,
                                type_only: false,
                                from: None,
                            });
                        }
                    }
                    symbol.clone()
                }
                None => continue,
            },
        };
        if !symbol.value && symbol.enumeration.is_none() {
            // A namespace has no export of its own name.
            removed.insert(position);
        }
        declared.symbols.insert(export.exported.clone(), symbol);
    }
    let mut position = 0;
    module_exports.retain(|_| {
        position += 1;
        !removed.contains(&(position - 1))
    });
    for export in added {
        if !module_exports.iter().any(|e| e.exported == export.exported) {
            module_exports.push(export);
        }
    }
    // `export * from './a'` forwards every symbol the module does not
    // declare itself; star exports already forward the generated names.
    for star in stars {
        if let Some(forwarded) = exports(star) {
            for (name, symbol) in forwarded.symbols {
                declared.symbols.entry(name).or_insert(symbol);
            }
        }
    }
}

/// Every generated name reachable from an exported symbol.
fn generated_names(symbol: &Symbol) -> Vec<String> {
    let mut names = Vec::new();
    if symbol.name.starts_with(PREFIX) {
        names.push(symbol.name.clone());
    }
    if let Some(setter) = symbol.field.as_ref().and_then(|f| f.setter.clone()) {
        names.push(setter);
    }
    if let Some(reverse) = symbol.enumeration.as_ref().and_then(|e| e.reverse.clone()) {
        names.push(reverse);
    }
    if let Some(object) = symbol.enumeration.as_ref().and_then(|e| e.object.clone()) {
        names.push(object);
    }
    if let Some(namespace) = &symbol.namespace {
        names.extend(namespace.object.clone());
        for member in namespace.members.values().filter(|m| m.exported) {
            names.extend(generated_names(&member.symbol));
        }
    }
    names
}
