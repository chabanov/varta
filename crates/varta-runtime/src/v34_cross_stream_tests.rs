//! Interactions between the v34 streams: value layout 15 records
//! (dictionaries, wide records, recursive types), `unknown` with checked
//! assertions, and promises/async functions. Expected logs of the
//! Node-comparable programs were produced by Node 24 (type stripping, `main()`
//! appended) and are pinned here; Varta-specific deviations are marked.
use super::*;
use varta_common::language::LanguageProfile as P;

const V34: P = P::CheckedF64Utf16V34;

fn compile(source: &str) -> Vec<u8> {
    compile_with(source, V34)
}
fn compile_with(source: &str, profile: P) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    // Emitted modules must also pass backend validation, not only compilation.
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
/// Each program also runs with the v35 liveness frames (`frame_liveness`).
fn run(source: &str, expected: &[&str]) {
    let expected: Vec<String> = expected.iter().map(|s| format!("varta log: {s}")).collect();
    for profile in [V34, P::CheckedF64Utf16V35] {
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&compile_with(source, profile), &[])
                .unwrap_or_else(|e| panic!("{profile:?} {e:?}\n{source}"))
                .lines,
            expected,
            "{profile:?}"
        );
    }
}
fn rejected(source: &str) -> String {
    varta_compiler::compile_typescript_with_profile(source, V34)
        .err()
        .unwrap_or_else(|| panic!("accepted:\n{source}"))
        .to_string()
}

#[test]
fn async_functions_return_dictionaries_of_wide_records_across_collections() {
    run(
        include_str!("v34_cross_cases/wide_dictionary.ts"),
        &[
            "started",
            r#"["0","k0","k1","k2"]"#,
            "w1 10 1 odd",
            r#"{"id":"w9","tags":{"kind":"odd"},"f38":90,"f0":9}"#,
            "true false true",
        ],
    );
}

#[test]
fn caught_rejections_are_unknown_and_checked_assertions_recover_dictionaries() {
    run(
        include_str!("v34_cross_cases/caught_unknown.ts"),
        &[
            "ApiError 503",
            "RangeError: range",
            "headers later [object Object]",
            "string text",
            "ok 4",
            r#"[["a","x"],["b","y"]]"#,
        ],
    );
}

#[test]
fn recursive_json_values_cross_awaits_inside_unknown_and_dictionaries() {
    run(
        include_str!("v34_cross_cases/json_tree.ts"),
        &[
            "object",
            r#"{"depth":2,"children":[{"depth":1,"children":["leaf","leaf"],"even":false},{"depth":1,"children":["leaf","leaf"],"even":false}],"even":true}"#,
            "string leaf",
            r#"["tree","leaf","count"]"#,
            "number 2",
        ],
    );
}

