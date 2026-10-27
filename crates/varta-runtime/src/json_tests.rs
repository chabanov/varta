use super::*;

#[test]
fn reference_union_payloads_validate_managed_kinds_masks_shapes_and_versions() {
    let mut memory = Memory::new();
    let record = memory.presence_record(&[("n", ValueKind::Float64, 1.5f64.to_bits())], 0, 1, &[0]);
    let ptr = record as u32 as usize;
    let descriptor = memory.words_at(ptr) as usize;
    memory.word(descriptor, REFERENCE_VALUE_VERSION);
    let cell = memory.reference_cell(ValueKind::Record, record);
    let cell_ptr = cell as u32 as usize;
    assert_eq!(
        ReferenceBox::read(&memory.bytes, cell).unwrap().payload,
        record
    );
    assert!(PrimitiveBox::read(&memory.bytes, cell).is_err());
    assert_eq!(
        Encoder::with_version(&memory.bytes, REFERENCE_VALUE_VERSION)
            .encode(ValueKind::ReferenceUnion as u32, cell)
            .unwrap(),
        br#"{"n":1.5}"#
    );
    assert!(
        Encoder::with_version(&memory.bytes, MISSING_VALUE_VERSION)
            .encode(ValueKind::ReferenceUnion as u32, cell)
            .is_err()
    );
    assert!(
        Encoder::with_version(&memory.bytes, REFERENCE_VALUE_VERSION)
            .encode(ValueKind::PrimitiveUnion as u32, cell)
            .is_err()
    );
    for (at, word) in [
        (cell_ptr - abi::STRING_HEADER_BYTES + HEAP_LINK_OFFSET, 0u32),
        (
            ptr - abi::STRING_HEADER_BYTES + HEAP_FLAGS_OFFSET,
            HEAP_ALLOCATED_FLAG | HEAP_ARRAY_FLAG as u32,
        ),
        (ptr - abi::STRING_HEADER_BYTES + HEAP_LINK_OFFSET, 1),
        (ptr - abi::STRING_HEADER_BYTES, 0),
        (ptr + 4, 33),
        (descriptor, MISSING_VALUE_VERSION),
        (ptr + presence_offset(1), 0),
        (
            descriptor + RECORD_DESCRIPTOR_HEADER + 8,
            OPTIONAL_FIELD_FLAG,
        ),
    ] {
        let saved = memory.words_at(at);
        memory.word(at, word);
        assert!(
            ReferenceBox::read(&memory.bytes, cell).is_err(),
            "at={at} word={word}"
        );
        memory.word(at, saved);
    }
    memory.slot(cell_ptr + UNION_META_BYTES, record ^ (1u64 << i32::BITS));
    assert!(ReferenceBox::read(&memory.bytes, cell).is_err());
    memory.slot(cell_ptr + UNION_META_BYTES, record);
    let array = memory.array(ValueKind::Float64, &[1.5f64.to_bits()]);
    let array_ptr = array as u32 as usize;
    let array_cell = memory.reference_cell(ValueKind::Array, array);
    assert_eq!(
        ReferenceBox::read(&memory.bytes, array_cell).unwrap().kind,
        ValueKind::Array
    );
    for (at, word) in [
        (array_ptr, ValueKind::I32 as u32),
        (array_ptr + 4, MAX_ARRAY_ELEMENTS as u32 + 1),
        (array_ptr - abi::STRING_HEADER_BYTES + HEAP_LINK_OFFSET, 1),
    ] {
        let saved = memory.words_at(at);
        memory.word(at, word);
        assert!(ReferenceBox::read(&memory.bytes, array_cell).is_err());
        memory.word(at, saved);
    }
}

#[test]
fn reference_union_json_rejects_cycles_without_recursive_cell_validation() {
    let mut memory = Memory::new();
    let cell = memory.reference_cell(ValueKind::Record, 0);
    let cell_ptr = cell as u32 as usize;
    let record = memory.presence_record(&[("self", ValueKind::ReferenceUnion, cell)], 0, 1, &[0]);
    let ptr = record as u32 as usize;
    memory.word(memory.words_at(ptr) as usize, REFERENCE_VALUE_VERSION);
    memory.word(ptr - abi::STRING_HEADER_BYTES + HEAP_LINK_OFFSET, 1);
    memory.slot(cell_ptr + UNION_META_BYTES, record);
    assert!(ReferenceBox::read(&memory.bytes, cell).is_ok());
    assert!(
        Encoder::with_version(&memory.bytes, REFERENCE_VALUE_VERSION)
            .encode(ValueKind::ReferenceUnion as u32, cell)
            .unwrap_err()
            .to_string()
            .contains("cyclic object")
    );
}

