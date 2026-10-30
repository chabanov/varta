//! Dense element typing (v37 `language_core`).
//!
//! TypeScript (without `noUncheckedIndexedAccess`) types `xs[i]` and the
//! variable of `for (const x of xs)` as the element type `T`, and `s[i]` as
//! `string`. Varta v13+ arrays may have holes and indices may be out of range,
//! where JavaScript reads `undefined`. Under v37 these reads have the element
//! type through a checked read: a hole or an out-of-range index throws a
//! `TypeError` instead of producing `undefined` (a documented deviation from
//! Node, which never throws there). Element types that admit `undefined` read
//! as before, without a check. Earlier profiles keep `T | undefined`.
use super::*;
use std::collections::BTreeSet;
use varta_common::value::ErrorClass;

/// The `TypeError` message of a missing array element.
pub(super) const MISSING_ELEMENT: &str =
    "array element is missing (a hole or an index out of range)";
/// The `TypeError` message of a string index out of range.
pub(super) const MISSING_UNIT: &str = "string index out of range";
/// The `TypeError` message of a hole visited by `for...of`.
pub(super) const ITERATED_HOLE: &str = "array element is a hole";

/// The identifiers `statements` compare with `undefined` anywhere inside
/// (`x === undefined`, `x != undefined`, `typeof x === 'undefined'`, either
/// operand order): TypeScript types `const x = xs[i]` as the element type,
/// and such code relies on JavaScript reading `undefined` there.
pub(super) fn undefined_comparisons(statements: &[ast::Stmt]) -> BTreeSet<String> {
    use swc_ecma_visit::{Visit, VisitWith};
    fn undefined(expr: &ast::Expr) -> bool {
        match stmt::unparen(expr) {
            ast::Expr::Ident(id) => id.sym == "undefined",
            ast::Expr::Lit(ast::Lit::Str(text)) => text.value.as_str() == Some("undefined"),
            ast::Expr::Unary(unary) => unary.op == ast::UnaryOp::Void,
            _ => false,
        }
    }
    fn operand(expr: &ast::Expr) -> Option<String> {
        match stmt::unparen(expr) {
            ast::Expr::Ident(id) => Some(id.sym.to_string()),
            ast::Expr::Unary(unary) if unary.op == ast::UnaryOp::TypeOf => {
                match stmt::unparen(&unary.arg) {
                    ast::Expr::Ident(id) => Some(id.sym.to_string()),
                    _ => None,
                }
            }
            _ => None,
        }
    }
    #[derive(Default)]
    struct Names(BTreeSet<String>);
    impl Visit for Names {
        fn visit_bin_expr(&mut self, binary: &ast::BinExpr) {
            use ast::BinaryOp::{EqEq, EqEqEq, NotEq, NotEqEq};
            if matches!(binary.op, EqEq | EqEqEq | NotEq | NotEqEq) {
                for (name, other) in [(&binary.left, &binary.right), (&binary.right, &binary.left)]
                {
                    if undefined(other)
                        && let Some(name) = operand(name)
                    {
                        self.0.insert(name);
                    }
                }
            }
            binary.visit_children_with(self);
        }
    }
    let mut names = Names::default();
    for statement in statements {
        statement.visit_with(&mut names);
    }
    names.0
}

impl Lower<'_> {
    /// Marks `expression`, when it is an index read `xs[i]`/`s[i]` lowered
    /// next in a context that admits `undefined` (an expected type with an
    /// `undefined` member, or the left operand of `??`), to read
    /// `T | undefined` without the check, as JavaScript does.
    pub(super) fn optional_index(&mut self, expression: &ast::Expr) {
        if !self.types.profile.language_core() {
            return;
        }
        if let ast::Expr::Member(member) = stmt::unparen(expression)
            && matches!(member.prop, ast::MemberProp::Computed(_))
        {
            self.core.optional_index = Some(member.span);
        }
    }
    /// Whether the index read at `span` was marked by `optional_index`.
    pub(super) fn take_optional_index(&mut self, span: swc_common::Span) -> bool {
        if self.core.optional_index == Some(span) {
            self.core.optional_index = None;
            return true;
        }
        false
    }
    /// `maybe` (of type `element | undefined`) as `element`, throwing
    /// `TypeError(message)` when it is `undefined`.
    pub(super) fn dense_read(&mut self, maybe: Expr, element: Type, message: &str) -> Result<Expr> {
        // `unknown` holds `undefined` itself, so a hole reads as one.
        if element == Type::Unknown
            || unions::members(element, self.types).contains(&Type::Undefined)
        {
            return Ok(maybe);
        }
        let span = maybe.span;
        let source = maybe.ty;
        let saved = self.hidden(source);
        let error = self.array_error(ErrorClass::TypeError, message, span)?;
        let read = Expr {
            span,
            ty: source,
            kind: E::Local(saved),
        };
        let converted = if element.is_cell() {
            Expr {
                ty: element,
                ..read
            }
        } else {
            Expr {
                span,
                ty: element,
                kind: E::UnboxUnion(Box::new(read)),
            }
        };
        let output = self.hidden(element);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty: element,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![saved],
                body: vec![
                    S::Init(saved, maybe),
                    S::If(
                        Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::IsUndefined(Box::new(Expr {
                                span,
                                ty: source,
                                kind: E::Local(saved),
                            })),
                        },
                        vec![S::Throw(error)],
                        vec![],
                    ),
                    S::Init(output, converted),
                ],
            },
        })
    }
}
