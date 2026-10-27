use super::*;
impl Lower<'_> {
    fn pattern_field(
        &self,
        span: swc_common::Span,
        source: u32,
        ty: Type,
        name: &str,
    ) -> Result<Expr> {
        let Type::Record(id) = ty else {
            return Err(invalid("missing pattern record"));
        };
        let found = self.types.records[id as usize]
            .fields
            .iter()
            .position(|field| field.name == name);
        if let Some(slot) = found {
            Ok(Expr {
                span,
                ty: self.types.records[id as usize].fields[slot].ty,
                kind: E::Field(Box::new(local(span, source, ty)), slot as u32),
            })
        } else if records::inherited(name) {
            Err(self
                .source
                .locate(unsupported("inherited object binding property"), span))
        } else {
            Ok(undefined(span))
        }
    }
    pub(super) fn object_parameter_pattern(
        &mut self,
        pattern: &ast::ObjectPat,
        value: Expr,
    ) -> Result<Vec<S>> {
        let span = pattern.span;
        let (source, ty, mut roots, mut body) = self.pattern_source(value, false)?;
        let mut excluded = std::collections::BTreeSet::new();
        for prop in &pattern.props {
            match prop {
                ast::ObjectPatProp::Assign(prop) => {
                    let name = prop.key.id.sym.to_string();
                    excluded.insert(name.clone());
                    let field = self.pattern_field(prop.span, source, ty, &name)?;
                    body.extend(self.initialize_parameter_pattern(
                        &ast::Pat::Ident(prop.key.clone()),
                        field,
                        prop.value.as_deref(),
                        None,
                    )?);
                }
                ast::ObjectPatProp::KeyValue(prop) => {
                    let name = key(&prop.key).map_err(|(e, span)| self.source.locate(e, span))?;
                    excluded.insert(name.clone());
                    let field = self.pattern_field(prop.key.span(), source, ty, &name)?;
                    body.extend(self.initialize_parameter_pattern(
                        &prop.value,
                        field,
                        None,
                        None,
                    )?);
                }
                ast::ObjectPatProp::Rest(prop) => {
                    let Type::Record(from) = ty else {
                        return Err(invalid("missing source record"));
                    };
                    let fields: Vec<_> = self.types.records[from as usize]
                        .fields
                        .iter()
                        .filter(|field| !excluded.contains(&field.name))
                        .cloned()
                        .collect();
                    let result = records::intern(fields, self.types)?;
                    let Type::Record(to) = result else {
                        return Err(invalid("missing rest record"));
                    };
                    let mapping: Vec<_> = self.types.records[to as usize]
                        .fields
                        .iter()
                        .enumerate()
                        .map(|(new, field)| {
                            let old = self.types.records[from as usize]
                                .fields
                                .iter()
                                .position(|old| old.name == field.name)
                                .ok_or_else(|| invalid("missing rest field"))?;
                            Ok((old as u32, new as u32))
                        })
                        .collect::<Result<_>>()?;
                    let fields = mapping
                        .iter()
                        .map(|(old, new)| {
                            (
                                *new,
                                Expr {
                                    span,
                                    ty: self.types.records[from as usize].fields[*old as usize].ty,
                                    kind: E::Field(Box::new(local(span, source, ty)), *old),
                                },
                            )
                        })
                        .collect();
                    let output = self.hidden(result);
                    roots.push(output);
                    body.push(S::Init(
                        output,
                        Expr {
                            span,
                            ty: result,
                            kind: E::Record(fields),
                        },
                    ));
                    body.push(S::RecordRest {
                        source,
                        target: output,
                        from,
                        to,
                        mapping,
                        index: self.hidden(Type::Number),
                        cursor: self.hidden(Type::Number),
                    });
                    body.extend(self.initialize_parameter_pattern(
                        &prop.arg,
                        local(span, output, result),
                        None,
                        None,
                    )?);
                }
            }
        }
        Ok(vec![S::Scope(roots, body)])
    }
}
