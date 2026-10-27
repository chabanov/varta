use super::*;
use varta_common::{
    exceptions::{self, ThrownValue},
    language::LanguageProfile,
    value::*,
};
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V9)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
#[test]
fn native_errors_have_distinct_names_and_match_only_their_family_and_error_base() {
    let output = run(r#"
function describe(error:Error):void{console.log(error.name);console.log(error.message);console.log(error.toString());console.log(error instanceof Error);console.log(error instanceof TypeError);console.log(error instanceof RangeError);console.log(error instanceof SyntaxError);}
export function main():void{describe(new TypeError("type"));describe(RangeError("range"));describe(new SyntaxError("syntax"));describe(Error("base"));console.log(TypeError()===RangeError());}
"#);
    assert_eq!(
        output.lines,
        [
            "TypeError",
            "type",
            "TypeError: type",
            "1",
            "1",
            "0",
            "0",
            "RangeError",
            "range",
            "RangeError: range",
            "1",
            "0",
            "1",
            "0",
            "SyntaxError",
            "syntax",
            "SyntaxError: syntax",
            "1",
            "0",
            "0",
            "1",
            "Error",
            "base",
            "Error: base",
            "1",
            "0",
            "0",
            "0",
            "0"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn native_errors_narrow_typed_unions_and_preserve_identity_across_function_calls() {
    let output = run(r#"
function typed(value:TypeError):TypeError{return value;}
function broader(value:TypeError|RangeError):Error{return value;}
function base():Error{return SyntaxError("syntax");}
export function main():void{
 const error:Error=TypeError("owned");if(error instanceof TypeError){console.log(typed(error)===error);console.log(error.name);}
 const mixed:TypeError|RangeError=RangeError("range");if(!(mixed instanceof TypeError)){console.log(mixed.message);}else{typed(mixed);}
 const broad=broader(mixed);console.log(broad===mixed);console.log(broad instanceof Error);console.log(base().toString());
 const lookalike={name:"TypeError",message:"fake"};console.log(lookalike instanceof TypeError);console.log(lookalike instanceof Error);
}"#);
    assert_eq!(
        output.lines,
        [
            "1",
            "TypeError",
            "range",
            "1",
            "1",
            "SyntaxError: syntax",
            "0",
            "0"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn native_error_catches_rethrow_finalizers_and_nested_guards_preserve_family() {
    let output = run(r#"
function fail():number{try{throw TypeError("first");}finally{console.log("unwind");}}
function returned():Error{try{return RangeError("return");}finally{try{throw SyntaxError("temporary");}catch(e){if(e instanceof SyntaxError){console.log(e.name);}}}}
export function main():void{
 try{try{console.log(1+(true?fail():0));}catch(e){if(e instanceof Error){console.log(e.name);if(e instanceof TypeError){throw e;}}}}catch(e){if(e instanceof TypeError){console.log(e.message);}}
 console.log(returned().toString());
 try{try{throw TypeError("old");}finally{throw RangeError("replacement");}}catch(e){if(e instanceof Error){console.log(e.name);console.log(e instanceof TypeError);}}
 try{throw "primitive";}catch(e){console.log(e instanceof SyntaxError);if(!(e instanceof Error)){console.log(e);}}
}"#);
    assert_eq!(
        output.lines,
        [
            "unwind",
            "TypeError",
            "first",
            "SyntaxError",
            "RangeError: return",
            "RangeError",
            "0",
            "0",
            "primitive"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn native_errors_keep_own_message_presence_and_non_enumerability_in_aggregates() {
    let output = run(r#"
function own(error:Error):void{console.log(Object.hasOwn(error,"message"));console.log(Object.hasOwn(error,"name"));console.log(error.name);}
export function main():void{
 own(TypeError());own(TypeError(undefined));own(RangeError(""));own(SyntaxError("message"));
 const values:Error[]=[TypeError("first"),RangeError("second")];const alias=values.at(0);console.log(values.includes(alias));console.log(values.indexOf(TypeError("first")));
 console.log(values.slice().at(0)===alias);const last=values.pop();if(last!==undefined){console.log(last.name);}
 const record:{error?:Error|undefined}={};record.error=SyntaxError("json");console.log(JSON.stringify({values:values,record:record}));
 console.log(JSON.stringify(varta.json.parse<{n?:number|string|undefined,v:number[]}>('{"v":[1.25]}')));
}"#);
    assert_eq!(
        output.lines,
        [
            "0",
            "0",
            "TypeError",
            "0",
            "0",
            "TypeError",
            "1",
            "0",
            "RangeError",
            "1",
            "0",
            "SyntaxError",
            "1",
            "-1",
            "1",
            "RangeError",
            "{\"values\":[{}],\"record\":{\"error\":{}}}",
            "{\"v\":[1.25]}"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn native_error_constructor_arguments_run_once_and_pure_observations_preserve_status() {
    let output = run(r#"
function message():string|undefined{console.log("message");return "owned";}
function absent():undefined{console.log("absent");return undefined;}
export function main():void{
 const denied=varta.result.capture(varta.env.get("VARTA_ERROR_FAMILY_DENIED"));
 const error:Error=TypeError(message());const missing=SyntaxError(absent());
 console.log(error);console.log(missing.toString());console.log(typeof error);console.log(!missing);if(missing){console.log("truthy");}
 console.log(error instanceof RangeError);console.log(error instanceof Error);console.log(JSON.stringify(error));
 console.log(varta.result.status());console.log(denied.status);
}"#);
    assert_eq!(
        output.lines,
        [
            "<capability denied: env> (VARTA_ERROR_FAMILY_DENIED)",
            "message",
            "absent",
            "TypeError: owned",
            "SyntaxError",
            "object",
            "0",
            "truthy",
            "0",
            "1",
            "{}",
            "1",
            "1"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn uncaught_native_errors_own_family_exact_utf16_and_presence_after_store_disposal() {
    for class in &ErrorClass::ALL[1..] {
        for (arg, message, has_message) in [
            ("", vec![], false),
            ("undefined", vec![], false),
            ("''", vec![], true),
            ("'\\uD800😀'", vec![0xd800, 0xd83d, 0xde00], true),
        ] {
            let bytes = compile(&format!(
                "export function main():void{{throw {}({arg});}}",
                class.name()
            ));
            let error = VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap_err();
            let RuntimeError::Uncaught(error) = error else {
                panic!("{error}");
            };
            assert_eq!(
                error.value,
                ThrownValue::NativeError {
                    class: *class,
                    message,
                    has_message
                }
            );
            assert!(error.to_string().contains(class.name()));
        }
    }
}
#[test]
fn native_error_annotations_reject_siblings_unguarded_base_values_and_unsupported_features() {
    for source in [
        "function typed(value:TypeError):void{}export function main():void{typed(RangeError());}",
        "function typed(value:TypeError):void{}export function main():void{const error:Error=TypeError();typed(error);}",
        "export function main():void{const value:TypeError|RangeError=SyntaxError();}",
        "export function main():void{try{throw SyntaxError();}catch(e){if(e instanceof TypeError){e=RangeError();}}}",
        "export function main():void{TypeError(1);}",
        "export function main():void{SyntaxError('x',{cause:Error()});}",
        "export function main():void{const error=RangeError();error.name='Changed';}",
        "export function main():void{varta.json.parse<TypeError>('{}');}",
        "export function main():void{varta.json.parse<{error:Error}>('{\"error\":{}}');}",
        "export function main():void{new ReferenceError('pending');}",
        "export function main():void{const TypeError=1;new TypeError();}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                source,
                LanguageProfile::CheckedF64Utf16V9
            )
            .is_err(),
            "{source}"
        );
    }
    for profile in [
        LanguageProfile::CheckedF64Utf16V7,
        LanguageProfile::CheckedF64Utf16V8,
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{throw TypeError('x');}",
                profile
            )
            .is_err()
        );
    }
}

#[test]
fn native_error_families_survive_collection_saved_returns_and_pending_finalizer_cells() {
    let source = r#"
function churn():void{let payload="x";for(let i=0;i<14;i=i+1){payload=payload+payload;}for(let i=0;i<400;i=i+1){const values=[payload+"!",payload+"?"];values.slice();}}
function returned():Error{try{return RangeError("returned"+"😀");}finally{churn();}}
export function main():void{
 const saved=returned();const values:Error[]=[saved,TypeError("array"+"😀")];const popped=values.pop();
 try{try{throw SyntaxError("thrown"+"😀");}finally{churn();try{throw TypeError("temporary");}catch{churn();}}}catch(e){churn();if(e instanceof Error){console.log(e.name);console.log(e.message);}}
 churn();console.log(saved.toString());if(popped!==undefined){console.log(popped.toString());}console.log(values.at(0)===saved);
 for(let i=0;i<7000;i=i+1){try{throw TypeError("garbage"+"!");}catch{}}
 churn();console.log(saved.name);
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
            "SyntaxError",
            "thrown😀",
            "RangeError: returned😀",
            "TypeError: array😀",
            "1",
            "RangeError"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn native_error_boundaries_reject_unknown_family_flags_and_older_readers() {
    let bytes = compile("export function main():void{throw SyntaxError('message');}");
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    for mutation in 0..5 {
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        instance
            .get_typed_func::<(), ()>(&mut execution.store, "_start")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap();
        let cell = instance
            .get_global(&mut execution.store, exceptions::VALUE)
            .unwrap()
            .get(&mut execution.store)
            .i64()
            .unwrap() as u64;
        let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
        let data = memory.data(&execution.store);
        let reference =
            ReferenceBox::read_with_version(data, cell, ERROR_FAMILY_VALUE_VERSION).unwrap();
        let object =
            ErrorObject::read_with_version(data, reference.payload, ERROR_FAMILY_VALUE_VERSION)
                .unwrap();
        assert_eq!(object.class, ErrorClass::SyntaxError);
        assert!(ErrorObject::read(data, reference.payload).is_err());
        assert!(ReferenceBox::read_with_version(data, cell, ERROR_VALUE_VERSION).is_err());
        assert!(
            crate::json::render_reference(
                data,
                ValueKind::Error as u32,
                reference.payload,
                ERROR_VALUE_VERSION
            )
            .is_err()
        );
        let ptr = reference.payload as u32 as usize;
        let (at, word) = match mutation {
            0 => (
                ptr,
                ((ErrorClass::ALL.len() as u32) << ERROR_CLASS_SHIFT) | ERROR_OWN_MESSAGE,
            ),
            1 => (ptr, (ErrorClass::SyntaxError as u32) << ERROR_CLASS_SHIFT),
            2 => (ptr + 4, 2),
            3 => (ptr - abi::STRING_HEADER_BYTES + HEAP_LINK_OFFSET, 0),
            _ => (ptr + 12, 1),
        };
        memory
            .write(&mut execution.store, at, &word.to_le_bytes())
            .unwrap();
        assert!(
            matches!(
                crate::exceptions::finish(&instance, &mut execution.store),
                Err(RuntimeError::ValueAbi(_))
            ),
            "{mutation}"
        );
        assert!(
            crate::json::render_reference(
                memory.data(&execution.store),
                ValueKind::Error as u32,
                reference.payload,
                ERROR_FAMILY_VALUE_VERSION
            )
            .is_err()
        );
    }
}

#[test]
fn native_family_channel_and_versioned_imports_fail_closed_before_guest_effects() {
    use crate::contract_tests::rewrite;
    let bytes = compile(
        "export function main():void{try{throw TypeError('bad');}catch(e){console.log(typeof e);console.log(e instanceof Error);}}",
    );
    let bad = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 2
            && let Some(at) = data
                .windows(b"value_typeof_v5".len())
                .position(|s| s == b"value_typeof_v5")
        {
            data[at..at + b"value_typeof_v5".len()].copy_from_slice(b"value_typeof_v4");
        }
        vec![data]
    });
    assert!(VartaRuntime::new().validate_module(&bad).is_err());
    for name in [
        exceptions::FAMILY_VERSION,
        abi::SECTION,
        varta_common::language::SECTION,
        VALUE_SECTION,
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
        if id == 6 {
            assert!(data.ends_with(&[
                0x7f,
                0,
                0x41,
                exceptions::FAMILY_CHANNEL_VERSION as u8,
                0x0b
            ]));
            let at = data.len() - 2;
            data[at] = exceptions::ERROR_CHANNEL_VERSION as u8;
        }
        vec![data]
    });
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bad).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    assert!(execution.instantiate(&module).is_err());
    assert!(execution.store.data().output.lines.is_empty());
}

#[test]
fn native_error_guards_cannot_intercept_budget_json_index_traps_or_authorized_exit() {
    for body in [
        "while(true){}",
        "varta.json.parse<{n:number}>('{');",
        "const values=[1];values[2]=3;",
        "varta.process.exit(7);",
    ] {
        let bytes = compile(&format!(
            "export function main():void{{try{{{body}}}catch(e){{if(e instanceof Error){{console.log('BAD_CATCH');}}}}finally{{console.log('BAD_FINALLY');return;}}}}"
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
