use super::*;
use varta_common::{CapabilityValue, abi::HostStatus};

fn execute(source: &str, grants: &[CapabilityGrant]) -> RunOutput {
    let bytes = varta_compiler::compile_typescript(source).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, grants).unwrap()
}

#[test]
fn function_signatures_do_not_depend_on_declaration_order() {
    let output = execute(
        r#"
function tri(a:number,b:number,c:number):number { return a+b+c; }
function one(a:number):number { return a; }
function four(a:number,b:number,c:number,d:number):number { return a+b+c+d; }
export function main():void { console.log(tri(1,2,3)); console.log(one(4)); console.log(four(1,2,3,4)); }
"#,
        &[],
    );
    assert_eq!(
        output.lines,
        ["varta log: 6", "varta log: 4", "varta log: 10"]
    );
}

#[test]
fn lexical_scopes_switch_default_fallthrough_and_for_continue() {
    let output = execute(
        r#"
export function main():void {
 let x=1; { let x=2; console.log(x); } console.log(x);
 switch(1) { case 1: console.log(11); break; case 2: console.log(22); break; default: console.log(33); }
 switch(2) { default: console.log(44); case 2: console.log(55); break; }
 switch(9) { case 1: console.log(66); break; default: console.log(77); case 2: console.log(88); }
 for(let i=0;i<3;i=i+1) { if(i==1) { continue; } console.log(i); }
}
"#,
        &[],
    );
    assert_eq!(
        output.lines,
        [
            "varta log: 2",
            "varta log: 1",
            "varta log: 11",
            "varta log: 55",
            "varta log: 77",
            "varta log: 88",
            "varta log: 0",
            "varta log: 2"
        ]
    );
}

#[test]
fn nested_switch_temporaries_and_continue_targets_are_independent() {
    let output = execute(
        r#"
export function main():void {
 for(let i=0;i<3;i=i+1) {
  switch(i) {
   case 0: switch(7) { case 7: console.log(7); break; } console.log(8); break;
   case 1: continue;
   default: console.log(9);
  }
 }
 console.log((0 || (2 && 3)) && (4 || 5));
}
"#,
        &[],
    );
    assert_eq!(
        output.lines,
        [
            "varta log: 7",
            "varta log: 8",
            "varta log: 9",
            "varta log: 4"
        ]
    );
}

#[test]
fn unsupported_constructs_and_type_errors_fail_before_execution() {
    for source in [
        "throw new Error('x'); export function main():void {}",
        "import 'missing'; export function main():void {}",
        "export async function main():void {}",
        "export function main():void { let x:string=7; console.log(x); }",
        "export function main():void { let x=1; { console.log(x); let x=2; } }",
        "export function main():void { const x=1; x=2; }",
        "export function main():void { console.log(1.5); }",
        "export function main():void { return 1; }",
        "export function main():void { var x=1; }",
        "export function main():void { console.log(0); log(1,2); }",
    ] {
        assert!(
            varta_compiler::compile_typescript(source).is_err(),
            "{source}"
        );
    }
}

#[test]
fn void_returns_booleans_and_recursive_calls_validate_and_execute() {
    let output = execute(
        r#"
function nothing():void { return; }
function fact(n:number):number { if(n<2) {return 1;} else {return n*fact(n-1);} }
export function main():void { nothing(); console.log(!false); console.log(fact(5)); return; }
"#,
        &[],
    );
    assert_eq!(output.lines, ["varta log: 1", "varta log: 120"]);
}

#[test]
fn cpu_and_output_budgets_trap() {
    for source in [
        "export function main():void { while(1) {} }",
        "export function main():void { while(1) { console.log(1); } }",
    ] {
        let bytes = varta_compiler::compile_typescript(source).unwrap();
        assert!(VartaRuntime::new().run_wasm_bytes(&bytes, &[]).is_err());
    }
}

#[test]
fn denied_host_results_reset_length_and_expose_status() {
    let output = execute(
        r#"export function main():void {
        console.log(varta.env.get("VARTA_AUDIT_DENIED")); console.log(varta.result.status());
    }"#,
        &[],
    );
    assert!(output.lines[0].contains("capability denied: env"));
    assert_eq!(output.lines[1], "varta log: ");
    assert_eq!(
        output.lines[2],
        format!("varta log: {}", HostStatus::Denied as i32)
    );
}

#[test]
fn guest_exit_does_not_terminate_the_host() {
    let output = execute(
        "export function main():void { varta.process.exit(7); console.log(99); }",
        &[CapabilityGrant {
            id: "process.exit".into(),
            value: CapabilityValue::Boolean(true),
        }],
    );
    assert_eq!(output.exit_code, 7);
    assert!(output.lines.is_empty());
    assert_eq!(
        execute("export function main():void { console.log(42); }", &[]).lines,
        ["varta log: 42"]
    );
}

#[test]
fn native_payloads_and_invalid_wasm_are_rejected_by_safe_api() {
    let runtime = VartaRuntime::new();
    assert!(
        runtime
            .run_wasm_bytes(b"VARTA2\0arbitrary native bytes", &[])
            .is_err()
    );
    assert!(runtime.run_wasm_bytes(b"\0asm\x01\0\0\0\x01\x04\x01\x60\0\0\x03\x02\x01\0\x07\x0a\x01\x06_start\0\0\x0a\x05\x01\x03\0\x1a\x0b", &[]).is_err());
    runtime.run_wasm_bytes(b"\0asm\x01\0\0\0", &[]).unwrap();
}

