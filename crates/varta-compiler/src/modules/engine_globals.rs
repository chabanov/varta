//! The engine's global identifiers (v38 `web_bytes`): the names a program
//! reads without declaring or importing them, with their JavaScript `typeof`.
//!
//! `typeof NAME` of such a name folds to its constant `typeof` string where no
//! binding of the program shadows it (feature detection such as
//! `typeof Blob !== 'undefined'`), and the scanner loads no prelude for that
//! operand alone. Every other identifier keeps the ordinary lookup, so an
//! unknown name stays a compile error.
//!
//! The set is derived from the engine's registries: the class and function
//! exports of the preludes the profile admits ([`super::engine::PRELUDES`]),
//! the members of the typed global object ([`GLOBAL_OBJECT_MEMBERS`]), the
//! error classes ([`varta_common::value::ErrorClass`]), the collection kinds
//! ([`crate::ir::CollectionKind`]) and the globals the compiler lowers itself
//! ([`INTRINSIC_GLOBALS`]). Prelude classes whose instances only engine APIs
//! create ([`super::engine::Namespace::Instance`]) and type-only exports are
//! no global values.
use super::engine::{GLOBAL_THIS, Namespace, PRELUDES};
use varta_common::language::LanguageProfile;
use varta_common::value::ErrorClass;

/// What a global identifier denotes, as JavaScript's `typeof` reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GlobalKind {
    /// A class or a function.
    Function,
    /// A namespace or another object (`Math`, `crypto`, `globalThis`).
    Object,
}
impl GlobalKind {
    /// The `typeof` string of a value of this kind.
    pub(crate) fn type_of(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Object => "object",
        }
    }
}

/// The getters of the global prelude's `GlobalObject` (`prelude/global.ts`):
/// the object-valued members of the typed `globalThis`.
pub(crate) const GLOBAL_OBJECT_MEMBERS: [&str; 3] = [GLOBAL_THIS, "crypto", "process"];

/// Globals the compiler lowers itself, with no prelude export: the
/// namespaces of `Math`, `JSON` and `console`, the primitive and object
/// constructors, `Promise`, the global numeric functions and the timers of
/// `lower::promises::timers`.
pub(crate) const INTRINSIC_GLOBALS: [(&str, GlobalKind); 18] = [
    ("Math", GlobalKind::Object),
    ("JSON", GlobalKind::Object),
    ("console", GlobalKind::Object),
    ("Number", GlobalKind::Function),
    ("String", GlobalKind::Function),
    ("Boolean", GlobalKind::Function),
    ("BigInt", GlobalKind::Function),
    ("Object", GlobalKind::Function),
    ("Array", GlobalKind::Function),
    ("Promise", GlobalKind::Function),
    ("parseInt", GlobalKind::Function),
    ("parseFloat", GlobalKind::Function),
    ("isNaN", GlobalKind::Function),
    ("isFinite", GlobalKind::Function),
    ("setTimeout", GlobalKind::Function),
    ("setInterval", GlobalKind::Function),
    ("clearTimeout", GlobalKind::Function),
    ("clearInterval", GlobalKind::Function),
];

/// Whether `profile` folds `typeof` of an engine global.
pub(crate) fn admitted(profile: LanguageProfile) -> bool {
    profile.web_bytes()
}

