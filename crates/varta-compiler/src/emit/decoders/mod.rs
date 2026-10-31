//! V38 compiled JSON decoders (`json_decoders`): one checker and one builder
//! per schema node of the module's checked parse targets, shared by every
//! call site, plus a scanner shared by all of them and one entry per target.
//!
//! An entry runs two passes over the guest's UTF-16 text, read in place:
//!
//! 1. **Check.** The node checkers scan the whole input against the target
//!    type: the JSON grammar, the target's shape (fields matched by length
//!    and their UTF-16 units, compared eight bytes at a time; union members
//!    selected by the value's first unit, exactly as the shared decoder
//!    selects them; required fields, duplicates) and the v38 limits. They
//!    allocate no value: what the build needs (container counts, field
//!    slots, union members, string spans, number bits) goes onto a tape in a
//!    guest heap block rooted for the call, beside the rooted input string.
//! 2. **Build.** Only after the whole input is accepted do the node
//!    builders read the tape and build the value top-down, as
//!    `varta_common::json::build` builds it: every container is published
//!    into its reachable parent (or retained, at the root) before any child
//!    is allocated, with the same layouts, presence and order words, missing
//!    `undefined` cells and own-key order.
//!
//! A checker accepts only inputs on which it is certain to agree with the
//! shared decoder; on anything else (every failure, and the rare inputs it
//! leaves alone: escaped or index-like keys, dictionaries beyond
//! [`DICTIONARY_ENTRIES`], nesting beyond [`DEPTH`], numbers near the
//! binary64 overflow, inputs that could transcode past the byte limit) the
//! entry drops its scratch roots and calls the linked `json_parse_v19` with
//! the target's static schema, which then decides the outcome alone: the
//! same values, `SyntaxError`/`TypeError` and typed limits as before.
//! Nothing survives a call.
mod asm;
mod build;
mod check;
mod scan;

use super::*;
use asm::Asm;
use varta_common::json::JsonType;
use varta_common::value::ValueKind;

/// Container nesting the checkers follow themselves (recursion through the
/// node functions); deeper inputs take the fallback.
const DEPTH: i32 = 128;
/// Dictionary entries the checkers deduplicate themselves (linearly);
/// larger dictionaries take the fallback.
const DICTIONARY_ENTRIES: i32 = 64;
/// Record fields one presence mask covers; wider records take the fallback.
const MASK_FIELDS: usize = u64::BITS as usize;
/// The first tape block's bytes, at most (it doubles while it fills).
const TAPE_BYTES: i32 = 4096;
/// Bytes of one tape entry.
const ENTRY: i32 = size_of::<u64>() as i32;
/// Tape bytes per input byte of the first block: about one entry per UTF-16
/// unit, the most a compact document needs; the tape doubles beyond it.
const TAPE_PER_BYTE: i32 = ENTRY / size_of::<u16>() as i32;
/// Inputs longer than this (in bytes) are left to the fallback: the shared
/// decoder's byte limit counts the input's UTF-8 transcoding, up to three
/// bytes per input byte, so shorter inputs never reach it.
const FALLBACK_INPUT_BYTES: i32 =
    (varta_common::json::scan::JsonLimits::V38.input_bytes / 4) as i32;
/// The high word of a tape entry holding a number literal the build parses
/// through the linked conversion (a NaN no literal produces), the literal's
/// address in the low word.
const SLOW_NUMBER: i64 = 0xfff8_0000;
/// The flag of a tape string entry whose text holds an escape (high word).
const ESCAPED: i64 = 1 << 31;
/// The exact powers of ten `10^0` to `10^22` (binary64 holds them exactly).
pub(super) const POWERS: usize = 23;

/// The bytes of the static table of [`POWERS`] powers of ten.
pub(super) fn powers() -> Vec<u8> {
    let mut power = 1.0f64;
    let mut bytes = Vec::with_capacity(POWERS * size_of::<f64>());
    for _ in 0..POWERS {
        bytes.extend(power.to_le_bytes());
        power *= 10.0;
    }
    bytes
}

