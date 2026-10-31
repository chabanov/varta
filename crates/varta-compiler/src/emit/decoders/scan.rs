//! The decoders' shared functions: the scanner (whitespace, strings,
//! numbers, literals), the tape, and the block, cell and vector builders of
//! the value layout `varta_common::json::build` writes.
use super::asm::{SLOT, TEXT, UNIT, WORD, at};
use super::*;
use varta_common::value as layout;
use wasm_encoder::Instruction as I;

pub(super) const QUOTE: i32 = b'"' as i32;
pub(super) const BACKSLASH: i32 = b'\\' as i32;
pub(super) const COMMA: i32 = b',' as i32;
pub(super) const COLON: i32 = b':' as i32;
pub(super) const OPEN_OBJECT: i32 = b'{' as i32;
pub(super) const CLOSE_OBJECT: i32 = b'}' as i32;
pub(super) const OPEN_ARRAY: i32 = b'[' as i32;
pub(super) const CLOSE_ARRAY: i32 = b']' as i32;
const MINUS: i32 = b'-' as i32;
const PLUS: i32 = b'+' as i32;
const DOT: i32 = b'.' as i32;
const ZERO: i32 = b'0' as i32;
const DIGITS: i32 = 10;
const LOWER_E: i32 = b'e' as i32;
const UPPER_E: i32 = b'E' as i32;
const LOWER_U: i32 = b'u' as i32;
const LOWER_A: i32 = b'a' as i32;
/// ASCII letters differ from their capitals in this bit.
const CASE_BIT: i32 = 0x20;
const HEX_LETTERS: i32 = 6;
const HEX_DIGIT_BITS: i32 = 4;
const HEX_DIGITS: i32 = 4;
pub(super) const LOWER_N: i32 = b'n' as i32;
pub(super) const LOWER_T: i32 = b't' as i32;
pub(super) const LOWER_F: i32 = b'f' as i32;
/// The first unit no JSON string holds unescaped.
const CONTROL_END: i32 = 0x20;
/// JSON whitespace (`' '`, `\t`, `\n`, `\r`) as a bit set over the units up
/// to [`SPACE`].
const SPACE: i32 = b' ' as i32;
const WHITESPACE: i64 = 1 << b' ' | 1 << b'\t' | 1 << b'\n' | 1 << b'\r';
/// The simple escapes after a backslash and the units they stand for.
const ESCAPES: [(u8, i32); 8] = [
    (b'"', b'"' as i32),
    (b'\\', b'\\' as i32),
    (b'/', b'/' as i32),
    (b'b', 0x08),
    (b'f', 0x0c),
    (b'n', 0x0a),
    (b'r', 0x0d),
    (b't', 0x09),
];
/// The unit count of a tape string entry's high word, without [`ESCAPED`].
const UNITS_MASK: i32 = (ESCAPED - 1) as i32;
/// Bytes of one UTF-16 unit.
pub(super) const UNIT_BYTES: i32 = size_of::<u16>() as i32;
/// Units of a `\uXXXX` escape and of a simple escape.
const UNICODE_ESCAPE_UNITS: i32 = 6;
const SIMPLE_ESCAPE_UNITS: i32 = 2;
/// Decimal digits of a mantissa binary64 holds exactly (`10^15 < 2^53`),
/// and the decimal exponents whose power of ten it holds exactly: within
/// both, one correctly rounded multiplication or division is the exact
/// conversion (as the shared decoder's exact path).
const EXACT_DIGITS: i32 = 15;
const EXACT_POWERS: i32 = POWERS as i32 - 1;
/// Exponent digits beyond this cap cannot change any decision.
const EXPONENT_CAP: i32 = 1 << 20;
/// The decimal exponent of a literal's leading digit from which its value
/// may round to infinity: such literals are left to the fallback.
const OVERFLOW_EXPONENT: i32 = f64::MAX_10_EXP;
/// `"true"`, `"fals"` and `"null"` as UTF-16LE words.
const TRUE: i64 = units(b"true");
const FALS: i64 = units(b"fals");
const NULL: i64 = units(b"null");
const LITERAL_UNITS: i32 = 4;
/// UTF-16LE bytes of four ASCII letters as one little-endian word.
const fn units(word: &[u8; 4]) -> i64 {
    (word[0] as i64) | (word[1] as i64) << 16 | (word[2] as i64) << 32 | (word[3] as i64) << 48
}
/// Heap block constants of the value layout.
const HEADER: i32 = varta_common::abi::STRING_HEADER_BYTES as i32;
const ALLOCATED: i32 = layout::HEAP_ALLOCATED_FLAG as i32;
pub(super) const UNION_BYTES: i32 = layout::UNION_BYTES as i32;
pub(super) const VECTOR_BYTES: i32 = layout::VECTOR_BYTES as i32;
const VECTOR_STORAGE: u64 = layout::VECTOR_STORAGE_OFFSET;
pub(super) const ARRAY_META: i32 = layout::ARRAY_META_BYTES as i32;
pub(super) const SLOT_BYTES: i32 = layout::ARRAY_SLOT_BYTES as i32;
const SLOT_SHIFT: i32 = SLOT_BYTES.trailing_zeros() as i32;
const UNION_META: u64 = layout::UNION_META_BYTES as u64;
/// Presence bits of one bitmap word.
const BITMAP_WORD_BITS: i32 = u64::BITS as i32;
const BITMAP_WORD_SHIFT: i32 = BITMAP_WORD_BITS.trailing_zeros() as i32;
/// The fixed minimum of an array's sparse bitmap
/// (`DataLimits::sparse_bitmap_bytes`).
const SPARSE_BITMAP: i32 = layout::SPARSE_BITMAP_BYTES as i32;
const BITS_PER_BYTE_SHIFT: i32 = u8::BITS.trailing_zeros() as i32;

