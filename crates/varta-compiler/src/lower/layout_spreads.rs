//! V38 (`expression_forms`): record spreads of views of objects of other
//! layouts, as CopyDataProperties copies them.
//!
//! A value of a closed object type `T` may hold an object of another record
//! layout (a structural view, `subtyping`), whose own properties are more
//! than `T`'s or in another order. `{ ...t }` copies every own property of
//! the actual object, so the copy's layout depends on the source's run-time
//! layout. A spread site whose source type is such a view is found by one
//! lowering pass (`subtyping::spread_views`) and lowered by the next
//! (`link::LAYOUT_PASSES`) as:
//!
//! 1. the members before the spread, evaluated in source order;
//! 2. `copy_T(source)`: one branch per record layout `T` may hold, tested by
//!    the record descriptor, each a plain spread of that exact layout (an
//!    object with accessors through the type it is viewed as, so its getters
//!    run in its own-key order); the copy is a fresh object of the copy's
//!    layout, viewed as `T`;
//! 3. the literal over the copy, lowered statically (its members after the
//!    spread are evaluated after the copy, as in JavaScript);
//! 4. `widen(result, copy)`: by the copy's layout, the result with the copy's
//!    extra own properties at the spread's position, viewed as the
//!    literal's type (one branch per copy layout).
//!
//! Both functions are synthesized after every body is lowered (`finish`),
//! when the view sources are known; their new layouts are view sources in
//! turn, which a further pass may need.
use super::object_accessors::local_value;
use super::*;

/// One member of a literal with a layout spread, for the widened key order.
#[derive(Clone)]
pub(crate) enum Part {
    /// A literal member (data, method) by name.
    Value(String),
    /// A spread of a source of one layout: its field names.
    Static(Vec<String>),
    /// The layout spread with this index among the literal's copies.
    Layout(usize),
}

/// A `widen` function of one layout spread site.
#[derive(Clone)]
struct Widen {
    index: usize,
    /// The literal's static result shape.
    result: u32,
    /// The view shape of each copy and its cell type when the source may be
    /// nullish, in member order.
    copies: Vec<(u32, Option<Type>)>,
    parts: Vec<Part>,
    written: bool,
}

/// The synthesized functions of layout spreads (v38).
#[derive(Default)]
pub(crate) struct Layouts {
    /// `copy_T` by view shape and the keys it leaves out (a rest pattern's
    /// named properties): the function and whether its body is written.
    copies: BTreeMap<(u32, Vec<String>), (usize, bool)>,
    /// The copy layouts of each view shape, once its copy is written.
    copy_shapes: BTreeMap<u32, Vec<u32>>,
    widens: Vec<Widen>,
    /// The source and span of the first layout spread.
    origin: Option<(crate::source::Source, swc_common::Span)>,
    /// While a synthesized function is lowered: its spreads read one exact
    /// layout and are neither noted nor copied again.
    pub(crate) exact: bool,
}

