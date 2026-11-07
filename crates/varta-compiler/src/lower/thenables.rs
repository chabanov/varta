//! V38 structural thenables in `await` and `Promise.resolve` (`expression_forms`).
//!
//! A value whose static type has a callable `then` (a record field or a
//! class method) is resolved as ECMAScript PromiseResolve does: a new
//! promise is created and, one job later (NewPromiseResolveThenableJob),
//! `then` is called on the value with the promise's resolve function (and
//! reject function when `then` takes an `onRejected` callback). The value
//! is evaluated once, before the job. The lowering is the program
//!
//! ```text
//! new Promise<T>((resolve, reject) => {
//!   Promise.resolve().then(() => { thenable.then(resolve, (e) => reject(e)); });
//! })
//! ```
//!
//! whose job count equals the specification's. `T` is the parameter type
//! of the first `then` callback. A thenable whose `then` has another shape,
//! or that is reached through a union or `unknown`, keeps the existing
//! refusal (`promises::reject_thenable`, the runtime `TypeError`).
use super::*;

/// Prefix of the hidden bindings the lowering creates; `#` cannot start a
/// source identifier.
const THENABLE: &str = "#thenable";

fn ident(span: swc_common::Span, name: &str) -> ast::Ident {
    ast::Ident::new(name.into(), span, Default::default())
}
fn call(span: swc_common::Span, callee: ast::Expr, args: Vec<ast::Expr>) -> ast::Expr {
    ast::Expr::Call(ast::CallExpr {
        span,
        ctxt: Default::default(),
        callee: ast::Callee::Expr(Box::new(callee)),
        args: args
            .into_iter()
            .map(|expr| ast::ExprOrSpread {
                spread: None,
                expr: Box::new(expr),
            })
            .collect(),
        type_args: None,
    })
}
fn member(span: swc_common::Span, object: ast::Expr, name: &str) -> ast::Expr {
    ast::Expr::Member(ast::MemberExpr {
        span,
        obj: Box::new(object),
        prop: ast::MemberProp::Ident(ast::IdentName {
            span,
            sym: name.into(),
        }),
    })
}
fn arrow(span: swc_common::Span, params: &[&str], body: ast::Expr) -> ast::Expr {
    ast::Expr::Arrow(ast::ArrowExpr {
        span,
        params: params
            .iter()
            .map(|name| {
                ast::Pat::Ident(ast::BindingIdent {
                    id: ident(span, name),
                    type_ann: None,
                })
            })
            .collect(),
        body: Box::new(ast::ArrowFunctionBody::FunctionBody(ast::FunctionBody {
            span,
            stmts: vec![ast::Stmt::Expr(ast::ExprStmt {
                span,
                expr: Box::new(body),
            })],
        })),
        ..Default::default()
    })
}

impl Lower<'_> {
    /// The fulfilled value type of a thenable of type `ty` and the number of
    /// callbacks its `then` takes (1 or 2), if `ty` is one this lowering
    /// admits.
    pub(super) fn thenable(&self, ty: Type) -> Option<(Type, usize)> {
        let then = match ty.erased() {
            Type::Record(id) => self.types.records[id as usize]
                .fields
                .iter()
                .find(|field| field.name == "then" && !field.optional)
                .map(|field| field.ty)?,
            Type::Class(class) => {
                let method = self.types.classes.get(class).methods.get("then")?;
                if method.getter {
                    return None;
                }
                let signature = &method.signature;
                return callbacks(&signature.params, self.types);
            }
            _ => return None,
        };
        let Type::Function(id) = then else {
            return None;
        };
        let signature = &self.types.functions[id as usize];
        if signature.receiver.is_some() || signature.shape.rest {
            return None;
        }
        callbacks(&signature.params, self.types)
    }

    /// The static type of a `Promise.resolve` operand, when known without
    /// lowering it: a binding path or a `new C(...)` of a class.
    pub(super) fn thenable_operand(&self, expr: &ast::Expr) -> Option<Type> {
        self.peek_type(expr, 0)
            .or_else(|| match stmt::unparen(expr) {
                ast::Expr::New(new) => self
                    .class_callee(&new.callee)
                    .filter(|class| !self.types.classes.get(*class).is_template())
                    .map(Type::Class),
                _ => None,
            })
    }
    /// `await value` or `Promise.resolve(value)` of a thenable: the promise
    /// that `then` settles. `value` is already lowered.
    pub(super) fn resolve_thenable(&mut self, value: Expr) -> Result<Expr> {
        let span = value.span;
        let Some((fulfilled, arity)) = self.thenable(value.ty) else {
            return Err(CompilerError::Internal("missing thenable".into()));
        };
        let name = format!("{THENABLE}{}", self.next);
        self.declare(&name, Some(value.ty), true, false)?;
        let binding = self.binding(&name)?;
        let (resolve, reject) = (format!("{name}r"), format!("{name}j"));
        let mut args = vec![ast::Expr::Ident(ident(span, &resolve))];
        if arity == 2 {
            // `(reason) => reject(reason)`: the callback takes the reason
            // type `then` declares, which `reject` accepts.
            let reason = format!("{name}e");
            args.push(arrow(
                span,
                &[&reason],
                call(
                    span,
                    ast::Expr::Ident(ident(span, &reject)),
                    vec![ast::Expr::Ident(ident(span, &reason))],
                ),
            ));
        }
        let invoke = call(
            span,
            member(span, ast::Expr::Ident(ident(span, &name)), "then"),
            args,
        );
        let job = call(
            span,
            member(
                span,
                call(
                    span,
                    member(span, ast::Expr::Ident(ident(span, "Promise")), "resolve"),
                    vec![],
                ),
                "then",
            ),
            vec![arrow(span, &[], invoke)],
        );
        let executor = arrow(span, &[&resolve, &reject], job);
        let construct = ast::Expr::New(ast::NewExpr {
            span,
            ctxt: Default::default(),
            callee: Box::new(ast::Expr::Ident(ident(span, "Promise"))),
            args: Some(vec![ast::ExprOrSpread {
                spread: None,
                expr: Box::new(executor),
            }]),
            type_args: None,
        });
        let promise_ty = promises::intern(fulfilled, self.types)?;
        let promise = self.expr_expected(&construct, Some(promise_ty))?;
        let output = self.hidden(promise_ty);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty: promise_ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![],
                body: vec![S::Init(binding.slot, value), S::Init(output, promise)],
            },
        })
    }
}

/// `then(onFulfilled: (value: T) => void[, onRejected: (reason: R) => void])`:
/// the fulfilled type `T` and the callback count.
fn callbacks(params: &[Type], types: &ir::ValueTypes) -> Option<(Type, usize)> {
    if params.is_empty() || params.len() > 2 {
        return None;
    }
    let Type::Function(fulfilled) = params[0] else {
        return None;
    };
    let fulfilled = &types.functions[fulfilled as usize];
    let [value] = fulfilled.params.as_slice() else {
        return None;
    };
    if fulfilled.receiver.is_some() || fulfilled.shape.extended() {
        return None;
    }
    if let Some(rejected) = params.get(1) {
        let Type::Function(rejected) = rejected else {
            return None;
        };
        if types.functions[*rejected as usize].params.len() != 1 {
            return None;
        }
    }
    Some((*value, params.len()))
}
