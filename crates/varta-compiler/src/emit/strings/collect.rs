//! Nonmoving mark/sweep with exact roots and bounded record-edge tracing.
use super::*;

const EDGE_CURSOR: u32 = 8;
const EDGE_MASK: u32 = 9;
const CHILD: u32 = 10;
const CHANGED: u32 = 11;
const BLOCK: u32 = 12;
const FIELD_COUNT: u32 = 13;
const IS_ARRAY: u32 = 14;
/// Wide-record tracing (value15): the next edge-bitmap word, or zero when the
/// block's edges are its 32-bit link-word mask.
const EDGE_WORDS: u32 = 15;
const EDGE_SLOT: u32 = 16;
/// V38 (`data_limits`) mark worklist pointer: header addresses of marked
/// blocks whose edges are still to be traced, stacked upwards in the unused
/// tail of the root region above the live roots.
const STACK: u32 = 17;
/// V38 scan cursor saved while the worklist drains inside a fallback scan.
const SCAN: u32 = 18;

pub(super) fn function(
    globals: Globals,
    trace_values: bool,
    closures: bool,
    frames: bool,
    wide: bool,
    limits: varta_common::limits::DataLimits,
) -> Function {
    let array_elements = limits.array_elements;
    let worklist = trace_values && wide && limits.amortized_growth;
    const ROOT_CURSOR: u32 = 0;
    const CURRENT: u32 = 1;
    const SIZE: u32 = 2;
    const PREVIOUS_FREE: u32 = 3;
    const HEAD: u32 = 4;
    const TAIL: u32 = 5;
    const LAST_PREVIOUS: u32 = 6;
    const PTR: u32 = 7;
    const AGAIN: u32 = 0;
    const DONE: u32 = 1;
    let mut function = Function::new([(
        if worklist {
            SCAN + 1
        } else if trace_values && wide {
            EDGE_SLOT + 1
        } else if trace_values {
            IS_ARRAY + 1
        } else {
            PTR + 1
        },
        ValType::I32,
    )]);
    instructions(
        &mut function,
        [
            I::I32Const(abi::STRING_ROOT_BASE as i32),
            I::LocalSet(ROOT_CURSOR),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(ROOT_CURSOR),
            I::GlobalGet(globals.roots),
            I::I32GeU,
            I::BrIf(DONE),
            I::LocalGet(ROOT_CURSOR),
            I::I64Load(ROOT),
            I::I32WrapI64,
            I::LocalTee(PTR),
            I::I32Const((abi::OWNED_STRING_BASE + abi::STRING_HEADER_BYTES) as i32),
            I::I32GeU,
            I::If(BlockType::Empty),
            I::LocalGet(PTR),
            I::GlobalGet(globals.heap),
            I::I32GeU,
            I::If(BlockType::Empty),
            I::Unreachable,
            I::End,
            I::LocalGet(PTR),
            I::I32Const(HEADER as i32),
            I::I32Sub,
            I::LocalSet(CURRENT),
            I::LocalGet(CURRENT),
            I::I32Load(FLAG),
            I::I32Const(ALLOCATED),
            I::I32And,
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::Unreachable,
            I::End,
            I::LocalGet(CURRENT),
            I::LocalGet(CURRENT),
            I::I32Load(FLAG),
            I::I32Const(MARKED),
            I::I32Or,
            I::I32Store(FLAG),
            I::End,
            I::LocalGet(ROOT_CURSOR),
            I::I32Const(abi::STRING_ROOT_SLOT_BYTES as i32),
            I::I32Add,
            I::LocalSet(ROOT_CURSOR),
            I::Br(AGAIN),
            I::End,
            I::End,
        ],
    );
    if worklist {
        worklist_trace(&mut function, globals, closures, frames, array_elements);
        // Only a worklist overflow leaves marked blocks with untraced edges;
        // the scans below then complete the mark exactly, draining the
        // worklist after every scanned block.
        instructions(
            &mut function,
            [I::LocalGet(CHANGED), I::If(BlockType::Empty)],
        );
    }
    if trace_values {
        trace(
            &mut function,
            globals,
            (closures, frames, wide),
            array_elements,
            worklist,
        );
    }
    if worklist {
        function.instruction(&I::End);
    }
    instructions(
        &mut function,
        [
            I::I32Const(abi::OWNED_STRING_BASE as i32),
            I::LocalSet(CURRENT),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(CURRENT),
            I::GlobalGet(globals.heap),
            I::I32GeU,
            I::BrIf(DONE),
            I::LocalGet(CURRENT),
            I::I32Load(WORD),
            I::LocalSet(SIZE),
            I::LocalGet(CURRENT),
            I::I32Load(FLAG),
            I::I32Const(MARKED),
            I::I32And,
            I::If(BlockType::Empty),
            I::LocalGet(CURRENT),
            I::LocalGet(CURRENT),
            I::I32Load(FLAG),
            I::I32Const(!MARKED),
            I::I32And,
            I::I32Store(FLAG),
            I::I32Const(0),
            I::LocalSet(PREVIOUS_FREE),
            I::Else,
            I::LocalGet(CURRENT),
            I::I32Const(0),
            I::I32Store(FLAG),
            I::LocalGet(PREVIOUS_FREE),
            I::If(BlockType::Empty),
            I::LocalGet(PREVIOUS_FREE),
            I::LocalGet(PREVIOUS_FREE),
            I::I32Load(WORD),
            I::LocalGet(SIZE),
            I::I32Add,
            I::I32Store(WORD),
            I::Else,
            I::LocalGet(TAIL),
            I::LocalSet(LAST_PREVIOUS),
            I::LocalGet(TAIL),
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::LocalGet(CURRENT),
            I::LocalSet(HEAD),
            I::Else,
            I::LocalGet(TAIL),
            I::LocalGet(CURRENT),
            I::I32Store(LINK),
            I::End,
            I::LocalGet(CURRENT),
            I::LocalSet(TAIL),
            I::LocalGet(CURRENT),
            I::LocalSet(PREVIOUS_FREE),
            I::LocalGet(CURRENT),
            I::I32Const(0),
            I::I32Store(LINK),
            I::End,
            I::End,
            I::LocalGet(CURRENT),
            I::LocalGet(SIZE),
            I::I32Add,
            I::LocalSet(CURRENT),
            I::Br(AGAIN),
            I::End,
            I::End,
            // A dead trailing run can be removed from both heap and free list.
            I::LocalGet(PREVIOUS_FREE),
            I::If(BlockType::Empty),
            I::LocalGet(PREVIOUS_FREE),
            I::GlobalSet(globals.heap),
            I::LocalGet(LAST_PREVIOUS),
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::I32Const(0),
            I::LocalSet(HEAD),
            I::Else,
            I::LocalGet(LAST_PREVIOUS),
            I::I32Const(0),
            I::I32Store(LINK),
            I::End,
            I::End,
            I::LocalGet(HEAD),
            I::GlobalSet(globals.free),
            I::End,
        ],
    );
    function
}

