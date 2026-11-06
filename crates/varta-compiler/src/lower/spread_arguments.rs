//! V38 spreads into fixed parameters (`expression_forms`).
//!
//! A spread argument whose length is static is expanded in place before the
//! ordinary call lowering: an array literal spreads its elements, and a
//! binding of a fixed-length tuple type spreads `t[0]`, ..., `t[n - 1]`
//! (reading a binding has no effect, so the reads keep the single,
//! in-order evaluation of the spread). `Math.max`/`Math.min` spread an
//! array as its fold `xs.reduce((m, x) => Math.max(m, x), -Infinity)`,
//! which is the same value for every input (NaN and signed zeros included).
//! Other spreads keep the earlier array-snapshot path.
use super::*;

fn spread_free(arg: &ast::Expr) -> ast::ExprOrSpread {
    ast::ExprOrSpread {
        spread: None,
        expr: Box::new(arg.clone()),
    }
}
fn ident(span: swc_common::Span, name: &str) -> ast::Ident {
    ast::Ident {
        span,
        ctxt: Default::default(),
        sym: name.into(),
        optional: false,
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
    /// `f(...t)` with static spreads expanded, or `Math.max(...xs)` folded.
    pub(super) fn spread_rewrite(&self, call: &ast::CallExpr) -> Option<ast::CallExpr> {
        if let Some(args) = self.expanded_arguments(&call.args) {
            return Some(ast::CallExpr {
                args,
                ..call.clone()
            });
        }
        let ast::Callee::Expr(callee) = &call.callee else {
            return None;
        };
        let ast::Expr::Member(member) = stmt::unparen(callee) else {
            return None;
        };
        let (ast::Expr::Ident(object), ast::MemberProp::Ident(method)) =
            (&*member.obj, &member.prop)
        else {
            return None;
        };
        if object.sym != "Math"
            || self.binding("Math").is_ok()
            || !matches!(method.sym.as_ref(), "max" | "min")
        {
            return None;
        }
        let identity = if method.sym == "max" {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
        let args = call
            .args
            .iter()
            .map(|arg| match arg.spread {
                None => arg.clone(),
                Some(_) => spread_free(&self.fold(call, &arg.expr, identity)),
            })
            .collect();
        Some(ast::CallExpr {
            args,
            ..call.clone()
        })
    }
    /// `xs.reduce((m: number, x: number): number => Math.max(m, x), identity)`.
    fn fold(&self, call: &ast::CallExpr, array: &ast::Expr, identity: f64) -> ast::Expr {
        let span = array.span();
        let number_type = || {
            Some(Box::new(ast::TsTypeAnn {
                span,
                type_ann: Box::new(ast::TsType::TsKeywordType(ast::TsKeywordType {
                    span,
                    kind: ast::TsKeywordTypeKind::TsNumberKeyword,
                })),
            }))
        };
        let param = |name: &str| {
            ast::Pat::Ident(ast::BindingIdent {
                id: ident(span, name),
                type_ann: number_type(),
            })
        };
        let (accumulator, element) = ("__varta_fold", "__varta_item");
        let step = ast::Expr::Call(ast::CallExpr {
            args: [accumulator, element]
                .iter()
                .map(|name| spread_free(&ast::Expr::Ident(ident(span, name))))
                .collect(),
            ..call.clone()
        });
        let reducer = ast::Expr::Arrow(ast::ArrowExpr {
            span,
            ctxt: Default::default(),
            params: vec![param(accumulator), param(element)],
            body: Box::new(ast::ArrowFunctionBody::Expr(Box::new(step))),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type: number_type(),
        });
        ast::Expr::Call(ast::CallExpr {
            span,
            ctxt: Default::default(),
            callee: ast::Callee::Expr(Box::new(ast::Expr::Member(ast::MemberExpr {
                span,
                obj: Box::new(array.clone()),
                prop: ast::MemberProp::Ident(ast::IdentName {
                    span,
                    sym: "reduce".into(),
                }),
            }))),
            args: vec![spread_free(&reducer), spread_free(&number(span, identity))],
            type_args: None,
        })
    }
}
