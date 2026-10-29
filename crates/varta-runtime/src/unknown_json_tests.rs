//! `JSON.parse` and checked parsing to `unknown`, dictionary assertions and
//! `object` narrowing (v35 `unknown_json`) through the runtime. Logs of the
//! Node-comparable cases were produced by Node 24 for the same programs
//! (`v35_cases`) and are pinned here; Varta's deliberate deviations (checked
//! schema limits, failed assertions, non-dictionary objects) are pinned
//! separately.
use super::*;
use varta_common::language::LanguageProfile as P;

const V35: P = P::CheckedF64Utf16V35;

fn compile(source: &str, profile: P) -> Result<Vec<u8>, varta_compiler::CompilerError> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)?;
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    Ok(bytes)
}
fn run(source: &str) -> Result<RunOutput, RuntimeError> {
    let bytes = compile(source, V35).unwrap_or_else(|e| panic!("{e}\n{source}"));
    VartaRuntime::new().run_wasm_bytes(&bytes, &[])
}
fn assert_logs(source: &str, expected: &[&str]) {
    assert_eq!(
        run(source)
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
}
fn rejected(source: &str, profile: P) -> String {
    compile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("accepted under {profile:?}:\n{source}"))
        .to_string()
}

#[test]
fn json_parse_to_unknown_matches_node() {
    assert_logs(
        include_str!("v35_cases/unknown_json.ts"),
        &[
            r#"object:{"0":7,"type":"urn:x","title":"Bad","status":400,"extra":{"a":[1,"b",null,true]}} {"0":7,"type":"urn:x","title":"Bad","status":400,"extra":{"a":[1,"b",null,true]}}"#,
            r#"array [1,"two",{"three":3},[false],null]"#,
            r#"string:hi number:42.5 null boolean:true"#,
            r#"Bad 400 ["0","type","title","status","extra"] object:{"a":[1,"b",null,true]}"#,
            r#"t {"title":"t","n":1,"status":201}"#,
            r#"{"a":1,"b":"x"} number:1 undefined:undefined"#,
            r#"1 ["a","b"] {"k":[]}"#,
            r#"{"x":[1,{"y":null}]}"#,
            r#"{"q":[true]} 3 object:{"b":null}"#,
            r#"500 {"status":500,"z":1}"#,
            r#"syntax true proto {"__proto__":1} escaped {"A":"é"}"#,
        ],
    );
}

#[test]
fn object_narrowing_matches_node() {
    assert_logs(
        include_str!("v35_cases/narrowing.ts"),
        &[
            r#"problem ["status","detail","x"] gone 404"#,
            r#"object x;y; false true"#,
            r#"array"#,
            r#"object"#,
            r#"string"#,
            r#"[["k",true],["n",null]] falsy or primitive"#,
            r#"yes - -"#,
        ],
    );
}

#[test]
fn checked_json_limits_and_assertions_throw_typed_errors() {
    // Deviations from Node, all recoverable TypeErrors of the checked schema
    // or assertion: duplicate keys and numbers outside the finite domain are
    // outside schema 15, a parse at T checks the whole graph, and dictionary
    // assertions prove the storage descriptor (an array is no dictionary).
    assert_logs(
        "interface P{status?:number;[k:string]:unknown}interface Item{id:number}\
         function attempt(label:string,text:string):void{\
         try{console.log(label+' ok '+JSON.stringify(JSON.parse(text)));}\
         catch(e){if(e instanceof TypeError){console.log(label+' '+e.message);}}}\
         export function main():void{attempt('dup','{\"a\":1,\"a\":2}');attempt('big','1e400');\
         try{JSON.parse('{\"a\":1}') as Item;}catch(e){if(e instanceof TypeError){console.log(e.message);}}\
         try{varta.json.parse<P>('{\"status\":\"x\"}');}catch(e){if(e instanceof TypeError){console.log(e.message);}}\
         const list:unknown=JSON.parse('[1]');\
         try{const d=list as Record<string,unknown>;console.log(JSON.stringify(d));}catch(e){if(e instanceof TypeError){console.log(e.message);}}\
         const strings:unknown=JSON.parse('{\"a\":\"x\"}');\
         try{const d=strings as Record<string,string>;console.log(JSON.stringify(d));}catch(e){if(e instanceof TypeError){console.log(e.message);}}\
         if(typeof list==='object'&&list!==null){\
         try{console.log(String('a' in list));}catch(e){if(e instanceof TypeError){console.log(e.message);}}}}",
        &[
            "dup JSON value does not match the checked schema",
            "big JSON value does not match the checked schema",
            "JSON value does not match the checked schema",
            "JSON value does not match the checked schema",
            "type assertion to Record<string,unknown> failed",
            "type assertion to Record<string,string> failed",
            "object held by unknown is not a Record<string, unknown> dictionary",
        ],
    );
}

