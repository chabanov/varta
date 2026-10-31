//! The builders of the record, array, dictionary and union nodes:
//! `(dst) -> handle`, each publishing its block at `dst` (or retaining it,
//! `dst` 0) before any child is allocated, in the layout, order and
//! allocation sequence of `varta_common::json::build`.
use super::asm::{SLOT, WORD, at};
use super::scan::{ARRAY_META, SLOT_BYTES, UNION_BYTES, VECTOR_BYTES, next, sized};
use super::*;
use varta_common::json::JsonField;
use varta_common::value as layout;
use wasm_encoder::Instruction as I;

const DST: u32 = 0;

pub(super) fn node(
    plan: &Plan,
    context: Context,
    index: u32,
    node: &JsonType,
) -> Result<wasm_encoder::Function> {
    let globals = context.globals;
    Ok(match node {
        JsonType::Record { descriptor, fields } => {
            let shape = plan.graph.records[index as usize]
                .ok_or_else(|| CompilerError::Internal("record node without a shape".into()))?;
            record(plan, globals, *descriptor, fields, shape)
        }
        JsonType::Array { element } => array(plan, globals, *element),
        JsonType::Dictionary { descriptor, value } => {
            dictionary(plan, globals, *descriptor, *value)
        }
        JsonType::PrimitiveUnion { members } | JsonType::ReferenceUnion { members } => {
            union(plan, globals, members)
        }
        _ => {
            return Err(CompilerError::Internal(
                "no decoder builder for this node".into(),
            ));
        }
    })
}

/// Build node `child` into the slot at the address `slot` pushes: a
/// container publishes itself there, any other payload is stored.
fn child(a: &mut Asm, plan: &Plan, child: u32, slot: impl Fn(&mut Asm)) {
    if plan.publishes(child) {
        slot(a);
        a.i(I::Call(plan.build(child))).i(I::Drop);
    } else {
        slot(a);
        a.int(0).i(I::Call(plan.build(child))).i(I::I64Store(SLOT));
    }
}

/// The address of slot `slot` of the block in `ptr` (pushes it).
fn field_slot(a: &mut Asm, ptr: u32, slot: usize) {
    a.get(ptr)
        .int((layout::RECORD_META_BYTES + slot * layout::RECORD_SLOT_BYTES) as i32)
        .i(I::I32Add);
}

/// A stamped record block of `len` bytes: descriptor and count words, zero
/// slots and trailers, published at `dst`.
fn record_block(
    a: &mut Asm,
    plan: &Plan,
    ptr: u32,
    (descriptor, count, len, link): (u32, usize, usize, u32),
) {
    a.int(len as i32)
        .int(layout::HEAP_RECORD_FLAG)
        .int(link as i32)
        .i(I::Call(plan.shared(Shared::Block)))
        .tee(ptr)
        .int(descriptor as i32)
        .i(I::I32Store(WORD));
    a.get(ptr)
        .int(count as i32)
        .i(I::I32Store(at(WORD, size_of::<u32>() as u64)));
    a.get(ptr)
        .int(layout::RECORD_META_BYTES as i32)
        .i(I::I32Add)
        .int(0)
        .int((len - layout::RECORD_META_BYTES) as i32)
        .i(I::MemoryFill(strings::MEMORY));
    a.get(DST);
    sized(a, len as i32, ptr);
    a.i(I::Call(plan.shared(Shared::Publish)));
}