/// Test-only switches, per thread: [`Mode::Generic`] compiles v38 checked
/// parses to the linked `json_parse_v19` alone (no decoders), the path the
/// decoders are compared against; [`Mode::Collecting`] compiles decoders
/// that run a full collection before every block they allocate.
pub(crate) mod hook {
    #[cfg(any(test, feature = "suspension-test-hook"))]
    use std::cell::Cell;
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(crate) enum Mode {
        Production,
        Generic,
        Collecting,
    }
    #[cfg(any(test, feature = "suspension-test-hook"))]
    thread_local! {
        static MODE: Cell<Mode> = const { Cell::new(Mode::Production) };
    }
    /// The mode of this thread.
    pub(crate) fn mode() -> Mode {
        #[cfg(any(test, feature = "suspension-test-hook"))]
        return MODE.with(Cell::get);
        #[cfg(not(any(test, feature = "suspension-test-hook")))]
        Mode::Production
    }
    /// Run `f` in `mode` on this thread.
    #[cfg(any(test, feature = "suspension-test-hook"))]
    pub(crate) fn with<T>(mode: Mode, f: impl FnOnce() -> T) -> T {
        MODE.with(|current| current.set(mode));
        let result = f();
        MODE.with(|current| current.set(Mode::Production));
        result
    }
}

/// Whether a module of `profile` compiles its checked parses to decoders.
pub(super) fn enabled(profile: varta_common::language::LanguageProfile) -> bool {
    profile.json_decoders()
        && crate::link::links(profile, profile.json_parse())
        && hook::mode() != hook::Mode::Generic
}

/// The shared functions, in index order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Shared {
    /// `(pos) -> pos`: skip JSON whitespace.
    Whitespace,
    /// `(pos) -> i64`: scan the string whose quote is at `pos`: the position
    /// after it (low word, 0 when malformed) and its units with
    /// [`ESCAPED`] (high word).
    Text,
    /// `(i64)`: append a tape entry, growing the tape.
    Push,
    /// `()`: double the tape.
    Grow,
    /// `(a, b, bytes) -> i32`: whether two ranges hold the same bytes.
    Same,
    /// `(len, flags, link) -> ptr`: a stamped heap block.
    Block,
    /// `(dst, handle)`: store a block at `dst`, or retain it (`dst` 0).
    Publish,
    /// `(dst, kind, link) -> ptr`: a published union cell with payload 0.
    Cell,
    /// `(dst, kind, count, link) -> cell`: a published vector cell with its
    /// published storage of `count` present zero slots.
    Vector,
    /// `(entry) -> handle`: the string of a tape string entry.
    Decode,
    /// `(start) -> bits`: the number literal at `start` through the linked
    /// conversion.
    Slow,
    /// Checkers `(pos, depth) -> pos` (0: fallback) of the primitive nodes.
    CheckString,
    CheckNumber,
    CheckBoolean,
    CheckNull,
    /// The checker of the nodes the checkers leave to the fallback.
    CheckFallback,
    /// Builders `(dst) -> payload` of the primitive nodes.
    BuildString,
    BuildNumber,
    BuildBoolean,
    BuildNull,
}
impl Shared {
    const ALL: [Self; 20] = [
        Self::Whitespace,
        Self::Text,
        Self::Push,
        Self::Grow,
        Self::Same,
        Self::Block,
        Self::Publish,
        Self::Cell,
        Self::Vector,
        Self::Decode,
        Self::Slow,
        Self::CheckString,
        Self::CheckNumber,
        Self::CheckBoolean,
        Self::CheckNull,
        Self::CheckFallback,
        Self::BuildString,
        Self::BuildNumber,
        Self::BuildBoolean,
        Self::BuildNull,
    ];
    fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        use ValType::{I32, I64};
        match self {
            Self::Whitespace => (vec![I32], vec![I32]),
            Self::Text => (vec![I32], vec![I64]),
            Self::Push => (vec![I64], vec![]),
            Self::Grow => (vec![], vec![]),
            Self::Same | Self::Block | Self::Cell => (vec![I32; 3], vec![I32]),
            Self::Publish => (vec![I32, I64], vec![]),
            Self::Vector => (vec![I32; 4], vec![I32]),
            Self::Decode => (vec![I64], vec![I64]),
            Self::Slow => (vec![I32], vec![I64]),
            Self::CheckString
            | Self::CheckNumber
            | Self::CheckBoolean
            | Self::CheckNull
            | Self::CheckFallback => CHECK.clone(),
            Self::BuildString | Self::BuildNumber | Self::BuildBoolean | Self::BuildNull => {
                BUILD.clone()
            }
        }
    }
}
/// The checker signature `(pos, depth) -> pos`.
static CHECK: std::sync::LazyLock<(Vec<ValType>, Vec<ValType>)> =
    std::sync::LazyLock::new(|| (vec![ValType::I32; 2], vec![ValType::I32]));
