use swc_ecma_ast::BinaryOp;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Type {
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
}

impl Type {
    pub(crate) fn is_handle(self) -> bool {
        matches!(
            self,
            Self::Function(_)
                | Self::Error
                | Self::NativeError(_)
                | Self::String
                | Self::Record(_)
                | Self::Array(_)
                | Self::Union(_)
                | Self::ReferenceUnion(_)
        )
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
}

#[derive(Default)]
pub(crate) struct ValueTypes {
    pub profile: varta_common::language::LanguageProfile,
    pub records: Vec<RecordShape>,
    pub arrays: Vec<Type>,
    pub unions: Vec<Vec<Type>>,
    pub functions: Vec<FunctionType>,
}
impl ValueTypes {
    pub fn value_version(&self) -> u32 {
        self.profile.value_abi()
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
}

impl Type {
    pub(crate) fn value_kind(self) -> Option<varta_common::value::ValueKind> {
        use varta_common::value::ValueKind as V;
        Some(match self {
            Self::Error | Self::NativeError(_) => V::Error,
            Self::Null => V::Null,
            Self::Undefined => V::Undefined,
            Self::Number => V::I32,
            Self::Float64 => V::Float64,
            Self::Boolean => V::Boolean,
            Self::String => V::String,
            Self::Record(_) => V::Record,
            Self::Array(_) => V::Array,
            Self::Union(_) => V::PrimitiveUnion,
            Self::ReferenceUnion(_) => V::ReferenceUnion,
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
    IsNullish(Box<Expr>),
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
    Negative(Box<Expr>),
    Not(Box<Expr>),
    Call(usize, Vec<Expr>),
    Host(Host, Vec<Expr>),
    Status,
    Capture(Box<Expr>),
    ResultField(ResultField, Box<Expr>),
    Record(Vec<(u32, Expr)>),
    OptionalRecord(Vec<(u32, Expr)>, u32),
    HasOwn(Box<Expr>, Option<u32>),
    MaybeIndex(Box<Expr>, Box<Expr>),
    Field(Box<Expr>, u32),
    Json(Box<Expr>),
    ParseJson(Box<Expr>),
    Array(Vec<Option<Expr>>),
    ArrayReserve {
        length: Box<Expr>,
        count: u32,
        bytes: u32,
    },
    ArrayHasOwn(Box<Expr>, Box<Expr>),
    ArrayDelete {
        array: Box<Expr>,
        index: Option<Box<Expr>>,
        error: Option<Box<Expr>>,
    },
    ArrayPush(Box<Expr>, Box<Expr>),
    ArrayPop(Box<Expr>),
    ArrayAt(Box<Expr>, Box<Expr>),
    ArraySearch(ArraySearch, Box<Expr>, Box<Expr>, Box<Expr>),
    ArraySlice(Box<Expr>, Box<Expr>, Box<Expr>),
    Index(Box<Expr>, Box<Expr>),
    Length(Box<Expr>),
}

pub(crate) use varta_common::HostOperation as Host;

pub(crate) enum Statement {
    Trap,
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
    For(Vec<Statement>, Option<Expr>, Vec<Statement>, Vec<Statement>),
    Switch {
        value: Expr,
        discriminant: u32,
        selected: u32,
        cases: Vec<(Option<Expr>, Vec<Statement>)>,
    },
    Break,
    Continue,
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Function(_) => "function",
            Self::Number | Self::Float64 => "number",
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
            Self::Array(_) => "array",
            Self::Union(_) => "primitive union",
            Self::ReferenceUnion(_) => "reference union",
        })
    }
}