#[test]
fn presence_records_validate_all_metadata_and_canonical_absent_slots() {
    let mut memory = Memory::new();
    let cell = memory.array(ValueKind::Undefined, &[0]);
    let cell_ptr = cell as u32 as usize;
    memory.word(
        cell_ptr - abi::STRING_HEADER_BYTES + HEAP_FLAGS_OFFSET,
        HEAP_ALLOCATED_FLAG | HEAP_UNION_FLAG as u32,
    );
    let record = memory.presence_record(
        &[
            ("optional", ValueKind::PrimitiveUnion, cell),
            ("kept", ValueKind::Float64, 1.5f64.to_bits()),
        ],
        1,
        2,
        &[1],
    );
    let ptr = record as u32 as usize;
    let presence = ptr + presence_offset(2);
    let descriptor = memory.words_at(ptr) as usize;
    assert_eq!(
        Encoder::with_version(&memory.bytes, MISSING_VALUE_VERSION)
            .encode(ValueKind::Record as u32, record)
            .unwrap(),
        br#"{"kept":1.5}"#
    );
    assert!(
        Encoder::with_version(&memory.bytes, UNION_VALUE_VERSION)
            .encode(ValueKind::Record as u32, record)
            .is_err()
    );
    for (at, word) in [
        (presence, 0u32),
        (presence, 4),
        (presence + 4, 0),
        (presence + 8, 1),
        (descriptor, UNION_VALUE_VERSION),
        (descriptor + 4, 3),
        (descriptor + RECORD_DESCRIPTOR_HEADER + 8, 0),
        (
            descriptor + RECORD_DESCRIPTOR_HEADER + RECORD_DESCRIPTOR_ENTRY + 8,
            OPTIONAL_FIELD_FLAG | 1,
        ),
        (
            ptr - abi::STRING_HEADER_BYTES + HEAP_FLAGS_OFFSET,
            HEAP_ALLOCATED_FLAG | HEAP_RECORD_FLAG as u32 | HEAP_UNION_FLAG as u32,
        ),
    ] {
        let saved = memory.words_at(at);
        memory.word(at, word);
        assert!(
            Encoder::with_version(&memory.bytes, MISSING_VALUE_VERSION)
                .encode(ValueKind::Record as u32, record)
                .is_err(),
            "at={at} word={word}"
        );
        memory.word(at, saved);
    }
    memory.slot(cell_ptr + UNION_META_BYTES, 1);
    assert!(
        Encoder::with_version(&memory.bytes, MISSING_VALUE_VERSION)
            .encode(ValueKind::Record as u32, record)
            .is_err()
    );
}

#[test]
fn presence_record_absent_slots_cannot_bypass_traversal_limits() {
    let mut memory = Memory::new();
    let record = memory.presence_record(
        &[
            ("a", ValueKind::Undefined, 0),
            ("b", ValueKind::Undefined, 0),
            ("c", ValueKind::Undefined, 0),
            ("d", ValueKind::Undefined, 0),
        ],
        15,
        0,
        &[],
    );
    assert_eq!(
        Encoder::with_version(&memory.bytes, MISSING_VALUE_VERSION)
            .encode(ValueKind::Record as u32, record)
            .unwrap(),
        b"{}"
    );
    let array = memory.array(ValueKind::Record, &vec![record; MAX_ARRAY_ELEMENTS]);
    assert!(
        Encoder::with_version(&memory.bytes, MISSING_VALUE_VERSION)
            .encode(ValueKind::Array as u32, array)
            .unwrap_err()
            .to_string()
            .contains("depth or node limit")
    );
}

