use swc_ecma_ast::BinaryOp;

/// Private direct-JSON attempt miss: structured successes always allocate a
/// nonzero reference. It is consumed before an application value is published.
pub(crate) const JSON_DECODE_MISS: i64 = 0;

pub(crate) fn is_number_bitwise(op: BinaryOp) -> bool {
    matches!(
        op,
        BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
            | BinaryOp::LShift
            | BinaryOp::RShift
            | BinaryOp::ZeroFillRShift
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Type {
    BigInt,
    /// ECMAScript `symbol` (v38 `symbol_values`, K9 D2): physically the
    /// handle of a private record of the symbol prelude class
    /// (`ValueTypes::symbol`), whose identity is the symbol's; erased to that
    /// class before emission, where it carries the value18 symbol tag.
    Symbol,
    Function(u32),
    Number,
    Float64,
    Boolean,
    String,
    Void,
    Error,
    NativeError(varta_common::value::ErrorClass),
    ResultNumber,
    ResultFloat64,
    ResultBoolean,
    ResultString,
    ResultVoid,
    Null,
    Undefined,
    Record(u32),
    Array(u32),
    Union(u32),
    ReferenceUnion(u32),
    /// Compile-time literal set (v32 `literal_types`): physically its base
    /// primitive; the id indexes `ValueTypes::literals`. Erased before emission.
    Literal(LiteralBase, u32),
    /// Nominal class instance (v32): a record block whose static descriptor
    /// identifies the most-derived class; see `lower::classes`.
    Class(u32),
    /// Synchronous generator object (v33 `generators`): the handle of its
    /// continuation frame; the id indexes `ValueTypes::generators`.
    Generator(u32),
    /// Value15 dynamic-key record (v34 `index_signatures`): physically the
    /// two-slot storage record `ValueTypes::dictionaries[id].storage`.
    Dictionary(u32),
    /// `unknown` (v34 `unknown_type`): a reference-union cell whose member may
    /// be any admitted value kind; only checked guards and assertions extract it.
    Unknown,
    /// TypeScript's `object` (v35 `unknown_json`): an `unknown` cell that a
    /// `typeof x === "object"` guard proved to hold a non-primitive value, or
    /// (`true`) that value or `null`. Physically the same cell as `unknown`;
    /// dictionary operations on it test the cell first.
    Object(bool),
    /// Promise object (v34 `promises`): a guest-heap record with private
    /// fields and no value tag; the id indexes `ValueTypes::promises`.
    Promise(u32),
    /// TypeScript's `never` (v37 `language_core`): the type of a binding whose
    /// narrowing excluded every member, or of a `never` annotation. No value
    /// has it; code that would observe one is unreachable and traps.
    Never,
    /// Engine collection object (v37 `collections`): `Map`, `Set` or
    /// `Uint8Array`, physically its private storage
    /// (`ValueTypes::collections[id].storage`), with no value tag of its own.
    /// Erased to the storage before emission; see `lower::collections`.
    Collection(CollectionKind, u32),
    /// Tuple type (v37 `collections`): physically the dense array
    /// `ValueTypes::tuples[id].array` of the union of its element types.
    /// Erased to that array before emission; see `lower::tuples`.
    Tuple(u32),
    /// A class object (v38 `declaration_forms`, `typeof C`): the identity of
    /// the class `C` or of one of its subclasses, the class id. Physically an
    /// `i32` (erased to `Number` before emission); `new`, static calls and
    /// `name` dispatch over the closed set of subclasses. It never enters a
    /// union, an aggregate or `unknown`; see `lower::class_values`.
    ClassValue(u32),
}

/// The engine collection families of v37 (`Type::Collection`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CollectionKind {
    Map,
    Set,
    Uint8Array,
}
impl CollectionKind {
    pub(crate) const ALL: [Self; 3] = [Self::Map, Self::Set, Self::Uint8Array];
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Map => "Map",
            Self::Set => "Set",
            Self::Uint8Array => "Uint8Array",
        }
    }
}
/// One instantiated engine collection type (v37 `collections`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CollectionType {
    pub kind: CollectionKind,
    /// The key type (`Set` elements; `number` for `Uint8Array`).
    pub key: Type,
    /// The value type (`Set`: its element type; `number` for `Uint8Array`).
    pub value: Type,
    /// The physical storage: the prelude record (`MapStore`/`SetStore`) or,
    /// for `Uint8Array`, a dense `number[]` of byte values.
    pub storage: Type,
}
/// One tuple type (v37 `collections`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TupleType {
    /// Element types in position order.
    pub elements: Vec<Type>,
    /// Trailing optional elements start at this position.
    pub required: usize,
    /// A rest element `[A, ...B[]]` (v38 `type_level`): positions after
    /// `elements` hold values of this type.
    pub rest: Option<Type>,
    /// Required elements after the rest element (`[...A[], B]`, v38
    /// `type_level`): the last `trailing.len()` positions.
    pub trailing: Vec<Type>,
    /// `readonly [..]`: writes through the tuple are rejected.
    pub readonly: bool,
    /// The physical dense array type.
    pub array: Type,
}

/// Base primitive of a literal set; it fixes the erased physical representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LiteralBase {
    String,
    Number,
    Boolean,
}
impl LiteralBase {
    pub(crate) fn erased(self) -> Type {
        match self {
            Self::String => Type::String,
            Self::Number => Type::Float64,
            Self::Boolean => Type::Boolean,
        }
    }
}

