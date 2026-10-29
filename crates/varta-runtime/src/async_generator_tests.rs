//! V36 `async_generators` (K4.4 step 6): async generator functions and
//! methods, their queued `next`/`return`/`throw` protocol and `for await`.
//! Expected logs were produced by Node 24 for the same programs (each case's
//! `main` called once) and are pinned here, so the tests need no Node
//! installation. Fuel figures are guest fuel under the production budgets.
use super::*;
use varta_common::language::LanguageProfile;

const V36: LanguageProfile = LanguageProfile::CheckedF64Utf16V36;

fn compile(source: &str) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, V36)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}
fn lines(expected: &[&str]) -> Vec<String> {
    expected.iter().map(|s| format!("varta log: {s}")).collect()
}
fn run(source: &str, expected: &[&str]) {
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        lines(expected),
        "{source}"
    );
}
fn failure(source: &str) -> RuntimeError {
    VartaRuntime::new()
        .run_wasm_bytes(&compile(source), &[])
        .expect_err(source)
}
/// Guest fuel of `_start` and the whole microtask checkpoint, or `None` when
/// the execution fails within the production budgets.
fn fuel(source: &str) -> Option<u64> {
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&compile(source)).ok()?;
    let mut execution = execution::Execution::new(&runtime.engine, &[], None).ok()?;
    let instance = execution.instantiate_prepared(&module).ok()?;
    let store = &mut execution.store;
    let start = instance.get_func(&mut *store, "_start")?;
    start.call(&mut *store, &[], &mut []).ok()?;
    exceptions::finish(&instance, store).ok()?;
    scheduler::checkpoint(&instance, store).ok()?;
    Some(abi::FUEL - store.get_fuel().ok()?)
}
/// Marginal fuel of one more unit of work: `(fuel(2n) - fuel(n)) / n`.
fn marginal(program: impl Fn(usize) -> String) -> u64 {
    const BASE: usize = 10;
    let fuel = |count| fuel(&program(count)).expect("within budget");
    (fuel(2 * BASE) - fuel(BASE)) / BASE as u64
}
/// Drive `source` by hand with a full collection forced before every job,
/// under the test-only fuel budget `budget` (the collections themselves are
/// paid outside it); return the job count and the log.
fn run_collecting(source: &str, budget: u64) -> (usize, Vec<String>) {
    use varta_compiler::suspension_test_hook as hook;
    let bytes = hook::compile_with_profile(source, V36).unwrap();
    let runtime = crate::VartaRuntime::new();
    let module = runtime.prepare_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    store.set_fuel(budget).unwrap();
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
        let remaining = store.get_fuel().unwrap();
        store.set_fuel(abi::FUEL).unwrap();
        collect.call(&mut *store, ()).unwrap();
        store.set_fuel(remaining).unwrap();
        resume.call(&mut *store, (token, 0, 0)).unwrap();
        jobs += 1;
    }
    assert!(store.data().scheduler.unhandled_rejection().is_none());
    (jobs, store.data().output.lines.clone())
}

const PROTOCOL: &[&str] = &[
    "q start",
    "sync end",
    "aw 0",
    "A tick 1",
    "q got y",
    "B tick 1",
    "C tick 1",
    "thr t",
    "n {\"done\":true}",
    "D tick 1",
    "E tick 1",
    "aw 1 5",
    "A tick 2",
    "r0 {\"value\":1,\"done\":false}",
    "q got z",
    "B tick 2",
    "n1 {\"value\":1,\"done\":false}",
    "r finally",
    "C tick 2",
    "ret {\"value\":3,\"done\":true}",
    "n {\"done\":true}",
    "D tick 2",
    "b1 {\"value\":1,\"done\":false}",
    "b2 err bad",
    "b3 {\"done\":true}",
    "E tick 2",
    "A tick 3",
    "r1 {\"value\":2,\"done\":false}",
    "B tick 3",
    "r after 5",
    "C tick 3",
    "D tick 3",
    "E tick 3",
    "a1 {\"value\":5,\"done\":false}",
    "A tick 4",
    "r2 {\"value\":9,\"done\":true}",
    "r3 {\"done\":true}",
    "B tick 4",
    "ret {\"value\":5,\"done\":false}",
    "n2 {\"value\":7,\"done\":true}",
    "n3 {\"done\":true}",
    "C tick 4",
    "E tick 4",
    "A tick 5",
    "B tick 5",
    "E tick 5",
    "a2 {\"value\":6,\"done\":true}",
    "a3 {\"done\":true}",
    "E tick 6",
];
const FOR_AWAIT: &[&str] = &[
    "g start",
    "A tick 1",
    "B tick 1",
    "C tick 1",
    "A tick 2",
    "arr 1",
    "B tick 2",
    "sg 1",
    "C tick 2",
    "ag 1",
    "A tick 3",
    "B tick 3",
    "sg finally",
    "C tick 3",
    "g finally",
    "A tick 4",
    "arr 2",
    "B tick 4",
    "sg caught boom",
    "C tick 4",
    "ag after",
    "g start",
    "A tick 5",
    "B tick 5",
    "C tick 5",
    "A tick 6",
    "arr done",
    "B tick 6",
    "sg2 1",
    "sg finally",
    "C tick 6",
    "ag2 1",
    "A tick 7",
    "B tick 7",
    "C tick 7",
    "g finally",
    "A tick 8",
    "ps 3",
    "B tick 8",
    "sg2 after",
    "C tick 8",
    "ag2 caught body",
    "g start",
    "ps after",
    "C tick 9",
    "C tick 10",
    "g finally",
    "total 3",
];
const LIFETIME: &[&str] = &[
    "a a0-2!",
    "b b0-2!",
    "q1 {\"value\":\"q0-2!\",\"done\":false}",
    "q2 {\"value\":\"q1-6!\",\"done\":false}",
    "a a1-6!",
    "b b1-6!",
    "q3 {\"value\":\"q:q0-2q1-6\",\"done\":true}",
    "a a2-10!",
    "b b2-10!",
    "a returned a:a0-2a1-6a2-10",
    "b returned b:b0-2b1-6b2-10",
    "chain 63 true",
];

