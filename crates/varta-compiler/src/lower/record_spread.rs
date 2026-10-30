//! Object spread into a closed record literal (v38 `structural_subtyping`):
//! `{ ...fields, key: value }` where every spread source is a closed record.
//!
//! Members are evaluated once, in source order, into hidden bindings. The
//! literal's type has every field of every member: a field is required when
//! some member provides it unconditionally (an initializer or a required
//! source field); its value is the last such member's, overridden by every
//! later optional source field that is present. A field that only optional
//! source fields provide is optional and present when one of them is. In a
//! record context the result is then viewed as the expected type, so a
//! literal that spreads a wider record keeps the extra fields, as in
//! JavaScript.
//!
//! Key order: fields present in every case come first, in the order of their
//! first member; optional fields follow in member order (JavaScript orders
//! keys by first definition, which a spread source fixes only at run time).
use super::*;
use ir::RecordField;

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}

/// One member providing a field.
#[derive(Clone, Copy)]
enum Provider {
    /// An initializer, in its hidden binding.
    Value { slot: u32, ty: Type },
    /// A field of a spread source in its hidden binding; a `nullable`
    /// source binding holds the source or `undefined`/`null` (in a cell).
    Spread {
        slot: u32,
        shape: u32,
        field: u32,
        ty: Type,
        optional: bool,
        nullable: Option<Type>,
    },
}
impl Provider {
    fn ty(self) -> Type {
        match self {
            Self::Value { ty, .. } | Self::Spread { ty, .. } => ty,
        }
    }
    fn certain(self) -> bool {
        match self {
            Self::Value { .. } => true,
            Self::Spread {
                optional, nullable, ..
            } => !optional && nullable.is_none(),
        }
    }
}

