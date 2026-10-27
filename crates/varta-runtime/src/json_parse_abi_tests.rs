use super::*;
use crate::contract_tests::{export_index, rewrite};
use varta_common::{
    json::{JsonSchema, JsonType},
    value::*,
};
use wasmtime::Memory;

fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript(source).unwrap()
}
fn rebind(bytes: &[u8], name: &str, index: u8) -> Vec<u8> {
    rewrite(bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            let start = data
                .windows(name.len())
                .position(|s| s == name.as_bytes())
                .unwrap();
            let slot = start + name.len() + 1;
            assert!(data[slot] < 128 && index < 128);
            data[slot] = index;
        }
        vec![data]
    })
}
fn execution(bytes: &[u8]) -> (crate::execution::Execution, wasmtime::Instance, Memory) {
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
    (execution, instance, memory)
}
fn static_schema(data: &[u8]) -> (usize, JsonSchema) {
    let marker = b"{\"version\":1,\"root\":";
    let start = data[..abi::RESULT_BASE]
        .windows(marker.len())
        .position(|s| s == marker)
        .unwrap();
    let value = serde_json::Deserializer::from_slice(&data[start..abi::RESULT_BASE])
        .into_iter::<serde_json::Value>()
        .next()
        .unwrap()
        .unwrap();
    let schema = JsonSchema::from_bytes(&serde_json::to_vec(&value).unwrap()).unwrap();
    (start, schema)
}

