//! Untagged template literals (v32): evaluate each substitution once, left to
//! right, apply primitive ToString, and join the parts with guest concatenation.
use super::*;

impl Lower<'_> {
    pub(super) fn template_literal(&mut self, template: &ast::Tpl) -> Result<Expr> {
        if template.quasis.len() != template.exprs.len() + 1 {
            return Err(invalid("malformed template literal"));
        }
        let mut joined: Option<Expr> = None;
        for (index, quasi) in template.quasis.iter().enumerate() {
            if let Some(index) = index.checked_sub(1) {
                let value = self.expr(&template.exprs[index])?;
                let text = self.template_text(value)?;
                joined = Some(concatenate(joined, text));
            }
            let bytes = self.template_part(quasi)?;
            if !bytes.is_empty() {
                let part = Expr {
                    span: quasi.span,
                    ty: Type::String,
                    kind: E::String(bytes),
                };
                joined = Some(concatenate(joined, part));
            }
        }
        Ok(match joined {
            Some(value) => Expr {
                span: template.span,
                ..value
            },
            None => Expr {
                span: template.span,
                ty: Type::String,
                kind: E::String(Vec::new()),
            },
        })
    }
    /// Cooked text keeps escapes and lone surrogates exactly, like string literals.
    fn template_part(&self, quasi: &ast::TplElement) -> Result<Vec<u8>> {
        let cooked = quasi.cooked.as_ref().ok_or_else(|| {
            self.source.locate(
                invalid("template literal contains an invalid escape sequence"),
                quasi.span,
            )
        })?;
        if self.types.profile.utf16() {
            return Ok(cooked
                .as_wtf8()
                .to_ill_formed_utf16()
                .flat_map(u16::to_le_bytes)
                .collect());
        }
        Ok(cooked
            .as_str()
            .ok_or_else(|| unsupported("non-UTF-8 string"))?
            .as_bytes()
            .to_vec())
    }
    /// ECMAScript ToString for admitted primitive kinds; strings pass through.
    fn template_text(&mut self, value: Expr) -> Result<Expr> {
        if value.ty == Type::String {
            return Ok(value);
        }
        if value.ty == Type::Unknown {
            let span = value.span;
            return self.unknown_string(value, span);
        }
        let bigint = self.types.profile.bigint_values();
        if let Some(object) = unions::members(value.ty, self.types)
            .into_iter()
            .find(|ty| !unions::primitive(*ty) && !(bigint && *ty == Type::BigInt))
        {
            return Err(self.source.locate(
                type_error(format!(
                    "template literal substitution requires a primitive value, found {object}; \
                     object-to-primitive conversion (ToPrimitive) is not supported"
                )),
                value.span,
            ));
        }
        let span = value.span;
        self.primitive_convert(value, &[], false, span)
    }
}

fn concatenate(left: Option<Expr>, right: Expr) -> Expr {
    match left {
        None => right,
        Some(left) => Expr {
            span: left.span,
            ty: Type::String,
            kind: E::Binary(ast::BinaryOp::Add, Box::new(left), Box::new(right)),
        },
    }
}
