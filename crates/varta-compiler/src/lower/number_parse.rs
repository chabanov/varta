//! Cache every argument before converting primitive input and optional radix.
use super::*;
impl Lower<'_> {
    pub(super) fn number_parse(&mut self, call: &ast::CallExpr, integer: bool) -> Result<Expr> {
        if !self.types.profile.number_parse() {
            return Err(unsupported(
                "numeric prefix parsing requires checked-f64-utf16-v23",
            ));
        }
        if call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(unsupported("spread to a numeric parser"));
        }
        let span = call.span;
        if call.args.is_empty() {
            return Ok(Expr {
                span,
                ty: Type::Float64,
                kind: E::Float64(f64::NAN),
            });
        }
        let count = if integer { 2 } else { 1 };
        let mut body = Vec::new();
        let mut roots = Vec::new();
        let mut inputs = Vec::new();
        for (index, arg) in call.args.iter().enumerate() {
            let value = self.expr(&arg.expr)?;
            if index >= count {
                body.push(S::Expr(value));
                continue;
            }
            if unions::members(value.ty, self.types)
                .iter()
                .any(|ty| !unions::primitive(*ty))
            {
                return Err(self.source.locate(
                    type_error(
                        "numeric parsing requires primitives; object conversion is unsupported",
                    ),
                    value.span,
                ));
            }
            let ty = value.ty;
            let slot = self.hidden(ty);
            roots.push(slot);
            body.push(S::Init(slot, value));
            inputs.push(Expr {
                span,
                ty,
                kind: E::Local(slot),
            });
        }
        if integer && inputs.len() == 1 {
            inputs.push(Expr {
                span,
                ty: Type::Undefined,
                kind: E::Integer(0),
            });
        }
        let target = unions::intern(
            vec![
                Type::Float64,
                Type::Boolean,
                Type::String,
                Type::Null,
                Type::Undefined,
            ],
            self.types,
        )?;
        let inputs = inputs
            .into_iter()
            .map(|value| unions::widen(value, Some(target), self.types))
            .collect::<Result<Vec<_>>>()?;
        let output = self.hidden(Type::Float64);
        roots.push(output);
        let marker = self.hidden(Type::Number);
        body.push(S::Init(
            output,
            Expr {
                span,
                ty: Type::Float64,
                kind: E::Host(
                    if integer {
                        Host::NumberParseInt
                    } else {
                        Host::NumberParseFloat
                    },
                    inputs,
                ),
            },
        ));
        Ok(Expr {
            span,
            ty: Type::Float64,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }
}
