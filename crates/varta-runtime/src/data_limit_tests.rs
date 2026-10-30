//! Batch v38 `data_limits`: arrays, dictionaries, collections, strings and JSON
//! bounded by the memory budget instead of fixed per-value ceilings, more than
//! 64 live continuation frames, and typed resource failures. Earlier profiles
//! keep their bytes and ceilings, pinned by the v37 digests below. Expected
//! logs come from Node 24 (type stripping, `main()` appended) and are pinned
//! here, so the tests need no Node installation.
use super::*;
use varta_common::language::LanguageProfile as P;
use varta_common::limits::{Resource, ResourceExhausted};

const V37: P = P::CheckedF64Utf16V37;
const V38: P = P::CheckedF64Utf16V38;

fn compile(source: &str, profile: P) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    // Emitted modules must also pass backend validation, not only compilation.
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
fn lines(expected: &[&str]) -> Vec<String> {
    expected.iter().map(|s| format!("varta log: {s}")).collect()
}
fn run(source: &str, profile: P) -> Result<Vec<String>, RuntimeError> {
    VartaRuntime::new()
        .run_wasm_bytes(&compile(source, profile), &[])
        .map(|output| output.lines)
}

/// FNV-1a over a module's bytes: v37 output is frozen by the digests below,
/// recorded before the data-limits stream changed any emission.
fn digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}
const FROZEN_V37_SOURCES: [&str; 14] = [
    include_str!("collection_cases/collections.ts"),
    include_str!("collection_cases/binary_text.ts"),
    include_str!("collection_cases/strings.ts"),
    include_str!("collection_cases/live_iteration.ts"),
    include_str!("collection_cases/tuples.ts"),
    include_str!("v37_integration_cases/union_collections.ts"),
    include_str!("v37_integration_cases/private_json.ts"),
    include_str!("v37_integration_cases/async_iterable.ts"),
    include_str!("value15_cases/dictionary.ts"),
    include_str!("value15_cases/dictionary_json.ts"),
    include_str!("promise_cases/async_order.ts"),
    include_str!("timer_cases/ordering.ts"),
    include_str!("language_core_cases/dense.ts"),
    // Arrays (dense, resizable, sparse, spread), strings and JSON.
    "function sum(...ns:number[]):number{let t=0;for(const n of ns){t+=n;}return t;}\
     export function main():void{const xs:number[]=[1,,3];xs.length=40;xs[50]=2;delete xs[0];\
     const ys:string[]=[];for(let i=0;i<40;i++){ys.push('v'+i);}\
     console.log(ys.slice(2,5).join(',')+xs.length+'ab'.repeat(3)+sum(...[1,2,3]));\
     const parsed=JSON.parse(JSON.stringify({a:ys,b:[[1],[2]]})) as unknown;\
     console.log(JSON.stringify(parsed));}",
];
const FROZEN_V37: [u64; 14] = [
    11415845110343043954,
    10798150618568729969,
    11768620899851489553,
    7785430916549467974,
    16383702132745089363,
    1212660727472207371,
    895193296770149157,
    6714276781863195572,
    15652847238520570651,
    1138486147511311127,
    16836258959093924556,
    2986555917714657622,
    1355792803980825303,
    8681907898598040574,
];

#[test]
fn v37_modules_keep_their_bytes() {
    let digests: Vec<u64> = FROZEN_V37_SOURCES
        .iter()
        .map(|source| digest(&compile(source, V37)))
        .collect();
    assert_eq!(digests, FROZEN_V37);
}

/// Run under explicit test-only budgets of `fuel` production fuel budgets
/// (CLAUDE.md §11), returning the logs or the typed failure and the fuel
/// consumed. Memory stays at the production 8 MiB budget.
fn run_budgeted(source: &str, profile: P, fuel: u64) -> (Result<Vec<String>, RuntimeError>, u64) {
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
    let used = fuel * abi::FUEL - store.get_fuel().unwrap();
    (
        result.map(|()| std::mem::take(&mut store.data_mut().output.lines)),
        used,
    )
}

fn resource(error: RuntimeError) -> ResourceExhausted {
    match error {
        RuntimeError::Resource(error) => error,
        other => panic!("expected a resource failure, got {other:?}"),
    }
}

/// Doubling a string: v37 traps at its 1 MiB per-value ceiling, v38 grows
/// it past 1 MiB until the 8 MiB memory budget and then reports the memory
/// resource instead of an anonymous trap.
const DOUBLING: &str = "export function main():void{let s='ab';for(let i=0;i<30;i++){s=s+s;\
     if(s.length>=(1<<20)){console.log(s.length);}}}";

#[test]
fn v38_strings_grow_past_one_mib_until_the_memory_budget() {
    let (result, used) = run_budgeted(DOUBLING, V38, 100);
    println!("doubling fuel {used}");
    assert_eq!(resource(result.unwrap_err()).resource, Resource::Memory);
    // V37 keeps its 1 MiB ceiling and its anonymous trap.
    let (result, _) = run_budgeted(DOUBLING, V37, 100);
    let error = result.unwrap_err();
    assert!(matches!(error, RuntimeError::Execution(_)), "{error:?}");
    assert!(error.to_string().contains("unreachable"), "{error}");
}