// Fixed-point tracing is iterative: no native/Wasm recursion or allocation during
// collection. Every newly reached block is marked once; scans consume guest fuel.
fn trace(
    function: &mut Function,
    globals: Globals,
    (closures, frames, wide): (bool, bool, bool),
    array_elements: usize,
    worklist: bool,
) {
    const CURRENT: u32 = 1;
    const AGAIN: u32 = 0;
    const DONE: u32 = 1;
    let mut head = vec![
        I::Loop(BlockType::Empty),
        I::I32Const(0),
        I::LocalSet(CHANGED),
        I::I32Const(abi::OWNED_STRING_BASE as i32),
        I::LocalSet(CURRENT),
        I::Block(BlockType::Empty),
        I::Loop(BlockType::Empty),
        I::LocalGet(CURRENT),
        I::GlobalGet(globals.heap),
        I::I32GeU,
        I::BrIf(DONE),
    ];
    head.extend(guard(closures, frames));
    head.push(I::If(BlockType::Empty));
    if worklist {
        // Newly marked children are pushed and drained at once; only another
        // overflow requires one more scan.
        head.extend(edges(globals, wide, array_elements, &push()));
        head.extend([I::LocalGet(CURRENT), I::LocalSet(SCAN)]);
        head.extend(drain(globals, closures, frames, array_elements));
        head.extend([I::LocalGet(SCAN), I::LocalSet(CURRENT)]);
    } else {
        head.extend(edges(
            globals,
            wide,
            array_elements,
            &[I::I32Const(1), I::LocalSet(CHANGED)],
        ));
    }
    head.extend([
        I::End,
        I::LocalGet(CURRENT),
        I::LocalGet(CURRENT),
        I::I32Load(WORD),
        I::I32Add,
        I::LocalSet(CURRENT),
        I::Br(AGAIN),
        I::End,
        I::End,
        I::LocalGet(CHANGED),
        I::BrIf(AGAIN),
        I::End,
    ]);
    instructions(function, head);
}