#[test]
fn guest_memory_growth_is_limited_independently_of_host_result_allocation() {
    // A valid core module grows memory by 144 pages and logs its resulting size.
    let bytes=b"\0asm\x01\0\0\0\x01\x08\x02\x60\x01\x7f\0\x60\0\0\x02\x11\x01\x05varta\x07log_i32\0\0\x03\x02\x01\x01\x05\x03\x01\0\x01\x07\x13\x02\x06memory\x02\0\x06_start\0\x01\x0a\x0e\x01\x0c\0\x41\x90\x01\x40\0\x1a\x3f\0\x10\0\x0b";
    let output = VartaRuntime::new().run_wasm_bytes(bytes, &[]).unwrap();
    assert_eq!(output.lines, ["varta log: 1"]);
}

#[test]
fn missing_result_globals_are_rejected_before_host_calls() {
    let mut bytes = varta_compiler::compile_typescript(
        "export function main():void { console.log(varta.env.get(\"NAME\")); }",
    )
    .unwrap();
    let offset = bytes
        .windows(abi::RESULT_LEN.len())
        .position(|window| window == abi::RESULT_LEN.as_bytes())
        .unwrap();
    bytes[offset] = b'x';
    assert!(VartaRuntime::new().validate_module(&bytes).is_err());
}

#[test]
fn output_is_bounded_even_when_each_individual_log_is_valid() {
    let source = format!(
        "export function main():void {{ while(1) {{ console.log({:?}); }} }}",
        "A".repeat(4096)
    );
    let bytes = varta_compiler::compile_typescript(&source).unwrap();
    let error = VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap_err();
    assert!(
        error.to_string().contains("output budget exhausted"),
        "{error}"
    );
}

#[test]
fn guest_pointers_and_utf8_fail_closed_without_panicking() {
    let source =
        "export function main():void { console.log(\"X\"); console.log(varta.result.status()); }";
    let original = varta_compiler::compile_typescript(source).unwrap();
    let mut invalid_pointer = original.clone();
    let mut pointer_offset = None;
    for payload in wasmparser::Parser::new(0).parse_all(&original) {
        if let wasmparser::Payload::CodeSectionEntry(body) = payload.unwrap() {
            let mut reader = body.get_operators_reader().unwrap();
            while !reader.eof() {
                let (operator, offset) = reader.read_with_offset().unwrap();
                if matches!(operator, wasmparser::Operator::I64Const { value } if value == 1_i64 << i32::BITS)
                {
                    pointer_offset = Some(offset + 1);
                    break;
                }
            }
        }
    }
    // Same-width packed slice: ptr = u32::MAX, len = 1. Host rejects negative ptr.
    let offset = pointer_offset.unwrap();
    invalid_pointer[offset..offset + 5].copy_from_slice(&[0xff, 0xff, 0xff, 0xff, 0x1f]);
    let mut invalid_utf8 = original.clone();
    let data = invalid_utf8.iter().rposition(|byte| *byte == b'X').unwrap();
    invalid_utf8[data] = 0xff;
    for bytes in [invalid_pointer, invalid_utf8] {
        let output = VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap();
        assert_eq!(
            output.lines,
            ["varta log: <invalid host input>", "varta log: 2"]
        );
    }
}

#[test]
fn named_source_spans_preserve_validated_wasm_behavior() {
    let source = "function twice(n:number):number {return n*2;} export function main():void {console.log(twice(21));}";
    let bytes = varta_compiler::compile_typescript_with_path(
        source,
        std::path::Path::new("virtual/unwritten source.ts"),
    )
    .unwrap();
    let engine = wasmtime::Engine::default();
    wasmtime::Module::new(&engine, &bytes).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap()
            .lines,
        ["varta log: 42"]
    );
}

#[test]
fn linked_modules_forward_aliases_and_library_main_execute_with_correct_indices() {
    use std::{
        collections::BTreeMap,
        path::{Path, PathBuf},
    };
    use varta_compiler::{CompilerError, ModuleLoader, ModuleSource};
    struct MemoryModules(BTreeMap<PathBuf, String>);
    impl ModuleLoader for MemoryModules {
        fn resolve(&self, _: &Path, specifier: &str) -> Result<PathBuf, CompilerError> {
            Ok(specifier.trim_start_matches("./").into())
        }
        fn load(&self, path: &Path) -> Result<String, CompilerError> {
            self.0
                .get(path)
                .cloned()
                .ok_or_else(|| CompilerError::Module("missing virtual module".into()))
        }
    }
    let loader = MemoryModules(BTreeMap::from([
        (PathBuf::from("leaf.ts"), "export function main(n:number):number{return n+1;} export function sum(n:number):number{if(n==0){return 0;}return n+sum(n-1);}".into()),
        (PathBuf::from("bridge.ts"), "import {main as inc,sum} from './leaf.ts'; export {inc as advance,sum};".into()),
    ]));
    let compilation = varta_compiler::compile_modules(ModuleSource {
        path: "main.ts".into(),
        text: "import {advance,sum} from './bridge.ts'; function tri(a:number,b:number,c:number):number{return a+b+c;} export function main():void{console.log(advance(sum(8))+tri(1,2,2));}".into(),
    }, &loader).unwrap();
    assert_eq!(compilation.modules.len(), 3);
    wasmtime::Module::new(&wasmtime::Engine::default(), &compilation.wasm).unwrap();
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compilation.wasm, &[])
        .unwrap();
    assert_eq!(output.lines, ["varta log: 42"]);
}
