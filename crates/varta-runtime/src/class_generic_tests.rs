//! V37 `class_generics`: generic methods, interface method signatures,
//! `implements` and `#private` members. Expected logs of the
//! `class_generic_cases` programs were produced by Node 24 (type stripping,
//! `main()` appended) and are pinned here.
use super::*;
use varta_common::language::LanguageProfile as P;

const V36: P = P::CheckedF64Utf16V36;
const V37: P = P::CheckedF64Utf16V37;

fn compile(source: &str, profile: P) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
fn run(source: &str, expected: &[&str]) {
    let lines = VartaRuntime::new()
        .run_wasm_bytes(&compile(source, V37), &[])
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
fn generic_methods_match_node() {
    run(
        include_str!("class_generic_cases/methods.ts"),
        &[
            "7 u7 #3 2 u1/#2 4",
            "[2,2]",
            r#""aa""#,
            r#"{"a":"x","b":1}"#,
            "1 none dog animal !12",
            "animal !8 3 0",
            "animal 0 | animal s | [animal true]",
            "dog animal 1 | dog animal s | [dog animal true]",
            "dog animal 2 | dog animal s | [dog animal true]",
        ],
    );
}

#[test]
fn private_names_match_node() {
    run(
        include_str!("class_generic_cases/private_names.ts"),
        &[
            "4 got /a 3",
            "105 100 5 false true",
            "true true false",
            "bump 2;bump 4;got /a;",
        ],
    );
}

#[test]
fn interface_methods_and_implements_match_node() {
    run(
        include_str!("class_generic_cases/interfaces.ts"),
        &[
            "closed 1",
            "literal abc 9",
            "closed 2",
            "literal abc 9",
            "arrow x 3",
            "ann greets bo box cy box boxed 8",
            "hi z -1 function",
            r#"{"kind":"literal"}"#,
            r#"1 {"total":0}"#,
        ],
    );
}

const LIFETIME_LOG: &str = "<api/p1>:a|<api/p1>:b|c2;<api/p2>:a|<api/p2>:b|end; 7 3 3";

#[test]
fn generic_async_method_instances_match_node() {
    run(
        include_str!("class_generic_cases/lifetime.ts"),
        &[LIFETIME_LOG],
    );
}

/// The checkpoint is driven by hand with a full collection forced before
/// every job: receivers, private fields and captured closures of suspended
/// generic async method instances survive.
#[test]
fn generic_method_frames_survive_forced_collections() {
    use varta_compiler::suspension_test_hook as hook;
    let bytes =
        hook::compile_with_profile(include_str!("class_generic_cases/lifetime.ts"), V37).unwrap();
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
    assert!(jobs >= 6, "{jobs}");
    assert!(store.data().scheduler.unhandled_rejection().is_none());
    assert_eq!(
        store.data().output.lines,
        [format!("varta log: {LIFETIME_LOG}")]
    );
}

#[test]
fn class_generic_constructs_are_rejected_with_diagnostics() {
    for (source, expected) in [
        (
            "class A{pick<T>(a:T,b:T):T{return a;}}export function main():void{const x=new A().pick(1,'x');}",
            "type arguments of method A.pick were inferred",
        ),
        (
            "class A{make<T>():T[]{return [];}}export function main():void{const x=new A().make();}",
            "cannot infer type argument T of method A.make",
        ),
        (
            "class A{m<T>(x:T):T{return x;}}class B extends A{m<T,U>(x:T):T{return x;}}export function main():void{}",
            "override of m must keep the inherited type parameters",
        ),
        (
            "class A{m<T>(x:T):T{return x;}}class B extends A{m(x:number):number{return x;}}export function main():void{}",
            "override of m must keep the inherited method signature",
        ),
        (
            "class A{m<T>(x:T):T{return x;}}export function main():void{const a=new A();const f=a.m;}",
            "class method value without a call",
        ),
        (
            "class A{private m<T>(x:T):T{return x;}}export function main():void{new A().m(1);}",
            "class member m is private to A",
        ),
        (
            "class A{m<T>(x:T,n:number):number{return n===0?0:this.m<T[]>([x],n-1);}}export function main():void{new A().m(1,3);}",
            "generic instantiation depth of method A.m exceeds limit",
        ),
        (
            "class A{m<T>(x:any):T{return x;}}export function main():void{}",
            "type annotation",
        ),
        (
            "class A{m<T>(...xs:T[]):number{return xs.length;}}export function main():void{const a=[1];new A().m(...a);}",
            "spread argument to a generic method",
        ),
        (
            "class A{static #x=1;}export function main():void{}",
            "static #private class field",
        ),
        (
            "class A{#x=1;}class B extends A{#x=2;}export function main():void{}",
            "#private name #x redeclared by a subclass",
        ),
        (
            "class A{#x=1;}export function main():void{const u:unknown=new A();}",
            "may reach a class instance with #private fields",
        ),
        (
            "class A{#x=1;get():number{return this['#x'];}}export function main():void{}",
            "computed access to a class member named like a #private name",
        ),
        (
            "class A{'#x'=1;}export function main():void{}",
            "quoted class member named like a #private name",
        ),
        (
            "class A{#x=1;static has(v:A|string):boolean{return #x in v;}}export function main():void{}",
            "must be an object type",
        ),
        (
            "class A{#v=0;set #x(n:number){this.#v=n;}}export function main():void{}",
            "#private class setter",
        ),
        (
            "interface I{a:number;b():string;}class C implements I{a=1;}export function main():void{}",
            "class C incorrectly implements I: member b is missing",
        ),
        (
            "interface I{a:number;}class C implements I{a='x';}export function main():void{}",
            "field a has type string, not number",
        ),
        (
            "interface I{a:number;}class C implements I{private a=1;}export function main():void{}",
            "member a is not public in the class",
        ),
        (
            "class D{a=1;}class C implements D{a=1;}export function main():void{}",
            "class implements class D",
        ),
        (
            "interface I{a:number;}class C implements I{a=1;}export function main():void{const i:I=new C();}",
            "expected object, found class instance",
        ),
        (
            "interface I{a:number;f():number;}export function main():void{const o:I={a:1,f(){return this.a;}};}",
            "this or super in an object literal method",
        ),
        (
            "interface I{json<T>():T;}export function main():void{const x:I|undefined=undefined;}",
            "generic method signature in an object type",
        ),
        (
            "export function main():void{const o={f<T>(x:T):T{return x;}};}",
            "generic object literal method",
        ),
    ] {
        let error = rejected(source, V37);
        assert!(error.contains(expected), "{error}\n{source}");
    }
}

/// V36 keeps every rejection the stream lifts for v37.
#[test]
fn v36_keeps_rejecting_class_generic_constructs() {
    for (source, expected) in [
        (
            "class A{m<T>(x:T):T{return x;}}export function main():void{}",
            "async, generator, generic or decorated function",
        ),
        (
            "class A{#x=1;}export function main():void{}",
            "#private class field",
        ),
        (
            "class A{#m():void{}}export function main():void{}",
            "#private class method",
        ),
        (
            "interface I{a:number;}class C implements I{a=1;}export function main():void{}",
            "class implements clause",
        ),
        (
            "interface I{f():number;}export function main():void{const i:I|undefined=undefined;}",
            "object type member",
        ),
        (
            "export function main():void{const o={f(x:number):number{return x;}};}",
            "object method, getter or setter",
        ),
        (
            "class A{static m<T>(x:T):T{return x;}}export function main():void{}",
            "async, generator, generic or decorated function",
        ),
    ] {
        let error = rejected(source, V36);
        assert!(error.contains(expected), "{error}\n{source}");
    }
}

/// FNV-1a over a module's bytes: v36 output is frozen by the digests below,
/// recorded from the compiler before the v37 class-generics stream.
const FROZEN_V36: [u64; 5] = [
    8111264769745579624,
    3695575224277677483,
    6200128730592170896,
    8861900056676514014,
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
        include_str!("class_generic_cases/frozen_v36.ts"),
        include_str!("async_method_cases/methods.ts"),
        include_str!("function_field_cases/fields.ts"),
        include_str!("v36_cases/declared_fields.ts"),
        include_str!("v36_cross_cases/backoff_stream.ts"),
    ]
    .iter()
    .map(|source| digest(&compile(source, V36)))
    .collect();
    assert_eq!(digests, FROZEN_V36);
}

