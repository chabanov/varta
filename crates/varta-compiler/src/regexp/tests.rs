//! Pattern compiler tests: flags, early errors, the rejected non-linear
//! constructs, `source` escaping, case canonicalization, program headers
//! and limits. Matching behaviour is pinned against Node 24 in
//! `varta-runtime` (`regexp_tests`).
use super::program::{self, op};
use super::*;

fn error(pattern: &str, flags: &str) -> RegExpError {
    literal(pattern, flags).expect_err(pattern)
}
fn units(pattern: &str, flags: &str) -> Vec<u16> {
    literal(pattern, flags)
        .unwrap_or_else(|e| panic!("{pattern}: {e:?}"))
        .compiled
        .units
}

#[test]
fn flags_are_parsed_once_and_printed_in_canonical_order() {
    let compiled = literal("a", "yusmig").unwrap();
    assert_eq!(compiled.flags.text(), "gimsuy");
    for flags in ["gg", "x", "G", "ii"] {
        assert!(
            matches!(error("a", flags), RegExpError::Syntax(m) if m.contains("Invalid flags")),
            "{flags}"
        );
    }
    for (flags, name) in [("d", "flag d"), ("v", "flag v")] {
        assert!(
            matches!(error("a", flags), RegExpError::Unsupported(m) if m.contains(name)),
            "{flags}"
        );
    }
}

#[test]
fn non_linear_constructs_are_rejected_by_name() {
    for (pattern, flags, kind) in [
        (r"(a)\1", "", "RegExp backreference \\1"),
        (r"\1(a)", "", "RegExp backreference \\1"),
        (r"(a)\1", "u", "RegExp backreference \\1"),
        (r"(?<n>a)\k<n>", "", "RegExp named backreference \\k<name>"),
        (r"(?<n>a)\k<n>", "u", "RegExp named backreference \\k<name>"),
        ("a(?=b)", "", "RegExp lookahead assertion (?=…)"),
        ("a(?!b)", "", "RegExp negative lookahead assertion (?!…)"),
        ("(?<=a)b", "", "RegExp lookbehind assertion (?<=…)"),
        ("(?<!a)b", "", "RegExp negative lookbehind assertion (?<!…)"),
        (r"\p{L}", "u", "RegExp Unicode property escape \\p{…}"),
        (r"\P{L}", "u", "RegExp Unicode property escape \\p{…}"),
        ("(?i:a)", "", "RegExp pattern modifiers (?ims-ims:…)"),
        (
            "(?<x>a)|(?<x>b)",
            "",
            "RegExp duplicate named capture groups",
        ),
        (
            "(?<\\u0061>x)",
            "",
            "RegExp escape sequence in a capture group name",
        ),
    ] {
        match error(pattern, flags) {
            RegExpError::Unsupported(found) => {
                assert!(found.starts_with(kind), "{pattern}: {found}");
            }
            other => panic!("{pattern}: {other:?}"),
        }
    }
    // The compiler error names the linear-time subset.
    let message = error(r"(a)\1", "").into_compiler().to_string();
    assert!(
        message.contains("checked-f64-utf16-v37 linear-time RegExp subset"),
        "{message}"
    );
}

#[test]
fn early_errors_follow_the_pattern_grammar() {
    for (pattern, flags, message) in [
        ("(a", "", "Unterminated group"),
        ("a)", "", "Unmatched ')'"),
        ("*a", "", "Nothing to repeat"),
        ("a**", "", "Nothing to repeat"),
        ("^*", "", "Nothing to repeat"),
        ("{1}", "", "Nothing to repeat"),
        ("a{2,1}", "", "numbers out of order in {} quantifier"),
        ("[b-a]", "", "Range out of order in character class"),
        ("[a", "", "Unterminated character class"),
        ("a\\", "", "\\ at end of pattern"),
        ("(?<1a>x)", "", "Invalid capture group name"),
        ("(?a)", "", "Invalid group"),
        ("]", "u", "Lone quantifier brackets"),
        ("a{", "u", "Incomplete quantifier"),
        ("\\a", "u", "Invalid escape"),
        ("\\c1", "u", "Invalid unicode escape"),
        ("\\u{110000}", "u", "Invalid Unicode escape"),
        ("[\\d-z]", "u", "Invalid character class"),
        ("\\01", "u", "Invalid decimal escape"),
        ("\\k", "u", "Invalid named reference"),
    ] {
        match error(pattern, flags) {
            RegExpError::Syntax(found) => assert_eq!(
                found,
                format!("Invalid regular expression: /{pattern}/{flags}: {message}"),
            ),
            other => panic!("{pattern}: {other:?}"),
        }
    }
    // Annex B keeps these valid without `u`.
    for pattern in [
        "]", "a{", "a{1", "{", "}", "\\a", "\\c1", "[\\d-z]", "\\01", "\\k", "\\8",
    ] {
        literal(pattern, "").unwrap_or_else(|e| panic!("{pattern}: {e:?}"));
    }
}

#[test]
fn constructed_sources_escape_slashes_and_line_terminators_like_v8() {
    for (pattern, source) in [
        ("a/b", "a\\/b"),
        ("", "(?:)"),
        ("[/]", "[/]"),
        ("\\/", "\\/"),
        ("\n", "\\n"),
        ("a\\\nb", "a\\nb"),
        ("\u{2028}", "\\u2028"),
    ] {
        assert_eq!(
            constructed(pattern, "").unwrap().source,
            source,
            "{pattern:?}"
        );
    }
    assert_eq!(literal("a\\/b", "g").unwrap().source, "a\\/b");
}

