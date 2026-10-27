use super::*;
use varta_common::{language::LanguageProfile, value};
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf8V1)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let wasm = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &wasm).unwrap();
    VartaRuntime::new().run_wasm_bytes(&wasm, &[]).unwrap()
}

#[test]
fn binary64_records_arrays_mutations_and_aliases_preserve_complete_slots() {
    let output = run(r#"
function make(n:number):{n:number;items:number[];nested:{label:string;zero:number}} {
 return {n,items:[0.5,1.5,2.5],nested:{label:"kept"+"!",zero:-0}};
}
export function main():void {
 const object=make(0.5);const alias=object;alias.n=alias.n+1.25;object.items[1]=-0;
 object.nested.zero=-0;console.log(1/object.nested.zero);console.log(1/object.items[1]);
 console.log(object.items.length/2);let sum=0;for(let i=0;i<object.items.length;i=i+1){sum=sum+object.items[i];}console.log(sum);
 console.log(JSON.stringify(object));console.log(object.items[-0]);
 const empty:number[]=[];console.log(JSON.stringify(empty));
}"#);
    assert_eq!(
        &output.lines[..4],
        [
            "varta log: -Infinity",
            "varta log: -Infinity",
            "varta log: 1.5",
            "varta log: 3"
        ]
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            output.lines[4].strip_prefix("varta log: ").unwrap()
        )
        .unwrap(),
        serde_json::json!({"n":1.75,"items":[0.5,0,2.5],"nested":{"label":"kept!","zero":0}})
    );
    assert_eq!(&output.lines[5..], ["varta log: 0.5", "varta log: []"]);
}

#[test]
fn binary64_json_uses_ecmascript_number_formatting_and_nonfinite_nulls() {
    let output = run(r#"export function main():void {
 console.log(JSON.stringify([1,1e20,1e21,1e-6,1e-7,0.1+0.2,9007199254740993,-0,NaN,Infinity,-Infinity,5e-324,1.7976931348623157e308]));
 console.log(JSON.stringify(-0));console.log(JSON.stringify(NaN));
}"#);
    assert_eq!(
        output.lines,
        [
            "varta log: [1,100000000000000000000,1e+21,0.000001,1e-7,0.30000000000000004,9007199254740992,0,null,null,null,5e-324,1.7976931348623157e+308]",
            "varta log: 0",
            "varta log: null"
        ]
    );
}

#[test]
fn binary64_checked_json_returns_full_bits_and_owned_nested_values() {
    let output = run(r#"
function parse(text:string):{n:number;items:{value:number;label:string}[];empty:null} {
 return varta.json.parse<{n:number;items:{value:number;label:string}[];empty:null}>(text);
}
export function main():void {
 const result=parse('{"n":-0,"items":[{"value":1.25,"label":"kept"}],"empty":null}');
 console.log(1/result.n);result.items[0].value=result.items[0].value+0.5;console.log(JSON.stringify(result));
 console.log(1/varta.json.parse<number>('-0'));
 console.log(varta.json.parse<number>('9007199254740993'));
 console.log(varta.json.parse<number>('5e-324')===5e-324);
 console.log(varta.json.parse<number>('1.0000000000000002')===1.0000000000000002);
 console.log(varta.json.parse<string>('"Привіт🦀"'));console.log(varta.json.parse<string>('""'));
 console.log(varta.json.parse<boolean>('false'));console.log(JSON.stringify(varta.json.parse<null>('null')));
}"#);
    assert_eq!(output.lines[0], "varta log: -Infinity");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            output.lines[1].strip_prefix("varta log: ").unwrap()
        )
        .unwrap(),
        serde_json::json!({"n":0,"items":[{"value":1.75,"label":"kept"}],"empty":null})
    );
    assert_eq!(
        &output.lines[2..],
        [
            "varta log: -Infinity",
            "varta log: 9007199254740992",
            "varta log: 1",
            "varta log: 1",
            "varta log: Привіт🦀",
            "varta log: ",
            "varta log: 0",
            "varta log: null"
        ]
    );
}

