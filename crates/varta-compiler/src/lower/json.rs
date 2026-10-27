use super::*;

impl Lower<'_> {
    pub(super) fn parse_json(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        if call.args.len() != 1 || call.args[0].spread.is_some() {
            return Err(invalid("varta.json.parse<T> expects one string argument"));
        }
        let arguments = call
            .type_args
            .as_ref()
            .filter(|a| a.params.len() == 1)
            .ok_or_else(|| invalid("varta.json.parse requires exactly one checked value type"))?;
        let target = &arguments.params[0];
        let ty =
            keyword_type(target, self.types).map_err(|e| self.source.locate(e, target.span()))?;
        fn json_type(ty: Type, types: &ir::ValueTypes, optional: bool) -> bool {
            match ty {
                Type::Error | Type::NativeError(_) => false,
                Type::Undefined => types.profile.checked_json() && optional,
                Type::Union(id) | Type::ReferenceUnion(id) => {
                    types.profile.checked_json()
                        && types.unions[id as usize]
                            .iter()
                            .all(|m| json_type(*m, types, true))
                }
                Type::Record(id) => types.records[id as usize]
                    .fields
                    .iter()
                    .all(|f| json_type(f.ty, types, f.optional)),
                Type::Array(id) => json_type(types.arrays[id as usize], types, false),
                _ => ty.value_kind().is_some(),
            }
        }
        if !json_type(ty, self.types, false) {
            return Err(self
                .source
                .locate(unsupported("JSON parsing target type"), target.span()));
        }
        let text = self.expr(&call.args[0].expr)?;
        if text.ty != Type::String {
            return Err(self.source.locate(
                type_error(format!(
                    "JSON input type mismatch: expected string, found {}",
                    text.ty
                )),
                text.span,
            ));
        }
        Ok(Expr {
            span: call.span,
            ty,
            kind: E::ParseJson(Box::new(text)),
        })
    }
}
