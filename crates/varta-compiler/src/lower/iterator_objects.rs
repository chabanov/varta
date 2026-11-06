//! V38 hand-written iterators (`expression_forms`).
//!
//! A class method `[Symbol.iterator](): Iterator<T> { ... }` that is not a
//! generator returns an iterator object, `{ next(): IteratorResult<T> }`.
//! The class is rewritten so the method is the generator
//!
//! ```text
//! *[Symbol.iterator](): Generator<T, void, undefined> {
//!   const it: { next(): IteratorResult<T> } = ((): { next(): IteratorResult<T> } => { body })();
//!   while (true) {
//!     const step = it.next();
//!     if (step.done) { return; }
//!     yield step.value as T;
//!   }
//! }
//! ```
//!
//! so every consumer of the iteration protocol (`for...of`, spreads,
//! `Array.from`, `yield*`) drives the object's `next` exactly once per
//! element, in order, and stops at the first result whose `done` is true.
//! Deviation: the method body runs at the first `next()` instead of when
//! `[Symbol.iterator]()` is called, which `for...of` and spreads cannot
//! observe; an iterator object's `return`/`throw` members are not called.
use super::*;

/// Names of the rewrite's bindings; `#` cannot start a source identifier.
const ITERATOR: &str = "#iterator";
const STEP: &str = "#step";

fn ident(span: swc_common::Span, name: &str) -> ast::Ident {
    ast::Ident::new(name.into(), span, Default::default())
}
fn reference(span: swc_common::Span, name: &str, params: Vec<ast::TsType>) -> ast::TsType {
    ast::TsType::TsTypeRef(ast::TsTypeRef {
        span,
        type_name: ast::TsEntityName::Ident(ident(span, name)),
        type_params: (!params.is_empty()).then(|| {
            Box::new(ast::TsTypeParamInstantiation {
                span,
                params: params.into_iter().map(Box::new).collect(),
            })
        }),
    })
}
fn keyword(span: swc_common::Span, kind: ast::TsKeywordTypeKind) -> ast::TsType {
    ast::TsType::TsKeywordType(ast::TsKeywordType { span, kind })
}
fn annotation(span: swc_common::Span, ty: ast::TsType) -> Option<Box<ast::TsTypeAnn>> {
    Some(Box::new(ast::TsTypeAnn {
        span,
        type_ann: Box::new(ty),
    }))
}
fn member(span: swc_common::Span, object: &str, name: &str) -> ast::Expr {
    ast::Expr::Member(ast::MemberExpr {
        span,
        obj: Box::new(ast::Expr::Ident(ident(span, object))),
        prop: ast::MemberProp::Ident(ast::IdentName {
            span,
            sym: name.into(),
        }),
    })
}
fn constant(
    span: swc_common::Span,
    name: &str,
    ty: Option<ast::TsType>,
    init: ast::Expr,
) -> ast::Stmt {
    ast::Stmt::Decl(ast::Decl::Var(Box::new(ast::VarDecl {
        span,
        ctxt: Default::default(),
        kind: ast::VarDeclKind::Const,
        declare: false,
        decls: vec![ast::VarDeclarator {
            span,
            name: ast::Pat::Ident(ast::BindingIdent {
                id: ident(span, name),
                type_ann: ty.and_then(|ty| annotation(span, ty)),
            }),
            init: Some(Box::new(init)),
            definite: false,
        }],
    })))
}

