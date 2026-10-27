use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V6)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
#[test]
fn array_search_distinguishes_same_value_zero_from_strict_number_equality() {
    let output = run(r#"
export function main():void {
 const nan=0/0;const v=[nan,-0,1.25,1/0,-1/0,1.25];
 console.log(v.includes(-(0/0)));console.log(v.indexOf(nan));console.log(v.includes(0));console.log(v.indexOf(0));
 console.log(v.includes(-0));console.log(v.indexOf(-0));console.log(v.indexOf(1.25));console.log(v.indexOf(1.25,3));
 console.log(v.includes(1/0));console.log(v.indexOf(-1/0));console.log(v.includes(99));console.log(v.indexOf(99));
 console.log(v.includes("1.25"));console.log(v.indexOf(true));console.log(v.includes(null));console.log(v.includes());
 const missing:undefined[]=[];missing.push(undefined);console.log(missing.includes());console.log(missing.indexOf());
 const nulls=[null];console.log(nulls.includes(undefined));console.log(nulls.indexOf(null));
 const empty:number[]=[];console.log(empty.includes(nan));console.log(empty.indexOf());
}"#);
    assert_eq!(
        output.lines,
        [
            "1", "-1", "1", "1", "1", "1", "2", "5", "1", "4", "0", "-1", "0", "-1", "0", "0", "1",
            "0", "0", "0", "0", "-1"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn array_search_normalizes_optional_relative_fractional_and_infinite_start_indices() {
    let output = run(r#"
function find(v:number[],index:number|undefined):number{return v.indexOf(1,index);}
export function main():void {
 const v=[1,2,1,3];console.log(v.indexOf(1));console.log(v.indexOf(1,1.9));console.log(v.indexOf(1,-2.9));console.log(v.indexOf(1,-1));
 console.log(v.indexOf(1,-0.9));console.log(v.indexOf(1,0/0));console.log(v.indexOf(1,undefined));console.log(find(v,undefined));console.log(find(v,-2));
 console.log(v.includes(1,1/0));console.log(v.indexOf(1,1/0));console.log(v.indexOf(1,-1/0));console.log(v.indexOf(1,1e300));console.log(v.indexOf(1,-1e300));
 console.log(v.includes(3,-1.9));console.log(v.includes(1,4));console.log(v.indexOf(1,4));
}"#);
    assert_eq!(
        output.lines,
        [
            "0", "2", "2", "-1", "0", "0", "0", "0", "2", "0", "-1", "0", "-1", "0", "1", "0", "-1"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn array_search_compares_utf16_contents_and_underlying_reference_identity() {
    let output = run(r#"
export function main():void {
 const text="Привіт"+"😀";const words=[text,"\uD800","", "a\u0000b"];
 console.log(words.includes("Привіт😀"));console.log(words.indexOf("При"+"віт😀"));console.log(words.indexOf("\uD800"));
 console.log(words.includes("\uD801"));console.log(words.indexOf(""));console.log(words.includes("a\u0000b"));
 const object={n:1.25};const alias=object;const objects=[object];console.log(objects.includes(alias));console.log(objects.indexOf({n:1.25}));
 console.log(objects.includes({other:1.25}));console.log(objects.includes([1.25]));object.n=2.5;console.log(objects.indexOf(alias));
 const nested=[1.25];const arrays=[nested];nested.push(2.5);console.log(arrays.includes(nested));console.log(arrays.indexOf([1.25,2.5]));
 const values:(number|string|boolean|null|undefined|{n:number}|number[])[]=[object,nested,0/0,-0,"😀",false,null,undefined];
 console.log(values.includes(object));console.log(values.indexOf(nested));console.log(values.includes(0/0));console.log(values.indexOf(0/0));
 console.log(values.indexOf(0));console.log(values.indexOf("😀"));console.log(values.indexOf(false));console.log(values.indexOf(null));console.log(values.indexOf(undefined));
 const needle:{n:number}|undefined=object;console.log(values.indexOf(needle));const n:number|string=0;console.log(values.indexOf(n));
 console.log(objects.includes(varta.json.parse<{n:number}>('{"n":2.5}')));
}"#);
    assert_eq!(
        output.lines,
        [
            "1", "0", "1", "0", "2", "1", "1", "-1", "0", "0", "0", "1", "-1", "1", "1", "1", "-1",
            "3", "4", "5", "6", "7", "0", "3", "0"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn array_search_evaluates_receiver_needle_and_start_once_before_reading_current_storage() {
    let output = run(r#"
function receiver(v:number[]):number[]{console.log("receiver");return v;}
function needle(v:number[]):number{console.log("needle");v.push(3);return 3;}
function start(v:number[]):number|undefined{console.log("start");v.push(4);return undefined;}
export function main():void {
 const v=[1,2];console.log(receiver(v).indexOf(needle(v),start(v)));console.log(JSON.stringify(v));
 const empty:number[]=[];console.log(receiver(empty).includes(void console.log("value"),void console.log("from")));
 console.log(v.includes(void v.pop(),void v.push(9)));console.log(JSON.stringify(v));
}"#);
    assert_eq!(
        output.lines,
        [
            "receiver",
            "needle",
            "start",
            "2",
            "[1,2,3,4]",
            "receiver",
            "value",
            "from",
            "0",
            "0",
            "[1,2,3,9]"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn array_search_rejects_unsupported_bounds_receivers_and_values_without_changing_old_profiles() {
    for body in [
        "const v=[1];v.includes(1,'0');",
        "const v=[1];v.indexOf(1,null);",
        "const v=[1];v.includes(1,true);",
        "const v=[1];v.indexOf(1,0,0);",
        "const v=[1];v.includes(...v);",
        "const v:number[]|undefined=[];v.includes(1);",
        "'text'.includes('t');",
        "const v=[1];const f=v.indexOf;",
        "const v=[1];v.includes(varta.result.capture(varta.env.get('X')));",
    ] {
        let source = format!("export function main():void{{{body}}}");
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &source,
                LanguageProfile::CheckedF64Utf16V6
            )
            .is_err(),
            "{body}"
        );
    }
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf16V4,
        LanguageProfile::CheckedF64Utf16V5,
    ] {
        for method in ["includes(1)", "indexOf(1)"] {
            let source = format!("export function main():void{{[1].{method};}}");
            assert!(varta_compiler::compile_typescript_with_profile(&source, profile).is_err());
        }
    }
    let output = run(
        "export function main():void{const v:number[]=[];for(let i=0;i<1024;i=i+1){v.push(i);}console.log(v.indexOf(1023));console.log(v.includes(1023,1023));console.log(v.indexOf(1023,1024));console.log(v.indexOf(1023,-1));}",
    );
    assert_eq!(
        output.lines,
        ["1023", "1", "-1", "1023"].map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn array_search_retains_detached_needles_and_aggregate_temporaries_through_argument_collection() {
    let source = r#"
function churn():number {
 let text="x";for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<400;i=i+1){const v=[text+"!",text+"?"];const copy=v.slice();copy.pop();}return 0;
}
function needle(v:{label:string}[]):{label:string}|undefined{const value=v.pop();return value;}
function start(v:{label:string}[]):number{churn();v.push({label:"new"});return 0;}
function numbers():number[]{const v:number[]=[];for(let i=0;i<42;i=i+1){v.push(2.775671592749e-309);}return v;}
export function main():void {
 const v:{label:string}[]=[];for(let i=0;i<42;i=i+1){v.push({label:"kept😀"});}const first=v.at(0);
 console.log(v.includes(needle(v),start(v)));if(first!==undefined){console.log(first.label);}console.log(v.indexOf(undefined));
 const strings=["kept"+"😀"];const haystack=["kept😀"];console.log(haystack.includes(strings.pop(),churn()));console.log(strings.length);
 console.log(numbers().includes(2.775671592749e-309,churn()));
 const rows=v.slice();console.log(rows.indexOf(rows.at(-1),churn()));
}"#;
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(300 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    assert_eq!(
        execution.store.data().output.lines,
        ["0", "kept😀", "-1", "1", "0", "1", "41"].map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn array_search_is_pure_and_uses_the_complete_ten_helper_dependency_set() {
    let source = r#"
export function main():void {
 const denied=varta.result.capture(varta.env.get("VARTA_ARRAY_SEARCH_DENIED"));const v:(number|string)[]=[1.25,"😀"];
 const copied=v.slice();copied.push("kept"+"!");copied[0]=2.5;console.log(copied.includes("kept!"));console.log(copied.indexOf("😀"));
 console.log(varta.result.status());console.log(denied.status);console.log(JSON.stringify(copied));
}"#;
    let bytes = compile(source);
    let functions = wasmparser::Parser::new(0)
        .parse_all(&bytes)
        .find_map(|payload| match payload.unwrap() {
            wasmparser::Payload::FunctionSection(section) => Some(section.count() as usize),
            _ => None,
        })
        .unwrap();
    assert_eq!(functions, 1 + abi::LEGACY_INTERNAL_FUNCTIONS);
    let output = VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap();
    assert!(output.lines.ends_with(
        &["1", "1", "1", "1", r#"[2.5,"😀","kept!"]"#].map(|s| format!("varta log: {s}"))
    ));
}
