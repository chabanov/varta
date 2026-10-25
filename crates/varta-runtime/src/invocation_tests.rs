use super::*;
use crate::contract_tests::{export_index, replace_catalog, rewrite};
use varta_common::{ToolInvocationCode as Code, ToolInvocationError, ToolType, ToolValue};

fn compile(body: &str) -> Vec<u8> {
    varta_compiler::compile_typescript(&format!(
        "{body}\nexport function main():void{{varta.process.exit(99);}}"
    ))
    .unwrap()
}
fn invoke(
    bytes: &[u8],
    name: &str,
    json: &[u8],
) -> Result<varta_common::ToolInvocationOutput, ToolInvocationError> {
    VartaRuntime::new().invoke_tool(bytes, name, json, &[])
}

#[test]
fn invocation_transfers_named_primitives_and_never_calls_main() {
    let bytes = compile(
        r#"
export function combine(left:string,n:number,right:string,flag:boolean):string{
 if(n!=42){return "WRONG_ORDER";}if(flag){return left+right;}return left;
}
export function empty():void{console.log("empty");}
export function zero(n:number):number{return n;}
export function echoBool(value:boolean):boolean{return value;}
varta.tool({name:"combine",description:"Combine"},combine);
varta.tool({name:"empty",description:"Void"},empty);
varta.tool({name:"zero",description:"Integer"},zero);
varta.tool({name:"flag",description:"Boolean"},echoBool);
"#,
    );
    let runtime = VartaRuntime::new();
    for flag in [true, false] {
        let json = serde_json::to_vec(
            &serde_json::json!({"right":"🦀\0!","flag":flag,"n":42,"left":"Привіт"}),
        )
        .unwrap();
        let output = runtime.invoke_tool(&bytes, "combine", &json, &[]).unwrap();
        assert_eq!(
            output.result.value,
            ToolValue::String(
                if flag {
                    "Привіт🦀\0!"
                } else {
                    "Привіт"
                }
                .into()
            )
        );
        assert!(output.logs.is_empty());
    }
    let empty = invoke(&bytes, "empty", b"{}").unwrap();
    assert_eq!(
        serde_json::to_value(&empty.result).unwrap(),
        serde_json::json!({"value":null})
    );
    assert_eq!(empty.logs, ["varta log: empty"]);
    for n in [i32::MIN, 0, i32::MAX] {
        assert_eq!(
            invoke(&bytes, "zero", format!("{{\"n\":{n}}}").as_bytes())
                .unwrap()
                .result
                .value,
            ToolValue::Integer(n)
        );
    }
    assert_eq!(
        invoke(&bytes, "flag", b"{\"value\":false}")
            .unwrap()
            .result
            .value,
        ToolValue::Boolean(false)
    );
}

#[test]
fn invalid_arguments_and_unpublished_names_fail_before_any_handler_executes() {
    let bytes = compile(
        r#"
export function exit(title:string,n:number):string{varta.process.exit(17);return title;}
export function privateFn():void{varta.process.exit(18);}
varta.tool({name:"exit",description:"Exit"},exit);
"#,
    );
    let grants = [CapabilityGrant {
        id: "process.exit".into(),
        value: varta_common::CapabilityValue::Boolean(true),
    }];
    let runtime = VartaRuntime::new();
    for json in [
        b"{}".as_slice(),
        b"{\"title\":\"x\",\"n\":1.5}",
        b"{\"title\":\"x\",\"n\":1,\"n\":2}",
        b"{\"title\":\"x\",\"n\":1,\"extra\":0}",
        b"[]",
        b"null",
        b"{}{}",
    ] {
        assert_eq!(
            runtime
                .invoke_tool(&bytes, "exit", json, &grants)
                .unwrap_err()
                .code(),
            Code::InvalidArguments
        );
    }
    for name in [
        "privateFn",
        "main",
        "missing",
        "varta.tool.exit",
        "varta.tool_abi.copy_string",
    ] {
        assert_eq!(
            runtime
                .invoke_tool(&bytes, name, b"{}", &grants)
                .unwrap_err()
                .code(),
            Code::UnknownTool
        );
    }
    assert_eq!(
        runtime
            .invoke_tool(&bytes, "exit", b"{\"title\":\"x\",\"n\":1}", &grants)
            .unwrap_err()
            .code(),
        Code::GuestExit
    );
    let large = vec![b' '; abi::MAX_INPUT_BYTES + 1];
    assert_eq!(
        invoke(&bytes, "exit", &large).unwrap_err().code(),
        Code::InvalidArguments
    );
    let legacy = varta_compiler::compile_typescript("export function main():void{}").unwrap();
    assert_eq!(
        invoke(&legacy, "main", b"{}").unwrap_err().code(),
        Code::UnknownTool
    );
}