impl Lower<'_> {
    /// Whether the spread of `expr` is a layout spread site of this pass.
    pub(super) fn layout_spread(&self, expr: &ast::Expr) -> bool {
        self.layout_site(expr.span())
    }
    /// Whether `span` is a layout spread or rest site of this pass.
    pub(super) fn layout_site(&self, span: swc_common::Span) -> bool {
        !self.types.layouts.exact
            && self
                .types
                .subtyping
                .layout_sites
                .contains(&subtyping::site(self.source, span))
    }

    /// `copy_T(value)`: a fresh copy of the own properties of the object in
    /// `value` (of view shape `T`) but the `excluded` keys, by its run-time
    /// layout, viewed as `T` without those keys.
    pub(super) fn layout_copy(
        &mut self,
        value: Expr,
        shape: u32,
        excluded: Vec<String>,
    ) -> Result<Expr> {
        let span = value.span;
        self.types
            .layouts
            .origin
            .get_or_insert_with(|| (self.source.clone(), span));
        let ty = Type::Record(shape);
        let target = rest_type(shape, &excluded, self.types)?;
        let key = (shape, excluded);
        let index = match self.types.layouts.copies.get(&key) {
            Some((index, _)) => *index,
            None => {
                let index = reserve(
                    self.types,
                    self.generated,
                    self.next_function,
                    vec![ty],
                    target,
                )?;
                self.types.layouts.copies.insert(key, (index, false));
                index
            }
        };
        Ok(Expr {
            span,
            ty: target,
            kind: E::Call(index, vec![value]),
        })
    }

    /// `widen(result, copies…)` of a literal with layout spreads: each copy
    /// with its view shape and nullable cell type.
    pub(super) fn layout_widen(
        &mut self,
        result: Expr,
        copies: Vec<(Expr, u32, Option<Type>)>,
        parts: Vec<Part>,
    ) -> Result<Expr> {
        let span = result.span;
        let Type::Record(id) = result.ty else {
            return Err(CompilerError::Internal("layout spread result".into()));
        };
        let params: Vec<Type> = std::iter::once(result.ty)
            .chain(copies.iter().map(|(copy, ..)| copy.ty))
            .collect();
        let index = reserve(
            self.types,
            self.generated,
            self.next_function,
            params,
            result.ty,
        )?;
        self.types.layouts.widens.push(Widen {
            index,
            result: id,
            copies: copies
                .iter()
                .map(|(_, shape, nullable)| (*shape, *nullable))
                .collect(),
            parts,
            written: false,
        });
        let args = std::iter::once(result)
            .chain(copies.into_iter().map(|(copy, ..)| copy))
            .collect();
        Ok(Expr {
            span,
            ty: Type::Record(id),
            kind: E::Call(index, args),
        })
    }

    /// The body of `copy_T` (local 0, of view shape `shape`) without the
    /// `excluded` keys.
    fn copy_body(
        &mut self,
        shape: u32,
        excluded: &[String],
        span: swc_common::Span,
    ) -> Result<(Vec<S>, Vec<u32>)> {
        let view = Type::Record(shape);
        let target = rest_type(shape, excluded, self.types)?;
        let mut body = Vec::new();
        let mut copies = Vec::new();
        // A class instance (of a class viewed as `shape` or a subclass)
        // copies its own fields: the declared ones, in declaration order.
        let mut classes = Vec::new();
        for class in self.types.subtyping.class_sources_of(shape) {
            for member in std::iter::once(class).chain(self.types.classes.descendants(class)) {
                if !classes.contains(&member) {
                    classes.push(member);
                }
            }
        }
        for class in classes {
            let (record, fields) = self.class_fields(class, span)?;
            let instance = self.hidden(record);
            let mut kept = Vec::new();
            for (slot, mut field) in fields {
                if excluded.contains(&field.name) {
                    continue;
                }
                // An absent optional field holds `undefined` (every reader
                // checks it).
                if field.optional
                    && !unions::members(field.ty, self.types).contains(&Type::Undefined)
                {
                    field.ty = unions::intern(vec![field.ty, Type::Undefined], self.types)?;
                }
                kept.push((slot, field));
            }
            let fields = kept;
            let copy_ty =
                records::intern(fields.iter().map(|(_, f)| f.clone()).collect(), self.types)?;
            let Type::Record(copy) = copy_ty else {
                return Err(CompilerError::Internal("class copy shape".into()));
            };
            let output = self.hidden(copy_ty);
            let mut branch = vec![S::Init(
                instance,
                Expr {
                    span,
                    ty: record,
                    kind: E::Reinterpret(Box::new(local_value(span, 0, view))),
                },
            )];
            // The copy's slots by name (an interned shape orders its fields).
            let copied = self.types.records[copy as usize].fields.clone();
            let mut values = Vec::new();
            let mut later = Vec::new();
            let mut order = Vec::new();
            for (slot, field) in &fields {
                let target = copied
                    .iter()
                    .position(|own| own.name == field.name)
                    .ok_or_else(|| invalid("class copy field"))?;
                order.push((target as u32, target as u32));
                let read = Expr {
                    span,
                    ty: self.types.records[record_shape(record)? as usize].fields[*slot as usize]
                        .ty,
                    kind: E::Field(Box::new(local_value(span, instance, record)), *slot),
                };
                if field.optional {
                    let absent = unions::widen(
                        Expr {
                            span,
                            ty: Type::Undefined,
                            kind: E::Integer(0),
                        },
                        Some(field.ty),
                        self.types,
                    )?;
                    values.push((target as u32, absent));
                    later.push(S::If(
                        Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::HasOwn(
                                Box::new(local_value(span, instance, record)),
                                Some(*slot),
                            ),
                        },
                        vec![S::SetField(
                            local_value(span, output, copy_ty),
                            target as u32,
                            unions::widen(read, Some(field.ty), self.types)?,
                        )],
                        vec![],
                    ));
                } else {
                    values.push((
                        target as u32,
                        unions::widen(read, Some(field.ty), self.types)?,
                    ));
                }
            }
            values.sort_by_key(|(slot, _)| *slot);
            let built = if copied.iter().any(|field| field.optional) {
                let present = copied
                    .iter()
                    .enumerate()
                    .filter(|(_, field)| !field.optional)
                    .fold(0u128, |mask, (slot, _)| mask | 1u128 << slot);
                E::OptionalRecord(values, present)
            } else {
                E::Record(values)
            };
            branch.push(S::Init(
                output,
                Expr {
                    span,
                    ty: copy_ty,
                    kind: built,
                },
            ));
            branch.extend(later);
            // An instance's own keys are in declaration order.
            let slots = [
                self.hidden(Type::Number),
                self.hidden(Type::Number),
                self.hidden(Type::Number),
            ];
            branch.push(S::RecordOrder {
                target: output,
                shape: copy,
                source: None,
                mapping: Vec::new(),
                start: true,
                slots,
            });
            branch.push(S::RecordOrder {
                target: output,
                shape: copy,
                source: None,
                mapping: order,
                start: false,
                slots,
            });
            let mut value =
                subtyping::coerce(local_value(span, output, copy_ty), Some(target), self.types)?;
            if value.ty != target {
                let Type::Record(id) = target else {
                    return Err(CompilerError::Internal("class copy view".into()));
                };
                subtyping::note_loose(copy_ty, id, self.types)?;
                value = Expr {
                    span,
                    ty: target,
                    kind: E::Reinterpret(Box::new(value)),
                };
            }
            branch.push(S::Return(Some(value)));
            copies.push(copy);
            let descriptor = self.hidden(Type::Number);
            body.push(S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsClass {
                        value: Box::new(local_value(span, 0, view)),
                        shapes: vec![self.types.classes.shape(class)],
                        slot: descriptor,
                    },
                },
                branch,
                vec![],
            ));
        }
        let mut layouts = self.types.subtyping.view_sources(shape);
        layouts.retain(|layout| *layout != shape);
        for (n, layout) in layouts
            .iter()
            .copied()
            .chain(std::iter::once(shape))
            .enumerate()
        {
            // An object with accessors is copied through the type it is
            // viewed as (its getters run); any other layout as itself.
            let read = self.types.receivers.logical(layout).unwrap_or(layout);
            let name = format!("#copy{n}");
            self.declare(&name, Some(Type::Record(read)), true, false)?;
            let binding = self.binding(&name)?;
            let copied = if excluded.is_empty() {
                self.record(&spread_of(&name, span), None)?
            } else {
                let (statements, roots, output, result) =
                    self.rest_record(binding.slot, read, excluded, span)?;
                let marker = self.hidden(Type::Number);
                Expr {
                    span,
                    ty: result,
                    kind: E::ValueBlock {
                        output,
                        marker,
                        roots: roots.into_iter().chain([binding.slot, output]).collect(),
                        body: statements,
                    },
                }
            };
            let Type::Record(copy) = copied.ty else {
                return Err(CompilerError::Internal("layout copy shape".into()));
            };
            copies.push(copy);
            let mut value = subtyping::coerce(copied, Some(target), self.types)?;
            if value.ty != target {
                // A copy of a checked view source (`object_methods`) is one.
                subtyping::note_loose(
                    Type::Record(copy),
                    match target {
                        Type::Record(id) => id,
                        _ => return Err(CompilerError::Internal("layout copy view".into())),
                    },
                    self.types,
                )?;
                value = Expr {
                    span,
                    ty: target,
                    kind: E::Reinterpret(Box::new(value)),
                };
            }
            let branch = vec![
                S::Init(
                    binding.slot,
                    Expr {
                        span,
                        ty: Type::Record(read),
                        kind: E::Reinterpret(Box::new(local_value(span, 0, view))),
                    },
                ),
                S::Return(Some(value)),
            ];
            if layout == shape {
                body.extend(branch);
            } else {
                let descriptor = self.hidden(Type::Number);
                body.push(S::If(
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::IsClass {
                            value: Box::new(local_value(span, 0, view)),
                            shapes: vec![layout],
                            slot: descriptor,
                        },
                    },
                    branch,
                    vec![],
                ));
            }
        }
        Ok((body, copies))
    }

    /// The body of a `widen` function (local 0 the result, then one local
    /// per copy): one branch per combination of the copies' layouts with
    /// extra own properties (a nullish copy has none), built from the
    /// result's fields and those extras in the members' key order.
    fn widen_body(&mut self, widen: &Widen, span: swc_common::Span) -> Result<Vec<S>> {
        let result = Type::Record(widen.result);
        let mut body = Vec::new();
        // The copies as their view shapes (nullish: absent).
        let mut copies = Vec::new();
        for (n, (shape, nullable)) in widen.copies.iter().enumerate() {
            let param = n as u32 + 1;
            let view = Type::Record(*shape);
            let copy = self.hidden(view);
            match nullable {
                Some(cell) => {
                    let present = self.hidden(Type::Boolean);
                    body.push(S::Init(
                        present,
                        Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::Conditional(
                                Box::new(Expr {
                                    span,
                                    ty: Type::Boolean,
                                    kind: E::IsNullish(Box::new(local_value(span, param, *cell))),
                                }),
                                Box::new(Expr {
                                    span,
                                    ty: Type::Boolean,
                                    kind: E::Integer(0),
                                }),
                                Box::new(Expr {
                                    span,
                                    ty: Type::Boolean,
                                    kind: E::Integer(1),
                                }),
                            ),
                        },
                    ));
                    // An absent copy is never read (its layout tests are
                    // guarded by `present`).
                    body.push(S::Init(
                        copy,
                        Expr {
                            span,
                            ty: view,
                            kind: E::Zero,
                        },
                    ));
                    body.push(S::If(
                        local_value(span, present, Type::Boolean),
                        vec![S::Set(
                            copy,
                            Expr {
                                span,
                                ty: view,
                                kind: E::UnboxUnion(Box::new(local_value(span, param, *cell))),
                            },
                        )],
                        vec![],
                    ));
                    copies.push((*shape, copy, Some(present)));
                }
                None => {
                    body.push(S::Init(copy, local_value(span, param, view)));
                    copies.push((*shape, copy, None));
                }
            }
        }
        let fields = self.types.records[widen.result as usize].fields.clone();
        let mut chosen = Vec::new();
        body.extend(self.widen_choose(widen, &copies, &fields, &mut chosen, span)?);
        body.push(S::Return(Some(local_value(span, 0, result))));
        Ok(body)
    }

    /// The branches choosing the layout of copy `chosen.len()` and the ones
    /// after it.
    fn widen_choose(
        &mut self,
        widen: &Widen,
        copies: &[(u32, u32, Option<u32>)],
        fields: &[ir::RecordField],
        chosen: &mut Vec<Option<u32>>,
        span: swc_common::Span,
    ) -> Result<Vec<S>> {
        let Some(&(shape, copy, present)) = copies.get(chosen.len()) else {
            return self.widen_build(widen, copies, fields, chosen, span);
        };
        let view = Type::Record(shape);
        let layouts = self
            .types
            .layouts
            .copy_shapes
            .get(&shape)
            .cloned()
            .unwrap_or_default();
        let mut branches = Vec::new();
        for layout in layouts {
            let extra = self.types.records[layout as usize]
                .fields
                .iter()
                .any(|field| !fields.iter().any(|known| known.name == field.name));
            if !extra {
                continue;
            }
            let descriptor = self.hidden(Type::Number);
            let mut test = Expr {
                span,
                ty: Type::Boolean,
                kind: E::IsClass {
                    value: Box::new(local_value(span, copy, view)),
                    shapes: vec![layout],
                    slot: descriptor,
                },
            };
            if let Some(present) = present {
                test = Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Binary(
                        ast::BinaryOp::LogicalAnd,
                        Box::new(local_value(span, present, Type::Boolean)),
                        Box::new(test),
                    ),
                };
            }
            chosen.push(Some(layout));
            let inner = self.widen_choose(widen, copies, fields, chosen, span)?;
            chosen.pop();
            branches.push((test, inner));
        }
        chosen.push(None);
        let mut rest = self.widen_choose(widen, copies, fields, chosen, span)?;
        chosen.pop();
        for (test, inner) in branches.into_iter().rev() {
            rest = vec![S::If(test, inner, rest)];
        }
        Ok(rest)
    }

    /// The widened object of one combination of copy layouts (`None`: the
    /// copy adds no property), or nothing when no copy adds one.
    fn widen_build(
        &mut self,
        widen: &Widen,
        copies: &[(u32, u32, Option<u32>)],
        fields: &[ir::RecordField],
        chosen: &[Option<u32>],
        span: swc_common::Span,
    ) -> Result<Vec<S>> {
        if chosen.iter().all(Option::is_none) {
            return Ok(vec![]);
        }
        let result = Type::Record(widen.result);
        // The extras by name: position of the first copy that has one, value
        // of the last (CopyDataProperties keeps the first definition's place).
        let mut extras: Vec<(ir::RecordField, usize, u32)> = Vec::new();
        for (n, layout) in chosen.iter().enumerate() {
            let Some(layout) = layout else { continue };
            for (slot, field) in self.types.records[*layout as usize]
                .fields
                .iter()
                .enumerate()
            {
                if fields.iter().any(|known| known.name == field.name) {
                    continue;
                }
                match extras
                    .iter_mut()
                    .find(|(known, ..)| known.name == field.name)
                {
                    Some(entry) => *entry = (field.clone(), n, slot as u32),
                    None => extras.push((field.clone(), n, slot as u32)),
                }
            }
        }
        let mut wide = fields.to_vec();
        wide.extend(extras.iter().map(|(field, ..)| field.clone()));
        let wide_ty = records::intern(wide, self.types)?;
        let Type::Record(wide_id) = wide_ty else {
            return Err(CompilerError::Internal("widened layout".into()));
        };
        let wide = self.types.records[wide_id as usize].fields.clone();
        let slot_of = |name: &str| {
            wide.iter()
                .position(|field| field.name == name)
                .map(|slot| slot as u32)
        };
        let mut branch = Vec::new();
        // Each chosen copy as its exact layout.
        let mut sources = Vec::new();
        for (n, layout) in chosen.iter().enumerate() {
            let source = layout.map(|layout| {
                let slot = self.hidden(Type::Record(layout));
                branch.push(S::Init(
                    slot,
                    Expr {
                        span,
                        ty: Type::Record(layout),
                        kind: E::Reinterpret(Box::new(local_value(
                            span,
                            copies[n].1,
                            Type::Record(copies[n].0),
                        ))),
                    },
                ));
                (slot, layout)
            });
            sources.push(source);
        }
        let output = self.hidden(wide_ty);
        let mut values = Vec::new();
        let mut later = Vec::new();
        for (slot, field) in fields.iter().enumerate() {
            let target = slot_of(&field.name).ok_or_else(|| invalid("widened field"))?;
            let read = Expr {
                span,
                ty: field.ty,
                kind: E::Field(Box::new(local_value(span, 0, result)), slot as u32),
            };
            let ty = wide[target as usize].ty;
            if field.optional {
                let absent = unions::widen(
                    Expr {
                        span,
                        ty: Type::Undefined,
                        kind: E::Integer(0),
                    },
                    Some(ty),
                    self.types,
                )?;
                values.push((target, absent));
                later.push(S::If(
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::HasOwn(Box::new(local_value(span, 0, result)), Some(slot as u32)),
                    },
                    vec![S::SetField(
                        local_value(span, output, wide_ty),
                        target,
                        unions::widen(read, Some(ty), self.types)?,
                    )],
                    vec![],
                ));
            } else {
                values.push((target, unions::widen(read, Some(ty), self.types)?));
            }
        }
        for (field, n, from) in &extras {
            let target = slot_of(&field.name).ok_or_else(|| invalid("widened field"))?;
            let (source, layout) = sources[*n].ok_or_else(|| invalid("copied layout"))?;
            let copied = local_value(span, source, Type::Record(layout));
            let value = unions::widen(
                Expr {
                    span,
                    ty: field.ty,
                    kind: E::Field(Box::new(copied.clone()), *from),
                },
                Some(wide[target as usize].ty),
                self.types,
            )?;
            if field.optional {
                let absent = unions::widen(
                    Expr {
                        span,
                        ty: Type::Undefined,
                        kind: E::Integer(0),
                    },
                    Some(wide[target as usize].ty),
                    self.types,
                )?;
                values.push((target, absent));
                later.push(S::If(
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::HasOwn(Box::new(copied), Some(*from)),
                    },
                    vec![S::SetField(
                        local_value(span, output, wide_ty),
                        target,
                        value,
                    )],
                    vec![],
                ));
            } else {
                values.push((target, value));
            }
        }
        values.sort_by_key(|(slot, _)| *slot);
        let built = if wide.iter().any(|field| field.optional) {
            let present = wide
                .iter()
                .enumerate()
                .filter(|(_, field)| !field.optional)
                .fold(0u128, |mask, (slot, _)| mask | 1u128 << slot);
            E::OptionalRecord(values, present)
        } else {
            E::Record(values)
        };
        branch.push(S::Init(
            output,
            Expr {
                span,
                ty: wide_ty,
                kind: built,
            },
        ));
        branch.extend(later);
        // Key order: the members again, each copy's keys at its spread.
        let slots = [
            self.hidden(Type::Number),
            self.hidden(Type::Number),
            self.hidden(Type::Number),
        ];
        branch.push(S::RecordOrder {
            target: output,
            shape: wide_id,
            source: None,
            mapping: Vec::new(),
            start: true,
            slots,
        });
        for part in &widen.parts {
            let (source, mapping) = match part {
                Part::Value(name) => (
                    None,
                    slot_of(name)
                        .map(|slot| vec![(slot, slot)])
                        .unwrap_or_default(),
                ),
                Part::Static(names) => (
                    Some((0, widen.result)),
                    names
                        .iter()
                        .filter_map(|name| {
                            let from = fields.iter().position(|field| field.name == *name)?;
                            Some((from as u32, slot_of(name)?))
                        })
                        .collect(),
                ),
                // A copy that adds no property keeps its keys' places in the
                // result.
                Part::Layout(n) => match sources.get(*n).copied().flatten() {
                    Some((source, layout)) => (
                        Some((source, layout)),
                        self.types.records[layout as usize]
                            .fields
                            .iter()
                            .enumerate()
                            .filter_map(|(from, field)| Some((from as u32, slot_of(&field.name)?)))
                            .collect(),
                    ),
                    None => {
                        let shape = copies[*n].0;
                        (
                            Some((0, widen.result)),
                            self.types.records[shape as usize]
                                .fields
                                .iter()
                                .filter_map(|field| {
                                    let from =
                                        fields.iter().position(|own| own.name == field.name)?;
                                    Some((from as u32, slot_of(&field.name)?))
                                })
                                .collect(),
                        )
                    }
                },
            };
            branch.push(S::RecordOrder {
                target: output,
                shape: wide_id,
                source,
                mapping,
                start: false,
                slots,
            });
        }
        branch.push(S::RecordOrder {
            target: output,
            shape: wide_id,
            source: None,
            mapping: (0..wide.len() as u32).map(|slot| (slot, slot)).collect(),
            start: false,
            slots,
        });
        let value =
            subtyping::coerce(local_value(span, output, wide_ty), Some(result), self.types)?;
        if value.ty != result {
            return Err(CompilerError::Internal("widened view".into()));
        }
        branch.push(S::Return(Some(value)));
        Ok(branch)
    }
}

