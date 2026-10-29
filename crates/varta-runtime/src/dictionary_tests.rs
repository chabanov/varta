//! Value15 dictionaries (v34 `index_signatures`) and recursive types
//! (`recursive_types`) through the runtime. Logs of the Node-comparable cases
//! were produced by Node 24 for the same programs and are pinned here.
use super::*;
use varta_common::{language::LanguageProfile as P, value};

const V34: P = P::CheckedF64Utf16V34;

fn compile(source: &str, profile: P) -> Result<Vec<u8>, varta_compiler::CompilerError> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)?;
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    Ok(bytes)
}
fn run(source: &str) -> Result<RunOutput, RuntimeError> {
    let bytes = compile(source, V34).unwrap_or_else(|e| panic!("{e}\n{source}"));
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
fn dictionaries_match_node() {
    assert_logs(
        include_str!("value15_cases/dictionary.ts"),
        &[
            r#"{"1":"one","2":"two","10":"ten","b":"bee","a":"ay!","c":"see"}"#,
            r#"["1","2","10","b","a","c"]"#,
            "a=ay! z=?",
            "true false true",
            "true false",
            "true true",
            r#"[["1","one"],["2","two"],["10","ten"],["a","ay!"],["c","see"]]"#,
            r#"["one","two","ten","ay!","see"]"#,
            r#"{"0":1,"7":1,"x":3,"y":2}"#,
            "7 object",
            r#"{"evens":[2,4],"odds":[1,3]}"#,
            "true 6",
        ],
    );
}

#[test]
fn recursive_types_match_node() {
    assert_logs(
        include_str!("value15_cases/recursive.ts"),
        &[
            r#"4 {"name":"root","children":[{"name":"a","children":[]},{"name":"b","children":[{"name":"c","children":[]}]}]}"#,
            r#"6 {"value":3,"next":{"value":2,"next":{"value":1,"next":{"value":0,"next":null}}}}"#,
            r#"5 {"name":"x","tags":["a",1,true,null],"nested":{"deep":[[{}]]}}"#,
            r#"{"fruit":{"apple":"red","pear":"green"},"plain":"text"}"#,
        ],
    );
}

#[test]
fn dictionaries_and_recursive_types_parse_checked_json() {
    assert_logs(
        include_str!("value15_cases/dictionary_json.ts"),
        &[
            r#"{"2":3,"10":10,"b":2,"a":1} 4"#,
            r#"{"limits":{"cpu":2,"mem":512,"disk":9},"name":"svc"} true"#,
            r#"2 b {"children":[{"children":[],"name":"a"},{"children":[{"children":[],"name":"c"}],"name":"b"}],"name":"r"}"#,
            r#"{"k":[1,"two",{"three":[null,true]}],"e":{},"s":"é"}"#,
            r#"["k","e","s","added"] [false]"#,
            "TypeError JSON value does not match the checked schema",
            "TypeError JSON value does not match the checked schema",
            "TypeError JSON value does not match the checked schema",
        ],
    );
}

#[test]
fn dictionary_entries_survive_collection() {
    assert_logs(
        "export function main():void{\
         const d:{[k:string]:{v:string}}={};\
         for(let i=0;i<24;i++){d['k'+String(i)]={v:'value-'+String(i)};\
         const junk:{v:string}={v:'garbage-'+String(i)};if(junk.v.length<0){return;}}\
         for(let i=0;i<24;i=i+2){delete d['k'+String(i)];}\
         for(let i=0;i<60;i++){const junk:{v:string}={v:'more-'+String(i)};if(junk.v.length<0){return;}}\
         const a=d['k1'];const b=d['k23'];const c=d['k0'];\
         console.log(String(Object.keys(d).length)+' '+(a===undefined?'':a.v)+' '+(b===undefined?'':b.v)+' '+String(c===undefined));}",
        &["12 value-1 value-23 true"],
    );
}

#[test]
fn dictionaries_cross_closures_generators_and_unions() {
    assert_logs(
        "type Bag={[k:string]:number};\
         function* walk(bag:Bag):Generator<string>{for(const k of Object.keys(bag)){if(k!==undefined){yield k;}}}\
         function adder(bag:Bag):(k:string)=>number{return (k:string)=>{bag[k]=(bag[k]??0)+1;const n=bag[k];return n===undefined?0:n;};}\
         export function main():void{\
         const bag:Bag={z:1};const add=adder(bag);add('a');console.log(add('a'));\
         const seen:string[]=[];for(const k of walk(bag)){seen.push(k);}console.log(JSON.stringify(seen));\
         const u:Bag|string[]|null=bag;\
         if(u!==null&&!Array.isArray(u)){console.log(JSON.stringify(u));}\
         console.log(String(u===bag)+' '+typeof u);}",
        &["2", r#"["z","a"]"#, r#"{"z":1,"a":2}"#, "true object"],
    );
}

#[test]
fn dictionary_checked_boundaries_throw_typed_errors() {
    assert_logs(
        "export function main():void{\
         const d:Record<string,number>={};\
         try{d['__proto__']=1;}catch(e){if(e instanceof TypeError){console.log(e.message);}}\
         try{const x=d['toString'];console.log(String(x));}catch(e){if(e instanceof TypeError){console.log(e.message);}}\
         d['constructor']=2;console.log(String(d['constructor'])+' '+String('valueOf' in d));}",
        &[
            "Dictionary __proto__ assignment is unsupported",
            "Inherited record property access is unsupported",
            "2 true",
        ],
    );
}

/// Run `main` with a test-only fuel budget: filling a dictionary to its entry
/// ceiling needs more guest work than the production budget allows.
fn run_with_test_fuel(source: &str) -> Vec<String> {
    const TEST_FUEL: u64 = 400 * varta_common::abi::FUEL;
    let bytes = compile(source, V34).unwrap();
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
    execution.store.set_fuel(TEST_FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    execution.store.data().output.lines.clone()
}

#[test]
fn dictionary_entry_ceiling_is_a_named_bound() {
    let limit = value::MAX_DICTIONARY_ENTRIES;
    // 32 x 32 two-letter keys built by guest slicing: no host call per entry
    // (the host call budget stays a production budget too).
    let letters: String = ('a'..='z').chain('A'..='F').collect();
    assert_eq!(limit, 32 * 32);
    let source = format!(
        "export function main():void{{const d:Record<string,boolean>={{}};const a='{letters}';\
         for(let i=0;i<32;i++){{for(let j=0;j<32;j++){{d[a.slice(i,i+1)+a.slice(j,j+1)]=true;}}}}\
         try{{d['over']=false;}}catch(e){{if(e instanceof RangeError){{console.log(e.message);}}}}\
         const keys=Object.keys(d);console.log(String(keys.length)+' '+String(keys[0])+' '+String(keys[keys.length-1]));}}"
    );
    assert_eq!(
        run_with_test_fuel(&source),
        [
            "varta log: Dictionary entry limit exceeded".to_owned(),
            format!("varta log: {limit} aa FF"),
        ]
    );
    // Production budgets are unchanged: the same program exhausts guest fuel.
    assert!(run(&source).is_err());
    // Checked JSON enforces the same ceiling before any guest allocation.
    let object = |count: usize| {
        format!(
            "{{{}}}",
            (0..count)
                .map(|i| format!("\\\"k{i}\\\":{i}"))
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let parse = |count: usize| {
        format!(
            "export function main():void{{const d=varta.json.parse<Record<string,number>>(\"{}\");console.log(Object.keys(d).length);}}",
            object(count)
        )
    };
    assert_logs(&parse(limit), &[&limit.to_string()]);
    let error = run(&parse(limit + 1)).unwrap_err().to_string();
    assert!(error.contains("dictionary entry limit"), "{error}");
}

#[test]
fn index_signatures_and_recursion_keep_older_profiles_and_bad_shapes_rejected() {
    let signature = "type D={[k:string]:number};export function main():void{const d:D={};console.log(JSON.stringify(d));}";
    let record = "export function main():void{const d:Record<string,number>={};console.log(JSON.stringify(d));}";
    let tree = "interface T{c:T[]}export function main():void{const t:T={c:[]};console.log(JSON.stringify(t));}";
    for source in [signature, record, tree] {
        assert!(compile(source, V34).is_ok(), "{source}");
    }
    for profile in [
        P::CheckedF64Utf16V33,
        P::CheckedF64Utf16V32,
        P::CheckedF64Utf16V31,
    ] {
        assert!(
            rejected(signature, profile).contains("object type member"),
            "{profile:?}"
        );
        let error = rejected(record, profile);
        assert!(
            error.contains("Record with non-literal keys")
                || profile == P::CheckedF64Utf16V31 && error.contains("generic type reference"),
            "{profile:?}: {error}"
        );
        assert!(
            rejected(tree, profile).contains("recursive type declaration"),
            "{profile:?}"
        );
    }
    for (source, message) in [
        (
            "type A=B;type B=A;export function main():void{const a:A=1;}",
            "recursive type declaration",
        ),
        (
            "type A=string|A;export function main():void{const a:A='x';}",
            "direct member of a union",
        ),
        (
            "interface M{id:string;[k:string]:string}export function main():void{const m:M={id:'x'};}",
            "mixed object type",
        ),
        (
            "type N={[k:number]:string};export function main():void{const n:N={};}",
            "index signature key other than string",
        ),
        (
            "type G<T>={v:T;next:G<T>|null};export function main():void{const g:G<number>={v:1,next:null};}",
            "recursive type declaration",
        ),
        (
            "export function main():void{const d:Record<string,number>={};const k:number=1;console.log(d[k]);}",
            "dictionary key must be a narrowed string",
        ),
        (
            "type Nest={[k:string]:Nest};export function main():void{const n:Nest={};const f=()=>Object.keys(n).length;console.log(f());}",
            "recursive type",
        ),
    ] {
        let error = rejected(source, V34);
        assert!(error.contains(message), "{source}: {error}");
    }
    // A recursive record or union reached again through a record field, an
    // array element or a union gets a shallow catalog node, so closures may
    // capture it (v34 cross-stream integration).
    assert_logs(
        "interface T{c:T[]}export function main():void{const t:T={c:[]};const f=()=>t.c.length;console.log(f());}",
        &["0"],
    );
}

#[test]
fn dictionary_layout_is_validated_before_serialization() {
    // A dictionary whose storage descriptor names other fields is rejected.
    let source = "export function main():void{const d:Record<string,number>={a:1};console.log(JSON.stringify(d));}";
    let bytes = compile(source, V34).unwrap();
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime.run_wasm_bytes(&bytes, &[]).unwrap().lines,
        [r#"varta log: {"a":1}"#]
    );
    let key = value::DICTIONARY_VALUES_FIELD.as_bytes();
    let at = bytes
        .windows(key.len())
        .position(|w| w == key)
        .expect("storage field name in the data section");
    let mut corrupt = bytes.clone();
    corrupt[at] = b'w';
    let error = runtime.run_wasm_bytes(&corrupt, &[]).unwrap_err();
    assert!(error.to_string().contains("dictionary"), "{error}");
}
