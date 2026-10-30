//! Checked binding patterns, preserving source order, TDZ and exact roots.
use super::*;
mod arrays;
mod caught;
mod defaults;
mod names;
mod objects;
mod targets;
pub(super) use names::{names, targets};
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Initialize,
    Assign,
}

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
    pub(super) fn initialize_binding_pattern(
        &mut self,
        pattern: &ast::Pat,
        value: Expr,
        default: Option<&ast::Expr>,
        expected: Option<Type>,
    ) -> Result<Vec<S>> {
        self.binding_pattern(pattern, value, default, expected, Mode::Initialize)
            .map_err(|e| self.source.locate(e, pattern.span()))
    }
    pub(super) fn binding_pattern(
        &mut self,
        pattern: &ast::Pat,
        value: Expr,
        default: Option<&ast::Expr>,
        expected: Option<Type>,
        mode: Mode,
    ) -> Result<Vec<S>> {
        if let ast::Pat::Assign(assign) = pattern {
            return self.binding_pattern(&assign.left, value, Some(&assign.right), expected, mode);
        }
        if mode == Mode::Assign
            && let Some(target) = self.prepare_pattern_target(pattern)?
        {
            return self.finish_pattern_target(target, value, default, vec![]);
        }
        if let Some(default) = default {
            return self.pattern_default(pattern, value, default, expected, mode);
        }
        match pattern {
            ast::Pat::Ident(id) => {
                let name = id.id.sym.as_ref();
                let slot = self.binding(name)?.slot;
                self.update(name, value.ty)?;
                Ok(vec![S::Init(slot, value)])
            }
            ast::Pat::Object(object) => self.object_binding_pattern(object, value, mode),
            ast::Pat::Array(array) => self.array_binding_pattern(array, value, mode),
            _ => Err(unsupported("binding pattern")),
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
                matches!(ty, Type::Array(_) | Type::Tuple(_))
            } else {
                matches!(ty, Type::Record(_))
                    // V36: dictionaries and mixed records (not `object` views,
                    // whose undeclared keys would read `any`).
                    || matches!(ty, Type::Dictionary(_))
                        && self.types.profile.checked_declared_fields()
                        && !self.types.declared(*ty).is_some_and(|members| members.object)
            } =>
            {
                *ty
            }
            _ => {
                return Err(self.source.locate(
                    type_error(if array {
                        "array binding pattern requires a checked array"
                    } else {
                        "object binding pattern requires a closed record"
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
