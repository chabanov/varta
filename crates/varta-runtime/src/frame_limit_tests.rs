//! Batch v38 `data_limits`, frames: more than 64 live continuation frames.
//! A v38 module keeps its live frames in a growable hashed table on the
//! managed heap and the host scheduler bounds pending tokens by the memory
//! budget; v37 keeps its fixed 64-slot table and fails at the 65th live frame
//! exactly as before. Expected logs follow Node 24 (deterministic sums of the
//! awaited values); the tests need no Node installation.
use super::*;
use varta_common::language::LanguageProfile as P;
use varta_common::limits::Resource;

const V37: P = P::CheckedF64Utf16V37;
const V38: P = P::CheckedF64Utf16V38;

fn compile(source: &str, profile: P) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    // Emitted modules must also pass backend validation, not only compilation.
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}

/// What one run consumed: fuel, host calls and the final memory pages.
#[derive(Debug)]
struct Cost {
    fuel: u64,
    host_calls: u32,
    pages: u64,
}

/// Run under an explicit test-only fuel budget of `fuel` production budgets
/// (CLAUDE.md §11); memory, host-call and output budgets stay production.
fn run_budgeted(source: &str, profile: P, fuel: u64) -> (Result<Vec<String>, RuntimeError>, Cost) {
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&compile(source, profile)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.allow_test_wall_clock(std::time::Duration::from_secs(120));
    execution.store.set_fuel(fuel * abi::FUEL).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    let result = (|| {
        let start = instance.get_func(&mut *store, "_start").unwrap();
        start.call(&mut *store, &[], &mut [])?;
        exceptions::finish(&instance, store)?;
        scheduler::checkpoint(&instance, store)?;
        Ok::<(), RuntimeError>(())
    })()
    .map_err(|error| resources::classify(&instance, store, scheduler::typed(error)));
    let pages = instance
        .get_memory(&mut *store, "memory")
        .map_or(0, |memory| memory.size(&*store));
    let cost = Cost {
        fuel: fuel * abi::FUEL - store.get_fuel().unwrap(),
        host_calls: store.data().host_calls,
        pages,
    };
    (
        result.map(|()| std::mem::take(&mut store.data_mut().output.lines)),
        cost,
    )
}

/// `count` async calls suspended at once, then awaited in order.
fn concurrent(count: usize) -> String {
    format!(
        "async function work(i: number): Promise<number> {{\n  await null;\n  return i * 2;\n}}\n\
         export async function main(): Promise<void> {{\n  const pending: Promise<number>[] = [];\n  \
         for (let i = 0; i < {count}; i++) {{\n    pending.push(work(i));\n  }}\n  let total = 0;\n  \
         for (const p of pending) {{\n    total = total + (await p);\n  }}\n  console.log(String(total));\n}}"
    )
}
fn sum_of_doubles(count: usize) -> String {
    (count * (count - 1)).to_string()
}

#[test]
fn v38_runs_1000_concurrent_awaits_beyond_the_64_frame_table() {
    for count in [65, 100, 1000] {
        let (result, cost) = run_budgeted(&concurrent(count), V38, 100);
        assert_eq!(
            result.unwrap_or_else(|e| panic!("{count}: {e:?}")),
            [format!("varta log: {}", sum_of_doubles(count))],
            "{count}"
        );
        println!("concurrent awaits {count}: {cost:?}");
        // Scheduler operations are not host calls; memory stays in budget.
        assert!(
            cost.host_calls < abi::MAX_HOST_CALLS && cost.fuel > 0,
            "{cost:?}"
        );
        assert!(cost.pages <= (abi::MAX_MEMORY_BYTES / abi::WASM_PAGE_BYTES) as u64);
    }
    // Within the production fuel budget as well.
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compile(&concurrent(100), V38), &[])
        .unwrap();
    assert_eq!(
        output.lines,
        [format!("varta log: {}", sum_of_doubles(100))]
    );
}

#[test]
fn v37_still_fails_at_the_65th_live_frame() {
    let (result, _) = run_budgeted(&concurrent(64), V37, 100);
    // Main's own frame is live too, so 64 suspended calls already fail.
    let error = result.unwrap_err();
    assert!(
        matches!(&error, RuntimeError::Execution(e) if format!("{e:#}").contains("unreachable")),
        "{error:?}"
    );
    let (result, _) = run_budgeted(&concurrent(63), V37, 100);
    assert_eq!(
        result.unwrap(),
        [format!("varta log: {}", sum_of_doubles(63))]
    );
}

