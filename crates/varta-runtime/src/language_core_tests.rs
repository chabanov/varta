//! V37 language core (`language_core`): captures before initialization with
//! temporal dead zones, deferred `let` initialization, dense element typing,
//! property-path narrowing, `switch` by strict equality, functions assignable
//! to `void` results, and the related class-field completions. Expected logs
//! of the `language_core_cases` programs were produced by Node 24 (v24.20.0,
//! type stripping, `main()` appended) and are pinned here, so the tests need
//! no Node installation; Varta's documented deviations are pinned separately.
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
fn execute(source: &str) -> Result<RunOutput, RuntimeError> {
    let bytes = compile(source, V37);
    scheduler::with_virtual_clock(|| VartaRuntime::new().run_wasm_bytes(&bytes, &timer_grant()))
}
fn run(source: &str, expected: &[&str]) {
    let output = execute(source).unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    assert_eq!(output.lines, lines(expected), "{source}");
}
/// `run`, then the same async program with a full collection forced before
/// every job (test-only collector export and fuel for the collections).
fn run_collecting(source: &str, expected: &[&str]) {
    run(source, expected);
    let (log, result) = super::timer_tests::observe_with_profile(source, &timer_grant(), true, V37);
    result.unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    assert_eq!(log, lines(expected), "collected: {source}");
}
/// `run` with a test-only fuel ceiling, so that the program's garbage loops
/// trigger many collections; production memory and fuel limits are unchanged.
fn run_with_stress_fuel(source: &str, expected: &[&str]) {
    let bytes = compile(source, V37);
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
    execution
        .store
        .set_fuel(1000 * varta_common::abi::FUEL)
        .unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    assert_eq!(
        execution.store.data().output.lines,
        lines(expected),
        "{source}"
    );
}

const TDZ: &str = include_str!("language_core_cases/tdz.ts");
const SLEEP: &str = include_str!("language_core_cases/sleep.ts");
const DEFERRED: &str = include_str!("language_core_cases/deferred.ts");
const DENSE: &str = include_str!("language_core_cases/dense.ts");
const DENSE_DEVIATIONS: &str = include_str!("language_core_cases/dense_deviations.ts");
const SWITCHES: &str = include_str!("language_core_cases/switches.ts");
const VOID_FUNCTIONS: &str = include_str!("language_core_cases/void_functions.ts");
const PATHS: &str = include_str!("language_core_cases/paths.ts");
const CLASSES: &str = include_str!("language_core_cases/classes.ts");
const LINE_PROTOCOL: &str = include_str!("language_core_cases/line_protocol.ts");
const RETRY_LOOP: &str = include_str!("language_core_cases/retry_loop.ts");

#[test]
fn captures_before_initialization_match_node() {
    run_collecting(
        TDZ,
        &[
            "2 timer sees function|abort true",
            "ReferenceError: Cannot access 'late' before initialization",
            "10",
            "12",
            "L0L1L2",
            "ReferenceError true",
            "Cannot access 'total' before initialization",
            "42",
            "run;cleanup;",
            "async ok",
        ],
    );
}

#[test]
fn cancellable_delay_captures_its_listener_before_declaration() {
    run_collecting(
        SLEEP,
        &["first", "rejected cancelled", "rejected already", "after"],
    );
}

#[test]
fn uncaught_temporal_dead_zone_access_reports_a_reference_error() {
    let source = "export function main():void{const read=():number=>later;\
        console.log('before');read();const later=1;}";
    let error = execute(source).unwrap_err().to_string();
    assert!(
        error.contains("ReferenceError")
            && error.contains("Cannot access 'later' before initialization"),
        "{error}"
    );
}

#[test]
fn deferred_initialization_matches_node() {
    run(
        DEFERRED,
        &[
            "b2 true",
            "caught negative",
            "onefewmany",
            "12 13",
            "0",
            "finally",
        ],
    );
}

#[test]
fn dense_element_reads_match_node() {
    run(
        DENSE,
        &[
            "12",
            "a:1;b:2; first=a",
            "abc",
            "1,undefined,3,undefined",
            "9",
            "x",
            "lit!",
            "eral!",
        ],
    );
}

