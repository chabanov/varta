use super::*;
use crate::{
    contract_tests::{export_index, rewrite},
    execution::Execution,
};
use varta_common::{abi, bigint, language::LanguageProfile, value};
#[test]
fn bigint_abi_host_rejects_malformed_managed_operands_without_guest_catch_or_finally() {
    let bytes=varta_compiler::compile_typescript_with_profile("function make():bigint{return 123n;}function negate(value:bigint):bigint{try{return -value;}catch(e){console.log('caught');return 0n;}finally{console.log('finally');}}export function main():void{}",LanguageProfile::CheckedF64Utf16V28).unwrap();
    let main = export_index(&bytes, "_start");
    let bytes = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(data[0] < 125 && main < 128);
            data[0] += 2;
            for (name, index) in [("make", main - 2), ("negate", main - 1)] {
                data.push(name.len() as u8);
                data.extend(name.as_bytes());
                data.extend([0, index]);
            }
        }
        vec![data]
    });
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    for mutation in 0..9 {
        let mut execution = Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
        let handle = instance
            .get_typed_func::<(), i64>(&mut execution.store, "make")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap() as u64;
        let pointer = handle as u32 as usize;
        let header = pointer - abi::STRING_HEADER_BYTES;
        assert!(bigint::BigIntView::read(memory.data(&execution.store), handle).is_ok());
        let (offset, word) = match mutation {
            0 => (
                header + value::HEAP_FLAGS_OFFSET,
                value::HEAP_ALLOCATED_FLAG,
            ),
            1 => (
                header + value::HEAP_FLAGS_OFFSET,
                value::HEAP_ALLOCATED_FLAG | bigint::HEAP_FLAG | 2,
            ),
            2 => (header + value::HEAP_LINK_OFFSET, 1),
            3 => (header + 12, 1),
            4 => (pointer, 3),
            5 => (pointer + 4, u32::MAX),
            6 => (pointer + bigint::META_BYTES, 0),
            7 => (header, u32::MAX),
            _ => (header, 16),
        };
        memory
            .write(&mut execution.store, offset, &word.to_le_bytes())
            .unwrap();
        let error = instance
            .get_typed_func::<i64, i64>(&mut execution.store, "negate")
            .unwrap()
            .call(&mut execution.store, handle as i64)
            .unwrap_err();
        assert_eq!(
            error.root_cause().downcast_ref::<varta_bigint::Error>(),
            Some(&varta_bigint::Error::InvalidPayload),
            "{mutation}: {error:?}"
        );
        assert!(execution.store.data().output.lines.is_empty());
    }
}
#[test]
fn bigint_abi_previous_profiles_reject_new_imports_and_signatures() {
    let bytes = varta_compiler::compile_typescript_with_profile(
        "export function main():void{console.log(1n+2n);}",
        LanguageProfile::CheckedF64Utf16V28,
    )
    .unwrap();
    let bad = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 0 {
            if data.get(1..1 + abi::SECTION.len()) == Some(abi::SECTION.as_bytes()) {
                let offset = 1 + abi::SECTION.len();
                data[offset..offset + 4].copy_from_slice(&abi::NUMBER_PARSE_VERSION.to_le_bytes());
            }
            if data.get(1..1 + varta_common::language::SECTION.len())
                == Some(varta_common::language::SECTION.as_bytes())
            {
                for byte in data.iter_mut().rev() {
                    if *byte == b'8' {
                        *byte = b'7';
                        break;
                    }
                }
            }
        }
        vec![data]
    });
    assert!(VartaRuntime::new().validate_module(&bad).is_err());
}
