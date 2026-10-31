//! The checkers of the record, array, dictionary and union nodes:
//! `(pos, depth) -> pos`, 0 for the fallback.
use super::asm::{SLOT, TEXT, UNIT, WORD, at};
use super::scan::{
    CLOSE_ARRAY, CLOSE_OBJECT, COLON, COMMA, LIMITS, LOWER_F, LOWER_N, LOWER_T, OPEN_ARRAY,
    OPEN_OBJECT, QUOTE, UNIT_BYTES, digit, fill, offset, push, token,
};
use super::*;
use varta_common::json::JsonField;
use wasm_encoder::Instruction as I;

const POS: u32 = 0;
const DEPTH_PARAM: u32 = 1;
/// Bytes of the wide, half and narrow key comparisons.
const WIDE: usize = size_of::<u64>();
const HALF: usize = size_of::<u32>();
const NARROW: usize = size_of::<u16>();

pub(super) fn node(
    plan: &Plan,
    context: Context,
    node: &JsonType,
) -> Result<wasm_encoder::Function> {
    Ok(match node {
        JsonType::Record { fields, .. } => record(plan, context.globals, fields),
        JsonType::Array { element } => array(plan, context.globals, *element),
        JsonType::Dictionary { value, .. } => dictionary(plan, context.globals, *value),
        JsonType::PrimitiveUnion { members } | JsonType::ReferenceUnion { members } => {
            union(plan, context.globals, members)
        }
        _ => {
            return Err(CompilerError::Internal(
                "no decoder checker for this node".into(),
            ));
        }
    })
}

/// Return 0 unless `depth < DEPTH` and the first unit at `pos` (past
/// whitespace) is `open`; then move `pos` past it. `u` is a scratch local.
fn open(a: &mut Asm, plan: &Plan, globals: Globals, u: u32, open: i32) {
    a.get(DEPTH_PARAM)
        .int(DEPTH)
        .i(I::I32GeS)
        .if_()
        .int(0)
        .i(I::Return)
        .end();
    token(a, plan, globals, POS, u);
    a.get(u)
        .int(open)
        .i(I::I32Ne)
        .if_()
        .int(0)
        .i(I::Return)
        .end();
    a.add(POS, UNIT_BYTES);
}
/// Reserve the container's count entry (its offset in `reserved`); an
/// immediate `close` (past whitespace) jumps to `"close"`.
fn reserve(a: &mut Asm, plan: &Plan, globals: Globals, (u, reserved): (u32, u32), close: i32) {
    offset(a, globals);
    a.set(reserved);
    a.long(0);
    push(a, plan);
    token(a, plan, globals, POS, u);
    a.get(u).int(close).i(I::I32Eq).if_();
    a.add(POS, UNIT_BYTES).br("close");
    a.end();
}
/// After a member or element at `pos`: past whitespace, a `,` continues at
/// `next` (past it), `close` ends at `"close"` (past it), anything else
/// fails. `u` is a scratch local.
fn separator(a: &mut Asm, plan: &Plan, globals: Globals, u: u32, close: i32, next: &str) {
    token(a, plan, globals, POS, u);
    a.get(u).int(COMMA).i(I::I32Eq).if_();
    a.add(POS, UNIT_BYTES).br(next);
    a.end();
    a.get(u).int(close).i(I::I32Eq).if_();
    a.add(POS, UNIT_BYTES).br("close");
    a.end();
    a.br("fail");
}
/// The checker of node `child` at `pos`, one level deeper; fails on 0.
fn child(a: &mut Asm, plan: &Plan, child: u32) {
    a.get(POS)
        .get(DEPTH_PARAM)
        .int(1)
        .i(I::I32Add)
        .i(I::Call(plan.check(child)))
        .tee(POS)
        .i(I::I32Eqz)
        .br_if("fail");
}
/// Close the container: its count from `count` into the reserved entry,
/// return `pos`; the `"fail"` path returns 0.
fn finish(a: &mut Asm, globals: Globals, reserved: u32, count: impl FnOnce(&mut Asm)) {
    fill(a, globals, reserved, count);
    a.get(POS).i(I::Return);
    a.end();
    a.int(0);
}

