use super::*;
use crate::contract_tests::{export_index, rewrite};
use varta_common::http::{self, HttpContract};
const SOURCE: &str = r#"
function handle(r:VartaHttpRequest):VartaHttpResponse{console.log(r.target);varta.env.get("NAME");return {status:200,body:r.body,headers:[]};}
export function main():void{}
varta.http.serve({hostname:"127.0.0.1",port:0},handle);
"#;
fn replace(bytes: &[u8], value: &[u8]) -> Vec<u8> {
    rewrite(bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 0 && data.get(1..1 + http::SECTION.len()) == Some(http::SECTION.as_bytes()) {
            data.truncate(1 + http::SECTION.len());
            data.extend(value);
        }
        vec![data]
    })
}
#[test]
fn http_metadata_signatures_and_effects_are_validated_without_running_guest_code() {
    let bytes = varta_compiler::compile_typescript(SOURCE).unwrap();
    let runtime = VartaRuntime::new();
    let inspection = runtime.inspect_module(&bytes).unwrap();
    assert!(inspection.tool_catalog.is_none());
    let mut contract = inspection.http_service.unwrap();
    assert_eq!(contract.required_effects, ["env.get", "net.listen"]);
    contract.required_effects = vec!["net.listen".into()];
    let bad = replace(&bytes, &contract.to_bytes().unwrap());
    assert!(
        runtime
            .validate_module(&bad)
            .unwrap_err()
            .to_string()
            .contains("effect mismatch")
    );
    for action in 0..3 {
        let bad = rewrite(&bytes, |id, data| {
            let http =
                id == 0 && data.get(1..1 + http::SECTION.len()) == Some(http::SECTION.as_bytes());
            match (action, http) {
                (0, true) => vec![],
                (1, true) => vec![data.to_vec(), data.to_vec()],
                _ => {
                    let mut data = data.to_vec();
                    if action == 2 && id == 7 {
                        let at = data
                            .windows(http::HANDLER.len())
                            .position(|s| s == http::HANDLER.as_bytes())
                            .unwrap();
                        data[at + http::HANDLER.len() + 1] = export_index(&bytes, "_start");
                    }
                    vec![data]
                }
            }
        });
        assert!(runtime.validate_module(&bad).is_err(), "{action}");
    }
    let mut value: serde_json::Value =
        serde_json::from_slice(&contract.to_bytes().unwrap()).unwrap();
    value["limits"]["connections"] = serde_json::json!(2);
    assert!(HttpContract::parse(&serde_json::to_vec(&value).unwrap()).is_err());
}
#[test]
fn http_input_transfer_cannot_call_hidden_host_imports() {
    let source =
        format!("function fake(p:number,n:number):string{{console.log(p);return \"\";}}{SOURCE}");
    let bytes = varta_compiler::compile_typescript(&source).unwrap();
    let fake = export_index(&bytes, "_start") - 2;
    let bad = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            let at = data
                .windows(http::COPY.len())
                .position(|s| s == http::COPY.as_bytes())
                .unwrap();
            data[at + http::COPY.len() + 1] = fake;
        }
        vec![data]
    });
    assert!(
        VartaRuntime::new()
            .validate_module(&bad)
            .unwrap_err()
            .to_string()
            .contains("cannot call host imports")
    );
}
#[test]
fn named_invocation_never_starts_an_http_listener_or_runs_main() {
    let source = format!(
        "{SOURCE}export function echo(text:string):string{{return text;}}varta.tool({{name:'echo',description:'Echo'}},echo);"
    );
    let bytes = varta_compiler::compile_typescript(&source).unwrap();
    let result = VartaRuntime::new()
        .invoke_tool(&bytes, "echo", br#"{"text":"ok"}"#, &[])
        .unwrap();
    assert_eq!(
        result.result.value,
        varta_common::ToolValue::String("ok".into())
    );
    assert!(result.logs.is_empty());
}

#[test]
fn http_v2_is_profile_bound_and_legacy_metadata_is_not_reinterpreted() {
    use varta_common::language::LanguageProfile as P;
    let runtime = VartaRuntime::new();
    let bytes =
        varta_compiler::compile_typescript_with_profile(SOURCE, P::CheckedF64Utf16V38).unwrap();
    let mut contract = runtime
        .inspect_module(&bytes)
        .unwrap()
        .http_service
        .unwrap();
    assert_eq!(contract.version, http::UTF16_VERSION);
    contract.version = http::VERSION;
    assert!(
        runtime
            .validate_module(&replace(&bytes, &contract.to_bytes().unwrap()))
            .is_err()
    );
    let old = varta_compiler::compile_typescript(SOURCE).unwrap();
    contract = runtime.inspect_module(&old).unwrap().http_service.unwrap();
    assert_eq!(contract.version, http::VERSION);
    contract.version = http::UTF16_VERSION;
    assert!(
        runtime
            .validate_module(&replace(&old, &contract.to_bytes().unwrap()))
            .is_err()
    );
    assert!(
        varta_compiler::compile_typescript_with_profile(SOURCE, P::CheckedF64Utf16V37).is_err()
    );
}

#[test]
fn async_http_v2_effects_and_completion_helpers_are_checked_before_execution() {
    use varta_common::language::LanguageProfile as P;
    let source = r#"
        function fakeState(p:Promise<string>):number {varta.env.get("HIDDEN_STATE");return 1;}
        function fakeValue(p:Promise<string>):string {varta.env.get("HIDDEN_VALUE");return "";}
        async function handle(r:VartaHttpRequest):Promise<VartaHttpResponse> {
            await null;varta.env.get("HTTP_ASYNC");return {status:200,body:r.body,headers:[]};
        }
        export function main():void{const p=Promise.resolve("");fakeState(p);fakeValue(p);}
        varta.http.serve({hostname:"127.0.0.1",port:0},handle);
    "#;
    let bytes =
        varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V38).unwrap();
    let runtime = VartaRuntime::new();
    let mut contract = runtime
        .inspect_module(&bytes)
        .unwrap()
        .http_service
        .unwrap();
    assert!(contract.asynchronous);
    assert_eq!(contract.required_effects, ["env.get", "net.listen"]);
    contract.required_effects = vec!["net.listen".into()];
    let error = runtime
        .validate_module(&replace(&bytes, &contract.to_bytes().unwrap()))
        .unwrap_err();
    assert!(error.to_string().contains("effect mismatch"), "{error}");
    for (name, relative) in [(http::STATE, 3), (http::VALUE, 2)] {
        let target = u32::from(export_index(&bytes, "_start")) - relative;
        let bad = rewrite(&bytes, |id, section| {
            let mut section = section.to_vec();
            if id == 7 {
                let start = section
                    .windows(name.len())
                    .position(|s| s == name.as_bytes())
                    .unwrap()
                    + name.len()
                    + 1;
                let mut end = start;
                while section[end] & 0x80 != 0 {
                    end += 1;
                }
                end += 1;
                let mut index = target;
                let mut encoded = Vec::new();
                loop {
                    let byte = (index & 0x7f) as u8;
                    index >>= 7;
                    encoded.push(byte | if index == 0 { 0 } else { 0x80 });
                    if index == 0 {
                        break;
                    }
                }
                section.splice(start..end, encoded);
            }
            vec![section]
        });
        let error = runtime.validate_module(&bad).unwrap_err();
        assert!(
            error.to_string().contains("cannot call host imports"),
            "{name}: {error}"
        );
    }
    let mut contract = runtime
        .inspect_module(&bytes)
        .unwrap()
        .http_service
        .unwrap();
    contract.asynchronous = false;
    assert!(
        runtime
            .validate_module(&replace(&bytes, &contract.to_bytes().unwrap()))
            .is_err()
    );
}
