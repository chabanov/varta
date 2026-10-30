use super::*;
use varta_common::language::LanguageProfile;

fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V5)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
#[test]
fn checked_json_selects_union_members_and_preserves_utf16_and_number_bits() {
    let output = run(r#"
function show(text:string):void {
 const value=varta.json.parse<number|string|boolean|null|{n:number}|number[]>(text);
 console.log(typeof value);console.log(JSON.stringify(value));
 if(Array.isArray(value)){console.log(value[0]);}
}
export function main():void {
 show('1.25');show('"\\uD800"');show('true');show('null');show('{"n":2.5}');show('[3.5]');
 const zero=varta.json.parse<number|string>('-0');if(typeof zero==="number"){console.log(1/zero);}
 const units=varta.json.parse<string|number>('"\\uD800😀\\uDC00"');
 if(typeof units==="string"){console.log(units.length);console.log(units.charCodeAt(0));console.log(units.charCodeAt(3));}
 console.log(varta.json.parse<string|undefined>('""'));
}"#);
    assert_eq!(
        output.lines,
        [
            "number",
            "1.25",
            "string",
            r#""\ud800""#,
            "boolean",
            "true",
            "object",
            "null",
            "object",
            r#"{"n":2.5}"#,
            "object",
            "[3.5]",
            "3.5",
            "-Infinity",
            "4",
            "55296",
            "56320",
            ""
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn checked_json_optional_fields_retain_presence_order_and_reference_identity() {
    let output = run(r#"
export function main():void {
 const value=varta.json.parse<{a?:number;b?:{n:number};c?:number[];d?:undefined;z:string|undefined}>('{"z":"ok","c":[1.5],"b":{"n":2.5}}');
 console.log(Object.hasOwn(value,"a"));console.log(Object.hasOwn(value,"d"));console.log(value.a);
 console.log(JSON.stringify(value));value.a=undefined;console.log(Object.hasOwn(value,"a"));console.log(JSON.stringify(value));
 value.a=3.5;console.log(JSON.stringify(value));
 const child=value.b;const other=value.b;console.log(child===other);if(child!==undefined){child.n=4.5;}
 const items=value.c;if(Array.isArray(items)){items[0]=5.5;}console.log(JSON.stringify(value));
 const a=varta.json.parse<{n:number}|null>('{"n":1}');const b=varta.json.parse<{n:number}|null>('{"n":1}');console.log(a===b);
 console.log(JSON.stringify(varta.json.parse<{a?:number}>('{}')));
 console.log(JSON.stringify(varta.json.parse<(number|string|null|{n:number})[]>('[1,"x",null,{"n":2}]')));
}"#);
    assert_eq!(
        output.lines,
        [
            "0",
            "0",
            "undefined",
            r#"{"b":{"n":2.5},"c":[1.5],"z":"ok"}"#,
            "1",
            r#"{"b":{"n":2.5},"c":[1.5],"z":"ok"}"#,
            r#"{"b":{"n":2.5},"c":[1.5],"z":"ok","a":3.5}"#,
            "1",
            r#"{"b":{"n":4.5},"c":[5.5],"z":"ok","a":3.5}"#,
            "0",
            "{}",
            r#"[1,"x",null,{"n":2}]"#
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn checked_json_rejects_wrong_members_duplicate_unknown_missing_fields_and_invalid_transport() {
    for input in [
        "{}",
        r#"{"z":1}"#,
        r#"{"z":null}"#,
        r#"{"z":"x","a":false}"#,
        r#"{"z":"x","b":[]}"#,
        r#"{"z":"x","b":{"n":1,"extra":0}}"#,
        r#"{"z":"x","c":[null]}"#,
        r#"{"z":"x","a":1,"\u0061":2}"#,
        r#"{"z":"x","extra":0}"#,
        r#"{"z":"x","a":1e400}"#,
        r#"{"z":"x","d":null}"#,
    ] {
        let source = format!(
            "export function main():void{{varta.json.parse<{{a?:number;b?:{{n:number}};c?:number[];d?:undefined;z:string|undefined}}>({});}}",
            serde_json::to_string(input).unwrap()
        );
        let error = VartaRuntime::new()
            .run_wasm_bytes(&compile(&source), &[])
            .unwrap_err();
        assert!(
            error.to_string().contains("checked JSON"),
            "{input}: {error}"
        );
    }
    for (ty, input) in [
        ("number|string", "null"),
        ("number|string", "[]"),
        ("number[]|null", "{}"),
        ("string|undefined", "undefined"),
        ("number|string", "1e400"),
    ] {
        let source = format!(
            "export function main():void{{varta.json.parse<{ty}>({});}}",
            serde_json::to_string(input).unwrap()
        );
        assert!(
            VartaRuntime::new()
                .run_wasm_bytes(&compile(&source), &[])
                .is_err(),
            "{ty}: {input}"
        );
    }
    for profile in [
        LanguageProfile::CheckedF64Utf16V2,
        LanguageProfile::CheckedF64Utf16V3,
        LanguageProfile::CheckedF64Utf16V4,
    ] {
        for target in ["number|string", "{n?:number}"] {
            let source =
                format!("export function main():void{{varta.json.parse<{target}>('null');}}");
            assert!(varta_compiler::compile_typescript_with_profile(&source, profile).is_err());
        }
    }
    for target in [
        "undefined",
        "undefined[]",
        "{x:undefined}",
        "{a:number}|{b:number}",
        "number[]|string[]",
    ] {
        let source = format!("export function main():void{{varta.json.parse<{target}>('null');}}");
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &source,
                LanguageProfile::CheckedF64Utf16V5
            )
            .is_err(),
            "{target}"
        );
    }
}
/// Wall-clock allowance of the 300x-fuel collection run (test-only).
const TEST_WALL_CLOCK: std::time::Duration = std::time::Duration::from_secs(120);

#[test]
fn checked_json_is_pure_and_retains_union_children_through_collection() {
    let source = r#"
function parse():{child?:{items:(string|number)[]};absent?:string}{return varta.json.parse<{child?:{items:(string|number)[]};absent?:string}>('{"child":{"items":["kept😀",1.25]}}');}
function churn():number {let text="x";for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<400;i=i+1){const value=varta.json.parse<{items:(string|number)[]}>('{"items":["'+text+'",1.25]}');}return 0;}
function keep(v:{child?:{items:(string|number)[]};absent?:string},unused:number):{child?:{items:(string|number)[]};absent?:string}{return v;}
export function main():void{
 const denied=varta.result.capture(varta.env.get("VARTA_CHECKED_JSON_DENIED"));
 const value=keep(parse(),churn());const child=value.child;value.child=undefined;
 churn();console.log(varta.result.status());console.log(denied.status);console.log(Object.hasOwn(value,"absent"));
 if(child!==undefined){console.log(JSON.stringify(child.items));const text=child.items[0];if(typeof text==="string"){console.log(text);}}
}
"#;
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    // Test-only fuel and wall-clock allowance for the collection churn; the
    // production deadline flaked this run under parallel test load.
    execution.store.set_fuel(300 * abi::FUEL).unwrap();
    execution.allow_test_wall_clock(TEST_WALL_CLOCK);
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    let lines = &execution.store.data().output.lines;
    assert!(
        lines.ends_with(&[
            "varta log: 1".into(),
            "varta log: 1".into(),
            "varta log: 0".into(),
            "varta log: [\"kept😀\",1.25]".into(),
            "varta log: kept😀".into()
        ]),
        "{lines:?}"
    );
}

#[test]
fn checked_json_profile_requires_matching_imports_helpers_and_metadata() {
    use crate::contract_tests::rewrite;
    let bytes = compile("export function main():void{varta.json.parse<number|string>('1');}");
    for name in [
        abi::SECTION,
        varta_common::language::SECTION,
        varta_common::value::VALUE_SECTION,
        varta_common::value::VALUE_ALLOCATE,
        varta_common::value::VALUE_RETAIN,
    ] {
        let bad = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if matches!(id, 0 | 7)
                && let Some(at) = data.windows(name.len()).position(|s| s == name.as_bytes())
            {
                data[at] = b'x';
            }
            vec![data]
        });
        assert!(VartaRuntime::new().validate_module(&bad).is_err(), "{name}");
    }
    let bad = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 2
            && let Some(at) = data
                .windows(b"json_parse_v9".len())
                .position(|s| s == b"json_parse_v9")
        {
            data[at + b"json_parse_v".len()] = b'8';
        }
        vec![data]
    });
    assert!(VartaRuntime::new().validate_module(&bad).is_err());
    let bad = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 0
            && let Some(at) = data
                .windows(b"checked-f64-utf16-v5".len())
                .position(|s| s == b"checked-f64-utf16-v5")
        {
            data[at + b"checked-f64-utf16-v".len()] = b'4';
        }
        vec![data]
    });
    assert!(VartaRuntime::new().validate_module(&bad).is_err());
}

