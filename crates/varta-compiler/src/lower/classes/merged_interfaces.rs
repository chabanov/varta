//! A class merged with a same-named interface (v38 `web_bytes`):
//!
//! ```ts
//! export interface Client extends Resources {}
//! export class Client {
//!   constructor(t: Transport) { this.t = t; Object.assign(this, createResources(t)); }
//! }
//! ```
//!
//! The interface's members (own and inherited) become ordinary instance
//! fields of the class. The constructor must provide them with one top-level
//! `Object.assign(this, X)` statement; it is rewritten before the constructor
//! discipline runs into `const tmp: interface C = X;` followed by
//! `this.m = tmp.m` for every merged member, so the ordinary field-assignment checks prove each
//! member provided with a compatible type, and the discipline proves that no
//! other use of `this` observes a merged member before the assignment. A
//! method member of the interface is a function field whose value comes from
//! `X`. Members of `X` that the interface does not declare are not copied
//! (they would not be visible through the class type).
use super::*;

/// The merged interface of a class and the names of the members it adds.
#[derive(Clone)]
pub(super) struct MergedInterface {
    pub interface: Box<interfaces::Interface>,
    pub members: Vec<String>,
}

/// Whether `statement` is `Object.assign(this, X);`; returns `X`.
fn assign_to_this(statement: &ast::Stmt) -> Option<&ast::Expr> {
    let ast::Stmt::Expr(statement) = statement else {
        return None;
    };
    let ast::Expr::Call(call) = &*statement.expr else {
        return None;
    };
    let ast::Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let ast::Expr::Member(member) = &**callee else {
        return None;
    };
    let (ast::Expr::Ident(object), ast::MemberProp::Ident(property)) = (&*member.obj, &member.prop)
    else {
        return None;
    };
    if object.sym != "Object" || property.sym != "assign" {
        return None;
    }
    match call.args.as_slice() {
        [target, value]
            if target.spread.is_none()
                && value.spread.is_none()
                && matches!(&*target.expr, ast::Expr::This(_)) =>
        {
            Some(&value.expr)
        }
        _ => None,
    }
}

/// The binding of the merged value: no identifier can spell it, so it never
/// collides, and diagnostics name the call it stands for.
const TEMPORARY: &str = "Object.assign(this, …)";

/// Rewrite the constructor statements of a class with merged members `merged`
/// (see the module documentation). `Object.assign(this, …)` must appear
/// exactly once, as a top-level statement.
pub(super) fn rewrite(
    statements: Vec<ast::Stmt>,
    merged: &MergedInterface,
    class: &str,
    source: &Source,
    span: Span,
) -> Result<Vec<ast::Stmt>> {
    let assigns: Vec<usize> = statements
        .iter()
        .enumerate()
        .filter(|(_, statement)| assign_to_this(statement).is_some())
        .map(|(index, _)| index)
        .collect();
    let at = match assigns.as_slice() {
        [at] => *at,
        [] => {
            return Err(source.locate(
                invalid(format!(
                    "class {class} merged with interface {class} must provide its members {} \
                     with one top-level Object.assign(this, …) in the constructor",
                    merged.members.join(", ")
                )),
                span,
            ));
        }
        [_, second, ..] => {
            return Err(source.locate(
                unsupported(&format!(
                    "more than one Object.assign(this, …) in the constructor of merged class \
                     {class}; provide every merged member from one value"
                )),
                statements[*second].span(),
            ));
        }
    };
    let value = assign_to_this(&statements[at])
        .ok_or_else(|| CompilerError::Internal("missing Object.assign value".into()))?
        .clone();
    let statement_span = statements[at].span();
    let ident = || ast::Ident::new(TEMPORARY.into(), statement_span, Default::default());
    // The temporary is typed with the merged interface, so the ordinary
    // assignment check names every missing or mismatched member.
    let annotation = ast::TsTypeAnn {
        span: statement_span,
        type_ann: Box::new(ast::TsType::TsTypeRef(ast::TsTypeRef {
            span: statement_span,
            type_name: ast::TsEntityName::Ident(ast::Ident::new(
                merged_type_name(class).into(),
                statement_span,
                Default::default(),
            )),
            type_params: None,
        })),
    };
    let mut replacement = vec![ast::Stmt::Decl(ast::Decl::Var(Box::new(ast::VarDecl {
        span: statement_span,
        ctxt: Default::default(),
        kind: ast::VarDeclKind::Const,
        declare: false,
        decls: vec![ast::VarDeclarator {
            span: statement_span,
            name: ast::Pat::Ident(ast::BindingIdent {
                id: ident(),
                type_ann: Some(Box::new(annotation)),
            }),
            init: Some(Box::new(value)),
            definite: false,
        }],
    })))];
    for member in &merged.members {
        let read = ast::Expr::Member(ast::MemberExpr {
            span: statement_span,
            obj: Box::new(ast::Expr::Ident(ident())),
            prop: ast::MemberProp::Ident(ast::IdentName::new(
                member.as_str().into(),
                statement_span,
            )),
        });
        replacement.push(constructors::this_assign(member, Box::new(read)));
    }
    let mut rewritten = statements;
    rewritten.splice(at..=at, replacement);
    Ok(rewritten)
}