#[test]
fn primitive_union_serialization_validates_omitted_payloads_and_counts_logical_values() {
    let mut memory = Memory::new();
    // A primitive cell has the same physical tag/count/payload size as a one-slot array,
    // but its kind flag and reference mask are separate, mandatory invariants.
    let cell = memory.array(ValueKind::Undefined, &[0]);
    let ptr = cell as u32 as usize;
    memory.word(
        ptr - abi::STRING_HEADER_BYTES + HEAP_FLAGS_OFFSET,
        HEAP_ALLOCATED_FLAG | HEAP_UNION_FLAG as u32,
    );
    let record = memory.record(&[
        ("a", ValueKind::PrimitiveUnion, cell),
        ("b", ValueKind::PrimitiveUnion, cell),
        ("c", ValueKind::PrimitiveUnion, cell),
        ("d", ValueKind::PrimitiveUnion, cell),
    ]);
    memory.word(0, UNION_VALUE_VERSION);
    assert_eq!(
        Encoder::with_version(&memory.bytes, UNION_VALUE_VERSION)
            .encode_optional(ValueKind::PrimitiveUnion as u32, cell)
            .unwrap(),
        None
    );
    assert_eq!(
        Encoder::with_version(&memory.bytes, UNION_VALUE_VERSION)
            .encode(ValueKind::Record as u32, record)
            .unwrap(),
        b"{}"
    );
    assert!(
        Encoder::with_version(&memory.bytes, UTF16_VALUE_VERSION)
            .encode_optional(ValueKind::PrimitiveUnion as u32, cell)
            .is_err()
    );
    memory.slot(ptr + UNION_META_BYTES, 1);
    assert!(
        Encoder::with_version(&memory.bytes, UNION_VALUE_VERSION)
            .encode(ValueKind::Record as u32, record)
            .is_err()
    );
    memory.slot(ptr + UNION_META_BYTES, 0);
    let array = memory.array(ValueKind::Record, &vec![record; MAX_ARRAY_ELEMENTS]);
    assert!(
        Encoder::with_version(&memory.bytes, UNION_VALUE_VERSION)
            .encode(ValueKind::Array as u32, array)
            .unwrap_err()
            .to_string()
            .contains("depth or node limit")
    );
}

#[test]
fn utf16_serialization_validates_lengths_ranges_and_versioned_descriptors() {
    let mut memory = Memory::new();
    memory.bytes[0..2].copy_from_slice(&0xd800u16.to_le_bytes());
    assert_eq!(
        Encoder::with_version(&memory.bytes, UTF16_VALUE_VERSION)
            .encode(ValueKind::String as u32, pack(0, 2))
            .unwrap(),
        br#""\ud800""#
    );
    for (ptr, len) in [
        (0, 1),
        (memory.bytes.len(), 2),
        (0, abi::MAX_STRING_BYTES + 2),
    ] {
        assert!(
            Encoder::with_version(&memory.bytes, UTF16_VALUE_VERSION)
                .encode(ValueKind::String as u32, pack(ptr, len))
                .is_err()
        );
    }
    let record = memory.record(&[("x", ValueKind::String, 0)]);
    assert!(
        Encoder::with_version(&memory.bytes, UTF16_VALUE_VERSION)
            .encode(ValueKind::Record as u32, record)
            .is_err()
    );
    memory.word(0, UTF16_VALUE_VERSION);
    assert_eq!(
        Encoder::with_version(&memory.bytes, UTF16_VALUE_VERSION)
            .encode(ValueKind::Record as u32, record)
            .unwrap(),
        br#"{"x":""}"#
    );
}

#[test]
fn undefined_serialization_checks_canonical_payloads_and_exact_version_contexts() {
    let mut memory = Memory::new();
    for version in [VALUE_VERSION, NUMBER_VALUE_VERSION] {
        assert!(
            Encoder::with_version(&memory.bytes, version)
                .encode_optional(ValueKind::Undefined as u32, 0)
                .is_err()
        );
    }
    assert_eq!(
        Encoder::with_version(&memory.bytes, UNDEFINED_VALUE_VERSION)
            .encode_optional(ValueKind::Undefined as u32, 0)
            .unwrap(),
        None
    );
    assert!(
        Encoder::with_version(&memory.bytes, UNDEFINED_VALUE_VERSION)
            .encode_optional(ValueKind::Undefined as u32, 1)
            .is_err()
    );
    let array = memory.array(ValueKind::Undefined, &[0, 0]);
    assert_eq!(
        Encoder::with_version(&memory.bytes, UNDEFINED_VALUE_VERSION)
            .encode(ValueKind::Array as u32, array)
            .unwrap(),
        b"[null,null]"
    );
    memory.slot(array as u32 as usize + ARRAY_META_BYTES, 1);
    assert!(
        Encoder::with_version(&memory.bytes, UNDEFINED_VALUE_VERSION)
            .encode(ValueKind::Array as u32, array)
            .is_err()
    );
    let record = memory.record(&[
        ("first", ValueKind::Undefined, 0),
        ("keep", ValueKind::Float64, 1.5f64.to_bits()),
        ("last", ValueKind::Undefined, 0),
    ]);
    let ptr = record as u32 as usize;
    let descriptor = u32::from_le_bytes(
        memory.bytes[ptr..ptr + size_of::<u32>()]
            .try_into()
            .unwrap(),
    ) as usize;
    memory.word(descriptor, UNDEFINED_VALUE_VERSION);
    assert_eq!(
        Encoder::with_version(&memory.bytes, UNDEFINED_VALUE_VERSION)
            .encode(ValueKind::Record as u32, record)
            .unwrap(),
        br#"{"keep":1.5}"#
    );
    memory.slot(ptr + RECORD_META_BYTES + 2 * RECORD_SLOT_BYTES, 1);
    assert!(
        Encoder::with_version(&memory.bytes, UNDEFINED_VALUE_VERSION)
            .encode(ValueKind::Record as u32, record)
            .is_err()
    );
}