/// The kind of engine global `name` under `profile`, if it is one.
pub(crate) fn kind(name: &str, profile: LanguageProfile) -> Option<GlobalKind> {
    if !admitted(profile) {
        return None;
    }
    if GLOBAL_OBJECT_MEMBERS.contains(&name) {
        return Some(GlobalKind::Object);
    }
    if let Some((_, kind)) = INTRINSIC_GLOBALS.iter().find(|(global, _)| *global == name) {
        return Some(*kind);
    }
    let prelude = PRELUDES
        .into_iter()
        .filter(|prelude| (prelude.admitted)(profile))
        .flat_map(|prelude| prelude.exports.iter())
        .any(|(export, namespace)| {
            *export == name && matches!(namespace, Namespace::Class | Namespace::Value)
        });
    let builtin = ErrorClass::from_name(name).is_some()
        || crate::ir::CollectionKind::ALL
            .into_iter()
            .any(|kind| kind.name() == name);
    (prelude || builtin).then_some(GlobalKind::Function)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::engine::{Prelude, Trigger};
    use std::collections::BTreeMap;

    const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;

    /// Every engine global of v38 with Node 24.20.0's `typeof` of the same
    /// name (`node -e 'console.log(typeof NAME)'`). A change to the set
    /// changes this table, checked against Node again.
    const NODE_24: &[(&str, &str)] = &[
        ("AbortController", "function"),
        ("AbortSignal", "function"),
        ("Array", "function"),
        ("ArrayBuffer", "function"),
        ("BigInt", "function"),
        ("Blob", "function"),
        ("Boolean", "function"),
        ("DOMException", "function"),
        ("Date", "function"),
        ("Error", "function"),
        ("FormData", "function"),
        ("Headers", "function"),
        ("JSON", "object"),
        ("Map", "function"),
        ("Math", "object"),
        ("Number", "function"),
        ("Object", "function"),
        ("Promise", "function"),
        ("RangeError", "function"),
        ("ReadableStream", "function"),
        ("ReadableStreamDefaultReader", "function"),
        ("RegExp", "function"),
        ("Request", "function"),
        ("Response", "function"),
        ("Set", "function"),
        ("String", "function"),
        ("SyntaxError", "function"),
        ("TextDecoder", "function"),
        ("TextEncoder", "function"),
        ("TypeError", "function"),
        ("URIError", "function"),
        ("URL", "function"),
        ("URLSearchParams", "function"),
        ("Uint8Array", "function"),
        ("atob", "function"),
        ("btoa", "function"),
        ("clearInterval", "function"),
        ("clearTimeout", "function"),
        ("console", "object"),
        ("crypto", "object"),
        ("encodeURI", "function"),
        ("encodeURIComponent", "function"),
        ("fetch", "function"),
        ("globalThis", "object"),
        ("isFinite", "function"),
        ("isNaN", "function"),
        ("parseFloat", "function"),
        ("parseInt", "function"),
        ("process", "object"),
        ("setInterval", "function"),
        ("setTimeout", "function"),
    ];

    /// Every name a prelude registry entry spells: exports, member objects,
    /// triggers and load keys that are identifiers.
    fn prelude_names(prelude: &Prelude) -> impl Iterator<Item = &'static str> + '_ {
        let identifier = |name: &&str| !name.contains('.') && !name.contains('/');
        prelude
            .exports
            .iter()
            .map(|(name, _)| *name)
            .chain(prelude.members.iter().map(|member| member.object))
            .chain(
                prelude
                    .triggers
                    .iter()
                    .flat_map(|trigger: &Trigger| trigger.names.iter().copied()),
            )
            .chain(prelude.loads.iter().copied())
            .filter(identifier)
    }

    #[test]
    fn the_engine_globals_match_node_24() {
        let mut names: Vec<&str> = PRELUDES
            .into_iter()
            .filter(|prelude| (prelude.admitted)(V38))
            .flat_map(prelude_names)
            .chain(GLOBAL_OBJECT_MEMBERS)
            .chain(INTRINSIC_GLOBALS.iter().map(|(name, _)| *name))
            .chain(ErrorClass::ALL.iter().map(|class| class.name()))
            .collect();
        names.sort_unstable();
        names.dedup();
        let found: BTreeMap<&str, &str> = names
            .into_iter()
            .filter_map(|name| kind(name, V38).map(|kind| (name, kind.type_of())))
            .collect();
        let expected: BTreeMap<&str, &str> = NODE_24.iter().copied().collect();
        assert_eq!(found, expected);
    }

    /// The names a prelude registry entry spells that are no global value:
    /// type-only exports and engine-created instances (Node 24: `undefined`),
    /// and the engine's `View` namespace, which no JavaScript host defines.
    #[test]
    fn prelude_names_outside_the_globals_are_types_or_engine_namespaces() {
        let outside: std::collections::BTreeSet<&str> = PRELUDES
            .into_iter()
            .filter(|prelude| (prelude.admitted)(V38))
            .flat_map(|prelude| {
                prelude_names(prelude)
                    .filter(|name| kind(name, V38).is_none())
                    .filter(|name| {
                        !prelude.exports.iter().any(|(export, namespace)| {
                            export == name && *namespace == Namespace::Type
                        })
                    })
            })
            .collect();
        assert_eq!(
            outside.into_iter().collect::<Vec<_>>(),
            [
                "ReadonlyMap",
                "ReadonlySet",
                "RegExpExecArray",
                "RegExpMatchArray",
                "View",
                "ViewEvent"
            ]
        );
    }

    #[test]
    fn the_global_object_members_are_its_getters() {
        let source = crate::modules::engine::GLOBAL.source;
        let class = source
            .split("export class GlobalObject {")
            .nth(1)
            .and_then(|rest| rest.split("\n}").next())
            .unwrap();
        let getters: Vec<&str> = class
            .lines()
            .filter_map(|line| line.trim().strip_prefix("get "))
            .filter_map(|line| line.split('(').next())
            .collect();
        assert_eq!(getters, GLOBAL_OBJECT_MEMBERS);
    }

    #[test]
    fn earlier_profiles_have_no_folded_globals() {
        for &profile in varta_common::language::SUPPORTED_PROFILES {
            let folded = NODE_24
                .iter()
                .any(|(name, _)| kind(name, profile).is_some());
            assert_eq!(folded, profile == V38, "{profile:?}");
        }
    }
}
