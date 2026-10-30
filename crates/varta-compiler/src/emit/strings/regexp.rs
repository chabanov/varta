//! The v37 guest RegExp matcher: a Pike VM over a compiled program
//! (`crate::regexp::program`) and an immutable UTF-16 input. It makes no host
//! call. One run allocates one scratch block (captures, working slots,
//! visited stamps, two thread lists and the follow stack) whose size the
//! program header bounds, and returns that block as a string whose code units
//! hold the 32-bit capture slots (-1 when unset), or the empty string when
//! nothing matches. Every input position is stepped at most once and every
//! instruction is added at most once per position, so a run costs at most
//! `O(program × input)` instructions, all metered as guest fuel.
use super::*;
use crate::regexp::program::{self, assertion, op};

const UNIT: MemArg = MemArg {
    offset: 0,
    align: 1,
    memory_index: MEMORY,
};
const BOUND: MemArg = UNIT;
const UNIT_BYTES: i32 = size_of::<u16>() as i32;
const WORD_BYTES: i32 = program::WORD_BYTES as i32;
const STACK_ENTRY_BYTES: i32 = WORD_BYTES * program::STACK_WORDS as i32;
const INSTRUCTION_BYTES: i32 = UNIT_BYTES * program::INSTRUCTION_UNITS as i32;
const HEADER_BYTES: i32 = UNIT_BYTES * program::HEADER_UNITS as i32;
const RANGE_BYTES: i32 = UNIT_BYTES * program::RANGE_UNITS as i32;
const UNSET_BYTE: i32 = 0xFF;
const HIGH_SURROGATE: i32 = 0xD800;
const LOW_SURROGATE: i32 = 0xDC00;
const SURROGATE_MASK: i32 = 0xFC00;
const SUPPLEMENTARY: i32 = 0x1_0000;
const SURROGATE_BITS: i32 = 10;
const ASCII: i32 = 0x80;
const LF: i32 = 0x0A;
const CR: i32 = 0x0D;
const PARAGRAPH_SEPARATOR: i32 = 0x2029;
const DIGIT_ZERO: i32 = b'0' as i32;
const DIGITS: i32 = 10;
const LOWER_A: i32 = b'a' as i32;
const LETTERS: i32 = 26;
const CASE_BIT: i32 = 0x20;
const UNDERSCORE: i32 = b'_' as i32;
const LONG_S: i32 = 0x017F;
const KELVIN: i32 = 0x212A;

// Parameters.
const PROGRAM: u32 = 0;
const INPUT: u32 = 1;
const START: u32 = 2;
const STICKY: u32 = 3;
// i32 locals.
const PROG: u32 = 4;
const IN: u32 = 5;
const LEN: u32 = 6;
const FLAGS: u32 = 7;
const CAPTURES: u32 = 8;
const SLOTS: u32 = 9;
const COUNT: u32 = 10;
const THREADS: u32 = 11;
const STACK_CAP: u32 = 12;
const BUF: u32 = 13;
const WC: u32 = 14;
const VISITED: u32 = 15;
const CLIST: u32 = 16;
const NLIST: u32 = 17;
const STACK: u32 = 18;
const ENTRY: u32 = 19;
const POS: u32 = 20;
const BEGIN: u32 = 21;
const CCOUNT: u32 = 22;
const NCOUNT: u32 = 23;
const MATCHED: u32 = 24;
const I_: u32 = 25;
const TPTR: u32 = 26;
const PC: u32 = 27;
const OP: u32 = 28;
const A: u32 = 29;
const B: u32 = 30;
const CP: u32 = 31;
const WIDTH: u32 = 32;
const AT: u32 = 33;
const SP: u32 = 34;
const TAG: u32 = 35;
const X: u32 = 36;
const K: u32 = 37;
const STAMP: u32 = 38;
const TMP: u32 = 39;
const ADDR: u32 = 40;
const LIST: u32 = 41;
const LCOUNT: u32 = 42;
const DEST: u32 = 43;
const N: u32 = 44;
const UNITV: u32 = 45;
const CINDEX: u32 = 46;
/// Nesting of empty-checked iterations: `2^NEST` visited states per
/// instruction.
const NEST: u32 = 47;
/// The depth of the instruction being followed.
const DEPTH: u32 = 48;
const LAST_LOCAL: u32 = DEPTH;
// i64 locals: the ASCII prefilter bitmap halves.
const LOW_MASK: u32 = LAST_LOCAL + 1;
const HIGH_MASK: u32 = LAST_LOCAL + 2;
const MASKS: u32 = 2;
const HALF: i32 = 64;
const MASK_BYTES: u64 = 8;
const WRITE_BYTES: i32 = UNIT_BYTES * program::WRITE_UNITS as i32;

/// Emits code for one case.
type Emit = fn(&mut Asm);