/// A record: members matched to fields by key length and units, duplicates
/// and unknown keys left to the fallback, required fields checked at `}`.
/// Tape: the present-field count, then each field's slot and value.
fn record(plan: &Plan, globals: Globals, fields: &[JsonField]) -> wasm_encoder::Function {
    let mut a = Asm::new(2);
    let u = a.local(ValType::I32);
    let reserved = a.local(ValType::I32);
    let present = a.local(ValType::I64);
    let scanned = a.local(ValType::I64);
    let start = a.local(ValType::I32);
    let units = a.local(ValType::I32);
    let slot = a.local(ValType::I32);
    let bit = a.local(ValType::I64);
    let required = fields
        .iter()
        .enumerate()
        .filter(|(_, field)| !field.optional)
        .fold(0u64, |mask, (slot, _)| mask | 1 << slot);
    let mut lengths: BTreeMap<usize, Vec<(usize, Vec<u16>)>> = BTreeMap::new();
    for (index, field) in fields.iter().enumerate() {
        let name: Vec<u16> = field.name.encode_utf16().collect();
        lengths.entry(name.len()).or_default().push((index, name));
    }
    open(&mut a, plan, globals, u, OPEN_OBJECT);
    a.block("fail").block("close");
    reserve(&mut a, plan, globals, (u, reserved), CLOSE_OBJECT);
    a.loop_("member");
    {
        key(&mut a, plan, globals, u, (scanned, start, units));
        // The field with this key, by length, then units.
        a.int(-1).set(slot);
        a.block("matched");
        for (length, names) in &lengths {
            a.get(units).int(*length as i32).i(I::I32Eq).if_();
            for (index, name) in names {
                equal_units(&mut a, start, name);
                a.if_().int(*index as i32).set(slot).br("matched").end();
            }
            a.end();
        }
        a.end();
        a.get(slot).int(0).i(I::I32LtS).br_if("fail");
        // A duplicate field is left to the fallback.
        a.long(1)
            .get(slot)
            .i(I::I64ExtendI32U)
            .i(I::I64Shl)
            .tee(bit)
            .get(present)
            .i(I::I64And)
            .i(I::I64Eqz)
            .i(I::I32Eqz)
            .br_if("fail");
        a.get(present).get(bit).i(I::I64Or).set(present);
        a.get(slot).i(I::I64ExtendI32U);
        push(&mut a, plan);
        colon(&mut a, plan, globals, u, scanned);
        // The field's checker, by slot.
        a.block("checked");
        let arms: Vec<String> = (0..fields.len())
            .map(|slot| format!("field{slot}"))
            .collect();
        for name in arms.iter().rev() {
            a.block(name);
        }
        let default = arms.last().cloned().unwrap_or_else(|| "checked".to_owned());
        a.get(slot).br_table(&arms, &default);
        for field in fields {
            a.end();
            child(&mut a, plan, field.ty);
            a.br("checked");
        }
        a.end();
        separator(&mut a, plan, globals, u, CLOSE_OBJECT, "member");
    }
    a.end();
    a.end();
    // `}`: every required field present.
    a.get(present)
        .long(required as i64)
        .i(I::I64And)
        .long(required as i64)
        .i(I::I64Ne)
        .br_if("fail");
    finish(&mut a, globals, reserved, |a| {
        a.get(present).i(I::I64Popcnt);
    });
    a.finish()
}

