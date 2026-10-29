//! Mixed index records and dictionary completeness (v35 `mixed_index_records`)
//! through the runtime. Logs of the Node-comparable cases were produced by
//! Node 24 for the same programs (`v35_cases`) and are pinned here; Varta's
//! deliberate deviations (checked declared reads) are pinned separately.
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
fn mixed_records_match_node() {
    assert_logs(
        include_str!("v35_cases/mixed.ts"),
        &[
            r#"{"2":"two","type":"about:blank","status":404,"extra":true,"title":"Not Found","trace":[1,2]}"#,
            r#"Not Found/404"#,
            r#"["2","type","status","extra","title","trace"] ["2","type","status","extra","title","trace"]"#,
            r#"true false true"#,
            r#"boolean"#,
            r#"{"2":"two","type":"about:blank","title":"Not Found","trace":[1,2]} Not Found/-"#,
            r#"Not Found"#,
            r#"x2 L {"label":"L","id":"x2","other":"o","dyn":"d"}"#,
            r#"{"size":4,"kind":"box","color":"red"} box red"#,
            r#"{"1":"one","name":"n"} n [["1","one"],["name","n"]]"#,
        ],
    );
}

#[test]
fn dictionary_iteration_and_updates_match_node() {
    assert_logs(
        include_str!("v35_cases/dictionary_ops.ts"),
        &[
            r#"3=3;b=1;a=2;"#,
            r#"b"#,
            r#"{"3":3,"b":5,"a":12,"c":7}"#,
            r#"{"b":2,"c":7} 8"#,
            r#"{"greet":"hello world","empty":"filled"}"#,
            r#"y,x,y;x;label;"#,
            r#"abd {"a":1,"b":2,"d":4,"e":5}"#,
            r#"total:6 name:s extra:three 6"#,
        ],
    );
}

#[test]
fn spreads_inference_and_number_keys_match_node() {
    assert_logs(
        include_str!("v35_cases/spread.ts"),
        &[
            r#"{"5":5,"b":1,"a":20,"c":30,"total":0}"#,
            r#"{"y":2,"x":1,"z":3}"#,
            r#"{"label":"here","y":2,"x":1} ["label","y","x"]"#,
            r#"{"1":1,"mode":"slow","retries":3} slow"#,
            r#"["b","a"] ["0","2","10","x","1.5"]"#,
            r#"why 0"#,
            r#"{"0":"zero","2":"two","10":"ten","x":"ex","1.5":"one and a half"}"#,
            r#"two one and a half zero true false"#,
            r#"{"0":"zero","2":"two","x":"ex","1.5":"one and a half"}"#,
            r#"{"1":11,"3":2} true"#,
        ],
    );
}