/// Structured code with named labels, so branch depths are computed.
struct Asm {
    code: Vec<I<'static>>,
    labels: Vec<Option<&'static str>>,
}
impl Asm {
    fn new() -> Self {
        Self {
            code: Vec::new(),
            labels: Vec::new(),
        }
    }
    fn ops(&mut self, code: impl IntoIterator<Item = I<'static>>) {
        self.code.extend(code);
    }
    fn block(&mut self, name: &'static str) {
        self.code.push(I::Block(BlockType::Empty));
        self.labels.push(Some(name));
    }
    fn looped(&mut self, name: &'static str) {
        self.code.push(I::Loop(BlockType::Empty));
        self.labels.push(Some(name));
    }
    fn when(&mut self) {
        self.code.push(I::If(BlockType::Empty));
        self.labels.push(None);
    }
    fn otherwise(&mut self) {
        self.code.push(I::Else);
    }
    fn end(&mut self) {
        self.code.push(I::End);
        self.labels.pop();
    }
    fn depth(&self, name: &str) -> u32 {
        let position = self
            .labels
            .iter()
            .rposition(|label| *label == Some(name))
            .unwrap_or_else(|| unreachable!("unknown matcher label {name}"));
        (self.labels.len() - 1 - position) as u32
    }
    fn br(&mut self, name: &'static str) {
        let depth = self.depth(name);
        self.code.push(I::Br(depth));
    }
    fn br_if(&mut self, name: &'static str) {
        let depth = self.depth(name);
        self.code.push(I::BrIf(depth));
    }
    fn get(&mut self, local: u32) {
        self.code.push(I::LocalGet(local));
    }
    fn set(&mut self, local: u32) {
        self.code.push(I::LocalSet(local));
    }
    fn constant(&mut self, value: i32) {
        self.code.push(I::I32Const(value));
    }
    fn add_to(&mut self, local: u32, value: i32) {
        self.ops([
            I::LocalGet(local),
            I::I32Const(value),
            I::I32Add,
            I::LocalSet(local),
        ]);
    }
    /// Push the code unit of the input at the position on the stack.
    fn input_unit(&mut self) {
        self.ops([
            I::I32Const(UNIT_BYTES),
            I::I32Mul,
            I::LocalGet(IN),
            I::I32Add,
            I::I32Load16U(UNIT),
        ]);
    }
    /// Push the program unit at header index `index`.
    fn header(&mut self, index: usize) {
        self.ops([
            I::LocalGet(PROG),
            I::I32Load16U(MemArg {
                offset: (index as u64) * UNIT_BYTES as u64,
                ..UNIT
            }),
        ]);
    }
    /// Whether header flag `bit` is set, as 0 or 1.
    fn flag(&mut self, bit: u16) {
        self.ops([
            I::LocalGet(FLAGS),
            I::I32Const(i32::from(bit)),
            I::I32And,
            I::I32Const(0),
            I::I32Ne,
        ]);
    }
    /// `unit & 0xFC00 == base` for the unit on the stack.
    fn surrogate(&mut self, base: i32) {
        self.ops([
            I::I32Const(SURROGATE_MASK),
            I::I32And,
            I::I32Const(base),
            I::I32Eq,
        ]);
    }
    /// Load instruction `pc` into `OP` and `A` (and `B`).
    fn decode(&mut self, pc: u32, with_b: bool) {
        self.decode_op(pc, false);
        self.operands(with_b);
    }
    /// Load the opcode of instruction `pc` into `OP` (and its nesting depth
    /// into `DEPTH`), keeping its address in `ADDR`.
    fn decode_op(&mut self, pc: u32, depth: bool) {
        self.ops([
            I::LocalGet(pc),
            I::I32Const(INSTRUCTION_BYTES),
            I::I32Mul,
            I::LocalGet(PROG),
            I::I32Add,
            I::I32Const(HEADER_BYTES),
            I::I32Add,
            I::LocalTee(ADDR),
            I::I32Load16U(UNIT),
            I::LocalTee(OP),
        ]);
        if depth {
            self.ops([
                I::I32Const(i32::from(program::DEPTH_SHIFT)),
                I::I32ShrU,
                I::LocalSet(DEPTH),
                I::LocalGet(OP),
            ]);
        }
        self.ops([
            I::I32Const(i32::from(program::OP_MASK)),
            I::I32And,
            I::LocalSet(OP),
        ]);
    }
    /// Load the operands of the instruction at `ADDR` into `A` (and `B`).
    fn operands(&mut self, with_b: bool) {
        self.ops([
            I::LocalGet(ADDR),
            I::I32Load16U(MemArg {
                offset: UNIT_BYTES as u64,
                ..UNIT
            }),
            I::LocalSet(A),
        ]);
        if with_b {
            self.ops([
                I::LocalGet(ADDR),
                I::I32Load16U(MemArg {
                    offset: 2 * UNIT_BYTES as u64,
                    ..UNIT
                }),
                I::LocalSet(B),
            ]);
        }
    }
    /// Address of working slot `local` on the stack.
    fn slot_address(&mut self, local: u32) {
        self.ops([
            I::LocalGet(local),
            I::I32Const(WORD_BYTES),
            I::I32Mul,
            I::LocalGet(WC),
            I::I32Add,
        ]);
    }
    /// Push a follow-stack entry `(tag, value)`; the tag is on the stack
    /// via `tag`, the value via `value`.
    fn push(&mut self, tag: impl FnOnce(&mut Self), value: impl FnOnce(&mut Self)) {
        self.ops([
            I::LocalGet(SP),
            I::I32Const(STACK_ENTRY_BYTES),
            I::I32Mul,
            I::LocalGet(STACK),
            I::I32Add,
            I::LocalTee(DEST),
        ]);
        tag(self);
        self.code.push(I::I32Store(WORD));
        self.get(DEST);
        value(self);
        self.code.push(I::I32Store(MemArg {
            offset: WORD_BYTES as u64,
            ..WORD
        }));
        self.add_to(SP, 1);
    }
    /// Whether the unit in `local` is a word character, on the stack.
    fn word_unit(&mut self, local: u32) {
        self.ops([
            I::LocalGet(local),
            I::I32Const(DIGIT_ZERO),
            I::I32Sub,
            I::I32Const(DIGITS),
            I::I32LtU,
            I::LocalGet(local),
            I::I32Const(CASE_BIT),
            I::I32Or,
            I::I32Const(LOWER_A),
            I::I32Sub,
            I::I32Const(LETTERS),
            I::I32LtU,
            I::I32Or,
            I::LocalGet(local),
            I::I32Const(UNDERSCORE),
            I::I32Eq,
            I::I32Or,
        ]);
        self.flag(program::EXTRA_WORD);
        self.ops([
            I::LocalGet(local),
            I::I32Const(LONG_S),
            I::I32Eq,
            I::LocalGet(local),
            I::I32Const(KELVIN),
            I::I32Eq,
            I::I32Or,
            I::I32And,
            I::I32Or,
        ]);
    }
    /// Whether the unit in `local` is a LineTerminator, on the stack.
    fn line_terminator(&mut self, local: u32) {
        self.ops([
            I::LocalGet(local),
            I::I32Const(LF),
            I::I32Eq,
            I::LocalGet(local),
            I::I32Const(CR),
            I::I32Eq,
            I::I32Or,
            I::LocalGet(local),
            I::I32Const(1),
            I::I32Or,
            I::I32Const(PARAGRAPH_SEPARATOR),
            I::I32Eq,
            I::I32Or,
        ]);
    }
    /// `TMP` = whether the input unit at `AT - 1` (`before`) or `AT` passes
    /// `test`, false outside the input.
    fn unit_test(&mut self, before: bool, test: fn(&mut Self, u32)) {
        self.constant(0);
        self.set(TMP);
        self.get(AT);
        if before {
            self.ops([I::I32Const(0), I::I32GtU]);
        } else {
            self.ops([I::LocalGet(LEN), I::I32LtU]);
        }
        self.when();
        self.get(AT);
        if before {
            self.ops([I::I32Const(1), I::I32Sub]);
        }
        self.input_unit();
        self.set(UNITV);
        test(self, UNITV);
        self.set(TMP);
        self.end();
    }
    /// `TMP` = whether assertion `A` holds at `AT`.
    fn assertion(&mut self) {
        let kinds: [(u16, Emit); 6] = [
            (assertion::START, |asm| {
                asm.ops([I::LocalGet(AT), I::I32Eqz, I::LocalSet(TMP)]);
            }),
            (assertion::END, |asm| {
                asm.ops([
                    I::LocalGet(AT),
                    I::LocalGet(LEN),
                    I::I32Eq,
                    I::LocalSet(TMP),
                ]);
            }),
            (assertion::LINE_START, |asm| {
                asm.unit_test(true, Self::line_terminator);
                asm.ops([
                    I::LocalGet(TMP),
                    I::LocalGet(AT),
                    I::I32Eqz,
                    I::I32Or,
                    I::LocalSet(TMP),
                ]);
            }),
            (assertion::LINE_END, |asm| {
                asm.unit_test(false, Self::line_terminator);
                asm.ops([
                    I::LocalGet(TMP),
                    I::LocalGet(AT),
                    I::LocalGet(LEN),
                    I::I32Eq,
                    I::I32Or,
                    I::LocalSet(TMP),
                ]);
            }),
            (assertion::WORD_BOUNDARY, |asm| asm.boundary(true)),
            (assertion::NOT_WORD_BOUNDARY, |asm| asm.boundary(false)),
        ];
        for (kind, emit) in kinds {
            self.ops([I::LocalGet(A), I::I32Const(i32::from(kind)), I::I32Eq]);
            self.when();
            emit(self);
            self.end();
        }
    }
    fn boundary(&mut self, differ: bool) {
        self.unit_test(true, Self::word_unit);
        self.get(TMP);
        self.set(N);
        self.unit_test(false, Self::word_unit);
        self.ops([
            I::LocalGet(N),
            I::LocalGet(TMP),
            if differ { I::I32Ne } else { I::I32Eq },
            I::LocalSet(TMP),
        ]);
    }