/// The builder signature `(dst) -> payload`.
static BUILD: std::sync::LazyLock<(Vec<ValType>, Vec<ValType>)> =
    std::sync::LazyLock::new(|| (vec![ValType::I32], vec![ValType::I64]));
/// The entry signature `(text) -> payload`.
static ENTRY_SIGNATURE: std::sync::LazyLock<(Vec<ValType>, Vec<ValType>)> =
    std::sync::LazyLock::new(|| (vec![ValType::I64], vec![ValType::I64]));

/// What the node functions are.
#[derive(Clone, Copy)]
enum Node {
    /// A record, array, dictionary or union: its own checker and builder.
    Own { check: u32, build: u32 },
    /// A primitive (or a node left to the fallback): shared functions.
    Shared { check: Shared, build: Shared },
}

/// The decoder globals (after every other global of the module).
#[derive(Clone, Copy)]
pub(super) struct Globals {
    /// The input's end address.
    pub end: u32,
    /// The tape's next entry (checking: the next write; building: the next
    /// read).
    pub tape: u32,
    /// The tape block's start and end.
    pub base: u32,
    pub limit: u32,
    /// The root-stack slot that retains the tape block.
    pub slot: u32,
}
pub(super) const GLOBALS: u32 = 5;

/// The decoders of one module: function indices from `begin`.
pub(super) struct Plan<'a> {
    graph: &'a schemas::Graph,
    /// The record shapes behind the record nodes (their canonical order).
    shapes: &'a [ir::RecordShape],
    nodes: Vec<Node>,
    shared: u32,
    /// The entry of each root type, with the root's static schema.
    entries: Vec<(Type, u32, (u32, u32))>,
    functions: u32,
}

