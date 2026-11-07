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
    /// The layout spread.
    Layout,
}

/// A `widen` function of one layout spread site.
#[derive(Clone)]
struct Widen {
    index: usize,
    /// The literal's static result shape.
    result: u32,
    /// The view shape the copy has.
    copy: u32,
    /// The copy's cell type when the source may be nullish.
    nullable: Option<Type>,
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

    /// `widen(result, copy)` of a layout spread site.
    pub(super) fn layout_widen(
        &mut self,
        result: Expr,
        copy: Expr,
        shape: u32,
        nullable: Option<Type>,
        parts: Vec<Part>,
    ) -> Result<Expr> {
        let span = result.span;
        let Type::Record(id) = result.ty else {
            return Err(CompilerError::Internal("layout spread result".into()));
        };
        let index = reserve(
            self.types,
            self.generated,
            self.next_function,
            vec![result.ty, copy.ty],
            result.ty,
        )?;
        self.types.layouts.widens.push(Widen {
            index,
            result: id,
            copy: shape,
            nullable,
            parts,
            written: false,
        });
        Ok(Expr {
            span,
            ty: result.ty,
            kind: E::Call(index, vec![result, copy]),
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
        if self.types.subtyping.class_sources(shape) {
            return Err(unsupported(&format!(
                "spread of a {} that may hold a class instance",
                subtyping::describe(view, self.types)
            )));
        }
        let mut layouts = self.types.subtyping.view_sources(shape);
        layouts.retain(|layout| *layout != shape);
        let mut body = Vec::new();
        let mut copies = Vec::new();
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

    /// The body of a `widen` function (local 0 the result, local 1 the copy).
    fn widen_body(&mut self, widen: &Widen, span: swc_common::Span) -> Result<Vec<S>> {
        let result = Type::Record(widen.result);
        let view = Type::Record(widen.copy);
        let copy_ty = widen.nullable.unwrap_or(view);
        let mut body = Vec::new();
        let copy = self.hidden(view);
        if widen.nullable.is_some() {
            body.push(S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsNullish(Box::new(local_value(span, 1, copy_ty))),
                },
                vec![S::Return(Some(local_value(span, 0, result)))],
                vec![],
            ));
            body.push(S::Init(
                copy,
                Expr {
                    span,
                    ty: view,
                    kind: E::UnboxUnion(Box::new(local_value(span, 1, copy_ty))),
                },
            ));
        } else {
            body.push(S::Init(copy, local_value(span, 1, view)));
        }
        let fields = self.types.records[widen.result as usize].fields.clone();
        let layouts = self
            .types
            .layouts
            .copy_shapes
            .get(&widen.copy)
            .cloned()
            .unwrap_or_default();
        for layout in layouts {
            let own = self.types.records[layout as usize].fields.clone();
            let extras: Vec<ir::RecordField> = own
                .iter()
                .filter(|field| !fields.iter().any(|known| known.name == field.name))
                .cloned()
                .collect();
            if extras.is_empty() {
                continue;
            }
            let mut wide = fields.clone();
            wide.extend(extras.iter().cloned());
            let wide_ty = records::intern(wide.clone(), self.types)?;
            let Type::Record(wide_id) = wide_ty else {
                return Err(CompilerError::Internal("widened layout".into()));
            };
            let wide = self.types.records[wide_id as usize].fields.clone();
            let slot_of = |name: &str| {
                wide.iter()
                    .position(|field| field.name == name)
                    .map(|slot| slot as u32)
            };
            let source = self.hidden(Type::Record(layout));
            let output = self.hidden(wide_ty);
            let mut branch = vec![S::Init(
                source,
                Expr {
                    span,
                    ty: Type::Record(layout),
                    kind: E::Reinterpret(Box::new(local_value(span, copy, view))),
                },
            )];
            // The result's fields (absent optional ones stay absent), then
            // the copy's extra fields.
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
                            kind: E::HasOwn(
                                Box::new(local_value(span, 0, result)),
                                Some(slot as u32),
                            ),
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
            for field in &extras {
                let target = slot_of(&field.name).ok_or_else(|| invalid("widened field"))?;
                let from = own
                    .iter()
                    .position(|own| own.name == field.name)
                    .ok_or_else(|| invalid("copied field"))?;
                values.push((
                    target,
                    unions::widen(
                        Expr {
                            span,
                            ty: field.ty,
                            kind: E::Field(
                                Box::new(local_value(span, source, Type::Record(layout))),
                                from as u32,
                            ),
                        },
                        Some(wide[target as usize].ty),
                        self.types,
                    )?,
                ));
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
            // Key order: the members again, the copy's keys at the spread.
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
                    Part::Layout => (
                        Some((source, layout)),
                        own.iter()
                            .enumerate()
                            .filter_map(|(from, field)| Some((from as u32, slot_of(&field.name)?)))
                            .collect(),
                    ),
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
            let descriptor = self.hidden(Type::Number);
            body.push(S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsClass {
                        value: Box::new(local_value(span, copy, view)),
                        shapes: vec![layout],
                        slot: descriptor,
                    },
                },
                branch,
                vec![],
            ));
        }
        body.push(S::Return(Some(local_value(span, 0, result))));
        Ok(body)
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
                let copy = widen.nullable.unwrap_or(Type::Record(widen.copy));
                lower.hidden(result);
                lower.hidden(copy);
                let body = lower
                    .widen_body(&widen, span)
                    .map_err(|error| source.locate(error, span))?;
                lower.function(body, vec![result, copy], result, span)
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
