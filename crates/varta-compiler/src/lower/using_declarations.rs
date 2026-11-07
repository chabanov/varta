//! V38 `using` and `await using` declarations (`expression_forms`).
//!
//! A block's `using x = e;` binds `x` like `const` and disposes it when the
//! rest of the block completes in any way (normally, by `return`, `break`,
//! `continue` or a throw): the statements after the declaration run in a
//! `try` whose `finally` calls `x[Symbol.dispose]()` (`await
//! x[Symbol.asyncDispose]()` for `await using`). Each declarator nests
//! one `try`, so resources are disposed in reverse order of declaration,
//! as ECMAScript DisposeResources does.
//!
//! Deviation: when the block throws and a disposal throws as well,
//! JavaScript throws a `SuppressedError` holding both; here the disposal's
//! error propagates (no `SuppressedError` class exists yet). `using` in a
//! `for` head and at module top level stay refused.
use super::*;

/// The block with its first `using` declaration (and every later one)
/// rewritten into `const` bindings and `try`/`finally` disposal, or `None`
/// without one.
pub(super) fn rewrite(statements: &[ast::Stmt]) -> Option<Vec<ast::Stmt>> {
    let index = statements
        .iter()
        .position(|statement| matches!(statement, ast::Stmt::Decl(ast::Decl::Using(_))))?;
    let ast::Stmt::Decl(ast::Decl::Using(using)) = &statements[index] else {
        return None;
    };
    let rest = &statements[index + 1..];
    let mut body = rewrite(rest).unwrap_or_else(|| rest.to_vec());
    for declarator in using.decls.iter().rev() {
        body = dispose(using, declarator, body);
    }
    let mut result = statements[..index].to_vec();
    result.extend(body);
    Some(result)
}

/// `const x = e; try { body } finally { x[Symbol.dispose](); }`.
fn dispose(
    using: &ast::UsingDecl,
    declarator: &ast::VarDeclarator,
    body: Vec<ast::Stmt>,
) -> Vec<ast::Stmt> {
    let span = declarator.span;
    let binding = ast::Stmt::Decl(ast::Decl::Var(Box::new(ast::VarDecl {
        span,
        ctxt: Default::default(),
        kind: ast::VarDeclKind::Const,
        declare: false,
        decls: vec![declarator.clone()],
    })));
    let target = match &declarator.name {
        ast::Pat::Ident(binding) => ast::Expr::Ident(binding.id.clone()),
        // A pattern is refused by the binding check below.
        _ => ast::Expr::Invalid(ast::Invalid { span }),
    };
    let method = if using.is_await {
        "asyncDispose"
    } else {
        "dispose"
    };
    let key = ast::Expr::Member(ast::MemberExpr {
        span,
        obj: Box::new(ast::Expr::Ident(ast::Ident::new(
            "Symbol".into(),
            span,
            Default::default(),
        ))),
        prop: ast::MemberProp::Ident(ast::IdentName {
            span,
            sym: method.into(),
        }),
    });
    let mut call = ast::Expr::Call(ast::CallExpr {
        span,
        ctxt: Default::default(),
        callee: ast::Callee::Expr(Box::new(ast::Expr::Member(ast::MemberExpr {
            span,
            obj: Box::new(target),
            prop: ast::MemberProp::Computed(ast::ComputedPropName {
                span,
                expr: Box::new(key),
            }),
        }))),
        args: vec![],
        type_args: None,
    });
    if using.is_await {
        call = ast::Expr::Await(ast::AwaitExpr {
            span,
            arg: Box::new(call),
        });
    }
    let finalizer = ast::BlockStmt {
        span,
        ctxt: Default::default(),
        stmts: vec![ast::Stmt::Expr(ast::ExprStmt {
            span,
            expr: Box::new(call),
        })],
    };
    vec![
        binding,
        ast::Stmt::Try(Box::new(ast::TryStmt {
            span,
            block: ast::BlockStmt {
                span,
                ctxt: Default::default(),
                stmts: body,
            },
            handler: None,
            finalizer: Some(finalizer),
        })),
    ]
}

impl Lower<'_> {
    /// Refuse the `using` forms the rewrite does not admit.
    pub(super) fn check_using(&self, statements: &[ast::Stmt]) -> Result<()> {
        for statement in statements {
            let ast::Stmt::Decl(ast::Decl::Using(using)) = statement else {
                continue;
            };
            for declarator in &using.decls {
                if !matches!(declarator.name, ast::Pat::Ident(_)) {
                    return Err(self.source.locate(
                        type_error("a using declaration binds an identifier, not a pattern"),
                        declarator.name.span(),
                    ));
                }
                if declarator.init.is_none() {
                    return Err(self.source.locate(
                        type_error("a using declaration requires an initializer"),
                        declarator.span,
                    ));
                }
            }
        }
        Ok(())
    }
}