#[test]
fn tool_calls_keep_capability_ceilings_and_guest_exit_separate_from_values() {
    let bytes = compile(
        r#"
export function exit():number{varta.process.exit(17);return varta.result.status();}
varta.tool({name:"exit",description:"Exit"},exit);
"#,
    );
    let yes = [CapabilityGrant {
        id: "process.exit".into(),
        value: varta_common::CapabilityValue::Boolean(true),
    }];
    let runtime = VartaRuntime::new();
    assert!(matches!(
        runtime.invoke_tool(&bytes, "exit", b"{}", &yes),
        Err(ToolInvocationError::GuestExit(17))
    ));
    for (requested, ceiling) in [(&yes[..], &[][..]), (&[][..], &yes[..])] {
        assert_eq!(
            runtime
                .invoke_tool_with_policy(&bytes, "exit", b"{}", requested, ceiling)
                .unwrap()
                .result
                .value,
            ToolValue::Integer(1)
        );
    }
    let invalid = [CapabilityGrant {
        id: "process.unknown".into(),
        value: varta_common::CapabilityValue::Boolean(true),
    }];
    assert_eq!(
        runtime
            .invoke_tool(&bytes, "exit", b"{}", &invalid)
            .unwrap_err()
            .code(),
        Code::InvalidPolicy
    );
}

#[test]
fn transfer_roots_survive_buffer_reuse_host_calls_and_collection() {
    let bytes = compile(
        r#"
function churn(a:string,b:string):void{for(let i=0;i<8;i=i+1){let temporary=a+b;}}
export function join(a:string,b:string):string{
 varta.env.get("VARTA_DENIED_TRANSFER");churn(a,b);return a+b;
}
varta.tool({name:"join",description:"Join"},join);
"#,
    );
    let a = "A".repeat(28 * 1024);
    let b = "Привіт🦀".repeat(1000);
    let json = serde_json::to_vec(&serde_json::json!({"a":a,"b":b})).unwrap();
    let output = invoke(&bytes, "join", &json).unwrap();
    assert_eq!(output.result.value, ToolValue::String(a + &b));
    assert!(
        output
            .logs
            .iter()
            .any(|line| line.contains("capability denied"))
    );
}

#[test]
fn execution_and_serialized_output_remain_bounded_and_calls_recover_after_failure() {
    let bytes = compile(
        r#"
export function hang():void{while(true){}}
export function logs(s:string):void{for(let i=0;i<2000;i=i+1){console.log(s);}}
export function grow(s:string):string{let value=s;for(let i=0;i<6;i=i+1){value=value+value;}return value;}
export function ok():number{return 42;}
varta.tool({name:"hang",description:"Hang"},hang);
varta.tool({name:"logs",description:"Output"},logs);
varta.tool({name:"grow",description:"Grow"},grow);
varta.tool({name:"ok",description:"OK"},ok);
"#,
    );
    let runtime = VartaRuntime::new();
    let error = runtime.invoke_tool(&bytes, "hang", b"{}", &[]).unwrap_err();
    assert_eq!(error.code(), Code::ExecutionFailed);
    assert!(error.to_string().contains("fuel"));
    let logs = serde_json::to_vec(&serde_json::json!({"s":"x".repeat(1024)})).unwrap();
    assert_eq!(
        runtime
            .invoke_tool(&bytes, "logs", &logs, &[])
            .unwrap_err()
            .code(),
        Code::ExecutionFailed
    );
    let ascii = serde_json::to_vec(&serde_json::json!({"s":"x".repeat(4095)})).unwrap();
    let result = runtime.invoke_tool(&bytes, "grow", &ascii, &[]).unwrap();
    assert!(serde_json::to_vec(&result.result).unwrap().len() < abi::MAX_OUTPUT_BYTES);
    let escaped = serde_json::to_vec(&serde_json::json!({"s":"\0".repeat(4095)})).unwrap();
    assert_eq!(
        runtime
            .invoke_tool(&bytes, "grow", &escaped, &[])
            .unwrap_err()
            .code(),
        Code::InvalidResult
    );
    assert_eq!(
        runtime
            .invoke_tool(&bytes, "ok", b"{}", &[])
            .unwrap()
            .result
            .value,
        ToolValue::Integer(42)
    );
}