/// A program of `resources` resource methods in the uarp-sdk pattern: each
/// returns `this.client.request(spec)` with its own response type, inferred
/// from the method's `Promise<R>` result, and `request<T>` decodes through a
/// checked `unknown as T`. With `distinct`, every response interface has its
/// own shape; otherwise they are structurally equal.
fn pilot_pattern(resources: usize, distinct: bool) -> String {
    let mut source = String::from(
        "interface Spec{method:string;path:string}\
         async function decode(path:string):Promise<unknown>{return JSON.parse(`{\"id\":${path.length}}`);}\
         class Transport{calls:number=0;\
         async request<T>(spec:Spec):Promise<T>{this.calls+=1;return (await decode(spec.path)) as T;}}\
         class Resource{readonly client:Transport;constructor(client:Transport){this.client=client;}",
    );
    for n in 0..resources {
        source.push_str(&format!(
            "get{n}():Promise<R{n}>{{return this.client.request({{method:'GET',path:'/r{n}'}});}}"
        ));
    }
    source.push('}');
    for n in 0..resources {
        if distinct {
            source.push_str(&format!("interface R{n}{{id:number;f{n}?:string}}"));
        } else {
            source.push_str(&format!("interface R{n}{{id:number}}"));
        }
    }
    // Every resource method is called: v37 removes unreachable functions, so
    // only reachable instances count against the function limit.
    let calls: String = (1..resources)
        .map(|n| format!("await r.get{n}();"))
        .collect();
    source.push_str(&format!(
        "export async function main():Promise<void>{{const r=new Resource(new Transport());\
         const a=await r.get0();{calls}console.log(`${{a.id}} ${{r.client.calls}}`);}}"
    ));
    source
}

