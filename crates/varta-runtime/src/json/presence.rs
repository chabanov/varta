//! Value v7 records: fixed typed slots plus own-property presence/insertion order.
use super::*;
use varta_common::value::{INDEX_SIGNATURE_KEY, array_index_units};

/// A record member in serialization order.
#[derive(Clone, Copy)]
enum Member {
    Field(usize),
    /// An entry of the class instance's index dictionary, by position.
    Entry(usize),
}
/// The present entries of an index dictionary in stored order: their array
/// index (if the key is one), packed key, value kind and payload.
type Entries = Vec<(Option<u32>, u64, u32, u64)>;

impl Encoder<'_, '_> {
    /// The present entries of the index dictionary `packed` of a class
    /// instance (holes of an indexed dictionary skipped).
    fn index_entries(&mut self, packed: u64) -> Result<Entries, wasmtime::Error> {
        let dictionary = Dictionary::read_in(self.data, packed, self.version, self.module)
            .map_err(wasmtime::Error::new)?;
        let mut entries = Vec::new();
        for position in 0..dictionary.keys.length {
            let Some((key, value)) = dictionary
                .entry(self.data, position)
                .map_err(wasmtime::Error::new)?
            else {
                continue;
            };
            let units = varta_common::text::units(
                self.bytes(key as u32 as usize, (key >> i32::BITS) as usize)?,
            )
            .map_err(|_| error("invalid UTF-16 byte length"))?;
            entries.push((
                array_index_units(units.iter().copied()),
                key,
                dictionary.values.kind as u32,
                value,
            ));
        }
        Ok(entries)
    }
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
        if !self.module.static_region.contains(
            descriptor,
            RECORD_DESCRIPTOR_HEADER + count * RECORD_DESCRIPTOR_ENTRY,
        ) {
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
                || !self
                    .module
                    .static_region
                    .contains(key as usize, len as usize)
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
            if function_layout(self.version) && name == "toJSON" && present & (1u128 << slot) != 0 {
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
        // V38 (`declaration_forms`): the entries of a class instance's index
        // dictionary are its own properties beside the fields.
        let index = fields.iter().position(|field| {
            matches!(field, Some((name, kind, value))
                if *name == INDEX_SIGNATURE_KEY
                    && *kind == ValueKind::Dictionary as u32
                    && *value != 0)
        });
        let extra = match index {
            Some(slot) => {
                let (_, _, value) = fields[slot].ok_or_else(|| error("missing field slot"))?;
                Some(self.index_entries(value)?)
            }
            None => None,
        };
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
        // Own-key order: array indices of fields and entries ascending, then
        // the fields, then the other entries, each in insertion order.
        let mut members: Vec<(u8, u32, Member)> = rank
            .into_iter()
            .map(|(noninteger, key, slot)| (u8::from(noninteger), key, Member::Field(slot)))
            .collect();
        if let Some(entries) = &extra {
            self.visit(depth + 1)?;
            for (position, (index, ..)) in entries.iter().enumerate() {
                let member = Member::Entry(position);
                members.push(match index {
                    Some(index) => (0, *index, member),
                    None => (2, position as u32, member),
                });
            }
        }
        members.sort_by_key(|(class, key, _)| (*class, *key));
        for (.., member) in members {
            let slot = match member {
                Member::Field(slot) => slot,
                Member::Entry(position) => {
                    let Some(entries) = &extra else {
                        return Err(error("missing index entries"));
                    };
                    let (_, key, kind, value) = entries[position];
                    let (kind, value) = self.resolve(kind, value)?;
                    if kind == ValueKind::Undefined as u32 {
                        self.visit(depth + 1)?;
                        if value != 0 {
                            return Err(error("noncanonical undefined"));
                        }
                        continue;
                    }
                    if written {
                        self.write(b",")?;
                    }
                    written = true;
                    let start = key as u32 as usize;
                    let units =
                        varta_common::text::units(self.bytes(start, (key >> i32::BITS) as usize)?)
                            .map_err(|_| error("invalid UTF-16 byte length"))?;
                    varta_common::text::write_json(&mut self.output, &units)
                        .map_err(|_| error("output byte limit"))?;
                    self.write(b":")?;
                    self.value(kind, value, depth + 1)?;
                    continue;
                }
            };
            let (name, kind, value) = fields[slot].ok_or_else(|| error("missing field slot"))?;
            if kind == ValueKind::Undefined as u32 || Some(slot) == index {
                continue;
            }
            // V37 private state is not a property (`json_stringify_v17`).
            if self.hide_private && name.starts_with(PRIVATE_KEY_PREFIX) {
                self.visit(depth + 1)?;
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