#[test]
fn binary64_array_indices_are_checked_before_integer_conversion_or_access() {
    for index in [
        "-1",
        "0.5",
        "NaN",
        "Infinity",
        "-Infinity",
        "4294967296",
        "2",
    ] {
        for write in [false, true] {
            let expression = if write {
                format!("a[{index}]=9.5;")
            } else {
                format!("console.log(a[{index}]);")
            };
            let wasm = compile(&format!(
                "export function main():void{{const a=[1.5,2.5];{expression}}}"
            ));
            assert!(
                VartaRuntime::new().run_wasm_bytes(&wasm, &[]).is_err(),
                "{expression}"
            );
        }
    }
}

#[test]
fn binary64_checked_json_rejects_invalid_input_before_guest_allocation() {
    for json in [
        r#"{"n":1e400,"text":"value"}"#,
        r#"{"n":1.25,"n":2,"text":"value"}"#,
        r#"{"n":1.25,"text":"value","other":0}"#,
        r#"{"n":true,"text":"value"}"#,
        r#"{"n":1.25}"#,
    ] {
        let source = format!(
            "export function main():void{{varta.json.parse<{{n:number;text:string}}>({});}}",
            serde_json::to_string(json).unwrap()
        );
        let runtime = VartaRuntime::new();
        let module = runtime.validate_module(&compile(&source)).unwrap();
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
        let before = memory.data_size(&execution.store);
        let error = instance
            .get_typed_func::<(), ()>(&mut execution.store, "_start")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap_err();
        assert!(format!("{error:#}").contains("checked JSON"));
        assert_eq!(memory.data_size(&execution.store), before);
        assert!(
            memory.data(&execution.store)[abi::OWNED_STRING_BASE..]
                .iter()
                .all(|byte| *byte == 0)
        );
    }
}

#[test]
fn binary64_value_versions_and_json_helpers_are_validated_before_execution() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        r#"
function fakeAlloc(n:boolean):boolean{varta.env.has("SECRET");return n;}
function fakeRetain(s:string):string{varta.env.get("SECRET");return s;}
export function main():void{console.log(JSON.stringify(varta.json.parse<{n:number;text:string}>('{"n":1.5,"text":"kept"}')));}
"#,
    );
    let runtime = VartaRuntime::new();
    for version in [value::VALUE_VERSION, u32::MAX] {
        let changed = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 0
                && data.get(1..1 + value::VALUE_SECTION.len())
                    == Some(value::VALUE_SECTION.as_bytes())
            {
                data.truncate(1 + value::VALUE_SECTION.len());
                data.extend(version.to_le_bytes());
            }
            vec![data]
        });
        assert!(runtime.validate_module(&changed).is_err());
    }
    for action in 0..2 {
        let changed = rewrite(&bytes, |id, data| {
            if id == 0
                && data.get(1..1 + value::VALUE_SECTION.len())
                    == Some(value::VALUE_SECTION.as_bytes())
            {
                if action == 0 {
                    return vec![];
                }
                return vec![data.to_vec(), data.to_vec()];
            }
            vec![data.to_vec()]
        });
        assert!(runtime.validate_module(&changed).is_err());
    }
    let start = export_index(&bytes, "_start");
    for (name, index) in [
        (value::VALUE_ALLOCATE, start - 2),
        (value::VALUE_RETAIN, start - 1),
    ] {
        let changed = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 7 {
                let at = data
                    .windows(name.len())
                    .position(|s| s == name.as_bytes())
                    .unwrap();
                data[at + name.len() + 1] = index;
            }
            vec![data]
        });
        let error = runtime.validate_module(&changed).unwrap_err();
        assert!(
            error.to_string().contains("cannot call host imports"),
            "{name}: {error}"
        );
    }
}

