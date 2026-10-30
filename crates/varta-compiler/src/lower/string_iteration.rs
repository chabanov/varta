//! `for (const ch of text)` over a string (v38 `structural_subtyping`): the
//! loop visits code points, as the String iterator does. It is lowered as
//!
//! ```text
//! { const #s = text;
//!   for (let #i = 0; #i < #s.length; ) {
//!     const #w = <#s[#i], #s[#i + 1] are a surrogate pair> ? 2 : 1;
//!     const ch = #s.slice(#i, #i + #w);  // or `ch = …` for an assignment head
//!     #i += #w;
//!     body } }
//! ```
//!
//! The index advances before the body, so `continue` keeps the iteration
//! order. The hidden names start with `#`, which no source identifier can.
use super::*;

/// The UTF-16 surrogate ranges.
const HIGH_FIRST: f64 = 55296.0;
const HIGH_LAST: f64 = 56319.0;
const LOW_FIRST: f64 = 56320.0;
const LOW_LAST: f64 = 57343.0;

fn ident(name: &str, span: swc_common::Span) -> ast::Ident {
    ast::Ident::new(name.into(), span, Default::default())
}
fn name(text: &str, span: swc_common::Span) -> Box<ast::Expr> {
    Box::new(ast::Expr::Ident(ident(text, span)))
}
fn number(value: f64, span: swc_common::Span) -> Box<ast::Expr> {
    Box::new(ast::Expr::Lit(ast::Lit::Num(ast::Number {
        span,
        value,
        raw: None,
    })))
}
fn binary(op: ast::BinaryOp, left: Box<ast::Expr>, right: Box<ast::Expr>) -> Box<ast::Expr> {
    Box::new(ast::Expr::Bin(ast::BinExpr {
        span: left.span(),
        op,
        left,
        right,
    }))
}
fn method(object: Box<ast::Expr>, property: &str, args: Vec<ast::Expr>) -> Box<ast::Expr> {
    let span = object.span();
    Box::new(ast::Expr::Call(ast::CallExpr {
        span,
        ctxt: Default::default(),
        callee: ast::Callee::Expr(Box::new(ast::Expr::Member(ast::MemberExpr {
            span,
            obj: object,
            prop: ast::MemberProp::Ident(ast::IdentName::new(property.into(), span)),
        }))),
        args: args
            .into_iter()
            .map(|expr| ast::ExprOrSpread {
                spread: None,
                expr: Box::new(expr),
            })
            .collect(),
        type_args: None,
    }))
}
fn declare(
    kind: ast::VarDeclKind,
    pattern: ast::Pat,
    init: Box<ast::Expr>,
    span: swc_common::Span,
) -> ast::Stmt {
    ast::Stmt::Decl(ast::Decl::Var(Box::new(ast::VarDecl {
        span,
        ctxt: Default::default(),
        kind,
        declare: false,
        decls: vec![ast::VarDeclarator {
            span,
            name: pattern,
            init: Some(init),
            definite: false,
        }],
    })))
}
fn binding(text: &str, span: swc_common::Span) -> ast::Pat {
    ast::Pat::Ident(ast::BindingIdent {
        id: ident(text, span),
        type_ann: None,
    })
}

impl Lower<'_> {
    /// The code-point loop of `for (… of text)` when `text` is a string.
    pub(super) fn string_for_of(&mut self, statement: &ast::ForOfStmt) -> Option<ast::Stmt> {
        let literal = matches!(
            stmt::unparen(&statement.right),
            ast::Expr::Lit(ast::Lit::Str(_)) | ast::Expr::Tpl(_)
        );
        if !self.types.profile.structural_subtyping()
            || statement.is_await
            || !literal
                && self.peek_type(&statement.right, 0).map(Type::erased) != Some(Type::String)
        {
            return None;
        }
        let span = statement.span;
        let id = self.next;
        let (text, index, width) = (format!("#fs{id}"), format!("#fi{id}"), format!("#fw{id}"));
        // A high surrogate followed by a low one is one code point.
        let unit = |offset: f64| {
            let at = if offset == 0.0 {
                name(&index, span)
            } else {
                binary(ast::BinaryOp::Add, name(&index, span), number(offset, span))
            };
            method(name(&text, span), "charCodeAt", vec![*at])
        };
        let between = |offset: f64, low: f64, high: f64| {
            binary(
                ast::BinaryOp::LogicalAnd,
                binary(ast::BinaryOp::GtEq, unit(offset), number(low, span)),
                binary(ast::BinaryOp::LtEq, unit(offset), number(high, span)),
            )
        };
        let width_value = Box::new(ast::Expr::Cond(ast::CondExpr {
            span,
            test: binary(
                ast::BinaryOp::LogicalAnd,
                between(0.0, HIGH_FIRST, HIGH_LAST),
                between(1.0, LOW_FIRST, LOW_LAST),
            ),
            cons: number(2.0, span),
            alt: number(1.0, span),
        }));
        let item = method(
            name(&text, span),
            "slice",
            vec![
                *name(&index, span),
                *binary(ast::BinaryOp::Add, name(&index, span), name(&width, span)),
            ],
        );
        let head = match &statement.left {
            ast::ForHead::VarDecl(declaration) if declaration.decls.len() == 1 => declare(
                declaration.kind,
                declaration.decls[0].name.clone(),
                item,
                span,
            ),
            ast::ForHead::Pat(pattern) => ast::Stmt::Expr(ast::ExprStmt {
                span,
                expr: Box::new(ast::Expr::Assign(ast::AssignExpr {
                    span,
                    op: ast::AssignOp::Assign,
                    left: match ast::AssignTarget::try_from((**pattern).clone()) {
                        Ok(target) => target,
                        Err(_) => return None,
                    },
                    right: item,
                })),
            }),
            _ => return None,
        };
        let advance = ast::Stmt::Expr(ast::ExprStmt {
            span,
            expr: Box::new(ast::Expr::Assign(ast::AssignExpr {
                span,
                op: ast::AssignOp::AddAssign,
                left: ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(
                    ast::BindingIdent {
                        id: ident(&index, span),
                        type_ann: None,
                    },
                )),
                right: name(&width, span),
            })),
        });
        let body = ast::Stmt::Block(ast::BlockStmt {
            span,
            ctxt: Default::default(),
            stmts: vec![
                declare(
                    ast::VarDeclKind::Const,
                    binding(&width, span),
                    width_value,
                    span,
                ),
                head,
                advance,
                (*statement.body).clone(),
            ],
        });
        let length = Box::new(ast::Expr::Member(ast::MemberExpr {
            span,
            obj: name(&text, span),
            prop: ast::MemberProp::Ident(ast::IdentName::new("length".into(), span)),
        }));
        let looped = ast::Stmt::For(ast::ForStmt {
            span,
            init: Some(ast::VarDeclOrExpr::VarDecl(Box::new(ast::VarDecl {
                span,
                ctxt: Default::default(),
                kind: ast::VarDeclKind::Let,
                declare: false,
                decls: vec![ast::VarDeclarator {
                    span,
                    name: binding(&index, span),
                    init: Some(number(0.0, span)),
                    definite: false,
                }],
            }))),
            test: Some(binary(ast::BinaryOp::Lt, name(&index, span), length)),
            update: None,
            body: Box::new(body),
        });
        Some(ast::Stmt::Block(ast::BlockStmt {
            span,
            ctxt: Default::default(),
            stmts: vec![
                declare(
                    ast::VarDeclKind::Const,
                    binding(&text, span),
                    statement.right.clone(),
                    span,
                ),
                looped,
            ],
        }))
    }
}
