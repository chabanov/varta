use super::*;
const HANDLER: &str = "function handle(r:VartaHttpRequest):VartaHttpResponse{return {body:r.target,status:200,headers:[]};}export function main():void{}";
#[test]
fn service_registration_rejects_unsupported_options_placements_and_signatures() {
    for declaration in [
        "varta.http.serve({hostname:\"localhost\",port:80},handle);",
        "varta.http.serve({hostname:\"127.0.0.1\",port:-1},handle);",
        "varta.http.serve({hostname:\"127.0.0.1\",port:65536},handle);",
        "varta.http.serve({hostname:\"127.0.0.1\",port:80,extra:true},handle);",
        "varta.http.serve({hostname:\"127.0.0.1\",port:80,port:81},handle);",
        "varta.http.serve({hostname:\"127.0.0.1\"},handle);",
        "varta.http.serve({hostname:\"127.0.0.1\",port:80},main);",
        "varta.http.serve({hostname:\"127.0.0.1\",port:80},unknown);",
        "varta.http.serve({hostname:\"127.0.0.1\",port:0},handle);varta.http.serve({hostname:\"127.0.0.1\",port:0},handle);",
    ] {
        let error = compile_typescript(&format!("{HANDLER}{declaration}")).unwrap_err();
        assert!(error.diagnostic().is_some(), "{error}");
    }
    assert!(
        compile_typescript(
            "export function main():void{varta.http.serve({hostname:'127.0.0.1',port:0},main);}"
        )
        .is_err()
    );
    assert!(compile_typescript("function handle(s:string):string{return s;}export function main():void{}varta.http.serve({hostname:'127.0.0.1',port:0},handle);").is_err());
}
#[test]
fn http_builtins_cannot_be_shadowed_and_wrapper_fits_the_user_function_budget() {
    assert!(
        compile_typescript("interface VartaHttpRequest{bad:number}export function main():void{}")
            .unwrap_err()
            .to_string()
            .contains("built-in")
    );
    let mut source = String::new();
    for i in 0..varta_common::abi::MAX_USER_FUNCTIONS - 2 {
        source.push_str(&format!("function f{i}():void{{}}"));
    }
    source.push_str(HANDLER);
    source.push_str("varta.http.serve({hostname:'127.0.0.1',port:0},handle);");
    compile_typescript(&source).unwrap();
}
