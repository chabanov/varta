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
        if literals::contains(ty, self.types) {
            // The schema ABI cannot express value sets; unchecked members would
            // otherwise reach literal-typed places.
            return Err(self.source.locate(
                unsupported("JSON parsing target with a literal type; parse the base primitive and narrow it"),
                target.span(),
            ));
        }
        fn json_type(ty: Type, types: &ir::ValueTypes, optional: bool) -> bool {
            json_visit(ty, types, optional, &mut std::collections::BTreeSet::new())
        }
        // Recursive types (v34) make the type graph cyclic: visit each type once.
        fn json_visit(
            ty: Type,
            types: &ir::ValueTypes,
            optional: bool,
            seen: &mut std::collections::BTreeSet<(Type, bool)>,
        ) -> bool {
            if !seen.insert((ty, optional)) {
                return true;
            }
            let mut json_type = |ty, types, optional| json_visit(ty, types, optional, seen);
            match ty {
                // A schema needs a closed type; `unknown` (v34) has none.
                Type::Literal(..) | Type::Unknown => false,
                Type::BigInt => types.profile.bigint_values(),
                // Parsing cannot construct nominal class instances.
                Type::Error | Type::NativeError(_) | Type::Class(_) => false,
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
                Type::Dictionary(id) => {
                    json_type(types.dictionaries[id as usize].value, types, true)
                }
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