#[test]
fn omitted_undefined_fields_still_consume_the_json_traversal_budget() {
    let mut memory = Memory::new();
    let record = memory.record(&[
        ("a", ValueKind::Undefined, 0),
        ("b", ValueKind::Undefined, 0),
        ("c", ValueKind::Undefined, 0),
        ("d", ValueKind::Undefined, 0),
    ]);
    memory.word(0, UNDEFINED_VALUE_VERSION);
    let array = memory.array(ValueKind::Record, &vec![record; MAX_ARRAY_ELEMENTS]);
    let error = Encoder::with_version(&memory.bytes, UNDEFINED_VALUE_VERSION)
        .encode(ValueKind::Array as u32, array)
        .unwrap_err();
    assert!(error.to_string().contains("depth or node limit"));
}

#[test]
fn serializer_context_does_not_reinterpret_numeric_tags_or_descriptor_versions() {
    let mut memory = Memory::new();
    let number = 1.5f64.to_bits();
    assert!(memory.encode(ValueKind::Float64, number).is_err());
    assert!(
        Encoder::with_version(&memory.bytes, NUMBER_VALUE_VERSION)
            .encode(ValueKind::I32 as u32, 1)
            .is_err()
    );
    let array = memory.array(ValueKind::Float64, &[]);
    assert!(memory.encode(ValueKind::Array, array).is_err());
    let record = memory.record(&[("n", ValueKind::Float64, number)]);
    assert!(
        Encoder::with_version(&memory.bytes, NUMBER_VALUE_VERSION)
            .encode(ValueKind::Record as u32, record)
            .is_err()
    );
    let ptr = record as u32 as usize;
    let descriptor = u32::from_le_bytes(
        memory.bytes[ptr..ptr + size_of::<u32>()]
            .try_into()
            .unwrap(),
    ) as usize;
    memory.word(descriptor, NUMBER_VALUE_VERSION);
    assert_eq!(
        Encoder::with_version(&memory.bytes, NUMBER_VALUE_VERSION)
            .encode(ValueKind::Record as u32, record)
            .unwrap(),
        br#"{"n":1.5}"#
    );
    assert!(memory.encode(ValueKind::Record, record).is_err());
}

