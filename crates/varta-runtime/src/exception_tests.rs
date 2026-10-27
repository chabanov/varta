use super::*;
use varta_common::{exceptions::ThrownValue as V, language::LanguageProfile};
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V7)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
#[test]
fn guest_exceptions_cross_calls_and_stop_incomplete_expressions_before_effects() {
    let output = run(r#"
function fail():number{throw "owned"+"😀";}
function text():string{throw "name";}
function boolean():boolean{throw false;}
function pair():VartaResult<string>{throw "pair";}
export function main():void {
 try{console.log(1+fail());console.log("bad");}catch(e){console.log(typeof e);if(typeof e==="string"){console.log(e);}}
 try{const v=true?fail():2;console.log(v);}catch(e){console.log(e);}
 try{console.log(true&&boolean());}catch(e){console.log(e);}
 try{const v:number|undefined=undefined;console.log(v??fail());}catch(e){console.log(e);}
 try{varta.env.get(text());}catch(e){console.log(e);}console.log(varta.result.status());
 try{console.log(pair().status);}catch(e){console.log(e);}
 console.log("after");
}"#);
    assert_eq!(
        output.lines,
        [
            "string",
            "owned😀",
            "owned😀",
            "0",
            "owned😀",
            "name",
            "0",
            "pair",
            "after"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn guest_exceptions_catch_all_primitives_preserve_bits_and_rethrow_to_outer_handlers() {
    let output = run(r#"
function fail(v:number|string|boolean|null|undefined):void{throw v;}
export function main():void {
 try{fail(-0);}catch(e){if(typeof e==="number"){console.log(1/e);}}
 try{fail(0/0);}catch(e){if(typeof e==="number"){console.log(e!==e);}}
 try{fail(1/0);}catch(e){console.log(e);}
 try{fail(null);}catch(e){console.log(e);}
 try{fail(undefined);}catch(e){console.log(e);}
 try{try{throw 1.25;}catch(e){console.log(e);throw "outer";}}catch(e){console.log(e);}
 try{try{throw "again";}catch(e){throw e;}}catch(e){console.log(e);}
 try{throw "discarded";}catch{console.log("caught");}
 try{console.log("normal");}catch{console.log("bad");}
}"#);
    assert_eq!(
        output.lines,
        [
            "-Infinity",
            "1",
            "Infinity",
            "null",
            "undefined",
            "1.25",
            "outer",
            "again",
            "caught",
            "normal"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn guest_exceptions_preserve_lexical_scopes_loop_branches_and_return_paths() {
    let output = run(r#"
function value(flag:boolean):number{try{if(flag){throw 1;}return 2;}catch(e){return 3;}}
function always():number{throw 4;}
export function main():void {
 const e="outer";try{const e="inner";throw e;}catch(e){console.log(e);}console.log(e);
 console.log(value(false));console.log(value(true));
 for(let i=0;i<4;i=i+1){try{if(i===1){continue;}if(i===3){break;}throw i;}catch(e){console.log(e);}}
 try{for(let i=0;i<4;i=i+1){if(i===2){always();}}}catch(e){console.log(e);}
 let count=0;while(count<3){try{throw count;}catch(e){count=count+1;continue;}}
 console.log(count);console.log("done");
}"#);
    assert_eq!(
        output.lines,
        ["inner", "outer", "2", "3", "0", "2", "4", "3", "done"].map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn guest_exceptions_uncaught_values_are_owned_typed_and_diagnostics_are_bounded() {
    for (expression, expected) in [
        ("-0", V::Number((-0f64).to_bits())),
        ("true", V::Boolean(true)),
        ("null", V::Null),
        ("undefined", V::Undefined),
        ("'\\uD800😀'", V::String(vec![0xd800, 0xd83d, 0xde00])),
    ] {
        let source =
            format!("export function main():void{{throw {expression};console.log('bad');}}");
        let error = VartaRuntime::new()
            .run_wasm_bytes(&compile(&source), &[])
            .unwrap_err();
        let RuntimeError::Uncaught(error) = error else {
            panic!("wrong error {error}");
        };
        assert_eq!(error.value, expected);
    }
    let text = "x".repeat(varta_common::exceptions::MAX_DIAGNOSTIC_UNITS + 10);
    let error = varta_common::exceptions::UncaughtException {
        value: V::String(text.encode_utf16().collect()),
    };
    assert!(error.to_string().len() < text.len() + 40);
    assert!(error.to_string().ends_with('…'));
}
#[test]
fn guest_exceptions_preserve_capability_denials_and_do_not_catch_runtime_failures() {
    let output = run(r#"
export function main():void {
 const denied=varta.result.capture(varta.env.get("VARTA_EXCEPTION_DENIED"));
 try{throw "handled";}catch(e){const v=["😀"];v.push("kept");console.log(e);console.log(varta.result.status());}
 console.log(denied.status);
}"#);
    assert!(
        output
            .lines
            .ends_with(&["handled", "1", "1"].map(|s| format!("varta log: {s}")))
    );
    for (body, expected) in [
        ("while(true){}", "fuel"),
        ("varta.json.parse<{n:number}>('{');", "checked JSON"),
        ("const v=[1];v[2]=3;", "unreachable"),
    ] {
        let source =
            format!("export function main():void{{try{{{body}}}catch(e){{console.log('bad');}}}}");
        let error = VartaRuntime::new()
            .run_wasm_bytes(&compile(&source), &[])
            .unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }
}
#[test]
fn guest_exceptions_reject_unsupported_values_annotations_and_old_profiles() {
    for source in [
        "export function main():void{throw {n:1};}",
        "export function main():void{throw [1];}",
        "export function main():void{try{throw 1;}catch(e:unknown){}}",
        "export function main():void{try{throw 1;}catch({message}){}}",
        "export function main():void{try{throw 1;}catch(e){}console.log(e);}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                source,
                LanguageProfile::CheckedF64Utf16V7
            )
            .is_err(),
            "{source}"
        );
    }
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf16V6,
    ] {
        for source in [
            "export function main():void{throw 1;}",
            "export function main():void{try{}catch{}}",
            "export function main():void{try{}finally{}}",
        ] {
            assert!(varta_compiler::compile_typescript_with_profile(source, profile).is_err());
        }
    }
}

#[test]
fn finally_runs_once_for_normal_caught_and_propagating_completions() {
    let output = run(r#"
function fail():number{throw "call";}
export function main():void {
 try{console.log("body");}finally{console.log("normal");}
 try{throw "caught";}catch(e){console.log(e);}finally{console.log("caught-final");}
 try{try{console.log(1+(true?fail():2));}finally{console.log("unwind");}}catch(e){console.log(e);}
 try{try{throw "first";}catch(e){throw "second";}finally{console.log("rethrow");}}catch(e){console.log(e);}
 console.log("done");
}"#);
    assert_eq!(
        output.lines,
        [
            "body",
            "normal",
            "caught",
            "caught-final",
            "unwind",
            "call",
            "rethrow",
            "second",
            "done"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn finally_preserves_return_values_and_orders_nested_finalizers() {
    let output = run(r#"
function value():number{let n=1;try{return n;}finally{n=2;console.log(n);}}
function nested():string{try{try{return "owned"+"😀";}finally{console.log("inner");}}finally{console.log("outer");}}
function caught():number{try{throw 1;}catch(e){return 3;}finally{console.log("catch-final");}}
function empty():void{try{return;}finally{console.log("void");}}
function pair():VartaResult<string>{try{return varta.result.capture(varta.env.get("VARTA_FINALLY_DENIED"));}finally{console.log("pair");}}
export function main():void{console.log(value());console.log(nested());console.log(caught());empty();console.log(pair().status);}
"#);
    assert!(
        output
            .lines
            .ends_with(&["pair", "1"].map(|s| format!("varta log: {s}")))
    );
    assert_eq!(
        &output.lines[..8],
        [
            "2",
            "1",
            "inner",
            "outer",
            "owned😀",
            "catch-final",
            "3",
            "void"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn finally_abrupt_completions_override_pending_return_or_throw() {
    let output = run(r#"
function a():number{try{return 1;}finally{return 2;}}
function b():number{try{throw "lost";}finally{return 3;}}
function c():number{try{return 4;}finally{throw "replacement";}}
function d():number{try{try{return 5;}finally{return 6;}}finally{return 7;}}
function replaceThrow():void{try{throw "old";}finally{throw "new";}}
export function main():void{console.log(a());console.log(b());try{c();}catch(e){console.log(e);}console.log(d());try{replaceThrow();}catch(e){console.log(e);}console.log("done");}
"#);
    assert_eq!(
        output.lines,
        ["2", "3", "replacement", "7", "new", "done"].map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn finally_runs_only_when_branches_leave_its_protected_region() {
    let output = run(r#"
export function main():void {
 for(let i=0;i<4;i=i+1){try{if(i===0){continue;}if(i===2){break;}console.log(i);}finally{console.log("for");}}
 let n=0;while(n<3){try{n=n+1;continue;}finally{console.log(n);}}
 try{for(let i=0;i<3;i=i+1){if(i===0){continue;}break;}console.log("inside");}finally{console.log("outside");}
 for(let i=0;i<3;i=i+1){try{try{continue;}finally{console.log("inner");}}finally{console.log("outer");}}
 for(let i=0;i<3;i=i+1){try{switch(i){case 0:continue;default:break;}}finally{console.log(i);}}
}"#);
    assert_eq!(
        output.lines,
        [
            "for", "1", "for", "for", "1", "2", "3", "inside", "outside", "inner", "outer",
            "inner", "outer", "inner", "outer", "0", "1", "2"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn finally_branches_override_returns_and_throws_and_target_outer_loops() {
    let output = run(r#"
function value():number{for(let i=0;i<3;i=i+1){try{return 1;}finally{break;}}return 2;}
export function main():void {
 console.log(value());
 for(let i=0;i<3;i=i+1){try{throw "discard";}finally{continue;}}
 for(let i=0;i<3;i=i+1){try{try{throw "discard";}finally{break;}}finally{console.log("outer");}}
}"#);
    // The final throw survives a loop-local continue in its finalizer.
    let error = VartaRuntime::new().run_wasm_bytes(&compile(r#"export function main():void{try{throw "preserved";}finally{for(let i=0;i<2;i=i+1){continue;}}}"#), &[]).unwrap_err();
    assert!(matches!(error, RuntimeError::Uncaught(_)));
    assert_eq!(
        output.lines,
        ["2", "outer"].map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn finally_saves_exception_and_return_roots_across_collection_and_nested_catches() {
    let source = r#"
function collect():void{let payload="x";for(let i=0;i<14;i=i+1){payload=payload+payload;}for(let i=0;i<400;i=i+1){const v=[payload+"!",payload+"?"];v.slice();}}
function text():string{try{return "return"+"😀";}finally{collect();try{throw "temporary";}catch{collect();}}}
function object():{value:string}{try{return {value:"record"+"😀"};}finally{collect();}}
function array():string[]{try{return ["array"+"😀"];}finally{collect();}}
function pair():VartaResult<string>{try{return varta.result.capture(varta.env.get("VARTA_FINALLY_DENIED"));}finally{collect();}}
export function main():void {
 try{try{throw "exception"+"😀";}finally{collect();try{throw 2;}catch(e){console.log(e);}collect();}}catch(e){console.log(e);}
 console.log(text());console.log(object().value);console.log(JSON.stringify(array()));console.log(pair().status);
 for(let i=0;i<7000;i=i+1){try{try{throw "x"+"y";}finally{try{throw "z";}catch{}}}catch{}}
 console.log("done");
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
    let output = &execution.store.data().output;
    assert_eq!(
        output.lines,
        [
            "2",
            "exception😀",
            "return😀",
            "record😀",
            "[\"array😀\"]",
            "<capability denied: env> (VARTA_FINALLY_DENIED)",
            "1",
            "done"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn finally_preserves_aliases_and_lexical_scope_and_does_not_reenter_own_catch() {
    let output = run(r#"
function result(v:{n:number}):{n:number}{try{return v;}finally{v.n=2;}}
export function main():void {
 const e="outer";try{const e="body";console.log(e);}catch(e){console.log("bad");}finally{console.log(e);}
 const obj={n:1};console.log(result(obj)===obj);console.log(obj.n);
 try{try{console.log("body");}catch{console.log("bad");}finally{throw "final";}}catch(e){console.log(e);}
}"#);
    assert_eq!(
        output.lines,
        ["body", "outer", "1", "2", "body", "final"].map(|s| format!("varta log: {s}"))
    );
    for source in [
        "function f():number{while(true){try{return 1;}finally{break;}}}export function main():void{}",
        "export function main():void{try{const x=1;}finally{console.log(x);}}",
        "export function main():void{try{throw 1;}catch(e){}finally{console.log(e);}}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                source,
                LanguageProfile::CheckedF64Utf16V7
            )
            .is_err(),
            "{source}"
        );
    }
}

#[test]
fn finally_cannot_override_fatal_traps_or_authorized_process_exit() {
    for body in [
        "while(true){}",
        "varta.json.parse<{n:number}>('{');",
        "const v=[1];v[2]=3;",
        "varta.process.exit(7);",
    ] {
        let bytes = compile(&format!(
            "export function main():void{{try{{{body}}}finally{{console.log('BAD_FINALLY');return;}}}}"
        ));
        let grants = [varta_common::CapabilityGrant {
            id: "process.exit".into(),
            value: varta_common::CapabilityValue::Boolean(true),
        }];
        let runtime = VartaRuntime::new();
        let module = runtime.validate_module(&bytes).unwrap();
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
        assert!(
            !execution
                .store
                .data()
                .output
                .lines
                .iter()
                .any(|line| line.contains("BAD_FINALLY"))
        );
    }
}

#[test]
fn finally_retains_exact_primitive_returns_and_restores_original_throw_after_local_catch() {
    let output = run(r#"
function zero():number{try{return -0;}finally{try{throw 2;}catch{}}}
function missing():string|undefined{try{return undefined;}finally{}}
function textUnion():string|undefined{try{return "union"+"😀";}finally{try{throw 2;}catch{}}}
function nullValue():null{try{return null;}finally{}}
function flag():boolean{try{return false;}finally{}}
export function main():void {
 console.log(1/zero());console.log(missing());console.log(textUnion());console.log(JSON.stringify(nullValue()));console.log(flag());
 try{try{throw -0;}finally{try{throw "temporary";}catch(e){console.log(e);}}}catch(e){if(typeof e==="number"){console.log(1/e);}}
}"#);
    assert_eq!(
        output.lines,
        [
            "-Infinity",
            "undefined",
            "union😀",
            "null",
            "0",
            "temporary",
            "-Infinity"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn guest_exceptions_keep_live_values_and_abandoned_scopes_correct_under_collection() {
    let source = r#"
function fail(text:string):string{const v=[text+"!"];throw v.at(0);}
function churn():number{let text="x";for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<400;i=i+1){const v=[text+"!",text+"?"];v.slice();}return 0;}
function read(text:string):string {try{const value="abandoned"+text;return "prefix"+fail(value);}catch(e){churn();if(typeof e==="string"){return e;}return "bad";}}
export function main():void {
 const keep={text:"kept"+"😀"};const saved=read("saved");churn();console.log(saved);console.log(keep.text);
 let count=0;for(let i=0;i<7000;i=i+1){try{const v=["garbage"+"!"];throw v.at(0);}catch(e){count=count+1;}}
 churn();console.log(count);
 try{throw "outer"+"😀";}catch(e){try{throw "inner";}catch(e){console.log(e);}churn();console.log(e);}
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
        ["abandonedsaved!", "kept😀", "7000", "inner", "outer😀"]
            .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn guest_exceptions_authorized_process_exit_bypasses_catch() {
    let bytes = compile(
        "export function main():void{try{console.log('before');varta.process.exit(7);console.log('bad');}catch(e){console.log('caught');}}",
    );
    let grant = varta_common::CapabilityGrant {
        id: "process.exit".into(),
        value: varta_common::CapabilityValue::Boolean(true),
    };
    let output = VartaRuntime::new()
        .run_wasm_bytes(&bytes, &[grant])
        .unwrap();
    assert_eq!(output.exit_code, 7);
    assert_eq!(output.lines, ["varta log: before"]);
}

#[test]
fn guest_exceptions_require_exact_channel_metadata_and_global_signatures() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile("export function main():void{try{throw 'x';}catch(e){console.log(e);}}");
    for name in [
        abi::SECTION,
        varta_common::value::VALUE_SECTION,
        varta_common::language::SECTION,
        varta_common::exceptions::PENDING,
        varta_common::exceptions::VALUE,
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
        if id == 7
            && let Some(at) = data
                .windows(varta_common::exceptions::PENDING.len())
                .position(|s| s == varta_common::exceptions::PENDING.as_bytes())
        {
            let slot = at + varta_common::exceptions::PENDING.len();
            data[slot] = 0;
            data[slot + 1] = export_index(&bytes, "_start");
        }
        vec![data]
    });
    assert!(VartaRuntime::new().validate_module(&bad).is_err());
    let old = varta_compiler::compile_typescript_with_profile(
        "export function main():void{}",
        LanguageProfile::CheckedF64Utf16V6,
    )
    .unwrap();
    assert!(VartaRuntime::new().validate_module(&old).is_ok());
}

#[test]
fn guest_exceptions_reject_corrupted_channel_states_and_payloads_at_the_boundary() {
    use varta_common::exceptions as abi_e;
    for mutation in 0..5 {
        let bytes = compile("export function main():void{throw 'owned'+'😀';}");
        let runtime = VartaRuntime::new();
        let module = runtime.validate_module(&bytes).unwrap();
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        instance
            .get_typed_func::<(), ()>(&mut execution.store, "_start")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap();
        let pending = instance
            .get_global(&mut execution.store, abi_e::PENDING)
            .unwrap();
        let value = instance
            .get_global(&mut execution.store, abi_e::VALUE)
            .unwrap();
        let cell = value.get(&mut execution.store).i64().unwrap() as u64;
        let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
        match mutation {
            0 => pending
                .set(&mut execution.store, wasmtime::Val::I32(2))
                .unwrap(),
            1 => value
                .set(&mut execution.store, wasmtime::Val::I64(0))
                .unwrap(),
            2 => memory
                .write(&mut execution.store, abi_e::ROOT, &0u64.to_le_bytes())
                .unwrap(),
            3 => memory
                .write(
                    &mut execution.store,
                    cell as u32 as usize,
                    &(varta_common::value::ValueKind::Record as u32).to_le_bytes(),
                )
                .unwrap(),
            _ => pending
                .set(&mut execution.store, wasmtime::Val::I32(0))
                .unwrap(),
        }
        assert!(matches!(
            crate::exceptions::finish(&instance, &mut execution.store),
            Err(RuntimeError::ValueAbi(_))
        ));
    }
}

#[test]
fn guest_exceptions_reject_native_start_and_invalid_initial_state_before_guest_effects() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile("export function main():void{console.log('BAD_INITIAL_EFFECT');}");
    // Insert a real start section before code, retaining the original bodies.
    let mut at = 8;
    while at < bytes.len() {
        let begin = at;
        let id = bytes[at];
        at += 1;
        let mut len = 0usize;
        let mut shift = 0;
        loop {
            let b = bytes[at];
            at += 1;
            len |= ((b & 0x7f) as usize) << shift;
            if b & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        if id == 10 {
            at = begin;
            break;
        }
        at += len;
    }
    let mut bad = bytes.clone();
    let index = export_index(&bytes, "_start");
    assert!(index < 128);
    bad.splice(at..at, [8, 1, index]);
    let error = VartaRuntime::new().validate_module(&bad).unwrap_err();
    assert!(error.to_string().contains("Wasm start function"), "{error}");
    let small = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 5 {
            assert_eq!(data[0], 1);
            assert_eq!(data[1], 1);
            assert!(data[2] < 128);
            data[2] = 1;
        }
        vec![data]
    });
    assert!(VartaRuntime::new().validate_module(&small).is_err());
    let mut pending = None;
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        if let wasmparser::Payload::ExportSection(reader) = payload.unwrap() {
            for e in reader {
                let e = e.unwrap();
                if e.name == varta_common::exceptions::PENDING {
                    pending = Some(e.index);
                }
            }
        }
    }
    let mut bad = bytes.clone();
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        if let wasmparser::Payload::GlobalSection(reader) = payload.unwrap() {
            for (index, g) in reader.into_iter().enumerate() {
                let g = g.unwrap();
                if Some(index as u32) == pending {
                    let at = g.init_expr.get_binary_reader().original_position();
                    assert_eq!(bytes[at], 0x41);
                    assert_eq!(bytes[at + 1], 0);
                    bad[at + 1] = 1;
                }
            }
        }
    }
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bad).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let error = execution.instantiate(&module).unwrap_err();
    assert!(
        error.to_string().contains("initial exception state"),
        "{error}"
    );
    assert!(execution.store.data().output.lines.is_empty());
}