/// The v38 data limits of schema 18.
pub(super) const LIMITS: varta_common::json::scan::JsonLimits =
    varta_common::json::scan::JsonLimits::V38;

/// `pos`'s unit, or -1 at the input's end.
pub(super) fn unit(a: &mut Asm, globals: Globals, pos: u32) {
    a.get(pos)
        .i(I::GlobalGet(globals.end))
        .i(I::I32LtU)
        .if_result(ValType::I32)
        .get(pos)
        .i(I::I32Load16U(UNIT))
        .else_()
        .int(-1)
        .end();
}
/// Skip whitespace from `pos`: `u` holds the first other unit (or -1 at
/// the end) and `pos` its position. Compact input pays one comparison.
pub(super) fn token(a: &mut Asm, plan: &Plan, globals: Globals, pos: u32, u: u32) {
    unit(a, globals, pos);
    a.tee(u).int(SPACE).i(I::I32LeU).if_();
    a.get(pos)
        .i(I::Call(plan.shared(Shared::Whitespace)))
        .set(pos);
    unit(a, globals, pos);
    a.set(u);
    a.end();
}
/// Whether the unit on the stack is a decimal digit (consumes it).
pub(super) fn digit(a: &mut Asm) {
    a.int(ZERO).i(I::I32Sub).int(DIGITS).i(I::I32LtU);
}
/// Append the i64 on the stack to the tape.
pub(super) fn push(a: &mut Asm, plan: &Plan) {
    a.i(I::Call(plan.shared(Shared::Push)));
}
/// The next tape entry (building).
pub(super) fn next(a: &mut Asm, globals: Globals) {
    a.i(I::GlobalGet(globals.tape))
        .i(I::I64Load(SLOT))
        .i(I::GlobalGet(globals.tape))
        .int(ENTRY)
        .i(I::I32Add)
        .i(I::GlobalSet(globals.tape));
}
/// The current tape offset (checking), to fill a reserved entry later.
pub(super) fn offset(a: &mut Asm, globals: Globals) {
    a.i(I::GlobalGet(globals.tape))
        .i(I::GlobalGet(globals.base))
        .i(I::I32Sub);
}
/// Store the i64 on the stack into the tape entry at the offset in `local`.
pub(super) fn fill(a: &mut Asm, globals: Globals, local: u32, value: impl FnOnce(&mut Asm)) {
    a.i(I::GlobalGet(globals.base)).get(local).i(I::I32Add);
    value(a);
    a.i(I::I64Store(SLOT));
}

pub(super) fn shared(
    plan: &Plan,
    context: Context,
    shared: Shared,
) -> Result<wasm_encoder::Function> {
    let globals = context.globals;
    Ok(match shared {
        Shared::Whitespace => whitespace(globals),
        Shared::Text => text(globals),
        Shared::Push => tape_push(plan, globals),
        Shared::Grow => grow(plan, globals),
        Shared::Same => same(),
        Shared::Block => block(context),
        Shared::Publish => publish(context),
        Shared::Cell => cell(plan),
        Shared::Vector => vector(plan),
        Shared::Decode => decode(plan),
        Shared::Slow => slow(plan, context)?,
        Shared::CheckString => check_string(plan, globals),
        Shared::CheckNumber => check_number(plan, context),
        Shared::CheckBoolean => check_boolean(plan, globals),
        Shared::CheckNull => check_null(plan, globals),
        Shared::CheckFallback => {
            let mut a = Asm::new(2);
            a.int(0);
            a.finish()
        }
        Shared::BuildString => {
            let mut a = Asm::new(1);
            next(&mut a, globals);
            a.i(I::Call(plan.shared(Shared::Decode)));
            a.finish()
        }
        Shared::BuildNumber => build_number(plan, globals),
        Shared::BuildBoolean => {
            let mut a = Asm::new(1);
            next(&mut a, globals);
            a.finish()
        }
        Shared::BuildNull => {
            let mut a = Asm::new(1);
            a.long(0);
            a.finish()
        }
    })
}

fn whitespace(globals: Globals) -> wasm_encoder::Function {
    const POS: u32 = 0;
    let mut a = Asm::new(1);
    let u = a.local(ValType::I32);
    a.loop_("next").block("done");
    a.get(POS)
        .i(I::GlobalGet(globals.end))
        .i(I::I32GeU)
        .br_if("done");
    a.get(POS)
        .i(I::I32Load16U(UNIT))
        .tee(u)
        .int(SPACE)
        .i(I::I32GtU)
        .br_if("done");
    a.long(WHITESPACE)
        .get(u)
        .i(I::I64ExtendI32U)
        .i(I::I64ShrU)
        .i(I::I32WrapI64)
        .int(1)
        .i(I::I32And)
        .i(I::I32Eqz)
        .br_if("done");
    a.add(POS, UNIT_BYTES).br("next");
    a.end().end();
    a.get(POS);
    a.finish()
}