struct Memory {
    bytes: Vec<u8>,
    heap: usize,
    static_end: usize,
}
impl Memory {
    fn reference_cell(&mut self, kind: ValueKind, payload: u64) -> u64 {
        let cell = self.array(kind, &[payload]);
        let ptr = cell as u32 as usize;
        self.word(
            ptr - abi::STRING_HEADER_BYTES + HEAP_FLAGS_OFFSET,
            HEAP_ALLOCATED_FLAG | HEAP_UNION_FLAG as u32,
        );
        self.word(
            ptr - abi::STRING_HEADER_BYTES + HEAP_LINK_OFFSET,
            u32::from(kind.is_reference()),
        );
        cell
    }
    fn presence_record(
        &mut self,
        fields: &[(&str, ValueKind, u64)],
        optional: u32,
        present: u32,
        order: &[u32],
    ) -> u64 {
        let value = self.record(fields);
        let ptr = value as u32 as usize;
        let descriptor = self.words_at(ptr);
        let length = record_bytes(MISSING_VALUE_VERSION, fields.len());
        self.heap = (ptr + length + HEAP_ALIGNMENT - 1) & !(HEAP_ALIGNMENT - 1);
        self.word(
            ptr - abi::STRING_HEADER_BYTES,
            (self.heap - ptr + abi::STRING_HEADER_BYTES) as u32,
        );
        self.word(descriptor as usize, MISSING_VALUE_VERSION);
        for i in 0..fields.len() {
            self.word(
                descriptor as usize + RECORD_DESCRIPTOR_HEADER + i * RECORD_DESCRIPTOR_ENTRY + 8,
                i as u32
                    | if optional & (1u32 << i) != 0 {
                        OPTIONAL_FIELD_FLAG
                    } else {
                        0
                    },
            );
        }
        self.word(ptr + presence_offset(fields.len()), present);
        for i in 0..fields.len() {
            self.word(
                ptr + presence_offset(fields.len()) + 4 + i * 4,
                order.get(i).copied().unwrap_or(0),
            );
        }
        pack(ptr, length)
    }
    fn words_at(&self, at: usize) -> u32 {
        u32::from_le_bytes(self.bytes[at..at + 4].try_into().unwrap())
    }
    fn new() -> Self {
        Self {
            bytes: vec![0; abi::OWNED_STRING_BASE + abi::WASM_PAGE_BYTES],
            heap: abi::OWNED_STRING_BASE,
            static_end: 0,
        }
    }
    fn word(&mut self, at: usize, value: u32) {
        self.bytes[at..at + size_of::<u32>()].copy_from_slice(&value.to_le_bytes());
    }
    fn slot(&mut self, at: usize, value: u64) {
        self.bytes[at..at + RECORD_SLOT_BYTES].copy_from_slice(&value.to_le_bytes());
    }
    fn record(&mut self, fields: &[(&str, ValueKind, u64)]) -> u64 {
        let descriptor = self.static_end;
        self.static_end += RECORD_DESCRIPTOR_HEADER + fields.len() * RECORD_DESCRIPTOR_ENTRY;
        let header = self.heap;
        let ptr = header + abi::STRING_HEADER_BYTES;
        let length = RECORD_META_BYTES + fields.len() * RECORD_SLOT_BYTES;
        self.heap += abi::STRING_HEADER_BYTES + length;
        self.word(header, (self.heap - header) as u32);
        self.word(header + size_of::<u32>(), 1 | HEAP_RECORD_FLAG as u32);
        self.word(ptr, descriptor as u32);
        self.word(ptr + size_of::<u32>(), fields.len() as u32);
        self.word(descriptor, VALUE_VERSION);
        self.word(descriptor + size_of::<u32>(), fields.len() as u32);
        for (index, (name, kind, value)) in fields.iter().enumerate() {
            let key = self.static_end;
            self.static_end += name.len();
            self.bytes[key..self.static_end].copy_from_slice(name.as_bytes());
            let entry = descriptor + RECORD_DESCRIPTOR_HEADER + index * RECORD_DESCRIPTOR_ENTRY;
            for (offset, word) in [key as u32, name.len() as u32, index as u32, *kind as u32]
                .into_iter()
                .enumerate()
            {
                self.word(entry + offset * size_of::<u32>(), word);
            }
            self.slot(ptr + RECORD_META_BYTES + index * RECORD_SLOT_BYTES, *value);
        }
        pack(ptr, length)
    }
    fn array(&mut self, kind: ValueKind, values: &[u64]) -> u64 {
        let header = self.heap;
        let ptr = header + abi::STRING_HEADER_BYTES;
        let length = ARRAY_META_BYTES + values.len() * ARRAY_SLOT_BYTES;
        self.heap += abi::STRING_HEADER_BYTES + length;
        self.word(header, (self.heap - header) as u32);
        self.word(header + size_of::<u32>(), 1 | HEAP_ARRAY_FLAG as u32);
        self.word(ptr, kind as u32);
        self.word(ptr + ARRAY_LENGTH_OFFSET as usize, values.len() as u32);
        for (index, value) in values.iter().enumerate() {
            self.slot(ptr + ARRAY_META_BYTES + index * ARRAY_SLOT_BYTES, *value);
        }
        pack(ptr, length)
    }
    fn encode(&self, kind: ValueKind, value: u64) -> Result<Vec<u8>, wasmtime::Error> {
        Encoder::new(&self.bytes).encode(kind as u32, value)
    }
}
fn pack(ptr: usize, length: usize) -> u64 {
    ((length as u64) << i32::BITS) | ptr as u64
}

