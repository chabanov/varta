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
