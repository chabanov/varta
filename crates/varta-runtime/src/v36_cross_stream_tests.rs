//! V36 cross-stream integration: the declared-fields, function-fields,
//! timers and async-generator streams' programs share value layout 17 (value17
//! readers, JSON schema 17, exception channel v8) and host ABI 32. Expected
//! logs were produced by Node 24 (v24.20.0, type stripping) for the same
//! programs with `main()` appended and are pinned here, so the tests need no
//! Node installation; timer programs run on the test-only virtual clock with
//! an exact `time.timer` grant.
use super::*;
use varta_common::scheduler::TIMER_CAPABILITY;
use varta_common::{HostOperation, language::LanguageProfile as P};

const V36: P = P::CheckedF64Utf16V36;

fn compile(source: &str) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, V36)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
fn rejected(source: &str) -> String {
    varta_compiler::compile_typescript_with_profile(source, V36)
        .err()
        .unwrap_or_else(|| panic!("expected a diagnostic\n{source}"))
        .to_string()
}
fn imports(bytes: &[u8]) -> Vec<HostOperation> {
    let module = wasmtime::Module::new(&VartaRuntime::new().engine, bytes).unwrap();
    module
        .imports()
        .map(|i| HostOperation::from_name(i.name()).unwrap())
        .collect()
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
    let bytes = compile(source);
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
    let (log, result) = super::timer_tests::observe(source, &timer_grant(), true);
    result.unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    assert_eq!(log, lines(expected), "{source}");
}

/// The programs of every v36 stream and of this module.
const CASES: [&str; 25] = [
    include_str!("function_field_cases/chains.ts"),
    include_str!("function_field_cases/fields.ts"),
    include_str!("function_field_cases/guide.ts"),
    include_str!("function_field_cases/lifetime.ts"),
    include_str!("function_field_cases/listeners.ts"),
    include_str!("function_field_cases/parameters.ts"),
    include_str!("v36_cases/conversions.ts"),
    include_str!("v36_cases/declared_fields.ts"),
    include_str!("v36_cases/problem_errors.ts"),
    include_str!("timer_cases/abort_timeout.ts"),
    include_str!("timer_cases/lifetime.ts"),
    include_str!("timer_cases/ordering.ts"),
    include_str!("timer_cases/sleep.ts"),
    include_str!("async_generator_cases/delegation.ts"),
    include_str!("async_generator_cases/features.ts"),
    include_str!("async_generator_cases/for_await.ts"),
    include_str!("async_generator_cases/guide.ts"),
    include_str!("async_generator_cases/lifetime.ts"),
    include_str!("async_generator_cases/protocol.ts"),
    include_str!("async_generator_cases/reentrancy.ts"),
    include_str!("v36_cross_cases/abort_composition.ts"),
    include_str!("v36_cross_cases/backoff_stream.ts"),
    include_str!("v36_cross_cases/declared_functions.ts"),
    include_str!("v36_cross_cases/guide.ts"),
    include_str!("v36_cross_cases/timer_fields.ts"),
];

/// Every stream's program validates under v36, so its record descriptors
/// (including the async generators' state and request records with promise
/// fields), schemas, capture and frame catalogs carry the value17 stamps, and
/// it imports only operations v36 binds: value17 readers rather than value16
/// ones, all within host ABI 32.
#[test]
fn v36_stream_programs_validate_and_import_only_value17_readers() {
    let runtime = VartaRuntime::new();
    let mut readers = std::collections::BTreeSet::new();
    for source in CASES {
        let bytes = compile(source);
        runtime
            .validate_module(&bytes)
            .unwrap_or_else(|e| panic!("{e}\n{source}"));
        for operation in imports(&bytes) {
            assert_eq!(V36.host_operation(operation), operation, "{operation:?}");
            assert!(operation.value15_counterpart().is_none(), "{operation:?}");
            assert!(operation.minimum_abi() <= V36.host_abi(), "{operation:?}");
            if operation.value16_counterpart().is_some() {
                readers.insert(operation);
            }
        }
    }
    // The cases exercise the logging, typeof, truthiness, equality, JSON and
    // String conversion readers of value17.
    for operation in [
        HostOperation::ValueLogV11,
        HostOperation::ValueTypeOfV10,
        HostOperation::JsonStringifyV16,
        HostOperation::JsonParseV18,
        HostOperation::PrimitiveStringV5,
    ] {
        assert!(readers.contains(&operation), "{operation:?}");
    }
}

