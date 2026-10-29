//! V36 `function_fields` (value layout 17): function values in fields,
//! elements, dictionaries, unions and `unknown`; optional and defaulted
//! function-typed parameters; optional chains; `Math.random` as a value; the
//! v36 abort listeners and `Promise.withResolvers`. Expected logs of the
//! `function_field_cases` programs were produced by Node 24 (type stripping,
//! `main()` appended) and are pinned here; Varta-specific results are marked.
use super::*;
use varta_common::{language::LanguageProfile as P, value};

const V35: P = P::CheckedF64Utf16V35;
const V36: P = P::CheckedF64Utf16V36;

fn compile(source: &str, profile: P) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
fn run(source: &str, expected: &[&str]) {
    let lines = VartaRuntime::new()
        .run_wasm_bytes(&compile(source, V36), &[])
        .unwrap_or_else(|e| panic!("{e}\n{source}"))
        .lines;
    let expected: Vec<String> = expected.iter().map(|l| format!("varta log: {l}")).collect();
    assert_eq!(lines, expected, "{source}");
}
fn rejected(source: &str, profile: P) -> String {
    varta_compiler::compile_typescript_with_profile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("{profile:?} accepted\n{source}"))
        .to_string()
}

#[test]
fn function_fields_elements_dictionaries_and_unions_match_node() {
    run(
        include_str!("function_field_cases/fields.ts"),
        &[
            "42 2 1",
            "10",
            "default 1",
            "custom 2",
            "16 4 8",
            "function true false",
            "true 1",
            "40",
            r#"["a","b","c"]"#,
            "10",
            r#"{"name":"double"}"#,
            r#"{"name":"inc"}"#,
            "[null,null,null,null]",
            "{}",
        ],
    );
}

#[test]
fn optional_and_defaulted_function_parameters_match_node() {
    run(
        include_str!("function_field_cases/parameters.ts"),
        &[
            "done build",
            "300",
            "true",
            "got b",
            "skipped called",
            "none some",
            "plain called 2 7",
            "function not fn",
            "true true",
            "undefined",
        ],
    );
}

#[test]
fn optional_chains_short_circuit_and_evaluate_once_like_node() {
    run(
        include_str!("function_field_cases/chains.ts"),
        &[
            "undefined undefined",
            "undefined n",
            "x m",
            "42 undefined",
            "8 undefined 4",
            "3 3",
            "undefined 10 1",
            "y 1",
            "stop",
        ],
    );
}

#[test]
fn abort_listeners_and_with_resolvers_match_node() {
    run(
        include_str!("function_field_cases/listeners.ts"),
        &[
            r#"["onabort","first","once","reason true true"]"#,
            "4",
            "2 why",
            r#"["promise","resolve","reject"] function"#,
            "sync end",
            "resolved 5",
            "cancelled stop",
            "woke",
            "rejected true",
            "finished",
        ],
    );
}

#[test]
fn closures_reached_only_through_fields_survive_the_scheduler() {
    run(
        include_str!("function_field_cases/lifetime.ts"),
        &["13 a!2 key/3 cell/4 12"],
    );
}

/// The checkpoint is driven by hand with a full collection forced before
/// every job: closure environments and their binding cells reachable only
/// through record fields, array elements, a dictionary entry, a class field
/// in an `unknown` cell and a union cell survive while the frames are queued.
#[test]
fn closure_environments_reached_only_through_fields_survive_forced_collections() {
    use varta_compiler::suspension_test_hook as hook;
    let bytes =
        hook::compile_with_profile(include_str!("function_field_cases/lifetime.ts"), V36).unwrap();
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
    assert!(jobs >= 4, "{jobs}");
    assert!(store.data().scheduler.unhandled_rejection().is_none());
    assert_eq!(
        store.data().output.lines,
        ["varta log: 13 a!2 key/3 cell/4 12"]
    );
}

