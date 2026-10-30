//! Engine-provided standard Web types (v35 `web_headers`, `abort_signals`)
//! and the v37 `RegExp` prelude (`regexp`).
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
    /// Static member calls that trigger the implicit import of an exported
    /// function, bound under the member's dotted name, which no source
    /// declaration can spell (v36 `AbortSignal.timeout`).
    pub members: &'static [Member],
    /// Referenced names (identifiers or dotted member chains) that load the
    /// prelude and bind one export under an unspellable dotted name (v37
    /// `permissionDenied` for the timer globals).
    pub triggers: &'static [Trigger],
    pub admitted: fn(LanguageProfile) -> bool,
    /// A later profile's source of the same module, replacing `source` where
    /// its selector admits it; earlier profiles keep `source` unchanged.
    pub extended: Option<Extension>,
    /// Reference keys (see [`References`]) that load the module without
    /// binding anything (v37): identifiers, member property names written
    /// with a leading `.`, and the regular expression literal key. The
    /// compiler reaches such a prelude's functions and classes through its
    /// hooks (`lower::engine`, regular expression lowering), never through a
    /// source binding.
    pub loads: &'static [&'static str],
    /// The module text under v38 (`data_limits`), when its per-value
    /// ceilings differ there; earlier profiles keep `source` or `extended`.
    pub lifted: Option<fn() -> &'static str>,
}
/// A profile selector and the prelude source it selects.
pub(crate) type Extension = (fn(LanguageProfile) -> bool, &'static str);
impl Prelude {
    /// The module text this profile compiles.
    pub(crate) fn text(&self, profile: LanguageProfile) -> &'static str {
        if let Some(lifted) = self.lifted
            && profile.data_limits()
        {
            return lifted();
        }
        match self.extended {
            Some((admitted, source)) if admitted(profile) => source,
            _ => self.source,
        }
    }
}

/// Names whose reference binds the prelude export `function` as `local`.
pub(crate) struct Trigger {
    pub names: &'static [&'static str],
    pub function: &'static str,
    pub local: &'static str,
}

