//! Checked declared fields (v36 `checked_declared_fields`) through the
//! runtime. Logs of the Node-comparable cases were produced by Node 24 for the
//! same programs (`v36_cases`) and are pinned here; Varta's deliberate
//! deviations (checked conversions, fresh values from JSON graphs, empty-array
//! identity) are pinned separately.
use super::*;
use varta_common::language::LanguageProfile as P;

const V36: P = P::CheckedF64Utf16V36;

fn compile(source: &str, profile: P) -> Result<Vec<u8>, varta_compiler::CompilerError> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)?;
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    Ok(bytes)
}
fn logs(expected: &[&str]) -> Vec<String> {
    expected.iter().map(|s| format!("varta log: {s}")).collect()
}
fn assert_logs(source: &str, expected: &[&str]) {
    let bytes = compile(source, V36).unwrap_or_else(|e| panic!("{e}\n{source}"));
    let output = VartaRuntime::new()
        .run_wasm_bytes(&bytes, &[])
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    assert_eq!(output.lines, logs(expected));
}
fn rejected(source: &str, profile: P) -> String {
    compile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("accepted under {profile:?}:\n{source}"))
        .to_string()
}
/// Run with a test-only fuel ceiling so the loop allocates enough garbage to
/// collect repeatedly; production memory and fuel limits are unchanged.
fn assert_logs_with_stress_fuel(source: &str, expected: &[&str]) {
    let bytes = compile(source, V36).unwrap_or_else(|e| panic!("{e}\n{source}"));
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
    assert_eq!(execution.store.data().output.lines, logs(expected));
}

#[test]
fn declared_fields_match_node() {
    assert_logs(
        include_str!("v36_cases/declared_fields.ts"),
        &[
            r#"{"title":"Bad","errors":[{"field":"a","message":"m"}],"meta":{"requestId":"r1","tags":["x"]},"labels":{"env":"prod"},"extensions":{"code":"E1","hint":3},"trace":[1,2]}"#,
            r#"true true"#,
            r#"2 a=m;undefined=second;"#,
            r#"r1 ["x"]"#,
            r#"prod 1"#,
            r#"E1 {"code":"E1","hint":3}"#,
            r#"b=undefined; [{"message":"as list"}]"#,
            r#"-1 false"#,
            r#""plain""#,
            r#"name=required;age=undefined; true"#,
            r#"name=required;age=undefined;added=undefined;"#,
            r#"r2 ["a","b"]"#,
            r#"1"#,
            r#"E2"#,
            r#"{"type":"about:blank","status":422,"errors":[{"message":"required","field":"name"},{"field":"age"},{"field":"added"}],"meta":{"tags":["a","b"],"requestId":"r2"},"labels":{"x":"1"},"extensions":{"code":"E2","n":[1]},"trace":"t"}"#,
            r#"422 name=required;age=undefined; {"tags":["a","b"],"requestId":"r2"}"#,
            r#"about:blank 2"#,
            r#"true"#,
            r#"{"errors":[{"field":"late"}]}"#,
            r#"copied name=required;age=undefined; 422"#,
            r#"2 f"#,
            r#"r3 true"#,
        ],
    );
}

#[test]
fn pilot_error_hierarchy_matches_node() {
    assert_logs(
        include_str!("v36_cases/problem_errors.ts"),
        &[
            r#"422 Invalid (correlationId: c-1) | UnprocessableEntityError"#,
            r#"VALIDATION fields true"#,
            r#"2 true"#,
            r#"name: required"#,
            r#"undefined: too long"#,
            r#"404 Missing undefined 0"#,
            r#"server <html>bad gateway</h"#,
        ],
    );
}

