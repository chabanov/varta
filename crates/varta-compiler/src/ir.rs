use swc_ecma_ast::BinaryOp;

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
                | Self::Dictionary(_)
                | Self::Class(_)
                | Self::Generator(_)
                | Self::Function(_)
                | Self::Error
                | Self::NativeError(_)
                | Self::String
                | Self::Record(_)
                | Self::Array(_)
                | Self::Union(_)
                | Self::ReferenceUnion(_)
                | Self::Unknown
        )
    }
    /// A managed union cell: a closed union or `unknown` (v34).
    pub(crate) fn is_cell(self) -> bool {
        self.is_union() || self == Self::Unknown
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
        if self == Self::Void {
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
}
#[derive(Clone, Copy)]
pub(crate) struct SuspensionPoint {
    pub input: Type,
    pub yielded: Type,
}

#[derive(Default)]
pub(crate) struct ValueTypes {
    pub profile: varta_common::language::LanguageProfile,
    pub records: Vec<RecordShape>,
    pub arrays: Vec<Type>,
    pub unions: Vec<Vec<Type>>,
    pub functions: Vec<FunctionType>,
    pub literals: crate::lower::literals::LiteralTypes,
    pub named: crate::lower::named::NamedTypes,
    pub classes: crate::lower::classes::Classes,
    pub generics: crate::lower::generics::Generics,
    pub generators: Vec<GeneratorType>,
    pub dictionaries: Vec<DictionaryType>,
}
/// `{ [key: string]: T }` / `Record<string, T>` with its storage record shape
/// `{ keys: string[]; values: T[] }` (canonical slots 0 and 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DictionaryType {
    pub value: Type,
    pub keys: Type,
    pub values: Type,
    pub storage: u32,
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
}
impl ValueTypes {
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
            Self::ReferenceUnion(_) | Self::Unknown => V::ReferenceUnion,
            Self::Literal(base, _) => return base.erased().value_kind(),
            _ => return None,
        })
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

pub(crate) struct Expr {
    pub span: swc_common::Span,
    pub ty: Type,
    pub kind: Expression,
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
}

pub(crate) use varta_common::HostOperation as Host;

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
            Self::Generator(_) => "generator",
            Self::Array(_) => "array",
            Self::Union(_) => "primitive union",
            Self::ReferenceUnion(_) => "reference union",
            Self::Unknown => "unknown",
            Self::Literal(LiteralBase::String, _) => "string literal type",
            Self::Literal(LiteralBase::Number, _) => "number literal type",
            Self::Literal(LiteralBase::Boolean, _) => "boolean literal type",
        })
    }
}