/// `(pos) -> i64`: the string whose quote is at `pos` (see [`Shared::Text`]).
fn text(globals: Globals) -> wasm_encoder::Function {
    const POS: u32 = 0;
    let mut a = Asm::new(1);
    let cursor = a.local(ValType::I32);
    let u = a.local(ValType::I32);
    let extra = a.local(ValType::I32);
    let escaped = a.local(ValType::I64);
    let k = a.local(ValType::I32);
    a.get(POS).int(UNIT_BYTES).i(I::I32Add).set(cursor);
    a.block("fail").block("done").loop_("unit");
    a.get(cursor)
        .i(I::GlobalGet(globals.end))
        .i(I::I32GeU)
        .br_if("fail");
    a.get(cursor)
        .i(I::I32Load16U(UNIT))
        .tee(u)
        .int(QUOTE)
        .i(I::I32Eq)
        .br_if("done");
    a.get(u).int(BACKSLASH).i(I::I32Eq).if_();
    {
        a.long(ESCAPED).set(escaped);
        a.get(cursor)
            .int(UNIT_BYTES)
            .i(I::I32Add)
            .tee(cursor)
            .i(I::GlobalGet(globals.end))
            .i(I::I32GeU)
            .br_if("fail");
        a.get(cursor)
            .i(I::I32Load16U(UNIT))
            .tee(u)
            .int(LOWER_U)
            .i(I::I32Eq)
            .if_();
        {
            // Four hexadecimal digits, all inside the input.
            a.get(cursor)
                .int(HEX_DIGITS * UNIT_BYTES)
                .i(I::I32Add)
                .i(I::GlobalGet(globals.end))
                .i(I::I32GeU)
                .br_if("fail");
            a.int(0).set(k);
            a.loop_("hex");
            a.get(cursor)
                .get(k)
                .i(I::I32Add)
                .i(I::I32Load16U(at(UNIT, UNIT_BYTES as u64)))
                .set(u);
            hex_digit(&mut a, u);
            a.i(I::I32Eqz).br_if("fail");
            a.add(k, UNIT_BYTES)
                .get(k)
                .int(HEX_DIGITS * UNIT_BYTES)
                .i(I::I32LtU)
                .br_if("hex");
            a.end();
            a.add(cursor, (HEX_DIGITS + 1) * UNIT_BYTES);
            a.add(extra, UNICODE_ESCAPE_UNITS - 1);
            a.br("unit");
        }
        a.end();
        // One of the simple escapes.
        for (index, (escape, _)) in ESCAPES.iter().enumerate() {
            a.get(u).int(i32::from(*escape)).i(I::I32Eq);
            if index > 0 {
                a.i(I::I32Or);
            }
        }
        a.i(I::I32Eqz).br_if("fail");
        a.add(cursor, UNIT_BYTES);
        a.add(extra, SIMPLE_ESCAPE_UNITS - 1);
        a.br("unit");
    }
    a.end();
    a.get(u).int(CONTROL_END).i(I::I32LtU).br_if("fail");
    a.add(cursor, UNIT_BYTES).br("unit");
    a.end().end();
    // The position after the closing quote; the units between the quotes.
    a.get(cursor)
        .int(UNIT_BYTES)
        .i(I::I32Add)
        .i(I::I64ExtendI32U)
        .get(cursor)
        .get(POS)
        .i(I::I32Sub)
        .int(UNIT_BYTES)
        .i(I::I32Sub)
        .int(1)
        .i(I::I32ShrU)
        .get(extra)
        .i(I::I32Sub)
        .i(I::I64ExtendI32U)
        .get(escaped)
        .i(I::I64Or)
        .long(strings::LENGTH_SHIFT as i64)
        .i(I::I64Shl)
        .i(I::I64Or)
        .i(I::Return);
    a.end();
    a.long(0);
    a.finish()
}
/// Whether the unit in `u` is a hexadecimal digit (pushes 1 or 0).
fn hex_digit(a: &mut Asm, u: u32) {
    a.get(u);
    digit(a);
    a.get(u)
        .int(CASE_BIT)
        .i(I::I32Or)
        .int(LOWER_A)
        .i(I::I32Sub)
        .int(HEX_LETTERS)
        .i(I::I32LtU)
        .i(I::I32Or);
}
/// The value of the hexadecimal digit in `u` (pushes it).
fn hex_value(a: &mut Asm, u: u32) {
    a.get(u)
        .int(ZERO)
        .i(I::I32Sub)
        .get(u)
        .int(CASE_BIT)
        .i(I::I32Or)
        .int(LOWER_A - DIGITS)
        .i(I::I32Sub)
        .get(u);
    digit(a);
    a.i(I::Select);
}

fn tape_push(plan: &Plan, globals: Globals) -> wasm_encoder::Function {
    const VALUE: u32 = 0;
    let mut a = Asm::new(1);
    a.i(I::GlobalGet(globals.tape))
        .i(I::GlobalGet(globals.limit))
        .i(I::I32GeU)
        .if_()
        .i(I::Call(plan.shared(Shared::Grow)))
        .end();
    a.i(I::GlobalGet(globals.tape))
        .get(VALUE)
        .i(I::I64Store(SLOT));
    a.i(I::GlobalGet(globals.tape))
        .int(ENTRY)
        .i(I::I32Add)
        .i(I::GlobalSet(globals.tape));
    a.finish()
}

/// Double the tape: a new rooted block holding the entries so far (the old
/// block stays rooted until the copy is done).
fn grow(plan: &Plan, globals: Globals) -> wasm_encoder::Function {
    let mut a = Asm::new(0);
    let used = a.local(ValType::I32);
    let capacity = a.local(ValType::I32);
    let block = a.local(ValType::I32);
    a.i(I::GlobalGet(globals.tape))
        .i(I::GlobalGet(globals.base))
        .i(I::I32Sub)
        .set(used);
    a.i(I::GlobalGet(globals.limit))
        .i(I::GlobalGet(globals.base))
        .i(I::I32Sub)
        .int(1)
        .i(I::I32Shl)
        .tee(capacity)
        .int(0)
        .int(0)
        .i(I::Call(plan.shared(Shared::Block)))
        .tee(block)
        .i(I::GlobalGet(globals.base))
        .get(used)
        .i(I::MemoryCopy {
            src_mem: strings::MEMORY,
            dst_mem: strings::MEMORY,
        });
    a.i(I::GlobalGet(globals.slot));
    handle(&mut a, capacity, block);
    a.i(I::I64Store(SLOT));
    a.get(block).i(I::GlobalSet(globals.base));
    a.get(block)
        .get(used)
        .i(I::I32Add)
        .i(I::GlobalSet(globals.tape));
    a.get(block)
        .get(capacity)
        .i(I::I32Add)
        .i(I::GlobalSet(globals.limit));
    a.finish()
}
/// Push the packed handle of block `ptr` with `len` bytes (both locals).
pub(super) fn handle(a: &mut Asm, len: u32, ptr: u32) {
    a.get(len)
        .i(I::I64ExtendI32U)
        .long(strings::LENGTH_SHIFT as i64)
        .i(I::I64Shl)
        .get(ptr)
        .i(I::I64ExtendI32U)
        .i(I::I64Or);
}
/// Push the packed handle of block `ptr` (a local) of constant size `len`.
pub(super) fn sized(a: &mut Asm, len: i32, ptr: u32) {
    a.get(ptr)
        .i(I::I64ExtendI32U)
        .long(i64::from(len) << strings::LENGTH_SHIFT)
        .i(I::I64Or);
}