/// Pending reactions of a promise that never settles: v37 fails beyond its
/// table, v38 keeps 1000 of them live and finishes with them pending.
#[test]
fn v38_keeps_1000_pending_reactions_live() {
    let source = "export function main(): void {\n  const never = new Promise<number>(() => {});\n  \
                  for (let i = 0; i < 1000; i++) {\n    never.then((v) => v + i);\n  }\n  console.log('registered');\n}";
    let (result, cost) = run_budgeted(source, V38, 100);
    assert_eq!(result.unwrap(), ["varta log: registered"]);
    println!("pending reactions 1000: {cost:?}");
    let (result, _) = run_budgeted(source, V37, 100);
    assert!(result.is_err());
}

/// Live frames holding strings survive collections forced by garbage
/// allocated while they are suspended (exact tracing through the table).
#[test]
fn v38_live_frames_survive_forced_collections() {
    let source = "async function hold(i: number): Promise<number> {\n  const text = 'x'.repeat(64 + (i % 7));\n  \
                  await null;\n  return text.length;\n}\n\
                  export async function main(): Promise<void> {\n  const pending: Promise<number>[] = [];\n  \
                  for (let i = 0; i < 1000; i++) {\n    pending.push(hold(i));\n    let garbage = '';\n    \
                  for (let j = 0; j < 4; j++) {\n      garbage = garbage + 'y'.repeat(256 + j);\n    }\n  }\n  \
                  let total = 0;\n  for (const p of pending) {\n    total = total + (await p);\n  }\n  console.log(String(total));\n}";
    // Lengths 64 + i % 7 for i below 1000.
    let expected: usize = (0..1000).map(|i| 64 + i % 7).sum();
    let (result, cost) = run_budgeted(source, V38, 4000);
    assert_eq!(result.unwrap(), [format!("varta log: {expected}")]);
    println!("forced collections 1000: {cost:?}");
}

/// Live frames until the 8 MiB memory budget is exhausted: a typed memory
/// resource failure, never an anonymous trap or a host-side bound.
/// 1000 live async frames, each suspended inside a `for...of` over its own
/// suspended generator (generator frames belong to their generator objects
/// and never occupy the live table; the async frames do).
#[test]
fn v38_keeps_1000_suspended_generators() {
    let source = "function* counter(n: number): Generator<number> {\n  yield n;\n  yield n + 1;\n}\n\
                  async function walk(n: number): Promise<number> {\n  let total = 0;\n  \
                  for (const v of counter(n)) {\n    await null;\n    total = total + v;\n  }\n  return total;\n}\n\
                  export async function main(): Promise<void> {\n  const pending: Promise<number>[] = [];\n  \
                  for (let i = 0; i < 1000; i++) {\n    pending.push(walk(i));\n  }\n  \
                  let total = 0;\n  for (const p of pending) {\n    total = total + (await p);\n  }\n  \
                  console.log(String(total));\n}";
    let (result, cost) = run_budgeted(source, V38, 400);
    assert_eq!(result.unwrap(), ["varta log: 1000000"]);
    println!("suspended generators 1000: {cost:?}");
}

#[test]
fn v38_live_frames_exhaust_the_memory_budget_with_a_typed_error() {
    // Each suspended frame keeps an 8 KiB string alive, so about a thousand
    // live frames fill the budget.
    let source = "async function hold(p: Promise<number>): Promise<number> {\n  \
                  const text = 'x'.repeat(4096);\n  const v = await p;\n  return text.length + v;\n}\n\
                  export function main(): void {\n  const never = new Promise<number>(() => {});\n  \
                  for (let i = 0; i < 100000; i++) {\n    hold(never);\n  }\n}";
    let (result, cost) = run_budgeted(source, V38, 1000);
    println!("frames until the memory budget: {cost:?}");
    match result {
        Err(RuntimeError::Resource(error)) => assert_eq!(error.resource, Resource::Memory),
        other => panic!("expected a memory resource failure, got {other:?}"),
    }
}
