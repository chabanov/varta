//! Catch inputs have the complete primitive/Error channel type. Do not infer a
//! narrower type from the protected body or admit aggregate throws implicitly.
use super::*;
impl Lower<'_> {
    pub(in crate::lower) fn catch_binding_pattern(
        &mut self,
        pattern: &ast::ObjectPat,
        source: u32,
        ty: Type,
    ) -> Result<Vec<S>> {
        let span = pattern.span;
        let error = self.array_error(
            varta_common::value::ErrorClass::TypeError,
            "Cannot destructure null or undefined",
            span,
        )?;
        let mut body = vec![S::If(
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::IsNullish(Box::new(local(span, source, ty))),
            },
            vec![S::Throw(error)],
            vec![],
        )];
        for prop in &pattern.props {
            match prop {
                ast::ObjectPatProp::Assign(prop) => {
                    let field =
                        self.caught_property(prop.span, source, ty, prop.key.id.sym.as_ref())?;
                    body.extend(self.binding_pattern(
                        &ast::Pat::Ident(prop.key.clone()),
                        field,
                        prop.value.as_deref(),
                        None,
                        Mode::Initialize,
                    )?);
                }
                ast::ObjectPatProp::KeyValue(prop) => {
                    let name = key(&prop.key).map_err(|(e, span)| self.source.locate(e, span))?;
                    let field = self.caught_property(prop.key.span(), source, ty, &name)?;
                    body.extend(self.binding_pattern(
                        &prop.value,
                        field,
                        None,
                        None,
                        Mode::Initialize,
                    )?);
                }
                ast::ObjectPatProp::Rest(prop) => {
                    return Err(self.source.locate(
                        unsupported("catch object rest requires broader caught-value enumeration"),
                        prop.span,
                    ));
                }
            }
        }
        Ok(body)
    }
    fn caught_property(
        &mut self,
        span: swc_common::Span,
        source: u32,
        ty: Type,
        name: &str,
    ) -> Result<Expr> {
        let value = Box::new(local(span, source, ty));
        let kind = match name {
            "name" => E::ErrorName(value),
            "message" => E::ErrorMessage(value),
            _ => {
                return Err(self.source.locate(
                    unsupported("catch object property other than name/message"),
                    span,
                ));
            }
        };
        let result = unions::intern(vec![Type::String, Type::Undefined], self.types)?;
        let present = unions::widen(
            Expr {
                span,
                ty: Type::String,
                kind,
            },
            Some(result),
            self.types,
        )?;
        let absent = unions::widen(undefined(span), Some(result), self.types)?;
        Ok(Expr {
            span,
            ty: result,
            kind: E::Conditional(
                Box::new(Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsError(Box::new(local(span, source, ty))),
                }),
                Box::new(present),
                Box::new(absent),
            ),
        })
    }
}