/// A static member `object.property` implemented by the prelude export
/// `function`.
pub(crate) struct Member {
    pub object: &'static str,
    pub property: &'static str,
    pub function: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Namespace {
    /// A class: its type and its value.
    Class,
    /// A function value only (`fetch`).
    Value,
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
    members: &[],
    triggers: &[],
    admitted: LanguageProfile::web_headers,
    extended: None,
    loads: &[],
    lifted: None,
};
/// `AbortController`, `AbortSignal` and the `DOMException` abort reasons;
/// v36 (`function_fields`) adds abort listeners in its own source.
pub(crate) const ABORT: Prelude = Prelude {
    path: "<varta>/web/abort.ts",
    source: include_str!("../prelude/abort.ts"),
    exports: &[
        ("AbortController", Namespace::Class),
        (ABORT_SIGNAL_CLASS, Namespace::Class),
        ("DOMException", Namespace::Class),
    ],
    members: &[],
    triggers: &[],
    admitted: LanguageProfile::abort_signals,
    extended: Some((
        LanguageProfile::function_fields,
        include_str!("../prelude/abort_listeners.ts"),
    )),
    loads: &[],
    lifted: None,
};
/// `AbortSignal.timeout` (v36 `timers`): an `AbortController` aborted by an
/// unreferenced timer; it imports the abort prelude implicitly.
pub(crate) const ABORT_TIMEOUT: Prelude = Prelude {
    path: "<varta>/web/abort_timeout.ts",
    source: include_str!("../prelude/abort_timeout.ts"),
    exports: &[],
    members: &[Member {
        object: ABORT_SIGNAL_CLASS,
        property: "timeout",
        function: "abortSignalTimeout",
    }],
    triggers: &[],
    admitted: LanguageProfile::timers,
    extended: None,
    loads: &[],
    lifted: None,
};
/// String and array methods of the v37 `collections` stream, called by
/// `lower::engine` for receivers of the matching type.
pub(crate) const TEXT: Prelude = Prelude {
    path: "<varta>/builtins/text.ts",
    source: include_str!("../prelude/text.ts"),
    exports: &[],
    members: &[
        Member {
            object: "String",
            property: "fromCharCode",
            function: "stringFromCharCode",
        },
        Member {
            object: "String",
            property: "fromCodePoint",
            function: "stringFromCodePoint",
        },
    ],
    triggers: &[],
    admitted: LanguageProfile::collections,
    extended: None,
    loads: TEXT_METHODS,
    lifted: Some(text_v38),
};
/// The member property names that load [`TEXT`].
pub(crate) const TEXT_METHODS: &[&str] = &[
    ".padStart",
    ".padEnd",
    ".toUpperCase",
    ".toLowerCase",
    ".split",
    ".join",
    ".concat",
    ".codePointAt",
];
/// `Map` and `Set` (v37 `collections`): generic functions over the private
/// storage records that `lower::collections` lays out.
pub(crate) const COLLECTIONS: Prelude = Prelude {
    path: "<varta>/builtins/collections.ts",
    source: include_str!("../prelude/collections.ts"),
    exports: &[],
    members: &[],
    triggers: &[],
    admitted: LanguageProfile::collections,
    extended: None,
    loads: &["Map", "Set", "ReadonlyMap", "ReadonlySet"],
    lifted: Some(collections_v38),
};
/// `Uint8Array` operations, `TextEncoder`/`TextDecoder`, base64 and the
/// `crypto` random functions (v37 `collections`).
pub(crate) const BINARY: Prelude = Prelude {
    path: "<varta>/builtins/binary.ts",
    source: include_str!("../prelude/binary.ts"),
    exports: &[
        ("TextEncoder", Namespace::Class),
        ("TextDecoder", Namespace::Class),
        ("TextDecoderOptions", Namespace::Type),
        ("TextDecodeOptions", Namespace::Type),
    ],
    members: &[
        Member {
            object: "crypto",
            property: "getRandomValues",
            function: "cryptoGetRandomValues",
        },
        Member {
            object: "crypto",
            property: "randomUUID",
            function: "cryptoRandomUUID",
        },
    ],
    triggers: &[],
    admitted: LanguageProfile::collections,
    extended: None,
    loads: &["Uint8Array"],
    lifted: Some(binary_v38),
};
/// `btoa` and `atob` (v37 `collections`), separate from [`BINARY`] because
/// their `InvalidCharacterError` is a `DOMException` of the abort prelude.
pub(crate) const BASE64: Prelude = Prelude {
    path: "<varta>/builtins/base64.ts",
    source: include_str!("../prelude/base64.ts"),
    exports: &[("btoa", Namespace::Class), ("atob", Namespace::Class)],
    members: &[],
    triggers: &[],
    admitted: LanguageProfile::collections,
    extended: None,
    loads: &[],
    lifted: None,
};
/// The `Date` subset (v37 `collections`): UTC time values, ISO-8601 and
/// IMF-fixdate parsing, and the `time.now`-gated clock.
pub(crate) const DATE: Prelude = Prelude {
    path: "<varta>/builtins/date.ts",
    source: include_str!("../prelude/date.ts"),
    exports: &[(DATE_CLASS, Namespace::Class)],
    members: &[
        Member {
            object: DATE_CLASS,
            property: "now",
            function: "dateNow",
        },
        Member {
            object: DATE_CLASS,
            property: "parse",
            function: "dateParse",
        },
    ],
    triggers: &[],
    admitted: LanguageProfile::collections,
    extended: None,
    loads: &[],
    lifted: None,
};
pub(crate) const DATE_CLASS: &str = "Date";
/// `RegExp`, `RegExpExecArray` and `RegExpMatchArray` (v37 `regexp`), also
/// loaded by a regular expression literal or a regex string method call.
pub(crate) const REGEXP: Prelude = Prelude {
    path: "<varta>/text/regexp.ts",
    source: include_str!("../prelude/regexp.ts"),
    exports: &[
        (REGEXP_CLASS, Namespace::Class),
        (EXEC_ARRAY_CLASS, Namespace::Class),
        (MATCH_ARRAY_CLASS, Namespace::Class),
    ],
    members: &[],
    triggers: &[],
    admitted: LanguageProfile::regexp,
    extended: None,
    loads: &[
        REGEXP_LITERAL,
        ".match",
        ".matchAll",
        ".replace",
        ".replaceAll",
        ".search",
        ".split",
    ],
    lifted: None,
};
/// `fetch`, `Request`, `Response` and the `ReadableStream` subset (v37
/// `net_request`) over the `net.request` host operations; it imports the
/// headers and abort preludes implicitly.
pub(crate) const FETCH: Prelude = Prelude {
    path: "<varta>/web/fetch.ts",
    source: include_str!("../prelude/fetch.ts"),
    exports: &[
        ("fetch", Namespace::Value),
        ("Request", Namespace::Class),
        ("Response", Namespace::Class),
        (READABLE_STREAM_CLASS, Namespace::Class),
        ("ReadableStreamDefaultReader", Namespace::Class),
        ("RequestInit", Namespace::Type),
        ("ResponseInit", Namespace::Type),
        ("BodyInit", Namespace::Type),
        ("RequestRedirect", Namespace::Type),
        ("ReadableStreamReadResult", Namespace::Type),
        ("ReadableStreamDefaultController", Namespace::Type),
        ("UnderlyingSource", Namespace::Type),
    ],
    members: &[],
    triggers: &[],
    admitted: LanguageProfile::net_request,
    extended: None,
    loads: &[],
    lifted: None,
};
/// `URL` and `URLSearchParams` (v37 `net_request`), a WHATWG subset in
/// guest code with no host import.
pub(crate) const URL: Prelude = Prelude {
    path: "<varta>/web/url.ts",
    source: include_str!("../prelude/url.ts"),
    exports: &[
        ("URL", Namespace::Class),
        ("URLSearchParams", Namespace::Class),
    ],
    members: &[],
    triggers: &[],
    admitted: LanguageProfile::net_request,
    extended: None,
    loads: &[],
    lifted: None,
};
/// `encodeURIComponent` and `encodeURI` (v38 `structural_subtyping`), in
/// guest code with no host import, and the `URIError` they throw.
pub(crate) const URI: Prelude = Prelude {
    path: "<varta>/builtins/uri.ts",
    source: include_str!("../prelude/uri.ts"),
    exports: &[
        ("encodeURIComponent", Namespace::Value),
        ("encodeURI", Namespace::Value),
        ("URIError", Namespace::Class),
    ],
    members: &[],
    triggers: &[],
    admitted: LanguageProfile::structural_subtyping,
    extended: None,
    loads: &[],
    lifted: None,
};
/// The recoverable denial of v37 timer starts (`permissionDenied`, a
/// `NotAllowedError` `DOMException`); it imports the abort prelude.
pub(crate) const PERMISSION: Prelude = Prelude {
    path: "<varta>/web/permission.ts",
    source: include_str!("../prelude/permission.ts"),
    exports: &[],
    members: &[],
    triggers: &[Trigger {
        names: &["setTimeout", "setInterval", "varta.web.unrefTimeout"],
        function: "permissionDenied",
        local: PERMISSION_DENIED,
    }],
    admitted: LanguageProfile::net_request,
    extended: None,
    loads: &[],
    lifted: None,
};
/// The dotted name the timer lowering calls for a denied timer start.
pub(crate) const PERMISSION_DENIED: &str = "varta.permission.denied";
/// Permission broker v1 (v38 `turn_budgets`): `varta.permissions.query` and
/// `varta.permissions.request`, bound under their dotted names.
pub(crate) const PERMISSIONS: Prelude = Prelude {
    path: "<varta>/app/permissions.ts",
    source: include_str!("../prelude/permissions.ts"),
    exports: &[
        ("PermissionState", Namespace::Type),
        ("PermissionDescriptor", Namespace::Type),
        ("PermissionStatus", Namespace::Type),
    ],
    members: &[],
    triggers: &[
        Trigger {
            names: &["varta.permissions.query"],
            function: "permissionsQuery",
            local: "varta.permissions.query",
        },
        Trigger {
            names: &["varta.permissions.request"],
            function: "permissionsRequest",
            local: "varta.permissions.request",
        },
    ],
    admitted: LanguageProfile::turn_budgets,
    extended: None,
    loads: &[],
    lifted: None,
};
/// Host event sources (v38 `turn_budgets`): `varta.events.listen` and
/// `varta.events.close`, bound under their dotted names.
pub(crate) const EVENTS: Prelude = Prelude {
    path: "<varta>/app/events.ts",
    source: include_str!("../prelude/events.ts"),
    exports: &[],
    members: &[],
    triggers: &[
        Trigger {
            names: &["varta.events.listen"],
            function: "eventsListen",
            local: "varta.events.listen",
        },
        Trigger {
            names: &["varta.events.close"],
            function: "eventsClose",
            local: "varta.events.close",
        },
    ],
    admitted: LanguageProfile::turn_budgets,
    extended: None,
    loads: &[],
    lifted: None,
};
/// The L0 view protocol (v38 `view_protocol`): the batch encoder, the event
/// dispatcher and the template bindings, reached through `View.*` members
/// (also what TSX templates compile to) and the `ViewEvent` class.
pub(crate) const VIEW: Prelude = Prelude {
    path: VIEW_PATH,
    source: include_str!("../prelude/view.ts"),
    exports: &[(VIEW_EVENT_CLASS, Namespace::Class)],
    members: &[
        Member {
            object: VIEW_OBJECT,
            property: "node",
            function: "allocateNode",
        },
        Member {
            object: VIEW_OBJECT,
            property: "handler",
            function: "registerHandler",
        },
        Member {
            object: VIEW_OBJECT,
            property: "createElement",
            function: "createElement",
        },
        Member {
            object: VIEW_OBJECT,
            property: "createText",
            function: "createText",
        },
        Member {
            object: VIEW_OBJECT,
            property: "setText",
            function: "setText",
        },
        Member {
            object: VIEW_OBJECT,
            property: "setAttribute",
            function: "setAttribute",
        },
        Member {
            object: VIEW_OBJECT,
            property: "removeAttribute",
            function: "removeAttribute",
        },
        Member {
            object: VIEW_OBJECT,
            property: "setClass",
            function: "setClass",
        },
        Member {
            object: VIEW_OBJECT,
            property: "setStyle",
            function: "setStyle",
        },
        Member {
            object: VIEW_OBJECT,
            property: "removeStyle",
            function: "removeStyle",
        },
        Member {
            object: VIEW_OBJECT,
            property: "insert",
            function: "insert",
        },
        Member {
            object: VIEW_OBJECT,
            property: "remove",
            function: "remove",
        },
        Member {
            object: VIEW_OBJECT,
            property: "listen",
            function: "listen",
        },
        Member {
            object: VIEW_OBJECT,
            property: "unlisten",
            function: "unlisten",
        },
        Member {
            object: VIEW_OBJECT,
            property: "setValue",
            function: "setValue",
        },
        Member {
            object: VIEW_OBJECT,
            property: "setChecked",
            function: "setChecked",
        },
        Member {
            object: VIEW_OBJECT,
            property: "command",
            function: "command",
        },
        Member {
            object: VIEW_OBJECT,
            property: "stylesheet",
            function: "stylesheet",
        },
        Member {
            object: VIEW_OBJECT,
            property: "setTitle",
            function: "setTitle",
        },
        Member {
            object: VIEW_OBJECT,
            property: "copyText",
            function: "copyText",
        },
        Member {
            object: VIEW_OBJECT,
            property: "onLinkResult",
            function: "onLinkResult",
        },
        Member {
            object: VIEW_OBJECT,
            property: "commit",
            function: "commit",
        },
        Member {
            object: VIEW_OBJECT,
            property: "mount",
            function: "mount",
        },
        Member {
            object: VIEW_OBJECT,
            property: "setEffect",
            function: "setEffect",
        },
        Member {
            object: VIEW_OBJECT,
            property: "bindText",
            function: "bindText",
        },
        Member {
            object: VIEW_OBJECT,
            property: "bindAttribute",
            function: "bindAttribute",
        },
        Member {
            object: VIEW_OBJECT,
            property: "bindFlag",
            function: "bindFlag",
        },
        Member {
            object: VIEW_OBJECT,
            property: "bindClass",
            function: "bindClass",
        },
        Member {
            object: VIEW_OBJECT,
            property: "bindStyle",
            function: "bindStyle",
        },
        Member {
            object: VIEW_OBJECT,
            property: "bindValue",
            function: "bindValue",
        },
        Member {
            object: VIEW_OBJECT,
            property: "bindChecked",
            function: "bindChecked",
        },
        Member {
            object: VIEW_OBJECT,
            property: "on",
            function: "on",
        },
    ],
    triggers: &[],
    admitted: LanguageProfile::view_protocol,
    extended: None,
    loads: &[],
    lifted: None,
};
pub(crate) const VIEW_PATH: &str = "<varta>/view/view.ts";
/// The object of the `View.*` members.
pub(crate) const VIEW_OBJECT: &str = "View";
pub(crate) const VIEW_EVENT_CLASS: &str = "ViewEvent";
/// The view prelude function the synthesized event export calls.
pub(crate) const VIEW_DISPATCH: &str = "dispatchViewEvent";
pub(crate) const PRELUDES: [&Prelude; 16] = [
    &HEADERS,
    &ABORT,
    &ABORT_TIMEOUT,
    &TEXT,
    &COLLECTIONS,
    &BINARY,
    &BASE64,
    &DATE,
    &REGEXP,
    &FETCH,
    &PERMISSION,
    &URL,
    &PERMISSIONS,
    &EVENTS,
    &URI,
    &VIEW,
];
/// The `fetch` prelude's stream class and its engine-internal members, which
/// only engine prelude modules may call.
pub(crate) const READABLE_STREAM_CLASS: &str = "ReadableStream";
pub(crate) const INTERNAL_MEMBERS: &[(&str, &str)] = &[
    (READABLE_STREAM_CLASS, "stateOf"),
    ("Request", "bodyText"),
    ("Request", "signalOf"),
    ("URLSearchParams", "parse"),
    ("URLSearchParams", "attach"),
    ("URL", "adoptQuery"),
];
/// Class names that compiler hooks recognize in their prelude modules.
pub(crate) const HEADERS_CLASS: &str = "Headers";
pub(crate) const ABORT_SIGNAL_CLASS: &str = "AbortSignal";
pub(crate) const REGEXP_CLASS: &str = "RegExp";
/// Prelude classes whose JavaScript JSON form is not their own properties
/// (`URL.prototype.toJSON`, array-valued match results); v37 keeps them out
/// of `JSON.stringify` and `unknown`.
pub(crate) const JSON_FORM_CLASSES: [&str; 3] = ["URL", EXEC_ARRAY_CLASS, MATCH_ARRAY_CLASS];
pub(crate) const EXEC_ARRAY_CLASS: &str = "RegExpExecArray";
pub(crate) const MATCH_ARRAY_CLASS: &str = "RegExpMatchArray";
/// The reference key of a regular expression literal; no identifier or
/// property can spell it.
const REGEXP_LITERAL: &str = "/regexp/";
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

/// The v37 builtin preludes (`collections`), whose class instances are
/// private to every host value reader.
pub(crate) const BUILTINS: [&Prelude; 5] = [&TEXT, &COLLECTIONS, &BINARY, &BASE64, &DATE];

/// Whether `source` is a v37 builtin prelude module.
pub(crate) fn is_builtin_source(source: &crate::source::Source) -> bool {
    BUILTINS
        .into_iter()
        .any(|prelude| source.is_path(Path::new(prelude.path)))
}

#[cfg(test)]
thread_local! {
    static WITHOUT_PRELUDES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
/// Test-only: run `compile` as if the engine had no preludes, so a program's
/// bytes can be compared with the same profile's output without the Web types.
#[cfg(test)]
pub(crate) fn without_preludes<T>(compile: impl FnOnce() -> T) -> T {
    WITHOUT_PRELUDES.with(|flag| flag.set(true));
    let result = compile();
    WITHOUT_PRELUDES.with(|flag| flag.set(false));
    result
}

/// Names bound at the top level of a module: declarations and imports.
/// Every name a pattern binds anywhere in `module` (parameters, variables,
/// catch bindings), at any depth.
fn local_bindings(module: &ast::Module) -> BTreeSet<String> {
    #[derive(Default)]
    struct Locals(BTreeSet<String>);
    impl Visit for Locals {
        fn visit_binding_ident(&mut self, binding: &ast::BindingIdent) {
            self.0.insert(binding.id.sym.to_string());
        }
    }
    let mut locals = Locals::default();
    module.visit_with(&mut locals);
    locals.0
}

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
    /// `Object.member` (and a longer chain of identifiers) also records its
    /// dotted name, and every member `x.name` records `.name` (v37 load
    /// triggers).
    fn visit_member_expr(&mut self, member: &ast::MemberExpr) {
        if let Some(name) = dotted(member) {
            self.found.entry(name).or_insert(member.span);
        }
        if let ast::MemberProp::Ident(property) = &member.prop {
            self.found
                .entry(format!(".{}", property.sym))
                .or_insert(member.span);
        }
        member.visit_children_with(self);
    }
    fn visit_regex(&mut self, regex: &ast::Regex) {
        self.found
            .entry(REGEXP_LITERAL.into())
            .or_insert(regex.span);
    }
}

/// `a.b.c` for a member chain of identifiers.
fn dotted(member: &ast::MemberExpr) -> Option<String> {
    let ast::MemberProp::Ident(property) = &member.prop else {
        return None;
    };
    let object = match &*member.obj {
        ast::Expr::Ident(object) => object.sym.to_string(),
        ast::Expr::Member(inner) => dotted(inner)?,
        _ => return None,
    };
    Some(format!("{object}.{}", property.sym))
}

/// Implicit prelude imports of one module, in prelude order. A prelude module
/// itself only imports what another prelude exports.
pub(super) fn implicit_imports(
    path: &Path,
    module: &ast::Module,
    imports: &[Dependency],
    profile: LanguageProfile,
) -> Vec<Dependency> {
    #[cfg(test)]
    if WITHOUT_PRELUDES.with(std::cell::Cell::get) {
        return Vec::new();
    }
    let admitted: Vec<&Prelude> = PRELUDES
        .into_iter()
        .filter(|prelude| (prelude.admitted)(profile) && Path::new(prelude.path) != path)
        .collect();
    if admitted.is_empty() {
        return Vec::new();
    }
    let mut references = References::default();
    module.visit_with(&mut references);
    let bound = top_level(module, imports);
    let locals = local_bindings(module);
    admitted
        .into_iter()
        .filter_map(|prelude| {
            let mut bindings: Vec<ImportBinding> = prelude
                .exports
                .iter()
                .filter(|(name, _)| !bound.contains(*name))
                // A module that binds a value export's name locally (v37
                // `fetch` as a parameter) means its own binding: functions
                // cannot be shadowed, so the global is not imported there.
                .filter(|(name, namespace)| {
                    *namespace != Namespace::Value || !locals.contains(*name)
                })
                .filter_map(|(name, namespace)| {
                    references.found.get(*name).map(|span| ImportBinding {
                        imported: (*name).into(),
                        local: (*name).into(),
                        span: *span,
                        type_only: *namespace == Namespace::Type,
                    })
                })
                .collect();
            bindings.extend(
                prelude
                    .members
                    .iter()
                    .filter(|member| !bound.contains(member.object))
                    .filter_map(|member| {
                        let local = format!("{}.{}", member.object, member.property);
                        references.found.get(&local).map(|span| ImportBinding {
                            imported: member.function.into(),
                            local,
                            span: *span,
                            type_only: false,
                        })
                    }),
            );
            bindings.extend(prelude.triggers.iter().filter_map(|trigger| {
                trigger
                    .names
                    .iter()
                    .filter(|name| !bound.contains(**name))
                    .find_map(|name| references.found.get(*name))
                    .map(|span| ImportBinding {
                        imported: trigger.function.into(),
                        local: trigger.local.into(),
                        span: *span,
                        type_only: false,
                    })
            }));
            // A load trigger imports the module without a binding.
            let span = bindings.first().map(|binding| binding.span).or_else(|| {
                prelude
                    .loads
                    .iter()
                    .find_map(|name| references.found.get(*name).copied())
            })?;
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
    (
        EXEC_ARRAY_CLASS,
        &[CONSTRUCTOR],
        "new RegExpExecArray() (match arrays are created by RegExp.prototype.exec, String.prototype.replace callbacks and String.prototype.matchAll)",
    ),
    (
        MATCH_ARRAY_CLASS,
        &[CONSTRUCTOR],
        "new RegExpMatchArray() (match arrays are created by String.prototype.match)",
    ),
    (
        REGEXP_CLASS,
        &["compile"],
        "RegExp.prototype.compile (Annex B recompilation; v37 compiles every pattern ahead of time)",
    ),
    (
        REGEXP_CLASS,
        &["toString"],
        "RegExp.prototype.toString (class members cannot override the inherited toString; build '/' + source + '/' + flags)",
    ),
    (
        REGEXP_CLASS,
        &["escape"],
        "RegExp.escape (its result could only be compiled at run time; v37 compiles every pattern ahead of time)",
    ),
];

/// Array methods a match array does not have: its elements are read by
/// index, destructuring or `for…of` (v37 `regexp`).
const MATCH_ARRAY_METHODS: &[&str] = &[
    "at",
    "concat",
    "entries",
    "every",
    "filter",
    "find",
    "findIndex",
    "forEach",
    "includes",
    "indexOf",
    "join",
    "keys",
    "lastIndexOf",
    "map",
    "pop",
    "push",
    "reduce",
    "slice",
    "some",
    "values",
];
const MATCH_ARRAY_REASON: &str =
    "array methods of a RegExp match array (read its elements by index, destructuring or for…of)";
const PENDING_MATCH_ARRAYS: &[(&str, &[&str], &str)] = &[
    (EXEC_ARRAY_CLASS, MATCH_ARRAY_METHODS, MATCH_ARRAY_REASON),
    (MATCH_ARRAY_CLASS, MATCH_ARRAY_METHODS, MATCH_ARRAY_REASON),
];

/// The diagnostic detail of a pending member of prelude class `class`.
pub(crate) fn pending_detail(class: &str) -> &'static str {
    if [REGEXP_CLASS, EXEC_ARRAY_CLASS, MATCH_ARRAY_CLASS].contains(&class) {
        "not implemented by the checked-f64-utf16-v37 RegExp prelude"
    } else {
        "not implemented by the checked-f64-utf16-v35 engine Web types"
    }
}

/// Members of prelude classes that v36 (`function_fields`, whose abort
/// prelude implements the listeners) still names but does not implement;
/// consulted before [`PENDING`].
const PENDING_FUNCTION_FIELDS: &[(&str, &[&str], &str)] = &[(
    ABORT_SIGNAL_CLASS,
    &["dispatchEvent"],
    "AbortSignal.dispatchEvent (DOM Event objects are not modelled; abort listeners run when the controller aborts)",
)];

/// Why member `name` of prelude class `class` is unsupported, if it is pending.
pub(crate) fn pending(class: &str, name: &str, profile: LanguageProfile) -> Option<&'static str> {
    let find = |table: &[(&str, &[&str], &'static str)]| {
        table
            .iter()
            .find(|(owner, members, _)| *owner == class && members.contains(&name))
            .map(|(_, _, reason)| *reason)
    };
    if profile.function_fields()
        && let Some(reason) = find(PENDING_FUNCTION_FIELDS)
    {
        return Some(reason);
    }
    find(PENDING).or_else(|| find(PENDING_MATCH_ARRAYS))
}

/// Replace the one declaration `from` of a prelude source with `to`.
fn lift(source: &str, from: &str, to: &str) -> String {
    debug_assert_eq!(source.matches(from).count(), 1, "{from}");
    source.replacen(from, to, 1)
}
/// The v38 collections prelude: entries up to the v38 array element limit.
fn collections_v38() -> &'static str {
    static LIFTED: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        lift(
            COLLECTIONS.source,
            COLLECTION_CEILING,
            &format!(
                "const MAX_ENTRIES = {};",
                varta_common::limits::V38_MAX_ARRAY_ELEMENTS
            ),
        )
    });
    &LIFTED
}
/// The v38 binary prelude: `Uint8Array` lengths up to the v38 array element
/// limit.
fn binary_v38() -> &'static str {
    static LIFTED: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        lift(
            BINARY.source,
            BINARY_CEILING,
            &format!(
                "const MAX_LENGTH = {};",
                varta_common::limits::V38_MAX_ARRAY_ELEMENTS
            ),
        )
    });
    &LIFTED
}
/// The v38 text prelude: `Array.prototype.join` concatenates balanced
/// halves, so joining n elements copies O(n log n) code units instead of the
/// O(n^2) of appending to an immutable string once per element.
fn text_v38() -> &'static str {
    static LIFTED: std::sync::LazyLock<String> =
        std::sync::LazyLock::new(|| lift(TEXT.source, LINEAR_JOIN, BALANCED_JOIN));
    &LIFTED
}
const LINEAR_JOIN: &str = "export function arrayJoin<T>(array: T[], separator?: string): string {
  const between = separator === undefined ? ',' : separator;
  let result = '';
  for (let index = 0; index < array.length; index++) {
    if (index > 0) {
      result += between;
    }
    result += joined(array[index]);
  }
  return result;
}";
const BALANCED_JOIN: &str = "// Elements below which a run is appended directly.
const JOIN_RUN = 32;

function joinRange<T>(array: T[], between: string, start: number, end: number): string {
  if (end - start <= JOIN_RUN) {
    let result = '';
    for (let index = start; index < end; index++) {
      if (index > start) {
        result += between;
      }
      result += joined(array[index]);
    }
    return result;
  }
  const middle = start + Math.floor((end - start) / 2);
  return joinRange(array, between, start, middle) + between + joinRange(array, between, middle, end);
}

export function arrayJoin<T>(array: T[], separator?: string): string {
  const between = separator === undefined ? ',' : separator;
  return joinRange(array, between, 0, array.length);
}";
/// The frozen per-value ceilings of the v37 collection preludes.
const COLLECTION_CEILING: &str = "const MAX_ENTRIES = 1024;";
const BINARY_CEILING: &str = "const MAX_LENGTH = 1024;";
