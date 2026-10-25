use swc_ecma_ast::BinaryOp;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Type {
    Number,
    Boolean,
    String,
    Void,
    ResultNumber,
    ResultBoolean,
    ResultString,
    ResultVoid,
}

impl Type {
    pub(crate) fn has_string(self) -> bool {
        matches!(self, Self::String | Self::ResultString)
    }
    pub(crate) fn captured(self) -> Option<Self> {
        Some(match self {
            Self::Number => Self::ResultNumber,
            Self::Boolean => Self::ResultBoolean,
            Self::String => Self::ResultString,
            Self::Void => Self::ResultVoid,
            _ => return None,
        })
    }
    pub(crate) fn result_value(self) -> Option<Self> {
        Some(match self {
            Self::ResultNumber => Self::Number,
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
}
pub(crate) struct Function {
    pub source: crate::source::Source,
    pub span: swc_common::Span,
    pub params: Vec<Type>,
    pub result: Type,
    pub locals: Vec<Type>,
    pub body: Vec<Statement>,
}

pub(crate) struct Expr {
    pub span: swc_common::Span,
    pub ty: Type,
    pub kind: Expression,
}
pub(crate) enum Expression {
    Integer(i32),
    String(String),
    Local(u32),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Negative(Box<Expr>),
    Not(Box<Expr>),
    Call(usize, Vec<Expr>),
    Host(Host, Vec<Expr>),
    Status,
    Capture(Box<Expr>),
    ResultField(ResultField, Box<Expr>),
}

pub(crate) use varta_common::HostOperation as Host;

pub(crate) enum Statement {
    Set(u32, Expr),
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
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::String => "string",
            Self::Void => "void",
            Self::ResultNumber => "VartaResult<number>",
            Self::ResultBoolean => "VartaResult<boolean>",
            Self::ResultString => "VartaResult<string>",
            Self::ResultVoid => "VartaResult<void>",
        })
    }
}