#[test]
fn conversions_and_record_completions_match_node() {
    assert_logs(
        include_str!("v36_cases/conversions.ts"),
        &[
            r#"[{"id":"a","tags":["x"]},{"id":"b"}] [[1,2],[3]]"#,
            r#"root 3"#,
            r#"number,text,null,"#,
            r#"{"a":1,"b":2} true"#,
            r#"true {"items":[{"id":"a","tags":["x"]},{"id":"b"},{"id":"c","tags":[]}],"tree":{"name":"root","kids":[{"name":"leaf"},{"name":"mid","kids":[{"name":"deep"}]}]},"matrix":[[1,2],[3]],"values":[1,"two",null],"choice":{"a":1,"b":2}}"#,
            r#"true"#,
            r#"from message from error"#,
            r#"[{"id":"u"}] "plain""#,
            r#"r 4"#,
        ],
    );
}

#[test]
fn checked_conversions_deviate_from_node() {
    // Node keeps whatever is stored (TypeScript erases the types) and returns
    // the same array every time. Varta converts at the declared type: a value
    // that does not fit throws, records take exactly their declared keys (as
    // in checked parsing), an empty array cannot prove its element type and
    // is replaced on each read, a JSON value is replaced by its conversion on
    // the first declared read, and assertions of JSON graphs build fresh
    // values.
    assert_logs(
        "interface Item{id:string;tags?:string[]}\
         interface Doc{items?:Item[];one?:Item;[key:string]:unknown}\
         function show(label:string,f:()=>string):void{try{console.log(label+' '+f());}\
         catch(e){console.log(label+' '+(e instanceof TypeError?'TypeError: '+e.message:'other'));}}\
         export function main():void{\
         const wrong=JSON.parse('{\"items\":[{\"id\":1}]}') as Record<string,unknown> as Doc;\
         show('wrong',()=>String(JSON.stringify(wrong.items)));\
         const extra=JSON.parse('{\"one\":{\"id\":\"x\",\"extra\":true}}') as Record<string,unknown> as Doc;\
         show('extra',()=>String(JSON.stringify(extra.one)));\
         const missing=JSON.parse('{\"one\":{\"tags\":[]}}') as Record<string,unknown> as Doc;\
         show('missing',()=>String(JSON.stringify(missing.one)));\
         show('parse',()=>String(JSON.stringify((JSON.parse('{\"items\":[{\"id\":\"a\",\"n\":1}]}') as Doc).items)));\
         const empty:Doc={items:[]};const a=empty.items;const b=empty.items;\
         show('empty',()=>String(a===b)+' '+String(b===empty.items));\
         const body=JSON.parse('{\"items\":[{\"id\":\"q\"}]}') as Record<string,unknown>;\
         const raw=body['items'];const doc=body as Doc;\
         show('alias',()=>String(raw===body['items'])+' '+String(doc.items===body['items'])+' '+String(raw===body['items']));\
         const list:unknown=JSON.parse('[{\"id\":\"q\"}]');\
         show('fresh',()=>String((list as Item[])===(list as Item[])));\
         const typed:Item[]=[{id:'t'}];const boxed:unknown=typed;\
         show('exact',()=>String((boxed as Item[])===typed));}",
        &[
            "wrong TypeError: property items does not hold its declared type",
            "extra TypeError: property one does not hold its declared type",
            "missing TypeError: property one does not hold its declared type",
            "parse TypeError: JSON value does not match the checked schema",
            "empty false false",
            "alias true true false",
            "fresh false",
            "exact true",
        ],
    );
}

