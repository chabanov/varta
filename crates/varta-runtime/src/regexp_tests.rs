//! V37 `regexp`: regular expression literals and constant `new RegExp`
//! compiled ahead of time into guest matcher programs, the engine `RegExp`
//! prelude, match arrays and the regex string methods. Expected logs of the
//! `regexp_cases` programs were produced by Node 24 (type stripping, `main()`
//! appended) and are pinned here, so the tests need no Node installation.
use super::*;
use varta_common::{HostOperation as H, language::LanguageProfile as P};

const V36: P = P::CheckedF64Utf16V36;
const V37: P = P::CheckedF64Utf16V37;

fn compile(source: &str, profile: P) -> Result<Vec<u8>, varta_compiler::CompilerError> {
    varta_compiler::compile_typescript_with_profile(source, profile)
}
fn module(source: &str) -> Vec<u8> {
    let bytes = compile(source, V37).unwrap_or_else(|e| panic!("{e}\n{source}"));
    // Emitted modules must also pass backend validation, not only compilation.
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
fn lines(expected: &[&str]) -> Vec<String> {
    expected.iter().map(|s| format!("varta log: {s}")).collect()
}
/// Run under the production budgets.
fn run(source: &str, expected: &[&str]) {
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&module(source), &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        lines(expected),
    );
}
fn rejected(source: &str, profile: P) -> String {
    compile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("accepted under {profile:?}:\n{source}"))
        .to_string()
}
/// Fuel and host calls a program consumes under the production budgets, or
/// the failure.
fn cost(source: &str) -> Result<(u64, u32), RuntimeError> {
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&module(source))?;
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None)?;
    let instance = execution.instantiate_prepared(&module)?;
    let store = &mut execution.store;
    let start = instance
        .get_func(&mut *store, "_start")
        .ok_or_else(|| wasmtime::Error::msg("missing _start"))?;
    start.call(&mut *store, &[], &mut [])?;
    crate::exceptions::finish(&instance, store)?;
    crate::scheduler::checkpoint(&instance, store)?;
    Ok((abi::FUEL - store.get_fuel()?, store.data().host_calls))
}
/// Run with a test-only fuel budget of `budget` production budgets, so long
/// corpora fit and allocation churn forces collections.
fn run_with_budget(source: &str, budget: u64) -> Vec<String> {
    let bytes = module(source);
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(budget * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    crate::exceptions::finish(&instance, &mut execution.store).unwrap();
    execution.store.data().output.lines.clone()
}
fn imports(bytes: &[u8]) -> Vec<H> {
    let module = wasmtime::Module::new(&VartaRuntime::new().engine, bytes).unwrap();
    module
        .imports()
        .map(|i| H::from_name(i.name()).unwrap())
        .collect()
}

const EXEC_BASICS: &[&str] = &[
    "2|\"abc\"",
    "null",
    "4|\"abc\"",
    "0|\"a\\nc\"",
    "2|\"bcab\"",
    "3|\"xyz\"",
    "1|\"1.25.3\"",
    "5|\"bob@example.com\"",
    "1|\" \\t\\n \"",
    "3|\"1! x \"",
    "0|\"\"",
    "0|\"aaab\"",
    "0|\"a\"",
    "0|\"aaa\"",
    "0|\"aa\"",
    "2|\"aa\"",
    "0|\"aaaaa\"",
    "1|\"y\"",
    "0|\"abcd\"|\"a\"|\"bcd\"|\"\"",
    "0|\"a\"",
    "0|\"abc\"",
    "0|\"b\"|~|\"b\"",
    "0|\"ab\"|\"b\"|~",
    "0|\"ab\"|~",
    "0|\"zaacbbbcac\"|\"z\"|\"ac\"|\"a\"|~|\"c\"",
    "0|\"\"|~",
    "0|\"\"|\"\"",
    "0|\"\"|~",
    "0|\"a\"|\"a\"",
    "0|\"aa\"|~",
    "0|\"aab\"|\"a\"",
    "0|\"xxy\"",
    "3|\"2026-10\"|\"2026\"|\"10\"",
    "0|\"abc\"",
    "null",
    "3|\"abc\"",
    "0|\"\"",
    "2|\"b\"",
    "0|\"a\"",
    "2|\"  \"",
    "2|\"foo\"",
    "null",
    "1|\"oo\"",
    "null",
    "1|\"\\b\"",
    "0|\"ABC\\u0000\"",
    "1|\"\\n\"",
    "1|\"-]-\"",
    "0|\"a]{\"",
    "0|\"a{1\"",
    "15|\"///\"",
    "0|\"//\"",
    "1|\"a\"@2 3|\"a\"@4 5|\"a\"@6 null@0 ",
    "0|\"an\"@2 2|\"an\"@4 null@0 ",
    "0|\"\"@0 1|\"aa\"@3 3|\"\"@3 4|\"\"@4 null@0 ",
    "0|\"\"@0 1|\"\"@1 2|\"\"@2 null@0 ",
    "1|\"1\"@2 3|\"22\"@5 6|\"333\"@9 null@0 ",
];

#[test]
fn exec_matches_node_over_the_core_pattern_corpus() {
    assert_eq!(
        run_with_budget(include_str!("regexp_cases/exec_basics.ts"), 4),
        lines(EXEC_BASICS)
    );
}

const CASE_UNICODE: &[&str] = &[
    "4|\"HeLLo\"",
    "3|\"ABCdef\"",
    "6|\"123\"",
    "false",
    "true",
    "true",
    "true",
    "true",
    "true",
    "false",
    "true",
    "true",
    "false",
    "false",
    "true",
    "true",
    "true",
    "false",
    "true",
    "true",
    "true",
    "true",
    "true",
    "true",
    "true",
    "true",
    "0|\"😀\"",
    "0|\"\\ud83d\"",
    "0|\"😀\"",
    "null",
    "1|\"😁😂\"",
    "1|\"😀\"",
    "1|\"😀\"",
    "2|\"\\ud83d\"",
    "0|\"\\ud83d\"",
    "0|\"😀\"",
    "1|\"😀b\"",
    "0|\"😀😁\"|\"😀\"|\"😁\"",
    "3|\"a\"",
    "0|\"\\u0001\\u001a\"",
    "0|\"\\\\c1\"",
    "0|\"\\u0011\"",
    "0|\"\\\\\"",
    "0|\"\\n\\u0000\"",
    "0|\"89\"",
    "0|\"x4\"",
    "0|\"u12\"",
    "0|\"ae-\"",
    "0|\"1-z\"",
    "0|\"k\"",
    "0|\"ā\"",
    "0|\"uuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuuu\"",
    "0|\"p\"",
];

#[test]
fn case_folding_unicode_mode_and_annex_b_escapes_match_node() {
    assert_eq!(
        run_with_budget(include_str!("regexp_cases/case_unicode.ts"), 4),
        lines(CASE_UNICODE)
    );
}

const STRING_METHODS: &[&str] = &[
    "5",
    "-1",
    "-1",
    "1",
    "0",
    "0|\"2026-10\"|\"2026\"|\"10\"",
    "-|\"1\"|\"22\"|\"333\"",
    "null",
    "-|\"\"|\"\"|\"\"|\"\"",
    "-|\"\"|\"\"|\"\"|\"\"",
    "\"k\"",
    "\"v\"",
    "2",
    "\"0:a1:1 2:b22:22 5:c333:333 \"",
    "2",
    "5",
    "\"path\"",
    "\"path\"",
    "\"a+b-c\"",
    "\"a+b+c\"",
    "\"a+b+c\"",
    "\"Smith, John\"",
    "\"a[b|a|c|$|$0|$1|$<n>]c\"",
    "\"a[b||$2|b|b0|$<n]c\"",
    "\"x1xx\"",
    "\"-a-a-a-\"",
    "\"xxba\"",
    "\".😀.😀.\"",
    "\".\\ud83d.\\ude00.\\ud83d.\\ude00.\"",
    "\"[\\\"a\\\",\\\"b\\\",\\\"\\\",\\\"c\\\"]\"",
    "\"[\\\"a\\\",\\\"b\\\"]\"",
    "\"[\\\"a\\\",\\\"b\\\",\\\"c\\\"]\"",
    "\"[\\\"\\\"]\"",
    "\"[]\"",
    "\"[\\\"abc\\\"]\"",
    "\"[]\"",
    "\"[\\\"a\\\",\\\"b\\\",\\\"c\\\"]\"",
    "\"[\\\"a\\\",\\\"1\\\",\\\"b\\\",\\\"2\\\",\\\"c\\\"]\"",
    "\"[\\\"a\\\",\\\"b\\\",\\\"c\\\"]\"",
    "\"[\\\"a\\\",\\\"b\\\",\\\"c\\\"]\"",
    "\"[\\\"😀\\\",\\\"x\\\"]\"",
    "\"[\\\"\\\\ud83d\\\",\\\"\\\\ude00\\\",\\\"x\\\"]\"",
    "\"[\\\"\\\"]\"",
    "\"[]\"",
    "\"[\\\"a\\\",\\\"b\\\"]\"",
    "\"[\\\"t\\\",\\\"e\\\",\\\"s\\\",\\\"t\\\"]\"",
    "\"a!b.c\"",
    "\"a!b!c\"",
    "\"a[bac$$1]c\"",
    "\"-a-a-a-\"",
    "\"-abc\"",
    "\"xyz\"",
];

#[test]
fn string_methods_match_node() {
    assert_eq!(
        run_with_budget(include_str!("regexp_cases/string_methods.ts"), 4),
        lines(STRING_METHODS)
    );
}

const REPLACERS: &[&str] = &[
    "\"a(1)b(2)c(3)\"",
    "\"a(1)b22c333\"",
    "\"a[1]b[22]c[333]\"",
    "\"v1=k1;v2=k2\"",
    "\"yx\"",
    "\"a~0~b1\"",
    "\"a1:ab\"",
    "\"10/2026 0 2026-10\"",
    "\"<>a<>a<>a<>\"",
    "\"1 2 3\"",
    "\"a..b.c\"",
    "\"a<.@1/5>b<.@3/5>c\"",
    "\"[0]abc\"",
];

#[test]
fn replacer_functions_match_node() {
    assert_eq!(
        run_with_budget(include_str!("regexp_cases/replacers.ts"), 4),
        lines(REPLACERS)
    );
}

const MATCH_VALUES: &[&str] = &[
    "number identifier blank other",
    "(\\w+)@(\\w+)\\.(\\w+) g truefalsefalsefalsefalsefalsefalse",
    "a\\/b gimsuy (?:)",
    "true",
    "ann@example.org ann example 6 35 21",
    "ann@example.org;ann;example;org;",
    "bob@test.dev 35",
    "x@y.z",
    "truefalse",
    "42",
    "zzz",
    "true2false0",
    "false0",
];

#[test]
fn regexps_and_match_arrays_are_ordinary_values() {
    assert_eq!(
        run_with_budget(include_str!("regexp_cases/match_values.ts"), 1),
        lines(MATCH_VALUES)
    );
}

/// Guest fuel of `pattern.test(input)` beyond building the input.
fn test_fuel(pattern: &str, input: &str) -> Option<u64> {
    let base = format!("export function main():void{{const s={input};}}");
    let program = format!("export function main():void{{const s={input};{pattern}.test(s);}}");
    let (empty, _) = cost(&base).ok()?;
    let (fuel, _) = cost(&program).ok()?;
    Some(fuel - empty)
}

/// Patterns that backtrack catastrophically in backtracking engines run in
/// time linear in the input: doubling the input at most doubles the fuel
/// (plus the fixed cost of one run), and every result matches Node.
#[test]
fn adversarial_patterns_run_in_linear_time_within_fuel() {
    run(
        include_str!("regexp_cases/redos.ts"),
        &["false false false false false false true", "true true true"],
    );
    for (pattern, unit, tail) in [
        (r"/^(a+)+$/", "a", "!"),
        (r"/(a|a)*b/", "a", "!"),
        (r"/(a|aa)+c/", "a", "!"),
        (r"/^(\w+\s?)*$/", "a", "!"),
        (r"/(x+x+)+y/", "x", ""),
        (r"/\s+$/", " ", "x"),
    ] {
        let input = |n: usize| format!("'{unit}'.repeat({n}) + '{tail}'");
        let short = test_fuel(pattern, &input(100)).expect("100 units fit the budget");
        let long = test_fuel(pattern, &input(200)).expect("200 units fit the budget");
        // Linear: the second 100 units cost about what the first did.
        assert!(long < 2 * short + 2_000, "{pattern}: {short} -> {long}");
        // A long adversarial input exhausts the fuel instead of hanging.
        let program = format!(
            "export function main():void{{const s={};{pattern}.test(s);}}",
            input(100_000)
        );
        let error = cost(&program).expect_err(pattern);
        assert!(
            format!("{error:?}").contains("fuel"),
            "{pattern}: {error:?}"
        );
    }
}

/// Measured guest fuel per input code unit under the production budgets
/// (the contract's figures): a literal-led search scans about 23 fuel per
/// unit, and a nested quantifier steps every live thread per unit.
#[test]
fn matcher_fuel_per_unit_stays_within_the_contract() {
    let per_unit = |pattern: &str, unit: &str, tail: &str| {
        let input = |n: usize| format!("'{unit}'.repeat({n}) + '{tail}'");
        let short = test_fuel(pattern, &input(250)).unwrap();
        let long = test_fuel(pattern, &input(500)).unwrap();
        (long - short) / 250
    };
    let scan = per_unit(r"/\r\n|\r|\n/", "a", "");
    assert!(scan <= 25, "{scan}");
    let class = per_unit(r"/[0-9]/", "a", "");
    assert!(class <= 40, "{class}");
    let nested = per_unit(r"/^(a+)+$/", "a", "!");
    assert!(nested <= 1_300, "{nested}");
    // One run's fixed cost (allocation, header, the RegExp object).
    let fixed = test_fuel("/b/", "'a'").unwrap();
    assert!(fixed <= 4_000, "{fixed}");
}

#[test]
fn match_arrays_and_regexps_survive_forced_collections() {
    // A 16-unit budget-heavy churn grows the heap past its committed pages,
    // so the collector runs while match arrays and RegExps are live.
    assert_eq!(
        run_with_budget(include_str!("regexp_cases/collect.ts"), 400),
        lines(&["ann example org 2026 10 [a-z]+ 4 199"])
    );
}

#[test]
fn regexps_need_no_capability_and_add_no_host_import() {
    let plain = imports(&module("export function main():void{}"));
    let source = "export function main():void{const m=/(\\d+)-(?<b>\\d+)/g.exec('1-2');\
        const s='a,b'.split(/,/);const r='x'.replace(/x/g,'y');const t='a b'.search(/\\s/);\
        const n='aa'.match(/a/g);const all='a1'.matchAll(/\\d/g);}";
    assert_eq!(imports(&module(source)), plain);
    let (_, host_calls) = cost(source).unwrap();
    assert_eq!(host_calls, 0);
}

#[test]
fn pilot_shaped_text_work_stays_within_production_budgets() {
    run(
        include_str!("regexp_cases/pilot_text.ts"),
        &[
            "https://api.example.com/v1/models",
            "HTTPS://other.example/x",
            "a%20b%2Bc~",
            "event:message|data:{\"a\":1}||data:2||:keep-alive|",
            "6",
        ],
    );
    let (fuel, host_calls) = cost(include_str!("regexp_cases/pilot_text.ts")).unwrap();
    // Logging and one Number-to-string conversion are the only host work.
    assert_eq!(host_calls, 6);
    assert!(fuel < abi::FUEL / 2, "{fuel}");
}

#[test]
fn regexp_stays_rejected_before_v37() {
    for source in [
        "export function main():void{const r=/a/;}",
        "export function main():void{const r=new RegExp('a');}",
        "export function main():void{const p='a,b'.split(',');}",
        "export function main():void{const s='ab'.replace('a','b');}",
    ] {
        let v36 = rejected(source, V36);
        let v35 = rejected(source, P::CheckedF64Utf16V35);
        assert_eq!(v36, v35, "{source}");
        assert!(!v36.contains("v37"), "{v36}");
    }
}

#[test]
fn unsupported_regexp_forms_fail_with_explicit_diagnostics() {
    for (source, message) in [
        ("const r=/(a)\\1/;", "unsupported RegExp backreference \\1"),
        (
            "const r=/(?<n>a)\\k<n>/;",
            "unsupported RegExp named backreference",
        ),
        (
            "const r=/a(?=b)/;",
            "unsupported RegExp lookahead assertion (?=…)",
        ),
        (
            "const r=/a(?!b)/;",
            "unsupported RegExp negative lookahead assertion (?!…)",
        ),
        (
            "const r=/(?<=a)b/;",
            "unsupported RegExp lookbehind assertion (?<=…)",
        ),
        (
            "const r=/\\p{L}/u;",
            "unsupported RegExp Unicode property escape",
        ),
        ("const r=/a/d;", "unsupported RegExp flag d"),
        ("const r=/a/v;", "unsupported RegExp flag v"),
        ("const r=/(?i:a)/;", "unsupported RegExp pattern modifiers"),
        (
            "const r=/a{100000}/;",
            "unsupported RegExp program larger than",
        ),
        (
            "const r=/(/;",
            "Invalid regular expression: /(/: Unterminated group",
        ),
        ("const r=/a/gg;", "Duplicated regular expression flag 'g'"),
        (
            "const r=new RegExp('a', 'gg');",
            "Invalid flags supplied to RegExp constructor 'gg'",
        ),
        (
            "const p='a';const r=new RegExp(p);",
            "unsupported new RegExp with a pattern or flags that are not string literals",
        ),
        (
            "const r=new RegExp('[');",
            "Invalid regular expression: /[/: Unterminated character class",
        ),
        (
            "const r=new RegExp('a', 'q');",
            "Invalid flags supplied to RegExp constructor 'q'",
        ),
        (
            "const r=/a/;r.compile();",
            "unsupported RegExp.prototype.compile",
        ),
        (
            "const m=/a/.exec('a');const n=new RegExpExecArray(0,'',[],undefined);",
            "unsupported new RegExpExecArray()",
        ),
        (
            "const n='a'.search(1);",
            "expected a RegExp or a string literal pattern",
        ),
        (
            "const n='a'.split(/a/, 'x');",
            "split limit must be a number",
        ),
        (
            "const r=/(a)/;const n='a'.replace(r,(m,a)=>m);",
            "unsupported RegExp replacer parameter beyond",
        ),
        (
            "const n='ab'.replace(/(a)/,(m,a,i,s,x)=>m);",
            "unsupported RegExp replacer parameter beyond",
        ),
        (
            "const n=(1 as number).toFixed(0).replace(/a/, 1);",
            "replacement must be a string or a replacer function",
        ),
        (
            "const m=/a/.exec('a');if(m!==null){const r=m.slice(1);}",
            "unsupported array methods of a RegExp match array",
        ),
        (
            "const r=/a/;const t=r.toString();",
            "unsupported RegExp.prototype.toString",
        ),
    ] {
        let program = format!("export function main():void{{{source}}}");
        let error = rejected(&program, V37);
        assert!(error.contains(message), "{source}\n{error}");
    }
    // Diagnostics point at the pattern.
    let error = compile(
        "export function main():void{\n  const r = /(a)\\1/;\n}",
        V37,
    )
    .unwrap_err();
    let diagnostic = error.diagnostic().expect("located");
    assert_eq!(
        (diagnostic.range.start.line, diagnostic.range.start.column),
        (2, 13)
    );
}
