//! Value v7 records: fixed typed slots plus own-property presence/insertion order.
use super::*;
impl Encoder<'_, '_> {
    pub(super) fn record_present(
        &mut self,
        packed: u64,
        depth: usize,
    ) -> Result<(), wasmtime::Error> {
        let max = max_record_fields(self.version);
        let (ptr, length) =
            self.managed(packed, HEAP_RECORD_FLAG, record_bytes(self.version, max))?;
        let [descriptor, count] = self.words::<2>(ptr)?;
        let count = count as usize;
        if count > max || length != record_bytes(self.version, count) {
            return Err(error("invalid presence record length"));
        }
        if self.path.contains(&ptr) {
            return Err(error("cyclic object"));
        }
        let descriptor = descriptor as usize;
        if descriptor
            .checked_add(RECORD_DESCRIPTOR_HEADER + count * RECORD_DESCRIPTOR_ENTRY)
            .is_none_or(|end| end > abi::RESULT_BASE)
        {
            return Err(error("descriptor outside static memory"));
        }
        if self.words::<2>(descriptor)? != [self.version, count as u32] {
            return Err(error("invalid presence descriptor version or count"));
        }
        let present = read_slot_set(
            self.data,
            ptr + presence_offset(count),
            presence_words(count),
        )
        .ok_or_else(|| error("presence outside memory"))?;
        if present & !wide_field_mask(count) != 0 {
            return Err(error("presence outside record shape"));
        }
        let mut order = Vec::new();
        let mut seen = 0u128;
        for index in 0..count {
            let [slot] = self.words::<1>(ptr + order_offset(count) + index * 4)?;
            if index < present.count_ones() as usize {
                if slot as usize >= count
                    || seen & (1u128 << slot) != 0
                    || present & (1u128 << slot) == 0
                {
                    return Err(error("invalid property insertion order"));
                }
                seen |= 1u128 << slot;
                order.push(slot as usize);
            } else if slot != 0 {
                return Err(error("noncanonical unused property order"));
            }
        }
        if seen != present {
            return Err(error("incomplete property insertion order"));
        }
        let mut fields = vec![None; count];
        let mut names = BTreeSet::new();
        for index in 0..count {
            let [key, len, raw_slot, kind] = self.words::<4>(
                descriptor + RECORD_DESCRIPTOR_HEADER + index * RECORD_DESCRIPTOR_ENTRY,
            )?;
            let optional = raw_slot & OPTIONAL_FIELD_FLAG != 0;
            let slot = (raw_slot & !OPTIONAL_FIELD_FLAG) as usize;
            if slot >= count
                || fields[slot].is_some()
                || len as usize > MAX_RECORD_KEY_BYTES
                || (key as usize)
                    .checked_add(len as usize)
                    .is_none_or(|end| end > abi::RESULT_BASE)
            {
                return Err(error("invalid presence field metadata"));
            }
            let name = std::str::from_utf8(self.bytes(key as usize, len as usize)?)
                .map_err(|_| error("invalid field UTF-8"))?;
            if !names.insert(name) {
                return Err(error("duplicate object field"));
            }
            let tag = ValueKind::from_tag(kind)
                .filter(|k| k.supported_by(self.version))
                .ok_or_else(|| error("incompatible field kind"))?;
            if optional
                && !matches!(
                    tag,
                    ValueKind::Undefined | ValueKind::PrimitiveUnion | ValueKind::ReferenceUnion
                )
            {
                return Err(error("optional field must admit undefined"));
            }
            let [lo, hi] = self.words::<2>(ptr + RECORD_META_BYTES + slot * RECORD_SLOT_BYTES)?;
            if self.version == FUNCTION_VALUE_VERSION
                && name == "toJSON"
                && present & (1u128 << slot) != 0
            {
                let payload = ((hi as u64) << i32::BITS) | lo as u64;
                let callable = if tag == ValueKind::ReferenceUnion {
                    ReferenceBox::read_envelope(self.data, payload, self.version)
                        .map_err(wasmtime::Error::new)?
                        .kind
                        == ValueKind::Function
                } else {
                    tag == ValueKind::Function
                };
                if callable {
                    return Err(error("toJSON hooks require guest call dispatch"));
                }
            }
            let (kind, value) = self.resolve(kind, ((hi as u64) << i32::BITS) | lo as u64)?;
            if present & (1u128 << slot) == 0
                && (!optional || kind != ValueKind::Undefined as u32 || value != 0)
            {
                return Err(error("invalid absent field payload or required presence"));
            }
            fields[slot] = Some((name, kind, value));
        }
        // Integer-index keys always precede other keys, including fields first
        // assigned after construction. Other names preserve insertion order.
        let rank: Vec<_> = order
            .iter()
            .enumerate()
            .map(|(ordinal, slot)| {
                let (name, _, _) = fields[*slot].ok_or_else(|| error("missing field slot"))?;
                let index = name
                    .parse::<u32>()
                    .ok()
                    .filter(|n| *n < u32::MAX && n.to_string() == name);
                Ok((index.is_none(), index.unwrap_or(ordinal as u32), *slot))
            })
            .collect::<Result<_, wasmtime::Error>>()?;
        let mut rank = rank;
        rank.sort_by_key(|(noninteger, index, _)| (*noninteger, *index));
        self.path.push(ptr);
        self.write(b"{")?;
        let mut written = false;
        // Every reserved slot, including absent ones, consumes the traversal budget.
        for (slot, field) in fields.iter().enumerate() {
            let (_, kind, value) = field.ok_or_else(|| error("missing field slot"))?;
            if present & (1u128 << slot) == 0 || kind == ValueKind::Undefined as u32 {
                self.visit(depth + 1)?;
                if value != 0 {
                    return Err(error("noncanonical undefined"));
                }
            }
        }
        for (_, _, slot) in rank {
            let (name, kind, value) = fields[slot].ok_or_else(|| error("missing field slot"))?;
            if kind == ValueKind::Undefined as u32 {
                continue;
            }
            if written {
                self.write(b",")?;
            }
            written = true;
            self.quoted(name)?;
            self.write(b":")?;
            self.value(kind, value, depth + 1)?;
        }
        self.write(b"}")?;
        self.path.pop();
        Ok(())
    }
}
