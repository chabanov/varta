//! Object spread into dictionaries and mixed records (v35
//! `mixed_index_records`): `{ ...defaults, ...overrides, key: value }` with a
//! dictionary context. The literal starts empty and applies its members in
//! source order, exactly as `CopyDataProperties` and `CreateDataProperty` do:
//! a spread copies the source's own keys in their own-key order, and a later
//! write of an existing key replaces its value in place, keeping its position.
//! Spread sources are dictionaries (their entries) and closed records (their
//! present fields, enumerated at run time so their insertion order is kept).
use super::*;
use ir::RecordField;

/// The `TypeError` message of a layout spread value outside the type of the
/// dictionary entry it is stored at (v38 `layout_spreads`).
const SPREAD_VALUE_ERROR: &str = "own property value is not of the dictionary's value type";

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn boolean(span: swc_common::Span, value: bool) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Integer(i32::from(value)),
    }
}
fn number(span: swc_common::Span, value: f64) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Float64(value),
    }
}
fn binary(op: ast::BinaryOp, left: Expr, right: Expr, ty: Type) -> Expr {
    Expr {
        span: left.span,
        ty,
        kind: E::Binary(op, Box::new(left), Box::new(right)),
    }
}
fn length(array: Expr) -> Expr {
    Expr {
        span: array.span,
        ty: Type::Float64,
        kind: E::Length(Box::new(array)),
    }
}

