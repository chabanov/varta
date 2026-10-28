use super::*;
use crate::{
    contract_tests::{export_index, rewrite},
    execution::Execution,
};
use varta_common::{abi, language::LanguageProfile as P, value};
#[test]
fn bigint_values_abi_coercion_validates_transport_before_recoverable_errors() {
    let source = "function make():number|string|boolean|null|undefined|bigint{return 1n;}function width(v:number|string|boolean|null|undefined|bigint):bigint{try{return BigInt.asIntN(-1,v);}catch(e){console.log('caught');return 0n;}finally{console.log('finally');}}function text(v:number|string|boolean|null|undefined|bigint):string{try{return (1n).toString(v);}catch(e){console.log('caught');return 'ok';}finally{console.log('finally');}}export function main():void{}";
    let bytes =
        varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V29).unwrap();
    let main = export_index(&bytes, "_start");
    let bytes = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(main < 128 && data[0] < 124);
            data[0] += 3;
            for (name, index) in [("make", main - 3), ("width", main - 2), ("text", main - 1)] {
                data.push(name.len() as u8);
                data.extend(name.as_bytes());
                data.extend([0, index]);
            }
        }
        vec![data]
    });
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    for name in ["width", "text"] {
        for corrupt in [false, true] {
            let mut execution = Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
            let instance = execution.instantiate(&module).unwrap();
            let cell = instance
                .get_typed_func::<(), i64>(&mut execution.store, "make")
                .unwrap()
                .call(&mut execution.store, ())
                .unwrap();
            if corrupt {
                let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
                let at = cell as u32 as usize;
                let payload = u64::from_le_bytes(
                    memory.data(&execution.store)[at + 8..at + 16]
                        .try_into()
                        .unwrap(),
                );
                let header = payload as u32 as usize - abi::STRING_HEADER_BYTES;
                memory
                    .write(
                        &mut execution.store,
                        header + value::HEAP_FLAGS_OFFSET,
                        &value::HEAP_ALLOCATED_FLAG.to_le_bytes(),
                    )
                    .unwrap();
            }
            let result = instance
                .get_typed_func::<i64, i64>(&mut execution.store, name)
                .unwrap()
                .call(&mut execution.store, cell);
            if corrupt {
                assert!(result.is_err(), "{name}");
                assert!(execution.store.data().output.lines.is_empty(), "{name}");
            } else {
                assert!(result.is_ok(), "{name}");
                assert_eq!(
                    execution.store.data().output.lines,
                    ["varta log: caught", "varta log: finally"]
                );
            }
        }
    }
}
#[test]
fn bigint_values_abi_json_validates_later_slots_before_recoverable_bigint_failure() {
    let source = "function make():{a:bigint;b:bigint}{return {a:1n,b:2n};}function encode(v:{a:bigint;b:bigint}):string{try{return JSON.stringify(v);}catch(e){console.log('caught');return 'ok';}finally{console.log('finally');}}export function main():void{}";
    let bytes =
        varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V29).unwrap();
    let main = export_index(&bytes, "_start");
    let bytes = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(main < 128 && data[0] < 125);
            data[0] += 2;
            for (name, index) in [("make", main - 2), ("encode", main - 1)] {
                data.push(name.len() as u8);
                data.extend(name.as_bytes());
                data.extend([0, index]);
            }
        }
        vec![data]
    });
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    for corrupt in [0, 1, 2] {
        let mut execution = Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
        let object = instance
            .get_typed_func::<(), i64>(&mut execution.store, "make")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap() as u64;
        if corrupt == 1 {
            let slot = object as u32 as usize + value::RECORD_META_BYTES + value::RECORD_SLOT_BYTES;
            let second = u64::from_le_bytes(
                memory.data(&execution.store)[slot..slot + 8]
                    .try_into()
                    .unwrap(),
            );
            let header = second as u32 as usize - abi::STRING_HEADER_BYTES;
            memory
                .write(
                    &mut execution.store,
                    header + value::HEAP_FLAGS_OFFSET,
                    &value::HEAP_ALLOCATED_FLAG.to_le_bytes(),
                )
                .unwrap();
        }
        if corrupt == 2 {
            let header = object as u32 as usize - abi::STRING_HEADER_BYTES;
            memory
                .write(
                    &mut execution.store,
                    header + value::HEAP_LINK_OFFSET,
                    &0u32.to_le_bytes(),
                )
                .unwrap();
        }
        let result = instance
            .get_typed_func::<i64, i64>(&mut execution.store, "encode")
            .unwrap()
            .call(&mut execution.store, object as i64);
        if corrupt != 0 {
            assert!(result.is_err());
            assert!(execution.store.data().output.lines.is_empty());
        } else {
            assert!(result.is_ok());
            assert_eq!(
                execution.store.data().output.lines,
                ["varta log: caught", "varta log: finally"]
            );
        }
    }
}
#[test]
fn bigint_values_abi_requires_new_value_and_exception_selectors() {
    let bytes = varta_compiler::compile_typescript_with_profile(
        "export function main():void{console.log(JSON.stringify([1]));}",
        P::CheckedF64Utf16V29,
    )
    .unwrap();
    for marker in [
        value::VALUE_SECTION,
        varta_common::exceptions::BIGINT_VERSION,
    ] {
        let bad = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if matches!(id, 0 | 7)
                && let Some(at) = data
                    .windows(marker.len())
                    .position(|v| v == marker.as_bytes())
            {
                data[at] = b'x';
            }
            vec![data]
        });
        assert!(
            VartaRuntime::new().validate_module(&bad).is_err(),
            "{marker}"
        );
    }
}
