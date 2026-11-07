use super::*;
impl Lower<'_> {
    fn pattern_field(
        &mut self,
        span: swc_common::Span,
        source: u32,
        ty: Type,
        name: &str,
    ) -> Result<Expr> {
        if let Type::Dictionary(_) = ty {
            // V36: a dictionary or mixed record property reads as `d[name]`
            // does, declared fields through their checked read.
            let key = Expr {
                span,
                ty: Type::String,
                kind: E::String(self.types.profile.string_bytes(name)),
            };
            let receiver = local(span, source, ty);
            let declared = self
                .types
                .declared(ty)
                .and_then(|members| members.fields.iter().find(|field| field.name == name))
                .cloned();
            return match declared {
                Some(field) => self.declared_get(span, receiver, key, &field),
                None => self.dictionary_get(receiver, key),
            };
        }
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
        } else if self.types.profile.strict_members() {
            Err(self
                .source
                .locate(records::undeclared(name, ty, self.types), span))
        } else {
            Ok(undefined(span))
        }
    }
    pub(super) fn object_binding_pattern(
        &mut self,
        pattern: &ast::ObjectPat,
        value: Expr,
        mode: Mode,
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
                    body.extend(self.binding_pattern(
                        &ast::Pat::Ident(prop.key.clone()),
                        field,
                        prop.value.as_deref(),
                        None,
                        mode,
                    )?);
                }
                ast::ObjectPatProp::KeyValue(prop) => {
                    let name = key(&prop.key).map_err(|(e, span)| self.source.locate(e, span))?;
                    excluded.insert(name.clone());
                    let field = self.pattern_field(prop.key.span(), source, ty, &name)?;
                    body.extend(self.binding_pattern(&prop.value, field, None, None, mode)?);
                }
                ast::ObjectPatProp::Rest(prop) => {
                    let prepared = if mode == Mode::Assign {
                        self.prepare_pattern_target(&prop.arg)?
                    } else {
                        None
                    };
                    let mut rest_body = Vec::new();
                    let Type::Record(from) = ty else {
                        return Err(unsupported("rest element of a dictionary binding pattern"));
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
                    // V38 (`object_accessors`): with accessor objects in the
                    // program, a rest of required fields reads the source in
                    // its own-key order (a getter runs when its key is
                    // reached) and takes the source's key order.
                    let keyed = self.types.receivers.has_accessors()
                        && !self.types.records[from as usize]
                            .fields
                            .iter()
                            .any(|field| field.optional);
                    let record = ty;
                    let cached = if keyed {
                        let (reads, locals) = self.read_in_key_order(
                            source,
                            from,
                            &excluded.iter().cloned().collect::<Vec<_>>(),
                            span,
                        )?;
                        roots.extend(locals.iter().copied());
                        rest_body.extend(reads);
                        Some(locals)
                    } else {
                        None
                    };
                    let fields = mapping
                        .iter()
                        .map(|(old, new)| {
                            let ty = self.types.records[from as usize].fields[*old as usize].ty;
                            let kind = match &cached {
                                Some(locals) => E::Local(locals[*old as usize]),
                                None => E::Field(Box::new(local(span, source, record)), *old),
                            };
                            (*new, Expr { span, ty, kind })
                        })
                        .collect();
                    let output = self.hidden(result);
                    roots.push(output);
                    rest_body.push(S::Init(
                        output,
                        Expr {
                            span,
                            ty: result,
                            kind: E::Record(fields),
                        },
                    ));
                    if keyed {
                        let slots = [
                            self.hidden(Type::Number),
                            self.hidden(Type::Number),
                            self.hidden(Type::Number),
                        ];
                        rest_body.push(S::RecordOrder {
                            target: output,
                            shape: to,
                            source: Some((source, from)),
                            mapping,
                            start: true,
                            slots,
                        });
                    } else {
                        rest_body.push(S::RecordRest {
                            source,
                            target: output,
                            from,
                            to,
                            mapping,
                            index: self.hidden(Type::Number),
                            cursor: self.hidden(Type::Number),
                        });
                    }
                    if let Some(target) = prepared {
                        body.extend(self.finish_pattern_target(
                            target,
                            local(span, output, result),
                            None,
                            rest_body,
                        )?);
                    } else {
                        body.extend(rest_body);
                        body.extend(self.binding_pattern(
                            &prop.arg,
                            local(span, output, result),
                            None,
                            None,
                            mode,
                        )?);
                    }
                }
            }
        }
        Ok(vec![S::Scope(roots, body)])
    }
}
