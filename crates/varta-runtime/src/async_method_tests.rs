//! V35 `async_methods`: async instance and static methods, async arrows and
//! async function expressions. Expected logs were produced by Node 24 for the
//! same programs (each case's `main` called once) and are pinned here, so the
//! tests need no Node installation.
use super::*;
use varta_common::language::LanguageProfile;

const V35: LanguageProfile = LanguageProfile::CheckedF64Utf16V35;

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
fn failure(source: &str) -> RuntimeError {
    VartaRuntime::new()
        .run_wasm_bytes(&compile(source), &[])
        .expect_err(source)
}

#[test]
fn async_methods_overrides_super_and_statics_match_node() {
    run(
        include_str!("async_method_cases/methods.ts"),
        &[
            "request /list 0",
            "sync",
            "after api/list",
            "x:api/list",
            "sub before",
            "request /list 0",
            "after svc/list",
            "sub after",
            "svc/list!",
            "done",
        ],
    );
}

#[test]
fn async_closures_share_captured_bindings_across_await_like_node() {
    run(
        include_str!("async_method_cases/closures.ts"),
        &[
            "sync 11",
            "bumped 22 22",
            "svc/a?try=1#2",
            "12",
            "svc/r#5",
            "10",
            "caught boom",
        ],
    );
}

#[test]
fn async_method_and_closure_job_order_matches_node() {
    run(
        include_str!("async_method_cases/ordering.ts"),
        &[
            "c step a1 start",
            "loud b1",
            "l step b1 start",
            "tick t 1",
            "sync end",
            "c step a1 resumed 1",
            "l step b1 resumed 1",
            "tick t 2",
            "reaction 1",
            "c step a2 start",
            "tick t 3",
            "c step a2 resumed 2",
            "loud b2",
            "l step b2 start",
            "l step b2 resumed 2",
            "results 3 30 6 2 2",
        ],
    );
}

/// The checkpoint is driven by hand with a collection forced before every
/// job: the receiver of a suspended async method, an async closure's captured
/// cells and both frames' saved values survive while the frames are queued.
/// Drive `source` by hand with a full collection forced before every job and
/// return the job count and the log lines.
fn run_collecting(source: &str) -> (usize, Vec<String>) {
    use varta_compiler::suspension_test_hook as hook;
    let bytes = hook::compile_with_profile(source, V35).unwrap();
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
    assert!(store.data().scheduler.unhandled_rejection().is_none());
    (jobs, store.data().output.lines.clone())
}

#[test]
fn receivers_and_captures_survive_a_collection_forced_before_every_job() {
    let (jobs, lines) = run_collecting(include_str!("async_method_cases/lifetime.ts"));
    assert!(jobs > 6, "{jobs}");
    assert_eq!(
        lines,
        [
            "varta log: L0start,a=2,b=2",
            "varta log: L0start,a=2,b=2",
            "varta log: k:a|a/y k:b|b/y",
        ]
    );
    // The same program runs through the ordinary checkpoint.
    run(
        include_str!("async_method_cases/lifetime.ts"),
        &["L0start,a=2,b=2", "L0start,a=2,b=2", "k:a|a/y k:b|b/y"],
    );
}

/// V35 liveness frames (`frame_liveness`) with async closures and methods
/// (`async_methods`): an async closure body's frame keeps the captured cells
/// as shared state-0 slots, an assignment after an `await` writes through the
/// captured cell (it is live although never read), and the enclosing async
/// method's frame keeps the cells it shares. The module carries liveness
/// layouts (frame catalog version 3) and passes runtime admission; the same
/// log holds with a collection forced before every job.
#[test]
fn async_closure_captures_stay_shared_cells_in_liveness_frames() {
    let source = include_str!("async_method_cases/liveness.ts");
    let expected = [
        "assigned",
        "1 2 assigned+b",
        "assigned+b/2",
        "c3! seen=3 total=6",
        "c2! seen=2 total=9",
        "3 assigned+b+x/3 c1! seen=1 total=10",
    ];
    let bytes = compile(source);
    let catalog = wasmparser::Parser::new(0)
        .parse_all(&bytes)
        .find_map(|payload| match payload.unwrap() {
            wasmparser::Payload::CustomSection(section)
                if section.name() == varta_common::function::CATALOG_SECTION =>
            {
                Some(
                    varta_common::function::FunctionCatalog::from_liveness_bytes(section.data())
                        .unwrap(),
                )
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(
        catalog.version,
        varta_common::function::frame::FRAME_LIVENESS_CATALOG_VERSION
    );
    // Some async closure body saves captured cells as shared state-0 entries.
    assert!(
        catalog
            .frames
            .iter()
            .any(|layout| layout.slots.iter().filter(|slot| slot.shared).count() >= 2),
        "{:?}",
        catalog.frames
    );
    run(source, &expected);
    let (jobs, lines) = run_collecting(source);
    assert!(jobs > 20, "{jobs}");
    assert_eq!(
        lines,
        expected
            .iter()
            .map(|line| format!("varta log: {line}"))
            .collect::<Vec<_>>()
    );
}

/// Async methods and closures draw on the one execution budget: exhaustion is
/// fatal and never a rejection, and a rejection nobody handles terminates the
/// execution after the checkpoint as for async functions.
#[test]
fn async_closures_and_methods_keep_the_fatal_and_rejection_contracts() {
    for source in [
        "class Spin { async run(): Promise<void> { for (;;) { await null; } } }\nexport function main(): void {\n  new Spin().run().catch(() => console.log(\"never a rejection\"));\n}",
        "export function main(): void {\n  const spin = async (): Promise<void> => { for (;;) { await null; } };\n  spin().catch(() => console.log(\"never a rejection\"));\n}",
    ] {
        let error = failure(source);
        assert!(
            matches!(&error, RuntimeError::Execution(e) if format!("{e:#}").contains("fuel")),
            "{error:?}"
        );
    }
    let error = failure(
        "export function main(): void {\n  const fail = async (): Promise<number> => { await null; throw new RangeError(\"late\"); };\n  fail();\n}",
    );
    assert!(
        format!("{error:#}").contains("RangeError") && format!("{error:#}").contains("late"),
        "{error:?}"
    );
}

#[test]
fn async_method_language_guide_example_matches_node() {
    run(
        include_str!("async_method_cases/guide.ts"),
        &[
            "api/pages/1 (retries 0)",
            "attempt 1 failed",
            "api/pages/2 (retries 0) after 2 calls",
        ],
    );
}