#[test]
fn absent_entries_update_like_javascript() {
    // Node 24: an absent entry reads undefined, so += and ++ store NaN and a
    // string += concatenates "undefined".
    assert_logs(
        "export function main():void{const d:Record<string,number>={};\
         d['a']+=1;console.log(JSON.stringify(d)+' '+String(d['a']));\
         d['b']++;console.log(String(d['b']));\
         const s:Record<string,string>={};s['x']+='y';console.log(String(s['x']));}",
        &[r#"{"a":null} NaN"#, "NaN", "undefinedy"],
    );
}

#[test]
fn optional_and_redeclared_class_fields_match_node() {
    // A redeclared field is defined again after super() returns (ES2022
    // class fields), so `cause?: unknown` reads undefined, as in Node.
    assert_logs(
        include_str!("v35_cases/class_fields.ts"),
        &[
            r#"UarpError failed undefined undefined"#,
            r#"42 true"#,
            r#"child undefined"#,
            r#"2"#,
        ],
    );
    assert_logs(
        "class Base extends Error{constructor(m:string,o?:{cause?:unknown}){super(m,o);}}\
         class Keep extends Base{declare readonly cause?:unknown;}\
         export function main():void{const k=new Keep('m',{cause:'kept'});console.log(String(k.cause));}",
        &["kept"],
    );
}

#[test]
fn declared_reads_are_checked_against_dynamic_writes() {
    // Deviation from TypeScript's erased types: a dynamic write or an alias of
    // the plain dictionary may store any index-type value under a declared
    // key, so a declared read tests the entry instead of reinterpreting it.
    assert_logs(
        "interface P{status?:number;title:string;[k:string]:unknown}\
         export function main():void{\
         const p:P={title:'t',status:1};const plain:Record<string,unknown>=p;\
         plain['status']='broken';\
         try{console.log(String(p.status));}catch(e){if(e instanceof TypeError){console.log(e.message);}}\
         const key:string='title';delete p[key];\
         try{console.log(p.title);}catch(e){if(e instanceof TypeError){console.log(e.message);}}\
         plain['status']=undefined;console.log(String(p.status)+' '+JSON.stringify(p));}",
        &[
            "property status does not hold its declared type",
            "declared property title is missing",
            "undefined {}",
        ],
    );
}

#[test]
fn mixed_entries_survive_collection() {
    assert_logs(
        "interface Bag{name:string;[k:string]:string|{v:string}}\
         export function main():void{\
         const bag:Bag={name:'n'};\
         for(let i=0;i<24;i++){bag['k'+String(i)]={v:'value-'+String(i)};\
         const junk:{v:string}={v:'garbage-'+String(i)};if(junk.v.length<0){return;}}\
         for(const k in bag){if(k.length===3){delete bag[k];}}\
         for(let i=0;i<60;i++){const junk:{v:string}={v:'more-'+String(i)};if(junk.v.length<0){return;}}\
         const copy:Bag={...bag,extra:'e'};\
         for(let i=0;i<60;i++){const junk:{v:string}={v:'again-'+String(i)};if(junk.v.length<0){return;}}\
         const k3=copy['k3'];\
         console.log(String(Object.keys(copy).length)+' '+copy.name+' '+(typeof k3==='object'?k3.v:'?'));}",
        &["12 n value-3"],
    );
}

#[test]
fn shared_dictionary_operations_keep_function_bodies_small() {
    // V34 inlines every dictionary write; v35 calls one shared function per
    // dictionary type, so a long run of writes fits the body budget.
    let writes: String = (0..60).map(|i| format!("d['k{i}']={i};")).collect();
    let source = format!(
        "export function main():void{{const d:Record<string,number>={{}};{writes}\
         console.log(String(Object.keys(d).length)+' '+String(d['k59']));}}"
    );
    assert_logs(&source, &["60 59"]);
    let v34 = rejected(&source, P::CheckedF64Utf16V34);
    assert!(
        v34.contains("function body exceeds compilation budget"),
        "{v34}"
    );
}

#[test]
fn mixed_records_reject_unsound_or_unsupported_shapes() {
    for (source, message) in [
        (
            "interface P{a:string;[k:string]:number}function f(p:P):void{}export function main():void{}",
            "not assignable to the string index type",
        ),
        (
            // TS2411: the `undefined` of an optional property counts.
            "interface P{a?:string;[k:string]:string}function f(p:P):void{}export function main():void{}",
            "not assignable to the string index type",
        ),
        (
            "interface I{id:number}interface P{items:I[];[k:string]:unknown}function f(p:P):void{}export function main():void{}",
            "not checkable from it",
        ),
        (
            "type P={a:string;[k:string]:string;[j:string]:string};function f(p:P):void{}export function main():void{}",
            "more than one index signature",
        ),
        (
            "interface A{[k:string]:unknown}interface B extends A{[k:string]:string}function f(b:B):void{}export function main():void{}",
            "index signatures of different value types",
        ),
        (
            "interface P{a:string;[k:string]:string}export function main():void{const p:P={b:'x'};}",
            "missing the required declared property a",
        ),
        (
            "interface P{a:string;[k:string]:string}export function main():void{const p:P={a:'x'};delete p.a;}",
            "the operand of delete must be optional",
        ),
        (
            "interface P{a:string;[k:string]:string}export function main():void{const p:P={a:'x'};p.a=1;}",
            "dictionary value type mismatch",
        ),
        (
            "export function main():void{const d:Record<string,number>={};const e:Record<string,string>={...d};}",
            "is not assignable to the dictionary's value type",
        ),
        (
            "export function main():void{const a:number[]=[1];const e:Record<string,number>={...a};}",
            "object spread of a array",
        ),
        (
            "export function main():void{const a:number[]=[1];for(const k in a){}}",
            "for-in requires a dictionary or closed record",
        ),
        (
            "export function main():void{const d:Record<string,number>={};const b:boolean=true;console.log(d[b]);}",
            "dictionary key must be a narrowed string",
        ),
        (
            "class A{private x:number=1;}class B extends A{x:number=2;}export function main():void{}",
            "redeclares a private inherited field",
        ),
        (
            "class A{x:number|string=1;}class B extends A{x:number=2;}export function main():void{}",
            "a redeclaration must keep the inherited type",
        ),
        (
            "class A{declare x:number;}export function main():void{}",
            "ambient class field",
        ),
    ] {
        let error = rejected(source, V35);
        assert!(error.contains(message), "{source}: {error}");
    }
}

#[test]
fn earlier_profiles_keep_rejecting_v35_records() {
    for (source, message) in [
        (
            "interface P{a:string;[k:string]:string}function f(p:P):void{}export function main():void{}",
            "mixed object type",
        ),
        (
            "export function main():void{const d:Record<string,number>={};for(const k in d){}}",
            "unsupported statement",
        ),
        (
            "export function main():void{const d:Record<string,number>={};d['a']+=1;}",
            "assignment to immutable result property",
        ),
        (
            "export function main():void{const d:Record<string,number>={};const e:Record<string,number>={...d};}",
            "object spread",
        ),
        (
            "export function main():void{const d:Record<string,number>={};d[1]=2;}",
            "dictionary key must be a narrowed string",
        ),
        (
            "class A{x?:number;}export function main():void{}",
            "optional class field",
        ),
        (
            "function k<T>(d:Record<string,T>):number{return Object.keys(d).length;}\
             export function main():void{console.log(k({a:1}));}",
            "cannot infer type argument",
        ),
    ] {
        let error = rejected(source, P::CheckedF64Utf16V34);
        assert!(error.contains(message), "{source}: {error}");
    }
}