/// Synchronous heap pressure: allocations beyond the current memory force
/// the nonmoving collector to run while closures are reachable only through
/// fields, and their captured cells keep their values and identity.
#[test]
fn heap_pressure_keeps_closures_reached_only_through_fields() {
    let source = "interface Slot{get:()=>string;bump:()=>number}\
        function make(tag:string):Slot{let n=0;const kept=tag+'#';\
        return{get:():string=>kept+String(n),bump:():number=>{n+=1;return n;}};}\
        function churn():void{let payload='x';for(let i=0;i<14;i=i+1){payload=payload+payload;}\
        for(let i=0;i<350;i=i+1){const values=[payload+'!',payload+'?'];values.slice();}}\
        export function main():void{const slots:Slot[]=[make('a'),make('b')];\
        const table:Record<string,()=>string>={};table['c']=make('c').get;\
        const first=slots[0];const same=first?first.get:undefined;churn();\
        let total=0;for(const s of slots){total+=s?.bump()??0;}churn();\
        console.log(`${slots[0]?.get()??''} ${slots[1]?.get()??''} ${table['c']?.()??''} ${total}`);\
        console.log(String(same===slots[0]?.get));}";
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source, V36)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    // Test-only budget for the churn, as in `closure_tests`.
    execution.store.set_fuel(300 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    crate::exceptions::finish(&instance, &mut execution.store).unwrap();
    assert_eq!(
        execution.store.data().output.lines,
        ["a#1 b#1 c#0 2", "true"].map(|s| format!("varta log: {s}"))
    );
}

/// Varta-specific display: a logged function shows as Node shows an
/// anonymous one (Node prints `[Function: f]` with the inferred name), inside
/// the diagnostic notation of logged aggregates.
#[test]
fn logged_functions_show_as_anonymous_functions() {
    let source = "interface Named{id:number;run:()=>number}\
        export function main():void{const f=():number=>1;console.log(f);\
        const n:Named={id:7,run:f};const nu:unknown=n;console.log(nu);\
        const a:unknown=[f];console.log(a);const m:(()=>number)|string=f;console.log(m);\
        const d:Record<string,()=>number>={a:f};const du:unknown=d;console.log(du);\
        console.log(JSON.stringify(n));console.log(String(JSON.stringify(f)===undefined));}";
    run(
        source,
        &[
            "[Function (anonymous)]",
            r#"{"id":7,"run":[Function (anonymous)]}"#,
            "[[Function (anonymous)]]",
            "[Function (anonymous)]",
            r#"{"a":[Function (anonymous)]}"#,
            r#"{"id":7}"#,
            "true",
        ],
    );
}

#[test]
fn function_field_misuse_fails_with_explicit_diagnostics() {
    for (source, expected) in [
        (
            "interface M{v:number;m:(this:M,x:number)=>number}\
             export function main():void{const o:M={v:1,m:function(this:M,x:number):number{return this.v+x;}};console.log(o.m(1));}",
            "method-style call of a function field whose type declares a this parameter",
        ),
        (
            "interface T{a:number;toJSON:()=>string}\
             export function main():void{const t:T={a:1,toJSON:():string=>'x'};console.log(JSON.stringify(t));}",
            "function-valued toJSON field",
        ),
        (
            "export function main():void{const f:(()=>number)|((x:number)=>number)=():number=>1;}",
            "promise or function types in one union",
        ),
        (
            "interface O{cb?:()=>void}export function main():void{const o:O={};o.cb();}",
            "narrow it or call it with ?.()",
        ),
        (
            "interface J{a:number;f:()=>void}\
             export function main():void{const j=varta.json.parse<J>('{\"a\":1}');console.log(j.a);}",
            "JSON parsing target type",
        ),
        (
            "export function main():void{try{throw ():number=>1;}catch(e){console.log('x');}}",
            "exception value other than primitives",
        ),
        (
            "export function main():void{const f=():number=>1;console.log(String(f));}",
            "String conversion requires primitives",
        ),
        (
            "export function main():void{const u:unknown=():number=>1;const f=u as ()=>number;}",
            "type assertion",
        ),
        (
            "export function main():void{const x:number=1;console.log(String(x?.toFixed(1)));}",
            "",
        ),
    ] {
        if expected.is_empty() {
            compile(source, V36);
            continue;
        }
        let error = rejected(source, V36);
        assert!(error.contains(expected), "{error}\n{source}");
    }
}