#[test]
fn parse_helpers_require_real_signatures_pure_call_graphs_and_versioned_metadata() {
    for fake in [
        "function fake(n:number):number{console.log(n);return n;}",
        "function fake(n:number):number{return varta.time.now();}",
    ] {
        let bytes = compile(&format!(
            "{fake}export function main():void{{varta.json.parse<number[]>(\"[]\");}}"
        ));
        let fake = export_index(&bytes, "_start") - 1;
        let error = VartaRuntime::new()
            .validate_module(&rebind(&bytes, VALUE_ALLOCATE, fake))
            .unwrap_err();
        assert!(
            error.to_string().contains("cannot call host imports"),
            "{error}"
        );
    }
    let bytes = compile("export function main():void{varta.json.parse<number[]>(\"[]\");}");
    let error = VartaRuntime::new()
        .validate_module(&rebind(
            &bytes,
            VALUE_ALLOCATE,
            export_index(&bytes, "_start"),
        ))
        .unwrap_err();
    assert!(error.to_string().contains("signature mismatch"), "{error}");
    for name in [
        VALUE_ALLOCATE,
        VALUE_RETAIN,
        VALUE_SECTION,
        abi::SECTION,
        abi::RESULT_LEN,
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
}

#[test]
fn parser_rejects_bad_allocator_and_retainer_results_and_checks_input_before_allocation() {
    for (fake, target, body, expected) in [
        (
            "function fake(n:number):number{return 0;}",
            VALUE_ALLOCATE,
            "varta.json.parse<number[]>(\"[]\");",
            "invalid pointer",
        ),
        (
            "function fake(s:string):string{return \"\";}",
            VALUE_RETAIN,
            "varta.json.parse<string>(\"\\\"value\\\"\");",
            "retention helper",
        ),
        (
            "function fake(n:number):number{return 1/0;}",
            VALUE_ALLOCATE,
            "varta.json.parse<number[]>(\"{\");",
            "invalid JSON input",
        ),
        (
            "function fake(n:number):number{return 1/0;}",
            VALUE_ALLOCATE,
            "varta.json.parse<number[]>(\"[]\");",
            "divide",
        ),
    ] {
        let bytes = compile(&format!("{fake}export function main():void{{{body}}}"));
        let fake = export_index(&bytes, "_start") - 1;
        let bad = rebind(&bytes, target, fake);
        let error = VartaRuntime::new().run_wasm_bytes(&bad, &[]).unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn hostile_json_input_slices_fail_before_any_heap_value_is_published() {
    let bytes = compile(
        r#"
export function decode(text:string):string{return JSON.stringify(varta.json.parse<{s:string}>(text));}
export function main():void{}
varta.tool({name:"decode",description:"Decode"},decode);
"#,
    );
    for (ptr, len) in [
        (u32::MAX, 1),
        (abi::RESULT_BASE as u32, abi::MAX_INPUT_BYTES as u32 + 1),
        (abi::MAX_MEMORY_BYTES as u32 - 1, 8),
        (abi::RESULT_BASE as u32, 1),
    ] {
        let (mut execution, instance, memory) = execution(&bytes);
        memory
            .write(&mut execution.store, abi::RESULT_BASE, &[0xff])
            .unwrap();
        let value = ((len as u64) << i32::BITS) | ptr as u64;
        let error = instance
            .get_typed_func::<i64, i64>(&mut execution.store, "varta.tool.decode")
            .unwrap()
            .call(&mut execution.store, value as i64)
            .unwrap_err();
        assert!(format!("{error:?}").contains("checked JSON"), "{error:?}");
        assert_eq!(memory.data_size(&execution.store), abi::OWNED_STRING_BASE);
    }
}

#[test]
fn malformed_schemas_and_inconsistent_record_descriptors_fail_closed() {
    let bytes = compile(
        "export function main():void{varta.json.parse<{s:string}>(\"{\\\"s\\\":\\\"value\\\"}\");}",
    );
    for mutation in 0..6 {
        let (mut execution, instance, memory) = execution(&bytes);
        let (start, schema) = static_schema(memory.data(&execution.store));
        let JsonType::Record { descriptor, .. } = schema.types[schema.root as usize] else {
            panic!()
        };
        let descriptor = descriptor as usize;
        let (at, data) = match mutation {
            0 => (start + b"{\"version\":".len(), vec![b'9']),
            1 => (start + b"{\"version\":1,\"root\":".len(), vec![b'9']),
            2 => (descriptor, (VALUE_VERSION + 1).to_le_bytes().to_vec()),
            3 => (
                descriptor + RECORD_DESCRIPTOR_HEADER,
                u32::MAX.to_le_bytes().to_vec(),
            ),
            4 => (
                descriptor + RECORD_DESCRIPTOR_HEADER + 2 * size_of::<u32>(),
                u32::MAX.to_le_bytes().to_vec(),
            ),
            _ => (
                descriptor + RECORD_DESCRIPTOR_HEADER + 3 * size_of::<u32>(),
                (ValueKind::I32 as u32).to_le_bytes().to_vec(),
            ),
        };
        memory.write(&mut execution.store, at, &data).unwrap();
        let error = instance
            .get_typed_func::<(), ()>(&mut execution.store, "_start")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap_err();
        assert!(format!("{error:?}").contains("checked JSON"), "{error:?}");
        assert_eq!(memory.data_size(&execution.store), abi::OWNED_STRING_BASE);
    }
}

#[test]
fn allocator_callbacks_share_the_guest_fuel_budget_and_scalars_clear_length() {
    let bytes = compile(
        "function fake(n:number):number{while(true){}return n;}export function main():void{varta.json.parse<number[]>(\"[]\");}",
    );
    let fake = export_index(&bytes, "_start") - 1;
    let error = VartaRuntime::new()
        .run_wasm_bytes(&rebind(&bytes, VALUE_ALLOCATE, fake), &[])
        .unwrap_err();
    assert!(error.to_string().contains("fuel"), "{error}");
    let bytes = compile(
        "export function main():void{console.log(varta.json.parse<number>(\"-2147483648\"));}",
    );
    let (mut execution, instance, _) = execution(&bytes);
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    let length = instance
        .get_global(&mut execution.store, abi::RESULT_LEN)
        .unwrap();
    assert_eq!(length.get(&mut execution.store).i32(), Some(0));
}