/// Budget measurement for the pilot pattern. Every distinct response type is
/// one `request<T>` instance (the async wrapper, its body and the checked
/// conversion of the response): ten Wasm functions per type with the resource
/// method itself, while structurally equal response types share one
/// instance. The artifact function limit (user plus internal functions)
/// admits 24 distinct response types in one program; the uarp-sdk declares
/// 670, so the whole pilot cannot be one program with one instance per type.
#[test]
fn pilot_request_pattern_function_counts() {
    let lines = VartaRuntime::new()
        .run_wasm_bytes(&compile(&pilot_pattern(3, true), V37), &[])
        .unwrap()
        .lines;
    assert_eq!(lines, ["varta log: 3 3"]);
    let count =
        |n: usize, distinct: bool| defined_functions(&compile(&pilot_pattern(n, distinct), V37));
    // Equal shapes: one more resource method costs exactly its own function.
    let one = count(1, false);
    assert_eq!((count(2, false) - one, count(10, false) - one), (1, 9));
    let per_type = [
        count(2, true) - count(1, true),
        count(3, true) - count(2, true),
        count(4, true) - count(3, true),
    ];
    assert_eq!(per_type, [10, 10, 10]);
    VartaRuntime::new()
        .run_wasm_bytes(&compile(&pilot_pattern(24, true), V37), &[])
        .unwrap();
    let error = VartaRuntime::new()
        .run_wasm_bytes(&compile(&pilot_pattern(25, true), V37), &[])
        .unwrap_err();
    assert!(error.to_string().contains("too many functions"), "{error}");
}

#[test]
fn generic_classes_match_node() {
    run(
        include_str!("class_generic_cases/classes.ts"),
        &[
            "3 1 #1 c1 empty",
            "false done true c2 2",
            "box(s) box(4) box(true)",
            "1 2 true",
            "9 2 4",
            "9 4 3",
            r##"{"page":{"items":["#1","#2","#3"],"cursor":"c1"},"tag":"t"}"##,
            r#"{"items":[true],"cursor":"z","hasMore":true}"#,
            "true cursor object",
            r#"false {"items":["x","y"],"cursor":"c2","hasMore":true}"#,
            "3",
        ],
    );
}

#[test]
fn class_generics_language_guide_example_matches_node() {
    run(include_str!("class_generic_cases/guide.ts"), &["2 6 1"]);
}