/// The request queue: requests made while the body runs or awaits wait in
/// order; `return()` and `throw()` on not-started, suspended and completed
/// generators; `return` and `yield` await their operands; `yield` inside
/// `finally` during a `return()`. Interleaved promise chains pin job order.
#[test]
fn async_generator_request_queue_matches_node() {
    run(include_str!("async_generator_cases/protocol.ts"), PROTOCOL);
}

/// `for await` over arrays, synchronous generators and async generators:
/// two jobs per async-from-sync step, one or two to close, closing on
/// `break` and on a throw from the body, and a rejected value closing the
/// synchronous generator.
#[test]
fn for_await_sources_and_closing_match_node() {
    run(
        include_str!("async_generator_cases/for_await.ts"),
        FOR_AWAIT,
    );
}

/// Generic async generators through `AsyncIterableIterator<T>` and
/// `AsyncIterable<T>` parameters, instance and static async generator
/// methods with optional and default parameters, closures over body locals,
/// `switch` and labelled `continue` around `yield`.
#[test]
fn async_generator_methods_generics_and_closures_match_node() {
    run(
        include_str!("async_generator_cases/features.ts"),
        &[
            "e0 {\"value\":0,\"done\":false}",
            "e1 {\"value\":5,\"done\":false}",
            "e2 {\"value\":12,\"done\":false}",
            "e3 {\"value\":\"stop 12\",\"done\":true}",
            "e4 {\"done\":true}",
            "numbers [1,2,3,4,5]",
            "names [\"a\",\"b\"]",
            "> one",
            "> two",
            "> three",
            "> a",
            "> b",
            "range 10",
            "range 20",
            "range 30",
        ],
    );
}

/// The conformance inventory's case (known unsupported under its v35 pin):
/// requests queued before the first settles, `yield await`, a `TReturn`
/// other than `undefined` closed by `for await` through `finally`.
#[test]
fn async_generator_conformance_case_matches_node() {
    run(
        include_str!("../../../conformance/cases/async-generator-iteration.ts"),
        &[
            "countdown closed",
            r#"[{"value":2,"done":false},{"value":1,"done":false},{"value":"liftoff","done":true}]"#,
            "countdown closed",
            "9",
        ],
    );
}

/// The language guide's example.
#[test]
fn async_generator_language_guide_example_matches_node() {
    run(
        include_str!("async_generator_cases/guide.ts"),
        &[
            "1. a",
            "2. b",
            "3. c",
            "numbered 3",
            r#"{"value":"1. a","done":false}"#,
            "numbered 1",
            r#"{"done":true}"#,
            "1",
            "2",
        ],
    );
}

/// A request from inside the running body (through a callback) is queued,
/// and `throw()` reaches a `catch` around the suspended `yield`.
#[test]
fn reentrant_requests_and_injected_throws_match_node() {
    run(
        include_str!("async_generator_cases/reentrancy.ts"),
        &[
            "body start",
            "body after hook",
            "body after 1",
            "caught injected",
            "outer {\"value\":1,\"done\":false}",
            "body end",
            "c1 {\"value\":\"first\",\"done\":false}",
            "inner {\"value\":2,\"done\":false}",
            "third {\"done\":true}",
            "c2 {\"value\":\"recovered\",\"done\":false}",
            "c3 {\"value\":\"end\",\"done\":true}",
            "c4 rejected RangeError late",
        ],
    );
}

