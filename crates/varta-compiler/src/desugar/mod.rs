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
mod namespaces;
mod overloads;
mod rewrite;
mod scope;
mod statics;

use crate::CompilerError;
use crate::modules::Dependency;
use crate::source::Source;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use swc_common::Span;
use swc_ecma_ast as ast;
use swc_ecma_visit::{Visit, VisitWith};

/// Generated names start with this prefix, which program code may not use.
pub(crate) const PREFIX: &str = "__vartaDecl";
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
}

/// The exported and unexported members of a namespace, merged over all of
/// its declarations.
#[derive(Default)]
pub(crate) struct Namespace {
    pub members: BTreeMap<String, Member>,
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
    Ok(exports)
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

/// Rewrite the module's uses of imported enums and namespaces, once its
/// dependencies are loaded, and import the generated names they need.
pub(crate) fn imports(
    module: &mut ast::Module,
    source: &Source,
    dependencies: &mut [Dependency],
    exports: &dyn Fn(&Path) -> Option<Exports>,
) -> Result<(), CompilerError> {
    let mut frame = BTreeMap::new();
    // Generated names, by the dependency that exports them.
    let mut providers: BTreeMap<String, PathBuf> = BTreeMap::new();
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
                providers.insert(name, dependency.target.clone());
            }
            if !symbol.value && symbol.enumeration.is_none() {
                namespaces.insert((dependency.target.clone(), binding.local.clone()));
            }
            let mut symbol = symbol.clone();
            symbol.name = binding.local.clone();
            frame.insert(binding.local.clone(), symbol);
        }
    }
    if frame.is_empty() {
        return Ok(());
    }
    let used = rewrite::module(module, source, &frame)?;
    // Each generated name is bound once, by the first import of its module.
    let mut bound: BTreeSet<String> = BTreeSet::new();
    for dependency in dependencies.iter_mut() {
        // A namespace without a value of its own has no export binding.
        dependency
            .bindings
            .retain(|b| !namespaces.contains(&(dependency.target.clone(), b.local.clone())));
        for name in &used {
            if providers.get(name) == Some(&dependency.target)
                && !dependency.bindings.iter().any(|b| &b.local == name)
                && bound.insert(name.clone())
            {
                dependency.bindings.push(crate::modules::ImportBinding {
                    imported: name.clone(),
                    local: name.clone(),
                    span: dependency.span,
                    type_only: false,
                });
            }
        }
    }
    Ok(())
}

/// Every generated name reachable from an exported symbol.
fn generated_names(symbol: &Symbol) -> Vec<String> {
    let mut names = Vec::new();
    if symbol.name.starts_with(PREFIX) {
        names.push(symbol.name.clone());
    }
    if let Some(reverse) = symbol.enumeration.as_ref().and_then(|e| e.reverse.clone()) {
        names.push(reverse);
    }
    if let Some(namespace) = &symbol.namespace {
        for member in namespace.members.values().filter(|m| m.exported) {
            names.extend(generated_names(&member.symbol));
        }
    }
    names
}
