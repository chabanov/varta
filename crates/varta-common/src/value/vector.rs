//! Stable v9-v11 dense vectors and v12 backing storage with exact presence bits.
use super::*;
use crate::abi;

#[derive(Debug, Clone, Copy)]
pub struct Vector {
    pub ptr: usize,
    pub storage: usize,
    pub kind: ValueKind,
    pub length: usize,
    pub capacity: usize,
    pub presence: Option<usize>,
}
fn invalid() -> ValueAbiError {
    ValueAbiError("noncanonical resizable array".into())
}
fn word(data: &[u8], at: usize) -> Result<u32, ValueAbiError> {
    at.checked_add(4)
        .and_then(|end| data.get(at..end))
        .and_then(|s| s.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(invalid)
}
fn managed(data: &[u8], handle: u64, flag: i32) -> Result<(usize, usize), ValueAbiError> {
    let ptr = handle as u32 as usize;
    let length = (handle >> i32::BITS) as usize;
    if ptr < abi::OWNED_STRING_BASE + abi::STRING_HEADER_BYTES
        || !ptr.is_multiple_of(HEAP_ALIGNMENT)
    {
        return Err(invalid());
    }
    let header = ptr - abi::STRING_HEADER_BYTES;
    let size = word(data, header)? as usize;
    if size < abi::STRING_HEADER_BYTES
        || !size.is_multiple_of(HEAP_ALIGNMENT)
        || length > size - abi::STRING_HEADER_BYTES
        || header.checked_add(size).is_none_or(|end| end > data.len())
        || word(data, header + HEAP_FLAGS_OFFSET)? != HEAP_ALLOCATED_FLAG | flag as u32
    {
        return Err(invalid());
    }
    Ok((ptr, length))
}
impl Vector {
    pub fn is_present(&self, data: &[u8], index: usize) -> Result<bool, ValueAbiError> {
        if index >= self.length {
            return Ok(false);
        }
        match self.presence {
            None => Ok(true),
            Some(at) => data
                .get(
                    at.checked_add(index / u8::BITS as usize)
                        .ok_or_else(invalid)?,
                )
                .map(|byte| byte & (1 << (index % u8::BITS as usize)) != 0)
                .ok_or_else(invalid),
        }
    }
    pub fn read(data: &[u8], handle: u64) -> Result<Self, ValueAbiError> {
        Self::read_with_version(data, handle, VECTOR_VALUE_VERSION)
    }
    pub fn read_with_version(
        data: &[u8],
        handle: u64,
        version: u32,
    ) -> Result<Self, ValueAbiError> {
        if !matches!(
            version,
            VECTOR_VALUE_VERSION
                | ERROR_VALUE_VERSION
                | ERROR_FAMILY_VALUE_VERSION
                | SPARSE_VALUE_VERSION
                | BIGINT_VALUE_VERSION
        ) {
            return Err(invalid());
        }
        let (ptr, bytes) = managed(data, handle, HEAP_VECTOR_FLAG)?;
        if bytes != VECTOR_BYTES
            || word(data, ptr - abi::STRING_HEADER_BYTES + HEAP_LINK_OFFSET)? != 1
        {
            return Err(invalid());
        }
        let kind = ValueKind::from_tag(word(data, ptr)?)
            .filter(|k| k.supported_by(version))
            .ok_or_else(invalid)?;
        let length = word(data, ptr + ARRAY_LENGTH_OFFSET as usize)? as usize;
        let storage_handle = (word(data, ptr + VECTOR_STORAGE_OFFSET as usize)? as u64)
            | ((word(data, ptr + VECTOR_STORAGE_OFFSET as usize + 4)? as u64) << i32::BITS);
        let sparse = matches!(version, SPARSE_VALUE_VERSION | BIGINT_VALUE_VERSION);
        let (storage, bytes) = managed(
            data,
            storage_handle,
            HEAP_ARRAY_FLAG | if sparse { HEAP_SPARSE_FLAG } else { 0 },
        )?;
        let bitmap_bytes = if sparse { SPARSE_BITMAP_BYTES } else { 0 };
        let payload_bytes = bytes.checked_sub(bitmap_bytes).ok_or_else(invalid)?;
        if !(ARRAY_META_BYTES..=ARRAY_META_BYTES + MAX_ARRAY_ELEMENTS * ARRAY_SLOT_BYTES)
            .contains(&payload_bytes)
            || !(payload_bytes - ARRAY_META_BYTES).is_multiple_of(ARRAY_SLOT_BYTES)
        {
            return Err(invalid());
        }
        let capacity = (payload_bytes - ARRAY_META_BYTES) / ARRAY_SLOT_BYTES;
        if length > capacity
            || (capacity != 0 && !capacity.is_power_of_two())
            || word(data, storage)? != kind as u32
            || word(data, storage + 4)? as usize != length
            || word(data, storage - abi::STRING_HEADER_BYTES + HEAP_LINK_OFFSET)?
                != u32::from(kind.is_reference())
        {
            return Err(invalid());
        }
        let spare = storage + ARRAY_META_BYTES + length * ARRAY_SLOT_BYTES;
        if data[spare..storage + payload_bytes].iter().any(|b| *b != 0) {
            return Err(invalid());
        }
        let presence = sparse.then_some(storage + payload_bytes);
        if let Some(presence) = presence {
            for index in 0..MAX_ARRAY_ELEMENTS {
                let present = data[presence + index / u8::BITS as usize]
                    & (1 << (index % u8::BITS as usize))
                    != 0;
                if index >= length {
                    if present {
                        return Err(invalid());
                    }
                } else if !present
                    && data[storage + ARRAY_META_BYTES + index * ARRAY_SLOT_BYTES
                        ..storage + ARRAY_META_BYTES + (index + 1) * ARRAY_SLOT_BYTES]
                        .iter()
                        .any(|byte| *byte != 0)
                {
                    return Err(invalid());
                }
            }
        }
        Ok(Self {
            presence,
            ptr,
            storage,
            kind,
            length,
            capacity,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Vec<u8>, u64, usize, usize) {
        let cell = abi::OWNED_STRING_BASE + abi::STRING_HEADER_BYTES;
        let backing = cell + 64;
        let mut data = vec![0; backing + 64];
        let mut put =
            |at: usize, value: u32| data[at..at + 4].copy_from_slice(&value.to_le_bytes());
        for (ptr, size, flag, mask) in [
            (cell, 32, HEAP_VECTOR_FLAG, 1),
            (backing, 40, HEAP_ARRAY_FLAG, 0),
        ] {
            put(ptr - abi::STRING_HEADER_BYTES, size);
            put(
                ptr - abi::STRING_HEADER_BYTES + HEAP_FLAGS_OFFSET,
                HEAP_ALLOCATED_FLAG | flag as u32,
            );
            put(ptr - abi::STRING_HEADER_BYTES + HEAP_LINK_OFFSET, mask);
        }
        for ptr in [cell, backing] {
            put(ptr, ValueKind::Float64 as u32);
            put(ptr + 4, 1);
        }
        let storage =
            ((ARRAY_META_BYTES + 2 * ARRAY_SLOT_BYTES) as u64) << i32::BITS | backing as u64;
        data[cell + 8..cell + 16].copy_from_slice(&storage.to_le_bytes());
        data[backing + 8..backing + 16].copy_from_slice(&1.25f64.to_bits().to_le_bytes());
        (
            data,
            ((VECTOR_BYTES as u64) << i32::BITS) | cell as u64,
            cell,
            backing,
        )
    }
    #[test]
    fn resizable_array_layout_validates_storage_capacity_counts_flags_and_spare_slots() {
        let (data, handle, cell, backing) = fixture();
        let vector = Vector::read(&data, handle).unwrap();
        assert_eq!(
            (vector.length, vector.capacity, vector.storage),
            (1, 2, backing)
        );
        for (at, value) in [
            (
                cell - abi::STRING_HEADER_BYTES + HEAP_FLAGS_OFFSET,
                HEAP_ALLOCATED_FLAG | HEAP_ARRAY_FLAG as u32,
            ),
            (cell - abi::STRING_HEADER_BYTES + HEAP_LINK_OFFSET, 0),
            (cell - abi::STRING_HEADER_BYTES, 31),
            (cell, ValueKind::I32 as u32),
            (cell + 4, 3),
            (cell + 8, 0),
            (cell + 12, ARRAY_META_BYTES as u32 + 1),
            (backing, ValueKind::String as u32),
            (backing + 4, 2),
            (backing + 16, 1),
            (
                backing - abi::STRING_HEADER_BYTES + HEAP_FLAGS_OFFSET,
                HEAP_ALLOCATED_FLAG | HEAP_VECTOR_FLAG as u32,
            ),
            (backing - abi::STRING_HEADER_BYTES + HEAP_LINK_OFFSET, 1),
        ] {
            let mut bad = data.clone();
            bad[at..at + 4].copy_from_slice(&value.to_le_bytes());
            assert!(Vector::read(&bad, handle).is_err(), "{at}: {value}");
        }
        for bad in [handle + 1, handle ^ (1u64 << i32::BITS), u64::MAX, 0] {
            assert!(Vector::read(&data, bad).is_err());
        }
    }
    #[test]
    fn resizable_reference_cells_require_value_v9_without_reinterpreting_v8() {
        let (mut data, handle, _, _) = fixture();
        let header = data.len();
        data.resize(header + 32, 0);
        let ptr = header + abi::STRING_HEADER_BYTES;
        for (index, value) in [
            32,
            HEAP_ALLOCATED_FLAG | HEAP_UNION_FLAG as u32,
            1,
            0,
            ValueKind::Array as u32,
            1,
        ]
        .iter()
        .enumerate()
        {
            data[header + index * 4..header + index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        data[ptr + 8..ptr + 16].copy_from_slice(&handle.to_le_bytes());
        let cell = ((UNION_BYTES as u64) << i32::BITS) | ptr as u64;
        assert_eq!(
            ReferenceBox::read_with_version(&data, cell, VECTOR_VALUE_VERSION)
                .unwrap()
                .payload,
            handle
        );
        assert!(ReferenceBox::read(&data, cell).is_err());
        assert!(PrimitiveBox::read(&data, cell).is_err());
    }
    #[test]
    fn sparse_vectors_require_exact_presence_zero_holes_and_the_new_selected_version() {
        let (mut data, handle, cell, backing) = fixture();
        let payload = ARRAY_META_BYTES + 2 * ARRAY_SLOT_BYTES;
        let bytes = payload + SPARSE_BITMAP_BYTES;
        data.resize(backing + bytes, 0);
        data[backing - abi::STRING_HEADER_BYTES..backing - abi::STRING_HEADER_BYTES + 4]
            .copy_from_slice(&((abi::STRING_HEADER_BYTES + bytes) as u32).to_le_bytes());
        data[backing - abi::STRING_HEADER_BYTES + HEAP_FLAGS_OFFSET
            ..backing - abi::STRING_HEADER_BYTES + HEAP_FLAGS_OFFSET + 4]
            .copy_from_slice(
                &(HEAP_ALLOCATED_FLAG | HEAP_ARRAY_FLAG as u32 | HEAP_SPARSE_FLAG as u32)
                    .to_le_bytes(),
            );
        data[cell + VECTOR_STORAGE_OFFSET as usize..cell + VECTOR_STORAGE_OFFSET as usize + 8]
            .copy_from_slice(&(((bytes as u64) << i32::BITS) | backing as u64).to_le_bytes());
        let bitmap = backing + payload;
        data[bitmap] = 1;
        let vector = Vector::read_with_version(&data, handle, SPARSE_VALUE_VERSION).unwrap();
        assert!(vector.is_present(&data, 0).unwrap());
        assert!(!vector.is_present(&data, 1).unwrap());
        assert!(Vector::read_with_version(&data, handle, ERROR_FAMILY_VALUE_VERSION).is_err());
        for (at, value) in [
            (bitmap, 0),
            (bitmap, 3),
            (bitmap + SPARSE_BITMAP_BYTES - 1, 128),
            (
                backing - abi::STRING_HEADER_BYTES + HEAP_FLAGS_OFFSET,
                HEAP_ALLOCATED_FLAG as u8,
            ),
            (backing + ARRAY_META_BYTES + ARRAY_SLOT_BYTES, 1),
        ] {
            let mut bad = data.clone();
            bad[at] = value;
            assert!(
                Vector::read_with_version(&bad, handle, SPARSE_VALUE_VERSION).is_err(),
                "{at}: {value}"
            );
        }
        let mut hole = data.clone();
        hole[bitmap] = 0;
        hole[backing + ARRAY_META_BYTES..backing + ARRAY_META_BYTES + ARRAY_SLOT_BYTES].fill(0);
        let vector = Vector::read_with_version(&hole, handle, SPARSE_VALUE_VERSION).unwrap();
        assert!(!vector.is_present(&hole, 0).unwrap());
    }
}