/// `(a, b, bytes) -> i32`: equal byte ranges (a whole number of units).
fn same() -> wasm_encoder::Function {
    const LEFT: u32 = 0;
    const RIGHT: u32 = 1;
    const BYTES: u32 = 2;
    const WIDE: i32 = size_of::<u64>() as i32;
    let mut a = Asm::new(3);
    a.block("different");
    a.loop_("wide").block("narrow");
    a.get(BYTES).int(WIDE).i(I::I32LtU).br_if("narrow");
    a.get(LEFT)
        .i(I::I64Load(TEXT))
        .get(RIGHT)
        .i(I::I64Load(TEXT))
        .i(I::I64Ne)
        .br_if("different");
    a.add(LEFT, WIDE)
        .add(RIGHT, WIDE)
        .add(BYTES, -WIDE)
        .br("wide");
    a.end().end();
    a.loop_("unit").block("equal");
    a.get(BYTES).i(I::I32Eqz).br_if("equal");
    a.get(LEFT)
        .i(I::I32Load16U(UNIT))
        .get(RIGHT)
        .i(I::I32Load16U(UNIT))
        .i(I::I32Ne)
        .br_if("different");
    a.add(LEFT, UNIT_BYTES)
        .add(RIGHT, UNIT_BYTES)
        .add(BYTES, -UNIT_BYTES)
        .br("unit");
    a.end().end();
    a.int(1).i(I::Return);
    a.end();
    a.int(0);
    a.finish()
}

/// `(len, flags, link) -> ptr`: a heap block from the guest allocator,
/// stamped allocated with `flags` and trace `link`, as the shared builder
/// stamps it.
fn block(context: Context) -> wasm_encoder::Function {
    const LEN: u32 = 0;
    const FLAGS: u32 = 1;
    const LINK: u32 = 2;
    let mut a = Asm::new(3);
    let ptr = a.local(ValType::I32);
    if let Some(collect) = context.collect {
        a.i(I::Call(collect));
    }
    a.get(LEN).i(I::Call(context.allocate)).set(ptr);
    a.get(ptr)
        .int(HEADER)
        .i(I::I32Sub)
        .get(FLAGS)
        .int(ALLOCATED)
        .i(I::I32Or)
        .i(I::I32Store(strings::FLAG));
    a.get(ptr)
        .int(HEADER)
        .i(I::I32Sub)
        .get(LINK)
        .i(I::I32Store(strings::LINK));
    a.get(ptr);
    a.finish()
}

/// `(dst, handle)`: into a published slot, or retained at the root.
fn publish(context: Context) -> wasm_encoder::Function {
    const DST: u32 = 0;
    const HANDLE: u32 = 1;
    let mut a = Asm::new(2);
    a.get(DST).i(I::I32Eqz).if_();
    a.get(HANDLE).i(I::Call(context.retain)).i(I::Drop);
    a.else_();
    a.get(DST).get(HANDLE).i(I::I64Store(SLOT));
    a.end();
    a.finish()
}

/// `(dst, kind, link) -> ptr`: a union cell of `kind`, payload 0, published.
fn cell(plan: &Plan) -> wasm_encoder::Function {
    const DST: u32 = 0;
    const KIND: u32 = 1;
    const LINK: u32 = 2;
    let mut a = Asm::new(3);
    let ptr = a.local(ValType::I32);
    a.int(UNION_BYTES)
        .int(layout::HEAP_UNION_FLAG)
        .get(LINK)
        .i(I::Call(plan.shared(Shared::Block)))
        .tee(ptr)
        .get(KIND)
        .i(I::I32Store(WORD));
    a.get(ptr)
        .int(1)
        .i(I::I32Store(at(WORD, size_of::<u32>() as u64)));
    a.get(ptr).long(0).i(I::I64Store(at(SLOT, UNION_META)));
    a.get(DST);
    sized(&mut a, UNION_BYTES, ptr);
    a.i(I::Call(plan.shared(Shared::Publish)));
    a.get(ptr);
    a.finish()
}