impl Lower<'_> {
    pub(super) fn record_spread(
        &mut self,
        object: &ast::ObjectLit,
        expected: Option<Type>,
    ) -> Result<Expr> {
        let span = object.span;
        let expected_field = |this: &Self, name: &str| match expected {
            Some(Type::Record(id)) => this.types.records[id as usize]
                .fields
                .iter()
                .find(|f| f.name == name)
                .map(|f| f.ty),
            _ => None,
        };
        let mut body = Vec::new();
        let mut roots = Vec::new();
        // Field names in order of their first member, with their providers.
        let mut providers: Vec<(String, Vec<Provider>)> = Vec::new();
        let mut provide =
            |name: &str, provider: Provider| match providers.iter_mut().find(|(n, _)| n == name) {
                Some((_, list)) => list.push(provider),
                None => providers.push((name.to_owned(), vec![provider])),
            };
        for property in &object.props {
            match property {
                ast::PropOrSpread::Spread(spread) => {
                    let value = self.expr(&spread.expr)?;
                    // `...optional` spreads nothing when it is nullish.
                    let present: Vec<Type> = unions::members(value.ty, self.types)
                        .into_iter()
                        .filter(|member| !matches!(member, Type::Undefined | Type::Null))
                        .collect();
                    let nullable = (value.ty.is_union() && present.len() == 1).then_some(value.ty);
                    let source = match nullable {
                        Some(_) => present[0],
                        None => value.ty,
                    };
                    let Type::Record(shape) = source else {
                        return Err(self.source.locate(
                            unsupported(&format!(
                                "object spread of a {} into a record; spread closed records, \
                                 or spread into a dictionary context",
                                subtyping::describe(value.ty, self.types)
                            )),
                            spread.expr.span(),
                        ));
                    };
                    if self.types.records[shape as usize].class.is_some() {
                        return Err(unsupported("object spread of a class instance"));
                    }
                    let slot = self.hidden(value.ty);
                    roots.push(slot);
                    body.push(S::Init(slot, value));
                    for (field, declared) in self.types.records[shape as usize]
                        .fields
                        .clone()
                        .into_iter()
                        .enumerate()
                    {
                        provide(
                            &declared.name,
                            Provider::Spread {
                                slot,
                                shape,
                                field: field as u32,
                                ty: declared.ty,
                                optional: declared.optional,
                                nullable,
                            },
                        );
                    }
                }
                ast::PropOrSpread::Prop(property) => {
                    let (name, value) = match &**property {
                        ast::Prop::KeyValue(field) => {
                            let name = records::property_key(&field.key)?;
                            let ty = expected_field(self, &name);
                            (name, self.expr_expected(&field.value, ty)?)
                        }
                        ast::Prop::Shorthand(id) => {
                            let name = records::key(id.sym.to_string())?;
                            let ty = expected_field(self, &name);
                            (name, self.expr_expected(&ast::Expr::Ident(id.clone()), ty)?)
                        }
                        ast::Prop::Method(method) => {
                            let name = records::property_key(&method.key)?;
                            let ty = expected_field(self, &name);
                            (name, self.object_method_value(method, ty)?)
                        }
                        _ => return Err(unsupported("object getter or setter")),
                    };
                    let ty = value.ty;
                    let slot = self.hidden(ty);
                    if ty.needs_root() {
                        roots.push(slot);
                    }
                    body.push(S::Init(slot, value));
                    provide(&name, Provider::Value { slot, ty });
                }
            }
        }
        if providers.len() > self.types.max_record_fields() {
            return Err(invalid("object field count exceeds limit"));
        }
        // The merged field types: from the last certain provider on.
        let mut merged = Vec::new();
        let mut plans = Vec::new();
        for (name, list) in providers {
            let start = list.iter().rposition(|p| p.certain());
            let relevant: Vec<Provider> = list[start.unwrap_or(0)..].to_vec();
            let mut members = Vec::new();
            for provider in &relevant {
                for member in unions::members(provider.ty(), self.types) {
                    if !members.contains(&member) {
                        members.push(member);
                    }
                }
            }
            let ty = if members.len() == 1 {
                members[0]
            } else {
                unions::intern(members, self.types)?
            };
            merged.push(RecordField {
                name: name.clone(),
                ty,
                optional: start.is_none(),
            });
            plans.push((name, relevant, start.is_some()));
        }
        let record = records::intern(merged, self.types)?;
        let Type::Record(id) = record else {
            return Err(CompilerError::Internal("spread record type".into()));
        };
        let shape = self.types.records[id as usize].clone();
        // The source record of a spread binding (unboxed when nullable).
        let source = |slot: u32, shape: u32, nullable: Option<Type>| match nullable {
            Some(cell) => Expr {
                span,
                ty: Type::Record(shape),
                kind: E::UnboxUnion(Box::new(local(span, slot, cell))),
            },
            None => local(span, slot, Type::Record(shape)),
        };
        let read = |this: &Self, provider: Provider, ty: Type| -> Result<Expr> {
            let value = match provider {
                Provider::Value { slot, ty } => local(span, slot, ty),
                Provider::Spread {
                    slot,
                    shape,
                    field,
                    ty,
                    nullable,
                    ..
                } => Expr {
                    span,
                    ty,
                    kind: E::Field(Box::new(source(slot, shape, nullable)), field),
                },
            };
            unions::widen(value, Some(ty), this.types)
        };
        let present = |provider: Provider| match provider {
            Provider::Spread {
                slot,
                shape,
                field,
                nullable,
                ..
            } => {
                let own = Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::HasOwn(Box::new(source(slot, shape, nullable)), Some(field)),
                };
                match nullable {
                    Some(cell) => Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Conditional(
                            Box::new(Expr {
                                span,
                                ty: Type::Boolean,
                                kind: E::IsNullish(Box::new(local(span, slot, cell))),
                            }),
                            Box::new(Expr {
                                span,
                                ty: Type::Boolean,
                                kind: E::Integer(0),
                            }),
                            Box::new(own),
                        ),
                    },
                    None => own,
                }
            }
            Provider::Value { .. } => Expr {
                span,
                ty: Type::Boolean,
                kind: E::Integer(1),
            },
        };
        let output = self.hidden(record);
        roots.push(output);
        let mut fields = Vec::new();
        let mut later = Vec::new();
        let mut presence = 0u128;
        for (name, relevant, certain) in plans {
            let slot = shape
                .fields
                .iter()
                .position(|f| f.name == name)
                .ok_or_else(|| CompilerError::Internal("spread field".into()))?;
            let ty = shape.fields[slot].ty;
            if certain {
                let mut value = read(self, relevant[0], ty)?;
                for provider in &relevant[1..] {
                    value = Expr {
                        span,
                        ty,
                        kind: E::Conditional(
                            Box::new(present(*provider)),
                            Box::new(read(self, *provider, ty)?),
                            Box::new(value),
                        ),
                    };
                }
                presence |= 1u128 << slot;
                fields.push((slot as u32, value));
            } else {
                let absent = unions::widen(
                    Expr {
                        span,
                        ty: Type::Undefined,
                        kind: E::Integer(0),
                    },
                    Some(ty),
                    self.types,
                )?;
                fields.push((slot as u32, absent));
                for provider in relevant {
                    later.push(S::If(
                        present(provider),
                        vec![S::SetField(
                            local(span, output, record),
                            slot as u32,
                            read(self, provider, ty)?,
                        )],
                        Vec::new(),
                    ));
                }
            }
        }
        let built = if shape.fields.iter().any(|f| f.optional) {
            E::OptionalRecord(fields, presence)
        } else {
            E::Record(fields)
        };
        body.push(S::Init(
            output,
            Expr {
                span,
                ty: record,
                kind: built,
            },
        ));
        body.extend(later);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty: record,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }
}
