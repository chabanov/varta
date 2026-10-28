//! Checked array-only delete and length changes; arbitrary own properties stay closed.
use super::*;
use varta_common::value::ErrorClass;
impl Lower<'_> {
    pub(super) fn array_error(
        &mut self,
        class: ErrorClass,
        message: &str,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let target = unions::intern(vec![Type::String, Type::Undefined], self.types)?;
        let message = unions::widen(
            Expr {
                span,
                ty: Type::String,
                kind: E::String(self.types.profile.string_bytes(message)),
            },
            Some(target),
            self.types,
        )?;
        let error = Expr {
            span,
            ty: Type::from_error_class(class),
            kind: E::NewError(class, Box::new(message)),
        };
        let thrown = self.exception_type()?;
        unions::widen(error, Some(thrown), self.types)
    }
    pub(super) fn array_delete(
        &mut self,
        operand: &ast::Expr,
        span: swc_common::Span,
    ) -> Result<Expr> {
        if !self.types.profile.array_mutations() {
            return Err(unsupported("array delete requires checked-f64-utf16-v14"));
        }
        if let ast::Expr::Paren(paren) = operand {
            return self.array_delete(&paren.expr, span);
        }
        let ast::Expr::Member(member) = operand else {
            return Err(unsupported("delete of a non-array property reference"));
        };
        let array = self.write_member_base(member)?;
        if self.types.profile.property_deletion() {
            return self.property_delete(member, array, span);
        }
        if !matches!(array.ty, Type::Array(_)) {
            return Err(type_error("delete requires a narrowed array receiver"));
        }
        let key = match &member.prop {
            ast::MemberProp::Ident(id) => Some(id.sym.to_string()),
            ast::MemberProp::Computed(key) => match &*key.expr {
                ast::Expr::Lit(ast::Lit::Str(text)) => Some(
                    text.value
                        .as_str()
                        .ok_or_else(|| unsupported("noncanonical delete key"))?
                        .to_owned(),
                ),
                _ => None,
            },
            _ => return Err(unsupported("private array delete key")),
        };
        let (index, error) = if let Some(key) = key {
            if key == "length" {
                (
                    None,
                    Some(Box::new(self.array_error(
                        ErrorClass::TypeError,
                        "Cannot delete array length",
                        span,
                    )?)),
                )
            } else if let Some(index) = key
                .parse::<u32>()
                .ok()
                .filter(|index| index.to_string() == key && *index != u32::MAX)
            {
                (
                    Some(Box::new(Expr {
                        span,
                        ty: Type::Float64,
                        kind: E::Float64(index as f64),
                    })),
                    None,
                )
            } else {
                (None, None)
            }
        } else {
            let ast::MemberProp::Computed(key) = &member.prop else {
                return Err(invalid("missing array delete key"));
            };
            let index = self.expr(&key.expr)?;
            if index.ty != Type::Float64 {
                return Err(self.source.locate(type_error("array delete index requires number or literal key; coercion is unsupported"),index.span));
            }
            (Some(Box::new(index)), None)
        };
        Ok(Expr {
            span,
            ty: Type::Boolean,
            kind: E::ArrayDelete {
                array: Box::new(array),
                index,
                error,
            },
        })
    }
}