/// The shape of a record type.
fn record_shape(ty: Type) -> Result<u32> {
    match ty {
        Type::Record(id) => Ok(id),
        _ => Err(CompilerError::Internal("record shape".into())),
    }
}

/// Record `shape` without the `excluded` fields.
fn rest_type(shape: u32, excluded: &[String], types: &mut ir::ValueTypes) -> Result<Type> {
    if excluded.is_empty() {
        return Ok(Type::Record(shape));
    }
    let fields = types.records[shape as usize]
        .fields
        .iter()
        .filter(|field| !excluded.contains(&field.name))
        .cloned()
        .collect();
    records::intern(fields, types)
}

/// `{ ...name }`.
fn spread_of(name: &str, span: swc_common::Span) -> ast::ObjectLit {
    ast::ObjectLit {
        span,
        props: vec![ast::PropOrSpread::Spread(ast::SpreadElement {
            dot3_token: span,
            expr: Box::new(ast::Expr::Ident(ast::Ident::new(
                name.into(),
                span,
                Default::default(),
            ))),
        })],
    }
}

/// Reserve a synthesized function with a placeholder body.
fn reserve(
    types: &mut ir::ValueTypes,
    generated: &mut BTreeMap<usize, ir::Function>,
    next: &mut usize,
    params: Vec<Type>,
    result: Type,
) -> Result<usize> {
    let Some((source, span)) = types.layouts.origin.clone() else {
        return Err(CompilerError::Internal(
            "layout spread without origin".into(),
        ));
    };
    let index = *next;
    if index >= types.function_limit() {
        return Err(invalid("too many functions including layout spreads"));
    }
    *next += 1;
    generated.insert(
        index,
        ir::Function {
            shape: ir::ParameterShape::fixed(params.len()),
            self_slot: None,
            receiver: None,
            cells: Default::default(),
            captures: vec![],
            environment: false,
            generator: None,
            source,
            span,
            params: params.clone(),
            result,
            locals: params,
            body: vec![S::Trap],
        },
    );
    Ok(index)
}

