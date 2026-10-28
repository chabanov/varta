use super::*;
use crate::ir::ResultField;

impl Lower<'_> {
    pub(super) fn capture(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        if call.args.len() != 1 {
            return Err(invalid("varta.result.capture expects one direct host call"));
        }
        let value = self.expr(&call.args[0].expr)?;
        let value = match value {
            Expr {
                kind: E::Undefined(inner),
                ..
            } if inner.ty == Type::Void => *inner,
            other => other,
        };
        if !matches!(&value.kind, E::Host(host, _) if host.capability().is_some()) {
            return Err(self.source.locate(
                unsupported("capture argument other than a direct privileged host call"),
                value.span,
            ));
        }
        let ty = value
            .ty
            .captured()
            .ok_or_else(|| invalid("unsupported captured value type"))?;
        Ok(Expr {
            span: call.span,
            ty,
            kind: E::Capture(Box::new(value)),
        })
    }

    pub(super) fn result_field(&mut self, member: &ast::MemberExpr) -> Result<Expr> {
        if let Some(value) = self.numeric_constant(member)? {
            return Ok(value);
        }
        let value = self.expr(&member.obj)?;
        if self.error_value(value.ty) {
            return self.error_field(member, value);
        }
        if value.ty == Type::String && self.types.profile.utf16() {
            if self.types.profile.missing()
                && let ast::MemberProp::Computed(key) = &member.prop
            {
                return self.maybe_index(member, value, &key.expr, Type::String);
            }
            if matches!(&member.prop, ast::MemberProp::Ident(n) if n.sym == "length") {
                return Ok(Expr {
                    span: member.span,
                    ty: self.types.number(),
                    kind: E::Length(Box::new(value)),
                });
            }
            return Err(unsupported(
                "string property; use length, charAt or charCodeAt",
            ));
        }
        if matches!(value.ty, Type::Array(_)) {
            return self.array_member(member, value, true);
        }
        if matches!(value.ty, Type::Record(_)) {
            return self.record_field(member, value, true);
        }
        let payload = value.ty.result_value().ok_or_else(|| {
            self.source.locate(
                unsupported("property access on a non-result value"),
                value.span,
            )
        })?;
        let ast::MemberProp::Ident(name) = &member.prop else {
            return Err(unsupported("computed result property"));
        };
        let (field, ty) = match name.sym.as_ref() {
            "ok" => (ResultField::Ok, Type::Boolean),
            "status" => (ResultField::Status, self.types.number()),
            "value" => (
                ResultField::Value,
                if payload == Type::Void && self.types.profile.has_undefined() {
                    Type::Undefined
                } else {
                    payload
                },
            ),
            _ => {
                return Err(self.source.locate(
                    invalid(format!("unknown result property: {}", name.sym)),
                    name.span,
                ));
            }
        };
        Ok(Expr {
            span: member.span,
            ty,
            kind: E::ResultField(field, Box::new(value)),
        })
    }
}