impl<'a> Plan<'a> {
    /// Index the decoders of `graph` from function `begin`; `schemas` holds
    /// each root's static schema.
    pub(super) fn new(
        graph: &'a schemas::Graph,
        shapes: &'a [ir::RecordShape],
        schemas: &BTreeMap<Type, (u32, u32)>,
        begin: u32,
    ) -> Result<Self> {
        let shared = begin;
        let mut next = begin + Shared::ALL.len() as u32;
        let nodes = graph
            .nodes
            .iter()
            .map(|node| match node {
                JsonType::Record { fields, .. } if fields.len() > MASK_FIELDS => Node::Shared {
                    check: Shared::CheckFallback,
                    build: Shared::BuildNull,
                },
                JsonType::Record { .. }
                | JsonType::Array { .. }
                | JsonType::Dictionary { .. }
                | JsonType::PrimitiveUnion { .. }
                | JsonType::ReferenceUnion { .. } => {
                    let own = Node::Own {
                        check: next,
                        build: next + 1,
                    };
                    next += 2;
                    own
                }
                JsonType::String => Node::Shared {
                    check: Shared::CheckString,
                    build: Shared::BuildString,
                },
                JsonType::Float64 => Node::Shared {
                    check: Shared::CheckNumber,
                    build: Shared::BuildNumber,
                },
                JsonType::Boolean => Node::Shared {
                    check: Shared::CheckBoolean,
                    build: Shared::BuildBoolean,
                },
                JsonType::Null => Node::Shared {
                    check: Shared::CheckNull,
                    build: Shared::BuildNull,
                },
                // JSON never holds `undefined`, an i32 slot is never a v38
                // target and BigInt text is left to the fallback.
                JsonType::Undefined | JsonType::I32 | JsonType::BigInt => Node::Shared {
                    check: Shared::CheckFallback,
                    build: Shared::BuildNull,
                },
            })
            .collect();
        let entries = graph
            .roots
            .iter()
            .map(|(ty, node)| {
                let schema = schemas
                    .get(ty)
                    .copied()
                    .ok_or_else(|| CompilerError::Internal("missing decoder schema".into()))?;
                Ok((*ty, *node, schema))
            })
            .collect::<Result<Vec<_>>>()?;
        let functions = next + entries.len() as u32 - begin;
        Ok(Self {
            graph,
            shapes,
            nodes,
            shared,
            entries,
            functions,
        })
    }
    /// The entry function of root type `ty`.
    pub(super) fn entry(&self, ty: Type) -> Option<u32> {
        let first = self.shared + self.functions - self.entries.len() as u32;
        self.entries
            .iter()
            .position(|(root, _, _)| *root == ty)
            .map(|index| first + index as u32)
    }
    /// The signatures of every decoder function, in index order.
    pub(super) fn signatures(&self) -> Vec<(Vec<ValType>, Vec<ValType>)> {
        let mut signatures: Vec<_> = Shared::ALL.iter().map(|s| s.signature()).collect();
        for node in &self.nodes {
            if matches!(node, Node::Own { .. }) {
                signatures.extend([CHECK.clone(), BUILD.clone()]);
            }
        }
        signatures.extend(self.entries.iter().map(|_| ENTRY_SIGNATURE.clone()));
        signatures
    }
    /// Whether some node is a number (its builder may call the linked
    /// conversion).
    pub(super) fn needs_number(graph: &schemas::Graph) -> bool {
        graph
            .nodes
            .iter()
            .any(|node| matches!(node, JsonType::Float64))
    }
    fn shared(&self, shared: Shared) -> u32 {
        self.shared
            + Shared::ALL
                .iter()
                .position(|s| *s == shared)
                .unwrap_or_else(|| unreachable!()) as u32
    }
    fn check(&self, node: u32) -> u32 {
        match self.nodes[node as usize] {
            Node::Own { check, .. } => check,
            Node::Shared { check, .. } => self.shared(check),
        }
    }
    fn build(&self, node: u32) -> u32 {
        match self.nodes[node as usize] {
            Node::Own { build, .. } => build,
            Node::Shared { build, .. } => self.shared(build),
        }
    }
    /// Whether node `node` builds a container its builder publishes itself
    /// (a record, array, dictionary or union cell); other builders return a
    /// payload the caller stores.
    fn publishes(&self, node: u32) -> bool {
        matches!(self.nodes[node as usize], Node::Own { .. })
    }
    fn kind(&self, node: u32) -> ValueKind {
        self.graph.nodes[node as usize].kind()
    }
}

/// What the decoder bodies call and read besides each other.
#[derive(Clone, Copy)]
pub(super) struct Context {
    pub globals: Globals,
    pub memory: strings::Globals,
    pub allocate: u32,
    pub retain: u32,
    pub result_len: u32,
    /// The linked checked parse, the fallback.
    pub fallback: u32,
    /// The linked `primitive_to_number`, when a node is a number.
    pub number: Option<u32>,
    /// The static table of [`POWERS`] powers of ten.
    pub powers: u32,
    /// Test-only ([`hook::Mode::Collecting`]): the collector, run before
    /// every decoder block.
    pub collect: Option<u32>,
}

/// Every decoder body, in index order, with a label for fuel profiles.
pub(super) fn bodies(
    plan: &Plan,
    context: Context,
) -> Result<Vec<(String, wasm_encoder::Function)>> {
    let mut bodies = Vec::new();
    for shared in Shared::ALL {
        bodies.push((
            format!("<json {shared:?}>"),
            scan::shared(plan, context, shared)?,
        ));
    }
    for (index, node) in plan.graph.nodes.iter().enumerate() {
        if !matches!(plan.nodes[index], Node::Own { .. }) {
            continue;
        }
        let index = index as u32;
        bodies.push((
            format!("<json check {index}>"),
            check::node(plan, context, node)?,
        ));
        bodies.push((
            format!("<json build {index}>"),
            build::node(plan, context, index, node)?,
        ));
    }
    for &(_, node, schema) in &plan.entries {
        bodies.push((
            format!("<json entry {node}>"),
            entry(plan, context, node, schema),
        ));
    }
    Ok(bodies)
}