    /// Add the thread at instruction `X` and position `AT` with the working
    /// slots to `LIST` (holding `LCOUNT` threads), following jumps, splits,
    /// slot writes and assertions. `STAMP` marks visited instructions; slot
    /// writes are undone in stack order before a lower-priority branch runs.
    fn add_thread(&mut self) {
        self.block("thread");
        self.precomputed();
        self.follow();
        self.end();
    }
    /// The precomputed closure of `X`, when the program has one: add each
    /// entry whose target is not yet in the list, with its slot writes.
    fn precomputed(&mut self) {
        self.get(CINDEX);
        self.when();
        self.ops([
            I::LocalGet(CINDEX),
            I::LocalGet(X),
            I::I32Add,
            I::I32Const(UNIT_BYTES),
            I::I32Mul,
            I::LocalGet(PROG),
            I::I32Add,
            I::I32Load16U(UNIT),
            I::LocalTee(ADDR),
        ]);
        self.when();
        self.ops([
            I::LocalGet(ADDR),
            I::I32Const(UNIT_BYTES),
            I::I32Mul,
            I::LocalGet(PROG),
            I::I32Add,
            I::LocalTee(ADDR),
            I::I32Load16U(UNIT),
            I::LocalSet(N),
        ]);
        self.add_to(ADDR, UNIT_BYTES);
        self.block("listed");
        self.looped("entry");
        self.ops([I::LocalGet(N), I::I32Eqz]);
        self.br_if("listed");
        self.ops([
            I::LocalGet(ADDR),
            I::I32Load16U(UNIT),
            I::LocalSet(TAG),
            I::LocalGet(ADDR),
            I::I32Load16U(MemArg {
                offset: UNIT_BYTES as u64,
                ..UNIT
            }),
            I::LocalSet(K),
            I::LocalGet(ADDR),
            I::I32Const(2 * UNIT_BYTES),
            I::I32Add,
            I::LocalSet(ADDR),
            I::LocalGet(TAG),
            I::LocalGet(NEST),
            I::I32Shl,
            I::I32Const(WORD_BYTES),
            I::I32Mul,
            I::LocalGet(VISITED),
            I::I32Add,
            I::LocalTee(DEST),
            I::I32Load(WORD),
            I::LocalGet(STAMP),
            I::I32Eq,
        ]);
        self.when();
        // Already added: skip the entry's writes.
        self.ops([
            I::LocalGet(K),
            I::I32Const(WRITE_BYTES),
            I::I32Mul,
            I::LocalGet(ADDR),
            I::I32Add,
            I::LocalSet(ADDR),
        ]);
        self.otherwise();
        self.ops([
            I::LocalGet(DEST),
            I::LocalGet(STAMP),
            I::I32Store(WORD),
            I::LocalGet(LCOUNT),
            I::LocalGet(ENTRY),
            I::I32Mul,
            I::LocalGet(LIST),
            I::I32Add,
            I::LocalTee(DEST),
            I::LocalGet(TAG),
            I::I32Store(WORD),
            I::LocalGet(DEST),
            I::I32Const(WORD_BYTES),
            I::I32Add,
            I::LocalTee(DEST),
            I::LocalGet(WC),
            I::LocalGet(SLOTS),
            I::I32Const(WORD_BYTES),
            I::I32Mul,
            I::MemoryCopy {
                src_mem: MEMORY,
                dst_mem: MEMORY,
            },
        ]);
        self.block("written");
        self.looped("write");
        self.ops([I::LocalGet(K), I::I32Eqz]);
        self.br_if("written");
        self.ops([
            I::LocalGet(ADDR),
            I::I32Load16U(UNIT),
            I::LocalSet(A),
            I::LocalGet(ADDR),
            I::I32Load16U(MemArg {
                offset: UNIT_BYTES as u64,
                ..UNIT
            }),
            I::LocalTee(B),
            I::I32Const(i32::from(program::SET)),
            I::I32Eq,
        ]);
        self.when();
        self.ops([
            I::LocalGet(A),
            I::I32Const(WORD_BYTES),
            I::I32Mul,
            I::LocalGet(DEST),
            I::I32Add,
            I::LocalGet(AT),
            I::I32Store(WORD),
        ]);
        self.otherwise();
        self.ops([
            I::LocalGet(A),
            I::I32Const(WORD_BYTES),
            I::I32Mul,
            I::LocalGet(DEST),
            I::I32Add,
            I::I32Const(UNSET_BYTE),
            I::LocalGet(B),
            I::LocalGet(A),
            I::I32Sub,
            I::I32Const(WORD_BYTES),
            I::I32Mul,
            I::MemoryFill(MEMORY),
        ]);
        self.end();
        self.add_to(ADDR, WRITE_BYTES);
        self.add_to(K, -1);
        self.br("write");
        self.end();
        self.end();
        self.add_to(LCOUNT, 1);
        self.end();
        self.add_to(N, -1);
        self.br("entry");
        self.end();
        self.end();
        self.br("thread");
        self.end();
        self.end();
    }
    /// Follow `X` at run time with the follow stack.
    fn follow(&mut self) {
        self.constant(0);
        self.set(SP);
        self.push(|asm| asm.get(X), |asm| asm.constant(0));
        self.block("added");
        self.looped("pop");
        self.ops([I::LocalGet(SP), I::I32Eqz]);
        self.br_if("added");
        self.add_to(SP, -1);
        self.ops([
            I::LocalGet(SP),
            I::I32Const(STACK_ENTRY_BYTES),
            I::I32Mul,
            I::LocalGet(STACK),
            I::I32Add,
            I::LocalTee(DEST),
            I::I32Load(WORD),
            I::LocalTee(TAG),
            I::I32Const(0),
            I::I32LtS,
        ]);
        self.when();
        // Restore slot `-(tag + 1)`.
        self.ops([I::I32Const(-1), I::LocalGet(TAG), I::I32Sub, I::LocalSet(K)]);
        self.slot_address(K);
        self.ops([
            I::LocalGet(DEST),
            I::I32Load(MemArg {
                offset: WORD_BYTES as u64,
                ..WORD
            }),
            I::I32Store(WORD),
        ]);
        self.br("pop");
        self.end();
        self.get(TAG);
        self.set(X);
        self.block("stop");
        self.looped("follow");
        self.decode_op(X, true);
        self.state_key();
        self.ops([
            I::LocalGet(X),
            I::LocalGet(NEST),
            I::I32Shl,
            I::LocalGet(TMP),
            I::I32Add,
            I::I32Const(WORD_BYTES),
            I::I32Mul,
            I::LocalGet(VISITED),
            I::I32Add,
            I::LocalTee(DEST),
            I::I32Load(WORD),
            I::LocalGet(STAMP),
            I::I32Eq,
        ]);
        self.br_if("stop");
        self.ops([I::LocalGet(DEST), I::LocalGet(STAMP), I::I32Store(WORD)]);
        self.operands(true);
        // JUMP
        self.ops([I::LocalGet(OP), I::I32Const(i32::from(op::JUMP)), I::I32Eq]);
        self.when();
        self.get(A);
        self.set(X);
        self.br("follow");
        self.end();
        // SPLIT: the second target waits on the stack.
        self.ops([I::LocalGet(OP), I::I32Const(i32::from(op::SPLIT)), I::I32Eq]);
        self.when();
        self.push(|asm| asm.get(B), |asm| asm.constant(0));
        self.get(A);
        self.set(X);
        self.br("follow");
        self.end();
        // SAVE: remember the old value, then store the position.
        self.ops([I::LocalGet(OP), I::I32Const(i32::from(op::SAVE)), I::I32Eq]);
        self.when();
        self.restore_entry(A);
        self.slot_address(A);
        self.ops([I::LocalGet(AT), I::I32Store(WORD)]);
        self.add_to(X, 1);
        self.br("follow");
        self.end();
        // RESET: clear slots A..B, remembering each.
        self.ops([I::LocalGet(OP), I::I32Const(i32::from(op::RESET)), I::I32Eq]);
        self.when();
        self.get(A);
        self.set(K);
        self.block("reset");
        self.looped("clear");
        self.ops([I::LocalGet(K), I::LocalGet(B), I::I32GeU]);
        self.br_if("reset");
        self.restore_entry(K);
        self.slot_address(K);
        self.ops([I::I32Const(-1), I::I32Store(WORD)]);
        self.add_to(K, 1);
        self.br("clear");
        self.end();
        self.end();
        self.add_to(X, 1);
        self.br("follow");
        self.end();
        // CHECK: an iteration that consumed nothing fails.
        self.ops([I::LocalGet(OP), I::I32Const(i32::from(op::CHECK)), I::I32Eq]);
        self.when();
        self.slot_address(A);
        self.ops([I::I32Load(WORD), I::LocalGet(AT), I::I32Eq]);
        self.br_if("stop");
        self.add_to(X, 1);
        self.br("follow");
        self.end();
        // ASSERT
        self.ops([
            I::LocalGet(OP),
            I::I32Const(i32::from(op::ASSERT)),
            I::I32Eq,
        ]);
        self.when();
        self.assertion();
        self.ops([I::LocalGet(TMP), I::I32Eqz]);
        self.br_if("stop");
        self.add_to(X, 1);
        self.br("follow");
        self.end();
        // A consuming instruction or MATCH: record the thread.
        self.ops([
            I::LocalGet(LCOUNT),
            I::LocalGet(ENTRY),
            I::I32Mul,
            I::LocalGet(LIST),
            I::I32Add,
            I::LocalTee(DEST),
            I::LocalGet(X),
            I::I32Store(WORD),
            I::LocalGet(DEST),
            I::I32Const(WORD_BYTES),
            I::I32Add,
            I::LocalGet(WC),
            I::LocalGet(SLOTS),
            I::I32Const(WORD_BYTES),
            I::I32Mul,
            I::MemoryCopy {
                src_mem: MEMORY,
                dst_mem: MEMORY,
            },
        ]);
        self.add_to(LCOUNT, 1);
        self.br("stop");
        self.end();
        self.end();
        self.br("pop");
        self.end();
        self.end();
    }
    /// `TMP` = which enclosing empty-checked iterations of the decoded
    /// instruction started at `AT` (bit `k` for the register of depth `k`):
    /// with the instruction, it decides every later empty check, so equal
    /// states have equal futures. Consuming instructions and `MATCH` use
    /// state 0: consuming makes every open iteration non-empty.
    fn state_key(&mut self) {
        self.constant(0);
        self.set(TMP);
        self.ops([
            I::LocalGet(DEPTH),
            I::I32Const(0),
            I::I32Ne,
            I::LocalGet(OP),
            I::I32Const(i32::from(op::JUMP)),
            I::I32GeU,
            I::I32And,
        ]);
        self.when();
        self.constant(0);
        self.set(K);
        self.block("keyed");
        self.looped("bit");
        self.ops([I::LocalGet(K), I::LocalGet(DEPTH), I::I32GeU]);
        self.br_if("keyed");
        self.ops([
            I::LocalGet(CAPTURES),
            I::LocalGet(K),
            I::I32Add,
            I::I32Const(WORD_BYTES),
            I::I32Mul,
            I::LocalGet(WC),
            I::I32Add,
            I::I32Load(WORD),
            I::LocalGet(AT),
            I::I32Eq,
        ]);
        self.when();
        self.ops([
            I::LocalGet(TMP),
            I::I32Const(1),
            I::LocalGet(K),
            I::I32Shl,
            I::I32Or,
            I::LocalSet(TMP),
        ]);
        self.end();
        self.add_to(K, 1);
        self.br("bit");
        self.end();
        self.end();
        self.end();
    }
    /// Push a restore entry for working slot `local`.
    fn restore_entry(&mut self, local: u32) {
        self.push(
            |asm| {
                asm.ops([I::I32Const(-1), I::LocalGet(local), I::I32Sub]);
            },
            |asm| {
                asm.slot_address(local);
                asm.code.push(I::I32Load(WORD));
            },
        );
    }
    /// `TMP` = whether code point `CP` is in the class at unit offset `A`.
    fn class(&mut self) {
        self.ops([
            I::I32Const(0),
            I::LocalSet(TMP),
            I::LocalGet(A),
            I::I32Const(UNIT_BYTES),
            I::I32Mul,
            I::LocalGet(PROG),
            I::I32Add,
            I::LocalTee(ADDR),
            I::I32Load16U(UNIT),
            I::LocalSet(N),
            I::LocalGet(ADDR),
            I::I32Const(UNIT_BYTES),
            I::I32Add,
            I::LocalSet(ADDR),
        ]);
        self.block("classified");
        self.looped("range");
        self.ops([I::LocalGet(N), I::I32Eqz]);
        self.br_if("classified");
        // Ranges are sorted: below a low bound, no later range matches.
        self.ops([
            I::LocalGet(CP),
            I::LocalGet(ADDR),
            I::I32Load(BOUND),
            I::I32LtU,
        ]);
        self.br_if("classified");
        self.ops([
            I::LocalGet(CP),
            I::LocalGet(ADDR),
            I::I32Load(MemArg {
                offset: (WORD_BYTES) as u64,
                ..BOUND
            }),
            I::I32LeU,
        ]);
        self.when();
        self.constant(1);
        self.set(TMP);
        self.br("classified");
        self.end();
        self.add_to(ADDR, RANGE_BYTES);
        self.add_to(N, -1);
        self.br("range");
        self.end();
        self.end();
    }
}