#[test]
fn structural_record_assertions_accept_every_key_order() {
    // A record literal keeps its source key order in its layout; v36 proves
    // the shape by any of its descriptors, v35 keeps the canonical test.
    let source = "type R={a:number;b:number};export function main():void{\
         const r:R={b:1,a:2};const u:unknown=r;\
         try{console.log(JSON.stringify(u as R));}catch(e){console.log('failed');}}";
    assert_logs(source, &[r#"{"b":1,"a":2}"#]);
    let bytes = compile(source, P::CheckedF64Utf16V35).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap()
            .lines,
        logs(&["failed"])
    );
}

#[test]
fn conversions_survive_collection() {
    // Each iteration parses a document, converts its declared fields (built
    // records, arrays and dictionaries are fresh allocations) and drops large
    // garbage strings, so collections run during and between conversions.
    assert_logs_with_stress_fuel(
        "interface Item{id:string;tags?:string[]}\
         interface Doc{items?:Item[];labels?:Record<string,string>;[key:string]:unknown}\
         export function main():void{\
         const kept:Doc[]=[];let total=0;\
         for(let i=0;i<40;i++){\
         const text='{\"items\":[{\"id\":\"a'+String(i)+'\",\"tags\":[\"x\",\"y'+String(i)+'\"]},{\"id\":\"b\"}],\"labels\":{\"k\":\"v'+String(i)+'\"}}';\
         const doc=JSON.parse(text) as Record<string,unknown> as Doc;\
         const junk='z'.repeat(200000);if(junk.length<0){return;}\
         const items=doc.items??[];total+=items.length;\
         const labels=doc.labels;if(labels!==undefined){total+=Object.keys(labels).length;}\
         if(i%8===0){kept.push(doc);}}\
         for(let i=0;i<24;i++){const junk='w'.repeat(300000);if(junk.length<0){return;}}\
         let tags='';for(const doc of kept){if(doc!==undefined){const items=doc.items;\
         if(items!==undefined){const first=items[0];if(first!==undefined){tags+=first.id+':'+String(JSON.stringify(first.tags))+';';}}}}\
         console.log(String(total)+' '+tags);}",
        &[r#"120 a0:["x","y0"];a8:["x","y8"];a16:["x","y16"];a24:["x","y24"];a32:["x","y32"];"#],
    );
}

#[test]
fn v36_rejects_what_it_cannot_convert() {
    for (source, message) in [
        (
            "interface I{id:string}export function main():void{const n:number=1;const x=n as I[];}",
            "neither type sufficiently overlaps",
        ),
        (
            "export function main():void{const d:Record<string,number>={a:1};const {a,...rest}=d;}",
            "rest element of a dictionary binding pattern",
        ),
        (
            "export function main():void{const u:unknown=JSON.parse('{}');\
             if(typeof u==='object'&&u!==null&&'a' in u){const {a}=u;}}",
            "object binding pattern requires a closed record",
        ),
        (
            "interface I{id:string}interface D{items?:I[];[k:string]:unknown}\
             export function main():void{const d:D={};d.items=[{id:1}];}",
            "mismatch",
        ),
        (
            "interface D{items?:{id:string}[];[k:string]:unknown}\
             export function main():void{const p=varta.json.parse<{doc:D}>('{}');}",
            "JSON parsing target type",
        ),
        (
            "function f(s:number):string{switch(s){case 1:return 'a';default:if(s>1){break;}return 'b';}}\
             export function main():void{}",
            "must return on every path",
        ),
    ] {
        let error = rejected(source, V36);
        assert!(error.contains(message), "{source}: {error}");
    }
}

#[test]
fn v35_keeps_rejecting_v36_declared_fields() {
    for (source, message) in [
        (
            "interface I{id:string}interface D{items?:I[];[k:string]:unknown}\
             function f(d:D):void{}export function main():void{}",
            "not checkable from it",
        ),
        (
            "interface D{a?:string;[k:string]:unknown}export function main():void{\
             const r:Record<string,unknown>={};const d=r as D;}",
            "neither type sufficiently overlaps",
        ),
        (
            "export function main():void{let x:unknown;}",
            "uninitialized variable",
        ),
        (
            "export function main():void{const d:Record<string,unknown>={};const {a}=d;}",
            "object binding pattern requires a closed record",
        ),
        (
            "export function main():void{const a:number[]|undefined=undefined;const b=a??[];}",
            "empty array requires an element type annotation",
        ),
        (
            "function f(s:number):string{switch(s){case 1:return 'a';default:return 'b';}}\
             export function main():void{}",
            "must return on every path",
        ),
        (
            "export function main():void{const d:Record<string,unknown>={};\
             const s:string=typeof d.a==='string'?d.a:'';}",
            "mismatch",
        ),
    ] {
        let error = rejected(source, P::CheckedF64Utf16V35);
        assert!(error.contains(message), "{source}: {error}");
    }
}
