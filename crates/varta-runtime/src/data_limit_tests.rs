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
    execution.allow_test_host_calls(u32::MAX);
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
    let pages = instance
        .get_memory(&mut *store, "memory")
        .map_or(0, |memory| memory.size(&*store));
    println!(
        "budgeted run: fuel {used}, host calls {}, pages {pages}",
        store.data().host_calls
    );
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

/// Fuel, host calls and final memory pages of v38 workloads at 10k and 100k
/// elements, under test-only fuel and host-call budgets and the production
/// 8 MiB memory budget. Each workload must finish or report the memory
/// resource; the numbers are recorded in docs/architecture.md.
#[test]
fn v38_data_costs_at_10k_and_100k_elements() {
    let workloads = [
        (
            "number array push",
            "const xs:number[]=[];for(let i=0;i<N;i++){xs.push(i);}console.log(String(xs.length));",
        ),
        (
            "number array indexed fill",
            "const xs:number[]=[];xs.length=N;for(let i=0;i<N;i++){xs[i]=i;}console.log(String(xs.length));",
        ),
        (
            "string array join",
            "const xs:string[]=[];xs.length=N;for(let i=0;i<N;i++){xs[i]='a';}console.log(String(xs.join('').length));",
        ),
        (
            "Map<number, number> set",
            "const m=new Map<number,number>();for(let i=0;i<N;i++){m.set(i,i);}console.log(String(m.size));",
        ),
        (
            "Set<number> add",
            "const s=new Set<number>();for(let i=0;i<N;i++){s.add(i);}console.log(String(s.size));",
        ),
        (
            "string of N code units",
            "let s='a';while(s.length<N){s=s+s;}console.log(String(s.slice(0,N).length));",
        ),
    ];
    for (name, body) in workloads {
        for n in [10_000, 100_000] {
            let source = format!(
                "export function main():void{{{}}}",
                body.replace('N', &n.to_string())
            );
            let (result, used) = run_budgeted(&source, V38, 10_000);
            match result {
                Ok(lines) => assert_eq!(lines, [format!("varta log: {n}")], "{name}"),
                Err(error) => assert_eq!(resource(error).resource, Resource::Memory, "{name}"),
            }
            println!("v38 cost: {name} n={n} fuel={used}");
        }
    }
}

const ARRAYS: &str = include_str!("data_limit_cases/arrays.ts");
const COLLECTIONS: &str = include_str!("data_limit_cases/collections.ts");

#[test]
fn v38_arrays_pass_1024_elements_and_match_node() {
    let (result, used) = run_budgeted(ARRAYS, V38, 400);
    println!("arrays fuel {used}");
    assert_eq!(
        result.unwrap_or_else(|e| panic!("{e:?}")),
        lines(&[
            "dense 5000 12497500 4999",
            "extended 7001 true 7",
            "present 7 true",
            "deleted 7001 true 11",
            "shrunk 3000 2999 2999",
            "slice 48 2000 2047",
            "keys 2047 9 2047",
            "spread 1140750",
            "strings 3000 s2999 13890 7:s1393",
            "sparse 4097 2 2",
            "mapped 2048 4094 4192236",
        ])
    );
}

#[test]
fn v38_collections_pass_1024_entries_and_match_node() {
    let (result, used) = run_budgeted(COLLECTIONS, V38, 1000);
    println!("collections fuel {used}");
    assert_eq!(
        result.unwrap_or_else(|e| panic!("{e:?}")),
        lines(&[
            "map 2000 2999 false 3000000",
            "set 2500 true false",
            "bytes 5000 230 58695",
            "text 3000 1500 true",
            "dictionary 550 p1 p1099 1099 false",
        ])
    );
}

/// V37 keeps every 1024-element ceiling: the same programs fail there.
#[test]
fn v37_keeps_its_1024_element_ceilings() {
    for (source, expected) in [
        (
            "export function main():void{const xs:number[]=[];for(let i=0;i<1025;i++){xs.push(i);}}",
            "unreachable",
        ),
        (
            "export function main():void{const m=new Map<number,number>();for(let i=0;i<1025;i++){m.set(i,i);}}",
            "Map maximum size exceeded",
        ),
        (
            "export function main():void{const b=new Uint8Array(1025);console.log(b.length);}",
            "exceeds the value-layout bound of 1024 elements",
        ),
        (
            "export function main():void{const d:Record<string,number>={};\
             for(let i=0;i<1025;i++){d[String.fromCharCode(256+i)]=i;}}",
            "Dictionary entry limit exceeded",
        ),
    ] {
        let (result, _) = run_budgeted(source, V37, 100);
        let error = result.unwrap_err().to_string();
        assert!(error.contains(expected), "{source}: {error}");
        let (result, _) = run_budgeted(source, V38, 100);
        assert!(result.is_ok(), "{source}: {result:?}");
    }
}

/// Collections forced while 20000 records are live overflow the v38 mark
/// worklist (the free tail of the root region) and fall back to the exact
/// fixed-point scans; a 5000-node linked list is marked in linear work. Both
/// keep every reachable value (no truncation) and match Node.
#[test]
fn v38_marking_overflows_its_worklist_exactly_and_marks_deep_chains_linearly() {
    let wide = "export function main():void{const xs:{v:number[]}[]=[];\
        for(let i=0;i<20000;i++){xs.push({v:[i]});}\
        let g='';for(let i=0;i<300;i++){g='x'+i+g.slice(0,64);}\
        let t=0;for(const x of xs){t+=x.v[0];}console.log(String(t));}";
    let (result, used) = run_budgeted(wide, V38, 2000);
    println!("wide marking fuel {used}");
    assert_eq!(result.unwrap(), lines(&["199990000"]));
    let deep = "interface Node{v:number;next:Node|null}\
        export function main():void{let head:Node|null=null;\
        for(let i=0;i<5000;i++){head={v:i,next:head};}\
        let g='';for(let i=0;i<300;i++){g='x'+i+g.slice(0,64);}\
        let t=0;let at:Node|null=head;while(at!==null){t+=at.v;at=at.next;}\
        console.log(String(t));}";
    let (result, used) = run_budgeted(deep, V38, 2000);
    println!("deep marking fuel {used}");
    assert_eq!(result.unwrap(), lines(&["12497500"]));
}