/// `(program: string, input: string, start: f64, sticky: i32) -> string`.
pub(super) fn exec(allocate: u32) -> Function {
    let mut asm = Asm::new();
    // Prologue: decode the header and the input.
    asm.ops([
        I::LocalGet(PROGRAM),
        I::I32WrapI64,
        I::LocalSet(PROG),
        I::LocalGet(INPUT),
        I::I32WrapI64,
        I::LocalSet(IN),
        I::LocalGet(INPUT),
        I::I64Const(LENGTH_SHIFT as i64),
        I::I64ShrU,
        I::I32WrapI64,
        I::I32Const(UNIT_BYTES),
        I::I32DivU,
        I::LocalSet(LEN),
    ]);
    for (index, local) in [
        (program::FLAGS, FLAGS),
        (program::CAPTURES, CAPTURES),
        (program::SLOTS, SLOTS),
        (program::INSTRUCTIONS, COUNT),
        (program::THREADS, THREADS),
        (program::STACK, STACK_CAP),
        (program::CLOSURE_INDEX, CINDEX),
        (program::NESTING, NEST),
    ] {
        asm.header(index);
        asm.set(local);
    }
    for (local, offset) in [(LOW_MASK, 0), (HIGH_MASK, MASK_BYTES)] {
        asm.ops([
            I::LocalGet(PROG),
            I::I64Load(MemArg {
                offset: program::FIRST_ASCII as u64 * UNIT_BYTES as u64 + offset,
                ..UNIT
            }),
            I::LocalSet(local),
        ]);
    }
    // A start beyond the input never matches.
    asm.ops([
        I::LocalGet(START),
        I::LocalGet(LEN),
        I::F64ConvertI32U,
        I::F64Gt,
        I::LocalGet(START),
        I::LocalGet(START),
        I::F64Ne,
        I::I32Or,
    ]);
    asm.when();
    asm.ops([I::I64Const(0), I::Return]);
    asm.end();
    asm.ops([
        I::LocalGet(START),
        I::F64Const(0.0.into()),
        I::F64Max,
        I::I32TruncF64U,
        I::LocalSet(BEGIN),
    ]);
    // Under `u`, a start inside a surrogate pair backs up to the pair.
    asm.flag(program::UNICODE);
    asm.when();
    asm.ops([
        I::LocalGet(BEGIN),
        I::I32Const(0),
        I::I32GtU,
        I::LocalGet(BEGIN),
        I::LocalGet(LEN),
        I::I32LtU,
        I::I32And,
    ]);
    asm.when();
    asm.get(BEGIN);
    asm.input_unit();
    asm.surrogate(LOW_SURROGATE);
    asm.ops([I::LocalGet(BEGIN), I::I32Const(1), I::I32Sub]);
    asm.input_unit();
    asm.surrogate(HIGH_SURROGATE);
    asm.code.push(I::I32And);
    asm.when();
    asm.add_to(BEGIN, -1);
    asm.end();
    asm.end();
    asm.end();
    // Anchored programs match only at position 0.
    asm.flag(program::ANCHORED);
    asm.ops([I::LocalGet(BEGIN), I::I32Const(0), I::I32Ne, I::I32And]);
    asm.when();
    asm.ops([I::I64Const(0), I::Return]);
    asm.end();
    // Scratch layout.
    asm.ops([
        I::LocalGet(SLOTS),
        I::I32Const(1),
        I::I32Add,
        I::I32Const(WORD_BYTES),
        I::I32Mul,
        I::LocalSet(ENTRY),
        I::LocalGet(CAPTURES),
        I::LocalGet(SLOTS),
        I::I32Add,
        I::LocalGet(COUNT),
        I::LocalGet(NEST),
        I::I32Shl,
        I::I32Add,
        I::I32Const(WORD_BYTES),
        I::I32Mul,
        I::LocalGet(THREADS),
        I::LocalGet(ENTRY),
        I::I32Mul,
        I::I32Const(2),
        I::I32Mul,
        I::I32Add,
        I::LocalGet(STACK_CAP),
        I::I32Const(STACK_ENTRY_BYTES),
        I::I32Mul,
        I::I32Add,
        I::Call(allocate),
        I::LocalTee(BUF),
        I::LocalGet(CAPTURES),
        I::I32Const(WORD_BYTES),
        I::I32Mul,
        I::I32Add,
        I::LocalTee(WC),
        I::LocalGet(SLOTS),
        I::I32Const(WORD_BYTES),
        I::I32Mul,
        I::I32Add,
        I::LocalTee(VISITED),
        I::LocalGet(COUNT),
        I::LocalGet(NEST),
        I::I32Shl,
        I::I32Const(WORD_BYTES),
        I::I32Mul,
        I::I32Add,
        I::LocalTee(CLIST),
        I::LocalGet(THREADS),
        I::LocalGet(ENTRY),
        I::I32Mul,
        I::I32Add,
        I::LocalTee(NLIST),
        I::LocalGet(THREADS),
        I::LocalGet(ENTRY),
        I::I32Mul,
        I::I32Add,
        I::LocalSet(STACK),
        I::LocalGet(VISITED),
        I::I32Const(0),
        I::LocalGet(COUNT),
        I::LocalGet(NEST),
        I::I32Shl,
        I::I32Const(WORD_BYTES),
        I::I32Mul,
        I::MemoryFill(MEMORY),
        I::LocalGet(BEGIN),
        I::LocalSet(POS),
        I::I32Const(0),
        I::LocalSet(CCOUNT),
        I::I32Const(0),
        I::LocalSet(MATCHED),
    ]);
    asm.block("done");
    asm.looped("step");
    asm.ops([I::LocalGet(MATCHED), I::I32Eqz]);
    asm.when();
    asm.ops([I::LocalGet(CCOUNT), I::I32Eqz]);
    asm.when();
    // No live thread: a sticky or anchored run ends after its start.
    asm.get(STICKY);
    asm.flag(program::ANCHORED);
    asm.ops([
        I::I32Or,
        I::LocalGet(POS),
        I::LocalGet(BEGIN),
        I::I32Ne,
        I::I32And,
    ]);
    asm.br_if("done");
    // Skip to the next unit that can start a match.
    asm.flag(program::PREFILTER);
    asm.ops([I::LocalGet(STICKY), I::I32Eqz, I::I32And]);
    asm.when();
    asm.block("found");
    // One or two possible first units: compare directly.
    asm.header(program::FIRST_COUNT);
    asm.when();
    asm.header(program::FIRST_UNIT);
    asm.set(A);
    asm.header(program::FIRST_UNIT + 1);
    asm.set(B);
    asm.looped("scan");
    asm.ops([I::LocalGet(POS), I::LocalGet(LEN), I::I32GeU]);
    asm.br_if("done");
    asm.get(POS);
    asm.input_unit();
    asm.ops([
        I::LocalTee(UNITV),
        I::LocalGet(A),
        I::I32Eq,
        I::LocalGet(UNITV),
        I::LocalGet(B),
        I::I32Eq,
        I::I32Or,
    ]);
    asm.br_if("found");
    asm.add_to(POS, 1);
    asm.br("scan");
    asm.end();
    asm.end();
    asm.looped("skip");
    asm.ops([I::LocalGet(POS), I::LocalGet(LEN), I::I32GeU]);
    asm.br_if("done");
    asm.get(POS);
    asm.input_unit();
    asm.set(UNITV);
    // ASCII units test the two 64-bit halves of the bitmap.
    asm.ops([I::LocalGet(UNITV), I::I32Const(HALF), I::I32LtU]);
    asm.when();
    asm.ops([
        I::LocalGet(LOW_MASK),
        I::LocalGet(UNITV),
        I::I64ExtendI32U,
        I::I64ShrU,
        I::I32WrapI64,
        I::I32Const(1),
        I::I32And,
    ]);
    asm.br_if("found");
    asm.otherwise();
    asm.ops([I::LocalGet(UNITV), I::I32Const(ASCII), I::I32LtU]);
    asm.when();
    asm.ops([
        I::LocalGet(HIGH_MASK),
        I::LocalGet(UNITV),
        I::I32Const(HALF),
        I::I32Sub,
        I::I64ExtendI32U,
        I::I64ShrU,
        I::I32WrapI64,
        I::I32Const(1),
        I::I32And,
    ]);
    asm.br_if("found");
    asm.otherwise();
    asm.header(program::FIRST_OTHER);
    asm.set(TMP);
    // Under `u`, never start inside a surrogate pair.
    asm.flag(program::UNICODE);
    asm.get(UNITV);
    asm.surrogate(LOW_SURROGATE);
    asm.ops([
        I::I32And,
        I::LocalGet(POS),
        I::LocalGet(BEGIN),
        I::I32GtU,
        I::I32And,
    ]);
    asm.when();
    asm.ops([I::LocalGet(POS), I::I32Const(1), I::I32Sub]);
    asm.input_unit();
    asm.surrogate(HIGH_SURROGATE);
    asm.when();
    asm.ops([I::I32Const(0), I::LocalSet(TMP)]);
    asm.end();
    asm.end();
    asm.get(TMP);
    asm.br_if("found");
    asm.end();
    asm.end();
    asm.add_to(POS, 1);
    asm.br("skip");
    asm.end();
    asm.end();
    asm.end();
    asm.end();
    // Add the lowest-priority thread starting here.
    asm.get(STICKY);
    asm.flag(program::ANCHORED);
    asm.ops([
        I::I32Or,
        I::I32Eqz,
        I::LocalGet(POS),
        I::LocalGet(BEGIN),
        I::I32Eq,
        I::I32Or,
    ]);
    asm.when();
    asm.ops([
        I::LocalGet(WC),
        I::I32Const(UNSET_BYTE),
        I::LocalGet(SLOTS),
        I::I32Const(WORD_BYTES),
        I::I32Mul,
        I::MemoryFill(MEMORY),
        I::LocalGet(POS),
        I::I32Const(1),
        I::I32Add,
        I::LocalSet(STAMP),
        I::LocalGet(CLIST),
        I::LocalSet(LIST),
        I::LocalGet(CCOUNT),
        I::LocalSet(LCOUNT),
        I::I32Const(0),
        I::LocalSet(X),
        I::LocalGet(POS),
        I::LocalSet(AT),
    ]);
    asm.add_thread();
    asm.get(LCOUNT);
    asm.set(CCOUNT);
    asm.end();
    asm.end();
    // Without a live thread after a match nothing can override it; without
    // a match, the next step starts another thread.
    asm.ops([
        I::LocalGet(CCOUNT),
        I::I32Eqz,
        I::LocalGet(MATCHED),
        I::I32And,
    ]);
    asm.br_if("done");
    // The character at the position (a code point under `u`), or -1.
    asm.ops([
        I::I32Const(1),
        I::LocalSet(WIDTH),
        I::I32Const(-1),
        I::LocalSet(CP),
    ]);
    asm.ops([I::LocalGet(POS), I::LocalGet(LEN), I::I32LtU]);
    asm.when();
    asm.get(POS);
    asm.input_unit();
    asm.set(CP);
    asm.flag(program::UNICODE);
    asm.get(CP);
    asm.surrogate(HIGH_SURROGATE);
    asm.ops([
        I::I32And,
        I::LocalGet(POS),
        I::I32Const(1),
        I::I32Add,
        I::LocalGet(LEN),
        I::I32LtU,
        I::I32And,
    ]);
    asm.when();
    asm.ops([I::LocalGet(POS), I::I32Const(1), I::I32Add]);
    asm.input_unit();
    asm.set(UNITV);
    asm.get(UNITV);
    asm.surrogate(LOW_SURROGATE);
    asm.when();
    asm.ops([
        I::LocalGet(CP),
        I::I32Const(HIGH_SURROGATE),
        I::I32Sub,
        I::I32Const(SURROGATE_BITS),
        I::I32Shl,
        I::LocalGet(UNITV),
        I::I32Const(LOW_SURROGATE),
        I::I32Sub,
        I::I32Add,
        I::I32Const(SUPPLEMENTARY),
        I::I32Add,
        I::LocalSet(CP),
        I::I32Const(2),
        I::LocalSet(WIDTH),
    ]);
    asm.end();
    asm.end();
    asm.end();
    // Step every thread in priority order over the character.
    asm.ops([
        I::I32Const(0),
        I::LocalSet(NCOUNT),
        I::LocalGet(POS),
        I::LocalGet(WIDTH),
        I::I32Add,
        I::I32Const(1),
        I::I32Add,
        I::LocalSet(STAMP),
        I::I32Const(0),
        I::LocalSet(I_),
    ]);
    asm.block("cut");
    asm.looped("threads");
    asm.ops([I::LocalGet(I_), I::LocalGet(CCOUNT), I::I32GeU]);
    asm.br_if("cut");
    asm.ops([
        I::LocalGet(I_),
        I::LocalGet(ENTRY),
        I::I32Mul,
        I::LocalGet(CLIST),
        I::I32Add,
        I::LocalTee(TPTR),
        I::I32Load(WORD),
        I::LocalSet(PC),
    ]);
    asm.decode(PC, false);
    // MATCH: record it and drop every lower-priority thread.
    asm.ops([I::LocalGet(OP), I::I32Const(i32::from(op::MATCH)), I::I32Eq]);
    asm.when();
    asm.ops([
        I::I32Const(1),
        I::LocalSet(MATCHED),
        I::LocalGet(BUF),
        I::LocalGet(TPTR),
        I::I32Const(WORD_BYTES),
        I::I32Add,
        I::LocalGet(CAPTURES),
        I::I32Const(WORD_BYTES),
        I::I32Mul,
        I::MemoryCopy {
            src_mem: MEMORY,
            dst_mem: MEMORY,
        },
    ]);
    asm.br("cut");
    asm.end();
    asm.ops([I::I32Const(0), I::LocalSet(TMP)]);
    asm.ops([I::LocalGet(CP), I::I32Const(0), I::I32GeS]);
    asm.when();
    asm.ops([I::LocalGet(OP), I::I32Const(i32::from(op::CHAR)), I::I32Eq]);
    asm.when();
    asm.ops([I::LocalGet(CP), I::LocalGet(A), I::I32Eq, I::LocalSet(TMP)]);
    asm.otherwise();
    asm.ops([I::LocalGet(OP), I::I32Const(i32::from(op::ANY)), I::I32Eq]);
    asm.when();
    asm.ops([I::I32Const(1), I::LocalSet(TMP)]);
    asm.otherwise();
    asm.class();
    asm.end();
    asm.end();
    asm.end();
    asm.get(TMP);
    asm.when();
    asm.ops([
        I::LocalGet(WC),
        I::LocalGet(TPTR),
        I::I32Const(WORD_BYTES),
        I::I32Add,
        I::LocalGet(SLOTS),
        I::I32Const(WORD_BYTES),
        I::I32Mul,
        I::MemoryCopy {
            src_mem: MEMORY,
            dst_mem: MEMORY,
        },
        I::LocalGet(NLIST),
        I::LocalSet(LIST),
        I::LocalGet(NCOUNT),
        I::LocalSet(LCOUNT),
        I::LocalGet(PC),
        I::I32Const(1),
        I::I32Add,
        I::LocalSet(X),
        I::LocalGet(POS),
        I::LocalGet(WIDTH),
        I::I32Add,
        I::LocalSet(AT),
    ]);
    asm.add_thread();
    asm.get(LCOUNT);
    asm.set(NCOUNT);
    asm.end();
    asm.add_to(I_, 1);
    asm.br("threads");
    asm.end();
    asm.end();
    // The next list becomes current.
    asm.ops([
        I::LocalGet(CLIST),
        I::LocalGet(NLIST),
        I::LocalSet(CLIST),
        I::LocalSet(NLIST),
        I::LocalGet(NCOUNT),
        I::LocalSet(CCOUNT),
        I::LocalGet(POS),
        I::LocalGet(LEN),
        I::I32GeU,
    ]);
    asm.br_if("done");
    asm.ops([
        I::LocalGet(POS),
        I::LocalGet(WIDTH),
        I::I32Add,
        I::LocalSet(POS),
    ]);
    asm.br("step");
    asm.end();
    asm.end();
    asm.get(MATCHED);
    asm.when();
    asm.ops([
        I::LocalGet(BUF),
        I::I64ExtendI32U,
        I::LocalGet(CAPTURES),
        I::I32Const(WORD_BYTES),
        I::I32Mul,
        I::I64ExtendI32U,
        I::I64Const(LENGTH_SHIFT as i64),
        I::I64Shl,
        I::I64Or,
        I::Return,
    ]);
    asm.end();
    asm.ops([I::I64Const(0), I::End]);
    debug_assert!(asm.labels.is_empty());
    let mut f = Function::new([(LAST_LOCAL - PROG + 1, ValType::I32), (MASKS, ValType::I64)]);
    instructions(&mut f, asm.code);
    f
}

