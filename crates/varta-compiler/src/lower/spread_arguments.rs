//! V38 spreads into fixed parameters (`expression_forms`).
//!
//! A spread argument whose length is static is expanded in place before the
//! ordinary call lowering: an array literal spreads its elements, and a
//! binding of a fixed-length tuple type spreads `t[0]`, ..., `t[n - 1]`
//! (reading a binding has no effect, so the reads keep the single,
//! in-order evaluation of the spread). Other spreads keep the
//! array-snapshot path of rest parameters and the standard library's
//! `Math.max(...xs)` (`standard_library`).
use super::*;

fn spread_free(arg: &ast::Expr) -> ast::ExprOrSpread {
    ast::ExprOrSpread {
        spread: None,
        expr: Box::new(arg.clone()),
    }
}
fn number(span: swc_common::Span, value: f64) -> ast::Expr {
    ast::Expr::Lit(ast::Lit::Num(ast::Number {
        span,
        value,
        raw: None,
    }))
}
/// The operand of an assertion or parentheses.
fn operand(mut expr: &ast::Expr) -> &ast::Expr {
    loop {
        expr = match expr {
            ast::Expr::Paren(paren) => &paren.expr,
            ast::Expr::TsAs(assertion) => &assertion.expr,
            ast::Expr::TsConstAssertion(assertion) => &assertion.expr,
            ast::Expr::TsSatisfies(assertion) => &assertion.expr,
            _ => return expr,
        };
    }
}

impl Lower<'_> {
    /// The fixed elements of a spread operand whose length is static.
    fn static_spread(&self, expr: &ast::Expr) -> Option<Vec<ast::ExprOrSpread>> {
        match operand(expr) {
            ast::Expr::Array(array) => {
                let mut elements = Vec::with_capacity(array.elems.len());
                for element in &array.elems {
                    let element = element.as_ref()?;
                    match &element.spread {
                        None => elements.push(spread_free(&element.expr)),
                        Some(_) => elements.extend(self.static_spread(&element.expr)?),
                    }
                }
                Some(elements)
            }
            ast::Expr::Ident(id) => {
                let Some(Type::Tuple(tuple)) = self.peek_type(expr, 0) else {
                    return None;
                };
                let tuple = &self.types.tuples[tuple as usize];
                if tuple.required != tuple.elements.len() {
                    return None;
                }
                Some(
                    (0..tuple.elements.len())
                        .map(|index| {
                            spread_free(&ast::Expr::Member(ast::MemberExpr {
                                span: id.span,
                                obj: Box::new(ast::Expr::Ident(id.clone())),
                                prop: ast::MemberProp::Computed(ast::ComputedPropName {
                                    span: id.span,
                                    expr: Box::new(number(id.span, index as f64)),
                                }),
                            }))
                        })
                        .collect(),
                )
            }
            _ => None,
        }
    }
    /// The arguments with every static spread expanded, if any was.
    pub(super) fn expanded_arguments(
        &self,
        args: &[ast::ExprOrSpread],
    ) -> Option<Vec<ast::ExprOrSpread>> {
        let mut expanded = false;
        let mut result = Vec::with_capacity(args.len());
        for arg in args {
            match arg.spread.map(|_| self.static_spread(&arg.expr)) {
                Some(Some(elements)) => {
                    expanded = true;
                    result.extend(elements);
                }
                _ => result.push(arg.clone()),
            }
        }
        expanded.then_some(result)
    }
    /// `f(...t)` with its static spreads expanded.
    pub(super) fn spread_rewrite(&self, call: &ast::CallExpr) -> Option<ast::CallExpr> {
        let args = self.expanded_arguments(&call.args)?;
        Some(ast::CallExpr {
            args,
            ..call.clone()
        })
    }
}