/// V38 (`data_limits`) marking: push every rooted heap block, then pop and
/// trace blocks, pushing each newly marked child, until the worklist is
/// empty. Marking work is linear in the live edges instead of one heap scan
/// per level of depth. A full worklist sets `CHANGED`, and the caller then
/// finishes with the fixed-point scans, so overflow never loses a mark. The
/// worklist allocates nothing and stays below the root region's end.
fn worklist_trace(
    function: &mut Function,
    globals: Globals,
    closures: bool,
    frames: bool,
    array_elements: usize,
) {
    const ROOT_CURSOR: u32 = 0;
    const PTR: u32 = 7;
    const AGAIN: u32 = 0;
    const DONE: u32 = 1;
    let push = push();
    let mut code = vec![
        I::I32Const(0),
        I::LocalSet(CHANGED),
        I::GlobalGet(globals.roots),
        I::LocalSet(STACK),
        I::I32Const(abi::STRING_ROOT_BASE as i32),
        I::LocalSet(ROOT_CURSOR),
        I::Block(BlockType::Empty),
        I::Loop(BlockType::Empty),
        I::LocalGet(ROOT_CURSOR),
        I::GlobalGet(globals.roots),
        I::I32GeU,
        I::BrIf(DONE),
        I::LocalGet(ROOT_CURSOR),
        I::I64Load(ROOT),
        I::I32WrapI64,
        I::LocalTee(PTR),
        I::I32Const((abi::OWNED_STRING_BASE + abi::STRING_HEADER_BYTES) as i32),
        I::I32GeU,
        I::If(BlockType::Empty),
        I::LocalGet(PTR),
        I::I32Const(HEADER as i32),
        I::I32Sub,
        I::LocalSet(BLOCK),
    ];
    code.extend(push.clone());
    code.extend([
        I::End,
        I::LocalGet(ROOT_CURSOR),
        I::I32Const(abi::STRING_ROOT_SLOT_BYTES as i32),
        I::I32Add,
        I::LocalSet(ROOT_CURSOR),
        I::Br(AGAIN),
        I::End,
        I::End,
    ]);
    code.extend(drain(globals, closures, frames, array_elements));
    instructions(function, code);
}

/// Bytes of one worklist entry (a block header address).
const ENTRY: i32 = size_of::<u32>() as i32;
/// Push the header in `BLOCK` onto the worklist; a full worklist records the
/// overflow in `CHANGED` instead.
fn push() -> Vec<I<'static>> {
    vec![
        I::LocalGet(STACK),
        I::I32Const(abi::STRING_ROOT_END as i32 - ENTRY),
        I::I32LeU,
        I::If(BlockType::Empty),
        I::LocalGet(STACK),
        I::LocalGet(BLOCK),
        I::I32Store(WORD),
        I::LocalGet(STACK),
        I::I32Const(ENTRY),
        I::I32Add,
        I::LocalSet(STACK),
        I::Else,
        I::I32Const(1),
        I::LocalSet(CHANGED),
        I::End,
    ]
}
/// Pop and trace worklist entries (in `CURRENT`) until the worklist is empty.
fn drain(globals: Globals, closures: bool, frames: bool, array_elements: usize) -> Vec<I<'static>> {
    const CURRENT: u32 = 1;
    const AGAIN: u32 = 0;
    const DONE: u32 = 1;
    let mut code = vec![
        I::Block(BlockType::Empty),
        I::Loop(BlockType::Empty),
        I::LocalGet(STACK),
        I::GlobalGet(globals.roots),
        I::I32LeU,
        I::BrIf(DONE),
        I::LocalGet(STACK),
        I::I32Const(ENTRY),
        I::I32Sub,
        I::LocalTee(STACK),
        I::I32Load(WORD),
        I::LocalSet(CURRENT),
    ];
    code.extend(guard(closures, frames));
    code.push(I::If(BlockType::Empty));
    code.extend(edges(globals, true, array_elements, &push()));
    code.extend([I::End, I::Br(AGAIN), I::End, I::End]);
    code
}