#[test]
fn dense_reads_of_holes_and_missing_indices_deviate_from_node() {
    // Node reads `undefined` (typed as the element type by TypeScript); Varta
    // throws `TypeError` so a value of the element type is never missing.
    // `at` keeps its `T | undefined` result in both.
    run(
        DENSE_DEVIATIONS,
        &[
            "oob TypeError array element is missing (a hole or an index out of range)",
            "hole TypeError array element is missing (a hole or an index out of range)",
            "forof TypeError array element is a hole",
            "string TypeError string index out of range",
            "at undefined",
            "true",
            "listener",
            "true",
        ],
    );
}

#[test]
fn string_literal_and_typeof_switches_match_node() {
    run(
        SWITCHES,
        &[
            "1 12 12 10 99",
            "EDother:id",
            "number 2|string BC|undefined|boolean true",
            "a;b;B; a;b;default;",
        ],
    );
}

#[test]
fn value_functions_convert_to_void_function_types_like_node() {
    run(VOID_FUNCTIONS, &["4 4 7"]);
}

#[test]
fn property_path_narrowing_matches_node() {
    run(
        PATHS,
        &[
            "label x unnamed",
            "70 2",
            "none|count 8|no count",
            "L!-",
            "ABc",
            "3-2",
            "2",
        ],
    );
}

#[test]
fn class_field_completions_match_node() {
    run(CLASSES, &["2 undefined undefined", "true outer", "true"]);
}

#[test]
fn combined_reader_and_retry_programs_match_node() {
    run(
        LINE_PROTOCOL,
        &[
            "a x+kind='q' 4",
            "plain y undefined",
            "plain w undefined",
            r"true a\'b undefined",
            "1",
        ],
    );
    run_collecting(
        RETRY_LOOP,
        &["200 1", "gave up after 1", "gave up after 1", "status 500"],
    );
}

#[test]
fn timer_handles_are_typed_by_return_type_queries() {
    // The pilot's `let timer: ReturnType<typeof setTimeout> | undefined;`.
    run_collecting(
        "export async function main():Promise<void>{\
         let timer:ReturnType<typeof setTimeout>|undefined;\
         const done=new Promise<string>((resolve)=>{timer=setTimeout(()=>resolve('fired'),1);});\
         console.log(await done);if(timer)clearTimeout(timer);\
         const interval:ReturnType<typeof setInterval>=setInterval(()=>{},5);clearInterval(interval);\
         console.log(String(timer!==undefined));}",
        &["fired", "true"],
    );
    let error = rejected(
        "export function main():void{const t:ReturnType<typeof main>=undefined;}",
        V37,
    );
    assert!(error.contains("ReturnType"), "{error}");
}

#[test]
fn language_core_guide_example_matches_node() {
    run_collecting(
        include_str!("language_core_cases/guide.ts"),
        &["slept", "keyvalue id", "ReferenceError", "1"],
    );
}

/// Early binding cells, their flags, `void` wrappers and narrowed path reads
/// stay rooted while loops allocate enough garbage to collect repeatedly.
#[test]
fn early_cells_and_void_wrappers_survive_collections() {
    run_with_stress_fuel(
        "interface Box{label?:string;n:number}\n\
         function churn():number{let s='x';for(let i=0;i<12;i++){s=s+s;}let n=0;\
         for(let i=0;i<300;i++){const parts:string[]=[s,s+'y'];n+=parts.length;}return n;}\n\
         function run(cb:()=>void):void{cb();}\n\
         export function main():void{\
         const read=():string=>held.label!+String(held.n);\
         const bump=():number=>{held.n+=churn();return held.n;};\
         churn();\
         const held:Box={label:'kept'+String(churn()),n:0};\
         run(bump);churn();run(bump);\
         console.log(read());\
         const box:Box={label:'p'+String(churn()),n:1};\
         if(box.label!==undefined){const label:string=box.label;churn();console.log(label);}\
         function late():string{return tail+'!';}\
         const tail:string='t'+String(churn());\
         churn();console.log(late());}",
        &["kept6001200", "p600", "t600!"],
    );
}

