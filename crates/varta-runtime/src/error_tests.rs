use super::*;
use varta_common::{exceptions::ThrownValue, language::LanguageProfile, value::*};
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V8)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
#[test]
fn errors_have_nominal_identity_default_name_message_and_own_property_presence() {
    let output = run(r#"
function same(error:Error):Error{return error;}
export function main():void{
 const absent=new Error();const missing=Error(undefined);const empty=new Error("");const value=Error("owned"+"😀");
 console.log(absent.name);console.log(absent.message);console.log(value.message);console.log(value.toString());console.log(empty.toString());
 console.log(Object.hasOwn(absent,"message"));console.log(Object.hasOwn(missing,"message"));console.log(Object.hasOwn(empty,"message"));console.log(Object.hasOwn(value,"name"));
 console.log(value===same(value));console.log(value===Error("owned😀"));console.log(value instanceof Error);console.log(({message:"owned😀",name:"Error"}) instanceof Error);
 console.log(typeof value);console.log(!value);if(value){console.log("truthy");}console.log(JSON.stringify(value));
}"#);
    assert_eq!(
        output.lines,
        [
            "Error",
            "",
            "owned😀",
            "Error: owned😀",
            "Error",
            "0",
            "0",
            "1",
            "0",
            "1",
            "0",
            "1",
            "0",
            "object",
            "0",
            "truthy",
            "{}"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn errors_cross_calls_rethrow_finalizers_and_preserve_catch_guard_identity() {
    let output = run(r#"
function fail(error:Error):number{try{throw error;}finally{console.log("final");}}
function value():Error{try{return Error("returned");}finally{try{throw Error("temporary");}catch(e){if(e instanceof Error){console.log(e.message);}}}}
export function main():void{
 const original=Error("original");
 try{try{console.log(1+fail(original));}catch(e){if(e instanceof Error){console.log(e===original);console.log(e.name);throw e;}}finally{console.log("outer-final");}}catch(e){if(e instanceof Error){console.log(e.message);}}
 console.log(value().toString());
 try{throw "primitive";}catch(e){if(e instanceof Error){console.log("bad");}else{console.log(e);}}
 try{throw null;}catch(e){console.log(typeof e);console.log(e instanceof Error);}
}"#);
    assert_eq!(
        output.lines,
        [
            "final",
            "1",
            "Error",
            "outer-final",
            "original",
            "temporary",
            "Error: returned",
            "primitive",
            "object",
            "0"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn errors_live_in_arrays_records_optional_unions_and_json_omits_non_enumerable_message() {
    let output = run(r#"
export function main():void{
 const error=Error("same");const values:Error[]=[error];values.push(Error("other"));console.log(values.includes(error));console.log(values.indexOf(Error("same")));console.log(values.slice().at(0)===error);
 const record:{error?:Error|undefined}={};record.error=error;console.log(JSON.stringify({values:values,record:record}));
 const mixed:Error|{n:number}|number[]|undefined=error;console.log(typeof mixed);console.log(Array.isArray(mixed));if(mixed instanceof Error){console.log(mixed.message);}
 const optional:Error|undefined=values.pop();if(optional!==undefined){console.log(optional.message);}
 console.log(JSON.stringify(varta.json.parse<{n?:number|string|undefined,v:number[]}>('{"v":[1.25,2.5]}')));
}"#);
    assert_eq!(
        output.lines,
        [
            "1",
            "-1",
            "1",
            "{\"values\":[{},{}],\"record\":{\"error\":{}}}",
            "object",
            "0",
            "same",
            "other",
            "{\"v\":[1.25,2.5]}"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn uncaught_errors_own_full_utf16_and_distinguish_missing_and_empty_message() {
    for (expression, message, has_message) in [
        ("Error()", vec![], false),
        ("Error(undefined)", vec![], false),
        ("Error('')", vec![], true),
        ("Error('\\uD800😀')", vec![0xd800, 0xd83d, 0xde00], true),
    ] {
        let error = VartaRuntime::new()
            .run_wasm_bytes(
                &compile(&format!(
                    "export function main():void{{throw {expression};}}"
                )),
                &[],
            )
            .unwrap_err();
        let RuntimeError::Uncaught(error) = error else {
            panic!("{error}");
        };
        assert_eq!(
            error.value,
            ThrownValue::Error {
                message,
                has_message
            }
        );
    }
}
#[test]
fn errors_reject_unsupported_mutation_coercion_causes_and_legacy_profiles() {
    for source in [
        "export function main():void{Error(1);}",
        "export function main():void{Error(null);}",
        "export function main():void{Error('x',{cause:Error()});}",
        "export function main():void{const error=Error();error.message='changed';}",
        "export function main():void{console.log(Error().stack);}",
        "export function main():void{varta.json.parse<Error>('{}');}",
        "export function main():void{try{throw {name:'Error',message:'fake'};}catch{}}",
        "export function main():void{try{throw Error();}catch(e){console.log(e.message);}}",
        "export function main():void{try{throw Error();}catch(e){if(e instanceof Error){e='changed';}}}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                source,
                LanguageProfile::CheckedF64Utf16V8
            )
            .is_err(),
            "{source}"
        );
    }
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf16V7,
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{throw Error('x');}",
                profile
            )
            .is_err()
        );
    }
}

#[test]
fn errors_preserve_argument_effects_once_and_pure_operations_keep_system_status() {
    let output = run(r#"
function message():string|undefined{console.log("message");return "owned";}
function absent():undefined{console.log("absent");return undefined;}
function fail():string{throw Error("argument");}
export function main():void{
 const denied=varta.result.capture(varta.env.get("VARTA_ERROR_DENIED"));
 const error=new Error(message());const missing=new Error(absent());
 console.log(error);console.log(Object.hasOwn(missing,"message"));
 console.log(error instanceof Error);console.log(typeof error);console.log(!error);console.log(JSON.stringify(error));
 try{throw error;}catch(e){if(!(e instanceof Error)){console.log("bad");}else{console.log(e.message);}}
 console.log(varta.result.status());console.log(denied.status);
 try{const never=Error(fail());console.log(never);}catch(e){if(e instanceof Error){console.log(e.message);}}
}"#);
    assert_eq!(
        output.lines,
        [
            "<capability denied: env> (VARTA_ERROR_DENIED)",
            "message",
            "absent",
            "Error: owned",
            "0",
            "1",
            "object",
            "0",
            "{}",
            "owned",
            "1",
            "1",
            "argument"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn errors_keep_messages_and_saved_exceptions_returns_and_arrays_alive_under_collection() {
    let source = r#"
function churn():void{let payload="x";for(let i=0;i<14;i=i+1){payload=payload+payload;}for(let i=0;i<400;i=i+1){const values=[payload+"!",payload+"?"];values.slice();}}
function value():Error{try{return Error("returned"+"😀");}finally{churn();}}
export function main():void {
 const saved=value();const values:Error[]=[saved];values.push(Error("array"+"😀"));const popped=values.pop();
 try{try{throw Error("thrown"+"😀");}finally{churn();try{throw Error("temporary");}catch{churn();}}}catch(e){churn();if(e instanceof Error){console.log(e.message);}}
 churn();console.log(saved.message);if(popped!==undefined){console.log(popped.message);}console.log(values.at(0)===saved);
 for(let i=0;i<7000;i=i+1){try{throw Error("garbage"+"!");}catch{}}
 churn();console.log(saved.toString());
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
            "thrown😀",
            "returned😀",
            "array😀",
            "1",
            "Error: returned😀"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn error_boundaries_reject_corrupt_objects_cells_and_old_value_interpretations() {
    let bytes = compile("export function main():void{throw Error('bad');}");
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    for mutation in 0..9 {
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        instance
            .get_typed_func::<(), ()>(&mut execution.store, "_start")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap();
        let cell = instance
            .get_global(&mut execution.store, varta_common::exceptions::VALUE)
            .unwrap()
            .get(&mut execution.store)
            .i64()
            .unwrap() as u64;
        let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
        let reference = ReferenceBox::read_with_version(
            memory.data(&execution.store),
            cell,
            ERROR_VALUE_VERSION,
        )
        .unwrap();
        assert!(PrimitiveBox::read(memory.data(&execution.store), cell).is_err());
        assert!(
            ReferenceBox::read_with_version(
                memory.data(&execution.store),
                cell,
                VECTOR_VALUE_VERSION
            )
            .is_err()
        );
        let ptr = reference.payload as u32 as usize;
        let (at, word) = match mutation {
            0 => (
                ptr - abi::STRING_HEADER_BYTES + HEAP_FLAGS_OFFSET,
                HEAP_ALLOCATED_FLAG | HEAP_RECORD_FLAG as u32,
            ),
            1 => (ptr - abi::STRING_HEADER_BYTES + HEAP_LINK_OFFSET, 0),
            2 => (ptr, 2),
            3 => (ptr + 4, 2),
            4 => (ptr + 12, 1),
            5 => (ptr + 8, u32::MAX),
            6 => (ptr, 0),
            7 => (ptr - abi::STRING_HEADER_BYTES, 1),
            _ => (cell as u32 as usize, ValueKind::Record as u32),
        };
        memory
            .write(&mut execution.store, at, &word.to_le_bytes())
            .unwrap();
        assert!(
            matches!(
                crate::exceptions::finish(&instance, &mut execution.store),
                Err(RuntimeError::ValueAbi(_))
            ),
            "mutation {mutation}"
        );
        assert!(
            crate::json::render_reference(
                memory.data(&execution.store),
                ValueKind::Error as u32,
                reference.payload,
                ERROR_VALUE_VERSION
            )
            .is_err()
                || mutation == 8
        );
    }
}

#[test]
fn error_profile_requires_new_channel_value_metadata_and_exact_new_host_operations() {
    use crate::contract_tests::rewrite;
    let bytes = compile(
        "export function main():void{try{throw Error('bad');}catch(e){console.log(typeof e);console.log(e instanceof Error);}}",
    );
    for (before, after) in [
        (
            varta_common::exceptions::VERSION.as_bytes(),
            b"varta.exception.invalid".as_slice(),
        ),
        (b"value_typeof_v4".as_slice(), b"value_typeof_v3".as_slice()),
    ] {
        let bad = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if matches!(id, 2 | 7)
                && let Some(at) = data.windows(before.len()).position(|s| s == before)
            {
                data[at..at + before.len()].copy_from_slice(after);
            }
            vec![data]
        });
        assert!(VartaRuntime::new().validate_module(&bad).is_err());
    }
    for name in [
        abi::SECTION,
        varta_common::value::VALUE_SECTION,
        varta_common::language::SECTION,
    ] {
        let bad = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 0
                && let Some(at) = data.windows(name.len()).position(|s| s == name.as_bytes())
            {
                data[at] = b'x';
            }
            vec![data]
        });
        assert!(VartaRuntime::new().validate_module(&bad).is_err(), "{name}");
    }
}

#[test]
fn error_channel_version_is_immutable_and_checked_before_guest_effects() {
    use crate::contract_tests::rewrite;
    let bytes = compile("export function main():void{console.log('BAD_INITIAL_EFFECT');}");
    for mutable in [false, true] {
        let bad = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 6 {
                assert!(data.ends_with(&[
                    0x7f,
                    0,
                    0x41,
                    varta_common::exceptions::ERROR_CHANNEL_VERSION as u8,
                    0x0b
                ]));
                let at = data.len() - if mutable { 4 } else { 2 };
                data[at] = if mutable { 1 } else { 3 };
            }
            vec![data]
        });
        let runtime = VartaRuntime::new();
        if mutable {
            assert!(runtime.validate_module(&bad).is_err());
        } else {
            let module = runtime.validate_module(&bad).unwrap();
            let mut execution =
                crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
            assert!(execution.instantiate(&module).is_err());
            assert!(execution.store.data().output.lines.is_empty());
        }
    }
}
