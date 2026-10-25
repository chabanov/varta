use super::*;
use crate::ir::ResultField;

impl Lower<'_> {
    pub(super) fn capture(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        if call.args.len() != 1 {
            return Err(invalid("varta.result.capture expects one direct host call"));
        }
        let value = self.expr(&call.args[0].expr)?;
        if !matches!(&value.kind, E::Host(host, _) if !matches!(host, Host::LogInt | Host::LogString))
        {
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
        let value = self.expr(&member.obj)?;
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
            "status" => (ResultField::Status, Type::Number),
            "value" => (ResultField::Value, payload),
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