/// `(text) -> payload`: check, then build, the target `node`; any input the
/// checkers do not accept goes to the linked parse with `schema`.
fn entry(plan: &Plan, context: Context, node: u32, schema: (u32, u32)) -> wasm_encoder::Function {
    use wasm_encoder::Instruction as I;
    let Globals {
        end,
        tape,
        base,
        limit,
        slot,
    } = context.globals;
    let roots = context.memory.roots;
    let mut a = Asm::new(1);
    const TEXT: u32 = 0;
    let ptr = a.local(ValType::I32);
    let len = a.local(ValType::I32);
    let mark = a.local(ValType::I32);
    let pos = a.local(ValType::I32);
    let capacity = a.local(ValType::I32);
    let block = a.local(ValType::I32);
    let result = a.local(ValType::I64);
    a.get(TEXT).i(I::I32WrapI64).set(ptr);
    a.get(TEXT)
        .long(strings::LENGTH_SHIFT as i64)
        .i(I::I64ShrU)
        .i(I::I32WrapI64)
        .set(len);
    a.i(I::GlobalGet(roots)).set(mark);
    a.block("fallback");
    // An odd length is malformed; an input that could transcode past the
    // byte limit is checked by the shared decoder.
    a.get(len).int(1).i(I::I32And).br_if("fallback");
    a.get(len)
        .int(FALLBACK_INPUT_BYTES)
        .i(I::I32GtU)
        .br_if("fallback");
    a.get(ptr).get(len).i(I::I32Add).i(I::GlobalSet(end));
    // The input and the tape stay rooted until the result is built.
    a.get(TEXT).i(I::Call(context.retain)).i(I::Drop);
    a.get(len)
        .int(TAPE_PER_BYTE)
        .i(I::I32Mul)
        .int(ENTRY)
        .i(I::I32Add)
        .tee(capacity)
        .int(TAPE_BYTES)
        .i(I::I32GtU)
        .if_()
        .int(TAPE_BYTES)
        .set(capacity)
        .end();
    a.get(capacity)
        .int(0)
        .int(0)
        .i(I::Call(plan.shared(Shared::Block)))
        .set(block);
    a.i(I::GlobalGet(roots)).i(I::GlobalSet(slot));
    a.get(capacity)
        .i(I::I64ExtendI32U)
        .long(strings::LENGTH_SHIFT as i64)
        .i(I::I64Shl)
        .get(block)
        .i(I::I64ExtendI32U)
        .i(I::I64Or)
        .i(I::Call(context.retain))
        .i(I::Drop);
    a.get(block).i(I::GlobalSet(base));
    a.get(block).i(I::GlobalSet(tape));
    a.get(block)
        .get(capacity)
        .i(I::I32Add)
        .i(I::GlobalSet(limit));
    a.get(ptr)
        .int(0)
        .i(I::Call(plan.check(node)))
        .tee(pos)
        .i(I::I32Eqz)
        .br_if("fallback");
    a.get(pos)
        .i(I::Call(plan.shared(Shared::Whitespace)))
        .i(I::GlobalGet(end))
        .i(I::I32Ne)
        .br_if("fallback");
    // Accepted: build from the start of the tape.
    a.i(I::GlobalGet(base)).i(I::GlobalSet(tape));
    a.int(0).i(I::Call(plan.build(node))).set(result);
    a.get(mark).i(I::GlobalSet(roots));
    if plan.kind(node).is_reference() {
        a.get(result).i(I::Call(context.retain)).i(I::Drop);
    }
    a.int(0).i(I::GlobalSet(context.result_len));
    a.get(result).i(I::Return);
    a.end();
    a.get(mark).i(I::GlobalSet(roots));
    a.get(ptr)
        .get(len)
        .int(schema.0 as i32)
        .int(schema.1 as i32)
        .i(I::Call(context.fallback));
    a.finish()
}

#[cfg(test)]
mod tests;