/// The `T` of a method returning `Iterator<T>`, if `method` is a
/// non-generator `[Symbol.iterator]()` method to rewrite.
fn iterator_method(method: &ast::ClassMethod) -> Option<ast::TsType> {
    let ast::PropName::Computed(key) = &method.key else {
        return None;
    };
    if classes::well_known_symbol(&key.expr, true) != Some(classes::ITERATOR_METHOD)
        || method.function.is_generator
        || method.function.is_async
        || method.is_static
        || method.kind != ast::MethodKind::Method
        || !method.function.params.is_empty()
    {
        return None;
    }
    let returned = method.function.return_type.as_ref()?;
    let ast::TsType::TsTypeRef(returned) = &*returned.type_ann else {
        return None;
    };
    let ast::TsEntityName::Ident(name) = &returned.type_name else {
        return None;
    };
    if name.sym != "Iterator" {
        return None;
    }
    let params = returned.type_params.as_ref()?;
    let [element] = params.params.as_slice() else {
        return None;
    };
    Some((**element).clone())
}

/// `class` with every hand-written `[Symbol.iterator]()` method rewritten.
pub(super) fn rewrite(mut class: Box<ast::Class>) -> Box<ast::Class> {
    for item in &mut class.body {
        let ast::ClassMember::Method(method) = item else {
            continue;
        };
        let Some(element) = iterator_method(method) else {
            continue;
        };
        let span = method.function.span;
        let Some(body) = method.function.body.take() else {
            continue;
        };
        let object = ast::TsType::TsTypeLit(ast::TsTypeLit {
            span,
            members: vec![ast::TsTypeElement::TsMethodSignature(
                ast::TsMethodSignature {
                    span,
                    key: Box::new(ast::Expr::Ident(ident(span, "next"))),
                    computed: false,
                    optional: false,
                    params: vec![],
                    type_ann: annotation(
                        span,
                        reference(span, "IteratorResult", vec![element.clone()]),
                    ),
                    type_params: None,
                },
            )],
        });
        let source = ast::Expr::Call(ast::CallExpr {
            span,
            ctxt: Default::default(),
            callee: ast::Callee::Expr(Box::new(ast::Expr::Paren(ast::ParenExpr {
                span,
                expr: Box::new(ast::Expr::Arrow(ast::ArrowExpr {
                    span,
                    params: vec![],
                    body: Box::new(ast::ArrowFunctionBody::FunctionBody(body)),
                    return_type: annotation(span, object.clone()),
                    ..Default::default()
                })),
            }))),
            args: vec![],
            type_args: None,
        });
        let next = ast::Expr::Call(ast::CallExpr {
            span,
            ctxt: Default::default(),
            callee: ast::Callee::Expr(Box::new(member(span, ITERATOR, "next"))),
            args: vec![],
            type_args: None,
        });
        let step = ast::Stmt::If(ast::IfStmt {
            span,
            test: Box::new(member(span, STEP, "done")),
            cons: Box::new(ast::Stmt::Return(ast::ReturnStmt { span, arg: None })),
            alt: None,
        });
        let produce = ast::Stmt::Expr(ast::ExprStmt {
            span,
            expr: Box::new(ast::Expr::Yield(ast::YieldExpr {
                span,
                arg: Some(Box::new(ast::Expr::TsAs(ast::TsAsExpr {
                    span,
                    expr: Box::new(member(span, STEP, "value")),
                    type_ann: Box::new(element.clone()),
                }))),
                delegate: false,
            })),
        });
        let looped = ast::Stmt::While(ast::WhileStmt {
            span,
            test: Box::new(ast::Expr::Lit(ast::Lit::Bool(ast::Bool {
                span,
                value: true,
            }))),
            body: Box::new(ast::Stmt::Block(ast::BlockStmt {
                span,
                ctxt: Default::default(),
                stmts: vec![constant(span, STEP, None, next), step, produce],
            })),
        });
        method.function.is_generator = true;
        method.function.return_type = annotation(
            span,
            reference(
                span,
                "Generator",
                vec![
                    element,
                    keyword(span, ast::TsKeywordTypeKind::TsVoidKeyword),
                    keyword(span, ast::TsKeywordTypeKind::TsUndefinedKeyword),
                ],
            ),
        );
        method.function.body = Some(ast::FunctionBody {
            span,
            stmts: vec![constant(span, ITERATOR, Some(object), source), looped],
        });
    }
    class
}