#[test]
fn canonicalize_follows_to_uppercase_without_u_and_simple_folding_with_u() {
    use case::canonicalize;
    let c = |ch: char, unicode: bool| char::from_u32(canonicalize(u32::from(ch), unicode));
    assert_eq!(c('a', false), Some('A'));
    // ſ uppercases to ASCII S, which non-`u` Canonicalize refuses.
    assert_eq!(c('ſ', false), Some('ſ'));
    assert_eq!(c('ſ', true), c('s', true));
    assert_eq!(c('K', true), c('k', true));
    assert_eq!(c('ς', true), c('σ', true));
    assert_eq!(c('ς', false), c('Σ', false));
    // ß uppercases to two characters.
    assert_eq!(c('ß', false), Some('ß'));
    assert_eq!(c('ẞ', true), c('ß', true));
    // The Turkic dotted and dotless I fold to themselves.
    assert_ne!(c('ı', true), c('i', true));
    assert_ne!(c('İ', true), c('i', true));
    assert_eq!(canonicalize(0xD800, false), 0xD800);
}

#[test]
fn headers_describe_anchors_prefilters_and_slots() {
    let header = |pattern: &str, flags: &str| {
        let units = units(pattern, flags);
        (
            units[program::FLAGS],
            units[program::CAPTURES],
            units[program::SLOTS],
            units[program::FIRST_COUNT],
            units[program::FIRST_UNIT],
        )
    };
    assert_eq!(units("a", "")[program::FORMAT], program::FORMAT_VERSION);
    let (flags, ..) = header("^a", "");
    assert_ne!(flags & program::ANCHORED, 0);
    let (flags, ..) = header("^a", "m");
    assert_eq!(flags & program::ANCHORED, 0);
    // `\r\n|\r|\n` starts with CR or LF: a direct two-unit scan.
    let (flags, captures, slots, count, first) = header("\\r\\n|\\r|\\n", "");
    assert_ne!(flags & program::PREFILTER, 0);
    assert_eq!((captures, slots, count, first), (2, 2, 2, 0x0A));
    // A pattern that can match empty has no prefilter.
    let (flags, ..) = header("a*", "");
    assert_eq!(flags & program::PREFILTER, 0);
    // Two groups, and one empty-check register for the nullable `(a?)?`.
    let (_, captures, slots, ..) = header("(a)(b?)?", "");
    assert_eq!((captures, slots), (6, 7));
    let (flags, ..) = header("\\bk", "iu");
    assert_ne!(flags & program::EXTRA_WORD, 0);
    let (flags, ..) = header(".", "u");
    assert_ne!(flags & program::UNICODE, 0);
}

#[test]
fn lean_programs_record_only_the_match_bounds() {
    let compiled = literal("(a)((b)|c)*", "").unwrap().compiled;
    assert_eq!(compiled.groups, 3);
    assert_eq!(compiled.units[program::CAPTURES], 8);
    assert_eq!(compiled.lean[program::CAPTURES], 2);
    let ops = |units: &[u16]| {
        let count = usize::from(units[program::INSTRUCTIONS]);
        (0..count)
            .map(|i| units[program::HEADER_UNITS + i * program::INSTRUCTION_UNITS])
            .collect::<Vec<_>>()
    };
    assert!(ops(&compiled.units).contains(&op::RESET));
    assert!(!ops(&compiled.lean).contains(&op::RESET));
    assert_eq!(
        ops(&compiled.lean)
            .iter()
            .filter(|o| **o == op::SAVE)
            .count(),
        2
    );
    // Without groups both programs are the same.
    let plain = literal("ab+", "").unwrap().compiled;
    assert_eq!(plain.units, plain.lean);
}

#[test]
fn programs_are_bounded() {
    match error("a{100000}", "") {
        RegExpError::Unsupported(kind) => assert!(kind.contains("RegExp program larger"), "{kind}"),
        other => panic!("{other:?}"),
    }
    // Large but bounded repetitions compile.
    let units = units("a{1000}", "");
    assert!(units.len() <= compile::MAX_PROGRAM_UNITS);
}

#[test]
fn v37_programs_without_regexps_compile_as_without_preludes() {
    use varta_common::language::LanguageProfile;
    // Strings, object keys and other property names never load the prelude;
    // a regex literal, the type names or a member named like a regex string
    // method (`split`, `replace`, …) do.
    let source = "export function main():void{const o={RegExp:'x',splits:1};\
        console.log(o.RegExp+'/a/g'+String(o.splits));}";
    let compile = |source: &str| {
        crate::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V37)
    };
    let with = compile(source).unwrap();
    wasmtime::Module::new(&wasmtime::Engine::default(), &with).unwrap();
    assert_eq!(
        with,
        crate::modules::engine::without_preludes(|| compile(source)).unwrap()
    );
    for named in [
        "export function main():void{const r=/a/;}",
        "export function main():void{const p='a,b'.split(',');}",
        "export function main():void{let r:RegExp|null=null;}",
    ] {
        let bytes = compile(named).unwrap_or_else(|e| panic!("{named}: {e}"));
        wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
        assert!(
            crate::modules::engine::without_preludes(|| compile(named)).is_err(),
            "{named}"
        );
    }
}
