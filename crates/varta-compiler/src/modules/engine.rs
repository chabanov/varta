//! Engine-provided standard Web types (v35 `web_headers`, `abort_signals`).
//!
//! Each type is a typed TypeScript prelude module embedded in the compiler and
//! compiled with the program through the shared module pipeline, so it is an
//! ordinary nominal class with no host import and no capability. A module that
//! names one of a prelude's exports without declaring or importing that name
//! receives an implicit import of it; a program that names none of them never
//! loads a prelude, so its Wasm bytes are unchanged. Prelude paths are reserved:
//! a host loader can never supply or shadow them.
use super::*;
use swc_ecma_visit::{Visit, VisitWith};
use varta_common::language::LanguageProfile;

/// One embedded prelude module.
pub(crate) struct Prelude {
    /// Reserved module identity, never produced by a host loader.
    pub path: &'static str,
    pub source: &'static str,
    /// Exported names that trigger the implicit import, with their namespace.
    pub exports: &'static [(&'static str, Namespace)],
    pub admitted: fn(LanguageProfile) -> bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Namespace {
    /// A class: its type and its value.
    Class,
    /// A type alias only.
    Type,
}

/// The `Headers` class and `HeadersInit` alias (WHATWG Fetch).
pub(crate) const HEADERS: Prelude = Prelude {
    path: "<varta>/web/headers.ts",
    source: include_str!("../prelude/headers.ts"),
    exports: &[
        (HEADERS_CLASS, Namespace::Class),
        ("HeadersInit", Namespace::Type),
    ],
    admitted: LanguageProfile::web_headers,
};
/// `AbortController`, `AbortSignal` and the `DOMException` abort reasons.
pub(crate) const ABORT: Prelude = Prelude {
    path: "<varta>/web/abort.ts",
    source: include_str!("../prelude/abort.ts"),
    exports: &[
        ("AbortController", Namespace::Class),
        (ABORT_SIGNAL_CLASS, Namespace::Class),
        ("DOMException", Namespace::Class),
    ],
    admitted: LanguageProfile::abort_signals,
};
pub(crate) const PRELUDES: [&Prelude; 2] = [&HEADERS, &ABORT];
/// Class names that compiler hooks recognize in their prelude modules.
pub(crate) const HEADERS_CLASS: &str = "Headers";
pub(crate) const ABORT_SIGNAL_CLASS: &str = "AbortSignal";
/// The member name under which [`pending`] lists a constructor.
pub(crate) const CONSTRUCTOR: &str = "constructor";

/// The prelude reserved at `path`, if any.
pub(crate) fn prelude(path: &Path) -> Option<&'static Prelude> {
    PRELUDES
        .into_iter()
        .find(|prelude| Path::new(prelude.path) == path)
}

/// Whether `path` names an engine prelude module.
pub(crate) fn is_prelude(path: &Path) -> bool {
    prelude(path).is_some()
}

/// Whether `source` is an engine prelude module.
pub(crate) fn is_prelude_source(source: &crate::source::Source) -> bool {
    PRELUDES
        .into_iter()
        .any(|prelude| source.is_path(Path::new(prelude.path)))
}

/// Names bound at the top level of a module: declarations and imports.
fn top_level(module: &ast::Module, imports: &[Dependency]) -> BTreeSet<String> {
    let mut names: BTreeSet<String> = imports
        .iter()
        .flat_map(|import| import.bindings.iter().map(|b| b.local.clone()))
        .collect();
    for item in &module.body {
        let declaration = match item {
            ast::ModuleItem::Stmt(ast::Stmt::Decl(declaration)) => declaration,
            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) => &export.decl,
            _ => continue,
        };
        match declaration {
            ast::Decl::Class(class) => {
                names.insert(class.ident.sym.to_string());
            }
            ast::Decl::Fn(function) => {
                names.insert(function.ident.sym.to_string());
            }
            ast::Decl::TsInterface(interface) => {
                names.insert(interface.id.sym.to_string());
            }
            ast::Decl::TsTypeAlias(alias) => {
                names.insert(alias.id.sym.to_string());
            }
            ast::Decl::TsEnum(declaration) => {
                names.insert(declaration.id.sym.to_string());
            }
            ast::Decl::Var(var) => {
                for declarator in &var.decls {
                    if let ast::Pat::Ident(binding) = &declarator.name {
                        names.insert(binding.id.sym.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    names
}

/// Identifier references (values and type names) of a module, with the span of
/// their first occurrence. Property names are not identifiers and never match.
#[derive(Default)]
struct References {
    found: BTreeMap<String, swc_common::Span>,
}
impl Visit for References {
    fn visit_ident(&mut self, ident: &ast::Ident) {
        self.found
            .entry(ident.sym.to_string())
            .or_insert(ident.span);
    }
}

/// Implicit prelude imports of one module, in prelude order. A prelude module
/// itself only imports what another prelude exports.
pub(super) fn implicit_imports(
    module: &ast::Module,
    imports: &[Dependency],
    profile: LanguageProfile,
) -> Vec<Dependency> {
    let admitted: Vec<&Prelude> = PRELUDES
        .into_iter()
        .filter(|prelude| (prelude.admitted)(profile))
        .collect();
    if admitted.is_empty() {
        return Vec::new();
    }
    let mut references = References::default();
    module.visit_with(&mut references);
    let bound = top_level(module, imports);
    admitted
        .into_iter()
        .filter_map(|prelude| {
            let bindings: Vec<ImportBinding> = prelude
                .exports
                .iter()
                .filter(|(name, _)| !bound.contains(*name))
                .filter_map(|(name, namespace)| {
                    references.found.get(*name).map(|span| ImportBinding {
                        imported: (*name).into(),
                        local: (*name).into(),
                        span: *span,
                        type_only: *namespace == Namespace::Type,
                    })
                })
                .collect();
            let span = bindings.first()?.span;
            Some(Dependency {
                target: PathBuf::from(prelude.path),
                bindings,
                span,
            })
        })
        .collect()
}

/// Members of prelude classes that v35 names but does not implement, with the
/// reason reported instead of an unknown-member diagnostic.
const PENDING: &[(&str, &[&str], &str)] = &[
    (
        ABORT_SIGNAL_CLASS,
        &[CONSTRUCTOR],
        "new AbortSignal() (the constructor is illegal in JavaScript; use new AbortController().signal or AbortSignal.abort())",
    ),
    (
        ABORT_SIGNAL_CLASS,
        &["timeout", "any"],
        "AbortSignal.timeout and AbortSignal.any (v35 has no timers and no abort listeners)",
    ),
    (
        ABORT_SIGNAL_CLASS,
        &[
            "addEventListener",
            "removeEventListener",
            "dispatchEvent",
            "onabort",
        ],
        "abort event listeners (a signal cannot hold listener functions: value layout 15 has no function-valued object fields)",
    ),
];

/// Why member `name` of prelude class `class` is unsupported, if it is pending.
pub(crate) fn pending(class: &str, name: &str) -> Option<&'static str> {
    PENDING
        .iter()
        .find(|(owner, members, _)| *owner == class && members.contains(&name))
        .map(|(_, _, reason)| *reason)
}