impl Type {
    /// The physical type: a literal set erases to its base primitive.
    pub(crate) fn erased(self) -> Self {
        if let Self::Literal(base, _) = self {
            base.erased()
        } else {
            self
        }
    }
    pub(crate) fn is_handle(self) -> bool {
        if let Self::Literal(base, _) = self {
            return base.erased().is_handle();
        }
        matches!(
            self,
            Self::BigInt
                | Self::Symbol
                | Self::Collection(..)
                | Self::Tuple(_)
                | Self::Dictionary(_)
                | Self::Class(_)
                | Self::Generator(_)
                | Self::Promise(_)
                | Self::Function(_)
                | Self::Error
                | Self::NativeError(_)
                | Self::String
                | Self::Record(_)
                | Self::Array(_)
                | Self::Union(_)
                | Self::ReferenceUnion(_)
                | Self::Unknown
                | Self::Object(_)
        )
    }
    /// A managed union cell: a closed union, `unknown` (v34) or `object` (v35).
    pub(crate) fn is_cell(self) -> bool {
        self.is_union() || matches!(self, Self::Unknown | Self::Object(_))
    }
    pub(crate) fn from_error_class(class: varta_common::value::ErrorClass) -> Self {
        if class == varta_common::value::ErrorClass::Error {
            Self::Error
        } else {
            Self::NativeError(class)
        }
    }
    pub(crate) fn error_class(self) -> Option<varta_common::value::ErrorClass> {
        match self {
            Self::Error => Some(varta_common::value::ErrorClass::Error),
            Self::NativeError(class) => Some(class),
            _ => None,
        }
    }
    pub(crate) fn is_error(self) -> bool {
        self.error_class().is_some()
    }
    pub(crate) fn is_union(self) -> bool {
        matches!(self, Self::Union(_) | Self::ReferenceUnion(_))
    }
    pub(crate) fn needs_root(self) -> bool {
        self.is_handle() || self == Self::ResultString
    }
    pub(crate) fn captured(self) -> Option<Self> {
        Some(match self {
            Self::Number => Self::ResultNumber,
            Self::Float64 => Self::ResultFloat64,
            Self::Boolean => Self::ResultBoolean,
            Self::String => Self::ResultString,
            Self::Void => Self::ResultVoid,
            _ => return None,
        })
    }
    pub(crate) fn result_value(self) -> Option<Self> {
        Some(match self {
            Self::ResultNumber => Self::Number,
            Self::ResultFloat64 => Self::Float64,
            Self::ResultBoolean => Self::Boolean,
            Self::ResultString => Self::String,
            Self::ResultVoid => Self::Void,
            _ => return None,
        })
    }
    pub(crate) fn width(self) -> u32 {
        if matches!(self, Self::Void | Self::Never) {
            0
        } else if self.result_value().is_some() {
            2
        } else {
            1
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ResultField {
    Ok,
    Status,
    Value,
}

pub(crate) struct Program {
    pub functions: Vec<Function>,
    pub main: usize,
    pub tools: Vec<(usize, varta_common::ToolContract)>,
    pub http: Option<(usize, varta_common::http::HttpContract)>,
    /// Pure state/value accessors of an async HTTP response promise.
    pub http_async: Option<(usize, usize)>,
    /// A v2 streaming service's `pull` and its chunk promise's pure
    /// state/value accessors.
    pub http_stream: Option<[usize; 3]>,
    /// V38 view protocol: the view prelude's event dispatcher, called by the
    /// synthesized `varta.view_abi.event` export with the copied record.
    pub view_event: Option<usize>,
    pub types: ValueTypes,
    /// Stored module constants (v32): permanent root slots, initialized by the
    /// `initializers` (dependency order) before `main` runs.
    pub constants: u32,
    pub initializers: Vec<usize>,
    /// Functions containing compiler-internal suspension points (K4.4 foundation).
    pub suspendable: std::collections::BTreeMap<usize, Suspendable>,
}

/// State-machine metadata produced by `lower::suspension` for one function.
pub(crate) struct Suspendable {
    /// Hidden i32 local: target state on entry, zero while running normally.
    pub state: u32,
    /// Hidden i32 local: the delivered `frame::Completion` kind.
    pub completion: u32,
    /// Hidden locals receiving the scalar resume input, one per admitted type.
    pub inputs: std::collections::BTreeMap<Type, u32>,
    /// Suspension points in state order; state `n` is `points[n - 1]`.
    pub points: Vec<SuspensionPoint>,
    /// The exception-channel union stored for an uncaught completion.
    pub exception: Type,
    /// The admissible `Return` input; `None` rejects `Return` resumption.
    pub return_input: Option<Type>,
    /// Generator type id of a `function*` (v33): its frame is owned by the
    /// generator object and resumed only by guest code, never by `varta_resume`.
    pub generator: Option<u32>,
    /// The body of an async generator (v36): the frame's completion cell
    /// holds the state record from the first suspension on and is never
    /// cleared, so later suspensions do not publish it again.
    pub keeps_completion: bool,
    /// A scheduled frame (v34 `promises`/`async_functions`): every suspension
    /// awaits a promise and the runtime scheduler resumes it through
    /// `varta_resume`; the suspension point issues the frame's generation.
    pub scheduled: bool,
    /// Liveness layout (v35 `frame_liveness`): the locals saved at state `n`
    /// are `live[n - 1]`, sorted. `None` keeps the cell layout that saves
    /// every local at every point (v33 and v34, and the test hook before v35).
    pub live: Option<Vec<Vec<u32>>>,
}
#[derive(Clone, Copy)]
pub(crate) struct SuspensionPoint {
    pub input: Type,
    pub yielded: Type,
}

#[derive(Default)]
pub(crate) struct ValueTypes {
    pub profile: varta_common::language::LanguageProfile,
    /// Class-name holes a class scan resolves through their types (K8);
    /// `None` for an ordinary compilation.
    pub class_holes: Option<crate::classes::HoleTracker>,
    pub records: Vec<RecordShape>,
    pub arrays: Vec<Type>,
    pub unions: Vec<Vec<Type>>,
    pub functions: Vec<FunctionType>,
    pub literals: crate::lower::literals::LiteralTypes,
    /// V38 compiler-only checked-assertion decode attempts. Their entries
    /// return a zero handle on a miss; the surrounding value block runs the
    /// ordinary unknown parse/conversion. They have no exported schema.
    pub direct_json_targets: std::collections::BTreeSet<Type>,
    /// Semantic children of reachable direct-decode types, saved before
    /// literal erasure. Physical record names/layouts keep their own registry.
    pub direct_json_members: std::collections::BTreeMap<Type, Vec<Type>>,
    pub named: crate::lower::named::NamedTypes,
    pub classes: crate::lower::classes::Classes,
    pub generics: crate::lower::generics::Generics,
    pub generators: Vec<GeneratorType>,
    pub dictionaries: Vec<DictionaryType>,
    /// Declared members of mixed index records (v35 `mixed_index_records`),
    /// indexed by `DictionaryType::declared`.
    pub mixed: Vec<MixedMembers>,
    /// Shared dictionary operation functions (v35), by operation and plain
    /// dictionary type id; see `lower::dictionary_helpers`.
    pub dictionary_helpers: std::collections::BTreeMap<(DictionaryHelper, u32), usize>,
    /// Checked declared-field conversion functions (v36), by kind and target
    /// type; see `lower::declared_fields`.
    pub converters: std::collections::BTreeMap<(Converter, Type), usize>,
    pub promises: Vec<PromiseType>,
    /// Built-in functions referenced as values (v36 `function_fields`): the
    /// synthesized wrapper of each, by name; see `lower::function_fields`.
    pub builtin_functions: std::collections::BTreeMap<&'static str, usize>,
    /// Promise lowering state (v34): synthesized helpers and scheduled frames.
    pub asynchrony: crate::lower::promises::Asynchrony,
    /// Engine collection types (v37), interned by kind and type arguments.
    pub collections: Vec<CollectionType>,
    /// Tuple types (v37), interned.
    pub tuples: Vec<TupleType>,
    /// Loaded engine prelude modules and their functions (v37).
    pub engine: crate::lower::engine::EngineModules,
    /// V38 (`symbol_values`): the program's use of symbols.
    pub symbols: crate::lower::symbols::Symbols,
    /// Structural views admitted by width subtyping (v38).
    pub subtyping: crate::lower::subtyping::Views,
    /// Property keys of `const` bindings initialized by a string or number
    /// literal, by binding identity (v38 `expression_forms` computed keys).
    pub constant_keys: std::collections::BTreeMap<(usize, u32), String>,
    /// Generic method signatures of object types (v38), erased.
    pub erased: crate::lower::erased_methods::ErasedMethods,
    /// The `unknown[]` view function of `Array.isArray` narrowing (v38).
    pub unknown_array: Option<usize>,
    /// V38 (`byte_values`): the array type id that every `Uint8Array` value
    /// erases to (`literal_erasure`). It is a `number[]` of its own, distinct
    /// from the byte storage `number[]` that the prelude operates on, so the
    /// value of a `Uint8Array` type carries the byte-array tag
    /// ([`varta_common::value::ValueKind::Bytes`]) wherever a tag is written
    /// ([`tag_of`]). Set only after erasure.
    pub bytes: Option<u32>,
    /// V38 (`declaration_forms`): overload signatures of functions and
    /// methods, by implementation.
    pub overloads: crate::lower::overloads::Overloads,
    /// K9 type-level state (v38 `type_level`); see `lower::type_level`.
    pub type_level: crate::lower::type_level::TypeLevel,
    /// Template objects of tagged template sites (v38 `expression_forms`);
    /// see `lower::tagged_templates`.
    pub templates: crate::lower::tagged_templates::TemplateObjects,
    /// The receiver register of object-literal methods that use `this`
    /// (v38 `expression_forms`); see `lower::object_methods`.
    pub receivers: crate::lower::object_methods::Receivers,
    /// V38 (`module_evaluation`): module bindings that code may read before
    /// their initialization, by root slot.
    pub module_flags: std::collections::BTreeMap<u32, ModuleFlag>,
    /// V38 (`declaration_forms`): the own static properties subclasses
    /// create by writing inherited static fields; see `lower::static_store`.
    pub static_stores: crate::lower::static_store::Stores,
    /// V38 (`symbol_values`, K9 D2): the class id of the symbol prelude's
    /// record class (`SymbolValue`) that every `symbol` value erases to
    /// (`literal_erasure`), so emission tags it with the value18 symbol tag
    /// ([`varta_common::value::ValueKind::Symbol`]) wherever a tag is written
    /// ([`tag_of`]). Set only after erasure.
    pub symbol: Option<u32>,
}
/// The initialization flag of a module binding that code may reach before
/// its initialization (v38 `module_evaluation`).
#[derive(Clone, Debug)]
pub struct ModuleFlag {
    /// The root slot of the flag (`0` or `1`).
    pub flag: u32,
    /// The binding's source name (diagnostic text).
    pub name: String,
    /// A `var` or a static field: JavaScript reads `undefined` before its
    /// definition and a write defines it, so an early read is a checked
    /// read (a `TypeError` when the declared type does not admit
    /// `undefined`) and an early write sets the flag. Otherwise the binding
    /// is in its temporal dead zone and every early access throws
    /// `ReferenceError`.
    pub hoisted: bool,
}
/// `{ [key: string]: T }` / `Record<string, T>` with its storage record shape
/// `{ keys: string[]; values: T[] }` (canonical slots 0 and 1), or under value
/// layout 18 (v38 `dictionary_hashing`) `{ holes: number; keys: string[];
/// table: number[]; values: T[] }` (slots 0 to 3; see
/// `lower::dictionary_hashing`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DictionaryType {
    pub value: Type,
    pub keys: Type,
    pub values: Type,
    /// The value18 hash-table vector type (`number[]`), if hashed.
    pub table: Option<Type>,
    pub storage: u32,
    /// Declared members of a mixed index record or an `object` view (v35),
    /// an index into `ValueTypes::mixed`; `None` for a plain dictionary. The
    /// storage is always the plain dictionary's: declared members are a
    /// static view whose reads are checked, never a layout.
    pub declared: Option<u32>,
}
impl DictionaryType {
    /// Canonical storage slot of the key vector.
    pub(crate) fn keys_slot(&self) -> u32 {
        if self.table.is_some() {
            varta_common::value::HASHED_DICTIONARY_KEYS_SLOT as u32
        } else {
            0
        }
    }
    /// Canonical storage slot of the value vector.
    pub(crate) fn values_slot(&self) -> u32 {
        if self.table.is_some() {
            varta_common::value::HASHED_DICTIONARY_VALUES_SLOT as u32
        } else {
            1
        }
    }
    /// Canonical storage slot of the value18 hash table.
    pub(crate) fn table_slot(&self) -> Option<u32> {
        self.table
            .map(|_| varta_common::value::HASHED_DICTIONARY_TABLE_SLOT as u32)
    }
    /// Canonical storage slot of the value18 hole count.
    pub(crate) fn holes_slot(&self) -> Option<u32> {
        self.table
            .map(|_| varta_common::value::HASHED_DICTIONARY_HOLES_SLOT as u32)
    }
}
/// A dictionary operation lowered once per dictionary type into a shared
/// function (v35) instead of inline at every use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DictionaryHelper {
    /// `(d, key) -> T | undefined`.
    Get,
    /// `(d, key, value) -> T`: replace in place or insert at the own-key position.
    Set,
    /// `(d, key) -> boolean`.
    Delete,
    /// Value18 (`dictionary_hashing`), per dictionary type: `(d) -> boolean`;
    /// drops the holes of an indexed dictionary, leaving dense vectors and no
    /// table.
    Compact,
    /// Value18, shared: `(keys, capacity) -> number[]`, a fresh table of
    /// `capacity` slots referencing every present entry position.
    Index,
}
impl DictionaryHelper {
    /// The type id under which the shared (type-independent) value18
    /// helpers are registered.
    pub(crate) const SHARED: u32 = u32::MAX;
}
/// A conversion of an `unknown` cell to a declared type lowered once per
/// target type into a shared function (v36 `checked_declared_fields`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Converter {
    /// `(array: T[]) -> boolean`: whether the array's static type provably
    /// is `T[]`, so the stored array itself may be returned.
    Exact,
    /// `(cell: unknown, message: string) -> T`: a fresh `T` built from the
    /// JSON-graph form of the cell, or a `TypeError(message)`.
    Build,
    /// `(d: Record<string, unknown>, key, wrong, missing) -> T`: the declared
    /// read of a mixed record's field of type `T`, which stores a built value
    /// back under the key.
    Read,
}
/// The declared fields of a mixed index record (v35 `mixed_index_records`),
/// sorted by name, or the keys an `in` guard proved on an `object` (v35
/// `unknown_json`). Field types include `undefined` for optional fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MixedMembers {
    pub fields: Vec<RecordField>,
    /// TypeScript's `object` narrowed by `in` guards: only the proven keys may
    /// be read by name, and dynamic keys are rejected (they would be `any`).
    pub object: bool,
}
/// `Promise<T>` (v34): the value type and the private record shape of its
/// objects; see `lower::promises`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PromiseType {
    /// The fulfillment value type; `void` for `Promise<void>`.
    pub value: Type,
    pub shape: u32,
}
/// How `Expression::Schedule` runs the frame its creator returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScheduleMode {
    /// Run the body now, until its first suspension (an async call or `then`).
    Start,
    /// Queue the start as a microtask (promise adoption).
    Enqueue,
}
/// What a generator step function reports to the lowered protocol code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub(crate) enum GeneratorOutcome {
    /// Suspended at a `yield`; the completion cell holds `T`.
    Yielded = 0,
    /// Completed by `return`; the completion cell holds `TReturn` (none for void).
    Returned = 1,
    /// `next` on a completed generator: `{ done: true, value: undefined }`.
    DoneUndefined = 2,
    /// `return(v)` on a not-started or completed generator: `{ done: true, value: v }`.
    DoneInput = 3,
    /// The generator is executing: the protocol throws a `TypeError`.
    Running = 4,
    /// `throw(e)` on a not-started or completed generator: `e` is thrown.
    ThrowInput = 5,
}
/// `Generator<T, TReturn, TNext>` with its `IteratorResult<T, TReturn>` record
/// `{ done: boolean, value: T | TReturn | undefined }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GeneratorType {
    pub yielded: Type,
    /// `void` when the generator returns no value.
    pub returned: Type,
    /// The resume input; never `void` (an omitted `TNext` is `undefined`).
    pub next: Type,
    pub result: Type,
    /// `AsyncGenerator<T, TReturn, TNext>` (v36 `async_generators`): objects
    /// of this type are the frames of the internal body generator type
    /// `AsyncGeneratorType::body`; no frame has this type itself.
    pub asynchronous: Option<AsyncGeneratorType>,
    /// The internal body type of an async generator (v36): every suspension
    /// publishes the async generator's state record as the frame completion,
    /// and its step function keeps that cell while the body runs, so the
    /// protocol finds the state record at any time.
    pub body: bool,
}
/// The private parts of an async generator type (v36 `async_generators`); see
/// `lower::promises::async_generators`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AsyncGeneratorType {
    /// The internal body generator type whose frames are the objects.
    pub body: u32,
    /// The state record `{ head, queue, state }`.
    pub state: Type,
    /// The request record `{ kind, next, promise, returned, thrown }`.
    pub request: Type,
    /// `Promise<IteratorResult<T, TReturn>>`, the result of every request.
    pub promise: u32,
    /// `TReturn | undefined`: return requests (closing a `for await` sends
    /// `undefined`) and the body's completion value.
    pub returned: Type,
}
impl ValueTypes {
    /// Function slots lowering may allocate. V37 (`language_core`) removes
    /// unreachable functions after lowering (`lower::prune`), so it links
    /// against a wider compile-time cap; the emitted module still holds at
    /// most `abi::MAX_USER_FUNCTIONS` functions. Earlier profiles allocate
    /// against that limit directly and keep their diagnostics.
    pub fn function_limit(&self) -> usize {
        if self.profile.program_limits() {
            self.profile.limits().user_functions
        } else if self.profile.language_core() {
            crate::lower::LINKED_FUNCTIONS
        } else {
            varta_common::abi::MAX_USER_FUNCTIONS
        }
    }
    /// Layout version stamped into descriptors and catalogs; see
    /// `LanguageProfile::value_layout`. The module's value section declares
    /// `LanguageProfile::value_abi` instead.
    pub fn value_version(&self) -> u32 {
        self.profile.value_layout()
    }
    /// Field ceiling of one record shape: value15 wide records (v34
    /// `wide_records`) or the frozen value13 limit.
    pub fn max_record_fields(&self) -> usize {
        if self.profile.wide_records() {
            varta_common::value::max_record_fields(self.value_version())
        } else {
            varta_common::value::MAX_RECORD_FIELDS
        }
    }
    /// The declared members of a mixed index record or `object` view (v35).
    pub fn declared(&self, ty: Type) -> Option<&MixedMembers> {
        match ty {
            Type::Dictionary(id) => self.dictionaries[id as usize]
                .declared
                .map(|members| &self.mixed[members as usize]),
            _ => None,
        }
    }
    /// The record shape that physically lays out a record or dictionary value.
    pub fn record_layout(&self, ty: Type) -> Option<u32> {
        match ty {
            Type::Record(id) => Some(id),
            Type::Dictionary(id) => Some(self.dictionaries[id as usize].storage),
            _ => None,
        }
    }
    pub fn number(&self) -> Type {
        if self.profile.binary64() {
            Type::Float64
        } else {
            Type::Number
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParameterShape {
    pub optional: Vec<bool>,
    pub rest: bool,
}
impl ParameterShape {
    pub(crate) fn fixed(count: usize) -> Self {
        Self {
            optional: vec![false; count],
            rest: false,
        }
    }
    pub(crate) fn extended(&self) -> bool {
        self.rest || self.optional.iter().any(|value| *value)
    }
    pub(crate) fn fixed_count(&self) -> usize {
        self.optional.len() - usize::from(self.rest)
    }
    pub(crate) fn minimum(&self) -> usize {
        self.optional[..self.fixed_count()]
            .iter()
            .rposition(|optional| !*optional)
            .map_or(0, |i| i + 1)
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FunctionType {
    pub shape: ParameterShape,
    pub receiver: Option<Type>,
    pub params: Vec<Type>,
    pub result: Type,
    /// The erased type of a generic method signature (v38): an index into
    /// `lower::erased_methods::ErasedMethods`. It keeps the type distinct
    /// from the same non-generic signature while lowering; erasure merges
    /// it with that signature afterwards.
    pub generic: Option<u32>,
    /// A construct signature `new (params) => R` (v38 `type_level`): a
    /// constructor value, physically the closure that constructs, invoked
    /// only by `new`.
    pub construct: bool,
    /// An overloaded function type (v38 `type_level`): an index into the
    /// overload sets of `lower::type_level`; the physical signature merges
    /// every overload's parameter and result types.
    pub overloads: Option<u32>,
}
impl FunctionType {
    pub(crate) fn physical_params(&self) -> Vec<Type> {
        self.receiver
            .into_iter()
            .chain(self.params.iter().copied())
            .collect()
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecordField {
    pub name: String,
    pub ty: Type,
    pub optional: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RecordShape {
    pub fields: Vec<RecordField>,
    /// Private instance layout of a class; never shared with structural shapes.
    pub class: Option<u32>,
}

impl Type {
    pub(crate) fn value_kind(self) -> Option<varta_common::value::ValueKind> {
        use varta_common::value::ValueKind as V;
        Some(match self {
            Self::BigInt => V::BigInt,
            Self::Symbol => V::Symbol,
            Self::Error | Self::NativeError(_) => V::Error,
            Self::Null => V::Null,
            Self::Undefined => V::Undefined,
            Self::Number => V::I32,
            Self::Float64 => V::Float64,
            Self::Boolean => V::Boolean,
            Self::String => V::String,
            // An instance is a genuine record carrying its own class descriptor.
            Self::Record(_) | Self::Class(_) => V::Record,
            Self::Dictionary(_) => V::Dictionary,
            Self::Array(_) => V::Array,
            Self::Union(_) => V::PrimitiveUnion,
            Self::ReferenceUnion(_) | Self::Unknown | Self::Object(_) => V::ReferenceUnion,
            Self::Literal(base, _) => return base.erased().value_kind(),
            // Physical kinds of the v37 erased types (their storage).
            Self::Collection(CollectionKind::Uint8Array, _) | Self::Tuple(_) => V::Array,
            Self::Collection(..) => V::Record,
            _ => return None,
        })
    }
}
/// The erased types that carry a value18 tag of their own (v38): the
/// `Uint8Array` array type (`ValueTypes::bytes`) and the symbol record
/// class (`ValueTypes::symbol`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct OwnTags {
    pub bytes: Option<u32>,
    pub symbol: Option<u32>,
}
impl OwnTags {
    /// Whether `ty` is the `Uint8Array` array type (v38 `byte_values`).
    pub(crate) fn is_bytes(self, ty: Type) -> bool {
        matches!(ty, Type::Array(id) if self.bytes == Some(id))
    }
    /// Whether `ty` is the symbol record class (v38 `symbol_values`).
    pub(crate) fn is_symbol(self, ty: Type) -> bool {
        matches!(ty, Type::Class(id) if self.symbol == Some(id))
    }
}
impl ValueTypes {
    /// The erased types with value18 tags of their own.
    pub(crate) fn own_tags(&self) -> OwnTags {
        OwnTags {
            bytes: self.bytes,
            symbol: self.symbol,
        }
    }
}
/// The value tag of an erased type under `profile`: the byte-array tag for
/// the `Uint8Array` array type (v38 `byte_values`), the symbol tag for the
/// symbol record class (v38 `symbol_values`), otherwise
/// [`Type::profile_kind`].
pub(crate) fn tag_of(
    ty: Type,
    tags: OwnTags,
    profile: varta_common::language::LanguageProfile,
) -> Option<varta_common::value::ValueKind> {
    if tags.is_bytes(ty) && profile.byte_values() {
        return Some(varta_common::value::ValueKind::Bytes);
    }
    if tags.is_symbol(ty) && profile.symbol_values() {
        return Some(varta_common::value::ValueKind::Symbol);
    }
    ty.profile_kind(profile)
}
impl Type {
    /// The value tag of this type under `profile`: value16 (v35
    /// `promise_values`) also tags promises, which earlier layouts leave
    /// untagged and keep out of every aggregate.
    pub(crate) fn profile_kind(
        self,
        profile: varta_common::language::LanguageProfile,
    ) -> Option<varta_common::value::ValueKind> {
        match self {
            Self::Promise(_) if profile.promise_values() => {
                Some(varta_common::value::ValueKind::Promise)
            }
            // Value17 (v36 `function_fields`) tags function values.
            Self::Function(_) if profile.function_fields() => {
                Some(varta_common::value::ValueKind::Function)
            }
            _ => self.value_kind(),
        }
    }
    /// The value tag of this type in a descriptor of value layout `version`.
    pub(crate) fn layout_kind(self, version: u32) -> Option<varta_common::value::ValueKind> {
        match self {
            Self::Promise(_) if varta_common::value::promise_layout(version) => {
                Some(varta_common::value::ValueKind::Promise)
            }
            Self::Function(_) if varta_common::value::function_field_layout(version) => {
                Some(varta_common::value::ValueKind::Function)
            }
            _ => self.value_kind(),
        }
    }
}
pub(crate) struct Function {
    pub shape: ParameterShape,
    pub self_slot: Option<u32>,
    pub receiver: Option<Type>,
    pub source: crate::source::Source,
    pub span: swc_common::Span,
    pub params: Vec<Type>,
    pub result: Type,
    pub locals: Vec<Type>,
    pub body: Vec<Statement>,
    pub cells: std::collections::BTreeSet<u32>,
    pub captures: Vec<u32>,
    pub environment: bool,
    /// Generator type id of a `function*` declaration (v33). Until
    /// `lower::suspension` builds its frame, `result` is the generator type
    /// that calls observe; afterwards it is the body's `TReturn`.
    pub generator: Option<u32>,
}

impl Function {
    /// The private named-expression self parameter is not a source argument.
    pub(crate) fn call_params(&self) -> &[Type] {
        &self.params[..self
            .self_slot
            .map_or(self.params.len(), |slot| slot as usize)]
    }
}

/// The locals a delegating suspension point (`yield*`, v38) sets when it
/// resumes: the completion kind (`Completion as i32`, a Number) and the
/// thrown or returned input, which the delegation forwards to its iterator
/// instead of throwing or returning at the point.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct SuspendObserver {
    pub kind: u32,
    pub thrown: u32,
    pub returned: u32,
}

#[derive(Clone)]
pub(crate) struct Expr {
    pub span: swc_common::Span,
    pub ty: Type,
    pub kind: Expression,
}
impl Expr {
    /// V38 (`codegen_v2`): evaluation writes no binding and runs no guest
    /// function, so a binding read before it still holds the same value
    /// afterwards (allocation is allowed).
    pub(crate) fn write_free(&self) -> bool {
        use Expression as E;
        match &self.kind {
            E::Integer(_) | E::Float64(_) | E::Status | E::ModuleConstant(_) | E::Local(_) => true,
            E::String(_) => self.ty != Type::BigInt,
            E::Binary(_, a, b) | E::NumberSameValue(a, b) | E::Sequence(a, b) | E::Index(a, b) => {
                a.write_free() && b.write_free()
            }
            E::Conditional(a, b, c) | E::StringSearch(_, a, b, c) => {
                a.write_free() && b.write_free() && c.write_free()
            }
            E::Not(value)
            | E::Negative(value)
            | E::ToBoolean(value)
            | E::Length(value)
            | E::Field(value, _)
            | E::Undefined(value)
            | E::IsUndefined(value)
            | E::IsNullish(value)
            | E::UnboxUnion(value)
            | E::BoxUnion(value)
            | E::IsUnionMember(value, _) => value.write_free(),
            E::Math(_, args) | E::NumberPredicate(_, args) => args.iter().all(Self::write_free),
            E::Intrinsic(Intrinsic::CodeUnit | Intrinsic::CharAt | Intrinsic::CharCodeAt, args) => {
                args.iter().all(Self::write_free)
            }
            _ => false,
        }
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArraySearch {
    Includes,
    IndexOf,
    LastIndexOf,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum StringSearch {
    Includes,
    StartsWith,
    EndsWith,
    IndexOf,
    LastIndexOf,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum StringTrimMode {
    Trim,
    TrimStart,
    TrimEnd,
}

/// Engine-internal UTF-16 code-unit operations of the v35 Web type preludes
/// (`lower::web_text`); they make no host call and read no capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TextUnits {
    /// Index of the first code unit in the class as a Number, or -1.
    Index(UnitClass),
    /// A copy with ASCII `A`-`Z` lowercased.
    AsciiLowercase,
    /// A copy with ASCII `a`-`z` uppercased (v37 `collections` text prelude).
    AsciiUppercase,
    /// A copy without leading and trailing HTTP whitespace (TAB, LF, CR, SP).
    TrimHttpWhitespace,
}

/// A class of UTF-16 code units matched by [`TextUnits::Index`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum UnitClass {
    /// Above U+00FF: not a WebIDL ByteString unit.
    AboveByte,
    /// Not an HTTP token character (RFC 9110 `tchar`).
    NonToken,
    /// NUL, LF or CR: not allowed in a header value.
    ForbiddenValue,
    /// Above U+007F (v37 text prelude: the ASCII case-mapping fast path).
    NonAscii,
}

/// Engine-internal guest operations of the v37 preludes (`lower::engine`
/// intrinsics); they make no host call and read no capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Intrinsic {
    /// `(unknown) -> boolean`: unsupported thenable resolution, without
    /// executing a hook. Reads own fields and the closed class method table.
    HasCallableThen,
    /// `(string, index) -> number`: the UTF-16 code unit at a non-negative
    /// integral index, or NaN outside the string.
    CodeUnit,
    /// `(units: number[]) -> string`: a fresh string of the array's code
    /// units, each an integer in 0..=0xFFFF.
    FromUnits,
    /// `(reference) -> number`: the stable address of a managed reference
    /// (the nonmoving heap never relocates it), a collection key hash.
    Identity,
    /// V38 (`codegen_v2`) `String.prototype.charAt`: `(string, index) ->
    /// string`, the one-unit string at `ToIntegerOrInfinity(index)` or the
    /// empty string outside, without a host call.
    CharAt,
    /// V38 (`codegen_v2`) `String.prototype.charCodeAt`: `(string, index) ->
    /// number`, the code unit at `ToIntegerOrInfinity(index)` or NaN outside,
    /// without a host call.
    CharCodeAt,
    /// V38 view prelude: `(u0, …, uN) -> string`, a fresh string of 1 to
    /// [`MAX_PACKED_UNITS`] code units given as integers in 0..=0xFFFF (the
    /// fixed fields of one L0 operation), without an array.
    Units,
    /// V38 (`dictionary_hashing`): `(keys, table, key) -> number`, the entry
    /// position of `key` by probing an indexed dictionary's table from the
    /// 32-bit FNV-1a hash of its UTF-16 code units (`value::dictionary_hash`),
    /// or -1.
    /// Allocation-free; reads only.
    DictionaryFind,
    /// V38: as [`Self::DictionaryFind`], and the found slot becomes a
    /// tombstone (the entry is being deleted).
    DictionaryUnlink,
    /// V38: `(table, key, position) -> 0`, reference `position` from the
    /// first empty slot of `key`'s probe. Allocation-free.
    DictionaryPlace,
    /// V38: `(keys, table) -> 0`, reference every entry of a dense key vector
    /// from a fresh table of empty slots. Allocation-free.
    DictionaryIndex,
    /// V38: `(keys, values, table, position) -> 0`, move the entry just
    /// appended to dense vectors to own-key `position`, shifting the tail up
    /// and every table reference to it (the zero table handle: none).
    /// Allocation-free.
    DictionaryInsertAt,
    /// V38 (`collection_hashing`): a `Map`/`Set` table probe for keys of one
    /// [`KeyHash`] class, with the dictionary table encoding (string and
    /// BigInt keys use the dictionary probes where one exists).
    /// Allocation-free.
    Collection(CollectionProbe, KeyHash),
}
impl Intrinsic {
    /// The operand that a [`Self::Collection`] probe takes as a key slot
    /// (`i64` bits: a number reinterpreted, a boolean extended).
    pub(crate) fn key_slot(self) -> Option<usize> {
        match self {
            Self::Collection(
                CollectionProbe::Find | CollectionProbe::Unlink | CollectionProbe::Insert,
                _,
            ) => Some(2),
            _ => None,
        }
    }
}
/// The table probes of a v38 `Map`/`Set` (`collection_hashing`), with the
/// operands of the dictionary probe of the same name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CollectionProbe {
    /// `(keys, table, key) -> number`: the position of `key`'s entry, or -1.
    Find,
    /// As [`Self::Find`], and the found slot becomes a tombstone.
    Unlink,
    /// `(keys, table, key, position) -> number`: as [`Self::Find`], and on a
    /// miss `position` is referenced from the first empty slot of the probe
    /// (the caller then appends the entry there). The table must stay below
    /// half load.
    Insert,
    /// `(keys, table) -> 0`: reference every entry of a dense key vector
    /// from a fresh table of empty slots.
    Index,
}
/// How a `Map`/`Set` probe hashes and compares a key slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum KeyHash {
    /// Strings and BigInts (canonical code-unit payloads): FNV-1a over the
    /// code units (`value::dictionary_hash`), content equality. Their
    /// lookups, deletions and placements use the dictionary probes.
    Units,
    /// Numbers: SameValueZero (every NaN alike, `-0` equal to `+0`) over the
    /// slot's binary64 bits.
    Number,
    /// Booleans and references: the slot bits themselves (a reference's
    /// stable heap address), compared by identity.
    Identity,
}
/// The most code units one `Intrinsic::Units` call packs.
pub(crate) const MAX_PACKED_UNITS: usize = 16;

/// Engine-internal guest operations of the v37 RegExp prelude
/// (`lower::regexp`); they make no host call and read no capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RegExpIntrinsic {
    /// `(program, input, start, sticky)`: one matcher run; the capture slots
    /// as a string, or the empty string without a match.
    Exec,
    /// `(captures, slot)`: one capture slot as a Number, -1 when unset.
    Slot,
    /// `(text, index)`: the code unit at `index` as a Number, -1 outside.
    Unit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MathIntrinsic {
    Abs,
    Floor,
    Ceil,
    Trunc,
    Round,
    Sqrt,
    Sign,
    Fround,
    F16round,
    Clz32,
    Imul,
    Min,
    Max,
    Pow,
    Native(varta_common::native_math::NativeMath),
    Random,
    /// Internal finite positive power-of-two scale for stable hypot lowering.
    BinadeScale,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumberPredicate {
    Finite,
    Nan,
    Integer,
    SafeInteger,
}

#[derive(Clone)]
pub(crate) enum Expression {
    Assignment {
        value: Box<Expr>,
        input: u32,
        marker: u32,
        body: Vec<Statement>,
    },
    FunctionCheck {
        value: Box<Expr>,
        slot: u32,
    },
    FunctionRef {
        index: usize,
        fresh: bool,
        captures: Vec<u32>,
    },
    FunctionCall {
        callee: Box<Expr>,
        ignored_receiver: Option<Box<Expr>>,
        args: Vec<Expr>,
        slot: u32,
        arguments: Vec<u32>,
        output: Option<u32>,
    },
    StringSearch(StringSearch, Box<Expr>, Box<Expr>, Box<Expr>),
    StringRange {
        value: Box<Expr>,
        start: Box<Expr>,
        end: Box<Expr>,
        substring: bool,
    },
    StringTrim {
        value: Box<Expr>,
        mode: StringTrimMode,
    },
    /// A v35 Web prelude code-unit operation on a string value.
    TextUnits {
        value: Box<Expr>,
        op: TextUnits,
    },
    /// A v37 engine prelude intrinsic; operands are evaluated in order.
    Intrinsic(Intrinsic, Vec<Expr>),
    StringRepeat {
        value: Box<Expr>,
        count: Box<Expr>,
        error: Box<Expr>,
        length: u32,
        filled: u32,
    },
    NewError(varta_common::value::ErrorClass, Box<Expr>),
    ErrorName(Box<Expr>),
    ErrorMessage(Box<Expr>),
    ErrorText(Box<Expr>),
    ErrorOwn(Box<Expr>),
    BoxUnion(Box<Expr>),
    UnboxUnion(Box<Expr>),
    IsUndefined(Box<Expr>),
    /// Exact member of a statically known union; shape/array IDs are proven by lowering.
    IsUnionMember(Box<Expr>, Type),
    IsError(Box<Expr>),
    /// Whether a class instance's descriptor names one of these class shapes;
    /// `slot` is a hidden i32 local holding the loaded descriptor offset.
    IsClass {
        value: Box<Expr>,
        shapes: Vec<u32>,
        slot: u32,
    },
    IsNullish(Box<Expr>),
    /// Whether an array's element kind tag is this kind (v34 checked
    /// assertions); the element type itself is proven by that tag.
    ArrayKind(Box<Expr>, varta_common::value::ValueKind),
    Conditional(Box<Expr>, Box<Expr>, Box<Expr>),
    UnionLogical(BinaryOp, Box<Expr>, Box<Expr>),
    Integer(i32),
    Float64(f64),
    Undefined(Box<Expr>),
    Sequence(Box<Expr>, Box<Expr>),
    /// Encoded according to the selected source profile; metadata names stay UTF-8.
    String(Vec<u8>),
    Local(u32),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    /// Cached numeric operands: SameValue distinguishes signed zero and joins NaNs.
    NumberSameValue(Box<Expr>, Box<Expr>),
    Negative(Box<Expr>),
    Math(MathIntrinsic, Vec<Expr>),
    /// A v37 RegExp prelude guest operation.
    RegExp(RegExpIntrinsic, Vec<Expr>),
    NumberPredicate(NumberPredicate, Vec<Expr>),
    ToBoolean(Box<Expr>),
    Not(Box<Expr>),
    Call(usize, Vec<Expr>),
    Host(Host, Vec<Expr>),
    CheckedBigIntHost {
        host: Host,
        args: Vec<Expr>,
        status: u32,
    },
    Status,
    Capture(Box<Expr>),
    ResultField(ResultField, Box<Expr>),
    Record(Vec<(u32, Expr)>),
    /// Fields plus the slot-indexed presence set (wide records use > 32 bits).
    OptionalRecord(Vec<(u32, Expr)>, u128),
    ValueBlock {
        output: u32,
        marker: u32,
        roots: Vec<u32>,
        body: Vec<Statement>,
    },
    HasOwn(Box<Expr>, Option<u32>),
    RecordDelete {
        object: Box<Expr>,
        absent: Box<Expr>,
        slot: u32,
        scan: u32,
        write: u32,
        count: u32,
    },
    MaybeIndex(Box<Expr>, Box<Expr>),
    Field(Box<Expr>, u32),
    Json(Box<Expr>, Option<u32>),
    ParseJson(Box<Expr>),
    /// V38 (`kv`): hand a value to the host in a `kv::StageMode`
    /// (`kv_stage`); the result is its stage id.
    KvStage(i32, Box<Expr>),
    /// V38 (`kv`): decode an entry of the last KV read (`kv_key` or
    /// `kv_value`, the f64 entry index) into the checked schema of the
    /// expression's type, like `ParseJson`.
    KvDecode(varta_common::HostOperation, Box<Expr>),
    Array(Vec<Option<Expr>>),
    ArrayEnumerate {
        value: Box<Expr>,
        keys: bool,
    },
    ArrayReserve {
        length: Box<Expr>,
        count: u32,
        bytes: u32,
    },
    ArrayHasOwn(Box<Expr>, Box<Expr>),
    ArrayStringHasOwn(Box<Expr>, Box<Expr>),
    /// Value15 dictionaries (v34): the Number value of a String key that is a
    /// canonical array index, or -1.
    IndexKey(Box<Expr>),
    ArrayDelete {
        array: Box<Expr>,
        index: Option<Box<Expr>>,
        error: Option<Box<Expr>>,
    },
    ArrayPush(Box<Expr>, Box<Expr>),
    ArrayPop(Box<Expr>),
    ArrayReverse(Box<Expr>),
    ArrayFill(Box<Expr>, Box<Expr>, Box<Expr>, Box<Expr>),
    ArrayAt(Box<Expr>, Box<Expr>),
    ArraySearch(ArraySearch, Box<Expr>, Box<Expr>, Box<Expr>),
    ArraySlice(Box<Expr>, Box<Expr>, Box<Expr>),
    Index(Box<Expr>, Box<Expr>),
    Length(Box<Expr>),
    /// Read the permanent root slot of stored module constant `n` (v32).
    ModuleConstant(u32),
    /// Compiler-internal suspension point (K4.4 foundation): yields `value` and
    /// evaluates to the resume input, whose admissible type is this node's type.
    /// No source syntax produces it; `state` is assigned by `lower::suspension`.
    Suspend {
        value: Box<Expr>,
        state: u32,
        /// A delegating `yield*` point (v38 `iteration_protocol`) observes
        /// its resumption instead of throwing or returning at it.
        observe: Option<SuspendObserver>,
    },
    /// Resume a generator (v33) by its frame's closed per-type step function;
    /// evaluates to a `lower::generators` step outcome code.
    GeneratorStep {
        generator: Box<Expr>,
        completion: varta_common::function::frame::Completion,
        value: Box<Expr>,
    },
    /// The yielded or returned value in a generator frame's completion cell,
    /// read only directly after the step that produced it.
    GeneratorValue(Box<Expr>),
    /// `await` (v34): evaluates `promise` into hidden local `slot`, suspends
    /// until the scheduler resumes the frame after the promise settled, then
    /// evaluates `result` (which reads the settled promise in `slot`).
    /// Normalization turns it into the canonical suspension point.
    Await {
        promise: Box<Expr>,
        slot: u32,
        result: Box<Expr>,
        /// In an async generator body (v36): the relay job function started
        /// with the body's own frame (in this local) and the promise right
        /// before the suspension; it resumes the body once the promise settled.
        relay: Option<(usize, u32)>,
    },
    /// Create the frame of scheduled function `function` with `args` and start
    /// or enqueue it (v34). A function without a suspension point is called.
    Schedule {
        function: usize,
        args: Vec<Expr>,
        mode: ScheduleMode,
    },
    /// The same managed handle viewed as another internal type: a promise
    /// and its private record (v34). Emits nothing.
    Reinterpret(Box<Expr>),
    /// The all-zero bit pattern of the node's type (v34): the unset value and
    /// reason slots of a pending promise. The collector skips zero handles,
    /// and lowering reads those slots only after the promise settled.
    Zero,
}

pub(crate) use varta_common::HostOperation as Host;

#[derive(Clone)]
pub(crate) enum Statement {
    Trap,
    /// Publish stored module constant `n` into its permanent root slot (v32).
    InitModuleConstant(u32, Expr),
    Throw(Expr),
    TryCatch {
        body: Vec<Statement>,
        binding: Option<u32>,
        handler: Vec<Statement>,
        marker: u32,
    },
    TryFinally {
        body: Vec<Statement>,
        finalizer: Vec<Statement>,
        marker: u32,
        completion: u32,
        exception: u32,
        result: Option<u32>,
    },
    Init(u32, Expr),
    Rebind(Vec<u32>),
    RecordEnumerate {
        source: u32,
        target: u32,
        shape: u32,
        slots: [u32; 4],
        values: Vec<Expr>,
    },
    RecordRest {
        source: u32,
        target: u32,
        from: u32,
        to: u32,
        mapping: Vec<(u32, u32)>,
        index: u32,
        cursor: u32,
    },
    Set(u32, Expr),
    SetField(Expr, u32, Expr),
    ArrayLength {
        array: Expr,
        length: Expr,
        error: Expr,
    },
    SetIndex {
        array: Expr,
        index: Expr,
        value: Expr,
        slots: Option<(u32, u32)>,
    },
    Expr(Expr),
    Return(Option<Expr>),
    Block(Vec<Statement>),
    Scope(Vec<u32>, Vec<Statement>),
    If(Expr, Vec<Statement>, Vec<Statement>),
    While(Expr, Vec<Statement>),
    DoWhile(Expr, Vec<Statement>),
    For(Vec<Statement>, Option<Expr>, Vec<Statement>, Vec<Statement>),
    Switch {
        value: Expr,
        discriminant: u32,
        selected: u32,
        cases: Vec<(Option<Expr>, Vec<Statement>)>,
    },
    Label {
        id: u32,
        iteration: bool,
        body: Vec<Statement>,
    },
    BreakLabel(u32),
    ContinueLabel(u32),
    Break,
    Continue,
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Function(_) => "function",
            Self::Number | Self::Float64 => "number",
            Self::BigInt => "bigint",
            Self::Symbol => "symbol",
            Self::Boolean => "boolean",
            Self::String => "string",
            Self::Void => "void",
            Self::Error => "Error",
            Self::NativeError(class) => class.name(),
            Self::ResultNumber | Self::ResultFloat64 => "VartaResult<number>",
            Self::ResultBoolean => "VartaResult<boolean>",
            Self::ResultString => "VartaResult<string>",
            Self::ResultVoid => "VartaResult<void>",
            Self::Null => "null",
            Self::Undefined => "undefined",
            Self::Record(_) => "object",
            Self::Dictionary(_) => "dictionary object",
            Self::Class(_) => "class instance",
            Self::ClassValue(_) => "class",
            Self::Generator(_) => "generator",
            Self::Promise(_) => "promise",
            Self::Collection(kind, _) => kind.name(),
            Self::Tuple(_) => "tuple",
            Self::Array(_) => "array",
            Self::Union(_) => "primitive union",
            Self::ReferenceUnion(_) => "reference union",
            Self::Unknown => "unknown",
            Self::Never => "never",
            Self::Object(false) => "object",
            Self::Object(true) => "object | null",
            Self::Literal(LiteralBase::String, _) => "string literal type",
            Self::Literal(LiteralBase::Number, _) => "number literal type",
            Self::Literal(LiteralBase::Boolean, _) => "boolean literal type",
        })
    }
}