/// Dispatch stays exact when instances appear late: a generic subclass of a
/// class whose method is overridden joins the dispatcher's descriptor tests,
/// and a declared subclass of an instance overrides the instance's method.
/// Generic async generator methods instantiate like other methods.
#[test]
fn generic_class_instances_join_dispatch_like_node() {
    run(
        "class Base{m():string{return 'base';}}class Mid extends Base{m():string{return 'mid';}}\
         class Gen<T> extends Mid{v:T;constructor(v:T){super();this.v=v;}}\
         class Page<T>{items:T[];constructor(items:T[]){this.items=items;}label():string{return `page ${this.items.length}`;}}\
         class Special extends Page<string>{label():string{return `special ${this.items[0]??''}`;}}\
         class Feed{async *each<T>(xs:T[]):AsyncGenerator<T,void,undefined>{for(let i=0;i<xs.length;i++)yield xs[i]!;}}\
         function show(b:Base):string{return b.m();}function label(p:Page<string>):string{return p.label();}\
         export async function main():Promise<void>{\
         console.log(`${show(new Base())} ${show(new Gen<number>(1))} ${show(new Gen('s'))}`);\
         console.log(`${label(new Page(['a','b']))} ${label(new Special(['z']))}`);\
         let out='';for await(const x of new Feed().each(['p','q'])){out+=x;}console.log(out);}",
        &["base mid mid", "page 2 special z", "pq"],
    );
}

#[test]
fn generic_class_constructs_are_rejected_with_diagnostics() {
    for (source, expected) in [
        (
            "class A{m():number{return 1;}}class B<T> extends A{m():number{return 2;}}export function main():void{new B<number>();}",
            "override of m in generic class B",
        ),
        (
            "class P<T>{v:T;constructor(v:T){this.v=v;}}export function main():void{const p:P=new P(1);}",
            "class P requires a type argument for T",
        ),
        (
            "class P<T>{static make(x:T):number{return 1;}}export function main():void{P.make(1);}",
            "type reference T",
        ),
        (
            "class E<T> extends Error{}export function main():void{}",
            "generic Error subclass",
        ),
        (
            "class P<T>{v:T[]=[];}export function main():void{const p=new P();}",
            "without its own constructor",
        ),
        (
            "class P<T>{}export function main():void{const p=new P<number,string>();}",
            "class P expects at most 1 type argument(s), found 2",
        ),
        (
            "class P<T extends string>{}export function main():void{const p=new P<number>();}",
            "type argument number for T of class P does not satisfy its constraint",
        ),
        (
            "interface S{size():number;}class Bag<T> implements S{}export function main():void{new Bag<string>();}",
            "class Bag<string> incorrectly implements S: member size is missing",
        ),
        (
            "class Base{}class G<T> extends Base{v:T;constructor(v:T){super();this.v=v;}}\
             export function main():void{const b:Base=new G(1);console.log(String(b instanceof G));}",
            "instanceof generic class G on a value not typed as one of its instantiations",
        ),
        (
            "class G<T>{}export function main():void{const u:unknown=new G<number>();console.log(String(u instanceof G));}",
            "instanceof generic class G",
        ),
    ] {
        let error = rejected(source, V37);
        assert!(error.contains(expected), "{error}\n{source}");
    }
    let error = rejected(
        "class P<T>{items:T[]=[];}export function main():void{}",
        V36,
    );
    assert!(error.contains("unsupported generic class"), "{error}");
}

/// A field initialized by `new C()` (or `new C<A>()`) takes the class type
/// without an annotation, as in the pilot's `readonly #controller = new
/// AbortController()`; Node 24 prints the same line.
#[test]
fn constructed_field_initializers_infer_their_class_like_node() {
    run(
        "class Box<T>{v:T;constructor(v:T){this.v=v;}}\
         class S{readonly #c=new AbortController();#b=new Box<number>(2);\
         close():void{this.#c.abort();}get closed():boolean{return this.#c.signal.aborted;}\
         get v():number{return this.#b.v;}}\
         export function main():void{const s=new S();const before=s.closed;s.close();\
         console.log(`${before} ${s.closed} ${s.v}`);}",
        &["false true 2"],
    );
    let error = rejected(
        "class Box<T>{v:T;constructor(v:T){this.v=v;}}class S{b=new Box(2);}export function main():void{}",
        V37,
    );
    assert!(error.contains("requires a type annotation"), "{error}");
}
