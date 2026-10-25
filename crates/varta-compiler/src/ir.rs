use swc_ecma_ast::BinaryOp;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Type {
    Number,
    Boolean,
    String,
    Void,
}

pub(crate) struct Program {
    pub functions: Vec<Function>,
    pub main: usize,
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
}

pub(crate) use varta_common::HostOperation as Host;

pub(crate) enum Statement {
    Set(u32, Expr),
    Expr(Expr),
    Return(Option<Expr>),
    Block(Vec<Statement>),
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
        })
    }
}
