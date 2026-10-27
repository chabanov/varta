use super::*;

fn run(source: &str) -> RunOutput {
    let wasm = varta_compiler::compile_typescript(source).unwrap();
    wasmtime::Module::new(&wasmtime::Engine::default(), &wasm).unwrap();
    VartaRuntime::new().run_wasm_bytes(&wasm, &[]).unwrap()
}

pub(super) fn run_collection_stress(source: &str) -> RunOutput {
    let wasm = varta_compiler::compile_typescript(source).unwrap();
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&wasm).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    // Repeated collections use a larger test-only budget, like the existing
    // reclamation harness. Production invocations keep their fixed ceiling.
    execution.store.set_fuel(abi::FUEL * 8).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
    assert!(
        memory.data_size(&execution.store) <= abi::OWNED_STRING_BASE + 2 * abi::WASM_PAGE_BYTES
    );
    std::mem::take(&mut execution.store.data_mut().output)
}

#[test]
fn objects_preserve_aliases_nested_fields_structural_types_and_json_values() {
    let output = run(r#"
function update(o:{ count:number; text:string; nested:{flag:boolean;empty:null} }):{text:string;nested:{empty:null;flag:boolean};count:number}{
 const alias=o;alias.count=alias.count+1;alias.text=alias.text+"!";return o;
}
export function main():void{
 const a={text:"Привіт🦀",count:1,nested:{empty:null,flag:true}};
 const b=update(a);console.log(a===b);console.log(a!==b);
 console.log(a.count);console.log(b.text);console.log(a.nested.flag);
 console.log(JSON.stringify(a));console.log(JSON.stringify({}));
 console.log(JSON.stringify(null));console.log(JSON.stringify(false));console.log(JSON.stringify(-7));
 console.log(JSON.stringify("quotes\"\\\n\u0000🦀"));
}
"#);
    assert_eq!(
        output.lines,
        [
            "varta log: 1",
            "varta log: 0",
            "varta log: 2",
            "varta log: Привіт🦀!",
            "varta log: 1",
            "varta log: {\"text\":\"Привіт🦀!\",\"count\":2,\"nested\":{\"empty\":null,\"flag\":true}}",
            "varta log: {}",
            "varta log: null",
            "varta log: false",
            "varta log: -7",
            "varta log: \"quotes\\\"\\\\\\n\\u0000🦀\""
        ]
    );
}

#[test]
fn object_initializers_assignments_and_key_enumeration_keep_js_order() {
    let output = run(r#"
function mark(n:number):number{console.log(n);return n;}
function base(o:{text:string}):{text:string}{console.log("base");return o;}
function rhs():string{console.log("rhs");return "after";}
export function main():void{
 const x={"2":mark(1),"1":mark(2),z:mark(3),a:mark(4),"01":mark(5)};
 console.log(JSON.stringify(x));
 const o={text:"before"};base(o).text=rhs();console.log(o["text"]);
 const same={a:1,b:true};const reordered:{b:boolean;a:number}={b:true,a:1};
 console.log(JSON.stringify(same));console.log(JSON.stringify(reordered));
 const value="short";console.log(JSON.stringify({value}));
}
"#);
    assert_eq!(
        output.lines,
        [
            "varta log: 1",
            "varta log: 2",
            "varta log: 3",
            "varta log: 4",
            "varta log: 5",
            "varta log: {\"1\":2,\"2\":1,\"z\":3,\"a\":4,\"01\":5}",
            "varta log: base",
            "varta log: rhs",
            "varta log: after",
            "varta log: {\"a\":1,\"b\":true}",
            "varta log: {\"b\":true,\"a\":1}",
            "varta log: {\"value\":\"short\"}"
        ]
    );
}

#[test]
fn collection_traces_nested_objects_strings_and_replaced_edges() {
    let output = run_collection_stress(
        r#"
function build():{node:{text:string;enabled:boolean};label:string}{
 const text="saved"+"🦀";return {node:{text,enabled:true},label:"label"+"!"};
}
function churn():void{
 for(let i=0;i<650;i=i+1){const dead={node:{text:"temporary"+"!",enabled:false},label:"garbage"};}
}
export function main():void{
 const saved=build();const alias=saved;churn();
 console.log(JSON.stringify(alias));const old=saved.node;
 saved.node={enabled:false,text:"changed"+"!"};churn();
 console.log(old.text);console.log(alias.node.text);churn();console.log(saved.label);
}
"#,
    );
    assert_eq!(
        output.lines,
        [
            "varta log: {\"node\":{\"text\":\"saved🦀\",\"enabled\":true},\"label\":\"label!\"}",
            "varta log: saved🦀",
            "varta log: changed!",
            "varta log: label!"
        ]
    );
}

#[test]
fn json_is_unprivileged_preserves_host_status_and_counts_encoded_output() {
    let output = run(r#"
export function main():void{
 const r=varta.result.capture(varta.env.get("DENIED_JSON_INPUT"));
 const value=JSON.stringify({status:r.status,value:r.value});
 console.log(varta.result.status());console.log(value);console.log(r.status);
}

"#);
    assert!(
        output
            .lines
            .iter()
            .any(|line| line == "varta log: {\"status\":1,\"value\":\"\"}")
    );
    assert_eq!(
        output
            .lines
            .iter()
            .filter(|line| *line == "varta log: 1")
            .count(),
        2
    );
    let wasm = varta_compiler::compile_typescript(
        r#"
export function encode(value:string):string{
 let text=value;for(let i=0;i<6;i=i+1){text=text+text;}return JSON.stringify({text});
}
export function main():void{}
varta.tool({name:"encode",description:"Encode"},encode);
"#,
    )
    .unwrap();
    let runtime = VartaRuntime::new();
    let plain = serde_json::to_vec(&serde_json::json!({"value":"x".repeat(2048)})).unwrap();
    assert!(runtime.invoke_tool(&wasm, "encode", &plain, &[]).is_ok());
    let escaped = serde_json::to_vec(&serde_json::json!({"value":"\0".repeat(4095)})).unwrap();
    let error = runtime
        .invoke_tool(&wasm, "encode", &escaped, &[])
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("JSON serialization: output byte limit"),
        "{error}"
    );
}

#[test]
fn json_import_requires_versioned_metadata_and_result_globals() {
    use crate::contract_tests::rewrite;
    use varta_common::value::{VALUE_SECTION, VALUE_VERSION};
    let bytes = varta_compiler::compile_typescript(
        "export function main():void{console.log(JSON.stringify({x:1}));}",
    )
    .unwrap();
    let runtime = VartaRuntime::new();
    for action in 0..5 {
        let bad = rewrite(&bytes, |id, section| {
            let mut data = section.to_vec();
            if id == 0 && section.get(1..1 + VALUE_SECTION.len()) == Some(VALUE_SECTION.as_bytes())
            {
                match action {
                    0 => return vec![],
                    1 => return vec![data.clone(), data],
                    2 => {
                        data.truncate(1 + VALUE_SECTION.len());
                        data.extend((VALUE_VERSION + 1).to_le_bytes());
                    }
                    _ => {}
                }
            }
            if action == 3 && id == 7 {
                let name = abi::RESULT_LEN.as_bytes();
                let offset = data.windows(name.len()).position(|s| s == name).unwrap();
                data[offset] = b'x';
            }
            if action == 4 && id == 2 {
                let name = varta_common::HostOperation::JsonStringify.name().as_bytes();
                let offset = data.windows(name.len()).position(|s| s == name).unwrap();
                // Bind the three-argument import to the two-argument log ABI.
                let mut changed = data[..offset - 1].to_vec();
                let log = varta_common::HostOperation::LogString.name().as_bytes();
                changed.push(log.len() as u8);
                changed.extend(log);
                changed.extend(&data[offset + name.len()..]);
                data = changed;
            }
            vec![data]
        });
        let error = runtime.validate_module(&bad).unwrap_err();
        assert!(
            error.to_string().contains(match action {
                0 => "requires structured value ABI",
                1 | 2 => "structured value ABI",
                3 => "result globals",
                _ => "invalid host signature",
            }),
            "mutation {action}: {error}"
        );
    }
}

#[test]
fn record_field_and_json_expressions_keep_transitive_tool_effects() {
    let bytes = varta_compiler::compile_typescript(
        r#"
function holder():{text:string}{return {text:varta.env.get("NAME")};}
export function encode():string{
 const o={nested:holder(),flag:varta.env.has("NAME")};
 o.nested.text=varta.fs.read("/not-granted");
 return JSON.stringify({value:o.nested.text,clock:varta.time.now(),flag:o.flag});
}
export function main():void{}
varta.tool({name:"encode",description:"Encode"},encode);
"#,
    )
    .unwrap();
    let (_, catalog) = VartaRuntime::new()
        .validate_module_and_tools(&bytes)
        .unwrap();
    assert_eq!(
        catalog.unwrap().tools[0].required_effects,
        ["env.get", "env.has", "fs.read", "time.now"]
    );
}

#[test]
fn collection_retains_all_reference_mask_bits_and_pending_object_arguments() {
    let fields = (0..varta_common::value::MAX_RECORD_FIELDS)
        .map(|i| format!("k{i}:\"saved\"+\"{i}\""))
        .collect::<Vec<_>>()
        .join(",");
    let annotations = (0..varta_common::value::MAX_RECORD_FIELDS)
        .map(|i| format!("k{i}:string"))
        .collect::<Vec<_>>()
        .join(";");
    let source = format!(
        r#"
function make():{{{annotations}}}{{return {{{fields}}};}}
function pick(o:{{{annotations}}},unused:number):{{{annotations}}}{{return o;}}
function churn():number{{
 for(let i=0;i<650;i=i+1){{const dead={{text:"temporary"+"!",nested:{{value:i}}}};}}
 return 0;
}}
export function main():void{{
 const saved=make();const other=pick(make(),churn());
 console.log(saved.k0);console.log(saved.k31);console.log(other.k31);
 console.log(saved!==other);const alias=saved;console.log(saved===alias);
}}
"#
    );
    let output = run_collection_stress(&source);
    assert_eq!(
        output.lines,
        [
            "varta log: saved0",
            "varta log: saved31",
            "varta log: saved31",
            "varta log: 1",
            "varta log: 1"
        ]
    );
}