#[test]
fn binary64_aggregate_tracing_preserves_reference_edges_beyond_32_and_numeric_bits() {
    let members = (0..40)
        .map(|n| format!(r#"{{n:{n}.5,label:"saved"+"{n}"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let fake_pointer = f64::from_bits(abi::OWNED_STRING_BASE as u64);
    let source = format!(
        r#"
function churn():void{{const piece={:?};for(let i=0;i<500;i=i+1){{const dead={{n:i+0.5,label:piece+"temporary",arr:[NaN,-0,i]}};}}}}
export function main():void{{
 const values=[{members}];const box={{items:values,fake:{fake_pointer:?},zero:-0,nan:NaN}};
 const parsed=varta.json.parse<{{n:number;labels:string[]}}>('{{"n":1.25,"labels":["parsed"]}}');
 churn();console.log(box.items[39].label);console.log(box.items[39].n);console.log(1/box.zero);
 console.log(box.nan===box.nan);console.log(box.fake==={fake_pointer:?});console.log(JSON.stringify(parsed));
}}
"#,
        "X".repeat(256)
    );
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(&source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    // Dedicated stress budget; production keeps its normal fixed fuel ceiling.
    execution.store.set_fuel(abi::FUEL * 8).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    assert_eq!(
        execution.store.data().output.lines,
        [
            "varta log: saved39",
            "varta log: 39.5",
            "varta log: -Infinity",
            "varta log: 0",
            "varta log: 1",
            r#"varta log: {"labels":["parsed"],"n":1.25}"#
        ]
    );
    assert!(
        instance
            .get_memory(&mut execution.store, "memory")
            .unwrap()
            .data_size(&execution.store)
            <= abi::OWNED_STRING_BASE + 2 * abi::WASM_PAGE_BYTES
    );
}

#[test]
fn binary64_parser_checks_static_descriptor_and_schema_versions_before_allocation() {
    let bytes = compile(
        r#"export function main():void{varta.json.parse<{n:number;text:string}>('{"n":1.25,"text":"kept"}');}"#,
    );
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    for action in 0..3 {
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
        let data = memory.data(&execution.store);
        let marker = br#"{"version":2,"root":"#;
        let schema_at = data[..abi::RESULT_BASE]
            .windows(marker.len())
            .position(|s| s == marker)
            .unwrap();
        let schema = serde_json::Deserializer::from_slice(&data[schema_at..abi::RESULT_BASE])
            .into_iter::<varta_common::json::JsonSchema>()
            .next()
            .unwrap()
            .unwrap();
        let descriptor = schema
            .types
            .iter()
            .find_map(|ty| {
                if let varta_common::json::JsonType::Record { descriptor, .. } = ty {
                    Some(*descriptor as usize)
                } else {
                    None
                }
            })
            .unwrap();
        match action {
            0 => memory
                .write(
                    &mut execution.store,
                    descriptor,
                    &value::VALUE_VERSION.to_le_bytes(),
                )
                .unwrap(),
            1 => memory
                .write(
                    &mut execution.store,
                    descriptor + value::RECORD_DESCRIPTOR_HEADER + 3 * size_of::<u32>(),
                    &(value::ValueKind::I32 as u32).to_le_bytes(),
                )
                .unwrap(),
            _ => memory
                .write(
                    &mut execution.store,
                    schema_at + br#"{"version":"#.len(),
                    b"1",
                )
                .unwrap(),
        }
        let before = memory.data_size(&execution.store);
        let error = instance
            .get_typed_func::<(), ()>(&mut execution.store, "_start")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap_err();
        assert!(format!("{error:#}").contains("checked JSON"));
        assert_eq!(memory.data_size(&execution.store), before);
        assert!(
            memory.data(&execution.store)[abi::OWNED_STRING_BASE..]
                .iter()
                .all(|b| *b == 0)
        );
    }
}