/// Earlier profiles keep rejecting every v36 construct with their own
/// diagnostics.
#[test]
fn earlier_profiles_keep_rejecting_function_fields() {
    for (source, expected) in [
        (
            "interface H{run:(x:number)=>number}export function main():void{const h:H={run:(x:number):number=>x};}",
            "function-valued record fields",
        ),
        (
            "class C{f:()=>void=():void=>{};}export function main():void{const c=new C();}",
            "function-valued class field",
        ),
        (
            "export function main():void{const d:Record<string,()=>void>={};}",
            "function-valued index signature",
        ),
        (
            "export function main():void{const f:(()=>void)|undefined=undefined;}",
            "union members other than number, boolean, string, null or undefined",
        ),
        (
            "export function main():void{const fs:(()=>void)[]=[];}",
            "array element type",
        ),
        (
            "export function main():void{const r:()=>number=Math.random;}",
            "Math.random constant or method value",
        ),
        (
            "export function main():void{const u:unknown=():number=>1;}",
            "arrow requires a function context",
        ),
        (
            "export function main():void{const s=new AbortController().signal;s.addEventListener('abort',()=>{});}",
            "abort event listeners",
        ),
        (
            "export function main():void{const p=Promise.withResolvers<number>();}",
            "Promise.withResolvers",
        ),
    ] {
        let error = rejected(source, V35);
        assert!(error.contains(expected), "{error}\n{source}");
    }
    // An optional chain stays outside the v35 subset.
    let error = rejected(
        "export function main():void{const s:string|undefined='a';console.log(String(s?.length));}",
        V35,
    );
    assert!(!error.is_empty());
}

/// FNV-1a over a module's bytes: v35 output is frozen by the digests below,
/// recorded from the compiler before the v36 function-fields stream.
const FROZEN_V35: [u64; 8] = [
    15351816373486663376,
    5125862885875761115,
    14995179864672439071,
    16723624736534093609,
    9723125809200081693,
    10287070445773548314,
    8037201275931455646,
    13656919633917562884,
];
fn digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[test]
fn v35_modules_keep_their_bytes() {
    let digests: Vec<u64> = [
        include_str!("web_cases/abort_basics.ts"),
        include_str!("web_cases/abort_flow.ts"),
        include_str!("web_cases/headers_values.ts"),
        include_str!("promise_value_cases/combinators.ts"),
        include_str!("async_method_cases/closures.ts"),
        include_str!("v35_cases/class_fields.ts"),
        include_str!("v35_cases/narrowing.ts"),
        include_str!("function_field_cases/frozen_v35.ts"),
    ]
    .iter()
    .map(|source| digest(&compile(source, V35)))
    .collect();
    assert_eq!(digests, FROZEN_V35);
}

/// A string field tagged as a function and a function field tagged as a
/// promise fail the value17 readers' envelope checks (closure block, promise
/// record) before the value is observed. Descriptor kinds are module data:
/// readers check that a payload is a well-formed value of the declared kind,
/// not where it came from.
#[test]
fn tampered_function_field_tags_are_rejected() {
    let source = "interface R{f:()=>number;s:string}\
        export function main():void{const r:R={f:():number=>1,s:'text'};console.log(JSON.stringify(r));}";
    let bytes = compile(source, V36);
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime.run_wasm_bytes(&bytes, &[]).unwrap().lines,
        [r#"varta log: {"s":"text"}"#]
    );
    // Descriptor entries: key pointer, key length, slot word, kind.
    let entry = |slot: u32, kind: value::ValueKind| {
        let mut bytes = 1u32.to_le_bytes().to_vec();
        bytes.extend(slot.to_le_bytes());
        bytes.extend((kind as u32).to_le_bytes());
        bytes
    };
    for (slot, kind, tampered) in [
        (0, value::ValueKind::Function, value::ValueKind::Promise),
        (1, value::ValueKind::String, value::ValueKind::Function),
    ] {
        let pattern = entry(slot, kind);
        let at = bytes
            .windows(pattern.len())
            .position(|w| w == pattern.as_slice())
            .expect("descriptor entry in the data section");
        let mut changed = bytes.clone();
        let kind_at = at + pattern.len() - 4;
        changed[kind_at..kind_at + 4].copy_from_slice(&(tampered as u32).to_le_bytes());
        let error = runtime.run_wasm_bytes(&changed, &[]).unwrap_err();
        assert!(
            !error.to_string().is_empty(),
            "{kind:?} -> {tampered:?}: {error}"
        );
    }
}

#[test]
fn function_field_language_guide_example_matches_node() {
    run(
        include_str!("function_field_cases/guide.ts"),
        &[
            "450",
            r#"["retry 1","retry 2"]"#,
            "{}",
            "aborted shutdown",
            "ready",
        ],
    );
}