/// Strings saved across `await` and `yield`, queued requests with their
/// promises and generators suspended at a `yield` inside other generators'
/// frames survive a collection forced before every job.
#[test]
fn async_generator_values_survive_a_collection_forced_before_every_job() {
    for (source, expected) in [
        (include_str!("async_generator_cases/lifetime.ts"), LIFETIME),
        (
            include_str!("async_generator_cases/for_await.ts"),
            FOR_AWAIT,
        ),
        (include_str!("async_generator_cases/protocol.ts"), PROTOCOL),
    ] {
        run(source, expected);
        let (jobs, log) = run_collecting(source, abi::FUEL);
        assert!(jobs > 20, "{jobs}");
        assert_eq!(log, lines(expected));
    }
}

/// Async generators draw on the one execution budget: exhaustion inside a
/// body or its relay jobs is fatal and never a rejection, and a request
/// promise rejected without a handler terminates the execution after the
/// checkpoint, as Node does.
#[test]
fn async_generators_keep_the_fatal_and_rejection_contracts() {
    let error = failure(
        "async function* spin(): AsyncGenerator<number, void, undefined> { for (;;) { await null; } }\nexport function main(): void {\n  spin().next().catch(() => console.log(\"never a rejection\"));\n}",
    );
    assert!(
        matches!(&error, RuntimeError::Execution(e) if format!("{e:#}").contains("fuel")),
        "{error:?}"
    );
    let error = failure(
        "async function* fail(): AsyncGenerator<number, void, undefined> { yield 1; throw new RangeError(\"late\"); }\nexport function main(): void {\n  const it = fail();\n  it.next();\n  it.next();\n}",
    );
    assert!(
        format!("{error:#}").contains("RangeError") && format!("{error:#}").contains("late"),
        "{error:?}"
    );
}

/// `count` values yielded by an async generator and consumed by `for await`.
fn yields(count: usize) -> String {
    format!(
        r#"async function* count(n: number): AsyncGenerator<number, void, undefined> {{
  for (let i = 0; i < n; i++) {{
    yield i;
  }}
}}
export async function main(): Promise<void> {{
  let total = 0;
  for await (const value of count({count})) {{
    total = total + value;
  }}
  console.log(String(total));
}}"#
    )
}
/// `count` awaits inside one async generator body that never yields.
fn awaits(count: usize) -> String {
    format!(
        r#"async function* wait(n: number): AsyncGenerator<number, void, undefined> {{
  for (let i = 0; i < n; i++) {{
    await i;
  }}
}}
export async function main(): Promise<void> {{
  for await (const value of wait({count})) {{
    console.log(String(value));
  }}
}}"#
    )
}
/// The same awaits in an async function, for comparison.
fn function_awaits(count: usize) -> String {
    format!(
        r#"async function wait(n: number): Promise<void> {{
  for (let i = 0; i < n; i++) {{
    await i;
  }}
}}
export async function main(): Promise<void> {{
  await wait({count});
}}"#
    )
}
/// `count` async generators created, started to their first `yield` and
/// abandoned.
fn abandoned(count: usize) -> String {
    format!(
        r#"async function* hold(label: string): AsyncGenerator<string, void, undefined> {{
  const head = label + "-a";
  yield head;
  yield head + "-b";
}}
export async function main(): Promise<void> {{
  let total = 0;
  for (let i = 0; i < {count}; i++) {{
    const first = await hold("g" + String(i)).next();
    const value = first.value;
    if (typeof value === "string") total = total + value.length;
  }}
  console.log(String(total));
}}"#
    )
}

/// The figures recorded in architecture.md (async generators) come from this
/// test; run it with `--nocapture` to print them.
#[test]
fn async_generator_suspensions_have_bounded_measured_costs() {
    let step = marginal(yields);
    let wait = marginal(awaits);
    let plain = marginal(function_awaits);
    let created = marginal(abandoned);
    println!(
        "fuel per for-await step {step}, per await in an async generator {wait} \
         (async function {plain}), per started and abandoned generator {created}"
    );
    // An await in a body costs one relay frame on top of an async function's.
    assert!(plain < wait && wait < 3 * plain, "{plain} {wait}");
    assert!(step < 4 * wait && created < 4 * wait, "{step} {created}");
}

/// Abandoned async generators hold no live frame and are collected: far more
/// of them than the live-frame table holds run in one execution, with a
/// collection forced before every job (test-only fuel budget).
#[test]
fn abandoned_async_generators_are_collected_beyond_the_live_frame_table() {
    let count = 4 * varta_common::function::frame::MAX_LIVE_FRAMES;
    let (jobs, lines) = run_collecting(&abandoned(count), 8 * abi::FUEL);
    assert!(jobs >= 2 * count, "{jobs}");
    let total: usize = (0..count).map(|i| format!("g{i}-a").len()).sum();
    assert_eq!(lines, [format!("varta log: {total}")]);
}
