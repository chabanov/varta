//! Expando property assignments (v38 `type_level`, K9 A step 4): a
//! module-level statement `f.name = value;` on a function `f` declared in
//! the same module defines an own property of the function (TypeScript
//! types `f` with it). Module evaluation admits declarations only, so the
//! statement becomes the initializer of a generated constant,
//! `const __vartaDecl$expando$n = (f.name = value);`, evaluated in source
//! order with the module's other initializers; `lower::type_level::queries`
//! reads these initializers to type the function's properties.
use super::*;

/// The generated name segment of an expando constant.
pub(crate) const SEGMENT: &str = "expando";

/// Rewrite the module's expando statements.
pub(super) fn lower(module: &mut ast::Module) {
    let functions: BTreeSet<String> = module
        .body
        .iter()
        .filter_map(|item| match item {
            ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Fn(function)))
            | ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(ast::ExportDecl {
                decl: ast::Decl::Fn(function),
                ..
            })) if !function.declare && function.function.body.is_some() => {
                Some(function.ident.sym.to_string())
            }
            _ => None,
        })
        .collect();
    if functions.is_empty() {
        return;
    }
    let mut count = 0usize;
    for item in &mut module.body {
        let ast::ModuleItem::Stmt(ast::Stmt::Expr(statement)) = item else {
            continue;
        };
        if target(&statement.expr).is_none_or(|name| !functions.contains(name)) {
            continue;
        }
        let span = statement.span;
        let name = generated(&[SEGMENT, &count.to_string()]);
        count += 1;
        let init = std::mem::replace(
            &mut statement.expr,
            Box::new(ast::Expr::Invalid(ast::Invalid { span })),
        );
        *item = ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Var(Box::new(ast::VarDecl {
            span,
            ctxt: Default::default(),
            kind: ast::VarDeclKind::Const,
            declare: false,
            decls: vec![ast::VarDeclarator {
                span,
                name: ast::Pat::Ident(ast::BindingIdent {
                    id: ast::Ident::new(name.into(), span, Default::default()),
                    type_ann: None,
                }),
                init: Some(init),
                definite: false,
            }],
        }))));
    }
}

/// The function an expando assignment `f.name = value` defines a property
/// of.
pub(crate) fn target(expression: &ast::Expr) -> Option<&str> {
    let ast::Expr::Assign(assign) = expression else {
        return None;
    };
    let ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(member)) = &assign.left else {
        return None;
    };
    match (&*member.obj, &member.prop) {
        (ast::Expr::Ident(function), ast::MemberProp::Ident(_))
            if assign.op == ast::AssignOp::Assign =>
        {
            Some(function.sym.as_ref())
        }
        _ => None,
    }
}