#[test]
fn serializer_validates_scalar_tags_ranges_and_utf8() {
    let memory = Memory::new();
    assert_eq!(
        memory.encode(ValueKind::I32, (-1i64) as u64).unwrap(),
        b"-1"
    );
    for (kind, packed) in [
        (ValueKind::Null, 1),
        (ValueKind::Boolean, 2),
        (ValueKind::I32, u32::MAX as u64),
        (ValueKind::I32, 1u64 << i32::BITS),
        (ValueKind::String, pack(u32::MAX as usize, 1)),
        (ValueKind::String, pack(0, abi::MAX_STRING_BYTES + 1)),
        (ValueKind::Record, pack(0, RECORD_META_BYTES)),
        (
            ValueKind::Record,
            pack(u32::MAX as usize, u32::MAX as usize),
        ),
    ] {
        assert!(memory.encode(kind, packed).is_err(), "{kind:?} {packed}");
    }
    assert!(Encoder::new(&[]).encode(u32::MAX, 0).is_err());
    assert!(
        Encoder::new(&[0xff])
            .encode(ValueKind::String as u32, pack(0, 1))
            .is_err()
    );
}

#[test]
fn serializer_rejects_corrupt_heap_and_static_metadata() {
    let mut memory = Memory::new();
    let value = memory.record(&[("x", ValueKind::I32, 7), ("y", ValueKind::Boolean, 1)]);
    assert_eq!(
        memory.encode(ValueKind::Record, value).unwrap(),
        b"{\"x\":7,\"y\":true}"
    );
    let ptr = value as u32 as usize;
    let header = ptr - abi::STRING_HEADER_BYTES;
    let entry = RECORD_DESCRIPTOR_HEADER;
    for (at, word) in [
        (header, 0),
        (header, u32::MAX),
        (header + size_of::<u32>(), 0),
        (ptr, abi::RESULT_BASE as u32),
        (ptr, u32::MAX),
        (ptr + size_of::<u32>(), MAX_RECORD_FIELDS as u32 + 1),
        (0, VALUE_VERSION + 1),
        (size_of::<u32>(), 0),
        (entry, u32::MAX),
        (entry + size_of::<u32>(), MAX_RECORD_KEY_BYTES as u32 + 1),
        (entry + 2 * size_of::<u32>(), 2),
        (entry + 3 * size_of::<u32>(), u32::MAX),
        (entry + RECORD_DESCRIPTOR_ENTRY + 2 * size_of::<u32>(), 0),
    ] {
        let mut bad = Memory {
            bytes: memory.bytes.clone(),
            heap: memory.heap,
            static_end: memory.static_end,
        };
        bad.word(at, word);
        assert!(
            bad.encode(ValueKind::Record, value).is_err(),
            "offset {at} value {word}"
        );
    }
    let key = memory.bytes[entry..entry + size_of::<u32>()]
        .try_into()
        .unwrap();
    let key = u32::from_le_bytes(key);
    memory.word(entry + RECORD_DESCRIPTOR_ENTRY, key);
    assert!(
        memory
            .encode(ValueKind::Record, value)
            .unwrap_err()
            .to_string()
            .contains("duplicate")
    );
    memory.bytes[key as usize] = 0xff;
    assert!(
        memory
            .encode(ValueKind::Record, value)
            .unwrap_err()
            .to_string()
            .contains("UTF-8")
    );
}

#[test]
fn serializer_bounds_cycles_depth_and_repeated_alias_expansion() {
    let mut memory = Memory::new();
    let leaf = memory.record(&[("v", ValueKind::Null, 0)]);
    let aliases = memory.record(&[
        ("a", ValueKind::Record, leaf),
        ("b", ValueKind::Record, leaf),
    ]);
    assert_eq!(
        memory.encode(ValueKind::Record, aliases).unwrap(),
        b"{\"a\":{\"v\":null},\"b\":{\"v\":null}}"
    );
    let cycle = memory.record(&[("self", ValueKind::Record, 0)]);
    memory.slot(cycle as u32 as usize + RECORD_META_BYTES, cycle);
    assert!(
        memory
            .encode(ValueKind::Record, cycle)
            .unwrap_err()
            .to_string()
            .contains("cyclic")
    );
    let mut deep = leaf;
    for _ in 0..MAX_JSON_DEPTH {
        deep = memory.record(&[("v", ValueKind::Record, deep)]);
    }
    assert!(
        memory
            .encode(ValueKind::Record, deep)
            .unwrap_err()
            .to_string()
            .contains("depth or node")
    );
    let mut wide = leaf;
    for _ in 0..MAX_JSON_NODES.ilog2() {
        wide = memory.record(&[
            ("a", ValueKind::Record, wide),
            ("b", ValueKind::Record, wide),
        ]);
    }
    assert!(
        memory
            .encode(ValueKind::Record, wide)
            .unwrap_err()
            .to_string()
            .contains("depth or node")
    );
}