#[test]
fn json_depth_limit_stays_fatal() {
    // The checked JSON depth bound is a fatal budget, not a recoverable error.
    let deep = format!("{}1{}", "[".repeat(80), "]".repeat(80));
    let source = format!(
        "export function main():void{{try{{console.log(JSON.stringify(JSON.parse('{deep}')));}}catch(e){{console.log('caught');}}}}"
    );
    let error = run(&source).unwrap_err().to_string();
    assert!(error.contains("depth"), "{error}");
}

#[test]
fn unknown_json_graphs_survive_collection() {
    assert_logs(
        "export function main():void{\
         const kept:unknown[]=[];\
         const text='{\"n\":1,\"tags\":[\"t\",{\"deep\":[true,null]}]}';\
         for(let i=0;i<12;i++){kept.push(JSON.parse(text));\
         const junk:{v:string}={v:'garbage-'+String(i)};if(junk.v.length<0){return;}}\
         for(let i=0;i<40;i++){const junk:{v:string}={v:'more-'+String(i)};if(junk.v.length<0){return;}}\
         const last=kept[11];const first=kept[0];\
         console.log(JSON.stringify(first)+' '+JSON.stringify(last)+' '+String(kept.length)+' '+String(first===last));}",
        &[
            r#"{"n":1,"tags":["t",{"deep":[true,null]}]} {"n":1,"tags":["t",{"deep":[true,null]}]} 12 false"#,
        ],
    );
}

#[test]
fn object_narrowing_admits_only_typescript_object_operations() {
    for (source, message) in [
        (
            "export function main():void{const v:unknown=JSON.parse('{}');\
             if(typeof v==='object'&&v!==null){console.log(String(v.detail));}}",
            "property access on an unknown value",
        ),
        (
            "export function main():void{const v:unknown=JSON.parse('{}');\
             if(typeof v==='object'){console.log(String('a' in v));}}",
            "may be null",
        ),
        (
            "export function main():void{const v:unknown=JSON.parse('{}');console.log(String('a' in v));}",
            "operator on an unknown value",
        ),
        (
            "export function main():void{const v:unknown=JSON.parse('{}');const k:string='a';\
             if(typeof v==='object'&&v!==null&&'a' in v){console.log(String(v[k]));}}",
            "dynamic key on an object narrowed from unknown",
        ),
        (
            "export function main():void{const v:unknown=JSON.parse('{}');\
             if(typeof v==='object'&&v!==null&&'a' in v){console.log(String(v.b));}}",
            "does not exist on an object narrowed from unknown",
        ),
        (
            "export function main():void{const v=JSON.parse('{}',(k:string,x:unknown)=>x);}",
            "JSON.parse with a reviver",
        ),
        (
            "export function main():void{const n:number=1;const v=JSON.parse(n);}",
            "JSON input type mismatch",
        ),
        (
            "interface I{id:number}interface P{x:{a:number};[k:string]:unknown}\
             export function main():void{const p=varta.json.parse<P[]>('[]');}",
            "JSON parsing target type",
        ),
        (
            // A recursive mixed record would nest itself below the root.
            "interface N{a?:string;[k:string]:N|string|undefined}\
             export function main():void{const n=varta.json.parse<N>('{}');}",
            "JSON parsing target type",
        ),
    ] {
        let error = rejected(source, V35);
        assert!(error.contains(message), "{source}: {error}");
    }
}

#[test]
fn earlier_profiles_keep_rejecting_json_to_unknown() {
    for (source, message) in [
        (
            "export function main():void{const v=JSON.parse('1');}",
            "unsupported call",
        ),
        (
            "export function main():void{const v=varta.json.parse<unknown>('1');}",
            "JSON parsing target type",
        ),
        (
            "export function main():void{const v:unknown=1;\
             if(typeof v==='object'&&v!==null){console.log(String('a' in v));}}",
            "operator on an unknown value",
        ),
        (
            "export function main():void{const v:unknown=1;const d=v as Record<string,unknown>;}",
            "dictionaries are checked by the element kind",
        ),
    ] {
        let error = rejected(source, P::CheckedF64Utf16V34);
        assert!(error.contains(message), "{source}: {error}");
    }
}
