//! Nonmoving immutable strings with exact compiler roots and a bounded reusable heap.
mod allocate;
mod array_enumeration;
mod array_fill;
mod array_index;
mod array_own_key;
mod array_push;
mod array_resize;
mod array_reverse;
mod array_search;
mod array_slice;
mod collect;
mod compare;
mod concat;
mod roots;
mod sparse;
mod string_index;
mod string_range;
mod string_search;
mod string_trim;
use std::collections::BTreeMap;
use varta_common::abi;
use wasm_encoder::{BlockType, Function, Instruction as I, MemArg, ValType};

pub(super) const LENGTH_SHIFT: u32 = i32::BITS;
pub(super) const MEMORY: u32 = 0;
const ALIGN: u32 = varta_common::value::HEAP_ALIGNMENT as u32;
const HEADER: u32 = abi::STRING_HEADER_BYTES as u32;
const FLAGS: u64 = varta_common::value::HEAP_FLAGS_OFFSET as u64;
const NEXT: u64 = varta_common::value::HEAP_LINK_OFFSET as u64;
pub(super) const ALLOCATED: i32 = varta_common::value::HEAP_ALLOCATED_FLAG as i32;
const MARKED: i32 = 2;
pub(super) const WORD: MemArg = MemArg {
    offset: 0,
    align: 2,
    memory_index: MEMORY,
};
pub(super) const FLAG: MemArg = MemArg {
    offset: FLAGS,
    ..WORD
};
pub(super) const LINK: MemArg = MemArg {
    offset: NEXT,
    ..WORD
};
pub(super) const ROOT: MemArg = MemArg {
    offset: 0,
    align: 3,
    memory_index: MEMORY,
};
#[derive(Clone, Copy)]
pub(super) struct Globals {
    pub heap: u32,
    pub free: u32,
    pub roots: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Helper {
    Snapshot,
    Concat,
    Compare,
    StringIncludes,
    StringStartsWith,
    StringEndsWith,
    StringIndexOf,
    StringLastIndexOf,
    StringSlice,
    StringSubstring,
    StringTrim,
    StringTrimStart,
    StringTrimEnd,
    Allocate,
    Collect,
    RootPush,
    ArrayIndex,
    ArrayKeys,
    ArrayValues,
    ArrayPush,
    ArraySlice,
    ArrayReverse,
    ArrayFill,
    ArraySearch,
    ArrayLastIndex,
    ArrayPresent,
    ArrayStringHasOwn,
    ArraySetPresent,
    ArrayResize,
}
impl Helper {
    pub(super) fn string_search(mode: crate::ir::StringSearch) -> Self {
        match mode {
            crate::ir::StringSearch::Includes => Self::StringIncludes,
            crate::ir::StringSearch::StartsWith => Self::StringStartsWith,
            crate::ir::StringSearch::EndsWith => Self::StringEndsWith,
            crate::ir::StringSearch::IndexOf => Self::StringIndexOf,
            crate::ir::StringSearch::LastIndexOf => Self::StringLastIndexOf,
        }
    }
    pub(super) fn string_trim(mode: crate::ir::StringTrimMode) -> Self {
        match mode {
            crate::ir::StringTrimMode::Trim => Self::StringTrim,
            crate::ir::StringTrimMode::TrimStart => Self::StringTrimStart,
            crate::ir::StringTrimMode::TrimEnd => Self::StringTrimEnd,
        }
    }
    pub(super) fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        match self {
            Self::Snapshot => (vec![ValType::I32; 2], vec![ValType::I64]),
            Self::Concat => (vec![ValType::I64; 2], vec![ValType::I64]),
            Self::Compare | Self::ArrayStringHasOwn => (vec![ValType::I64; 2], vec![ValType::I32]),
            Self::Allocate => (vec![ValType::I32], vec![ValType::I32]),
            Self::Collect => (vec![], vec![]),
            Self::RootPush => (vec![ValType::I64], vec![ValType::I64]),
            Self::ArraySearch | Self::ArrayLastIndex => (
                vec![
                    ValType::I64,
                    ValType::I32,
                    ValType::I64,
                    ValType::F64,
                    ValType::I32,
                ],
                vec![ValType::I32],
            ),
            Self::StringIncludes
            | Self::StringStartsWith
            | Self::StringEndsWith
            | Self::StringIndexOf
            | Self::StringLastIndexOf => (
                vec![ValType::I64, ValType::I64, ValType::F64],
                vec![ValType::I32],
            ),
            Self::StringSlice | Self::StringSubstring | Self::ArraySlice => (
                vec![ValType::I64, ValType::F64, ValType::F64],
                vec![ValType::I64],
            ),
            Self::StringTrim | Self::StringTrimStart | Self::StringTrimEnd => {
                (vec![ValType::I64], vec![ValType::I64])
            }
            Self::ArrayKeys | Self::ArrayValues => (vec![ValType::I64], vec![ValType::I64]),
            Self::ArrayPush => (vec![ValType::I64; 2], vec![ValType::I32]),
            Self::ArrayResize => (vec![ValType::I64, ValType::I32], vec![]),
            Self::ArrayReverse => (vec![ValType::I64], vec![ValType::I64]),
            Self::ArrayFill => (
                vec![ValType::I64, ValType::I64, ValType::F64, ValType::F64],
                vec![ValType::I64],
            ),
            Self::ArrayPresent => (vec![ValType::I64, ValType::I32], vec![ValType::I32]),
            Self::ArraySetPresent => (vec![ValType::I64, ValType::I32, ValType::I32], vec![]),
            Self::ArrayIndex => (vec![ValType::I64, ValType::I32], vec![ValType::I32]),
        }
    }
    pub(super) fn needs_arena(self) -> bool {
        !matches!(
            self,
            Self::Compare
                | Self::ArrayStringHasOwn
                | Self::StringIncludes
                | Self::StringStartsWith
                | Self::StringEndsWith
                | Self::StringIndexOf
                | Self::StringLastIndexOf
        )
    }
    pub(super) fn body(
        self,
        globals: Globals,
        helpers: &BTreeMap<Self, u32>,
        trace_values: bool,
        profile: varta_common::language::LanguageProfile,
    ) -> Function {
        // Emission adds the complete allocator dependency set before indexing.
        match self {
            Self::Snapshot => snapshot(helpers[&Self::Allocate]),
            Self::Concat => concat::function(helpers[&Self::Allocate]),
            Self::Compare => compare::function(profile.utf16()),
            Self::Allocate => allocate::function(globals, helpers[&Self::Collect]),
            Self::Collect => collect::function(globals, trace_values, profile.closures()),
            Self::RootPush => roots::function(globals.roots),
            Self::ArraySearch => array_search::function(
                helpers[&Self::Compare],
                profile.sparse_arrays(),
                false,
                profile.bigint_values(),
            ),
            Self::ArrayLastIndex => array_search::function(
                helpers[&Self::Compare],
                profile.sparse_arrays(),
                true,
                profile.bigint_values(),
            ),
            Self::StringIncludes => string_search::function(crate::ir::StringSearch::Includes),
            Self::StringStartsWith => string_search::function(crate::ir::StringSearch::StartsWith),
            Self::StringEndsWith => string_search::function(crate::ir::StringSearch::EndsWith),
            Self::StringIndexOf => string_index::function(false),
            Self::StringLastIndexOf => string_index::function(true),
            Self::StringSlice | Self::StringSubstring => {
                string_range::function(helpers[&Self::Allocate], self == Self::StringSubstring)
            }
            Self::StringTrim => string_trim::function(helpers[&Self::Allocate], true, true),
            Self::StringTrimStart => string_trim::function(helpers[&Self::Allocate], true, false),
            Self::StringTrimEnd => string_trim::function(helpers[&Self::Allocate], false, true),
            Self::ArraySlice => array_slice::function(
                helpers[&Self::Allocate],
                helpers[&Self::RootPush],
                profile.sparse_arrays(),
            ),
            Self::ArrayReverse => array_reverse::function(profile.sparse_arrays()),
            Self::ArrayFill => array_fill::function(profile.sparse_arrays()),
            Self::ArrayKeys | Self::ArrayValues => array_enumeration::function(
                globals,
                helpers[&Self::Allocate],
                helpers[&Self::RootPush],
                self == Self::ArrayKeys,
                profile.sparse_arrays(),
            ),
            Self::ArrayPush => {
                array_push::function(helpers[&Self::Allocate], profile.sparse_arrays())
            }
            Self::ArrayResize => array_resize::function(helpers[&Self::Allocate]),
            Self::ArrayPresent => sparse::function(false),
            Self::ArrayStringHasOwn => array_own_key::function(helpers[&Self::ArrayPresent]),
            Self::ArraySetPresent => sparse::function(true),
            Self::ArrayIndex => array_index::function(profile.resizable_arrays()),
        }
    }
}
fn instructions(function: &mut Function, code: impl IntoIterator<Item = I<'static>>) {
    for instruction in code {
        function.instruction(&instruction);
    }
}
fn length(function: &mut Function, input: u32, output: u32) {
    instructions(
        function,
        [
            I::LocalGet(input),
            I::I64Const(LENGTH_SHIFT as i64),
            I::I64ShrU,
            I::I32WrapI64,
            I::LocalSet(output),
        ],
    );
}
fn pack(function: &mut Function, len: u32, dest: u32) {
    instructions(
        function,
        [
            I::LocalGet(dest),
            I::I64ExtendI32U,
            I::LocalGet(len),
            I::I64ExtendI32U,
            I::I64Const(LENGTH_SHIFT as i64),
            I::I64Shl,
            I::I64Or,
        ],
    );
}
fn snapshot(allocate: u32) -> Function {
    const INPUT_PTR: u32 = 0;
    const INPUT_LEN: u32 = 1;
    const DEST: u32 = 2;
    let mut function = Function::new([(1, ValType::I32)]);
    instructions(
        &mut function,
        [
            I::LocalGet(INPUT_LEN),
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::I64Const(0),
            I::Return,
            I::End,
            I::LocalGet(INPUT_LEN),
            I::Call(allocate),
            I::LocalTee(DEST),
            I::LocalGet(INPUT_PTR),
            I::LocalGet(INPUT_LEN),
            I::MemoryCopy {
                src_mem: MEMORY,
                dst_mem: MEMORY,
            },
        ],
    );
    pack(&mut function, INPUT_LEN, DEST);
    function.instruction(&I::End);
    function
}
