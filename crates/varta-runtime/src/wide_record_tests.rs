//! Value15 wide records (v34 `wide_records`): records wider than 32 fields use a
//! multiword presence bitmap and trace through the edge bitmap that trails their
//! descriptor. Node 24 produced the pinned logs of the Node-comparable case.
use super::*;
use varta_common::{language::LanguageProfile as P, value};

const V34: P = P::CheckedF64Utf16V34;

fn compile(source: &str, profile: P) -> Result<Vec<u8>, varta_compiler::CompilerError> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)?;
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    Ok(bytes)
}
fn logs(source: &str) -> Vec<String> {
    let bytes = compile(source, V34).unwrap_or_else(|e| panic!("{e}\n{source}"));
    VartaRuntime::new()
        .run_wasm_bytes(&bytes, &[])
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
        .lines
}
fn assert_logs(source: &str, expected: &[&str]) {
    assert_eq!(
        logs(source),
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
}
fn wide_interface(count: usize) -> String {
    let fields: String = (0..count).map(|i| format!("f{i}:number;")).collect();
    format!("interface Wide{{{fields}}}")
}
fn wide_literal(count: usize) -> String {
    let fields: Vec<_> = (0..count).map(|i| format!("f{i}:{i}")).collect();
    format!("{{{}}}", fields.join(","))
}

#[test]
fn sixty_field_records_match_node() {
    let expected_json = r#"{"7":7,"f00":"s0x","f01":110,"f02":false,"f04":["a4","bx"],"f05":"s5x","f06":6.5,"f07":true,"f08":"n8","f09":["a9","bx"],"f11":11.5,"f12":false,"f13":null,"f14":["a14","bx"],"f15":"s15x","f16":16.5,"f18":"n18","f19":["a19","bx"],"f20":"s20x","f21":21.5,"f22":false,"f23":null,"f25":"s25x","f26":26.5,"f27":true,"f28":"n28","f29":["a29","bx"],"f30":"s30x","f32":false,"f33":null,"f34":["a34","bx"],"f35":"s35x","f36":36.5,"f37":true,"f39":["a39","bx"],"f40":"s40x","f41":41.5,"f42":false,"f43":null,"f44":["a44","bx"],"f46":46.5,"f47":true,"f48":"n48","f49":["a49","bx"],"f50":"s50x","f51":51.5,"f53":null,"f54":["a54","bx"],"f55":"s55x","f56":56.5,"f57":true,"f58":"n58","f59":["late"],"f45":"opt","f03":"three"}"#;
    assert_logs(
        include_str!("value15_cases/wide.ts"),
        &[
            "52 true",
            "55 true",
            expected_json,
            "53 7 f00 f59",
            "110 n58 2",
            "s0x true 51 true",
            "671",
            "s55x",
        ],
    );
}

#[test]
fn wide_records_parse_and_round_trip_checked_json() {
    assert_logs(
        include_str!("value15_cases/wide_json.ts"),
        &[
            "39 33 true",
            "39 false",
            r#"37,"w39":39,"w38":"set"}"#,
            "true 40",
        ],
    );
}

#[test]
fn wide_record_ceiling_is_a_value15_layout_limit() {
    let max = value::MAX_WIDE_RECORD_FIELDS;
    let at_limit = format!(
        "{}export function main():void{{const w:Wide={};console.log(w.f{});console.log(JSON.stringify(w).length);}}",
        wide_interface(max),
        wide_literal(max),
        max - 1
    );
    let json_len = format!(
        "{{{}}}",
        (0..max)
            .map(|i| format!("\"f{i}\":{i}"))
            .collect::<Vec<_>>()
            .join(",")
    )
    .len();
    assert_eq!(
        logs(&at_limit),
        [
            format!("varta log: {}", max - 1),
            format!("varta log: {json_len}")
        ]
    );
    let over = format!(
        "{}export function main():void{{const w:Wide={};console.log(w.f0);}}",
        wide_interface(max + 1),
        wide_literal(max + 1)
    );
    let error = compile(&over, V34).unwrap_err().to_string();
    assert!(
        error.contains("object field count exceeds limit"),
        "{error}"
    );
    // Earlier profiles keep the frozen 32-field value13 ceiling.
    let wide = format!(
        "{}export function main():void{{const w:Wide={};console.log(w.f32);}}",
        wide_interface(value::MAX_RECORD_FIELDS + 1),
        wide_literal(value::MAX_RECORD_FIELDS + 1)
    );
    assert!(compile(&wide, V34).is_ok());
    for profile in [P::CheckedF64Utf16V33, P::CheckedF64Utf16V31] {
        let error = compile(&wide, profile).unwrap_err().to_string();
        assert!(
            error.contains("object field count exceeds limit"),
            "{profile:?}: {error}"
        );
    }
    let literal = format!(
        "export function main():void{{const w={};console.log(w.f0);}}",
        wide_literal(value::MAX_RECORD_FIELDS + 1)
    );
    assert!(compile(&literal, V34).is_ok());
    let error = compile(&literal, P::CheckedF64Utf16V33)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("object field count exceeds limit"),
        "{error}"
    );
}

