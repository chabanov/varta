//! V38 `structural_subtyping`: width subtyping of records, interfaces and
//! class instances through views. Expected logs of the `subtyping_cases`
//! programs were produced by Node 24 (type stripping, `main()` appended) and
//! are pinned here.
use super::*;
use varta_common::language::LanguageProfile as P;

const V37: P = P::CheckedF64Utf16V37;
const V38: P = P::CheckedF64Utf16V38;

fn compile(source: &str, profile: P) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
fn run(source: &str, expected: &[&str]) {
    let lines = VartaRuntime::new()
        .run_wasm_bytes(&compile(source, V38), &[])
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
/// Functions defined by a module (its Wasm function section).
fn defined_functions(bytes: &[u8]) -> u32 {
    wasmparser::Parser::new(0)
        .parse_all(bytes)
        .find_map(|payload| match payload.unwrap() {
            wasmparser::Payload::FunctionSection(section) => Some(section.count()),
            _ => None,
        })
        .unwrap_or(0)
}

#[test]
fn record_and_class_views_match_node() {
    run(
        include_str!("subtyping_cases/views.ts"),
        &[
            "a=3",
            "a 3",
            "b b=3",
            "p:none q:n",
            r#"p2 {"label":"p2","extra":true}"#,
            r#"{"label":"p2","extra":true} true false"#,
            "3 -1",
            "b b=3",
            "c=4 10 c=5",
            "l=20 6",
            "b=3;c=5;l=30;lit=9;",
            "7 14 c",
            "true",
        ],
    );
}

#[test]
fn views_in_unions_generics_and_identity_match_node() {
    run(
        include_str!("subtyping_cases/unions.ts"),
        &[
            "1 1",
            "s true",
            "box 5",
            "readable s,kind k,kind boxed",
            r#"renamed s2 {"value":1,"label":"s2","count":2} {"label":"renamed","value":5,"kind":"boxed"}"#,
            "s2 1",
            r#"{"label":"renamed","value":5,"kind":"boxed"}"#,
            "true false",
            "1365",
        ],
    );
}

const LIFETIME_LOG: [&str; 2] = [
    "t1 o7 6,6,6,6,6,20;t2 o7 6,6,6,6,6,20;t3 o7 6,6,6,6,6,20;",
    r#"{"name":"entry2","parts":["p2"]} 4"#,
];

#[test]
fn views_across_suspensions_match_node() {
    run(include_str!("subtyping_cases/lifetime.ts"), &LIFETIME_LOG);
}

/// The scheduler is driven by hand with a full collection forced before
/// every job: objects reached only through view-typed arrays and parameters
/// of suspended frames survive, and their accessors still select the layout.
#[test]
fn views_survive_forced_collections() {
    use varta_compiler::suspension_test_hook as hook;
    let bytes =
        hook::compile_with_profile(include_str!("subtyping_cases/lifetime.ts"), V38).unwrap();
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
    assert!(jobs >= 3, "{jobs}");
    assert!(store.data().scheduler.unhandled_rejection().is_none());
    let expected: Vec<String> = LIFETIME_LOG
        .iter()
        .map(|line| format!("varta log: {line}"))
        .collect();
    assert_eq!(store.data().output.lines, expected);
}

/// A shape that never receives a wider value keeps direct slot access: the
/// program compiles to the same functions as under v37, and each view adds
/// only the accessors its member accesses use.
#[test]
fn only_view_accesses_get_accessors() {
    let program = |q: &str| {
        format!(
            "interface P{{x:number;y:number}}\
             function len(p:P):number{{return p.x+p.y;}}\
             export function main():void{{const q={q};\
             console.log(`${{len({{x:1,y:2}})}} ${{len(q)}}`);}}"
        )
    };
    let direct = program("{x:1,y:2}");
    let viewed = program("{x:1,y:2,z:3}");
    assert_eq!(
        defined_functions(&compile(&direct, V38)),
        defined_functions(&compile(&direct, V37))
    );
    // One read accessor per member read through the view `P`.
    assert_eq!(
        defined_functions(&compile(&viewed, V38)),
        defined_functions(&compile(&direct, V38)) + 2
    );
    run(&viewed, &["3 3"]);
}

#[test]
fn view_mismatches_name_both_types_and_the_members() {
    let prelude = "interface Readable<T>{readonly value:T;label:string}\
                   interface Signal<T> extends Readable<T>{count:number;set(v:T):void}\
                   function show(r:Readable<number>):string{return r.label+'='+r.value;}\
                   interface H{inner:Readable<number>}\
                   interface W{v:number|undefined}\
                   interface K{kind:string;a:number}interface L{kind:number;b:number}\
                   class C{label='c';value=1;count=0;set(v:number):void{this.value=v;}}\
                   class P{private label='p';value=1;}";
    for (body, expected) in [
        (
            "const p={label:'x'};show(p);",
            "argument type mismatch: expected Readable<number>, found { label: string } \
             (missing field value)",
        ),
        (
            "const p={label:1,value:2};show(p);",
            "expected Readable<number>, found { label: number; value: number } \
             (field label: expected string, found number)",
        ),
        (
            "const r:Readable<number>={label:'a',value:'b',more:true};",
            "found { label: string; more: boolean; value: string } (field value: expected \
             number, found string; extra fields (allowed): more)",
        ),
        (
            "show(new P());",
            "expected Readable<number>, found P (member label is not public in the class)",
        ),
        (
            "const a:Signal<number>[]=[];const b:Readable<number>[]=a;",
            "expected Readable<number>[], found Signal<number>[] (array element types must be \
             equal",
        ),
        (
            "const s:Signal<number>={value:1,label:'a',count:0,set(v:number):void{}};\
             const h={inner:s};const v:H=h;v.inner={value:2,label:'b'};",
            "cannot assign inner through H: an object of type { inner: Signal<number> } \
             assigned to it declares inner as Signal<number>, not Readable<number>",
        ),
        (
            "const o={v:1};const w:W=o;w.v=undefined;",
            "cannot assign v through W: an object of type { v: number } assigned to it \
             declares v as number, not number | undefined",
        ),
        (
            "const s:Signal<number>=new C();const f=s.set;",
            "method set of class C as a function value through a structural view",
        ),
        (
            "const x={kind:'k',a:1,b:'no'};const u:K|L=x;",
            "cannot enter this union: it has b, which only L declares",
        ),
    ] {
        let source = format!("{prelude}export function main():void{{{body}}}");
        let error = rejected(&source, V38);
        assert!(error.contains(expected), "{error}\n{source}");
    }
}

#[test]
fn views_reject_rest_patterns() {
    let source = "interface N{name:string}\
                  export function main():void{const e={name:'a',n:1};const v:N=e;\
                  const {name,...rest}=v;console.log(name);}";
    let error = rejected(source, V38);
    assert!(
        error.contains("rest pattern over N, which a wider object type is assigned to"),
        "{error}\n{source}"
    );
}

/// V37 keeps rejecting width subtyping with its old diagnostics, and its
/// `readonly` interface fields stay unsupported.
#[test]
fn v37_keeps_rejecting_structural_subtyping() {
    for (source, expected) in [
        (
            "interface N{name:string}function f(n:N):string{return n.name;}\
             export function main():void{const e={name:'a',n:1};f(e);}",
            "argument type mismatch: expected object, found object",
        ),
        (
            "interface I{a:number;}class C{a=1;}export function main():void{const i:I=new C();}",
            "expected object, found class instance",
        ),
        (
            "interface R{readonly a:number}function f(r:R):number{return r.a;}\
             export function main():void{}",
            "unsupported optional, readonly or computed object field",
        ),
    ] {
        let error = rejected(source, V37);
        assert!(error.contains(expected), "{error}\n{source}");
    }
}

/// FNV-1a digests of v37 modules compiled before the v38 streams: v37 keeps
/// its bytes while v38 admits views.
const FROZEN_V37: [u64; 10] = [
    5707565312455711993,
    15521361347602854831,
    2277750135931080506,
    8839106122054494097,
    17396809930316693728,
    11415845110343043954,
    10668204331314575741,
    13309652669340266339,
    1212660727472207371,
    7775064625867875443,
];
fn digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[test]
fn v37_modules_keep_their_bytes() {
    let digests: Vec<u64> = [
        include_str!("class_generic_cases/classes.ts"),
        include_str!("class_generic_cases/interfaces.ts"),
        include_str!("class_generic_cases/methods.ts"),
        include_str!("language_core_cases/unions.ts"),
        include_str!("language_core_cases/classes.ts"),
        include_str!("collection_cases/collections.ts"),
        include_str!("v37_integration_cases/instanceof_unions.ts"),
        include_str!("v37_integration_cases/sse_lines.ts"),
        include_str!("v37_integration_cases/union_collections.ts"),
        include_str!("regexp_cases/pilot_text.ts"),
    ]
    .iter()
    .map(|source| digest(&compile(source, V37)))
    .collect();
    assert_eq!(digests, FROZEN_V37);
}

#[test]
fn generic_method_signatures_erase_to_checked_calls_and_match_node() {
    run(
        include_str!("subtyping_cases/generic_methods.ts"),
        &["r1 3", r#"{"id":"r1","steps":3}"#, "object", "5 s"],
    );
}

#[test]
fn object_annotations_match_node() {
    run(
        include_str!("subtyping_cases/object_type.ts"),
        &["a=1&b=x&c=[1,2]", "none", r#"10 7 object {"z":true}"#],
    );
}

#[test]
fn array_is_array_narrows_unknown_to_unknown_arrays_like_node() {
    run(
        include_str!("subtyping_cases/unknown_arrays.ts"),
        &[
            "[3] number:1,number:2,number:3,",
            "[2] string:a,string:b,",
            "[5] number:1,string:x,boolean:true,object:[object Object],",
            "[2] boolean:true,boolean:false,",
            "2",
            "string",
            "tag=a&tag=b&n=3&ids=1&ids=2",
            r#"2 {"id":2}"#,
        ],
    );
}

#[test]
fn record_spreads_with_optional_sources_match_node() {
    run(
        include_str!("subtyping_cases/spreads.ts"),
        &[
            r#"{"label":"x","reconnect":false}"#,
            "3 false y",
            "undefined e 3 false undefined",
            "7 f 2 true 5",
            r#"{"x":1,"y":"three","z":true}"#,
            r#"{"y":"two","x":1}"#,
            r#"GET /b {"q":1} true"#,
            r#"{"a":1,"b":2}"#,
            r#"{"Accept":"text/event-stream"}"#,
        ],
    );
}

#[test]
fn property_narrowing_survives_calls_like_typescript_and_is_checked() {
    run(
        include_str!("subtyping_cases/property_narrowing.ts"),
        &[
            "note connected",
            "hello 2",
            "none",
            "note a",
            "stop",
            "no signal",
            "TypeError",
        ],
    );
}

#[test]
fn records_read_as_dictionaries_match_node() {
    run(
        include_str!("subtyping_cases/record_dictionaries.ts"),
        &[r#"{"subject_id":"s1","limit":3}"#, "{}"],
    );
}

#[test]
fn literal_fallbacks_keep_literal_types() {
    run(
        include_str!("subtyping_cases/literal_fallbacks.ts"),
        &["decode json", "decode void"],
    );
}

#[test]
fn uri_functions_match_node() {
    run(
        include_str!("subtyping_cases/uri.ts"),
        &[
            "/api/v1/agents/a%20b%2Fc%3Fd%3D%C3%A9%26%C3%BC%F0%9F%98%80/schedule",
            "safe-_.!~*'()AZaz09",
            "https://x.test/a%20b?q=1&r=%C3%A9#frag",
            "42",
            "URIError URI malformed",
        ],
    );
}

#[test]
fn promise_results_widen_to_unknown_and_records_read_as_json_objects() {
    run(
        include_str!("subtyping_cases/promise_covariance.ts"),
        &["a,b,c,"],
    );
}

/// Documented deviations from Node, pinned with their Varta results.
#[test]
fn checked_deviations_are_pinned() {
    // A generic method's `unknown` result is checked at the instantiated
    // type; TypeScript's unchecked `as T` would hand back the string.
    run(
        "interface E{data:string;json<T>():T}\
         interface Run{id:string}\
         function make(data:string):E{return {data,json<T>():T{return JSON.parse(data) as T;}};}\
         export function main():void{try{const e=make('\"text\"');const r=e.json<Run>();console.log(r.id);}\
         catch(e){console.log(e instanceof TypeError?'TypeError':'other');}}",
        &["TypeError"],
    );
    // Spread key order: fields every member provides come first; Node
    // prints {"retries":3,"reconnect":false,"label":"y"}.
    run(
        "interface O{reconnect?:boolean;retries?:number;label?:string}\
         function f(o:O|undefined,label:string):O{return {...o,label,reconnect:false};}\
         export function main():void{console.log(JSON.stringify(f({retries:3,reconnect:true},'y')));}",
        &[r#"{"label":"y","reconnect":false,"retries":3}"#],
    );
    // Key enumeration through a view holding another source's object lists
    // the view's members in canonical order (Node: tag,limit,cursor); an
    // object of the view's own layout keeps its order.
    run(
        "interface P{limit?:number;cursor?:string;tag:string}\
         function keys(p:P):string{let out='';for(const k in p)out+=k+';';return `${Object.keys(p).join(',')} ${out}`;}\
         export function main():void{const b:P={tag:'t',limit:2};console.log(keys({...b,cursor:'c'}));console.log(keys(b));}",
        &["cursor,limit,tag cursor;limit;tag;", "tag,limit tag;limit;"],
    );
    // A stale property narrowing throws instead of reading the new value.
    run(
        "class B{t='x';}class R{#b:B|null=new B();get body():B|null{return this.#b;}drop():void{this.#b=null;}}\
         export function main():void{const r=new R();\
         try{if(r.body){r.drop();console.log(r.body.t);}}\
         catch(e){console.log(e instanceof TypeError?e.message:'other');}}",
        &["property body no longer has the type an earlier test proved"],
    );
}

#[test]
fn pilot_item_mismatches_are_diagnosed() {
    for (source, expected) in [
        (
            "interface I{m<T>(x:T[]):T}export function main():void{const i:I={m<T>(x:T[]):T{return x[0]!;}};i.m<number>([1]);}",
            "type parameter of method m inside a parameter type",
        ),
        (
            "export function main():void{const o={f<T>(x:T):T{return x;}};}",
            "generic object literal method outside a generic method signature",
        ),
        (
            "class C{a=1;}export function main():void{const c=new C();const o={...c};}",
            "object spread of a C into a record",
        ),
        (
            "export function main():void{const n=1;const o={...n};}",
            "object spread of a number into a record",
        ),
    ] {
        let error = rejected(source, V38);
        assert!(error.contains(expected), "{error}\n{source}");
    }
}

/// V37 keeps rejecting the v38 pilot items with its old diagnostics.
#[test]
fn v37_keeps_rejecting_pilot_items() {
    for (source, expected) in [
        (
            "function f(o:object):number{return 1;}export function main():void{}",
            "unsupported type annotation",
        ),
        (
            "interface I{a:number;b?:string}function f(i:I):I{return {...i};}export function main():void{}",
            "unsupported object spread",
        ),
        (
            "interface E{json<T>():T}function f(e:E):void{}export function main():void{}",
            "generic method signature in an object type",
        ),
        (
            "export function main():void{console.log(encodeURIComponent('a b'));}",
            "encodeURIComponent",
        ),
    ] {
        let error = rejected(source, V37);
        assert!(error.contains(expected), "{error}\n{source}");
    }
}

/// `unknown[]` views and record graphs survive forced collections: the
/// synthesized boxing loop and the record-to-dictionary function allocate
/// while their sources are rooted.
#[test]
fn unknown_array_and_record_graph_views_survive_forced_collections() {
    use varta_compiler::suspension_test_hook as hook;
    let source = include_str!("subtyping_cases/promise_covariance.ts");
    let bytes = hook::compile_with_profile(source, V38).unwrap();
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
    while let Some(token) = store.data_mut().scheduler.next().unwrap() {
        // Test-only budget for the forced collections.
        let remaining = store.get_fuel().unwrap();
        store.set_fuel(abi::FUEL).unwrap();
        collect.call(&mut *store, ()).unwrap();
        store.set_fuel(remaining).unwrap();
        resume.call(&mut *store, (token, 0, 0)).unwrap();
    }
    assert!(store.data().scheduler.unhandled_rejection().is_none());
    assert_eq!(store.data().output.lines, ["varta log: a,b,c,"]);
}

#[test]
fn string_iteration_and_exhaustive_switches_match_node() {
    run(
        include_str!("subtyping_cases/language_items.ts"),
        &["[h][é][l][l][o][😀][!] 8", "z BC 20"],
    );
}