/// A record: the block, then each present field (in input order, by slot),
/// then `undefined` cells for the missing fields of a non-`undefined` type,
/// the presence words and the order words of the present fields.
fn record(
    plan: &Plan,
    globals: Globals,
    descriptor: u32,
    fields: &[JsonField],
    shape: u32,
) -> wasm_encoder::Function {
    let mut a = Asm::new(1);
    let ptr = a.local(ValType::I32);
    let remaining = a.local(ValType::I32);
    let slot = a.local(ValType::I32);
    let present = a.local(ValType::I64);
    let order = a.local(ValType::I32);
    let count = fields.len();
    let version = layout::HASHED_DICTIONARY_VALUE_VERSION;
    let len = layout::record_bytes(version, count);
    // Wide records trace through their descriptor's edge bitmap.
    let link = if layout::is_wide_record(count) {
        0
    } else {
        fields
            .iter()
            .enumerate()
            .filter(|(_, field)| plan.kind(field.ty).is_reference())
            .fold(0u32, |mask, (slot, _)| mask | 1 << slot)
    };
    record_block(&mut a, plan, ptr, (descriptor, count, len, link));
    next(&mut a, globals);
    a.i(I::I32WrapI64).set(remaining);
    a.block("fields").loop_("field");
    a.get(remaining).i(I::I32Eqz).br_if("fields");
    a.add(remaining, -1);
    next(&mut a, globals);
    a.i(I::I32WrapI64).set(slot);
    a.get(present)
        .long(1)
        .get(slot)
        .i(I::I64ExtendI32U)
        .i(I::I64Shl)
        .i(I::I64Or)
        .set(present);
    a.block("built");
    let arms: Vec<String> = (0..count).map(|slot| format!("field{slot}")).collect();
    for name in arms.iter().rev() {
        a.block(name);
    }
    let default = arms.last().cloned().unwrap_or_else(|| "built".to_owned());
    a.get(slot).br_table(&arms, &default);
    for (slot, field) in fields.iter().enumerate() {
        a.end();
        child(&mut a, plan, field.ty, |a| field_slot(a, ptr, slot));
        a.br("built");
    }
    a.end();
    a.br("field");
    a.end().end();
    // Missing fields: an `undefined` cell unless the field's type is
    // `undefined` itself (its zero payload stays).
    for (slot, field) in fields.iter().enumerate() {
        if !field.optional || matches!(plan.graph.nodes[field.ty as usize], JsonType::Undefined) {
            continue;
        }
        a.get(present)
            .long(1i64 << slot)
            .i(I::I64And)
            .i(I::I64Eqz)
            .if_();
        field_slot(&mut a, ptr, slot);
        a.int(ValueKind::Undefined as i32)
            .int(0)
            .i(I::Call(plan.shared(Shared::Cell)))
            .i(I::Drop);
        a.end();
    }
    for word in 0..layout::presence_words(count) {
        a.get(ptr)
            .get(present)
            .long((layout::PRESENCE_WORD_BITS * word) as i64)
            .i(I::I64ShrU)
            .i(I::I32WrapI64)
            .i(I::I32Store(at(
                WORD,
                (layout::presence_offset(count) + word * size_of::<u32>()) as u64,
            )));
    }
    // Order words: the canonical own-key order of the present fields; the
    // remaining words stay zero.
    let (_, canonical) = super::super::records::canonical_layout(shape, plan.shapes);
    a.get(ptr)
        .int(layout::order_offset(count) as i32)
        .i(I::I32Add)
        .set(order);
    for slot in canonical {
        let store = |a: &mut Asm| {
            a.get(order).int(slot as i32).i(I::I32Store(WORD));
            a.add(order, size_of::<u32>() as i32);
        };
        if fields[slot as usize].optional {
            a.get(present)
                .long(1i64 << slot)
                .i(I::I64And)
                .i(I::I64Eqz)
                .i(I::I32Eqz)
                .if_();
            store(&mut a);
            a.end();
        } else {
            store(&mut a);
        }
    }
    sized(&mut a, len as i32, ptr);
    a.finish()
}

/// An array: the shared vector, then each element into its slot.
fn array(plan: &Plan, globals: Globals, element: u32) -> wasm_encoder::Function {
    let mut a = Asm::new(1);
    let count = a.local(ValType::I32);
    let cell = a.local(ValType::I32);
    let slots = a.local(ValType::I32);
    let end = a.local(ValType::I32);
    let kind = plan.kind(element);
    a.get(DST).int(kind as i32);
    next(&mut a, globals);
    a.i(I::I32WrapI64)
        .tee(count)
        .int(i32::from(kind.is_reference()))
        .i(I::Call(plan.shared(Shared::Vector)))
        .set(cell);
    elements(&mut a, (cell, slots, end), count, |a, slot| {
        child(a, plan, element, |a| {
            a.get(slot);
        });
    });
    sized(&mut a, VECTOR_BYTES, cell);
    a.finish()
}
/// Visit the `count` slots of the vector `cell` (`slots`/`end` scratch
/// locals), the slot address in `slots` for `each`.
fn elements(
    a: &mut Asm,
    (cell, slots, end): (u32, u32, u32),
    count: u32,
    each: impl FnOnce(&mut Asm, u32),
) {
    a.get(cell)
        .i(I::I32Load(at(WORD, layout::VECTOR_STORAGE_OFFSET)))
        .int(ARRAY_META)
        .i(I::I32Add)
        .tee(slots)
        .get(count)
        .int(SLOT_BYTES)
        .i(I::I32Mul)
        .i(I::I32Add)
        .set(end);
    a.block("elements").loop_("element");
    a.get(slots).get(end).i(I::I32GeU).br_if("elements");
    each(a, slots);
    a.add(slots, SLOT_BYTES).br("element");
    a.end().end();
}