/// `[] -> [i32]`: whether the block at `CURRENT` is marked and has edges.
fn guard(closures: bool, frames: bool) -> Vec<I<'static>> {
    use varta_common::value as values;
    const CURRENT: u32 = 1;
    vec![
        I::LocalGet(CURRENT),
        I::I32Load(FLAG),
        I::I32Const(ALLOCATED | MARKED),
        I::I32And,
        I::I32Const(ALLOCATED | MARKED),
        I::I32Eq,
        I::LocalGet(CURRENT),
        I::I32Load(FLAG),
        I::I32Const(
            values::HEAP_ERROR_FLAG
                    | values::HEAP_RECORD_FLAG
                    | values::HEAP_ARRAY_FLAG
                    | values::HEAP_UNION_FLAG
                    | values::HEAP_VECTOR_FLAG
                    | if closures {
                        varta_common::function::HEAP_FLAG
                            | varta_common::function::ENVIRONMENT_FLAG
                            | varta_common::function::BINDING_FLAG
                    } else {
                        0
                    }
                    // Continuation frames trace their environment, completion
                    // and handler edges through the same exact edge mask.
                    | if frames {
                        varta_common::function::frame::FRAME_FLAG
                    } else {
                        0
                    },
        ),
        I::I32And,
        I::I32Eqz,
        I::I32Eqz,
        I::I32And,
    ]
}