/// Varta-specific: TypeScript erases `as`, so Node never throws where these
/// dictionary assertions fail; Varta tests the tag and the value vector's
/// element kind instead of reinterpreting the cell.
#[test]
fn dictionary_assertions_check_the_tag_and_value_element_kind() {
    run(
        "function check(value:unknown,label:string):void{\
         try{const d=value as Record<string,string>;console.log(`${label} ok ${Object.keys(d).length}`);}\
         catch(e){if(e instanceof TypeError){console.log(`${label} ${e.message}`);}}}\
         export function main():void{\
         const strings:Record<string,string>={a:'x'};const numbers:Record<string,number>={a:1};\
         check(strings,'strings');check(numbers,'numbers');check({a:'x'},'record');check(['x'],'array');\
         check('x','string');const n:unknown=numbers;\
         const back=n as Record<string,number>;back['b']=2;console.log(JSON.stringify(numbers));\
         const maybe:Record<string,number>|undefined=numbers;console.log(Object.keys(maybe!).length);}",
        &[
            "strings ok 1",
            "numbers type assertion to Record<string,string> failed",
            "record type assertion to Record<string,string> failed",
            "array type assertion to Record<string,string> failed",
            "string type assertion to Record<string,string> failed",
            r#"{"a":1,"b":2}"#,
            "2",
        ],
    );
    for (source, expected) in [
        (
            "export function main():void{const u:unknown={};const d=u as Record<string,unknown>;}",
            "unsupported type assertion to Record<string,unknown>: dictionaries are checked",
        ),
        (
            "interface P{x:number}export function main():void{const u:unknown={};const d=u as Record<string,P>;}",
            "unsupported type assertion to Record<string,P>: dictionaries are checked",
        ),
    ] {
        let error = rejected(source);
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn unknown_rejects_promises_and_dictionaries_reject_promise_values() {
    for (source, expected) in [
        (
            "export async function main():Promise<void>{const p=Promise.resolve(1);const u:unknown=p;await p;}",
            "promise value is not assignable to unknown; promises carry no value tag",
        ),
        (
            "async function f():Promise<number>{return 1;}export function main():void{const u:unknown[]=[];u.push(f());}",
            "promise",
        ),
        (
            "export function main():void{const d:Record<string,Promise<number>>={};}",
            "unsupported index signature value type",
        ),
    ] {
        let error = rejected(source);
        assert!(error.contains(expected), "{source}\n{error}");
    }
    // Promises may carry dictionaries, wide records and `unknown` values.
    run(
        "async function pair():Promise<Record<string,unknown>>{await null;return {a:1,b:'two'};}\
         async function cell():Promise<unknown>{await null;return [1,2];}\
         export async function main():Promise<void>{const d=await pair();const c=await cell();\
         console.log(`${typeof d['a']} ${typeof d['b']} ${Array.isArray(c)}`);}",
        &["number string true"],
    );
}

/// Varta-specific report: a thrown `unknown` holding a dictionary is not an
/// exception-channel value, so the throw site raises a readable `TypeError`.
#[test]
fn uncaught_dictionaries_held_by_unknown_report_a_type_error() {
    let source = "export async function main():Promise<void>{const d:Record<string,string>={a:'x'};\
                  const u:unknown=d;await null;throw u;}";
    let error = VartaRuntime::new()
        .run_wasm_bytes(&compile(source), &[])
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("unhandled promise rejection")
            && error.contains("thrown value is not a primitive, BigInt or Error value"),
        "{error}"
    );
}

/// Narrowed `unknown` bindings survive the join after a branch that ran a
/// checked assertion or a nested statement (both paths keep the refinement).
#[test]
fn unknown_refinements_survive_branch_joins() {
    // Node 24 prints the same lines.
    run(
        "class ApiError extends Error{constructor(message:string,options?:{cause?:unknown}){super(message,options);this.name='ApiError';}}\
         interface Detail{code:number}\
         function first(e:unknown,kind:number):string{if(e instanceof ApiError){const cause=e.cause;\
         if(kind===0){const d=cause as Detail;return `${d.code}`;}return `${e.message}`;}return 'other';}\
         function second(x:unknown,flag:boolean):string{if(typeof x==='string'||typeof x==='number'){\
         if(flag){console.log('flag');}return `${typeof x} ${x}`;}return 'none';}\
         export function main():void{const e=new ApiError('m',{cause:{code:7}});console.log(first(e,0));\
         console.log(first(e,1));console.log(second('s',true));console.log(second(2,false));console.log(second(null,false));}",
        &["7", "m", "flag", "string s", "number 2", "none"],
    );
}

/// Recursive types in async frames and closure cells: the catalog stays an
/// acyclic DAG with shallow back-edge nodes (a cell for a union, an aggregate
/// for a record); a dictionary that contains itself directly has none.
#[test]
fn recursive_records_and_unions_live_in_frames_and_closures() {
    // Node 24 prints the same lines.
    run(
        "interface Tree{label:string;children:Tree[]}\
         type Json=null|number|string|Json[]|{[key:string]:Json};\
         async function grow(depth:number):Promise<Tree>{const node:Tree={label:`d${depth}`,children:[]};\
         if(depth>0){await null;const child=await grow(depth-1);node.children.push(child);}return node;}\
         export async function main():Promise<void>{const tree=await grow(2);\
         const value:Json={tree:[tree.label,tree.children.length]};const read=()=>JSON.stringify(value);\
         await null;console.log(JSON.stringify(tree));console.log(read());}",
        &[
            r#"{"label":"d2","children":[{"label":"d1","children":[{"label":"d0","children":[]}]}]}"#,
            r#"{"tree":["d2",1]}"#,
        ],
    );
    let error = rejected(
        "type Nest={[key:string]:Nest};export async function main():Promise<void>{\
         const n:Nest={};await null;console.log(Object.keys(n).length);}",
    );
    assert!(
        error.contains("a recursive type in a captured binding or frame slot"),
        "{error}"
    );
}