#[test]
fn hostile_same_signature_results_cannot_escape_type_and_memory_checks() {
    let bytes = compile(
        "export function bad():number{return 2;}varta.tool({name:'bad',description:'Bad'},bad);",
    );
    let mut catalog = VartaRuntime::new()
        .validate_module_and_tools(&bytes)
        .unwrap()
        .1
        .unwrap();
    catalog.tools[0].result = ToolType::Boolean;
    let bad = replace_catalog(&bytes, &catalog.to_bytes().unwrap());
    assert_eq!(
        invoke(&bad, "bad", b"{}").unwrap_err().code(),
        Code::InvalidResult
    );

    let bytes = compile(
        "export function text():string{return 'A';}varta.tool({name:'text',description:'Text'},text);",
    );
    let invalid_utf8 = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 11 {
            assert_eq!(data.last(), Some(&b'A'));
            *data.last_mut().unwrap() = 0xff;
        }
        vec![data]
    });
    assert_eq!(
        invoke(&invalid_utf8, "text", b"{}").unwrap_err().code(),
        Code::InvalidResult
    );
    // Replace the first packed literal with an equal-width out-of-bounds slice.
    let mut bad = bytes.clone();
    'outer: for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        if let wasmparser::Payload::CodeSectionEntry(body) = payload.unwrap() {
            let mut reader = body.get_operators_reader().unwrap();
            while !reader.eof() {
                let start = reader.original_position();
                if matches!(reader.read().unwrap(),wasmparser::Operator::I64Const{value} if value==1_i64<<i32::BITS)
                {
                    assert_eq!(reader.original_position() - start, 6);
                    bad[start + 1..start + 6].copy_from_slice(&[0xff, 0xff, 0xff, 0xff, 0x1f]);
                    break 'outer;
                }
            }
        }
    }
    assert_eq!(
        invoke(&bad, "text", b"{}").unwrap_err().code(),
        Code::InvalidResult
    );
}

#[test]
fn corrupted_transfer_helpers_are_rejected_before_the_tool_receives_arguments() {
    let bytes = compile(
        r#"
export function badCopy(p:number,len:number):string{return "";}
export function echo(text:string):string{return text;}
varta.tool({name:"badCopy",description:"Bad"},badCopy);
varta.tool({name:"echo",description:"Echo"},echo);
"#,
    );
    let index = export_index(&bytes, "varta.tool.badCopy");
    let bad = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            let name = varta_common::TOOL_COPY_STRING.as_bytes();
            let start = data
                .windows(name.len())
                .position(|part| part == name)
                .unwrap();
            let slot = start + name.len() + 1;
            assert!(data[slot] < 128 && index < 128);
            data[slot] = index;
        }
        vec![data]
    });
    let error = invoke(&bad, "echo", b"{\"text\":\"hello\"}").unwrap_err();
    assert_eq!(error.code(), Code::InvalidResult);
    assert!(error.to_string().contains("transfer changed"));
}

#[test]
fn shared_execution_setup_preserves_start_section_exit_logs_for_normal_runs() {
    let bytes = varta_compiler::compile_typescript(
        "export function main():void{console.log(7);varta.process.exit(17);}",
    )
    .unwrap();
    let index = export_index(&bytes, "_start");
    let mut with_start = bytes.clone();
    let mut before = 8;
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        if let Some((id, range)) = payload.unwrap().as_section() {
            if id == 10 {
                with_start.splice(before..before, [8, 1, index]);
                break;
            }
            before = range.end;
        }
    }
    let yes = [CapabilityGrant {
        id: "process.exit".into(),
        value: varta_common::CapabilityValue::Boolean(true),
    }];
    let output = VartaRuntime::new()
        .run_wasm_bytes(&with_start, &yes)
        .unwrap();
    assert_eq!(output.exit_code, 17);
    assert_eq!(output.lines, ["varta log: 7"]);
}
