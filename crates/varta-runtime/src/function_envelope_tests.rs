//! Independent common views must agree with real compiler-emitted heap objects.
use super::*;
use varta_common::{
    abi,
    function::{BindingView, ClosureView},
    language::LanguageProfile,
};
#[test]
fn function_envelopes_match_emitted_closures_environments_and_captured_result_pairs() {
    let source = "function counter(start:number):()=>number{let n=start;return ()=>{n++;return n;};}function result():()=>VartaResult<string>{const r=varta.result.capture(varta.env.get('VARTA_FUNCTION_ENVELOPE_DENIED'));return ()=>r;}export function main():void{const first=counter(3);const second=counter(8);console.log(first());console.log(second());const text='kept';const read=():string=>text;console.log(read());const pair=result();console.log(pair().status);const plain=():number=>7;console.log(plain());}";
    let bytes = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V30,
    )
    .unwrap();
    let catalog = wasmparser::Parser::new(0)
        .parse_all(&bytes)
        .find_map(|entry| match entry.unwrap() {
            wasmparser::Payload::CustomSection(section)
                if section.name() == varta_common::function::CATALOG_SECTION =>
            {
                Some(varta_common::function::FunctionCatalog::from_bytes(section.data()).unwrap())
            }
            _ => None,
        })
        .expect("compiler function catalog");
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    assert_eq!(
        execution.store.data().output.lines,
        [
            "varta log: 4",
            "varta log: 9",
            "varta log: kept",
            "varta log: <capability denied: env> (VARTA_FUNCTION_ENVELOPE_DENIED)",
            "varta log: 1",
            "varta log: 7"
        ]
    );
    let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
    let data = memory.data(&execution.store);
    let word = |at: usize| u32::from_le_bytes(data[at..at + 4].try_into().unwrap());
    let mut at = abi::OWNED_STRING_BASE;
    let mut functions = 0;
    let mut captures = 0;
    let mut pairs = 0;
    let mut plain = 0;
    while at + abi::STRING_HEADER_BYTES <= data.len() {
        let size = word(at) as usize;
        if size == 0 {
            break;
        }
        assert!(
            size >= abi::STRING_HEADER_BYTES && size.is_multiple_of(8) && at + size <= data.len()
        );
        if word(at + 4)
            == varta_common::value::HEAP_ALLOCATED_FLAG | varta_common::function::HEAP_FLAG as u32
        {
            let handle = ((varta_common::function::CLOSURE_BYTES as u64) << 32)
                | (at + abi::STRING_HEADER_BYTES) as u64;
            let function = ClosureView::read_envelope(data, handle).unwrap();
            let proof = catalog.validate_closure(data, handle, abi::FUEL).unwrap();
            assert!(proof.nodes > 0 && proof.work > 0);

            functions += 1;
            if let Some(environment) = function.environment {
                for handle in environment.bindings() {
                    let binding = BindingView::read_envelope(data, handle).unwrap();
                    captures += 1;
                    if let Some(status) = binding.status {
                        assert_eq!(status, 1);
                        pairs += 1;
                    }
                }
            } else {
                plain += 1;
            }
        }
        at += size;
    }
    assert!(functions >= 5, "{functions} function envelopes");
    assert!(captures >= 4, "{captures} capture bindings");
    assert!(pairs >= 1, "captured physical result pair missing");
    assert!(plain >= 1, "capture-free closure missing");
}
#[test]
fn function_envelope_foundation_keeps_v30_function_aggregate_admission_closed() {
    for source in [
        "export function main():void{const r={f:()=>1};}",
        "export function main():void{const fs:(()=>number)[]=[()=>1];}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                source,
                LanguageProfile::CheckedF64Utf16V30
            )
            .is_err()
        );
    }
}
pub(super) fn catalog_fixture(
    source: &str,
) -> (Vec<u8>, varta_common::function::FunctionCatalog, Vec<u64>) {
    let bytes = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V30,
    )
    .unwrap();
    let catalog = wasmparser::Parser::new(0)
        .parse_all(&bytes)
        .find_map(|entry| match entry.unwrap() {
            wasmparser::Payload::CustomSection(section)
                if section.name() == varta_common::function::CATALOG_SECTION =>
            {
                Some(varta_common::function::FunctionCatalog::from_bytes(section.data()).unwrap())
            }
            _ => None,
        })
        .unwrap();
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
    let data = memory.data(&execution.store).to_vec();
    let mut functions = Vec::new();
    let mut at = abi::OWNED_STRING_BASE;
    while at + 16 <= data.len() {
        let size = u32::from_le_bytes(data[at..at + 4].try_into().unwrap()) as usize;
        if size == 0 {
            break;
        }
        assert!(size >= 16 && size.is_multiple_of(8) && at + size <= data.len());
        if u32::from_le_bytes(data[at + 4..at + 8].try_into().unwrap())
            == 1 | varta_common::function::HEAP_FLAG as u32
        {
            functions.push((24u64 << 32) | (at + 16) as u64);
        }
        at += size;
    }
    (data, catalog, functions)
}
#[test]
fn function_catalog_validates_structured_captures_cycles_and_rejects_guest_type_forgery() {
    use varta_common::function::CaptureType;
    let source = "function make():()=>number{const r:{n?:number;s:string}={n:3,s:'x'};const a:(number|undefined)[]=[1,,undefined];const big=9007199254740993n;const e=TypeError('x');const flag=true;let value:number|string=1;const next=():number=>7;const fn=():number=>{console.log(r.s);console.log(JSON.stringify(a));console.log(big);console.log(e.name);console.log(flag);console.log(value);return next();};value='changed';return fn;}function cycle():()=>number{let fn:()=>number=()=>7;const wrapper=()=>fn();fn=wrapper;return wrapper;}export function main():void{const f=make();console.log(f());const recursive=cycle();console.log(Object.is(recursive,recursive));}";
    let (data, catalog, functions) = catalog_fixture(source);
    assert!(
        catalog
            .types
            .iter()
            .any(|ty| matches!(ty, CaptureType::Record { .. }))
    );
    assert!(
        catalog
            .types
            .iter()
            .any(|ty| matches!(ty, CaptureType::Array { .. }))
    );
    assert!(
        catalog
            .types
            .iter()
            .any(|ty| matches!(ty, CaptureType::BigInt))
    );
    assert!(
        catalog
            .types
            .iter()
            .any(|ty| matches!(ty, CaptureType::Error { .. }))
    );
    assert!(
        catalog
            .types
            .iter()
            .any(|ty| matches!(ty, CaptureType::Function { .. }))
    );
    for &function in &functions {
        catalog
            .validate_closure(&data, function, abi::FUEL)
            .unwrap();
    }
    let mut checked_boolean = false;
    let mut checked_record = false;
    for &root in &functions {
        let function = ClosureView::read_envelope(&data, root).unwrap();
        let schema = catalog
            .functions
            .iter()
            .find(|row| row.code == function.code)
            .unwrap();
        if let Some(env) = function.environment {
            for (handle, id) in env.bindings().zip(&schema.captures) {
                let binding = BindingView::read_envelope(&data, handle).unwrap();
                let ptr = handle as u32 as usize;
                match &catalog.types[catalog.bindings[*id as usize].ty as usize] {
                    CaptureType::Boolean => {
                        let mut bad = data.clone();
                        bad[ptr + 8..ptr + 16].copy_from_slice(&2u64.to_le_bytes());
                        assert!(catalog.validate_closure(&bad, root, abi::FUEL).is_err());
                        checked_boolean = true;
                    }
                    CaptureType::Record { .. } => {
                        let object = binding.value as u32 as usize;
                        let descriptor =
                            u32::from_le_bytes(data[object..object + 4].try_into().unwrap())
                                as usize;
                        let key = u32::from_le_bytes(
                            data[descriptor + 8..descriptor + 12].try_into().unwrap(),
                        ) as usize;
                        let mut bad = data.clone();
                        bad[key] = b'q';
                        assert!(catalog.validate_closure(&bad, root, abi::FUEL).is_err());
                        // Descriptor entry order may vary, but typed slot identities may not.
                        let mut permuted = data.clone();
                        for entry in 0..2 {
                            let at = descriptor + 8 + entry * 16 + 8;
                            let raw = u32::from_le_bytes(data[at..at + 4].try_into().unwrap());
                            permuted[at..at + 4].copy_from_slice(
                                &((raw & varta_common::value::OPTIONAL_FIELD_FLAG)
                                    | (1 - (raw & !varta_common::value::OPTIONAL_FIELD_FLAG)))
                                    .to_le_bytes(),
                            );
                        }
                        varta_common::value::validate_aggregate(
                            &permuted,
                            varta_common::value::ValueKind::Record,
                            binding.value,
                            catalog.value_version,
                        )
                        .unwrap();
                        assert!(
                            catalog
                                .validate_closure(&permuted, root, abi::FUEL)
                                .is_err()
                        );
                        checked_record = true;
                    }
                    _ => {}
                }
            }
        }
    }
    assert!(checked_boolean && checked_record);
}
fn append_section(module: &mut Vec<u8>, id: u8, data: &[u8]) {
    module.push(id);
    let mut n = data.len();
    loop {
        let mut byte = (n & 127) as u8;
        n >>= 7;
        if n != 0 {
            byte |= 128;
        }
        module.push(byte);
        if n == 0 {
            break;
        }
    }
    module.extend(data);
}
fn append_catalog(module: &mut Vec<u8>, data: &[u8]) {
    let name = varta_common::function::CATALOG_SECTION.as_bytes();
    assert!(name.len() < 128);
    let mut payload = vec![name.len() as u8];
    payload.extend(name);
    payload.extend(data);
    append_section(module, 0, &payload);
}
#[test]
fn function_catalog_preflight_rejects_duplicate_malformed_and_wrong_profile_metadata() {
    let source = "export function main():void{const f=():number=>7;console.log(f());}";
    let bytes = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V30,
    )
    .unwrap();
    let mut stripped = bytes[..8].to_vec();
    let mut catalog = None;
    for entry in wasmparser::Parser::new(0).parse_all(&bytes) {
        let entry = entry.unwrap();
        if let wasmparser::Payload::CustomSection(section) = &entry
            && section.name() == varta_common::function::CATALOG_SECTION
        {
            catalog = Some(section.data().to_vec());
            continue;
        }
        if let Some((id, range)) = entry.as_section() {
            append_section(&mut stripped, id, &bytes[range]);
        }
    }
    let catalog = catalog.unwrap();
    let runtime = VartaRuntime::new();
    runtime.validate_module(&stripped).unwrap();
    let mut duplicate = bytes;
    append_catalog(&mut duplicate, &catalog);
    assert!(runtime.validate_module(&duplicate).is_err());
    let mut malformed = stripped;
    append_catalog(&mut malformed, b"{}");
    assert!(runtime.validate_module(&malformed).is_err());
    let mut old = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V29,
    )
    .unwrap();
    append_catalog(&mut old, &catalog);
    assert!(runtime.validate_module(&old).is_err());
}