/// A member key at `pos` (past whitespace): an unescaped string (escaped
/// keys are left to the fallback); its scan result, first unit address and
/// unit count. `u` is a scratch local.
fn key(
    a: &mut Asm,
    plan: &Plan,
    globals: Globals,
    u: u32,
    (scanned, start, units): (u32, u32, u32),
) {
    token(a, plan, globals, POS, u);
    a.get(u).int(QUOTE).i(I::I32Ne).br_if("fail");
    a.get(POS)
        .i(I::Call(plan.shared(Shared::Text)))
        .tee(scanned)
        .i(I::I32WrapI64)
        .i(I::I32Eqz)
        .br_if("fail");
    a.get(scanned).long(0).i(I::I64LtS).br_if("fail");
    a.get(POS).int(UNIT_BYTES).i(I::I32Add).set(start);
    a.get(scanned)
        .long(strings::LENGTH_SHIFT as i64)
        .i(I::I64ShrU)
        .i(I::I32WrapI64)
        .set(units);
}
/// After the key `scanned` (its low word the position after it): past
/// whitespace a `:`; `pos` after it (the value's checker skips whitespace).
fn colon(a: &mut Asm, plan: &Plan, globals: Globals, u: u32, scanned: u32) {
    a.get(scanned).i(I::I32WrapI64).set(POS);
    token(a, plan, globals, POS, u);
    a.get(u).int(COLON).i(I::I32Ne).br_if("fail");
    a.add(POS, UNIT_BYTES);
}
/// Whether the key units at `start` equal `name` (its length already
/// matched): eight bytes at a time, then four and two (pushes 1 or 0).
fn equal_units(a: &mut Asm, start: u32, name: &[u16]) {
    let bytes: Vec<u8> = name.iter().flat_map(|unit| unit.to_le_bytes()).collect();
    let mut offset = 0;
    let mut tests = 0;
    while offset < bytes.len() {
        let rest = bytes.len() - offset;
        let width = if rest >= WIDE {
            WIDE
        } else if rest >= HALF {
            HALF
        } else {
            NARROW
        };
        let mut word = [0u8; WIDE];
        word[..width].copy_from_slice(&bytes[offset..offset + width]);
        let value = u64::from_le_bytes(word);
        a.get(start);
        match width {
            WIDE => {
                a.i(I::I64Load(at(TEXT, offset as u64)))
                    .long(value as i64)
                    .i(I::I64Ne);
            }
            HALF => {
                a.i(I::I32Load(at(TEXT, offset as u64)))
                    .int(value as u32 as i32)
                    .i(I::I32Ne);
            }
            _ => {
                a.i(I::I32Load16U(at(UNIT, offset as u64)))
                    .int(value as u32 as i32)
                    .i(I::I32Ne);
            }
        }
        if tests > 0 {
            a.i(I::I32Or);
        }
        tests += 1;
        offset += width;
    }
    if tests == 0 {
        a.int(1);
    } else {
        a.i(I::I32Eqz);
    }
}

/// An array of `element`: the element count (at most the v38 limit), then
/// the elements.
fn array(plan: &Plan, globals: Globals, element: u32) -> wasm_encoder::Function {
    let mut a = Asm::new(2);
    let u = a.local(ValType::I32);
    let reserved = a.local(ValType::I32);
    let count = a.local(ValType::I32);
    open(&mut a, plan, globals, u, OPEN_ARRAY);
    a.block("fail").block("close");
    reserve(&mut a, plan, globals, (u, reserved), CLOSE_ARRAY);
    a.loop_("element");
    child(&mut a, plan, element);
    a.add(count, 1);
    separator(&mut a, plan, globals, u, CLOSE_ARRAY, "element");
    a.end();
    a.end();
    a.get(count)
        .int(LIMITS.array_elements as i32)
        .i(I::I32GtU)
        .br_if("fail");
    finish(&mut a, globals, reserved, |a| {
        a.get(count).i(I::I64ExtendI32U);
    });
    a.finish()
}