/// The edges of the marked block at `CURRENT` (inside its guard): every
/// unmarked child is marked, then `on_mark` runs with the child's header in
/// `BLOCK`.
fn edges(
    globals: Globals,
    wide: bool,
    array_elements: usize,
    on_mark: &[I<'static>],
) -> Vec<I<'static>> {
    use varta_common::value as values;
    const CURRENT: u32 = 1;
    const AGAIN: u32 = 0;
    const DONE: u32 = 1;
    let record_limit = if wide {
        values::MAX_WIDE_RECORD_FIELDS
    } else {
        values::MAX_RECORD_FIELDS
    };
    let mut code = vec![
        I::LocalGet(CURRENT),
        I::I32Load(FLAG),
        I::I32Const(values::HEAP_ARRAY_FLAG),
        I::I32And,
        I::LocalSet(IS_ARRAY),
        I::LocalGet(CURRENT),
        I::I32Load(LINK),
        I::LocalSet(EDGE_MASK),
        I::LocalGet(CURRENT),
        I::I32Const((HEADER as usize + values::RECORD_META_BYTES) as i32),
        I::I32Add,
        I::LocalSet(EDGE_CURSOR),
        I::LocalGet(CURRENT),
        I::I32Load(FLAG),
        I::I32Const(values::HEAP_VECTOR_FLAG),
        I::I32And,
        I::If(BlockType::Result(ValType::I32)),
        I::I32Const(1),
        I::Else,
        I::LocalGet(CURRENT),
        I::I32Const(HEADER as i32 + 4),
        I::I32Add,
        I::I32Load(WORD),
        I::End,
        I::LocalTee(FIELD_COUNT),
        I::LocalGet(IS_ARRAY),
        I::If(BlockType::Result(ValType::I32)),
        // The profile's element limit (v38 `data_limits` lifts the 1024 one).
        I::I32Const(array_elements as i32),
        I::Else,
        I::LocalGet(CURRENT),
        I::I32Load(FLAG),
        I::I32Const(values::HEAP_RECORD_FLAG),
        I::I32And,
        I::If(BlockType::Result(ValType::I32)),
        I::I32Const(record_limit as i32),
        I::Else,
        I::I32Const(values::MAX_RECORD_FIELDS as i32),
        I::End,
        I::End,
        I::I32GtU,
        I::If(BlockType::Empty),
        I::Unreachable,
        I::End,
    ];
    if wide {
        // A record wider than 32 fields reads its edges from the bitmap that
        // trails its static descriptor (payload word 0), one word per 32 slots.
        code.extend([
            I::I32Const(0),
            I::LocalSet(EDGE_WORDS),
            I::I32Const(0),
            I::LocalSet(EDGE_SLOT),
            I::LocalGet(IS_ARRAY),
            I::I32Eqz,
            I::LocalGet(FIELD_COUNT),
            I::I32Const(values::MAX_RECORD_FIELDS as i32),
            I::I32GtU,
            I::I32And,
            I::If(BlockType::Empty),
            I::LocalGet(CURRENT),
            I::I32Load(wasm_encoder::MemArg {
                offset: HEADER as u64,
                ..WORD
            }),
            I::LocalGet(FIELD_COUNT),
            I::I32Const(values::RECORD_DESCRIPTOR_ENTRY as i32),
            I::I32Mul,
            I::I32Add,
            I::I32Const(values::RECORD_DESCRIPTOR_HEADER as i32),
            I::I32Add,
            I::LocalTee(EDGE_WORDS),
            I::I32Load(WORD),
            I::LocalSet(EDGE_MASK),
            I::End,
        ]);
    }
    code.extend([
        I::Block(BlockType::Empty),
        I::Loop(BlockType::Empty),
        I::LocalGet(FIELD_COUNT),
        I::I32Eqz,
        I::BrIf(DONE),
        I::LocalGet(EDGE_MASK),
        I::I32Const(1),
        I::I32And,
        I::If(BlockType::Empty),
        I::LocalGet(EDGE_CURSOR),
        I::I64Load(ROOT),
        I::I32WrapI64,
        I::LocalTee(CHILD),
        I::I32Const((abi::OWNED_STRING_BASE + abi::STRING_HEADER_BYTES) as i32),
        I::I32GeU,
        I::If(BlockType::Empty),
        I::LocalGet(CHILD),
        I::GlobalGet(globals.heap),
        I::I32GeU,
        I::If(BlockType::Empty),
        I::Unreachable,
        I::End,
        I::LocalGet(CHILD),
        I::I32Const(HEADER as i32),
        I::I32Sub,
        I::LocalSet(BLOCK),
        I::LocalGet(BLOCK),
        I::I32Load(FLAG),
        I::I32Const(ALLOCATED),
        I::I32And,
        I::I32Eqz,
        I::If(BlockType::Empty),
        I::Unreachable,
        I::End,
        I::LocalGet(BLOCK),
        I::I32Load(FLAG),
        I::I32Const(MARKED),
        I::I32And,
        I::I32Eqz,
        I::If(BlockType::Empty),
        I::LocalGet(BLOCK),
        I::LocalGet(BLOCK),
        I::I32Load(FLAG),
        I::I32Const(MARKED),
        I::I32Or,
        I::I32Store(FLAG),
    ]);
    code.extend_from_slice(on_mark);
    code.extend([
        I::End,
        I::End,
        I::End,
        I::LocalGet(EDGE_CURSOR),
        I::I32Const(values::RECORD_SLOT_BYTES as i32),
        I::I32Add,
        I::LocalSet(EDGE_CURSOR),
        I::LocalGet(IS_ARRAY),
        I::I32Eqz,
        I::If(BlockType::Empty),
        I::LocalGet(EDGE_MASK),
        I::I32Const(1),
        I::I32ShrU,
        I::LocalSet(EDGE_MASK),
        I::End,
    ]);
    if wide {
        code.extend([
            I::LocalGet(EDGE_WORDS),
            I::If(BlockType::Empty),
            I::LocalGet(EDGE_SLOT),
            I::I32Const(1),
            I::I32Add,
            I::LocalTee(EDGE_SLOT),
            I::I32Const(values::PRESENCE_WORD_BITS as i32 - 1),
            I::I32And,
            I::I32Eqz,
            I::LocalGet(FIELD_COUNT),
            I::I32Const(1),
            I::I32GtU,
            I::I32And,
            I::If(BlockType::Empty),
            I::LocalGet(EDGE_WORDS),
            I::I32Const(size_of::<u32>() as i32),
            I::I32Add,
            I::LocalTee(EDGE_WORDS),
            I::I32Load(WORD),
            I::LocalSet(EDGE_MASK),
            I::End,
            I::End,
        ]);
    }
    code.extend([
        I::LocalGet(FIELD_COUNT),
        I::I32Const(1),
        I::I32Sub,
        I::LocalSet(FIELD_COUNT),
        I::Br(AGAIN),
        I::End,
        I::End,
    ]);
    code
}