#[test]
fn wide_records_survive_collection_through_their_edge_bitmap() {
    // Reference fields beyond slot 32 (and across the second presence word)
    // must be traced: allocate garbage after building the records, then read.
    let fields: String = (0..70)
        .map(|i| {
            if i % 3 == 0 {
                format!("r{i}:{{v:string}};")
            } else {
                format!("n{i}:number;")
            }
        })
        .collect();
    let literal: Vec<_> = (0..70)
        .map(|i| {
            if i % 3 == 0 {
                format!("r{i}:{{v:'v'+String({i})+tag}}")
            } else {
                format!("n{i}:{i}")
            }
        })
        .collect();
    let source = format!(
        "interface W{{{fields}}}\
         function make(tag:string):W{{return {{{}}};}}\
         export function main():void{{\
         const ws:W[]=[];for(let i=0;i<4;i++){{ws.push(make(String(i)));}}\
         for(let i=0;i<300;i++){{const t:{{v:string}}={{v:'garbage-'+String(i)}};if(t.v.length<0){{return;}}}}\
         const w=ws[3];if(w===undefined){{return;}}\
         console.log(w.r0.v+' '+w.r33.v+' '+w.r63.v+' '+w.r66.v+' '+w.r69.v+' '+String(w.n68));}}",
        literal.join(",")
    );
    assert_logs(&source, &["v03 v333 v633 v663 v693 68"]);
}

#[test]
fn wide_record_descriptors_carry_an_exact_edge_bitmap() {
    let source = format!(
        "{}export function main():void{{const w:Wide={};console.log(JSON.stringify(w).length);}}",
        wide_interface(40),
        wide_literal(40)
    );
    let bytes = compile(&source, V34).unwrap();
    let runtime = VartaRuntime::new();
    assert!(runtime.run_wasm_bytes(&bytes, &[]).is_ok());
    // A 40-field Number record has an all-zero two-word edge bitmap after its
    // descriptor entries; marking a Number slot as an edge must be rejected.
    let mut header = value::DYNAMIC_VALUE_VERSION.to_le_bytes().to_vec();
    header.extend(40u32.to_le_bytes());
    let at = bytes
        .windows(header.len())
        .position(|w| w == header.as_slice())
        .expect("wide descriptor");
    let edges = at + value::RECORD_DESCRIPTOR_HEADER + 40 * value::RECORD_DESCRIPTOR_ENTRY;
    assert_eq!(bytes[edges..edges + 8], [0; 8]);
    let mut corrupt = bytes.clone();
    corrupt[edges + 4] = 1;
    let error = runtime.run_wasm_bytes(&corrupt, &[]).unwrap_err();
    assert!(error.to_string().contains("invalid value ABI"), "{error}");
}