impl Lower<'_> {
    fn dictionary_copy_set(
        &mut self,
        target: Expr,
        key: Expr,
        value: Expr,
        expected: Type,
        create_data: bool,
    ) -> Result<Expr> {
        if !create_data || !self.types.profile.checked_snapshots() {
            return self.dictionary_set(target, key, value, expected);
        }
        let Type::Dictionary(id) = target.ty else {
            return Err(invalid("data-property target"));
        };
        let element = self.types.dictionaries[id as usize].value;
        let value = self.dictionary_value(value, expected, element)?;
        self.dictionary_helper_call(ir::DictionaryHelper::Define, vec![target, key, value])
    }

    pub(super) fn dictionary_spread_literal(
        &mut self,
        object: &ast::ObjectLit,
        id: u32,
    ) -> Result<Expr> {
        self.dictionary_spread_literal_mode(object, id, true)
    }
    pub(super) fn dictionary_spread_literal_mode(
        &mut self,
        object: &ast::ObjectLit,
        id: u32,
        create_data: bool,
    ) -> Result<Expr> {
        let ty = Type::Dictionary(id);
        let dictionary = self.types.dictionaries[id as usize];
        let span = object.span;
        let declared = self
            .types
            .declared(ty)
            .map(|members| members.fields.clone())
            .unwrap_or_default();
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        let empty = self.dictionary_storage(
            span,
            ty,
            Expr {
                span,
                ty: dictionary.keys,
                kind: E::Array(vec![]),
            },
            Expr {
                span,
                ty: dictionary.values,
                kind: E::Array(vec![]),
            },
        )?;
        let mut body = vec![S::Init(output, empty)];
        let mut roots = vec![output];
        let mut names: Vec<String> = Vec::new();
        let mut provided: Vec<String> = Vec::new();
        for property in &object.props {
            match property {
                ast::PropOrSpread::Spread(spread) => {
                    let source = self.expr(&spread.expr)?;
                    // `...optional` (v38): spreading `undefined` or `null`
                    // copies nothing, as in JavaScript.
                    let present: Vec<Type> = unions::members(source.ty, self.types)
                        .into_iter()
                        .filter(|member| !matches!(member, Type::Undefined | Type::Null))
                        .collect();
                    if self.types.profile.structural_subtyping()
                        && source.ty.is_union()
                        && let [member] = present[..]
                        && matches!(member, Type::Dictionary(_) | Type::Record(_))
                    {
                        let cell_ty = source.ty;
                        let cell = self.hidden(cell_ty);
                        roots.push(cell);
                        body.push(S::Init(cell, source));
                        let unboxed = Expr {
                            span,
                            ty: member,
                            kind: E::UnboxUnion(Box::new(local(span, cell, cell_ty))),
                        };
                        let copied = self.spread_entries_mode(
                            unboxed,
                            local(span, output, ty),
                            &declared,
                            create_data,
                        )?;
                        roots.extend(copied.1);
                        body.push(S::If(
                            Expr {
                                span,
                                ty: Type::Boolean,
                                kind: E::Not(Box::new(Expr {
                                    span,
                                    ty: Type::Boolean,
                                    kind: E::IsNullish(Box::new(local(span, cell, cell_ty))),
                                })),
                            },
                            copied.2,
                            Vec::new(),
                        ));
                        continue;
                    }
                    let copied = self.spread_entries_mode(
                        source,
                        local(span, output, ty),
                        &declared,
                        create_data,
                    )?;
                    provided.extend(copied.0);
                    roots.extend(copied.1);
                    body.extend(copied.2);
                }
                ast::PropOrSpread::Prop(property) => {
                    let (name, value) = match &**property {
                        ast::Prop::KeyValue(field) => {
                            (records::property_key(&field.key)?, (*field.value).clone())
                        }
                        ast::Prop::Shorthand(id) => (
                            records::key(id.sym.to_string())?,
                            ast::Expr::Ident(id.clone()),
                        ),
                        _ => return Err(unsupported("object method, getter or setter")),
                    };
                    if names.contains(&name) {
                        return Err(unsupported("duplicate object key"));
                    }
                    self.literal_key(ty, &name, property.span(), true)?;
                    names.push(name.clone());
                    let expected = declared
                        .iter()
                        .find(|field| field.name == name)
                        .map_or(dictionary.value, |field| field.ty);
                    let value = self.expr_expected(&value, Some(expected))?;
                    let key = Expr {
                        span,
                        ty: Type::String,
                        kind: E::String(self.types.profile.string_bytes(&name)),
                    };
                    let stored =
                        self.dictionary_set(local(span, output, ty), key, value, expected)?;
                    body.push(S::Expr(stored));
                    provided.push(name);
                }
            }
        }
        if let Some(missing) = declared
            .iter()
            .find(|field| !field.optional && !provided.contains(&field.name))
        {
            return Err(self.source.locate(
                type_error(format!(
                    "object literal is missing the required declared property {}",
                    missing.name
                )),
                span,
            ));
        }
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }
    /// A fresh dictionary of type `target` holding the present fields of the
    /// closed record `record`, in their order.
    pub(super) fn dictionary_from_record(&mut self, record: Expr, target: Type) -> Result<Expr> {
        let Type::Dictionary(id) = target else {
            return Err(invalid("missing dictionary type"));
        };
        let span = record.span;
        let dictionary = self.types.dictionaries[id as usize];
        let output = self.hidden(target);
        let marker = self.hidden(Type::Number);
        let empty = self.dictionary_storage(
            span,
            target,
            Expr {
                span,
                ty: dictionary.keys,
                kind: E::Array(vec![]),
            },
            Expr {
                span,
                ty: dictionary.values,
                kind: E::Array(vec![]),
            },
        )?;
        let declared = self
            .types
            .declared(target)
            .map(|members| members.fields.clone())
            .unwrap_or_default();
        let (provided, mut roots, copy) =
            self.spread_entries_mode(record, local(span, output, target), &declared, true)?;
        if let Some(missing) = declared
            .iter()
            .find(|field| !field.optional && !provided.contains(&field.name))
        {
            return Err(type_error(format!(
                "object is missing the required declared property {}",
                missing.name
            )));
        }
        let mut body = vec![S::Init(output, empty)];
        body.extend(copy);
        roots.push(output);
        Ok(Expr {
            span,
            ty: target,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }
    /// Copy the entries of `graph`, a JSON-object read (a dictionary of
    /// `unknown`) of a layout spread source, into the dictionary `target`
    /// of another value type (v38 `layout_spreads`). Each value is stored
    /// at its key's declared type, else the index type; a value of another
    /// type (a property the source's view does not declare) throws
    /// `TypeError` where it is read. Returns the hidden roots and the
    /// statements.
    fn checked_entries(
        &mut self,
        graph: Expr,
        target: Expr,
        declared: &[RecordField],
        create_data: bool,
    ) -> Result<(Vec<u32>, Vec<S>)> {
        let span = graph.span;
        let (Type::Dictionary(id), Type::Dictionary(from)) = (target.ty, graph.ty) else {
            return Err(invalid("missing dictionary type"));
        };
        let element = self.types.dictionaries[id as usize].value;
        let from = self.types.dictionaries[from as usize];
        let graph_ty = graph.ty;
        let held = self.hidden(graph_ty);
        let index = self.hidden(Type::Float64);
        let name = self.hidden(Type::String);
        let raw = self.hidden(Type::Unknown);
        let item = self.hidden(element);
        let found = self.hidden(Type::Boolean);
        let roots = vec![held, index, name, raw, item];
        let mut body = vec![S::Init(held, graph), S::Init(index, number(span, 0.0))];
        body.extend(self.dictionary_compact(local(span, held, graph_ty))?);
        body.extend([
            S::Init(
                name,
                Expr {
                    span,
                    ty: Type::String,
                    kind: E::Zero,
                },
            ),
            S::Init(
                raw,
                Expr {
                    span,
                    ty: Type::Unknown,
                    kind: E::Zero,
                },
            ),
            S::Init(
                item,
                Expr {
                    span,
                    ty: element,
                    kind: E::Zero,
                },
            ),
            S::Init(found, boolean(span, false)),
        ]);
        let at = || local(span, index, Type::Float64);
        let vector = |slot: u32, ty: Type| Expr {
            span,
            ty,
            kind: E::Field(Box::new(local(span, held, graph_ty)), slot),
        };
        let entry = |vector: Expr, ty: Type| Expr {
            span,
            ty,
            kind: E::Index(Box::new(vector), Box::new(at())),
        };
        let checked = |this: &mut Self, expected: Type| -> Result<Expr> {
            let value = this.assert_value(
                local(span, raw, Type::Unknown),
                expected,
                &subtyping::describe(expected, this.types),
                SPREAD_VALUE_ERROR,
                span,
            )?;
            unions::widen(value, Some(element), this.types)
        };
        let mut cases = Vec::new();
        for field in declared {
            let value = checked(self, field.ty)?;
            cases.push((
                self.types.profile.string_bytes(&field.name),
                field.name.clone(),
                vec![S::Set(item, value), S::Set(found, boolean(span, true))],
            ));
        }
        cases.sort_by(|a, b| a.1.encode_utf16().cmp(b.1.encode_utf16()));
        let mut entries: Vec<_> = cases.into_iter().map(|(k, _, s)| (k, s)).collect();
        let fallback = checked(self, element)?;
        let stored = self.dictionary_copy_set(
            target,
            local(span, name, Type::String),
            local(span, item, element),
            element,
            create_data,
        )?;
        let mut visit = vec![
            S::Set(
                name,
                entry(vector(from.keys_slot(), from.keys), Type::String),
            ),
            S::Set(
                raw,
                entry(vector(from.values_slot(), from.values), Type::Unknown),
            ),
            S::Set(found, boolean(span, false)),
        ];
        visit.extend(own_keys::lookup(span, name, &mut entries));
        visit.push(S::If(
            local(span, found, Type::Boolean),
            vec![],
            vec![S::Set(item, fallback)],
        ));
        visit.push(S::Expr(stored));
        visit.push(S::Set(
            index,
            binary(ast::BinaryOp::Add, at(), number(span, 1.0), Type::Float64),
        ));
        body.push(S::While(
            binary(
                ast::BinaryOp::Lt,
                at(),
                length(vector(from.keys_slot(), from.keys)),
                Type::Boolean,
            ),
            visit,
        ));
        Ok((roots, body))
    }
    /// Copy the own entries of `source` into the dictionary `target`. Returns
    /// the declared names the source always provides, the hidden roots and
    /// the statements.
    pub(super) fn spread_entries(
        &mut self,
        source: Expr,
        target: Expr,
        declared: &[RecordField],
    ) -> Result<(Vec<String>, Vec<u32>, Vec<S>)> {
        self.spread_entries_mode(source, target, declared, false)
    }
    fn spread_entries_mode(
        &mut self,
        source: Expr,
        target: Expr,
        declared: &[RecordField],
        create_data: bool,
    ) -> Result<(Vec<String>, Vec<u32>, Vec<S>)> {
        let span = source.span;
        let Type::Dictionary(id) = target.ty else {
            return Err(invalid("missing dictionary type"));
        };
        let element = self.types.dictionaries[id as usize].value;
        let source_ty = source.ty;
        let held = self.hidden(source_ty);
        let index = self.hidden(Type::Float64);
        let mut roots = vec![held, index];
        let mut body = vec![S::Init(held, source), S::Init(index, number(span, 0.0))];
        let at = || local(span, index, Type::Float64);
        let step = S::Set(
            index,
            binary(ast::BinaryOp::Add, at(), number(span, 1.0), Type::Float64),
        );
        // The type every copied key is stored at: its declared field's type
        // when the target declares it, else the index type.
        let accepts = |this: &Self, name: Option<&str>, ty: Type| -> bool {
            let expected = name
                .and_then(|name| declared.iter().find(|field| field.name == name))
                .map_or(element, |field| field.ty);
            (mixed_records::assignable(expected, ty, this.types)
                || (this.types.profile.checked_snapshots() && expected == ty))
                && (name.is_some()
                    || declared
                        .iter()
                        .all(|field| mixed_records::assignable(field.ty, ty, this.types)))
        };
        let mismatch = |ty: Type| {
            type_error(format!(
                "spread value of type {ty} is not assignable to the dictionary's value type {element}"
            ))
        };
        match source_ty {
            Type::Dictionary(from) => {
                let from = self.types.dictionaries[from as usize];
                // TypeScript relates each target declaration to the source's
                // own declaration of that key, else to its index type.
                let own = self
                    .types
                    .declared(source_ty)
                    .map(|members| members.fields.clone())
                    .unwrap_or_default();
                let related = mixed_records::assignable(element, from.value, self.types)
                    && declared.iter().all(|field| {
                        let source = own
                            .iter()
                            .find(|f| f.name == field.name)
                            .map_or(from.value, |f| f.ty);
                        mixed_records::assignable(field.ty, source, self.types)
                    });
                if !related {
                    return Err(self.source.locate(mismatch(from.value), span));
                }
                // Value18: copy dense, own-key-ordered entries.
                body.extend(self.dictionary_compact(local(span, held, source_ty))?);
                let keys = Expr {
                    span,
                    ty: from.keys,
                    kind: E::Field(Box::new(local(span, held, source_ty)), from.keys_slot()),
                };
                let entry = |vector: Expr, ty: Type| Expr {
                    span,
                    ty,
                    kind: E::Index(Box::new(vector), Box::new(at())),
                };
                let values = Expr {
                    span,
                    ty: from.values,
                    kind: E::Field(Box::new(local(span, held, source_ty)), from.values_slot()),
                };
                let key = entry(
                    Expr {
                        span,
                        ty: from.keys,
                        kind: E::Field(Box::new(local(span, held, source_ty)), from.keys_slot()),
                    },
                    Type::String,
                );
                let value = unions::widen(entry(values, from.value), Some(element), self.types)?;
                let stored = self.dictionary_copy_set(target, key, value, element, create_data)?;
                body.push(S::While(
                    binary(ast::BinaryOp::Lt, at(), length(keys), Type::Boolean),
                    vec![S::Expr(stored), step],
                ));
                let provided = self
                    .types
                    .declared(source_ty)
                    .map(|members| {
                        members
                            .fields
                            .iter()
                            .filter(|field| !field.optional && !members.object)
                            .map(|field| field.name.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                Ok((provided, roots, body))
            }
            Type::Record(shape) if self.types.records[shape as usize].class.is_none() => {
                let fields = self.types.records[shape as usize].fields.clone();
                for field in &fields {
                    if !accepts(self, Some(&field.name), field.ty) {
                        return Err(self.source.locate(mismatch(field.ty), span));
                    }
                }
                // V38 (`layout_spreads`): a view of objects of other layouts
                // copies the actual object's own entries through its
                // JSON-object read (one branch per layout). Into a dictionary
                // of another value type, each value the view does not declare
                // is checked against the type it is stored at (checked reads
                // at TypeScript unsoundness).
                if self.layout_site(span)
                    || (self.types.profile.checked_snapshots()
                        && self
                            .types
                            .subtyping
                            .view_sources(shape)
                            .iter()
                            .any(|source| snapshots::physical(*source, self.types).is_some()))
                {
                    let provided = fields
                        .iter()
                        .filter(|field| !field.optional)
                        .map(|field| field.name.clone())
                        .collect();
                    let cell = unknown::widen(local(span, held, source_ty), self.types)?;
                    let graph = self.record_graph(cell, span)?;
                    let (more, statements) = if element == Type::Unknown
                        && (!self.types.profile.checked_snapshots() || declared.is_empty())
                    {
                        let (_, more, statements) =
                            self.spread_entries_mode(graph, target, declared, create_data)?;
                        (more, statements)
                    } else {
                        self.checked_entries(graph, target, declared, create_data)?
                    };
                    roots.extend(more);
                    body.extend(statements);
                    return Ok((provided, roots, body));
                }
                if self.types.profile.sdk_paths() {
                    self.types.subtyping.note_key_copy(
                        shape,
                        if self.types.profile.expression_forms() {
                            subtyping::KeyCopy::LayoutSpread
                        } else {
                            subtyping::KeyCopy::Spread
                        },
                        self.source,
                        span,
                    );
                }
                // Present fields in their run-time order, then a static switch
                // over the field names selects each value.
                let keys = self.enumerate_value(
                    local(span, held, source_ty),
                    object_enumeration::Kind::Keys,
                    span,
                )?;
                let keys_ty = keys.ty;
                let names = self.hidden(keys_ty);
                let name = self.hidden(Type::String);
                let item = self.hidden(element);
                let found = self.hidden(Type::Boolean);
                roots.extend([names, name, item]);
                body.push(S::Init(names, keys));
                let mut cases = Vec::new();
                for (slot, field) in fields.iter().enumerate() {
                    let value = Expr {
                        span,
                        ty: field.ty,
                        kind: E::Field(Box::new(local(span, held, source_ty)), slot as u32),
                    };
                    let expected = declared
                        .iter()
                        .find(|d| d.name == field.name)
                        .map_or(element, |d| d.ty);
                    let value = unions::widen(value, Some(expected), self.types)?;
                    let value = unions::widen(value, Some(element), self.types)?;
                    cases.push((
                        self.types.profile.string_bytes(&field.name),
                        field.name.clone(),
                        vec![
                            S::Set(item, value),
                            S::Set(
                                found,
                                Expr {
                                    span,
                                    ty: Type::Boolean,
                                    kind: E::Integer(1),
                                },
                            ),
                        ],
                    ));
                }
                cases.sort_by(|a, b| a.1.encode_utf16().cmp(b.1.encode_utf16()));
                let mut entries: Vec<_> = cases.into_iter().map(|(k, _, s)| (k, s)).collect();
                let select = own_keys::lookup(span, name, &mut entries);
                let stored = self.dictionary_copy_set(
                    target,
                    local(span, name, Type::String),
                    local(span, item, element),
                    element,
                    create_data,
                )?;
                let mut visit = vec![
                    S::Set(
                        name,
                        Expr {
                            span,
                            ty: Type::String,
                            kind: E::Index(Box::new(local(span, names, keys_ty)), Box::new(at())),
                        },
                    ),
                    S::Set(
                        found,
                        Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::Integer(0),
                        },
                    ),
                ];
                visit.extend(select);
                visit.push(S::If(
                    local(span, found, Type::Boolean),
                    vec![S::Expr(stored)],
                    vec![],
                ));
                visit.push(step);
                body.push(S::While(
                    binary(
                        ast::BinaryOp::Lt,
                        at(),
                        length(local(span, names, keys_ty)),
                        Type::Boolean,
                    ),
                    visit,
                ));
                let provided = fields
                    .iter()
                    .filter(|field| !field.optional)
                    .map(|field| field.name.clone())
                    .collect();
                Ok((provided, roots, body))
            }
            _ => Err(self.source.locate(
                unsupported(&format!(
                    "object spread of a {source_ty} into a dictionary; spread dictionaries or \
                     closed records"
                )),
                span,
            )),
        }
    }
}
