//! V35 `promise_values`: value16 promises inside arrays, records, unions,
//! `unknown` and dictionaries, their logging, equality and JSON, awaiting and
//! resolving unions that may hold a promise, and the `Promise.all`,
//! `allSettled`, `race` and `any` combinators. Expected logs were produced by
//! Node 24 for the same programs (each case's `main` called once) and are
//! pinned here; the one documented deviation is the notation of a string
//! inside a logged promise (`"x"`, where Node prints `'x'`).
use super::*;
use varta_common::language::LanguageProfile;

const V35: LanguageProfile = LanguageProfile::CheckedF64Utf16V35;

#[test]
fn typed_async_returns_adopt_before_widening_and_after_finalizers() {
    let source = include_str!("promise_return_cases/typed-returns.ts");
    let bytes = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V38,
    )
    .unwrap();
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    let output = VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap();
    let expected = [
        "fixture-ok",
        "7",
        "record",
        "undefined",
        "plain",
        "finally 0",
        "original",
        "finally 1",
        "9",
        "finally 2",
        "8",
        "finally 3",
        "same target",
        "reject finally",
        "typed rejection",
        "yes 2",
        "arrow",
        "order finally",
        "after call",
        "tick 1",
        "tick 2",
        "adopted",
        "tick 3",
        "tick 4",
        "unknown promise refused",
    ];
    let expected = expected.map(|line| format!("varta log: {line}"));
    assert_eq!(output.lines, expected);
    assert_eq!(
        forced_collections(source, LanguageProfile::CheckedF64Utf16V38),
        expected,
    );
}

#[test]
fn typed_async_returns_reject_incompatible_fulfillment_types() {
    let source = "async function wrong(): Promise<string> { return Promise.resolve(1); } export function main(): void {}";
    let error = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V38,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("return type mismatch"),
        "{error}"
    );
}

fn compile(source: &str) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, V35)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}
fn run(source: &str, expected: &[&str]) {
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>(),
        "{source}"
    );
}

#[test]
fn promises_live_in_arrays_records_unions_unknown_and_dictionaries_like_node() {
    run(
        include_str!("promise_value_cases/containers.ts"),
        &[
            "total 12",
            "r 10",
            "maybe 2",
            "true false object",
            "object",
            "held",
            "dict 14",
        ],
    );
}

#[test]
fn promises_log_compare_stringify_and_resolve_like_node() {
    run(
        include_str!("promise_value_cases/logging.ts"),
        &[
            "Promise { <pending> }",
            "Promise { 7 }",
            // Node prints `Promise { 'x' }`: nested strings use the notation
            // of logged aggregates.
            r#"Promise { "x" }"#,
            "Promise { undefined }",
            r#"{"name":"j","done":{}}"#,
            "[{},{}]",
            "object",
            "10",
            "same promise",
            "3",
            "held",
            "truthy",
        ],
    );
}

#[test]
fn promise_combinators_match_node_job_order_and_results() {
    run(
        include_str!("promise_value_cases/combinators.ts"),
        &[
            "settle b",
            "settle c",
            "settle a",
            "all 3 a b c",
            "fail e",
            "settle d",
            "all rejected e",
            "settle fast",
            "settle slow",
            "race fast",
            "fail x",
            "settle y",
            "any y",
            "fail p",
            "fail q",
            "any rejected All promises were rejected",
            "fail t",
            "settle s",
            r#"fulfilled {"status":"fulfilled","value":"s"}"#,
            r#"rejected {"status":"rejected","reason":{}}"#,
            "empty all 0",
            "settle t2",
            "settle t1",
            "settle t0",
            "t0t1t2",
            "end",
        ],
    );
}

/// The checkpoint is driven by hand with a collection forced before every
/// job: promises reachable only through arrays, records, unions, `unknown`
/// and dictionaries (and the combinators' private state) survive.
#[test]
fn promise_values_survive_a_collection_forced_before_every_job() {
    let source = include_str!("promise_value_cases/lifetime.ts");
    let expected = [
        "a!~~b!~c!~~~d!~f!~object",
        r#"[{"status":"fulfilled","value":"g!~~"},{"status":"rejected","reason":{}}]"#,
    ];
    assert_eq!(
        forced_collections(source, V35),
        expected.map(|line| format!("varta log: {line}"))
    );
    run(source, &expected);
}

fn forced_collections(source: &str, profile: LanguageProfile) -> Vec<String> {
    use varta_compiler::suspension_test_hook as hook;
    let bytes = hook::compile_with_profile(source, profile).unwrap();
    let runtime = crate::VartaRuntime::new();
    let module = runtime.prepare_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    let start = instance.get_func(&mut *store, "_start").unwrap();
    start.call(&mut *store, &[], &mut []).unwrap();
    let collect = instance
        .get_typed_func::<(), ()>(&mut *store, hook::COLLECT)
        .unwrap();
    let resume = instance
        .get_typed_func::<(i64, i32, i64), i32>(
            &mut *store,
            varta_common::function::frame::RESUME_EXPORT,
        )
        .unwrap();
    let mut jobs = 0;
    while let Some(token) = store.data_mut().scheduler.next().unwrap() {
        // Test-only budget: each forced full collection is paid for outside
        // the execution's production fuel, which the jobs alone consume.
        let remaining = store.get_fuel().unwrap();
        store.set_fuel(abi::FUEL).unwrap();
        collect.call(&mut *store, ()).unwrap();
        store.set_fuel(remaining).unwrap();
        resume.call(&mut *store, (token, 0, 0)).unwrap();
        jobs += 1;
    }
    assert!(jobs > 10, "{jobs}");
    assert!(store.data().scheduler.unhandled_rejection().is_none());
    store.data().output.lines.clone()
}

/// A promise held by `unknown` has no static `T`: awaiting or resolving with
/// it throws a recoverable `TypeError` instead of silently not adopting it,
/// and a combinator's unhandled rejection terminates after the checkpoint.
#[test]
fn unknown_promises_are_never_adopted_silently_and_rejections_stay_reported() {
    run(
        "export async function main(): Promise<void> {\n  const u: unknown = Promise.resolve(1);\n  try {\n    await u;\n  } catch (error) {\n    if (error instanceof TypeError) {\n      console.log(error.message);\n    }\n  }\n  const v: unknown = 2;\n  const w = await v;\n  console.log(String(w));\n}",
        &[
            "a promise held by unknown cannot be adopted; keep it typed as Promise<T>",
            "2",
        ],
    );
    let error = VartaRuntime::new()
        .run_wasm_bytes(
            &compile(
                "export function main(): void {\n  Promise.all([Promise.resolve(1), Promise.reject<number>(new RangeError(\"lost\"))]);\n}",
            ),
            &[],
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("lost"), "{error:?}");
}

#[test]
fn promise_value_language_guide_example_matches_node() {
    run(
        include_str!("promise_value_cases/guide.ts"),
        &[
            "Promise { <pending> }",
            "100 200",
            "500",
            r#"[{"status":"fulfilled","value":200},{"status":"rejected","reason":{}}]"#,
            "100",
        ],
    );
}