/// Diagnostics of the subset boundaries: what stays a compile-time error.
#[test]
fn language_core_diagnostics_name_the_rejected_construct() {
    let narrowing = "interface O{n?:number}\nfunction touch(o:O):void{o.n=undefined;}\n\
        function use(n:number):number{return n;}\n";
    for (source, expected) in [
        (
            "export function main():void{const f=():string=>x;const x=String(1);console.log(f());}"
                .to_owned(),
            "capture before initialization of a binding whose type is inferred from its initializer",
        ),
        (
            "export function main():void{console.log(String(x));const x=1;}".to_owned(),
            "binding used before initialization: x",
        ),
        (
            "export function main():void{const f=():number=>a;const [a]=[1];console.log(String(f()));}"
                .to_owned(),
            "capture before binding initialization",
        ),
        (
            "export function main():void{let x:number;console.log(String(x));}".to_owned(),
            "binding used before being assigned: x",
        ),
        (
            "export function main():void{let x:number;if(Math.random()>0.5)x=1;console.log(String(x));}"
                .to_owned(),
            "binding used before being assigned: x",
        ),
        (
            "export function main():void{let x:number;for(let i=0;i<2;i++){x=i;}console.log(String(x));}"
                .to_owned(),
            "binding used before being assigned: x",
        ),
        (
            "function f():number{throw new Error('e');}\n\
             export function main():void{let x:number;try{x=f();}catch(e){console.log(String(x));}}"
                .to_owned(),
            "binding used before being assigned: x",
        ),
        (
            "export function main():void{let x:number;x+=1;}".to_owned(),
            "binding used before being assigned: x",
        ),
        (
            "export function main():void{let x:number;x++;}".to_owned(),
            "binding used before being assigned: x",
        ),
        (
            "export function main():void{let x:number;const f=():number=>x;x=1;console.log(String(f()));}"
                .to_owned(),
            "capture before binding initialization",
        ),
        (
            "export function main():void{const f=():number=>x;let x:number;x=1;console.log(String(f()));}"
                .to_owned(),
            "capture of a deferred let binding before its declaration",
        ),
        (
            "export function main():void{let x;x=1;}".to_owned(),
            "uninitialized variable without an undefined-capable let annotation",
        ),
        (
            format!("{narrowing}export function main():void{{const o:O={{n:1}};if(o.n!==undefined){{touch(o);use(o.n);}}}}"),
            "argument type mismatch: expected number, found primitive union",
        ),
        (
            format!("{narrowing}export function main():void{{const o:O={{n:1}};const p=o;if(o.n!==undefined){{p.n=undefined;use(o.n);}}}}"),
            "argument type mismatch: expected number, found primitive union",
        ),
        (
            format!("{narrowing}export function main():void{{let o:O={{n:1}};if(o.n!==undefined){{o={{}};use(o.n);}}}}"),
            "argument type mismatch: expected number, found primitive union",
        ),
        (
            format!("{narrowing}function f(o:O):boolean{{o.n=undefined;return true;}}\n\
                export function main():void{{const o:O={{n:1}};if(o.n!==undefined&&f(o)){{use(o.n);}}}}"),
            "argument type mismatch: expected number, found primitive union",
        ),
        (
            format!("{narrowing}export function main():void{{const o:O={{n:1}};if(o.n!==undefined){{const g=():number=>use(o.n);g();}}}}"),
            "argument type mismatch: expected number, found primitive union",
        ),
        (
            "export function main():void{const s:string='a';switch(s){case 1:break;}}".to_owned(),
            "switch case type mismatch",
        ),
        (
            // A test with an effect disables clause narrowing.
            "function other(x:number|string):string{return 'string';}\n\
             function f(x:number|string):void{\
             switch(typeof x){case other(x):break;case 'number':{const n:number=x;console.log(String(n));}}}\nexport function main():void{f(1);}"
                .to_owned(),
            "type mismatch for n: expected number, found primitive union",
        ),
        (
            "export function main():void{const s:string='a';switch(s){case 'a':const t=1;break;}}"
                .to_owned(),
            "case declaration without a block",
        ),
        (
            "function run(cb:(...n:number[])=>void):void{cb(1);}\n\
             function f(...n:number[]):number{return 1;}\n\
             export function main():void{run(f);}"
                .to_owned(),
            "argument type mismatch: expected (...p0: number[]) => void, found (...p0: number[]) => number",
        ),
        (
            "class K{a:number;b:number;f:()=>number;constructor(){this.a=1;this.f=():number=>this.b;this.b=2;}}\n\
             export function main():void{const k=new K();console.log(String(k.f()));}"
                .to_owned(),
            "class field b is read before it is definitely assigned",
        ),
        (
            "function keep(k:K):void{}\nclass K{a=1;f:()=>void;constructor(){this.f=():void=>keep(this);}}\n\
             export function main():void{new K();}"
                .to_owned(),
            "constructor closures cannot capture this",
        ),
        (
            "class K{a=1;f:()=>number;constructor(){this.f=():number=>this.m();}m():number{return this.a;}}\n\
             export function main():void{const k=new K();console.log(String(k.f()));}"
                .to_owned(),
            "constructor this may only access declared fields",
        ),
        (
            "class K{a:number;constructor(){}}\nexport function main():void{new K();}".to_owned(),
            "class field a is not definitely assigned in the constructor",
        ),
    ] {
        let error = rejected(&source, V37);
        assert!(error.contains(expected), "{error}\n{source}");
    }
}