/// `(dst, kind, count, link) -> cell`: the shared builder's vector: the
/// identity cell published at `dst`, then its sparse storage of `count`
/// present zero slots (capacity the next power of two) published into it.
fn vector(plan: &Plan) -> wasm_encoder::Function {
    const DST: u32 = 0;
    const KIND: u32 = 1;
    const COUNT: u32 = 2;
    const LINK: u32 = 3;
    let mut a = Asm::new(4);
    let cell = a.local(ValType::I32);
    let capacity = a.local(ValType::I32);
    let bitmap = a.local(ValType::I32);
    let len = a.local(ValType::I32);
    let storage = a.local(ValType::I32);
    let full = a.local(ValType::I32);
    a.int(VECTOR_BYTES)
        .int(layout::HEAP_VECTOR_FLAG)
        .int(1)
        .i(I::Call(plan.shared(Shared::Block)))
        .tee(cell)
        .get(KIND)
        .i(I::I32Store(WORD));
    a.get(cell)
        .get(COUNT)
        .i(I::I32Store(at(WORD, size_of::<u32>() as u64)));
    a.get(cell).long(0).i(I::I64Store(at(SLOT, VECTOR_STORAGE)));
    a.get(DST);
    sized(&mut a, VECTOR_BYTES, cell);
    a.i(I::Call(plan.shared(Shared::Publish)));
    // `array_capacity`: 0, or the next power of two.
    a.get(COUNT).if_result(ValType::I32);
    a.int(1)
        .int(u32::BITS as i32)
        .get(COUNT)
        .int(1)
        .i(I::I32Sub)
        .i(I::I32Clz)
        .i(I::I32Sub)
        .i(I::I32Shl);
    a.else_().int(0).end().set(capacity);
    a.get(capacity)
        .int(SLOT_SHIFT)
        .i(I::I32Shl)
        .int(ARRAY_META)
        .i(I::I32Add)
        .set(bitmap);
    // `DataLimits::sparse_bitmap_bytes`: a bit per slot, at least the fixed
    // minimum.
    a.get(capacity)
        .int(BITS_PER_BYTE_SHIFT)
        .i(I::I32ShrU)
        .tee(len)
        .int(SPARSE_BITMAP)
        .get(len)
        .int(SPARSE_BITMAP)
        .i(I::I32GtU)
        .i(I::Select)
        .get(bitmap)
        .i(I::I32Add)
        .set(len);
    a.get(len)
        .int(layout::HEAP_ARRAY_FLAG | layout::HEAP_SPARSE_FLAG)
        .get(LINK)
        .i(I::Call(plan.shared(Shared::Block)))
        .tee(storage)
        .get(KIND)
        .i(I::I32Store(WORD));
    a.get(storage)
        .get(COUNT)
        .i(I::I32Store(at(WORD, size_of::<u32>() as u64)));
    a.get(storage)
        .int(ARRAY_META)
        .i(I::I32Add)
        .int(0)
        .get(len)
        .int(ARRAY_META)
        .i(I::I32Sub)
        .i(I::MemoryFill(strings::MEMORY));
    // Presence bits of slots `0..count`, a word at a time.
    a.get(storage)
        .get(bitmap)
        .i(I::I32Add)
        .int(u8::MAX as i32)
        .get(COUNT)
        .int(BITMAP_WORD_SHIFT)
        .i(I::I32ShrU)
        .tee(full)
        .int(SLOT_SHIFT)
        .i(I::I32Shl)
        .i(I::MemoryFill(strings::MEMORY));
    a.get(COUNT).int(BITMAP_WORD_BITS - 1).i(I::I32And).if_();
    a.get(storage)
        .get(bitmap)
        .i(I::I32Add)
        .get(full)
        .int(SLOT_SHIFT)
        .i(I::I32Shl)
        .i(I::I32Add)
        .long(1)
        .get(COUNT)
        .i(I::I64ExtendI32U)
        .i(I::I64Shl)
        .long(1)
        .i(I::I64Sub)
        .i(I::I64Store(SLOT));
    a.end();
    a.get(cell);
    handle(&mut a, len, storage);
    a.i(I::I64Store(at(SLOT, VECTOR_STORAGE)));
    a.get(cell);
    a.finish()
}

/// `(entry) -> handle`: an owned string of a tape string entry, raw runs
/// copied and escapes decoded (the empty string is the zero handle).
fn decode(plan: &Plan) -> wasm_encoder::Function {
    const ENTRY_VALUE: u32 = 0;
    let mut a = Asm::new(1);
    let len = a.local(ValType::I32);
    let source = a.local(ValType::I32);
    let ptr = a.local(ValType::I32);
    let target = a.local(ValType::I32);
    let end = a.local(ValType::I32);
    let u = a.local(ValType::I32);
    let code = a.local(ValType::I32);
    let k = a.local(ValType::I32);
    a.get(ENTRY_VALUE)
        .long(strings::LENGTH_SHIFT as i64)
        .i(I::I64ShrU)
        .i(I::I32WrapI64)
        .int(UNITS_MASK)
        .i(I::I32And)
        .int(1)
        .i(I::I32Shl)
        .tee(len)
        .i(I::I32Eqz)
        .if_()
        .long(0)
        .i(I::Return)
        .end();
    a.get(ENTRY_VALUE).i(I::I32WrapI64).set(source);
    a.get(len)
        .int(0)
        .int(0)
        .i(I::Call(plan.shared(Shared::Block)))
        .set(ptr);
    a.get(ENTRY_VALUE).long(0).i(I::I64LtS).if_();
    {
        a.get(ptr).set(target);
        a.get(ptr).get(len).i(I::I32Add).set(end);
        a.loop_("unit");
        a.get(source)
            .i(I::I32Load16U(UNIT))
            .tee(u)
            .int(BACKSLASH)
            .i(I::I32Eq)
            .if_();
        {
            a.get(source)
                .i(I::I32Load16U(at(UNIT, UNIT_BYTES as u64)))
                .tee(u)
                .int(LOWER_U)
                .i(I::I32Eq)
                .if_();
            {
                a.int(0).set(code).int(0).set(k);
                a.loop_("hex");
                a.get(source)
                    .get(k)
                    .i(I::I32Add)
                    .i(I::I32Load16U(at(UNIT, (2 * UNIT_BYTES) as u64)))
                    .set(u);
                a.get(code).int(HEX_DIGIT_BITS).i(I::I32Shl);
                hex_value(&mut a, u);
                a.i(I::I32Or).set(code);
                a.add(k, UNIT_BYTES)
                    .get(k)
                    .int(HEX_DIGITS * UNIT_BYTES)
                    .i(I::I32LtU)
                    .br_if("hex");
                a.end();
                a.add(source, UNICODE_ESCAPE_UNITS * UNIT_BYTES);
            }
            a.else_();
            {
                // A simple escape: its unit is itself unless mapped.
                a.get(u).set(code);
                for (escape, value) in ESCAPES {
                    if i32::from(escape) != value {
                        a.int(value)
                            .get(code)
                            .get(u)
                            .int(i32::from(escape))
                            .i(I::I32Eq)
                            .i(I::Select)
                            .set(code);
                    }
                }
                a.add(source, SIMPLE_ESCAPE_UNITS * UNIT_BYTES);
            }
            a.end();
            a.get(target).get(code).i(I::I32Store16(UNIT));
        }
        a.else_();
        a.get(target).get(u).i(I::I32Store16(UNIT));
        a.add(source, UNIT_BYTES);
        a.end();
        a.add(target, UNIT_BYTES)
            .get(target)
            .get(end)
            .i(I::I32LtU)
            .br_if("unit");
        a.end();
    }
    a.else_();
    a.get(ptr).get(source).get(len).i(I::MemoryCopy {
        src_mem: strings::MEMORY,
        dst_mem: strings::MEMORY,
    });
    a.end();
    handle(&mut a, len, ptr);
    a.finish()
}

