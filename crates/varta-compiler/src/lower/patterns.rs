//! Checked parameter binding, preserving source order, TDZ and exact roots.
use super::*;
mod arrays;
mod defaults;
mod names;
mod objects;
pub(super) use names::names;

fn key(name: &ast::PropName) -> std::result::Result<String, (CompilerError, swc_common::Span)> {
    if let ast::PropName::Computed(computed) = name {
        let mut expr = &*computed.expr;
        while let ast::Expr::Paren(paren) = expr {
            expr = &paren.expr;
        }
        if !matches!(expr, ast::Expr::Lit(ast::Lit::Str(_) | ast::Lit::Num(_))) {
            return Err((unsupported("dynamic object binding key"), expr.span()));
        }
        records::expression_key(expr).map_err(|e| (e, expr.span()))
    } else {
        records::property_key(name).map_err(|e| (e, name.span()))
    }
}

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn number(span: swc_common::Span, value: usize) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Float64(value as f64),
    }
}
fn undefined(span: swc_common::Span) -> Expr {
    Expr {
        span,
        ty: Type::Undefined,
        kind: E::Integer(0),
    }
}
fn binary(span: swc_common::Span, op: ast::BinaryOp, a: Expr, b: Expr, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Binary(op, Box::new(a), Box::new(b)),
    }
}
impl Lower<'_> {
    pub(super) fn initialize_parameter_pattern(
        &mut self,
        pattern: &ast::Pat,
        value: Expr,
        default: Option<&ast::Expr>,
        expected: Option<Type>,
    ) -> Result<Vec<S>> {
        self.parameter_pattern_inner(pattern, value, default, expected)
            .map_err(|e| self.source.locate(e, pattern.span()))
    }
    fn parameter_pattern_inner(
        &mut self,
        pattern: &ast::Pat,
        value: Expr,
        default: Option<&ast::Expr>,
        expected: Option<Type>,
    ) -> Result<Vec<S>> {
        if let Some(default) = default {
            return self.pattern_default(pattern, value, default, expected);
        }
        match pattern {
            ast::Pat::Ident(id) => {
                let name = id.id.sym.as_ref();
                let slot = self.binding(name)?.slot;
                self.update(name, value.ty)?;
                Ok(vec![S::Init(slot, value)])
            }
            ast::Pat::Assign(assign) => {
                self.pattern_default(&assign.left, value, &assign.right, None)
            }
            ast::Pat::Object(object) => self.object_parameter_pattern(object, value),
            ast::Pat::Array(array) => self.array_parameter_pattern(array, value),
            _ => Err(unsupported("parameter binding pattern")),
        }
    }
    /// Own the input before testing/narrowing. Nested reads may allocate or throw.
    fn pattern_source(
        &mut self,
        value: Expr,
        array: bool,
    ) -> Result<(u32, Type, Vec<u32>, Vec<S>)> {
        let span = value.span;
        let members = unions::members(value.ty, self.types);
        let present: Vec<_> = members
            .iter()
            .copied()
            .filter(|t| !matches!(t, Type::Null | Type::Undefined))
            .collect();
        let ty = match present.as_slice() {
            [ty] if if array {
                matches!(ty, Type::Array(_))
            } else {
                matches!(ty, Type::Record(_))
            } =>
            {
                *ty
            }
            _ => {
                return Err(self.source.locate(
                    type_error(if array {
                        "array parameter pattern requires a checked array"
                    } else {
                        "object parameter pattern requires a closed record"
                    }),
                    span,
                ));
            }
        };
        let input = self.hidden(value.ty);
        let input_ty = value.ty;
        let mut roots = vec![input];
        let mut body = vec![S::Init(input, value)];
        if input_ty == ty {
            return Ok((input, ty, roots, body));
        }
        let test = Expr {
            span,
            ty: Type::Boolean,
            kind: E::IsNullish(Box::new(local(span, input, input_ty))),
        };
        let error = self.array_error(
            varta_common::value::ErrorClass::TypeError,
            "Cannot destructure null or undefined",
            span,
        )?;
        body.push(S::If(test, vec![S::Throw(error)], vec![]));
        let source = self.hidden(ty);
        roots.push(source);
        body.push(S::Init(
            source,
            Expr {
                span,
                ty,
                kind: E::UnboxUnion(Box::new(local(span, input, input_ty))),
            },
        ));
        Ok((source, ty, roots, body))
    }
}