/// `(captures: string, slot: f64) -> f64`: capture slot `slot` of a match
/// returned by [`exec`], -1 when unset or outside the result.
pub(super) fn slot() -> Function {
    const RESULT: u32 = 0;
    const INDEX: u32 = 1;
    const SLOT: u32 = 2;
    let mut f = Function::new([(1, ValType::I32)]);
    instructions(
        &mut f,
        [
            I::LocalGet(INDEX),
            I::F64Const(0.0.into()),
            I::F64Lt,
            I::LocalGet(INDEX),
            I::LocalGet(INDEX),
            I::F64Ne,
            I::I32Or,
            I::LocalGet(INDEX),
            I::LocalGet(RESULT),
            I::I64Const(LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I32WrapI64,
            I::I32Const(WORD_BYTES),
            I::I32DivU,
            I::F64ConvertI32U,
            I::F64Ge,
            I::I32Or,
            I::If(BlockType::Empty),
            I::F64Const((-1.0).into()),
            I::Return,
            I::End,
            I::LocalGet(INDEX),
            I::I32TruncF64U,
            I::LocalSet(SLOT),
            I::LocalGet(SLOT),
            I::I32Const(WORD_BYTES),
            I::I32Mul,
            I::LocalGet(RESULT),
            I::I32WrapI64,
            I::I32Add,
            I::I32Load(BOUND),
            I::F64ConvertI32S,
            I::End,
        ],
    );
    f
}

/// `(text: string, index: f64) -> f64`: the code unit at `index`, or -1
/// outside the string (a `charCodeAt` without a host call).
pub(super) fn unit() -> Function {
    const TEXT: u32 = 0;
    const INDEX: u32 = 1;
    let mut f = Function::new([]);
    instructions(
        &mut f,
        [
            I::LocalGet(INDEX),
            I::F64Const(0.0.into()),
            I::F64Lt,
            I::LocalGet(INDEX),
            I::LocalGet(INDEX),
            I::F64Ne,
            I::I32Or,
            I::LocalGet(INDEX),
            I::LocalGet(TEXT),
            I::I64Const(LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I32WrapI64,
            I::I32Const(UNIT_BYTES),
            I::I32DivU,
            I::F64ConvertI32U,
            I::F64Ge,
            I::I32Or,
            I::If(BlockType::Empty),
            I::F64Const((-1.0).into()),
            I::Return,
            I::End,
            I::LocalGet(INDEX),
            I::I32TruncF64U,
            I::I32Const(UNIT_BYTES),
            I::I32Mul,
            I::LocalGet(TEXT),
            I::I32WrapI64,
            I::I32Add,
            I::I32Load16U(UNIT),
            I::F64ConvertI32U,
            I::End,
        ],
    );
    f
}
