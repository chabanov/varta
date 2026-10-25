use super::*;
use std::path::Path;

#[test]
fn captures_require_one_direct_privileged_call_and_immutable_typed_fields() {
    for body in [
        "varta.result.capture();",
        "varta.result.capture(varta.time.now(),varta.time.now());",
        "varta.result.capture(1);",
        "varta.result.capture(console.log(1));",
        "varta.result.capture(varta.result.status());",
        "varta.result.capture(varta.result.capture(varta.time.now()));",
        "varta.result.capture(wrapped());",
        "const r=varta.result.capture(varta.time.now());r.ok=false;",
        "const r=varta.result.capture(varta.time.now());console.log(r.missing);",
        "const r=varta.result.capture(varta.time.now());console.log(r['status']);",
        "const r=varta.result.capture(varta.time.now());console.log(r);",
        "const r=varta.result.capture(varta.time.now());if(r){}",
        "const r=varta.result.capture(varta.time.now());console.log(r+1);",
        "let r=varta.result.capture(varta.time.now());r=varta.result.capture(varta.env.get('X'));",
        "const r:VartaResult<string>=varta.result.capture(varta.time.now());",
        "const r:VartaResult<number>={ok:true,status:0,value:1};",
        "const r:VartaResult<any>=varta.result.capture(varta.time.now());",
        "const r:VartaResult<VartaResult<number>>=varta.result.capture(varta.time.now());",
        "const r:VartaResult=varta.result.capture(varta.time.now());",
        "const r:VartaResult<string,number>=varta.result.capture(varta.time.now());",
        "const r:Other<number>=varta.result.capture(varta.time.now());",
        "const r=varta.result.capture(varta.fs.print('x'));console.log(r.value);",
    ] {
        let source = format!(
            "function wrapped():number{{return varta.time.now();}} export function main():void{{{body}}}"
        );
        let error = compile_typescript_with_path(&source, Path::new("results.ts")).unwrap_err();
        assert_eq!(error.diagnostic().unwrap().file, "results.ts", "{body}");
    }
    for declaration in [
        "interface VartaResult<T>{value:T;}",
        "type VartaResult<T> = T;",
        "function wrong(r:VartaResult<string>):VartaResult<number>{return r;}",
        "function wrong(r:VartaResult<string>):void{} function invoke():void{wrong(varta.result.capture(varta.time.now()));}",
    ] {
        assert!(
            compile_typescript(&format!("{declaration} export function main():void{{}}")).is_err(),
            "{declaration}"
        );
    }
}

#[test]
fn result_property_diagnostics_point_to_the_property_and_bad_payload() {
    let source = "export function main():void{const r=varta.result.capture(varta.time.now());console.log(r.missing);}";
    let error = compile_typescript(source).unwrap_err();
    let diagnostic = error.diagnostic().unwrap();
    assert_eq!(
        &source[diagnostic.range.start.byte_offset..diagnostic.range.end.byte_offset],
        "missing"
    );
    let source = "export function main():void{const r=varta.result.capture(varta.time.now());const s:string=r.value;}";
    let error = compile_typescript(source).unwrap_err();
    let diagnostic = error.diagnostic().unwrap();
    assert_eq!(diagnostic.code, varta_common::DiagnosticCode::TypeError);
    assert_eq!(
        &source[diagnostic.range.start.byte_offset..diagnostic.range.end.byte_offset],
        "r.value"
    );
}

#[test]
fn result_parameter_expansion_preserves_the_existing_wasm_complexity_limit() {
    for (count, valid) in [(16, true), (17, false)] {
        let params = (0..count)
            .map(|i| format!("r{i}:VartaResult<string>"))
            .collect::<Vec<_>>()
            .join(",");
        let source = format!("function f({params}):void{{}} export function main():void{{}}");
        assert_eq!(compile_typescript(&source).is_ok(), valid);
    }
}
