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
fn string_repeat_normalizes_number_undefined_and_union_counts_and_preserves_utf16() {
    let output = run(r#"
function repeat(text:string,count:number|undefined):string{return text.repeat(count);}
export function main():void{
 console.log("ab".repeat(3));console.log("x".repeat(2.9));console.log("x".repeat(-0.5));console.log("x".repeat());console.log("x".repeat(undefined));console.log("x".repeat(0/0));console.log("x".repeat(-0));
 console.log("".repeat(1e300));console.log("😀".repeat(2));console.log(JSON.stringify("\uD800\0".repeat(2)));
 console.log(repeat("a",3));console.log(repeat("a",undefined));console.log(repeat("a",1.9));console.log("a".repeat(3).repeat(2));console.log("x".repeat("ab".repeat(2).length));
}"#);
    assert_eq!(
        output.lines,
        [
            "ababab",
            "xx",
            "",
            "",
            "",
            "",
            "",
            "",
            "😀😀",
            "\"\\ud800\\u0000\\ud800\\u0000\"",
            "aaa",
            "",
            "a",
            "aaaaaa",
            "xxxx"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn string_repeat_raises_range_errors_for_negative_and_infinite_counts_even_when_empty() {
    let output = run(r#"
function repeat(text:string,count:number):string{return text.repeat(count);}
export function main():void{
 for(let i=0;i<3;i=i+1){try{repeat("abc",i===0?-1:i===1?1/0:-1/0);}catch(e){if(e instanceof RangeError){console.log(e.name);console.log(e.message);}}}
 try{"".repeat(1/0);}catch(e){if(e instanceof RangeError){console.log("empty");}}
 try{try{console.log(1+"x".repeat(-1).length);}finally{console.log("finally");}}catch(e){if(e instanceof Error){console.log(e.name);}}
}"#);
    assert_eq!(
        output.lines,
        [
            "RangeError",
            "Invalid string repeat count",
            "RangeError",
            "Invalid string repeat count",
            "RangeError",
            "Invalid string repeat count",
            "empty",
            "finally",
            "RangeError"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn repeat_receiver_count_and_later_argument_effects_run_in_source_order_once() {
    let output = run(r#"
function text():string{console.log("receiver");return "ab";}
function count():number{console.log("count");return 2;}
function bad():number{console.log("bad count");return -1;}
function empty():string{console.log("empty");return "";}
function later():string{console.log("BAD_LATER");return "later";}
function pair(a:string,b:string):string{return a+b;}
function throwCount():number{throw TypeError("count error");}
export function main():void{
 console.log(text().repeat(count()));console.log(empty().repeat(count()));
 try{pair(text().repeat(bad()),later());}catch(e){if(e instanceof RangeError){console.log("caught");}}
 try{text().repeat(throwCount());}catch(e){if(e instanceof TypeError){console.log(e.message);}}
 try{text().repeat(varta.json.parse<number>('{'));}catch(e){if(e instanceof SyntaxError){console.log(e.name);}}
}"#);
    assert_eq!(
        output.lines,
        [
            "receiver",
            "count",
            "abab",
            "empty",
            "count",
            "",
            "receiver",
            "bad count",
            "caught",
            "receiver",
            "count error",
            "receiver",
            "SyntaxError"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn repeat_operations_preserve_status_and_uncaught_range_errors_are_owned() {
    let output = run(r#"
export function main():void{
 const denied=varta.result.capture(varta.env.get("VARTA_REPEAT_DENIED"));console.log("😀".repeat(3));
 try{"x".repeat(-1);}catch(e){if(e instanceof RangeError){console.log(varta.result.status());}}
 console.log(denied.status);console.log(varta.result.status());
}"#);
    assert_eq!(
        output.lines,
        [
            "<capability denied: env> (VARTA_REPEAT_DENIED)",
            "😀😀😀",
            "1",
            "1",
            "1"
        ]
        .map(|s| format!("varta log: {s}"))
    );
    let error = VartaRuntime::new()
        .run_wasm_bytes(
            &compile("export function main():void{'x'.repeat(-1);}"),
            &[],
        )
        .unwrap_err();
    let RuntimeError::Uncaught(error) = error else {
        panic!("{error}");
    };
    assert_eq!(
        error.value,
        ThrownValue::NativeError {
            class: ErrorClass::RangeError,
            message: "Invalid string repeat count".encode_utf16().collect(),
            has_message: true
        }
    );
}
#[test]
fn repeat_uses_one_pair_of_scratch_slots_and_logarithmic_copying_at_the_byte_limit() {
    let source = format!(
        "export function main():void{{const value='x'.repeat({});console.log(value.length);console.log(value.charAt(value.length-1));console.log('ab'.repeat(3));}}",
        abi::MAX_STRING_BYTES / 2
    );
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(&source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(10 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    crate::exceptions::finish(&instance, &mut execution.store).unwrap();
    let output = &execution.store.data().output;
    assert_eq!(
        output.lines,
        [
            format!("varta log: {}", abi::MAX_STRING_BYTES / 2),
            "varta log: x".into(),
            "varta log: ababab".into()
        ]
    );
}
#[test]
fn repeat_size_exhaustion_stays_fatal_and_bypasses_finally() {
    for count in [(abi::MAX_STRING_BYTES / 2 + 1).to_string(), "1e300".into()] {
        let bytes = compile(&format!(
            "export function main():void{{try{{'x'.repeat({count});}}catch(e){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');return;}}}}"
        ));
        let runtime = VartaRuntime::new();
        let module = runtime.validate_module(&bytes).unwrap();
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        assert!(
            instance
                .get_typed_func::<(), ()>(&mut execution.store, "_start")
                .unwrap()
                .call(&mut execution.store, ())
                .is_err()
        );
        assert!(execution.store.data().output.lines.is_empty());
    }
}
#[test]
fn repeat_rejects_unsupported_counts_receivers_and_older_profiles() {
    for source in [
        "export function main():void{'x'.repeat('2');}",
        "export function main():void{'x'.repeat(true);}",
        "export function main():void{'x'.repeat(null);}",
        "export function main():void{'x'.repeat(1,2);}",
        "export function main():void{[1].repeat(2);}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                source,
                LanguageProfile::CheckedF64Utf16V10
            )
            .is_err(),
            "{source}"
        );
    }
    assert!(
        varta_compiler::compile_typescript_with_profile(
            "export function main():void{'x'.repeat(2);}",
            LanguageProfile::CheckedF64Utf16V8
        )
        .is_err()
    );
    let bytes=varta_compiler::compile_typescript_with_profile("export function main():void{try{'x'.repeat(-1);}catch(e){if(e instanceof RangeError){console.log(e.name);}}}",LanguageProfile::CheckedF64Utf16V9).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap()
            .lines,
        ["varta log: RangeError"]
    );
}

#[test]
fn repeated_strings_keep_receivers_results_and_exceptions_alive_across_collection() {
    let source = r#"
function churn():void{let payload="x";for(let i=0;i<14;i=i+1){payload=payload+payload;}for(let i=0;i<400;i=i+1){const values=[payload+"!",payload+"?"];values.slice();}}
function receiver():string{return "owned"+"😀";}
function count():number{churn();return 3;}
function value():string{try{return receiver().repeat(count());}finally{churn();}}
export function main():void{
 const saved=value();churn();console.log(saved);
 for(let i=0;i<7000;i=i+1){const temp="ab".repeat(256);}
 churn();console.log(saved);try{try{receiver().repeat(-1);}finally{churn();}}catch(e){churn();if(e instanceof RangeError){console.log(e.message);}}
 console.log("x".repeat("ab".repeat(2).length));
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
            "owned😀owned😀owned😀",
            "owned😀owned😀owned😀",
            "Invalid string repeat count",
            "xxxx"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn repeat_uses_existing_ten_helpers_and_reuses_activation_local_copy_scratch() {
    let bytes = compile(
        r#"export function main():void{const values=["x".repeat(2)];values[0]=("a"+"b").charAt(0);values.push("y".repeat(3));values.pop();values.at(-1);values.slice();values.includes("x");values.indexOf("x");console.log("ab".repeat(3));}"#,
    );
    let mut count = 0;
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        if let wasmparser::Payload::FunctionSection(reader) = payload.unwrap() {
            count = reader.count();
        }
    }
    assert_eq!(count as usize, 1 + abi::LEGACY_INTERNAL_FUNCTIONS);
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap()
            .lines,
        ["varta log: ababab"]
    );
}
