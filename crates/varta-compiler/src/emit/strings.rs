//! Nonmoving immutable strings with exact compiler roots and a bounded reusable heap.
mod allocate;
mod free_classes;
pub(super) use free_classes::CLASSES as FREE_CLASSES;
mod array_append;
pub(super) mod incremental;
pub(super) use array_append::{MAX_VALUES as MAX_APPENDED, MIN_CAPACITY as MIN_APPEND_CAPACITY};
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
pub(super) use concat::MAX_OPERANDS as MAX_CONCATENATED;
mod dictionary_probe;
mod index_key;
mod primitives;
pub(super) use primitives::{FastPaths, spellings};
mod readers;
mod regexp;
mod remainder;
mod roots;
mod sparse;
pub(super) use sparse::Sizes;
mod string_index;
mod string_range;
mod string_search;
mod string_trim;
mod text_codes;
mod text_units;
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
    /// First managed-heap address: `abi::OWNED_STRING_BASE`, or after the
    /// static extension of a v38 module (`abi::StaticRegion::heap_base`).
    /// Handles below it are static and never marked.
    pub base: u32,
    /// V38 (`data_limits`): the exported resource-failure global; the
    /// request global follows it.
    pub failure: Option<u32>,
    /// V38 (`codegen_v2`): the heap end up to which memory grows without a
    /// collection (`allocate`).
    pub collect_at: Option<u32>,
    /// V38 (`codegen_v2`): the first of the [`free_classes::CLASSES`] globals
    /// heading the exact-size free lists of small blocks; the i64 mask of
    /// nonempty classes follows them.
    pub classes: Option<u32>,
    /// V38 (`incremental_collection`): the first of the collector's state
    /// globals ([`incremental::State`]).
    pub collector: Option<incremental::State>,
}
impl Globals {
    /// Trap, first recording `resource` and the amount in local `amount` when
    /// this module reports resources (v38); a plain `unreachable` otherwise.
    pub(super) fn fail(
        self,
        resource: varta_common::limits::Resource,
        amount: u32,
    ) -> Vec<I<'static>> {
        let mut code = Vec::new();
        if let Some(failure) = self.failure {
            code.extend([
                I::I32Const(resource as i32),
                I::GlobalSet(failure),
                I::LocalGet(amount),
                I::GlobalSet(failure + 1),
            ]);
        }
        code.push(I::Unreachable);
        code
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Helper {
    Snapshot,
    Concat,
    /// V38 (`codegen_v2`): one concatenation of this many operands.
    ConcatMany(u8),
    /// V38 (`codegen_v2`): a builder block with doubled capacity holding
    /// both operands (`string_builder`).
    StringGrow,
    /// V38 (`codegen_v2`): content equality of two strings, 1 or 0.
    StringEquals,
    /// V38 (`codegen_v2`): `charAt` without a host call.
    CharAt,
    /// V38 (`codegen_v2`): `charCodeAt` without a host call.
    CharCodeAt,
    /// V38 (`codegen_v2`): Number `%`, exact in the guest for safe integers
    /// and delegated to the host remainder otherwise.
    FloatRemainder,
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
    /// V38 (`incremental_collection`): the idle collection export
    /// (`varta_common::collector::IDLE_EXPORT`).
    CollectIdle,
    /// V38: start an incremental cycle (`incremental::begin`).
    CollectBegin,
    /// V38: one bounded quantum of the cycle in progress.
    CollectStep,
    /// V38: shade the children of one marked block.
    CollectTrace,
    /// V38: drain the mark stack within a work budget.
    CollectMark,
    /// V38: the deletion barrier's slow path for one old reference.
    Shade,
    /// V38: the barrier before a bulk change of one object.
    ShadeObject,
    /// V38: make a block handed out while marking black.
    Blacken,
    RootPush,
    ArrayIndex,
    ArrayKeys,
    ArrayValues,
    ArrayPush,
    /// V38 (`codegen_v2`): `push` of this many evaluated slots, without an
    /// argument array (`array_append`).
    ArrayAppend(u8),
    ArraySlice,
    ArrayReverse,
    ArrayFill,
    ArraySearch,
    ArrayLastIndex,
    ArrayPresent,
    ArrayStringHasOwn,
    ArraySetPresent,
    ArrayResize,
    /// Value15 dictionaries: canonical array index of a key, or -1.
    ArrayIndexKey,
    /// Value18 dictionaries: `(keys, table, key) -> position | -1`.
    DictionaryFind,
    /// Value18 dictionaries: as `DictionaryFind`, tombstoning the slot.
    DictionaryUnlink,
    /// Value18 dictionaries: `(table, key, position) -> 0`, one placed slot.
    DictionaryPlace,
    /// Value18 dictionaries: `(keys, table) -> 0`, every dense key placed.
    DictionaryIndex,
    /// Value18 dictionaries: `(keys, values, table, position) -> 0`, the
    /// appended entry rotated to an own-key position.
    DictionaryInsertAt,
    /// V35 Web preludes: index of the first code unit of a class, or -1.
    UnitIndex(crate::ir::UnitClass),
    /// V35 Web preludes: ASCII-lowercased copy.
    AsciiLowercase,
    /// V37 text prelude: ASCII-uppercased copy.
    AsciiUppercase,
    /// V37 preludes: the code unit at an index, or NaN.
    CodeUnit,
    /// V37 preludes: a fresh string of an array's code units.
    FromUnits,
    /// V38 view prelude: a fresh string of this many code units, passed as
    /// numbers.
    Units(u8),
    /// V35 Web preludes: copy without leading/trailing HTTP whitespace.
    TrimHttpWhitespace,
    /// V37 RegExp prelude: one matcher run over a compiled program.
    RegExpExec,
    /// V37 RegExp prelude: one capture slot of a matcher result.
    RegExpSlot,
    /// V37 RegExp prelude: one code unit without a host call, or -1.
    RegExpUnit,
    /// V38 (`data_limits`): register, find or release a live continuation
    /// frame in the growable hashed live-frame table.
    FrameTable,
    /// V38 (`linked_intrinsics`): `String` of a primitive cell, inline for
    /// strings, literals and safe integers, else the linked conversion:
    /// `primitive_to_string` (no BigInt code) for Numbers and, when the cell
    /// may hold a BigInt (`true`), `primitive_to_string_v6` for BigInts.
    PrimitiveText(bool),
    /// V38: `Number` of a primitive cell, inline except strings
    /// (`primitive_to_number`) and BigInts (`primitive_to_number_v6`, when
    /// the cell may hold one).
    PrimitiveNumber(bool),
    /// V38: `String(n)` of a Number that is boxed nowhere: inline for safe
    /// integers, else boxed for `primitive_to_string`.
    NumberText,
    /// V38: `Number(s)` of a string that is boxed nowhere: inline for digit
    /// strings, else boxed for `primitive_to_number`.
    StringNumber,
    /// V38: `primitive_compare` with inline fast paths for the kinds its
    /// cells may hold.
    PrimitiveOrder(Cells),
    /// V38: `primitive_equal` with inline fast paths for the kinds its cells
    /// may hold.
    PrimitiveEquals(Cells),
    /// V38: the value18 readers answered from the cell's tag, the linked
    /// reader behind them (`readers`).
    ValueTypeOf,
    ValueTruthy,
    ValueNullish,
    ValueIsArray,
    ValueStrictEqual,
    ValueNullishEqual,
    ValueIsErrorClass,
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
    pub(super) fn text_units(op: crate::ir::TextUnits) -> Self {
        match op {
            crate::ir::TextUnits::Index(class) => Self::UnitIndex(class),
            crate::ir::TextUnits::AsciiLowercase => Self::AsciiLowercase,
            crate::ir::TextUnits::AsciiUppercase => Self::AsciiUppercase,
            crate::ir::TextUnits::TrimHttpWhitespace => Self::TrimHttpWhitespace,
        }
    }
    pub(super) fn regexp(op: crate::ir::RegExpIntrinsic) -> Self {
        match op {
            crate::ir::RegExpIntrinsic::Exec => Self::RegExpExec,
            crate::ir::RegExpIntrinsic::Slot => Self::RegExpSlot,
            crate::ir::RegExpIntrinsic::Unit => Self::RegExpUnit,
        }
    }
    /// The content-equality helper of array searches: v38 uses
    /// [`Self::StringEquals`], earlier profiles the ordering comparison.
    pub(super) fn equality(profile: varta_common::language::LanguageProfile) -> Self {
        if crate::codegen_v2(profile) {
            Self::StringEquals
        } else {
            Self::Compare
        }
    }
    pub(super) fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        match self {
            Self::RegExpExec => (
                vec![ValType::I64, ValType::I64, ValType::F64, ValType::I32],
                vec![ValType::I64],
            ),
            Self::RegExpSlot | Self::RegExpUnit => {
                (vec![ValType::I64, ValType::F64], vec![ValType::F64])
            }
            Self::Snapshot => (vec![ValType::I32; 2], vec![ValType::I64]),
            Self::Concat | Self::StringGrow => (vec![ValType::I64; 2], vec![ValType::I64]),
            Self::ConcatMany(count) => (vec![ValType::I64; usize::from(count)], vec![ValType::I64]),
            Self::StringEquals => (vec![ValType::I64; 2], vec![ValType::I32]),
            Self::DictionaryFind | Self::DictionaryUnlink => {
                (vec![ValType::I64; 3], vec![ValType::F64])
            }
            Self::DictionaryPlace => (
                vec![ValType::I64, ValType::I64, ValType::F64],
                vec![ValType::F64],
            ),
            Self::DictionaryIndex => (vec![ValType::I64; 2], vec![ValType::F64]),
            Self::DictionaryInsertAt => (
                vec![ValType::I64, ValType::I64, ValType::I64, ValType::F64],
                vec![ValType::F64],
            ),
            Self::Compare | Self::ArrayStringHasOwn => (vec![ValType::I64; 2], vec![ValType::I32]),
            Self::Allocate => (vec![ValType::I32], vec![ValType::I32]),
            Self::ArrayIndexKey | Self::UnitIndex(_) => (vec![ValType::I64], vec![ValType::F64]),
            Self::CodeUnit | Self::CharCodeAt => {
                (vec![ValType::I64, ValType::F64], vec![ValType::F64])
            }
            Self::CharAt => (vec![ValType::I64, ValType::F64], vec![ValType::I64]),
            Self::FloatRemainder => (vec![ValType::F64; 2], vec![ValType::F64]),
            Self::FromUnits => (vec![ValType::I64], vec![ValType::I64]),
            Self::Units(count) => (vec![ValType::F64; usize::from(count)], vec![ValType::I64]),
            Self::Collect => (vec![], vec![]),
            Self::CollectIdle | Self::CollectStep => (vec![ValType::I32], vec![ValType::I32]),
            Self::CollectBegin => (vec![], vec![ValType::I32]),
            Self::CollectMark => (vec![ValType::I32], vec![ValType::I32]),
            Self::CollectTrace => (vec![ValType::I32; 3], vec![ValType::I32]),
            Self::Shade => (vec![ValType::I64], vec![]),
            Self::ShadeObject | Self::Blacken => (vec![ValType::I32], vec![]),
            Self::FrameTable => (vec![ValType::I64, ValType::I32], vec![ValType::I32]),
            Self::PrimitiveText(_) => (vec![ValType::I64], vec![ValType::I64]),
            Self::NumberText => (vec![ValType::F64], vec![ValType::I64]),
            Self::StringNumber => (vec![ValType::I64], vec![ValType::F64]),
            Self::PrimitiveNumber(_) => (vec![ValType::I64], vec![ValType::F64]),
            Self::PrimitiveOrder(_) => (vec![ValType::I64; 2], vec![ValType::F64]),
            Self::PrimitiveEquals(_) => (
                vec![ValType::I32, ValType::I64, ValType::I64],
                vec![ValType::I32],
            ),
            Self::ValueTypeOf => (vec![ValType::I64], vec![ValType::I64]),
            Self::ValueTruthy | Self::ValueNullish | Self::ValueIsArray => {
                (vec![ValType::I64], vec![ValType::I32])
            }
            Self::ValueStrictEqual | Self::ValueNullishEqual => {
                (vec![ValType::I64; 2], vec![ValType::I32])
            }
            Self::ValueIsErrorClass => (vec![ValType::I64, ValType::I32], vec![ValType::I32]),
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
            Self::StringTrim
            | Self::StringTrimStart
            | Self::StringTrimEnd
            | Self::AsciiLowercase
            | Self::AsciiUppercase
            | Self::TrimHttpWhitespace => (vec![ValType::I64], vec![ValType::I64]),
            Self::ArrayKeys | Self::ArrayValues => (vec![ValType::I64], vec![ValType::I64]),
            Self::ArrayPush => (vec![ValType::I64; 2], vec![ValType::I32]),
            Self::ArrayAppend(count) => (
                vec![ValType::I64; 1 + usize::from(count)],
                vec![ValType::I32],
            ),
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
    /// The collector helpers of a v38 module (`incremental_collection`).
    fn collector_calls(helpers: &BTreeMap<Self, u32>) -> Option<incremental::Calls> {
        Some(incremental::Calls {
            collect: *helpers.get(&Self::Collect)?,
            begin: *helpers.get(&Self::CollectBegin)?,
            step: *helpers.get(&Self::CollectStep)?,
            trace: *helpers.get(&Self::CollectTrace)?,
            shade: *helpers.get(&Self::Shade)?,
            blacken: *helpers.get(&Self::Blacken)?,
        })
    }
    /// As [`Self::collector_calls`] for a collector helper, which exists only
    /// with the others.
    fn calls(helpers: &BTreeMap<Self, u32>) -> incremental::Calls {
        Self::collector_calls(helpers).unwrap_or(incremental::Calls {
            collect: 0,
            begin: 0,
            step: 0,
            trace: 0,
            shade: 0,
            blacken: 0,
        })
    }
    /// The write barrier of a v38 module (`incremental_collection`).
    pub(super) fn barrier(
        globals: Option<Globals>,
        helpers: &BTreeMap<Self, u32>,
    ) -> Option<incremental::Barrier> {
        let state = globals?.collector?;
        Some(incremental::Barrier {
            marking: state.global(incremental::Slot::Marking),
            shade: *helpers.get(&Self::Shade)?,
            shade_object: *helpers.get(&Self::ShadeObject)?,
        })
    }
    pub(super) fn needs_arena(self) -> bool {
        !matches!(
            self,
            Self::Compare
                | Self::StringEquals
                | Self::ArrayStringHasOwn
                | Self::ArrayIndexKey
                | Self::DictionaryFind
                | Self::DictionaryUnlink
                | Self::DictionaryPlace
                | Self::DictionaryIndex
                | Self::DictionaryInsertAt
                | Self::UnitIndex(_)
                | Self::RegExpSlot
                | Self::RegExpUnit
                | Self::CodeUnit
                | Self::CharCodeAt
                | Self::FloatRemainder
                | Self::PrimitiveNumber(_)
                | Self::PrimitiveOrder(_)
                | Self::PrimitiveEquals(_)
                | Self::ValueTruthy
                | Self::ValueNullish
                | Self::ValueIsArray
                | Self::ValueStrictEqual
                | Self::ValueNullishEqual
                | Self::ValueIsErrorClass
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
        frames: Option<super::frames::Resources>,
        linked: Linked,
    ) -> Function {
        let Linked {
            ascii,
            remainder,
            fast_paths,
        } = linked;
        let tracing = incremental::Tracing {
            closures: profile.closures(),
            frames: frames.is_some(),
            wide: profile.wide_records(),
            limits: profile.data(),
        };
        // Emission adds the complete allocator dependency set before indexing.
        match self {
            Self::Snapshot => snapshot(helpers[&Self::Allocate]),
            Self::Concat => concat::function(helpers[&Self::Allocate]),
            Self::ConcatMany(count) => {
                concat::many(count, helpers[&Self::Allocate], globals, profile.data())
            }
            Self::StringGrow => concat::grow(helpers[&Self::Allocate]),
            Self::StringEquals => compare::equals(profile.utf16()),
            Self::Compare => compare::function(profile.utf16(), crate::codegen_v2(profile)),
            Self::Allocate => allocate::function(
                globals,
                helpers[&Self::Collect],
                profile.data(),
                Self::collector_calls(helpers),
            ),
            Self::Collect => collect::function(
                globals,
                trace_values,
                profile.closures(),
                frames.is_some(),
                profile.wide_records(),
                profile.data(),
            ),
            Self::CollectIdle => incremental::idle(globals, Self::calls(helpers)),
            Self::CollectBegin => incremental::begin(globals, Self::calls(helpers)),
            Self::CollectStep => incremental::step(
                globals,
                Self::calls(helpers),
                helpers.get(&Self::CollectMark).copied().unwrap_or(0),
            ),
            Self::CollectTrace => incremental::trace(globals, tracing),
            Self::CollectMark => incremental::mark(globals, tracing),
            Self::Shade => incremental::shade(globals),
            Self::ShadeObject => incremental::shade_object(globals, helpers[&Self::CollectTrace]),
            Self::Blacken => incremental::blacken(globals),
            Self::FrameTable => super::frames::table_helper(
                globals,
                frames,
                helpers[&Self::Allocate],
                helpers[&Self::RootPush],
                profile.data(),
                Self::barrier(Some(globals), helpers),
            ),
            Self::RootPush => roots::function(globals),
            Self::ArraySearch => array_search::function(
                helpers[&Self::equality(profile)],
                crate::codegen_v2(profile),
                profile.sparse_arrays(),
                false,
                profile.bigint_values(),
                sparse::Sizes::new(profile.data()),
            ),
            Self::ArrayLastIndex => array_search::function(
                helpers[&Self::equality(profile)],
                crate::codegen_v2(profile),
                profile.sparse_arrays(),
                true,
                profile.bigint_values(),
                sparse::Sizes::new(profile.data()),
            ),
            Self::StringIncludes => string_search::function(crate::ir::StringSearch::Includes),
            Self::StringStartsWith => string_search::function(crate::ir::StringSearch::StartsWith),
            Self::StringEndsWith => string_search::function(crate::ir::StringSearch::EndsWith),
            Self::StringIndexOf => string_index::function(false),
            Self::StringLastIndexOf => string_index::function(true),
            Self::StringSlice | Self::StringSubstring => {
                string_range::function(helpers[&Self::Allocate], self == Self::StringSubstring)
            }
            Self::StringTrim => {
                string_trim::function(helpers[&Self::Allocate], true, true, Whitespace::Script)
            }
            Self::StringTrimStart => {
                string_trim::function(helpers[&Self::Allocate], true, false, Whitespace::Script)
            }
            Self::StringTrimEnd => {
                string_trim::function(helpers[&Self::Allocate], false, true, Whitespace::Script)
            }
            Self::TrimHttpWhitespace => {
                string_trim::function(helpers[&Self::Allocate], true, true, Whitespace::Http)
            }
            Self::UnitIndex(class) => text_units::index(class),
            Self::AsciiLowercase => text_units::ascii_case(helpers[&Self::Allocate], false),
            Self::AsciiUppercase => text_units::ascii_case(helpers[&Self::Allocate], true),
            Self::CodeUnit => text_codes::code_unit(),
            Self::CharCodeAt => text_codes::char_code_at(),
            Self::FloatRemainder => remainder::function(remainder.unwrap_or(0)),
            Self::CharAt => text_codes::char_at(ascii.unwrap_or(0), helpers[&Self::Allocate]),
            Self::FromUnits => {
                text_codes::from_units(helpers[&Self::Allocate], profile.resizable_arrays())
            }
            Self::Units(count) => text_codes::units(helpers[&Self::Allocate], u32::from(count)),
            Self::RegExpExec => regexp::exec(helpers[&Self::Allocate]),
            Self::RegExpSlot => regexp::slot(),
            Self::RegExpUnit => regexp::unit(),
            Self::ArraySlice => array_slice::function(
                helpers[&Self::Allocate],
                helpers[&Self::RootPush],
                profile.sparse_arrays(),
                sparse::Sizes::new(profile.data()),
            ),
            Self::ArrayReverse => array_reverse::function(
                profile.sparse_arrays(),
                sparse::Sizes::new(profile.data()),
                Self::barrier(Some(globals), helpers),
            ),
            Self::ArrayFill => array_fill::function(
                profile.sparse_arrays(),
                sparse::Sizes::new(profile.data()),
                Self::barrier(Some(globals), helpers),
            ),
            Self::ArrayKeys | Self::ArrayValues => array_enumeration::function(
                globals,
                helpers[&Self::Allocate],
                helpers[&Self::RootPush],
                self == Self::ArrayKeys,
                profile.sparse_arrays(),
                profile.data(),
            ),
            Self::ArrayPush => array_push::function(
                helpers[&Self::Allocate],
                profile.sparse_arrays(),
                profile.data(),
                Self::barrier(Some(globals), helpers),
            ),
            Self::ArrayAppend(count) => array_append::function(
                count,
                helpers[&Self::Allocate],
                profile.sparse_arrays(),
                profile.data(),
                Self::barrier(Some(globals), helpers),
            ),
            Self::ArrayResize => array_resize::function(
                helpers[&Self::Allocate],
                profile.data(),
                Self::barrier(Some(globals), helpers),
            ),
            Self::ArrayPresent => sparse::function(false, profile.data()),
            Self::ArrayStringHasOwn => {
                array_own_key::function(helpers[&Self::ArrayPresent], profile.data())
            }
            Self::ArraySetPresent => sparse::function(true, profile.data()),
            Self::ArrayIndex => array_index::function(profile.resizable_arrays()),
            Self::ArrayIndexKey => index_key::function(),
            Self::DictionaryFind => dictionary_probe::find(helpers[&Self::StringEquals], false),
            Self::DictionaryUnlink => dictionary_probe::find(helpers[&Self::StringEquals], true),
            Self::DictionaryPlace => dictionary_probe::place(),
            Self::DictionaryIndex => dictionary_probe::index(),
            Self::DictionaryInsertAt => {
                dictionary_probe::insert_at(Self::barrier(Some(globals), helpers))
            }
            Self::PrimitiveText(bigint) => {
                primitives::text(helpers[&Self::Allocate], fast_paths, bigint)
            }
            Self::PrimitiveNumber(bigint) => primitives::number(fast_paths, bigint),
            Self::NumberText => primitives::number_text(helpers[&Self::Allocate], fast_paths),
            Self::StringNumber => primitives::string_number(
                helpers[&Self::Allocate],
                helpers[&Self::RootPush],
                globals.roots,
                fast_paths,
            ),
            Self::PrimitiveOrder(cells) => {
                primitives::order(helpers[&Self::Compare], fast_paths, cells)
            }
            Self::PrimitiveEquals(cells) => {
                primitives::equal(helpers[&Self::StringEquals], fast_paths, cells)
            }
            Self::ValueTypeOf => readers::type_of(fast_paths),
            Self::ValueTruthy => readers::truthy(fast_paths),
            Self::ValueNullish => readers::kind_test(fast_paths, true),
            Self::ValueIsArray => readers::kind_test(fast_paths, false),
            Self::ValueStrictEqual => {
                readers::equal(helpers.get(&Self::StringEquals).copied(), fast_paths, true)
            }
            Self::ValueNullishEqual => readers::equal(None, fast_paths, false),
            Self::ValueIsErrorClass => readers::is_error_class(fast_paths),
        }
    }
}
/// The kinds the cells of a v38 comparison may hold beyond the Number-like
/// ones: what its fast-path helper must handle and link.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Cells {
    pub bigint: bool,
    pub string: bool,
}
/// V38 (`codegen_v2`) targets some helpers embed: the static ASCII unit table
/// of `charAt`, the host remainder import of `%` and the linked primitive
/// conversions and comparisons with their static spellings.
#[derive(Clone, Copy)]
pub(super) struct Linked {
    pub ascii: Option<u32>,
    pub remainder: Option<u32>,
    pub fast_paths: FastPaths,
}
impl Helper {
    /// The fast-path helper of a linked primitive conversion or comparison
    /// or value18 reader (v38) with operands `args`, if `host` is one.
    pub(super) fn fast_path(
        profile: varta_common::language::LanguageProfile,
        host: varta_common::HostOperation,
        args: &[crate::ir::Expr],
        unions: &[Vec<crate::ir::Type>],
    ) -> Option<Self> {
        use varta_common::HostOperation as Host;
        if !crate::codegen_v2(profile) || !profile.links(host) {
            return None;
        }
        // Whether a cell operand's union holds `kind` (v38 lowering admits
        // BigInt, and in comparisons any kind, only where a source may hold
        // it).
        let holds = |kind: crate::ir::Type| {
            args.iter().any(|arg| match arg.ty {
                crate::ir::Type::Union(id) | crate::ir::Type::ReferenceUnion(id) => unions
                    .get(id as usize)
                    .is_none_or(|members| members.contains(&kind)),
                other => other == kind,
            })
        };
        let bigint = || holds(crate::ir::Type::BigInt);
        let cells = || Cells {
            bigint: bigint(),
            string: holds(crate::ir::Type::String),
        };
        // A boxed Number (to text) or string (to a Number) needs no cell.
        let boxed = |ty: crate::ir::Type| {
            args.first().is_some_and(
                |arg| matches!(&arg.kind, crate::ir::Expression::BoxUnion(inner) if inner.ty == ty),
            )
        };
        Some(match host {
            Host::PrimitiveStringV6 if boxed(crate::ir::Type::Float64) => Self::NumberText,
            Host::PrimitiveNumberV6 if boxed(crate::ir::Type::String) => Self::StringNumber,
            Host::PrimitiveStringV6 => Self::PrimitiveText(bigint()),
            Host::PrimitiveNumberV6 => Self::PrimitiveNumber(bigint()),
            Host::PrimitiveCompare => Self::PrimitiveOrder(cells()),
            Host::PrimitiveEqual => Self::PrimitiveEquals(cells()),
            Host::ValueTypeOfV11 => Self::ValueTypeOf,
            Host::ValueTruthyV11 => Self::ValueTruthy,
            Host::ValueNullishV11 => Self::ValueNullish,
            Host::ValueIsArrayV11 => Self::ValueIsArray,
            Host::ValueStrictEqualV11 => Self::ValueStrictEqual,
            Host::ValueNullishEqualV11 => Self::ValueNullishEqual,
            Host::ValueIsErrorClassV7 => Self::ValueIsErrorClass,
            _ => return None,
        })
    }
    /// The helpers a fast-path helper calls.
    pub(super) fn fast_path_dependencies(self) -> &'static [Self] {
        match self {
            // The linked conversions return owned strings (`link::owned_text`).
            Self::PrimitiveText(_) | Self::NumberText | Self::ValueTypeOf => &[Self::Allocate],
            Self::StringNumber => &[Self::Allocate, Self::RootPush],
            Self::PrimitiveOrder(_) => &[Self::Compare],
            Self::PrimitiveEquals(_) | Self::ValueStrictEqual => &[Self::StringEquals],
            _ => &[],
        }
    }
    /// Whether the helper returns static spellings.
    pub(super) fn spells(self) -> bool {
        matches!(
            self,
            Self::PrimitiveText(_) | Self::NumberText | Self::ValueTypeOf
        )
    }
    /// The linked operations a fast-path helper calls, which the module
    /// imports (and linking then replaces) instead of `host`.
    pub(super) fn fast_path_hosts(
        self,
        host: varta_common::HostOperation,
    ) -> Vec<varta_common::HostOperation> {
        use varta_common::HostOperation as Host;
        match self {
            Self::NumberText => vec![Host::PrimitiveString],
            Self::StringNumber => vec![Host::PrimitiveNumber],
            Self::PrimitiveText(bigint) => {
                let mut hosts = vec![Host::PrimitiveString];
                hosts.extend(bigint.then_some(host));
                hosts
            }
            Self::PrimitiveNumber(bigint) => {
                let mut hosts = vec![Host::PrimitiveNumber];
                hosts.extend(bigint.then_some(host));
                hosts
            }
            // Strings compared with Numbers are parsed by the BigInt-free
            // conversion; the comparison itself is linked for BigInts.
            Self::PrimitiveOrder(cells) | Self::PrimitiveEquals(cells) => {
                let mut hosts = Vec::new();
                hosts.extend(cells.string.then_some(Host::PrimitiveNumber));
                hosts.extend(cells.bigint.then_some(host));
                hosts
            }
            _ => vec![host],
        }
    }
}
/// The code units a trimming helper removes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Whitespace {
    /// ECMAScript WhiteSpace and LineTerminator (`String.prototype.trim`).
    Script,
    /// HTTP whitespace: TAB, LF, CR and SP (Fetch header value normalization).
    Http,
}
/// The UTF-16 code units 0..128 in static data: v38 `charAt` of an ASCII
/// unit returns a static one-unit slice of this table.
pub(super) fn ascii_units() -> Vec<u8> {
    (0..text_codes::ASCII_UNITS)
        .flat_map(u16::to_le_bytes)
        .collect()
}
/// The handle of the owned string a linked v38 operation returned (its
/// pointer on the stack, its byte length in the `result_len` global): packed
/// in place, with no copy (`link::owned_text`).
pub(super) fn owned_text(result_len: u32) -> [I<'static>; 6] {
    [
        I::I64ExtendI32U,
        I::GlobalGet(result_len),
        I::I64ExtendI32U,
        I::I64Const(LENGTH_SHIFT as i64),
        I::I64Shl,
        I::I64Or,
    ]
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