/// `(start) -> bits`: the literal at `start` converted by the linked
/// `primitive_to_number` (correctly rounded, as the shared decoder's `core`
/// path) through a string cell over its units.
fn slow(plan: &Plan, context: Context) -> Result<wasm_encoder::Function> {
    const START: u32 = 0;
    let mut a = Asm::new(1);
    let cursor = a.local(ValType::I32);
    let u = a.local(ValType::I32);
    let cell = a.local(ValType::I32);
    let Some(number) = context.number else {
        // No number node: never called.
        a.i(I::Unreachable);
        return Ok(a.finish());
    };
    a.get(START).set(cursor);
    a.block("end").loop_("unit");
    unit(&mut a, context.globals, cursor);
    a.tee(u);
    digit(&mut a);
    for symbol in [MINUS, PLUS, DOT, LOWER_E, UPPER_E] {
        a.get(u).int(symbol).i(I::I32Eq).i(I::I32Or);
    }
    a.i(I::I32Eqz).br_if("end");
    a.add(cursor, UNIT_BYTES).br("unit");
    a.end().end();
    a.int(UNION_BYTES)
        .int(layout::HEAP_UNION_FLAG)
        .int(i32::from(ValueKind::String.is_reference()))
        .i(I::Call(plan.shared(Shared::Block)))
        .tee(cell)
        .int(ValueKind::String as i32)
        .i(I::I32Store(WORD));
    a.get(cell)
        .int(1)
        .i(I::I32Store(at(WORD, size_of::<u32>() as u64)));
    a.get(cell)
        .get(cursor)
        .get(START)
        .i(I::I32Sub)
        .i(I::I64ExtendI32U)
        .long(strings::LENGTH_SHIFT as i64)
        .i(I::I64Shl)
        .get(START)
        .i(I::I64ExtendI32U)
        .i(I::I64Or)
        .i(I::I64Store(at(SLOT, UNION_META)));
    sized(&mut a, UNION_BYTES, cell);
    a.i(I::Call(number)).i(I::I64ReinterpretF64);
    Ok(a.finish())
}

/// `(pos, depth) -> pos`: a string value, its span on the tape.
fn check_string(plan: &Plan, globals: Globals) -> wasm_encoder::Function {
    const POS: u32 = 0;
    let mut a = Asm::new(2);
    let u = a.local(ValType::I32);
    let scanned = a.local(ValType::I64);
    token(&mut a, plan, globals, POS, u);
    a.get(u)
        .int(QUOTE)
        .i(I::I32Ne)
        .if_()
        .int(0)
        .i(I::Return)
        .end();
    a.get(POS)
        .i(I::Call(plan.shared(Shared::Text)))
        .tee(scanned)
        .i(I::I32WrapI64)
        .i(I::I32Eqz)
        .if_()
        .int(0)
        .i(I::Return)
        .end();
    a.get(scanned)
        .long(-1i64 << strings::LENGTH_SHIFT)
        .i(I::I64And)
        .get(POS)
        .int(UNIT_BYTES)
        .i(I::I32Add)
        .i(I::I64ExtendI32U)
        .i(I::I64Or);
    push(&mut a, plan);
    a.get(scanned).i(I::I32WrapI64);
    a.finish()
}

/// `(pos, depth) -> pos`: `true` or `false`, its value on the tape.
fn check_boolean(plan: &Plan, globals: Globals) -> wasm_encoder::Function {
    const POS: u32 = 0;
    let mut a = Asm::new(2);
    let u = a.local(ValType::I32);
    let room = |a: &mut Asm, units: i32| {
        a.get(POS)
            .int(units * UNIT_BYTES)
            .i(I::I32Add)
            .i(I::GlobalGet(globals.end))
            .i(I::I32LeU);
    };
    token(&mut a, plan, globals, POS, u);
    a.get(u).int(LOWER_T).i(I::I32Eq).if_();
    room(&mut a, LITERAL_UNITS);
    a.if_();
    a.get(POS).i(I::I64Load(TEXT)).long(TRUE).i(I::I64Eq).if_();
    a.long(1);
    push(&mut a, plan);
    a.get(POS)
        .int(LITERAL_UNITS * UNIT_BYTES)
        .i(I::I32Add)
        .i(I::Return);
    a.end().end().end();
    a.get(u).int(LOWER_F).i(I::I32Eq).if_();
    room(&mut a, LITERAL_UNITS + 1);
    a.if_();
    a.get(POS)
        .i(I::I64Load(TEXT))
        .long(FALS)
        .i(I::I64Eq)
        .get(POS)
        .i(I::I32Load16U(at(UNIT, (LITERAL_UNITS * UNIT_BYTES) as u64)))
        .int(LOWER_E)
        .i(I::I32Eq)
        .i(I::I32And)
        .if_();
    a.long(0);
    push(&mut a, plan);
    a.get(POS)
        .int((LITERAL_UNITS + 1) * UNIT_BYTES)
        .i(I::I32Add)
        .i(I::Return);
    a.end().end().end();
    a.int(0);
    a.finish()
}

