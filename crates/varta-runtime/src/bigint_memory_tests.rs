//! Exercise the pending binary leaf against real generated allocation and GC.
//! No BigInt source profile or historical value-tag admission is implied.
use super::*;
use crate::{contract_tests::rewrite, execution::Execution};
use varta_common::{
    abi,
    bigint::{self, BigIntView, Sign},
    value,
};
use wasmparser::{Operator, Parser, Payload};
use wasmtime::{ExternType, Instance, Memory};
const COLLECT: &str = "bigint.test.collect";

fn fixture() -> (Execution, Instance, Memory) {
    let bytes = varta_compiler::compile_typescript_with_profile(
        "export function main():void{varta.json.parse<number[]>(\"[]\");}",
        varta_common::language::LanguageProfile::CheckedF64Utf16V27,
    )
    .unwrap();
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let imported = module
        .imports()
        .filter(|i| matches!(i.ty(), ExternType::Func(_)))
        .count();
    let mut code_index = 0;
    let mut collector = None;
    for payload in Parser::new(0).parse_all(&bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if matches!(body.get_operators_reader().unwrap().read().unwrap(), Operator::I32Const { value } if value == abi::STRING_ROOT_BASE as i32)
            {
                assert!(collector.is_none());
                collector = Some(u8::try_from(imported + code_index).unwrap());
            }
            code_index += 1;
        }
    }
    let collector = collector.expect("real generated root-scanning collector");
    let bytes = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(data[0] < 127 && collector < 128 && COLLECT.len() < 128);
            data[0] += 1;
            data.push(COLLECT.len() as u8);
            data.extend_from_slice(COLLECT.as_bytes());
            data.extend_from_slice(&[0, collector]);
        }
        vec![data]
    });
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
    (execution, instance, memory)
}
fn word(memory: Memory, execution: &Execution, at: usize) -> u32 {
    u32::from_le_bytes(
        memory.data(&execution.store)[at..at + 4]
            .try_into()
            .unwrap(),
    )
}
#[test]
fn bigint_memory_managed_leaf_survives_collection_without_tracing_digit_bits() {
    let (mut execution, instance, memory) = fixture();
    let allocate = instance
        .get_typed_func::<i32, i32>(&mut execution.store, value::VALUE_ALLOCATE)
        .unwrap();
    let retain = instance
        .get_typed_func::<i64, i64>(&mut execution.store, value::VALUE_RETAIN)
        .unwrap();
    let collect = instance
        .get_typed_func::<(), ()>(&mut execution.store, COLLECT)
        .unwrap();
    let dead = allocate.call(&mut execution.store, 256).unwrap() as usize;
    let length = bigint::META_BYTES + 64 * bigint::DIGIT_BYTES;
    let pointer = allocate.call(&mut execution.store, length as i32).unwrap() as usize;
    let header = pointer - abi::STRING_HEADER_BYTES;
    let handle = ((length as u64) << 32) | pointer as u64;
    let mut payload = vec![0; length];
    payload[..4].copy_from_slice(&(Sign::Negative as u32).to_le_bytes());
    payload[4..8].copy_from_slice(&64u32.to_le_bytes());
    // Digits deliberately resemble a child handle and invalid pointer. Neither
    // may become a reference, even though their containing BigInt is retained.
    payload[8..12].copy_from_slice(&(dead as u32).to_le_bytes());
    payload[12..16].copy_from_slice(&256u32.to_le_bytes());
    payload[length - 4..].copy_from_slice(&u32::MAX.to_le_bytes());
    memory
        .write(&mut execution.store, pointer, &payload)
        .unwrap();
    memory
        .write(
            &mut execution.store,
            header + value::HEAP_FLAGS_OFFSET,
            &(value::HEAP_ALLOCATED_FLAG | bigint::HEAP_FLAG).to_le_bytes(),
        )
        .unwrap();
    memory
        .write(&mut execution.store, header + 12, &0u32.to_le_bytes())
        .unwrap();
    assert_eq!(
        retain.call(&mut execution.store, handle as i64).unwrap() as u64,
        handle
    );
    let root = (abi::STRING_ROOT_BASE..abi::STRING_ROOT_END)
        .step_by(abi::STRING_ROOT_SLOT_BYTES)
        .find(|slot| {
            &memory.data(&execution.store)[*slot..*slot + 8] == handle.to_le_bytes().as_slice()
        })
        .unwrap();
    for _ in 0..3 {
        collect.call(&mut execution.store, ()).unwrap();
        let view = BigIntView::read(memory.data(&execution.store), handle).unwrap();
        assert_eq!(view.sign(), Sign::Negative);
        assert_eq!(view.magnitude(), &payload[bigint::META_BYTES..]);
        assert_eq!(
            word(
                memory,
                &execution,
                dead - abi::STRING_HEADER_BYTES + value::HEAP_FLAGS_OFFSET
            ),
            0
        );
    }
    // The dead digit-looking target was actually reclaimed and is reusable.
    let reused = allocate.call(&mut execution.store, 256).unwrap() as usize;
    assert_eq!(reused, dead);
    memory
        .write(&mut execution.store, root, &0u64.to_le_bytes())
        .unwrap();
    collect.call(&mut execution.store, ()).unwrap();
    assert!(BigIntView::read(memory.data(&execution.store), handle).is_err());
    assert!(execution.store.data().output.lines.is_empty());
    assert_eq!(execution.store.data().host_calls, 0);
}
