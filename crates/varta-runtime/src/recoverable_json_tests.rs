use super::*;
use varta_common::{exceptions::ThrownValue, language::LanguageProfile, value::ErrorClass};
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V10)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
#[test]
fn json_syntax_and_checked_type_errors_are_catchable_without_echoing_input() {
    let output = run(r#"
function syntax(text:string):void{try{varta.json.parse<{n:number}>(text);console.log("bad");}catch(e){if(e instanceof SyntaxError){console.log(e.name);console.log(e.message);}}}
function type(text:string):void{try{varta.json.parse<{n:number}>(text);console.log("bad");}catch(e){if(e instanceof TypeError){console.log(e.name);console.log(e.message);}}}
export function main():void{
 syntax('{"n":');syntax('1 trailing');
 type('{"n":"SECRET_INPUT"}');type('{"n":1,"n":2}');type('{}');type('{"n":1,"extra":2}');type('{"n":1e400}');
 console.log(varta.json.parse<number>('1.25'));
}"#);
    let mut expected = Vec::new();
    for _ in 0..2 {
        expected.extend(["SyntaxError", "Invalid JSON syntax"]);
    }
    for _ in 0..5 {
        expected.extend(["TypeError", "JSON value does not match the checked schema"]);
    }
    expected.push("1.25");
    assert_eq!(
        output.lines,
        expected
            .into_iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
    assert!(
        !output
            .lines
            .iter()
            .any(|line| line.contains("SECRET_INPUT"))
    );
}
#[test]
fn json_failures_cross_calls_stop_expressions_and_arguments_and_run_finalizers() {
    let output = run(r#"
function fail():number{return varta.json.parse<number>('{');}
function pair():VartaResult<string>{varta.json.parse<number>('{');return varta.result.capture(varta.env.get("BAD_EFFECT"));}
function later():number{console.log("BAD_LATER");return 2;}
function add(a:number,b:number):number{return a+b;}
export function main():void{
 try{console.log(1+(true?fail():0));}catch(e){if(e instanceof Error){console.log(e.name);}}
 try{add(fail(),later());}catch(e){if(e instanceof SyntaxError){console.log("arguments");}}
 try{console.log(pair().value);}catch(e){if(e instanceof SyntaxError){console.log("pair");}}
 try{try{fail();}finally{console.log("finally");}}catch(e){if(e instanceof SyntaxError){console.log("outer");}}
 try{varta.env.get(varta.json.parse<string>('{'));}catch(e){if(e instanceof SyntaxError){console.log("host argument");}}
 console.log(varta.result.status());
 console.log("continued");
}"#);
    assert_eq!(
        output.lines,
        [
            "SyntaxError",
            "arguments",
            "pair",
            "finally",
            "outer",
            "host argument",
            "0",
            "continued"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn json_failure_rethrows_and_finalizer_overrides_preserve_error_family() {
    let output = run(r#"
function value():number{try{return varta.json.parse<number>('"wrong"');}finally{return 7;}}
export function main():void{
 try{try{varta.json.parse<number>('{');}catch(e){throw e;}}catch(e){if(e instanceof Error){console.log(e.toString());}}
 try{try{varta.json.parse<number>('{');}finally{throw RangeError("replacement");}}catch(e){if(e instanceof RangeError){console.log(e.message);}}
 console.log(value());
}"#);
    assert_eq!(
        output.lines,
        ["SyntaxError: Invalid JSON syntax", "replacement", "7"].map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn uncaught_json_errors_are_owned_typed_native_values() {
    for (input, class, message) in [
        ("{", ErrorClass::SyntaxError, "Invalid JSON syntax"),
        (
            "\"wrong\"",
            ErrorClass::TypeError,
            "JSON value does not match the checked schema",
        ),
    ] {
        let literal = serde_json::to_string(input).unwrap();
        let bytes = compile(&format!(
            "export function main():void{{varta.json.parse<number>({literal});}}"
        ));
        let error = VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap_err();
        let RuntimeError::Uncaught(error) = error else {
            panic!("{error}");
        };
        assert_eq!(
            error.value,
            ThrownValue::NativeError {
                class,
                message: message.encode_utf16().collect(),
                has_message: true
            }
        );
    }
}
#[test]
fn json_decode_preserves_status_and_recovers_to_valid_owned_optional_union_values() {
    let output = run(r#"
export function main():void{
 const denied=varta.result.capture(varta.env.get("VARTA_RECOVERABLE_JSON_DENIED"));
 try{varta.json.parse<number>('{');}catch(e){if(e instanceof SyntaxError){console.log(varta.result.status());}}
 try{varta.json.parse<number>('"wrong"');}catch(e){if(e instanceof TypeError){console.log(denied.status);}}
 const value=varta.json.parse<{name?:string|undefined,v:number|string|null}[]>('[{"v":1.25},{"name":"😀","v":"\\ud800"}]');
 console.log(JSON.stringify(value));console.log(varta.result.status());
}"#);
    assert_eq!(
        output.lines,
        [
            "<capability denied: env> (VARTA_RECOVERABLE_JSON_DENIED)",
            "1",
            "1",
            "[{\"v\":1.25},{\"name\":\"😀\",\"v\":\"\\ud800\"}]",
            "1"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn invalid_unicode_json_source_is_syntax_but_valid_lone_units_are_preserved() {
    let output = run(r#"
export function main():void{
 try{varta.json.parse<string>("\uD800");}catch(e){if(e instanceof SyntaxError){console.log(e.name);}}
 const value=varta.json.parse<string>('"\uD800😀"');console.log(JSON.stringify(value));
 try{varta.json.parse<{v:string}>('{"\\ud800":"x"}');}catch(e){if(e instanceof TypeError){console.log(e.name);}}
}"#);
    assert_eq!(
        output.lines,
        ["SyntaxError", "\"\\ud800😀\"", "TypeError"].map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn json_resource_limits_stay_fatal_even_for_wrong_schema_or_unknown_fields() {
    use varta_common::{
        json::{JsonDecodeError, JsonInputLimit},
        value::*,
    };
    let row = format!("[{}]", vec!["0"; MAX_ARRAY_ELEMENTS].join(","));
    for (input, expected) in [
        (
            format!("[{}]", vec!["0"; MAX_ARRAY_ELEMENTS + 1].join(",")),
            JsonInputLimit::ArrayElements,
        ),
        (format!("[{row},{row},{row},{row}]"), JsonInputLimit::Nodes),
        (
            format!(
                "{}0{}",
                "[".repeat(MAX_JSON_DEPTH + 1),
                "]".repeat(MAX_JSON_DEPTH + 1)
            ),
            JsonInputLimit::Depth,
        ),
    ] {
        let literal = serde_json::to_string(&input).unwrap();
        let bytes = compile(&format!(
            "export function main():void{{try{{varta.json.parse<number>({literal});}}catch(e){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');return;}}}}"
        ));
        let runtime = VartaRuntime::new();
        let module = runtime.validate_module(&bytes).unwrap();
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let error = instance
            .get_typed_func::<(), ()>(&mut execution.store, "_start")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap_err();
        assert!(
            matches!(error.downcast_ref::<JsonDecodeError>(),Some(JsonDecodeError::Limit(actual)) if *actual==expected),
            "{error:?}"
        );
        assert!(execution.store.data().output.lines.is_empty());
    }
    for body in [
        "let text=' ';for(let i=0;i<16;i=i+1){text=text+text;}varta.json.parse<number>(text);",
        "let text='\\uD800';for(let i=0;i<14;i=i+1){text=text+text;}varta.json.parse<string>('\\\"'+text+'\\\"');",
        "while(true){}",
        "const value=[1];value[2]=3;",
        "varta.process.exit(7);",
    ] {
        let bytes = compile(&format!(
            "export function main():void{{try{{{body}}}catch(e){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');return;}}}}"
        ));
        let runtime = VartaRuntime::new();
        let module = runtime.validate_module(&bytes).unwrap();
        let grants = [varta_common::CapabilityGrant {
            id: "process.exit".into(),
            value: varta_common::CapabilityValue::Boolean(true),
        }];
        let mut execution =
            crate::execution::Execution::new(&runtime.engine, &grants, None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let error = instance
            .get_typed_func::<(), ()>(&mut execution.store, "_start")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap_err();
        if body.contains("process.exit") {
            assert_eq!(error.downcast_ref::<GuestExit>().unwrap().0, 7);
        }
        assert!(execution.store.data().output.lines.is_empty());
    }
}

#[test]
fn json_exception_values_and_earlier_references_survive_collection_and_repeated_recovery() {
    let source = r#"
function churn():void{let payload="x";for(let i=0;i<14;i=i+1){payload=payload+payload;}for(let i=0;i<400;i=i+1){const values=[payload+"!",payload+"?"];values.slice();}}
function saved():Error{try{varta.json.parse<number>('"wrong"');}catch(e){if(e instanceof Error){return e;}}return Error("bad");}
export function main():void{
 const keep=varta.json.parse<{s:string}>('{"s":"kept😀"}');const error=saved();churn();console.log(error.toString());console.log(keep.s);
 try{try{varta.json.parse<number>('{');}finally{churn();try{throw TypeError("temporary");}catch{churn();}}}catch(e){churn();if(e instanceof SyntaxError){console.log(e.message);}}
 let count=0;for(let i=0;i<200;i=i+1){try{varta.json.parse<number>('{');}catch{count=count+1;}}
 churn();console.log(count);console.log(JSON.stringify(varta.json.parse<{s:string}>('{"s":"after😀"}')));
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
    crate::exceptions::finish(&instance, &mut execution.store).unwrap();
    assert_eq!(
        execution.store.data().output.lines,
        [
            "TypeError: JSON value does not match the checked schema",
            "kept😀",
            "Invalid JSON syntax",
            "200",
            "{\"s\":\"after😀\"}"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn json_decoder_metadata_and_untrusted_helpers_cannot_become_catchable_errors() {
    use crate::contract_tests::{export_index, rewrite};
    use varta_common::{
        json::{JsonSchema, JsonType},
        value::*,
    };
    let bytes = compile(
        "export function main():void{try{varta.json.parse<{n:number}>('{');}catch(e){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
    );
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    for mutation in 0..4 {
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
        let data = memory.data(&execution.store);
        let marker = b"{\"version\":11,\"root\":";
        let at = data[..abi::RESULT_BASE]
            .windows(marker.len())
            .position(|s| s == marker)
            .unwrap();
        let raw = serde_json::Deserializer::from_slice(&data[at..abi::RESULT_BASE])
            .into_iter::<serde_json::Value>()
            .next()
            .unwrap()
            .unwrap();
        let schema = JsonSchema::from_bytes(&serde_json::to_vec(&raw).unwrap()).unwrap();
        let JsonType::Record { descriptor, .. } = schema.types[schema.root as usize] else {
            panic!()
        };
        let (at, word) = match mutation {
            0 => (at + b"{\"version\":".len(), vec![b'9']),
            1 => (descriptor as usize, 999u32.to_le_bytes().to_vec()),
            2 => (
                descriptor as usize + RECORD_DESCRIPTOR_HEADER + 12,
                u32::MAX.to_le_bytes().to_vec(),
            ),
            _ => (
                descriptor as usize + RECORD_DESCRIPTOR_HEADER,
                u32::MAX.to_le_bytes().to_vec(),
            ),
        };
        memory.write(&mut execution.store, at, &word).unwrap();
        assert!(
            instance
                .get_typed_func::<(), ()>(&mut execution.store, "_start")
                .unwrap()
                .call(&mut execution.store, ())
                .is_err()
        );
        assert!(execution.store.data().output.lines.is_empty());
    }
    let source = "function fake(n:boolean):boolean{return false;}export function main():void{try{varta.json.parse<number>('{');}catch(e){console.log('BAD_CATCH');}}";
    let bytes = compile(source);
    let fake = export_index(&bytes, "_start") - 1;
    let bad = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            let at = data
                .windows(VALUE_ALLOCATE.len())
                .position(|s| s == VALUE_ALLOCATE.as_bytes())
                .unwrap()
                + VALUE_ALLOCATE.len()
                + 1;
            assert!(fake < 128 && data[at] < 128);
            data[at] = fake;
        }
        vec![data]
    });
    let module = runtime.validate_module(&bad).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    let error = instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap_err();
    assert!(format!("{error:?}").contains("allocator returned an invalid pointer"));
    assert!(execution.store.data().output.lines.is_empty());
}

#[test]
fn json_imports_are_profile_bound_and_legacy_parse_failures_remain_fatal() {
    use crate::contract_tests::rewrite;
    let source = "export function main():void{try{varta.json.parse<number>('{');}catch(e){console.log('BAD_CATCH');}}";
    let legacy =
        varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V9)
            .unwrap();
    assert!(VartaRuntime::new().run_wasm_bytes(&legacy, &[]).is_err());
    let bytes = compile(source);
    let bad = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 2
            && let Some(at) = data
                .windows(b"json_parse_v13".len())
                .position(|s| s == b"json_parse_v13")
        {
            data[at..at + b"json_parse_v13".len()].copy_from_slice(b"json_parse_v12");
        }
        vec![data]
    });
    assert!(VartaRuntime::new().validate_module(&bad).is_err());
}

#[test]
fn repeated_json_failures_cannot_catch_host_operation_exhaustion() {
    let bytes = compile(&format!(
        "export function main():void{{try{{for(let i=0;i<{};i=i+1){{try{{varta.json.parse<number>('{{');}}catch{{}}}}}}finally{{console.log('BAD_FINALLY');return;}}}}",
        abi::MAX_HOST_CALLS + 1
    ));
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(20 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    let error = instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap_err();
    assert!(format!("{error:?}").contains("host operation budget"));
    assert!(execution.store.data().output.lines.is_empty());
}

#[test]
fn decoder_rejects_an_existing_pending_channel_instead_of_replacing_its_error() {
    let bytes = compile("export function main():void{varta.json.parse<number>('{');}");
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    let start = instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap();
    start.call(&mut execution.store, ()).unwrap();
    let value = instance
        .get_global(&mut execution.store, varta_common::exceptions::VALUE)
        .unwrap();
    let original = value.get(&mut execution.store).i64().unwrap();
    assert_ne!(original, 0);
    let error = start.call(&mut execution.store, ()).unwrap_err();
    assert!(format!("{error:?}").contains("noncanonical JSON exception channel"));
    assert_eq!(value.get(&mut execution.store).i64(), Some(original));
}