const ABORT_COMPOSITION: [&str; 7] = [
    "[\"before false\",\"a first\",\"a second true\",\"any stop true\",\"nested onabort\",\"nested stop\",\"after stop stop\",\"pre-aborted true later\",\"empty false\"]",
    "slept 10 false",
    "deadline TimeoutError",
    "sleep rejected TimeoutError",
    "user abort ignored TimeoutError",
    "second rejected cancelled cancelled",
    "already aborted cancelled",
];

/// `AbortSignal.any` (v36 abort prelude, with listeners) composes controller
/// and `AbortSignal.timeout` signals in the DOM order Node follows: dependent
/// signals take the reason before the source's listeners run and dispatch
/// after them. The pilot's abortable sleep removes its listener when the
/// timer fires and clears the timer when the signal aborts.
#[test]
fn abort_any_timeout_and_listeners_compose_like_node() {
    run_collecting(
        include_str!("v36_cross_cases/abort_composition.ts"),
        &ABORT_COMPOSITION,
    );
}

/// The language guide's abortable sleep with a deadline.
#[test]
fn language_guide_abort_deadline_example_matches_node() {
    run(include_str!("v36_cross_cases/guide.ts"), &["TimeoutError"]);
}

/// Timer callbacks read from record, class, array and dictionary fields, and
/// closures reachable only through fields of a scheduler that only the
/// pending interval's frame holds, survive a collection forced before every
/// job.
#[test]
fn timer_callbacks_held_in_function_fields_stay_rooted() {
    run_collecting(
        include_str!("v36_cross_cases/timer_fields.ts"),
        &[
            "[\"element 0\",\"element 1\",\"job-0:x@1\",\"job-1:xx@1\",\"job-2:xxx@1\",\"hook tick\",\"holder later\",\"job-0:x@2\",\"job-1:xx@2\",\"job-2:xxx@2\",\"hook tick\",\"done 3\"]",
        ],
    );
}

const BACKOFF_STREAM: [&str; 16] = [
    "event a after 0 ticks",
    "retry 1 in 15",
    "retry 2 in 30",
    "event b after 0 ticks",
    "retry 1 in 15",
    "retry 2 in 30",
    "tick 1",
    "retry 3 in 60",
    "tick 2",
    "event c after 2 ticks",
    "manual m",
    "retry 1 in 15",
    "manual n",
    "returned 2",
    "first x",
    "closed true undefined",
];

/// The pilot's SSE reconnect shape: a `for await` over an async generator
/// that sleeps on `setTimeout` between attempts with the defaulted
/// `streamBackoffDelay`, interleaved with an interval on the virtual clock,
/// and a `break` that closes a generator suspended at `yield`.
#[test]
fn for_await_over_a_sleeping_async_generator_matches_node() {
    run_collecting(
        include_str!("v36_cross_cases/backoff_stream.ts"),
        &BACKOFF_STREAM,
    );
}

/// Async generators yield function values and records with function fields
/// under value17; the yielded closures stay rooted in the request queue and
/// result records across forced collections.
#[test]
fn async_generators_yield_function_values() {
    let source = "interface Step { name: string; apply: (x: number) => number }\n\
        async function* steps(count: number): AsyncGenerator<Step, string, undefined> {\n\
          for (let i = 1; i <= count; i++) {\n\
            const factor = i * 10;\n\
            yield { name: 'step-' + String(i), apply: (x: number): number => x * factor };\n\
          }\n\
          return 'done ' + String(count);\n\
        }\n\
        export async function main(): Promise<void> {\n\
          let value = 1;\n\
          for await (const step of steps(3)) {\n\
            value = step.apply(value);\n\
            console.log(step.name + ' ' + String(value) + ' ' + typeof step.apply);\n\
          }\n\
          const generator = steps(1);\n\
          await generator.next();\n\
          const last = await generator.next();\n\
          if (last.done === true) console.log(last.value);\n\
        }";
    run_collecting(
        source,
        &[
            "step-1 10 function",
            "step-2 200 function",
            "step-3 6000 function",
            "done 1",
        ],
    );
}