#[test]
fn checked_json_malformed_descriptors_and_schemas_fail_before_allocation() {
    use varta_common::{
        json::{JsonSchema, JsonType},
        value::*,
    };
    let bytes = compile(
        r#"export function main():void{varta.json.parse<{a?:number;b:{n:number}|null}>('{"b":{"n":1}}');}"#,
    );
    for mutation in 0..8 {
        let runtime = VartaRuntime::new();
        let module = runtime.validate_module(&bytes).unwrap();
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
        let data = memory.data(&execution.store);
        let marker = b"{\"version\":8,\"root\":";
        let start = data[..abi::RESULT_BASE]
            .windows(marker.len())
            .position(|s| s == marker)
            .unwrap();
        let schema = serde_json::Deserializer::from_slice(&data[start..abi::RESULT_BASE])
            .into_iter::<JsonSchema>()
            .next()
            .unwrap()
            .unwrap();
        let JsonType::Record { descriptor, .. } = schema.types[schema.root as usize] else {
            panic!()
        };
        if mutation >= 6 {
            let at = if mutation == 6 {
                start + b"{\"version\":".len()
            } else {
                let marker = b"\"members\":[";
                start
                    + data[start..abi::RESULT_BASE]
                        .windows(marker.len())
                        .position(|s| s == marker)
                        .unwrap()
                    + marker.len()
            };
            memory
                .write(
                    &mut execution.store,
                    at,
                    &[if mutation == 6 { b'7' } else { b'9' }],
                )
                .unwrap();
            let error = instance
                .get_typed_func::<(), ()>(&mut execution.store, "_start")
                .unwrap()
                .call(&mut execution.store, ())
                .unwrap_err();
            assert!(format!("{error:?}").contains("checked JSON"), "{error:?}");
            assert_eq!(memory.data_size(&execution.store), abi::OWNED_STRING_BASE);
            continue;
        }
        let first = descriptor as usize + RECORD_DESCRIPTOR_HEADER;
        let (at, word) = match mutation {
            0 => (first + 8, 0), // Remove an optional flag.
            1 => (first + RECORD_DESCRIPTOR_ENTRY + 8, OPTIONAL_FIELD_FLAG | 1),
            2 => (
                first + RECORD_DESCRIPTOR_ENTRY + 12,
                ValueKind::PrimitiveUnion as u32,
            ),
            3 => (first + RECORD_DESCRIPTOR_ENTRY + 8, 0), // Alias descriptor slots.
            4 => (first, abi::RESULT_BASE as u32),
            _ => (descriptor as usize, MISSING_VALUE_VERSION),
        };
        memory
            .write(&mut execution.store, at, &word.to_le_bytes())
            .unwrap();
        let error = instance
            .get_typed_func::<(), ()>(&mut execution.store, "_start")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap_err();
        assert!(format!("{error:?}").contains("checked JSON"), "{error:?}");
        assert_eq!(memory.data_size(&execution.store), abi::OWNED_STRING_BASE);
    }
}

