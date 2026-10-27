use super::*;

fn run(source: &str) -> RunOutput {
    let bytes = varta_compiler::compile_typescript(source).unwrap();
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
#[test]
fn checked_json_builds_owned_nested_arrays_records_and_all_scalar_types() {
    let output = run(r#"
function decode(text:string):{count:number;empty:null;enabled:boolean;labels:string[];rows:{name:string}[]}{
 return varta.json.parse<{rows:{name:string}[];labels:Array<string>;enabled:boolean;empty:null;count:number}>(text);
}
export function main():void{
 const o=decode("{\"rows\":[{\"name\":\"old\"}],\"labels\":[\"Привіт🦀\",\"\"],\"empty\":null,\"enabled\":false,\"count\":-2147483648}");
 const alias=o;alias.labels[1]="new";alias.rows[0].name="changed";
 console.log(o.count);console.log(o.enabled);console.log(JSON.stringify(o));
 console.log(varta.json.parse<number>("2147483647"));console.log(varta.json.parse<boolean>("true"));
 console.log(JSON.stringify(varta.json.parse<null>("null")));
 console.log(varta.json.parse<string>("\"quoted\\\"\\u0000🦀\""));
 console.log(JSON.stringify(varta.json.parse<number[][]>("[[],[1,2]]")));
 console.log(JSON.stringify(varta.json.parse<{}>("{}")));
 const a=varta.json.parse<number[]>("[]");const b=varta.json.parse<number[]>("[]");console.log(a!==b);
}
"#);
    assert_eq!(
        output.lines,
        [
            "varta log: -2147483648",
            "varta log: 0",
            "varta log: {\"count\":-2147483648,\"empty\":null,\"enabled\":false,\"labels\":[\"Привіт🦀\",\"new\"],\"rows\":[{\"name\":\"changed\"}]}",
            "varta log: 2147483647",
            "varta log: 1",
            "varta log: null",
            "varta log: quoted\"\0🦀",
            "varta log: [[],[1,2]]",
            "varta log: {}",
            "varta log: 1",
        ]
    );
}
#[test]
fn parsed_values_survive_buffer_reuse_nested_allocations_and_collection() {
    let source = r#"
function parse():{rows:{text:string}[]}{return varta.json.parse<{rows:{text:string}[]}>("{\"rows\":[{\"text\":\"saved🦀\"}]}");}
function churn():number{
 for(let i=0;i<400;i=i+1){const o=varta.json.parse<{rows:{text:string}[]}>("{\"rows\":[{\"text\":\"garbage\"}]}");}
 return 0;
}
function first(o:{rows:{text:string}[]},unused:number):{rows:{text:string}[]}{return o;}
export function main():void{
 const o=first(parse(),churn());const saved=o.rows[0];o.rows[0]={text:"changed"+"!"};
 const json=JSON.stringify(o);churn();console.log(json);console.log(saved.text);console.log(o.rows[0].text);
}
"#;
    let output = crate::record_tests::run_collection_stress(source);
    assert_eq!(
        output.lines,
        [
            "varta log: {\"rows\":[{\"text\":\"changed!\"}]}",
            "varta log: saved🦀",
            "varta log: changed!"
        ]
    );
}
#[test]
fn pure_parsing_preserves_system_status_and_tool_effects() {
    let output = run(r#"
export function main():void{
 const denied=varta.result.capture(varta.env.get("DENIED_JSON_PARSE"));
 const o=varta.json.parse<{value:string}>("{\"value\":\"parsed\"}");
 console.log(varta.result.status());console.log(o.value);console.log(denied.status);
}
"#);
    assert!(output.lines.ends_with(&[
        "varta log: 1".into(),
        "varta log: parsed".into(),
        "varta log: 1".into()
    ]));
    let bytes = varta_compiler::compile_typescript(
        r#"
function text():string{return varta.env.get("JSON_INPUT");}
export function encode():string{return JSON.stringify(varta.json.parse<{n:number}>(text()));}
export function main():void{}
varta.tool({name:"encode",description:"Encode"},encode);
"#,
    )
    .unwrap();
    let (_, catalog) = VartaRuntime::new()
        .validate_module_and_tools(&bytes)
        .unwrap();
    assert_eq!(catalog.unwrap().tools[0].required_effects, ["env.get"]);
}
#[test]
fn malformed_or_wrong_shaped_input_aborts_invocation_without_poisoning_next_call() {
    let bytes = varta_compiler::compile_typescript(r#"
export function decode(text:string):string{return JSON.stringify(varta.json.parse<{n:number;values:string[]}>(text));}
export function main():void{}
varta.tool({name:"decode",description:"Decode"},decode);
"#).unwrap();
    let runtime = VartaRuntime::new();
    for json in [
        "{",
        "{}",
        r#"{"n":1,"values":[],"extra":0}"#,
        r#"{"n":1,"n":2,"values":[]}"#,
        r#"{"n":1.0,"values":[]}"#,
        r#"{"n":1,"values":[false]}"#,
    ] {
        let arguments = serde_json::to_vec(&serde_json::json!({"text":json})).unwrap();
        let error = runtime
            .invoke_tool(&bytes, "decode", &arguments, &[])
            .unwrap_err();
        assert!(error.to_string().contains("checked JSON"), "{error}");
    }
    let arguments =
        serde_json::to_vec(&serde_json::json!({"text":r#"{"values":["ok"],"n":2}"#})).unwrap();
    let result = runtime
        .invoke_tool(&bytes, "decode", &arguments, &[])
        .unwrap();
    assert_eq!(
        result.result.value,
        varta_common::ToolValue::String("{\"n\":2,\"values\":[\"ok\"]}".into())
    );
}
