//! ABI22 primitive String conversion; checked UTF-16 input is copied before transfer.
use crate::{RuntimeContext, host, number};
use varta_common::{
    HostOperation,
    value::{PrimitiveBox, ValueKind},
};
use wasmtime::{Caller, Extern, Linker};
fn convert(bytes: &[u8], cell: i64) -> Result<Vec<u8>, wasmtime::Error> {
    let value = PrimitiveBox::read(bytes, cell as u64).map_err(wasmtime::Error::new)?;
    let text = match value.kind {
        ValueKind::Float64 => number::canonical_display(f64::from_bits(value.payload)),
        ValueKind::Boolean => if value.payload == 0 { "false" } else { "true" }.into(),
        ValueKind::Null => "null".into(),
        ValueKind::Undefined => "undefined".into(),
        ValueKind::String => {
            let start = value.payload as u32 as usize;
            let length = (value.payload >> 32) as usize;
            return Ok(bytes[start..start + length].to_vec());
        }
        _ => {
            return Err(wasmtime::Error::msg(
                "String conversion requires a primitive value cell",
            ));
        }
    };
    Ok(varta_common::text::encode(&text))
}
pub(super) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "varta",
        HostOperation::PrimitiveString.name(),
        |mut caller: Caller<'_, RuntimeContext>, cell: i64| -> Result<i32, wasmtime::Error> {
            caller.data_mut().tick()?;
            let Some(Extern::Memory(memory)) = caller.get_export("memory") else {
                return Err(wasmtime::Error::msg("missing value memory"));
            };
            let result = convert(memory.data(&caller), cell)?;
            let pointer = host::result_bytes(&mut caller, &result)
                .map_err(|_| wasmtime::Error::msg("String conversion result transfer"))?;
            caller.data().check_budget()?;
            Ok(pointer)
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use varta_common::{abi, value};
    #[test]
    fn primitive_string_copies_every_utf16_unit_and_rejects_noncanonical_cells() {
        let units = (0..=u16::MAX)
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let header = abi::OWNED_STRING_BASE;
        let cell = header + abi::STRING_HEADER_BYTES;
        let text = cell + value::UNION_BYTES;
        let mut bytes = vec![0; text + units.len()];
        for (index, word) in [
            32,
            value::HEAP_ALLOCATED_FLAG | value::HEAP_UNION_FLAG as u32,
            1,
            0,
            ValueKind::String as u32,
            1,
        ]
        .into_iter()
        .enumerate()
        {
            bytes[header + index * 4..header + index * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        let packed = ((units.len() as u64) << 32) | text as u64;
        bytes[cell + value::UNION_META_BYTES..cell + value::UNION_BYTES]
            .copy_from_slice(&packed.to_le_bytes());
        bytes[text..].copy_from_slice(&units);
        let handle = (((value::UNION_BYTES as u64) << 32) | cell as u64) as i64;
        assert_eq!(convert(&bytes, handle).unwrap(), units);
        for (offset, word) in [
            (header, 16),
            (header + 4, 0),
            (header + 8, 0),
            (cell, ValueKind::Record as u32),
            (cell + 4, 0),
            (cell + 12, 1),
        ] {
            let mut bad = bytes.clone();
            bad[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
            // A one-byte string is not canonical UTF-16.
            assert!(convert(&bad, handle).is_err(), "offset {offset}");
        }
        assert!(convert(&bytes, handle ^ (1_i64 << 32)).is_err());
    }
}