/// After every body is lowered: write the copies (first, since a widen
/// reads their layouts) and the widens, until no new one is reserved.
pub(super) fn finish(
    types: &mut ir::ValueTypes,
    generated: &mut BTreeMap<usize, ir::Function>,
    next: &mut usize,
) -> Result<()> {
    let Some((source, span)) = types.layouts.origin.clone() else {
        return Ok(());
    };
    let functions_map = BTreeMap::new();
    let mut effects = BTreeMap::new();
    types.layouts.exact = true;
    loop {
        let copies: Vec<((u32, Vec<String>), usize)> = types
            .layouts
            .copies
            .iter()
            .filter(|(_, (_, written))| !written)
            .map(|(key, (index, _))| (key.clone(), *index))
            .collect();
        for ((shape, excluded), index) in &copies {
            let (function, layouts) = {
                let mut lower = Lower::root(
                    &source,
                    types,
                    &functions_map,
                    (generated, next, &mut effects),
                    *index,
                );
                lower.hidden(Type::Record(*shape));
                let (body, layouts) = lower
                    .copy_body(*shape, excluded, span)
                    .map_err(|error| source.locate(error, span))?;
                let view = Type::Record(*shape);
                let target = rest_type(*shape, excluded, lower.types)?;
                (lower.function(body, vec![view], target, span), layouts)
            };
            generated.insert(*index, function);
            types
                .layouts
                .copies
                .insert((*shape, excluded.clone()), (*index, true));
            if excluded.is_empty() {
                types.layouts.copy_shapes.insert(*shape, layouts);
            }
        }
        let widens: Vec<Widen> = types
            .layouts
            .widens
            .iter()
            .filter(|widen| !widen.written)
            .cloned()
            .collect();
        if copies.is_empty() && widens.is_empty() {
            break;
        }
        for widen in widens {
            let function = {
                let mut lower = Lower::root(
                    &source,
                    types,
                    &functions_map,
                    (generated, next, &mut effects),
                    widen.index,
                );
                let result = Type::Record(widen.result);
                let params: Vec<Type> = std::iter::once(result)
                    .chain(
                        widen
                            .copies
                            .iter()
                            .map(|(shape, nullable)| nullable.unwrap_or(Type::Record(*shape))),
                    )
                    .collect();
                for param in &params {
                    lower.hidden(*param);
                }
                let body = lower
                    .widen_body(&widen, span)
                    .map_err(|error| source.locate(error, span))?;
                lower.function(body, params, result, span)
            };
            generated.insert(widen.index, function);
            if let Some(entry) = types
                .layouts
                .widens
                .iter_mut()
                .find(|w| w.index == widen.index)
            {
                entry.written = true;
            }
        }
    }
    types.layouts.exact = false;
    Ok(())
}
