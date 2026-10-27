use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V1)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
#[test]
fn utf16_literals_units_order_and_numeric_positions_follow_the_selected_profile() {
    let output = run(r#"
function text():string{console.log("once");return "😀";}
function position():number{console.log("index");return 1;}
export function main():void {
 const s="A😀é";console.log(s.length);console.log(s.charCodeAt(1));console.log(s.charCodeAt(2));
 console.log(JSON.stringify(s.charAt(1)));console.log(JSON.stringify(s.charAt(2)));
 console.log("\ud83d"+"\ude00"==="😀");console.log("\ud800"<"\ue000");
 console.log("\u0100">"\u00ff");console.log("\ud800"<"\ud801");console.log("\ud800"<"\ud800a");
 console.log(JSON.stringify(""));console.log("".length);console.log(s.charAt());
 console.log(s.charAt(NaN));console.log(s.charAt(-0.5));console.log(s.charAt(3.9));
 console.log(JSON.stringify(s.charAt(-1)));console.log(JSON.stringify(s.charAt(Infinity)));
 console.log(s.charCodeAt(-Infinity));console.log(s.charCodeAt(4));console.log("".charCodeAt());
 console.log(text().charAt(0).charCodeAt());console.log(typeof undefined);
 console.log(JSON.stringify(text().charAt(position())));
}"#);
    assert_eq!(
        output.lines,
        [
            "4",
            "55357",
            "56832",
            r#""\ud83d""#,
            r#""\ude00""#,
            "1",
            "1",
            "1",
            "1",
            "1",
            r#""""#,
            "0",
            "A",
            "A",
            "A",
            "é",
            r#""""#,
            r#""""#,
            "NaN",
            "NaN",
            "NaN",
            "once",
            "55357",
            "undefined",
            "once",
            "index",
            r#""\ude00""#
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn utf16_json_preserves_surrogates_in_values_and_keeps_checked_record_keys() {
    let output = run(r#"export function main():void {
 const value={"é": "\ud800",n:1.25,items:["\udfff","😀","a\u0000b"],u:undefined};
 const encoded=JSON.stringify(value);console.log(encoded);
 const copy=varta.json.parse<{é:string;n:number;items:string[]}>(encoded);
 console.log(copy.é.charCodeAt());console.log(copy.items[1].length);console.log(JSON.stringify(copy));
 console.log(JSON.stringify(varta.json.parse<string>('"\\ud800"')));
 console.log(JSON.stringify(varta.json.parse<string>('"'+'\ud800'+'"')));
 console.log(JSON.stringify(varta.json.parse<string>('"\\ud83d\\ude00"')));
 console.log(JSON.stringify(varta.json.parse<string>('""')));
 console.log(JSON.stringify([undefined,void 0]));console.log(JSON.stringify(undefined));
}"#);
    assert_eq!(
        output.lines,
        [
            r#"{"é":"\ud800","n":1.25,"items":["\udfff","😀","a\u0000b"]}"#,
            "55296",
            "2",
            r#"{"items":["\udfff","😀","a\u0000b"],"n":1.25,"é":"\ud800"}"#,
            r#""\ud800""#,
            r#""\ud800""#,
            r#""😀""#,
            r#""""#,
            "[null,null]",
            "undefined"
        ]
        .map(|s| format!("varta log: {s}"))
    );
}
#[test]
fn utf16_resource_conversion_rejects_lone_surrogates_and_nul_before_authority_or_io() {
    let output = run(r#"export function main():void {
 const a=varta.result.capture(varta.env.get("VARTA_UTF16_DENIED"));console.log(a.status);
 console.log(JSON.stringify("\ud800"));console.log("😀".charCodeAt());console.log(varta.result.status());
 const b=varta.result.capture(varta.env.has("VARTA\ud800"));console.log(b.status);
 const c=varta.result.capture(varta.env.print("VARTA\u0000NAME"));console.log(c.status);
 const d=varta.result.capture(varta.fs.read("\udfff"));console.log(d.status);
 const e=varta.result.capture(varta.net.fetch("http://localhost/\ud800"));console.log(e.status);
}"#);
    assert!(output.lines[0].contains("capability denied: env"));
    assert_eq!(
        &output.lines[1..5],
        [
            "varta log: 1",
            r#"varta log: "\ud800""#,
            "varta log: 55357",
            "varta log: 1"
        ]
    );
    assert_eq!(
        output.lines.iter().filter(|s| *s == "varta log: 2").count(),
        4
    );
    assert_eq!(
        output
            .lines
            .iter()
            .filter(|s| s.contains("capability denied"))
            .count(),
        1
    );
}
#[test]
fn utf16_roots_and_snapshots_survive_nested_calls_and_collection() {
    let source=r#"
function churn():string {const piece="PIECE";let text="";for(let i=0;i<190;i=i+1){const dead={text:piece+piece+"\ud800",items:["\udfff"+"a"]};text=dead.text;}return text;}
function keep(a:string,b:string):string{return a;}
export function main():void {
 const saved={text:"saved"+"\ud800",items:["\udfff"+"!"]};
 const first=keep("😀".charAt(0),churn());const second=keep(JSON.stringify(saved),churn());
 const parsed=varta.json.parse<{s:string;items:string[]}>('{"s":"\\ud800","items":["😀"]}');
 churn();console.log(JSON.stringify(first));console.log(second);console.log(JSON.stringify(parsed));
 console.log(saved.items[0].charCodeAt());
}"#.replace("PIECE", &"x".repeat(8192));
    // Force multiple sweeps at the production memory ceiling. This stress case
    // receives more fuel so collection, rather than the ordinary fuel cap, is tested.
    let bytes = compile(&source);
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution
        .store
        .set_fuel(200 * varta_common::abi::FUEL)
        .unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    let output = std::mem::take(&mut execution.store.data_mut().output);
    assert_eq!(
        output.lines,
        [
            r#"varta log: "\ud83d""#,
            r#"varta log: {"text":"saved\ud800","items":["\udfff!"]}"#,
            r#"varta log: {"items":["😀"],"s":"\ud800"}"#,
            "varta log: 57343"
        ]
    );
}
#[test]
fn utf16_requires_its_own_abi_imports_and_preserves_legacy_diagnostics() {
    use crate::contract_tests::rewrite;
    use varta_common::{abi, value};
    let bytes = compile("export function main():void{console.log(JSON.stringify({text:'ok'}));}");
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime.inspect_module(&bytes).unwrap().value_abi_version,
        Some(value::UTF16_VALUE_VERSION)
    );
    for name in [abi::SECTION, varta_common::language::SECTION] {
        let bad = rewrite(&bytes, |id, data| {
            if id == 0 && data.get(1..1 + name.len()) == Some(name.as_bytes()) {
                vec![]
            } else {
                vec![data.to_vec()]
            }
        });
        assert!(runtime.validate_module(&bad).is_err());
    }
    for (name, version) in [
        (abi::SECTION, abi::UNDEFINED_VERSION),
        (value::VALUE_SECTION, value::UNDEFINED_VALUE_VERSION),
    ] {
        let bad = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 0 && data.get(1..1 + name.len()) == Some(name.as_bytes()) {
                data.truncate(1 + name.len());
                data.extend(version.to_le_bytes());
            }
            vec![data]
        });
        assert!(runtime.run_wasm_bytes(&bad, &[]).is_err());
    }
    let bad = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 2 {
            let from = b"json_stringify_v5";
            let at = data.windows(from.len()).position(|s| s == from).unwrap();
            data[at..at + from.len()].copy_from_slice(b"json_stringify_v4");
        }
        vec![data]
    });
    assert!(runtime.validate_module(&bad).is_err());
    // An old string import under a UTF-16 declaration must not reinterpret bytes.
    let bytes = compile("export function main():void{console.log('ok');}");
    let bad = rewrite(&bytes, |id, data| {
        if id != 2 {
            return vec![data.to_vec()];
        }
        let mut data = data.to_vec();
        let at = data.windows(12).position(|s| s == b"log_string16").unwrap();
        data[at - 1] = 10;
        data.drain(at + 10..at + 12);
        vec![data]
    });
    assert!(runtime.validate_module(&bad).is_err());
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf8V1,
        LanguageProfile::CheckedF64Utf8V2,
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                r#"export function main():void{console.log("\ud800");}"#,
                profile
            )
            .is_err()
        );
    }
    for source in [
        r#"export function main():void{console.log("a"[0]);}"#,
        r#"export function main():void{console.log("a".charAt("0"));}"#,
        r#"export function main():void{console.log(varta.json.parse<string>('"\\'+ '\ud800'+'"'));}"#,
    ] {
        match varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V1,
        ) {
            Err(_) => {}
            Ok(wasm) => assert!(runtime.run_wasm_bytes(&wasm, &[]).is_err()),
        }
    }
}

#[test]
fn utf16_static_and_json_byte_limits_apply_after_encoding() {
    let text = "x".repeat(varta_common::abi::RESULT_BASE / 2 + 1);
    let source = format!("export function main():void{{console.log({text:?});}}");
    assert!(
        varta_compiler::compile_typescript_with_profile(
            &source,
            LanguageProfile::CheckedF64Utf16V1
        )
        .unwrap_err()
        .to_string()
        .contains("static string data")
    );
    assert!(
        varta_compiler::compile_typescript_with_profile(&source, LanguageProfile::CheckedF64Utf8V2)
            .is_ok()
    );
    for json in [
        r#"{"text":"\ud800","text":"x"}"#,
        r#"{"unknown":"\ud800"}"#,
        r#"{"text":null}"#,
        r#"{"text":"\ud800","extra":1}"#,
    ] {
        let source =
            format!("export function main():void{{varta.json.parse<{{text:string}}>({json:?});}}");
        assert!(
            VartaRuntime::new()
                .run_wasm_bytes(&compile(&source), &[])
                .is_err(),
            "{json}"
        );
    }
}