/// Every construct of this stream keeps its v36 diagnostic.
#[test]
fn v36_keeps_rejecting_the_language_core_constructs() {
    for (source, expected) in [
        (
            "export function main():void{const f=():void=>{g();};function g():void{}f();}",
            "unsupported call",
        ),
        (
            "export function main():void{const f=():number=>x+1;const x=41;console.log(String(f()));}",
            "capture before binding initialization",
        ),
        (
            "export function main():void{let x:number;x=1;console.log(String(x));}",
            "uninitialized variable without an undefined-capable let annotation",
        ),
        (
            "export function main():void{const xs:number[]=[1];const y:number=xs[0];}",
            "type mismatch for y: expected number, found primitive union",
        ),
        (
            "interface A{n?:number}\nfunction use(n:number):void{}\n\
             export function main():void{const a:A={n:1};if(a.n!==undefined)use(a.n);}",
            "argument type mismatch: expected number, found primitive union",
        ),
        (
            "export function main():void{const s:string='a';switch(s){case 'a':break;}}",
            "operand requiring number or boolean",
        ),
        (
            "function run(cb:()=>void):void{cb();}\nfunction val():number{return 3;}\n\
             export function main():void{run(val);}",
            "argument type mismatch: expected function, found function",
        ),
        (
            "class K{a:number|undefined;}\nexport function main():void{new K();}",
            "class field a is not definitely assigned in the constructor",
        ),
        (
            "class K{n=0;f:()=>number;constructor(){this.f=():number=>this.n;}}\n\
             export function main():void{new K();}",
            "constructor closures cannot capture this",
        ),
        (
            "class C{}\nclass K{c=new C();}\nexport function main():void{new K();}",
            "class field c requires a type annotation or a literal initializer",
        ),
        (
            "function f():number{for(;;){return 1;}}\nexport function main():void{f();}",
            "value-returning function must return on every path",
        ),
    ] {
        let error = rejected(source, V36);
        assert!(error.contains(expected), "{error}\n{source}");
        compile(source, V37);
    }
}

/// FNV-1a over a module's bytes: v36 output is frozen by the digests below,
/// recorded from the compiler before the v37 language-core stream.
const FROZEN_V36: [u64; 6] = [
    6200128730592170896,
    15086325483135027354,
    8861900056676514014,
    5469260486046824319,
    2493880408488466042,
    3148053378128132457,
];
fn digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[test]
fn v36_modules_keep_their_bytes() {
    let digests: Vec<u64> = [
        include_str!("function_field_cases/fields.ts"),
        include_str!("function_field_cases/chains.ts"),
        include_str!("v36_cases/declared_fields.ts"),
        include_str!("timer_cases/sleep.ts"),
        include_str!("async_generator_cases/features.ts"),
        include_str!("v36_cross_cases/backoff_stream.ts"),
    ]
    .iter()
    .map(|source| digest(&compile(source, V36)))
    .collect();
    assert_eq!(digests, FROZEN_V36);
}
