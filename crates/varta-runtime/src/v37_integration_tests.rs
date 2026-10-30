//! Cross-stream behaviour of the v37 batch: features of the language-core,
//! class-generics, collections, RegExp and net-request streams used together.
//! Expected logs of the `v37_integration_cases` programs were produced by
//! Node 24 (v24.20.0, type stripping, `main()` appended) and are pinned here;
//! Varta's documented deviations are pinned separately.
use super::*;
use varta_common::language::LanguageProfile as P;
use varta_common::scheduler::TIMER_CAPABILITY;

const V36: P = P::CheckedF64Utf16V36;
const V37: P = P::CheckedF64Utf16V37;

fn compile(source: &str, profile: P) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
fn rejected(source: &str, profile: P) -> String {
    varta_compiler::compile_typescript_with_profile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("{profile:?} accepted\n{source}"))
        .to_string()
}
fn timer_grant() -> Vec<CapabilityGrant> {
    vec![CapabilityGrant {
        id: TIMER_CAPABILITY.into(),
        value: varta_common::CapabilityValue::Boolean(true),
    }]
}
fn lines(expected: &[&str]) -> Vec<String> {
    expected.iter().map(|s| format!("varta log: {s}")).collect()
}
fn run(source: &str, expected: &[&str]) {
    let bytes = compile(source, V37);
    let output = scheduler::with_virtual_clock(|| {
        VartaRuntime::new().run_wasm_bytes(&bytes, &timer_grant())
    })
    .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    assert_eq!(output.lines, lines(expected), "{source}");
}
/// `run`, then the same program with a full collection forced before every
/// job (test-only collector export and fuel for the collections).
fn run_collecting(source: &str, expected: &[&str]) {
    run(source, expected);
    let (log, result) = super::timer_tests::observe_with_profile(source, &timer_grant(), true, V37);
    result.unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    assert_eq!(log, lines(expected), "collected: {source}");
}

const PRIVATE_JSON: &str = include_str!("v37_integration_cases/private_json.ts");

#[test]
fn json_stringify_skips_private_state_like_node() {
    // Node prints `{"status":404,"name":"ApiError"}`: its `name` becomes an
    // own property only when assigned, after the subclass defined `status`.
    // Varta's Error slots are placed by the Error initializer, so an assigned
    // `name` keeps its earlier insertion position (a documented deviation in
    // key order only).
    run(
        PRIVATE_JSON,
        &[
            "{}",
            r#"{"headers":{},"n":1}"#,
            "[{},{}]",
            "{}",
            r#"{"reason":"stop"}"#,
            r#"{"x":1} 2"#,
            r#"{"name":"ApiError","status":404}"#,
            r#"{"code":7}"#,
            r#"{"headers":{},"list":[1,"a"]}"#,
            r#"{"name":"ApiError","status":500}"#,
            "{}",
        ],
    );
}

#[test]
fn json_stringify_imports_the_private_state_reader_only_in_v37() {
    let source = "export function main():void{console.log(JSON.stringify({a:1}));}";
    let imports = |profile| {
        let bytes = compile(source, profile);
        let module = wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
        module
            .imports()
            .map(|import| import.name().to_owned())
            .collect::<Vec<_>>()
    };
    let v37 = imports(V37);
    assert!(
        v37.iter().any(|name| name == "json_stringify_v17"),
        "{v37:?}"
    );
    assert!(
        !v37.iter().any(|name| name == "json_stringify_v16"),
        "{v37:?}"
    );
    let v36 = imports(V36);
    assert!(
        v36.iter().any(|name| name == "json_stringify_v16"),
        "{v36:?}"
    );
}

#[test]
fn private_state_json_rules_stay_frozen_before_v37() {
    // V36 keeps rejecting JSON.stringify of Web type and Error subclass
    // instances, and of `unknown` once such classes exist.
    for source in [
        "export function main():void{console.log(JSON.stringify(new Headers()));}",
        "class E extends Error{}export function main():void{console.log(JSON.stringify(new E('m')));}",
        "export function main():void{const s=new AbortController().signal;\
         const u:unknown={s};console.log(JSON.stringify(u));}",
    ] {
        let error = rejected(source, V36);
        assert!(error.contains("JSON.stringify"), "{error}");
    }
}

#[test]
fn json_forms_and_reserved_keys_are_rejected_by_name_in_v37() {
    for (source, expected) in [
        (
            "export function main():void{console.log(JSON.stringify(new URL('http://a/')));}",
            "JSON.stringify of a URL",
        ),
        (
            "export function main():void{const u:unknown=new URL('http://a/');console.log(typeof u);}",
            "not assignable to unknown",
        ),
        (
            "export function main():void{const m='ab'.match(/a/);console.log(JSON.stringify(m));}",
            "RegExp match array",
        ),
        (
            "export function main():void{const o={'#x':1};console.log(String(o['#x']));}",
            "reserved for private names",
        ),
    ] {
        let error = rejected(source, V37);
        assert!(error.contains(expected), "{source}: {error}");
    }
}

/// V37 removes unreachable functions before the function limit applies:
/// unused prelude members and unused program functions take no slot, while
/// v36 keeps every linked function and its diagnostic.
#[test]
fn unreachable_functions_are_removed_before_the_limit_in_v37() {
    let unused: String = (0..300)
        .map(|n| format!("function unused{n}():number{{return {n};}}"))
        .collect();
    let source = format!("{unused}export function main():void{{console.log('ok');}}");
    run(&source, &["ok"]);
    let error = rejected(&source, V36);
    assert!(error.contains("too many functions"), "{error}");
    // A reachable chain above the limit still fails, naming the count.
    let reachable = varta_common::abi::MAX_USER_FUNCTIONS + 4;
    let chain: String = (0..reachable)
        .map(|n| {
            let next = if n + 1 < reachable {
                format!("f{}();", n + 1)
            } else {
                String::new()
            };
            format!("function f{n}():void{{{next}}}")
        })
        .collect();
    let error = rejected(&format!("{chain}export function main():void{{f0();}}"), V37);
    assert!(
        error.contains("too many functions across modules")
            && error.contains(&format!("{} reachable functions", reachable + 1)),
        "{error}"
    );
}