/// `(pos, depth) -> pos`: `null` (nothing on the tape).
fn check_null(plan: &Plan, globals: Globals) -> wasm_encoder::Function {
    const POS: u32 = 0;
    let mut a = Asm::new(2);
    let u = a.local(ValType::I32);
    token(&mut a, plan, globals, POS, u);
    a.get(POS)
        .int(LITERAL_UNITS * UNIT_BYTES)
        .i(I::I32Add)
        .i(I::GlobalGet(globals.end))
        .i(I::I32LeU)
        .if_();
    a.get(POS).i(I::I64Load(TEXT)).long(NULL).i(I::I64Eq).if_();
    a.get(POS)
        .int(LITERAL_UNITS * UNIT_BYTES)
        .i(I::I32Add)
        .i(I::Return);
    a.end().end();
    a.int(0);
    a.finish()
}

/// `(pos, depth) -> pos`: a number literal in the strict JSON grammar, its
/// binary64 bits on the tape: the exact conversion within [`EXACT_DIGITS`]
/// significant digits and decimal exponents within [`EXACT_POWERS`], else a
/// [`SLOW_NUMBER`] entry the build converts. A literal whose leading digit's
/// decimal exponent reaches [`OVERFLOW_EXPONENT`] is left to the fallback.
///
/// Every digit enters the mantissa (it wraps past 19 digits, when the
/// literal is no longer exact anyway); the significant digits are those from
/// the first nonzero one (only `0.…` has zeros before it), the scale the
/// exponent less the fraction digits.
fn check_number(plan: &Plan, context: Context) -> wasm_encoder::Function {
    const POS: u32 = 0;
    let globals = context.globals;
    let mut a = Asm::new(2);
    let cursor = a.local(ValType::I32);
    let u = a.local(ValType::I32);
    let negative = a.local(ValType::I32);
    let mantissa = a.local(ValType::I64);
    let significant = a.local(ValType::I32);
    let integer_digits = a.local(ValType::I32);
    let fraction = a.local(ValType::I32);
    let fraction_digits = a.local(ValType::I32);
    let exponent = a.local(ValType::I32);
    let exponent_negative = a.local(ValType::I32);
    let scale = a.local(ValType::I32);
    let value = a.local(ValType::F64);
    let power = a.local(ValType::F64);
    let d = a.local(ValType::I32);
    let zeros = a.local(ValType::I32);
    let advance = |a: &mut Asm| {
        a.add(cursor, UNIT_BYTES);
        unit(a, globals, cursor);
        a.set(u);
    };
    // A run of digits from `u` (a digit) into the mantissa, `d` each digit's
    // value, with one bound check per digit; `u` the unit after the run.
    let digits = |a: &mut Asm, name: &str| {
        let end = format!("{name}_end");
        a.get(u).int(ZERO).i(I::I32Sub).set(d);
        a.block(&end).loop_(name);
        a.get(mantissa)
            .long(10)
            .i(I::I64Mul)
            .get(d)
            .i(I::I64ExtendI32U)
            .i(I::I64Add)
            .set(mantissa);
        a.add(cursor, UNIT_BYTES);
        a.get(cursor)
            .i(I::GlobalGet(globals.end))
            .i(I::I32GeU)
            .if_()
            .int(-1)
            .set(u)
            .br(&end)
            .end();
        a.get(cursor)
            .i(I::I32Load16U(UNIT))
            .int(ZERO)
            .i(I::I32Sub)
            .tee(d)
            .int(DIGITS)
            .i(I::I32LtU)
            .br_if(name);
        a.end();
        a.get(d).int(ZERO).i(I::I32Add).set(u);
        a.end();
    };
    token(&mut a, plan, globals, POS, u);
    a.get(POS).set(cursor);
    a.block("fail");
    a.get(u).int(MINUS).i(I::I32Eq).if_();
    a.int(1).set(negative);
    advance(&mut a);
    a.end();
    // The integer part: `0`, or a nonzero digit and more digits.
    a.get(cursor).set(integer_digits);
    a.get(u).int(ZERO).i(I::I32Eq).if_();
    {
        advance(&mut a);
        a.get(u);
        digit(&mut a);
        a.br_if("fail");
        a.get(cursor).set(integer_digits);
    }
    a.else_();
    {
        a.get(u);
        digit(&mut a);
        a.i(I::I32Eqz).br_if("fail");
        digits(&mut a, "integer");
    }
    a.end();
    // `integer_digits` held the first digit's address.
    a.get(cursor)
        .get(integer_digits)
        .i(I::I32Sub)
        .int(1)
        .i(I::I32ShrU)
        .set(integer_digits);
    a.get(u).int(DOT).i(I::I32Eq).if_();
    {
        advance(&mut a);
        a.get(cursor).set(fraction);
        a.get(u);
        digit(&mut a);
        a.i(I::I32Eqz).br_if("fail");
        // `0.…`: the zeros before the first nonzero digit are not
        // significant.
        a.get(integer_digits).i(I::I32Eqz).if_();
        a.block("zeros_end").loop_("zeros");
        a.get(cursor)
            .i(I::GlobalGet(globals.end))
            .i(I::I32GeU)
            .br_if("zeros_end");
        a.get(cursor)
            .i(I::I32Load16U(UNIT))
            .int(ZERO)
            .i(I::I32Ne)
            .br_if("zeros_end");
        a.add(cursor, UNIT_BYTES).br("zeros");
        a.end().end();
        a.get(cursor)
            .get(fraction)
            .i(I::I32Sub)
            .int(1)
            .i(I::I32ShrU)
            .set(zeros);
        a.end();
        digits(&mut a, "fraction");
        a.get(cursor)
            .get(fraction)
            .i(I::I32Sub)
            .int(1)
            .i(I::I32ShrU)
            .set(fraction_digits);
    }
    a.end();
    a.get(integer_digits)
        .get(fraction_digits)
        .i(I::I32Add)
        .get(zeros)
        .i(I::I32Sub)
        .set(significant);
    a.get(u)
        .int(LOWER_E)
        .i(I::I32Eq)
        .get(u)
        .int(UPPER_E)
        .i(I::I32Eq)
        .i(I::I32Or)
        .if_();
    {
        advance(&mut a);
        a.get(u).int(MINUS).i(I::I32Eq).set(exponent_negative);
        a.get(u)
            .int(MINUS)
            .i(I::I32Eq)
            .get(u)
            .int(PLUS)
            .i(I::I32Eq)
            .i(I::I32Or)
            .if_();
        advance(&mut a);
        a.end();
        a.get(u);
        digit(&mut a);
        a.i(I::I32Eqz).br_if("fail");
        a.block("exponent_end").loop_("exponent");
        a.get(u)
            .int(ZERO)
            .i(I::I32Sub)
            .tee(d)
            .int(DIGITS)
            .i(I::I32GeU)
            .br_if("exponent_end");
        a.get(exponent)
            .int(10)
            .i(I::I32Mul)
            .get(d)
            .i(I::I32Add)
            .tee(exponent)
            .int(EXPONENT_CAP)
            .get(exponent)
            .int(EXPONENT_CAP)
            .i(I::I32LtS)
            .i(I::Select)
            .set(exponent);
        advance(&mut a);
        a.br("exponent");
        a.end().end();
        a.int(0)
            .get(exponent)
            .i(I::I32Sub)
            .get(exponent)
            .get(exponent_negative)
            .i(I::Select)
            .set(exponent);
    }
    a.end();
    a.get(exponent).get(fraction_digits).i(I::I32Sub).set(scale);
    // The exact path.
    a.get(significant)
        .int(EXACT_DIGITS)
        .i(I::I32LeS)
        .get(scale)
        .int(EXACT_POWERS)
        .i(I::I32Add)
        .int(2 * EXACT_POWERS + 1)
        .i(I::I32LtU)
        .i(I::I32And)
        .if_();
    {
        a.get(mantissa).i(I::F64ConvertI64U).set(value);
        // The power `10^|scale|`.
        a.get(scale)
            .int(0)
            .get(scale)
            .i(I::I32Sub)
            .get(scale)
            .int(0)
            .i(I::I32GeS)
            .i(I::Select)
            .int(SLOT_SHIFT)
            .i(I::I32Shl)
            .i(I::F64Load(at(SLOT, u64::from(context.powers))))
            .set(power);
        a.get(value)
            .get(power)
            .i(I::F64Div)
            .get(value)
            .get(power)
            .i(I::F64Mul)
            .get(scale)
            .int(0)
            .i(I::I32LtS)
            .i(I::Select)
            .set(value);
        a.get(value)
            .i(I::F64Neg)
            .get(value)
            .get(negative)
            .i(I::Select)
            .i(I::I64ReinterpretF64);
        push(&mut a, plan);
        a.get(cursor).i(I::Return);
    }
    a.end();
    // Converted by the build. The leading digit's decimal exponent: the
    // integer digits less one, or (`0.…`) less one than minus the zeros
    // before the first nonzero fraction digit; zero needs no check.
    a.get(significant).if_();
    a.get(integer_digits)
        .int(1)
        .i(I::I32Sub)
        .get(significant)
        .get(fraction_digits)
        .i(I::I32Sub)
        .int(1)
        .i(I::I32Sub)
        .get(integer_digits)
        .i(I::Select)
        .get(exponent)
        .i(I::I32Add)
        .int(OVERFLOW_EXPONENT)
        .i(I::I32GeS)
        .br_if("fail");
    a.end();
    a.long(SLOW_NUMBER << strings::LENGTH_SHIFT)
        .get(POS)
        .i(I::I64ExtendI32U)
        .i(I::I64Or);
    push(&mut a, plan);
    a.get(cursor).i(I::Return);
    a.end();
    a.int(0);
    a.finish()
}

/// `(dst) -> bits`: a number entry, converting a [`SLOW_NUMBER`] literal.
fn build_number(plan: &Plan, globals: Globals) -> wasm_encoder::Function {
    let mut a = Asm::new(1);
    let bits = a.local(ValType::I64);
    next(&mut a, globals);
    a.tee(bits)
        .long(strings::LENGTH_SHIFT as i64)
        .i(I::I64ShrU)
        .long(SLOW_NUMBER)
        .i(I::I64Eq)
        .if_result(ValType::I64);
    a.get(bits)
        .i(I::I32WrapI64)
        .i(I::Call(plan.shared(Shared::Slow)));
    a.else_().get(bits).end();
    a.finish()
}