#[test]
fn serializer_checks_array_blocks_counts_tags_and_nested_aliases() {
    let mut memory = Memory::new();
    let leaf = memory.record(&[("value", ValueKind::Null, 0)]);
    let array = memory.array(ValueKind::Record, &[leaf, leaf]);
    assert_eq!(
        memory.encode(ValueKind::Array, array).unwrap(),
        b"[{\"value\":null},{\"value\":null}]"
    );
    let ptr = array as u32 as usize;
    let header = ptr - abi::STRING_HEADER_BYTES;
    for (at, word) in [
        (header, 0),
        (header, u32::MAX),
        (header + size_of::<u32>(), 1 | HEAP_RECORD_FLAG as u32),
        (
            header + size_of::<u32>(),
            1 | HEAP_ARRAY_FLAG as u32 | HEAP_RECORD_FLAG as u32,
        ),
        (ptr, u32::MAX),
        (ptr + ARRAY_LENGTH_OFFSET as usize, 0),
        (
            ptr + ARRAY_LENGTH_OFFSET as usize,
            MAX_ARRAY_ELEMENTS as u32 + 1,
        ),
    ] {
        let mut bad = Memory {
            bytes: memory.bytes.clone(),
            heap: memory.heap,
            static_end: memory.static_end,
        };
        bad.word(at, word);
        assert!(
            bad.encode(ValueKind::Array, array).is_err(),
            "offset {at}: {word}"
        );
    }
    let empty = memory.array(ValueKind::Boolean, &[]);
    assert_eq!(memory.encode(ValueKind::Array, empty).unwrap(), b"[]");
    memory.word(empty as u32 as usize, u32::MAX);
    assert!(memory.encode(ValueKind::Array, empty).is_err());
    let noncanonical = memory.array(ValueKind::Boolean, &[2]);
    assert!(memory.encode(ValueKind::Array, noncanonical).is_err());
    let noncanonical = memory.array(ValueKind::I32, &[u32::MAX as u64]);
    assert!(memory.encode(ValueKind::Array, noncanonical).is_err());
    assert!(
        memory
            .encode(ValueKind::Array, pack(0, ARRAY_META_BYTES))
            .is_err()
    );
    assert!(
        memory
            .encode(ValueKind::Array, pack(u32::MAX as usize, u32::MAX as usize))
            .is_err()
    );
}

#[test]
fn serializer_bounds_array_cycles_depth_and_expansion() {
    let mut memory = Memory::new();
    let cycle = memory.array(ValueKind::Array, &[0]);
    memory.slot(cycle as u32 as usize + ARRAY_META_BYTES, cycle);
    assert!(
        memory
            .encode(ValueKind::Array, cycle)
            .unwrap_err()
            .to_string()
            .contains("cyclic")
    );
    let leaf = memory.array(ValueKind::Boolean, &[1]);
    let mut deep = leaf;
    for _ in 0..MAX_JSON_DEPTH {
        deep = memory.array(ValueKind::Array, &[deep]);
    }
    assert!(
        memory
            .encode(ValueKind::Array, deep)
            .unwrap_err()
            .to_string()
            .contains("depth or node")
    );
    let wide = memory.array(ValueKind::Array, &vec![leaf; MAX_ARRAY_ELEMENTS]);
    assert!(memory.encode(ValueKind::Array, wide).is_ok());
    let excess = memory.array(ValueKind::Array, &[wide, wide]);
    assert!(
        memory
            .encode(ValueKind::Array, excess)
            .unwrap_err()
            .to_string()
            .contains("depth or node")
    );
}

#[test]
fn legacy_record_descriptor_versions_retain_their_scalar_meaning() {
    let mut memory = Memory::new();
    let value = memory.record(&[("v", ValueKind::I32, (-7i64) as u64)]);
    memory.word(0, LEGACY_VALUE_VERSION);
    assert_eq!(
        memory.encode(ValueKind::Record, value).unwrap(),
        b"{\"v\":-7}"
    );
}