/// A dictionary of `value`: the entry count, then per entry the key's
/// span, a link entry (the tape offset of the dictionary's previous key, -1
/// for none) and the value. Keys must be distinct; escaped keys, keys
/// starting with a digit (possible array indices, which change the own-key
/// order) and more than [`DICTIONARY_ENTRIES`] entries are left to the
/// fallback.
fn dictionary(plan: &Plan, globals: Globals, value: u32) -> wasm_encoder::Function {
    let mut a = Asm::new(2);
    let u = a.local(ValType::I32);
    let reserved = a.local(ValType::I32);
    let count = a.local(ValType::I32);
    let scanned = a.local(ValType::I64);
    let start = a.local(ValType::I32);
    let units = a.local(ValType::I32);
    let last = a.local(ValType::I32);
    let other = a.local(ValType::I32);
    let entry = a.local(ValType::I64);
    let key_offset = a.local(ValType::I32);
    open(&mut a, plan, globals, u, OPEN_OBJECT);
    a.int(-1).set(last);
    a.block("fail").block("close");
    reserve(&mut a, plan, globals, (u, reserved), CLOSE_OBJECT);
    a.loop_("member");
    {
        key(&mut a, plan, globals, u, (scanned, start, units));
        a.add(count, 1)
            .get(count)
            .int(DICTIONARY_ENTRIES)
            .i(I::I32GtS)
            .br_if("fail");
        // A key starting with a digit may be an array index.
        a.get(units).if_();
        a.get(start).i(I::I32Load16U(UNIT));
        digit(&mut a);
        a.br_if("fail");
        a.end();
        // Distinct from every earlier key of this dictionary.
        a.get(last).set(other);
        a.block("distinct").loop_("earlier");
        a.get(other).int(0).i(I::I32LtS).br_if("distinct");
        a.i(I::GlobalGet(globals.base))
            .get(other)
            .i(I::I32Add)
            .i(I::I64Load(SLOT))
            .tee(entry)
            .long(strings::LENGTH_SHIFT as i64)
            .i(I::I64ShrU)
            .i(I::I32WrapI64)
            .get(units)
            .i(I::I32Eq)
            .if_();
        a.get(entry)
            .i(I::I32WrapI64)
            .get(start)
            .get(units)
            .int(UNIT_BYTES)
            .i(I::I32Mul)
            .i(I::Call(plan.shared(Shared::Same)))
            .br_if("fail");
        a.end();
        a.i(I::GlobalGet(globals.base))
            .get(other)
            .i(I::I32Add)
            .i(I::I32Load(at(WORD, ENTRY as u64)))
            .set(other)
            .br("earlier");
        a.end().end();
        offset(&mut a, globals);
        a.set(key_offset);
        a.get(scanned)
            .long(-1i64 << strings::LENGTH_SHIFT)
            .i(I::I64And)
            .get(start)
            .i(I::I64ExtendI32U)
            .i(I::I64Or);
        push(&mut a, plan);
        a.get(last).i(I::I64ExtendI32S);
        push(&mut a, plan);
        a.get(key_offset).set(last);
        colon(&mut a, plan, globals, u, scanned);
        child(&mut a, plan, value);
        separator(&mut a, plan, globals, u, CLOSE_OBJECT, "member");
    }
    a.end();
    a.end();
    finish(&mut a, globals, reserved, |a| {
        a.get(count).i(I::I64ExtendI32U);
    });
    a.finish()
}

/// A union: the member the value's first unit selects, as the shared
/// decoder selects it (the first member of that value kind; an object also
/// selects a dictionary member), on the tape, then that member's value at
/// the same depth.
fn union(plan: &Plan, globals: Globals, members: &[u32]) -> wasm_encoder::Function {
    let mut a = Asm::new(2);
    let u = a.local(ValType::I32);
    let kinds: Vec<ValueKind> = members.iter().map(|member| plan.kind(*member)).collect();
    let select = |wanted: &[ValueKind]| {
        kinds
            .iter()
            .position(|kind| wanted.contains(kind))
            .map(|index| (index, members[index]))
    };
    let classes: [(&[i32], &[ValueKind]); 5] = [
        (&[LOWER_N], &[ValueKind::Null]),
        (&[LOWER_T, LOWER_F], &[ValueKind::Boolean]),
        (&[QUOTE], &[ValueKind::String]),
        (&[OPEN_OBJECT], &[ValueKind::Record, ValueKind::Dictionary]),
        (&[OPEN_ARRAY], &[ValueKind::Array]),
    ];
    token(&mut a, plan, globals, POS, u);
    let take = |a: &mut Asm, member: Option<(usize, u32)>| match member {
        Some((index, node)) => {
            a.long(index as i64);
            push(a, plan);
            a.get(POS)
                .get(DEPTH_PARAM)
                .i(I::Call(plan.check(node)))
                .i(I::Return);
        }
        None => {
            a.int(0).i(I::Return);
        }
    };
    for (units, wanted) in classes {
        for (index, value) in units.iter().enumerate() {
            a.get(u).int(*value).i(I::I32Eq);
            if index > 0 {
                a.i(I::I32Or);
            }
        }
        a.if_();
        take(&mut a, select(wanted));
        a.end();
    }
    // Any other first unit reads as a number.
    take(&mut a, select(&[ValueKind::Float64]));
    a.i(I::Unreachable);
    a.finish()
}