/// A dictionary: its storage record (value18: zero holes, zero table, all
/// slots present, canonical order), then its key and value vectors
/// published into it, then each entry's key string and value.
fn dictionary(
    plan: &Plan,
    globals: Globals,
    descriptor: u32,
    value: u32,
) -> wasm_encoder::Function {
    let mut a = Asm::new(1);
    let ptr = a.local(ValType::I32);
    let count = a.local(ValType::I32);
    let keys = a.local(ValType::I32);
    let values = a.local(ValType::I32);
    let slots = a.local(ValType::I32);
    let end = a.local(ValType::I32);
    let key = a.local(ValType::I32);
    let version = layout::HASHED_DICTIONARY_VALUE_VERSION;
    let storage = layout::dictionary_storage(version);
    let fields = storage.len();
    let len = layout::record_bytes(version, fields);
    let edges = storage
        .iter()
        .enumerate()
        .filter(|(_, (_, kind))| kind.is_reference())
        .fold(0u32, |edges, (slot, _)| edges | 1 << slot);
    record_block(&mut a, plan, ptr, (descriptor, fields, len, edges));
    a.get(ptr)
        .int(layout::wide_field_mask(fields) as u32 as i32)
        .i(I::I32Store(at(
            WORD,
            layout::presence_offset(fields) as u64,
        )));
    for slot in 0..fields {
        a.get(ptr).int(slot as i32).i(I::I32Store(at(
            WORD,
            (layout::order_offset(fields) + slot * size_of::<u32>()) as u64,
        )));
    }
    let slot_of = |name: &str| {
        storage
            .iter()
            .position(|(field, _)| *field == name)
            .unwrap_or_else(|| unreachable!("value18 dictionary storage names its {name} slot"))
    };
    next(&mut a, globals);
    a.i(I::I32WrapI64).set(count);
    field_slot(&mut a, ptr, slot_of(layout::DICTIONARY_KEYS_FIELD));
    a.int(ValueKind::String as i32)
        .get(count)
        .int(i32::from(ValueKind::String.is_reference()))
        .i(I::Call(plan.shared(Shared::Vector)))
        .set(keys);
    let kind = plan.kind(value);
    field_slot(&mut a, ptr, slot_of(layout::DICTIONARY_VALUES_FIELD));
    a.int(kind as i32)
        .get(count)
        .int(i32::from(kind.is_reference()))
        .i(I::Call(plan.shared(Shared::Vector)))
        .set(values);
    // The key slots, walked beside the value slots.
    a.get(keys)
        .i(I::I32Load(at(WORD, layout::VECTOR_STORAGE_OFFSET)))
        .int(ARRAY_META)
        .i(I::I32Add)
        .set(key);
    elements(&mut a, (values, slots, end), count, |a, slot| {
        // The key's entry, then its link entry (checking only).
        a.get(key);
        next(a, globals);
        a.i(I::Call(plan.shared(Shared::Decode)));
        a.i(I::I64Store(SLOT));
        next(a, globals);
        a.i(I::Drop);
        a.add(key, SLOT_BYTES);
        child(a, plan, value, |a| {
            a.get(slot);
        });
    });
    sized(&mut a, len as i32, ptr);
    a.finish()
}

/// A union: the member on the tape selects the cell kind; the cell is
/// published, then the member's value built into it.
fn union(plan: &Plan, globals: Globals, members: &[u32]) -> wasm_encoder::Function {
    let mut a = Asm::new(1);
    let member = a.local(ValType::I32);
    let ptr = a.local(ValType::I32);
    next(&mut a, globals);
    a.i(I::I32WrapI64).set(member);
    a.block("built");
    let arms: Vec<String> = (0..members.len())
        .map(|member| format!("member{member}"))
        .collect();
    for name in arms.iter().rev() {
        a.block(name);
    }
    let default = arms.last().cloned().unwrap_or_else(|| "built".to_owned());
    a.get(member).br_table(&arms, &default);
    for node in members {
        a.end();
        let kind = plan.kind(*node);
        a.get(DST)
            .int(kind as i32)
            .int(i32::from(kind.is_reference()))
            .i(I::Call(plan.shared(Shared::Cell)))
            .set(ptr);
        child(&mut a, plan, *node, |a| {
            a.get(ptr).int(layout::UNION_META_BYTES as i32).i(I::I32Add);
        });
        a.br("built");
    }
    a.end();
    sized(&mut a, UNION_BYTES, ptr);
    a.finish()
}