#[test]
fn resizable_array_json_validates_backing_metadata_spare_slots_and_cycles() {
    fn vector(memory: &mut Memory, kind: ValueKind, values: &[u64], capacity: usize) -> u64 {
        let mut slots = values.to_vec();
        slots.resize(capacity, 0);
        let storage = memory.array(kind, &slots);
        let backing = storage as u32 as usize;
        memory.word(backing + 4, values.len() as u32);
        memory.word(
            backing - abi::STRING_HEADER_BYTES + HEAP_LINK_OFFSET,
            u32::from(kind.is_reference()),
        );
        let cell = memory.array(ValueKind::Array, &[storage]);
        let ptr = cell as u32 as usize;
        memory.word(
            ptr - abi::STRING_HEADER_BYTES + HEAP_FLAGS_OFFSET,
            HEAP_ALLOCATED_FLAG | HEAP_VECTOR_FLAG as u32,
        );
        memory.word(ptr - abi::STRING_HEADER_BYTES + HEAP_LINK_OFFSET, 1);
        memory.word(ptr, kind as u32);
        memory.word(ptr + 4, values.len() as u32);
        cell
    }
    let mut memory = Memory::new();
    let value = vector(&mut memory, ValueKind::Float64, &[1.25f64.to_bits()], 2);
    assert_eq!(
        Encoder::with_version(&memory.bytes, VECTOR_VALUE_VERSION)
            .encode(ValueKind::Array as u32, value)
            .unwrap(),
        b"[1.25]"
    );
    assert!(
        Encoder::with_version(&memory.bytes, REFERENCE_VALUE_VERSION)
            .encode(ValueKind::Array as u32, value)
            .is_err()
    );
    let layout = Vector::read(&memory.bytes, value).unwrap();
    for (at, word) in [
        (layout.ptr + 4, 2),
        (layout.storage + 4, 2),
        (layout.storage + ARRAY_META_BYTES + ARRAY_SLOT_BYTES, 1),
        (layout.ptr - abi::STRING_HEADER_BYTES + HEAP_LINK_OFFSET, 0),
    ] {
        let saved = memory.words_at(at);
        memory.word(at, word);
        assert!(
            Encoder::with_version(&memory.bytes, VECTOR_VALUE_VERSION)
                .encode(ValueKind::Array as u32, value)
                .is_err()
        );
        memory.word(at, saved);
    }
    let record =
        memory.presence_record(&[("n", ValueKind::Float64, 1.25f64.to_bits())], 0, 1, &[0]);
    let ptr = record as u32 as usize;
    memory.word(memory.words_at(ptr) as usize, VECTOR_VALUE_VERSION);
    memory.word(ptr - abi::STRING_HEADER_BYTES + HEAP_LINK_OFFSET, 0);
    assert!(
        Encoder::with_version(&memory.bytes, VECTOR_VALUE_VERSION)
            .encode(ValueKind::Record as u32, record)
            .is_ok()
    );
    memory.word(
        ptr - abi::STRING_HEADER_BYTES + HEAP_FLAGS_OFFSET,
        HEAP_ALLOCATED_FLAG | HEAP_RECORD_FLAG as u32 | HEAP_VECTOR_FLAG as u32,
    );
    assert!(
        Encoder::with_version(&memory.bytes, VECTOR_VALUE_VERSION)
            .encode(ValueKind::Record as u32, record)
            .is_err()
    );
    let child = vector(&mut memory, ValueKind::Boolean, &[2], 1);
    assert!(
        Encoder::with_version(&memory.bytes, VECTOR_VALUE_VERSION)
            .encode(ValueKind::Array as u32, child)
            .is_err()
    );
    let cycle = vector(&mut memory, ValueKind::Array, &[0], 1);
    let layout = Vector::read(&memory.bytes, cycle).unwrap();
    memory.slot(layout.storage + ARRAY_META_BYTES, cycle);
    assert!(Vector::read(&memory.bytes, cycle).is_ok());
    assert!(
        Encoder::with_version(&memory.bytes, VECTOR_VALUE_VERSION)
            .encode(ValueKind::Array as u32, cycle)
            .unwrap_err()
            .to_string()
            .contains("cyclic object")
    );
}