/// Declared reads of mixed records combine with optional chains, the
/// `unknown` index holds function values (`typeof` reads their tag, JSON
/// drops them), and a checked assertion of an `unknown` cell to a record type
/// with a function field passes only for that record's descriptor: a record
/// of another shape, a function and a string all throw `TypeError` (Node
/// throws when the missing method is called).
#[test]
fn declared_fields_combine_with_function_values_and_optional_chains() {
    run(
        include_str!("v36_cross_cases/declared_functions.ts"),
        &[
            "name 2 5 3",
            "function number",
            "function cell true",
            "{\"title\":\"bad\",\"errors\":[{\"field\":\"name\"},{\"message\":\"m\"}],\"meta\":{\"retry\":{\"after\":5}},\"trace\":1}",
            "name 2 undefined 3",
            "none 0 undefined undefined",
            "object double=40",
            "object rejected TypeError",
            "function rejected TypeError",
            "string rejected TypeError",
        ],
    );
}

/// Checked assertions from `unknown` never claim a function signature: a
/// function target, a union or array with a function member, and a declared
/// function field of a mixed record indexed by `unknown` stay rejected with a
/// diagnostic (TypeScript erases the signature, so no tag test can prove it).
#[test]
fn checked_assertions_to_function_types_stay_rejected() {
    for (source, expected) in [
        (
            "export function main(): void { const u: unknown = (): void => {}; const f = u as () => void; f(); }",
            "type assertion",
        ),
        (
            "export function main(): void { const u: unknown = 'a'; const f = u as (() => void) | string; console.log(typeof f); }",
            "type assertion",
        ),
        (
            "export function main(): void { const u: unknown = JSON.parse('[]'); const f = u as Array<() => void>; console.log(String(f.length)); }",
            "type assertion",
        ),
        (
            "interface M { handler?: () => void; [key: string]: unknown }\n\
             export function main(): void { const u: unknown = JSON.parse('{}'); const m = u as Record<string, unknown> as M; m.handler?.(); }",
            "handler",
        ),
    ] {
        let error = rejected(source);
        assert!(error.contains(expected), "{source}\n{error}");
        assert!(error.contains("unsupported"), "{source}\n{error}");
    }
}

/// A v36 module that uses every stream (declared fields, function fields,
/// timers and async generators) and is relabelled as v35 fails validation
/// before any guest effect, with or without its host ABI section rewritten.
#[test]
fn v36_modules_relabelled_as_v35_fail_before_guest_effects() {
    let bytes = compile(include_str!("v36_cross_cases/backoff_stream.ts"));
    let mut section = abi::SECTION.as_bytes().to_vec();
    section.extend(abi::TIMERS_VERSION.to_le_bytes());
    let mut older = abi::SECTION.as_bytes().to_vec();
    older.extend(abi::PROMISE_VALUES_VERSION.to_le_bytes());
    let renamed =
        super::timer_tests::patched(&bytes, b"checked-f64-utf16-v36", b"checked-f64-utf16-v35");
    let downgraded = super::timer_tests::patched(&renamed, &section, &older);
    let runtime = VartaRuntime::new();
    for module in [&renamed, &downgraded] {
        let refused = runtime.validate_module(module).unwrap_err().to_string();
        // Running fails with the same validation refusal, before the module is
        // instantiated, so no guest code (and no timer) ever runs.
        let error =
            scheduler::with_virtual_clock(|| runtime.run_wasm_bytes(module, &timer_grant()))
                .unwrap_err();
        assert_eq!(error.to_string(), refused);
    }
    // The unmodified module runs.
    run(
        include_str!("v36_cross_cases/backoff_stream.ts"),
        &BACKOFF_STREAM,
    );
}