#[test]
fn checked_json_validates_complete_input_before_helpers_and_requires_pure_retention() {
    use crate::contract_tests::{export_index, rewrite};
    use varta_common::value::VALUE_RETAIN;
    let bytes = compile(
        r#"function fake(s:string):string{console.log(s);return s;}
export function main():void{varta.json.parse<number|string>('"x"');}"#,
    );
    let fake = export_index(&bytes, "_start") - 1;
    let bad = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7
            && let Some(start) = data
                .windows(VALUE_RETAIN.len())
                .position(|s| s == VALUE_RETAIN.as_bytes())
        {
            let slot = start + VALUE_RETAIN.len() + 1;
            assert!(data[slot] < 128 && fake < 128);
            data[slot] = fake;
        }
        vec![data]
    });
    let error = VartaRuntime::new().validate_module(&bad).unwrap_err();
    assert!(
        error.to_string().contains("cannot call host imports"),
        "{error}"
    );
    for input in [
        r#"{"a":{"n":1},"z":false}"#,
        r#"{"a":{"n":1},"z":"x","z":"y"}"#,
        r#"{"a":{"n":1},"z":"x","extra":0}"#,
    ] {
        let bytes = compile(&format!(
            "export function main():void{{varta.json.parse<{{a?:{{n:number}};z:string|number}}>({});}}",
            serde_json::to_string(input).unwrap()
        ));
        let runtime = VartaRuntime::new();
        let module = runtime.validate_module(&bytes).unwrap();
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
        let error = instance
            .get_typed_func::<(), ()>(&mut execution.store, "_start")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap_err();
        assert!(format!("{error:?}").contains("checked JSON"), "{error:?}");
        assert_eq!(memory.data_size(&execution.store), abi::OWNED_STRING_BASE);
    }
}
