//! V38 object-literal getters and setters (`expression_forms`).
//!
//! An object literal with `get p() {…}`/`set p(v) {…}` has accessor
//! properties, as in JavaScript: reading `o.p` calls the getter, assigning
//! it calls the setter (an assignment without a setter throws `TypeError`),
//! and spreads, `Object.keys`/`values`/`entries`, `JSON.stringify` and
//! enumeration read through the getter. The object's type is its contextual
//! record type, or the record of its data fields, annotated methods and
//! accessor types (a getter's return or a setter's parameter annotation).
//!
//! The object itself has a physical shape of its own: its data fields, and
//! for each accessor property the getter and setter functions in fields with
//! reserved names (`accessor_field`) that no source key can spell and that
//! hosts skip as private state. The literal's value is a view of that shape
//! as its type ([structural subtyping](crate::lower::subtyping)), so every
//! member access of the type selects the object's layout by its descriptor:
//! the view reads data fields directly and calls the accessor functions
//! with the object as their receiver. Getters and setters are closures that
//! take their receiver like object-literal methods (`object_methods`).
use super::object_methods::{Receiver, SELF};
use super::*;
use std::collections::BTreeSet;
use swc_ecma_visit::VisitMutWith;

/// The reserved field names of a property's getter and setter functions;
/// the space keeps them out of every source key.
const GETTER: &str = "#get ";
const SETTER: &str = "#set ";

/// The type of a present annotation.
fn annotated(annotation: &ast::TsTypeAnn, types: &mut ir::ValueTypes) -> Result<Type> {
    super::annotation(Some(annotation), types)?
        .ok_or_else(|| CompilerError::Internal("missing accessor annotation".into()))
}

/// Whether a field of an object with accessors is not a property: a getter
/// or a setter.
pub(crate) fn hidden_field(name: &str) -> bool {
    accessor_field(name).is_some()
}

/// The property a reserved accessor field belongs to, and whether it holds
/// the getter.
pub(crate) fn accessor_field(name: &str) -> Option<(&str, bool)> {
    name.strip_prefix(GETTER)
        .map(|property| (property, true))
        .or_else(|| name.strip_prefix(SETTER).map(|property| (property, false)))
}
pub(crate) fn getter_field(name: &str) -> String {
    format!("{GETTER}{name}")
}
pub(crate) fn setter_field(name: &str) -> String {
    format!("{SETTER}{name}")
}

impl Lower<'_> {
    /// An object literal with a getter or setter: its physical object viewed
    /// as its type.
    pub(super) fn accessor_literal(
        &mut self,
        object: &ast::ObjectLit,
        expected: Option<Type>,
    ) -> Result<Expr> {
        let span = object.span;
        let spreads = object
            .props
            .iter()
            .any(|prop| matches!(prop, ast::PropOrSpread::Spread(_)));
        let ty = match expected {
            Some(ty @ Type::Record(_)) => ty,
            None => self.accessor_type(object)?,
            Some(other) => {
                return Err(self.source.locate(
                    unsupported(&format!(
                        "object literal with getters or setters in a {other} context (expected a closed object type)"
                    )),
                    span,
                ));
            }
        };
        let Type::Record(logical) = ty else {
            return Err(CompilerError::Internal("accessor literal type".into()));
        };
        let field_type = |this: &Self, name: &str, at: swc_common::Span| -> Result<Type> {
            this.types.records[logical as usize]
                .fields
                .iter()
                .find(|field| field.name == name)
                .map(|field| field.ty)
                .ok_or_else(|| {
                    this.source.locate(
                        type_error(format!(
                            "object literal member {name} is not declared by its type {}",
                            subtyping::describe(ty, this.types)
                        )),
                        at,
                    )
                })
        };
        let mut values: Vec<(String, Expr)> = Vec::new();
        let mut getters = Vec::new();
        let mut setters = Vec::new();
        // With spreads (CopyDataProperties): every member is evaluated into a
        // local in source order, a spread copies its source's own properties
        // when it is reached (getters in the source's own-key order), a later
        // definition replaces an earlier one at its first position, and the
        // object's key order follows the members (`spread_order`).
        let mut prelude: Vec<S> = Vec::new();
        let mut roots: Vec<u32> = Vec::new();
        let mut members: Vec<super::record_spread::Member> = Vec::new();
        let mut explicit: Vec<String> = Vec::new();
        // Properties only optional spread members provide so far, and the
        // optional members to copy where their sources have them.
        let mut optional_names: Vec<String> = Vec::new();
        let mut pending: Vec<(String, Expr, u32, u32, u32)> = Vec::new();
        let mut ordered: Vec<String> = Vec::new();
        // V38 (`layout_spreads`): the copies of spreads of views of other
        // layouts, by their hidden slot and view shape.
        let mut layouts: Vec<(u32, u32)> = Vec::new();
        for prop in &object.props {
            let prop = match prop {
                ast::PropOrSpread::Prop(prop) => prop,
                ast::PropOrSpread::Spread(spread) => {
                    let dynamic = self.layout_spread(&spread.expr);
                    let value = self.expr(&spread.expr)?;
                    let Type::Record(shape) = value.ty else {
                        return Err(self.source.locate(
                            unsupported(&format!(
                                "spread of a {} into an object literal with getters or setters; spread a closed object",
                                subtyping::describe(value.ty, self.types)
                            )),
                            spread.expr.span(),
                        ));
                    };
                    let declared = self.types.records[shape as usize].fields.clone();
                    if self.types.records[shape as usize].class.is_some() {
                        return Err(self.source.locate(
                            unsupported(&format!(
                                "spread of a {} that is a class instance into an object literal with getters or setters",
                                subtyping::describe(value.ty, self.types)
                            )),
                            spread.expr.span(),
                        ));
                    }
                    if self.types.profile.sdk_paths() && !dynamic && !self.types.layouts.exact {
                        self.types.subtyping.note_key_copy(
                            shape,
                            if self.types.profile.expression_forms() {
                                subtyping::KeyCopy::LayoutSpread
                            } else {
                                subtyping::KeyCopy::Spread
                            },
                            self.source,
                            spread.expr.span(),
                        );
                    }
                    // A layout spread copies the object first (its getters
                    // run in its own-key order); the literal reads the copy
                    // and gains the copy's other properties (`layout_widen`).
                    let value = if dynamic {
                        self.layout_copy(value, shape, Vec::new())?
                    } else {
                        value
                    };
                    let slot = self.hidden(value.ty);
                    roots.push(slot);
                    prelude.push(S::Init(slot, value));
                    if dynamic {
                        layouts.push((slot, shape));
                    }
                    // The spread site is noted above; its key read is not
                    // another key copy.
                    let exact = std::mem::replace(&mut self.types.layouts.exact, true);
                    let read = self.read_in_key_order(slot, shape, &[], span);
                    self.types.layouts.exact = exact;
                    let (reads, locals) = read?;
                    roots.extend(locals.iter().copied());
                    prelude.extend(reads);
                    members.push(super::record_spread::Member::Spread(slot, shape, None));
                    for (index, (field, local)) in declared.iter().zip(locals).enumerate() {
                        let expected = field_type(self, &field.name, spread.expr.span())?;
                        let read = unions::widen(
                            local_value(span, local, field.ty),
                            Some(expected),
                            self.types,
                        )?;
                        // An optional member is copied only where the source
                        // has it (after the object is built); until a certain
                        // provider, the property is optional.
                        if field.optional {
                            if values.iter().any(|(name, _)| {
                                accessor_field(name)
                                    .is_some_and(|(property, _)| property == field.name)
                            }) {
                                return Err(self.source.locate(
                                    unsupported(&format!(
                                        "optional member {} of a spread after an accessor of that name",
                                        field.name
                                    )),
                                    spread.expr.span(),
                                ));
                            }
                            if !values.iter().any(|(name, _)| *name == field.name) {
                                if !unions::members(expected, self.types).contains(&Type::Undefined)
                                {
                                    return Err(self.source.locate(
                                        type_error(format!(
                                            "optional member {} of a spread for a required member",
                                            field.name
                                        )),
                                        spread.expr.span(),
                                    ));
                                }
                                let absent = unions::widen(
                                    Expr {
                                        span,
                                        ty: Type::Undefined,
                                        kind: E::Integer(0),
                                    },
                                    Some(expected),
                                    self.types,
                                )?;
                                values.push((field.name.clone(), absent));
                                optional_names.push(field.name.clone());
                            }
                            pending.push((field.name.clone(), read, slot, shape, index as u32));
                            continue;
                        }
                        optional_names.retain(|name| *name != field.name);
                        pending.retain(|(name, ..)| *name != field.name);
                        // A data property replaces an accessor of that name.
                        values.retain(|(name, _)| {
                            accessor_field(name).is_none_or(|(property, _)| property != field.name)
                        });
                        getters.retain(|name| *name != field.name);
                        setters.retain(|name| *name != field.name);
                        match values.iter_mut().find(|(name, _)| *name == field.name) {
                            Some((_, existing)) => *existing = read,
                            None => values.push((field.name.clone(), read)),
                        }
                    }
                    continue;
                }
            };
            let (name, value) = match &**prop {
                ast::Prop::KeyValue(pair) => {
                    let name = records::property_key(&pair.key)?;
                    let expected = field_type(self, &name, pair.key.span())?;
                    (name, self.expr_expected(&pair.value, Some(expected))?)
                }
                ast::Prop::Shorthand(id) => {
                    let name = records::key(id.sym.to_string())?;
                    let expected = field_type(self, &name, id.span)?;
                    let value = ast::Expr::Ident(id.clone());
                    (name, self.expr_expected(&value, Some(expected))?)
                }
                ast::Prop::Method(method) => {
                    let name = records::property_key(&method.key)?;
                    let expected = field_type(self, &name, method.function.span)?;
                    let value = if object_methods::bound(&method.function) {
                        let function = &method.function;
                        self.core.method_receivers.insert(
                            (function.span.lo.0, function.span.hi.0),
                            (SELF.to_owned(), ty),
                        );
                        let ast::Prop::KeyValue(pair) = self.bound_method(method)? else {
                            return Err(CompilerError::Internal("bound method form".into()));
                        };
                        self.expr_expected(&pair.value, Some(expected))?
                    } else {
                        self.object_method_value(method, Some(expected))?
                    };
                    (name, value)
                }
                ast::Prop::Getter(getter) => {
                    let name = records::property_key(&getter.key)?;
                    let expected = field_type(self, &name, getter.span)?;
                    let function = functions::intern_full(
                        None,
                        vec![],
                        ir::ParameterShape::fixed(0),
                        expected,
                        self.types,
                    )?;
                    let body = getter.function.body.clone().ok_or_else(|| {
                        self.source
                            .locate(invalid("getter without a body"), getter.span)
                    })?;
                    let arrow = self.accessor_arrow(
                        vec![],
                        body,
                        getter.function.return_type.clone(),
                        getter.span,
                        ty,
                    )?;
                    getters.push(name.clone());
                    (
                        getter_field(&name),
                        self.expr_expected(&arrow, Some(function))?,
                    )
                }
                ast::Prop::Setter(setter) => {
                    let name = records::property_key(&setter.key)?;
                    let expected = field_type(self, &name, setter.span)?;
                    if setter.function.this_param.is_some() {
                        return Err(self
                            .source
                            .locate(unsupported("this parameter of a setter"), setter.span));
                    }
                    let function = functions::intern_full(
                        None,
                        vec![expected],
                        ir::ParameterShape::fixed(1),
                        Type::Void,
                        self.types,
                    )?;
                    let body = setter.function.body.clone().ok_or_else(|| {
                        self.source
                            .locate(invalid("setter without a body"), setter.span)
                    })?;
                    let void = ast::TsTypeAnn {
                        span: setter.span,
                        type_ann: Box::new(ast::TsType::TsKeywordType(ast::TsKeywordType {
                            span: setter.span,
                            kind: ast::TsKeywordTypeKind::TsVoidKeyword,
                        })),
                    };
                    let arrow = self.accessor_arrow(
                        setter
                            .function
                            .params
                            .iter()
                            .map(|p| p.pat.clone())
                            .collect(),
                        body,
                        Some(Box::new(void)),
                        setter.span,
                        ty,
                    )?;
                    setters.push(name.clone());
                    (
                        setter_field(&name),
                        self.expr_expected(&arrow, Some(function))?,
                    )
                }
                _ => return Err(unsupported("object literal member form")),
            };
            if explicit.contains(&name) {
                return Err(self.source.locate(
                    unsupported(&format!("object literal member {name} defined twice")),
                    span,
                ));
            }
            explicit.push(name.clone());
            let property =
                accessor_field(&name).map_or(name.clone(), |(property, _)| property.to_owned());
            optional_names.retain(|known| *known != property);
            pending.retain(|(known, ..)| *known != property);
            if !ordered.contains(&property) {
                ordered.push(property.clone());
                members.push(super::record_spread::Member::Value(property.clone()));
            }
            let value = if spreads {
                let slot = self.hidden(value.ty);
                if value.ty.needs_root() {
                    roots.push(slot);
                }
                let read = local_value(span, slot, value.ty);
                prelude.push(S::Init(slot, value));
                read
            } else {
                value
            };
            // A definition after a spread replaces the spread's property in
            // its place: an accessor drops the data value, data replaces it.
            if accessor_field(&name).is_some() {
                if let Some((_, existing)) = values
                    .iter_mut()
                    .find(|(existing, _)| *existing == property)
                {
                    let cache = unions::intern(
                        vec![field_type(self, &property, span)?, Type::Undefined],
                        self.types,
                    )?;
                    let undefined = Expr {
                        span,
                        ty: Type::Undefined,
                        kind: E::Integer(0),
                    };
                    *existing = unions::widen(undefined, Some(cache), self.types)?;
                }
            } else if let Some((_, existing)) =
                values.iter_mut().find(|(existing, _)| *existing == name)
            {
                *existing = value;
                continue;
            }
            // An accessor property owns a data slot at its creation position
            // (its first definition): the cache readers see after a refresh.
            if let Some((property, _)) = accessor_field(&name)
                && !values.iter().any(|(existing, _)| existing == property)
            {
                let property = property.to_owned();
                let ty = field_type(self, &property, span)?;
                let cache = unions::intern(vec![ty, Type::Undefined], self.types)?;
                let undefined = Expr {
                    span,
                    ty: Type::Undefined,
                    kind: E::Integer(0),
                };
                values.push((property, unions::widen(undefined, Some(cache), self.types)?));
            }
            values.push((name, value));
        }
        if let Some(name) = setters.iter().find(|name| !getters.contains(name)) {
            return Err(self.source.locate(
                unsupported(&format!(
                    "setter {name} without a getter (reading it would be undefined)"
                )),
                span,
            ));
        }
        // Every declared member is provided: a data field, a method or an
        // accessor with a getter.
        for field in &self.types.records[logical as usize].fields.clone() {
            let provided = values.iter().any(|(name, _)| *name == field.name);
            if !provided && !field.optional {
                return Err(self.source.locate(
                    type_error(format!(
                        "object literal does not provide {} of {}",
                        field.name,
                        subtyping::describe(ty, self.types)
                    )),
                    span,
                ));
            }
        }
        let physical = records::intern_accessors(
            values
                .iter()
                .map(|(name, value)| ir::RecordField {
                    name: name.clone(),
                    ty: value.ty,
                    optional: optional_names.contains(name),
                })
                .collect(),
            self.types,
        )?;
        let Type::Record(shape) = physical else {
            return Err(CompilerError::Internal("accessor object shape".into()));
        };
        for name in getters.iter().filter(|name| !setters.contains(name)) {
            if self
                .types
                .receivers
                .read_only
                .contains_key(&(shape, name.clone()))
            {
                continue;
            }
            let error = self.array_error(
                varta_common::value::ErrorClass::TypeError,
                &format!("Cannot set property {name} of #<Object> which has only a getter"),
                span,
            )?;
            self.types
                .receivers
                .read_only
                .insert((shape, name.clone()), error);
        }
        let fields = values
            .into_iter()
            .map(|(name, value)| {
                let slot = self.types.records[shape as usize]
                    .fields
                    .iter()
                    .position(|field| field.name == name)
                    .ok_or_else(|| invalid("missing accessor object field"))?;
                Ok((slot as u32, value))
            })
            .collect::<Result<_>>()?;
        self.types.subtyping.note_view(physical, logical);
        self.types.receivers.logical.insert(shape, logical);
        let physical_fields = self.types.records[shape as usize].fields.clone();
        let built = Expr {
            span,
            ty: physical,
            kind: if physical_fields.iter().any(|field| field.optional) {
                let present = physical_fields
                    .iter()
                    .enumerate()
                    .filter(|(_, field)| !field.optional)
                    .fold(0u128, |mask, (slot, _)| mask | 1u128 << slot);
                E::OptionalRecord(fields, present)
            } else {
                E::Record(fields)
            },
        };
        if !spreads {
            return Ok(Expr {
                span,
                ty,
                kind: E::Reinterpret(Box::new(built)),
            });
        }
        let output = self.hidden(physical);
        roots.push(output);
        prelude.push(S::Init(output, built));
        for (name, read, source, from, index) in pending {
            let slot = physical_fields
                .iter()
                .position(|field| field.name == name)
                .ok_or_else(|| invalid("missing accessor object field"))?;
            let value = unions::widen(read, Some(physical_fields[slot].ty), self.types)?;
            prelude.push(S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::HasOwn(
                        Box::new(local_value(span, source, Type::Record(from))),
                        Some(index),
                    ),
                },
                vec![S::SetField(
                    local_value(span, output, physical),
                    slot as u32,
                    value,
                )],
                vec![],
            ));
        }
        // Reserved fields keep their places after the properties.
        for field in self.types.records[shape as usize].fields.clone() {
            if hidden_field(&field.name) {
                members.push(super::record_spread::Member::Value(field.name));
            }
        }
        prelude.extend(self.spread_order(output, shape, &members, span)?);
        if !layouts.is_empty() {
            let widened = self.accessor_widen(output, physical, &members, &layouts, span)?;
            prelude.push(S::Set(output, widened));
        }
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty,
            kind: E::Reinterpret(Box::new(Expr {
                span,
                ty: physical,
                kind: E::ValueBlock {
                    output,
                    marker,
                    roots,
                    body: prelude,
                },
            })),
        })
    }

    /// The object with accessors in local `output` (of physical shape
    /// `physical`) with the other own properties of its layout spreads'
    /// copies at their spreads (v38 `layout_spreads`): its widened layouts
    /// keep the accessor fields and are objects with accessors of the same
    /// type.
    fn accessor_widen(
        &mut self,
        output: u32,
        physical: Type,
        members: &[super::record_spread::Member],
        layouts: &[(u32, u32)],
        span: swc_common::Span,
    ) -> Result<Expr> {
        use super::layout_spreads::Part;
        let parts = members
            .iter()
            .map(|member| match member {
                super::record_spread::Member::Value(name) => Part::Value(name.clone()),
                super::record_spread::Member::Spread(slot, shape, _) => {
                    match layouts.iter().position(|(copy, _)| copy == slot) {
                        Some(index) => Part::Layout(index),
                        None => Part::Static(
                            self.types.records[*shape as usize]
                                .fields
                                .iter()
                                .map(|field| field.name.clone())
                                .collect(),
                        ),
                    }
                }
            })
            .collect();
        let copies = layouts
            .iter()
            .map(|(copy, shape)| (local_value(span, *copy, Type::Record(*shape)), *shape, None))
            .collect();
        self.layout_widen(local_value(span, output, physical), copies, parts)
    }

    /// The closure of an accessor body: an arrow whose `this` is the hidden
    /// receiver binding of `object_methods`, taken at entry.
    fn accessor_arrow(
        &mut self,
        params: Vec<ast::Pat>,
        mut body: ast::FunctionBody,
        return_type: Option<Box<ast::TsTypeAnn>>,
        span: swc_common::Span,
        ty: Type,
    ) -> Result<ast::Expr> {
        let mut params = params;
        let mut visitor = Receiver { sup: None };
        for param in &mut params {
            param.visit_mut_with(&mut visitor);
        }
        body.visit_mut_with(&mut visitor);
        if let Some(at) = visitor.sup {
            return Err(self
                .source
                .locate(unsupported("super in an object literal accessor"), at));
        }
        self.core
            .method_receivers
            .insert((span.lo.0, span.hi.0), (SELF.to_owned(), ty));
        Ok(ast::Expr::Arrow(ast::ArrowExpr {
            span,
            ctxt: Default::default(),
            params,
            body: Box::new(ast::ArrowFunctionBody::FunctionBody(body)),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type,
        }))
    }

    /// The record type of an accessor literal without a context: data field
    /// values, annotated methods and the annotated getter (or setter) types.
    fn accessor_type(&mut self, object: &ast::ObjectLit) -> Result<Type> {
        let mut fields: Vec<ir::RecordField> = Vec::with_capacity(object.props.len());
        for prop in &object.props {
            let prop = match prop {
                ast::PropOrSpread::Prop(prop) => prop,
                ast::PropOrSpread::Spread(spread) => {
                    let source = self.evident_type(&spread.expr, spread.expr.span())?;
                    let Type::Record(shape) = source else {
                        return Err(self.source.locate(
                            unsupported("spread of a value that is not a closed object into an unannotated object literal with getters or setters"),
                            spread.expr.span(),
                        ));
                    };
                    for field in self.types.records[shape as usize].fields.clone() {
                        match fields.iter_mut().find(|own| own.name == field.name) {
                            Some(own) => *own = field,
                            None => fields.push(field),
                        }
                    }
                    continue;
                }
            };
            let (name, ty) = match &**prop {
                ast::Prop::KeyValue(pair) => (
                    records::property_key(&pair.key)?,
                    self.evident_type(&pair.value, pair.value.span())?,
                ),
                ast::Prop::Shorthand(id) => (
                    id.sym.to_string(),
                    self.evident_type(&ast::Expr::Ident(id.clone()), id.span)?,
                ),
                ast::Prop::Method(method) => (
                    records::property_key(&method.key)?,
                    self.method_type(method)?,
                ),
                ast::Prop::Getter(getter) => {
                    let Some(annotation) = getter.function.return_type.as_deref() else {
                        return Err(self.source.locate(
                            unsupported(
                                "getter without a return type annotation in an unannotated object literal",
                            ),
                            getter.span,
                        ));
                    };
                    (
                        records::property_key(&getter.key)?,
                        literals::erased(annotated(annotation, self.types)?, self.types),
                    )
                }
                ast::Prop::Setter(setter) => {
                    let name = records::property_key(&setter.key)?;
                    if fields.iter().any(|field| field.name == name) {
                        continue;
                    }
                    let Some(ast::Param {
                        pat: ast::Pat::Ident(param),
                        ..
                    }) = setter.function.params.first()
                    else {
                        return Err(self.source.locate(
                            unsupported(
                                "setter parameter pattern in an unannotated object literal",
                            ),
                            setter.span,
                        ));
                    };
                    let Some(annotation) = param.type_ann.as_deref() else {
                        return Err(self.source.locate(
                            unsupported(
                                "setter without a parameter type annotation in an unannotated object literal",
                            ),
                            setter.span,
                        ));
                    };
                    (name, annotated(annotation, self.types)?)
                }
                _ => return Err(unsupported("object literal member form")),
            };
            // A getter after its setter (or the reverse) names one property.
            if let Some(field) = fields.iter_mut().find(|field| field.name == name) {
                field.ty = ty;
                continue;
            }
            fields.push(ir::RecordField {
                name,
                ty,
                optional: false,
            });
        }
        records::intern(fields, self.types)
    }
}

impl Lower<'_> {
    /// A value a structured reader (KV staging; `structuredClone`) reads
    /// as data (v38): in a program with accessor objects, the value after a
    /// structured refresh — every object with accessors reachable from it is
    /// visited once by identity, depth first in own-key order as
    /// StructuredSerializeInternal reads it, and each getter's result is
    /// written into its property's data slot, which the reader then sees.
    pub(super) fn structured_refreshed(&mut self, value: Expr) -> Result<Expr> {
        if !self.types.receivers.accessors
            || !matches!(
                value.ty,
                Type::Record(_)
                    | Type::Array(_)
                    | Type::Dictionary(_)
                    | Type::Class(_)
                    | Type::Unknown
                    | Type::Union(_)
                    | Type::ReferenceUnion(_)
            )
        {
            return Ok(value);
        }
        let span = value.span;
        self.types
            .receivers
            .origin
            .get_or_insert_with(|| (self.source.clone(), span));
        let ty = value.ty;
        let index = refresher(ty, self.types, self.generated, self.next_function)?;
        let seen = identities(self.types)?;
        let slot = self.hidden(ty);
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![slot],
                body: vec![
                    S::Init(slot, value),
                    S::Expr(Expr {
                        span,
                        ty: Type::Void,
                        kind: E::Call(
                            index,
                            vec![
                                local_value(span, slot, ty),
                                Expr {
                                    span,
                                    ty: seen,
                                    kind: E::Array(Vec::new()),
                                },
                            ],
                        ),
                    }),
                    S::Init(output, local_value(span, slot, ty)),
                ],
            },
        })
    }

    /// The body of the structured refresh of `ty` (local 0; local 1 holds the
    /// identities of the objects the refresh has seen).
    fn refresh_walk(
        &mut self,
        ty: Type,
        views: &BTreeSet<u32>,
        span: swc_common::Span,
    ) -> Result<Vec<S>> {
        if !reaches(ty, views, self.types, &mut BTreeSet::new()) {
            return Ok(vec![S::Return(None)]);
        }
        let mut body = Vec::new();
        // An object of a layout with other own keys is refreshed as itself.
        if let Type::Record(shape) = ty {
            for (test, value) in self.wider_layouts(shape, span) {
                if let Some(call) = self.refresh_call(value, span)? {
                    body.push(S::If(test, vec![call, S::Return(None)], vec![]));
                }
            }
        }
        if let Type::Class(class) = ty {
            let seen = identities(self.types)?;
            for (test, derived) in self.subclass_tests(class, span)? {
                let index = refresher(derived.ty, self.types, self.generated, self.next_function)?;
                body.push(S::If(
                    test,
                    vec![
                        S::Expr(Expr {
                            span,
                            ty: Type::Void,
                            kind: E::Call(index, vec![derived, local_value(span, 1, seen)]),
                        }),
                        S::Return(None),
                    ],
                    vec![],
                ));
            }
        }
        if matches!(
            ty,
            Type::Record(_) | Type::Array(_) | Type::Dictionary(_) | Type::Class(_)
        ) {
            body.extend(self.refresh_enter(ty, span)?);
        }
        match ty {
            Type::Record(shape) => {
                let record = Type::Record(shape);
                let accessors = accessor_shapes(shape, self.types);
                let fields = self.types.records[shape as usize].fields.clone();
                let keys = self.enumerate_value(
                    local_value(span, 0, record),
                    super::object_enumeration::Kind::Keys,
                    span,
                )?;
                let keys_ty = keys.ty;
                let (key_slot, index) = (self.hidden(keys_ty), self.hidden(Type::Float64));
                body.extend([S::Init(key_slot, keys), S::Init(index, float(span, 0.0))]);
                let key = Expr {
                    span,
                    ty: Type::String,
                    kind: E::Index(
                        Box::new(local_value(span, key_slot, keys_ty)),
                        Box::new(local_value(span, index, Type::Float64)),
                    ),
                };
                let mut dispatch = Vec::new();
                for (slot, field) in fields.iter().enumerate().rev() {
                    let value = self.hidden(field.ty);
                    let mut branch = vec![S::Init(
                        value,
                        Expr {
                            span,
                            ty: field.ty,
                            kind: E::Field(Box::new(local_value(span, 0, record)), slot as u32),
                        },
                    )];
                    // The getter's result is the property's data.
                    for physical in &accessors {
                        let fields = &self.types.records[*physical as usize].fields;
                        if !is_accessor(*physical, &field.name, self.types) {
                            continue;
                        }
                        let Some(cache) = fields.iter().position(|own| own.name == field.name)
                        else {
                            continue;
                        };
                        let cache_ty = fields[cache].ty;
                        let descriptor = self.hidden(Type::Number);
                        let stored = unions::widen(
                            local_value(span, value, field.ty),
                            Some(cache_ty),
                            self.types,
                        )?;
                        branch.push(S::If(
                            Expr {
                                span,
                                ty: Type::Boolean,
                                kind: E::IsClass {
                                    value: Box::new(local_value(span, 0, record)),
                                    shapes: vec![*physical],
                                    slot: descriptor,
                                },
                            },
                            vec![S::SetField(
                                local_value(span, 0, Type::Record(*physical)),
                                cache as u32,
                                stored,
                            )],
                            vec![],
                        ));
                    }
                    if let Some(call) =
                        self.refresh_call(local_value(span, value, field.ty), span)?
                    {
                        branch.push(call);
                    }
                    let test = Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Binary(
                            ast::BinaryOp::EqEqEq,
                            Box::new(key.clone()),
                            Box::new(string(span, &field.name, self.types)),
                        ),
                    };
                    dispatch = vec![S::If(test, branch, dispatch)];
                }
                dispatch.push(S::Set(index, increment(span, index)));
                body.push(S::While(
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Binary(
                            ast::BinaryOp::Lt,
                            Box::new(local_value(span, index, Type::Float64)),
                            Box::new(Expr {
                                span,
                                ty: Type::Float64,
                                kind: E::Length(Box::new(local_value(span, key_slot, keys_ty))),
                            }),
                        ),
                    },
                    dispatch,
                ));
            }
            Type::Class(class) => {
                let (record, fields) = self.class_fields(class, span)?;
                let source = self.hidden(record);
                body.push(S::Init(
                    source,
                    Expr {
                        span,
                        ty: record,
                        kind: E::Reinterpret(Box::new(local_value(span, 0, ty))),
                    },
                ));
                for (slot, field) in fields {
                    let read = Expr {
                        span,
                        ty: field.ty,
                        kind: E::Field(Box::new(local_value(span, source, record)), slot),
                    };
                    if let Some(call) = self.refresh_call(read, span)? {
                        body.push(call);
                    }
                }
            }
            Type::Unknown => {
                for (test, held) in self.unknown_holders(views, span)? {
                    if let Some(call) = self.refresh_call(held, span)? {
                        body.push(S::If(test, vec![call], vec![]));
                    }
                }
            }
            Type::Dictionary(_) => {
                let walk = self.own_key_loop(ty, span, |this, key| {
                    let read = this.dictionary_get(local_value(span, 0, ty), key)?;
                    Ok(this.refresh_call(read, span)?.into_iter().collect())
                })?;
                body.extend(walk);
            }
            Type::Array(id) => {
                let array = Type::Array(id);
                let element = self.types.arrays[id as usize];
                let index = self.hidden(Type::Float64);
                body.push(S::Init(index, float(span, 0.0)));
                let read = Expr {
                    span,
                    ty: element,
                    kind: E::Index(
                        Box::new(local_value(span, 0, array)),
                        Box::new(local_value(span, index, Type::Float64)),
                    ),
                };
                let mut step = Vec::new();
                if let Some(call) = self.refresh_call(read, span)? {
                    step.push(call);
                }
                step.push(S::Set(index, increment(span, index)));
                body.push(S::While(
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Binary(
                            ast::BinaryOp::Lt,
                            Box::new(local_value(span, index, Type::Float64)),
                            Box::new(Expr {
                                span,
                                ty: Type::Float64,
                                kind: E::Length(Box::new(local_value(span, 0, array))),
                            }),
                        ),
                    },
                    step,
                ));
            }
            Type::Union(id) | Type::ReferenceUnion(id) => {
                for member in self.types.unions[id as usize].clone() {
                    if !matches!(
                        member,
                        Type::Record(_) | Type::Array(_) | Type::Dictionary(_) | Type::Class(_)
                    ) {
                        continue;
                    }
                    let value = Expr {
                        span,
                        ty: member,
                        kind: E::UnboxUnion(Box::new(local_value(span, 0, ty))),
                    };
                    if let Some(call) = self.refresh_call(value, span)? {
                        body.push(S::If(
                            Expr {
                                span,
                                ty: Type::Boolean,
                                kind: E::IsUnionMember(Box::new(local_value(span, 0, ty)), member),
                            },
                            vec![call],
                            vec![],
                        ));
                    }
                }
            }
            other => {
                return Err(unsupported(&format!(
                    "a structured read of a {} that may hold an object with getters",
                    subtyping::describe(other, self.types)
                )));
            }
        }
        body.push(S::Return(None));
        Ok(body)
    }

    /// The start of an object's refresh (local 0): an object the refresh has
    /// seen (local 1) returns, as StructuredSerializeInternal's memory does,
    /// so every object is read once and a cycle ends; otherwise it is seen.
    fn refresh_enter(&mut self, ty: Type, span: swc_common::Span) -> Result<Vec<S>> {
        let number = self.types.number();
        let seen = identities(self.types)?;
        let identity = self.hidden(number);
        Ok(vec![
            S::Init(
                identity,
                Expr {
                    span,
                    ty: number,
                    kind: E::Intrinsic(ir::Intrinsic::Identity, vec![local_value(span, 0, ty)]),
                },
            ),
            S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::ArraySearch(
                        ir::ArraySearch::Includes,
                        Box::new(local_value(span, 1, seen)),
                        Box::new(local_value(span, identity, number)),
                        Box::new(float(span, 0.0)),
                    ),
                },
                vec![S::Return(None)],
                vec![],
            ),
            push(
                span,
                local_value(span, 1, seen),
                local_value(span, identity, number),
            ),
        ])
    }

    /// The refresh of a nested value, or `None` for a value that holds no
    /// object.
    fn refresh_call(&mut self, value: Expr, span: swc_common::Span) -> Result<Option<S>> {
        if !matches!(
            value.ty,
            Type::Record(_)
                | Type::Array(_)
                | Type::Dictionary(_)
                | Type::Class(_)
                | Type::Unknown
                | Type::Union(_)
                | Type::ReferenceUnion(_)
        ) {
            return Ok(None);
        }
        let seen = identities(self.types)?;
        let index = refresher(value.ty, self.types, self.generated, self.next_function)?;
        Ok(Some(S::Expr(Expr {
            span,
            ty: Type::Void,
            kind: E::Call(index, vec![value, local_value(span, 1, seen)]),
        })))
    }

    /// `JSON.stringify(value)` of a record, array or union in a program with
    /// accessor objects (v38): the value's serializer, which is the host
    /// serializer for types that cannot hold an object with accessors and a
    /// guest walk for those that can (`finish`). `ty` is the result type the
    /// host serialization has.
    pub(super) fn json_serialized(&mut self, value: Expr, ty: Type) -> Result<Option<Expr>> {
        if !self.types.receivers.accessors
            || !matches!(
                value.ty,
                Type::Record(_)
                    | Type::Array(_)
                    | Type::Dictionary(_)
                    | Type::Class(_)
                    | Type::Unknown
                    | Type::Union(_)
                    | Type::ReferenceUnion(_)
            )
        {
            return Ok(None);
        }
        let span = value.span;
        self.types
            .receivers
            .origin
            .get_or_insert_with(|| (self.source.clone(), span));
        let index = serializer(value.ty, self.types, self.generated, self.next_function)?;
        let (identities, path) = (identities(self.types)?, path_type(self.types)?);
        let text = Expr {
            span,
            ty: text_type(self.types)?,
            kind: E::Call(
                index,
                vec![
                    value,
                    Expr {
                        span,
                        ty: identities,
                        kind: E::Array(Vec::new()),
                    },
                    Expr {
                        span,
                        ty: path,
                        kind: E::Array(Vec::new()),
                    },
                    string(span, "", self.types),
                ],
            ),
        };
        // The host serialization's type: a `string` result is the text.
        if ty == Type::String {
            return Ok(Some(Expr {
                span,
                ty,
                kind: E::UnboxUnion(Box::new(text)),
            }));
        }
        Ok(Some(text))
    }

    /// `JSON.stringify` of `value` by the host serializer, as the call
    /// itself lowers it: the text, or `undefined` for a value without one.
    fn json_host_text(&mut self, value: Expr, span: swc_common::Span) -> Result<Expr> {
        // Symbol values, like functions, have no JSON text (v38).
        let function = |ty: &Type| matches!(ty, Type::Function(_) | Type::Symbol);
        let ty = if value.ty == Type::Unknown
            || value.ty.is_union()
                && unions::members(value.ty, self.types)
                    .iter()
                    .any(|ty| *ty == Type::Undefined || function(ty))
        {
            text_type(self.types)?
        } else if value.ty == Type::Undefined || function(&value.ty) {
            Type::Undefined
        } else {
            Type::String
        };
        let status = self
            .types
            .profile
            .bigint_values()
            .then(|| self.hidden(Type::Number));
        let result = Expr {
            span,
            ty,
            kind: E::Json(Box::new(value), status),
        };
        let result = match status {
            Some(status) => self.bigint_checked_result(result, status)?,
            None => result,
        };
        unions::widen(result, Some(text_type(self.types)?), self.types)
    }

    /// The body of the serializer of `ty` (local 0; locals 1 to 3 are the
    /// walk's ancestor identities, their keys and constructors, and the key
    /// the value was reached by): the host serializer when no object with
    /// accessors can be reached, else a walk that reads each own property
    /// when it is reached (calling its getter) and serializes it at once, as
    /// SerializeJSONProperty does.
    fn json_walk(
        &mut self,
        ty: Type,
        views: &BTreeSet<u32>,
        span: swc_common::Span,
    ) -> Result<Vec<S>> {
        let param = |ty: Type| local_value(span, 0, ty);
        if let Type::Record(shape) = ty
            && let Some(snapshot) = snapshots::physical(shape, self.types)
        {
            // A snapshot's retained keys are dynamic even when its declared
            // names equal another logical layout. Walk its complete graph,
            // while retaining the original object's cycle identity.
            let mut body = self.json_enter(ty, "Object", span)?;
            let graph = dictionaries::intern(Type::Unknown, self.types)?;
            let graph = Expr {
                span,
                ty: graph,
                kind: E::Call(snapshot.graph, vec![param(ty)]),
            };
            let value = self.json_value(graph, local_value(span, JSON_KEY, Type::String), span)?;
            let text = text_type(self.types)?;
            let saved = self.hidden(text);
            body.push(S::Init(saved, value));
            body.extend(self.json_leave(span)?);
            body.push(S::Return(Some(local_value(span, saved, text))));
            return Ok(body);
        }
        if !reaches(ty, views, self.types, &mut BTreeSet::new()) {
            let value = self.json_host_text(param(ty), span)?;
            return Ok(vec![S::Return(Some(value))]);
        }
        match ty {
            Type::Record(shape) => self.json_record(shape, span),
            Type::Array(id) => self.json_array(id, span),
            Type::Dictionary(id) => self.json_dictionary(id, span),
            Type::Class(class) => self.json_class(class, span),
            Type::Unknown => {
                let mut body = Vec::new();
                for (test, held) in self.unknown_holders(views, span)? {
                    let value =
                        self.json_value(held, local_value(span, JSON_KEY, Type::String), span)?;
                    body.push(S::If(test, vec![S::Return(Some(value))], vec![]));
                }
                let rest = self.json_host_text(param(ty), span)?;
                body.push(S::Return(Some(rest)));
                Ok(body)
            }
            Type::Union(id) | Type::ReferenceUnion(id) => {
                let mut body = Vec::new();
                for member in self.types.unions[id as usize].clone() {
                    if !matches!(
                        member,
                        Type::Record(_) | Type::Array(_) | Type::Dictionary(_) | Type::Class(_)
                    ) || !reaches(member, views, self.types, &mut BTreeSet::new())
                    {
                        continue;
                    }
                    let value = self.json_value(
                        Expr {
                            span,
                            ty: member,
                            kind: E::UnboxUnion(Box::new(param(ty))),
                        },
                        local_value(span, JSON_KEY, Type::String),
                        span,
                    )?;
                    body.push(S::If(
                        Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::IsUnionMember(Box::new(param(ty)), member),
                        },
                        vec![S::Return(Some(value))],
                        vec![],
                    ));
                }
                let rest = self.json_host_text(param(ty), span)?;
                body.push(S::Return(Some(rest)));
                Ok(body)
            }
            other => Err(unsupported(&format!(
                "JSON.stringify of a {} that may hold an object with getters",
                subtyping::describe(other, self.types)
            ))),
        }
    }

    /// The serialized text of a nested value (`string | undefined`) reached
    /// by `key` (its description in a circular-structure message).
    fn json_value(&mut self, value: Expr, key: Expr, span: swc_common::Span) -> Result<Expr> {
        if matches!(
            value.ty,
            Type::Record(_)
                | Type::Array(_)
                | Type::Dictionary(_)
                | Type::Class(_)
                | Type::Unknown
                | Type::Union(_)
                | Type::ReferenceUnion(_)
        ) {
            let index = serializer(value.ty, self.types, self.generated, self.next_function)?;
            let (identities, path) = (identities(self.types)?, path_type(self.types)?);
            return Ok(Expr {
                span,
                ty: text_type(self.types)?,
                kind: E::Call(
                    index,
                    vec![
                        value,
                        local_value(span, JSON_IDENTITIES, identities),
                        local_value(span, JSON_PATH, path),
                        key,
                    ],
                ),
            });
        }
        self.json_host_text(value, span)
    }

    /// The cycle check of the object in local 0 (SerializeJSONObject and
    /// SerializeJSONArray step 1): an object already on the walk's stack
    /// throws V8's `TypeError: Converting circular structure to JSON`
    /// message; otherwise the object is pushed with its key and constructor.
    fn json_enter(
        &mut self,
        ty: Type,
        constructor: &str,
        span: swc_common::Span,
    ) -> Result<Vec<S>> {
        let number = self.types.number();
        let (identities, path) = (identities(self.types)?, path_type(self.types)?);
        let ids = || local_value(span, JSON_IDENTITIES, identities);
        let trail = || local_value(span, JSON_PATH, path);
        let (identity, start, message, cursor, end) = (
            self.hidden(number),
            self.hidden(number),
            self.hidden(Type::String),
            self.hidden(number),
            self.hidden(number),
        );
        let num = |slot| local_value(span, slot, number);
        let text = |this: &Self, value: &str| string(span, value, this.types);
        let path_at = |level: Expr, offset: f64| Expr {
            span,
            ty: Type::String,
            kind: E::Index(
                Box::new(trail()),
                Box::new(binary(
                    span,
                    ast::BinaryOp::Add,
                    binary(span, ast::BinaryOp::Mul, level, float(span, 2.0)),
                    float(span, offset),
                )),
            ),
        };
        let length = Expr {
            span,
            ty: number,
            kind: E::Length(Box::new(ids())),
        };
        // `|     <key> -> object with constructor '<C>'` of level `cursor`.
        let line = concat(
            span,
            vec![
                local_value(span, message, Type::String),
                text(self, "\n    |     "),
                path_at(num(cursor), 0.0),
                text(self, " -> object with constructor '"),
                path_at(num(cursor), 1.0),
                text(self, "'"),
            ],
        );
        let lines = |until: u32| {
            S::While(
                binary(span, ast::BinaryOp::Lt, num(cursor), num(until)),
                vec![
                    S::Set(message, line.clone()),
                    S::Set(cursor, increment(span, cursor)),
                ],
            )
        };
        let target = unions::intern(vec![Type::String, Type::Undefined], self.types)?;
        let error = Expr {
            span,
            ty: Type::from_error_class(varta_common::value::ErrorClass::TypeError),
            kind: E::NewError(
                varta_common::value::ErrorClass::TypeError,
                Box::new(unions::widen(
                    local_value(span, message, Type::String),
                    Some(target),
                    self.types,
                )?),
            ),
        };
        let thrown = self.exception_type()?;
        let error = unions::widen(error, Some(thrown), self.types)?;
        // V8 prints the first two levels after the start, an ellipsis for
        // more than three, and the last one.
        let circular = vec![
            S::Set(
                message,
                concat(
                    span,
                    vec![
                        text(
                            self,
                            "Converting circular structure to JSON\n    --> starting at object with constructor '",
                        ),
                        path_at(num(start), 1.0),
                        text(self, "'"),
                    ],
                ),
            ),
            S::Set(
                cursor,
                binary(span, ast::BinaryOp::Add, num(start), float(span, 1.0)),
            ),
            S::Init(
                end,
                binary(span, ast::BinaryOp::Add, num(start), float(span, 3.0)),
            ),
            S::If(
                binary(span, ast::BinaryOp::Gt, num(end), length.clone()),
                vec![S::Set(end, length.clone())],
                vec![],
            ),
            lines(end),
            S::If(
                binary(
                    span,
                    ast::BinaryOp::Gt,
                    length.clone(),
                    binary(span, ast::BinaryOp::Add, num(start), float(span, 4.0)),
                ),
                vec![S::Set(
                    message,
                    concat(
                        span,
                        vec![
                            local_value(span, message, Type::String),
                            text(self, "\n    |     ..."),
                        ],
                    ),
                )],
                vec![],
            ),
            S::If(
                binary(
                    span,
                    ast::BinaryOp::Lt,
                    num(cursor),
                    binary(span, ast::BinaryOp::Sub, length.clone(), float(span, 1.0)),
                ),
                vec![S::Set(
                    cursor,
                    binary(span, ast::BinaryOp::Sub, length.clone(), float(span, 1.0)),
                )],
                vec![],
            ),
            S::Set(end, length.clone()),
            lines(end),
            S::Set(
                message,
                concat(
                    span,
                    vec![
                        local_value(span, message, Type::String),
                        text(self, "\n    --- "),
                        local_value(span, JSON_KEY, Type::String),
                        text(self, " closes the circle"),
                    ],
                ),
            ),
            S::Throw(error),
        ];
        let entry = Expr {
            span,
            ty: path,
            kind: E::Array(vec![
                Some(local_value(span, JSON_KEY, Type::String)),
                Some(text(self, constructor)),
            ]),
        };
        Ok(vec![
            S::Init(
                identity,
                Expr {
                    span,
                    ty: number,
                    kind: E::Intrinsic(ir::Intrinsic::Identity, vec![local_value(span, 0, ty)]),
                },
            ),
            S::Init(
                start,
                Expr {
                    span,
                    ty: number,
                    kind: E::ArraySearch(
                        ir::ArraySearch::IndexOf,
                        Box::new(ids()),
                        Box::new(num(identity)),
                        Box::new(float(span, 0.0)),
                    ),
                },
            ),
            S::Init(message, text(self, "")),
            S::Init(cursor, float(span, 0.0)),
            S::Init(end, float(span, 0.0)),
            S::If(
                binary(span, ast::BinaryOp::GtEq, num(start), float(span, 0.0)),
                circular,
                vec![],
            ),
            push(span, ids(), num(identity)),
            S::Expr(Expr {
                span,
                ty: number,
                kind: E::ArrayPush(Box::new(trail()), Box::new(entry)),
            }),
        ])
    }

    /// The end of an object's serialization: it leaves the walk's stack.
    fn json_leave(&mut self, span: swc_common::Span) -> Result<Vec<S>> {
        let number = self.types.number();
        let (identities, path) = (identities(self.types)?, path_type(self.types)?);
        let popped = |array: Expr, element: Type, types: &mut ir::ValueTypes| -> Result<S> {
            Ok(S::Expr(Expr {
                span,
                ty: unions::intern(vec![element, Type::Undefined], types)?,
                kind: E::ArrayPop(Box::new(array)),
            }))
        };
        Ok(vec![
            popped(
                local_value(span, JSON_IDENTITIES, identities),
                number,
                self.types,
            )?,
            popped(local_value(span, JSON_PATH, path), Type::String, self.types)?,
            popped(local_value(span, JSON_PATH, path), Type::String, self.types)?,
        ])
    }

    /// `{"key":value,…}` over the object's own keys in order; a property
    /// whose value has no JSON text is left out.
    fn json_record(&mut self, shape: u32, span: swc_common::Span) -> Result<Vec<S>> {
        let record = Type::Record(shape);
        let text = text_type(self.types)?;
        let fields = self.types.records[shape as usize].fields.clone();
        // An object of a layout with other own keys serializes as itself.
        let mut body = Vec::new();
        for (test, value) in self.wider_layouts(shape, span) {
            let serialized =
                self.json_value(value, local_value(span, JSON_KEY, Type::String), span)?;
            body.push(S::If(test, vec![S::Return(Some(serialized))], vec![]));
        }
        body.extend(self.json_enter(record, "Object", span)?);
        let source = self.hidden(record);
        body.push(S::Init(source, local_value(span, 0, record)));
        let keys = self.enumerate_value(
            local_value(span, source, record),
            super::object_enumeration::Kind::Keys,
            span,
        )?;
        let keys_ty = keys.ty;
        let (key_slot, index, out, separator, value, described) = (
            self.hidden(keys_ty),
            self.hidden(Type::Float64),
            self.hidden(Type::String),
            self.hidden(Type::String),
            self.hidden(text),
            self.hidden(Type::String),
        );
        body.extend([
            S::Init(key_slot, keys),
            S::Init(index, float(span, 0.0)),
            S::Init(out, string(span, "{", self.types)),
            S::Init(separator, string(span, "", self.types)),
            S::Init(value, self.json_host_text(undefined(span), span)?),
            S::Init(described, string(span, "", self.types)),
        ]);
        let key = Expr {
            span,
            ty: Type::String,
            kind: E::Index(
                Box::new(local_value(span, key_slot, keys_ty)),
                Box::new(local_value(span, index, Type::Float64)),
            ),
        };
        let mut dispatch = Vec::new();
        for (slot, field) in fields.iter().enumerate().rev() {
            let read = Expr {
                span,
                ty: field.ty,
                kind: E::Field(Box::new(local_value(span, source, record)), slot as u32),
            };
            let serialized =
                self.json_value(read, local_value(span, described, Type::String), span)?;
            let test = Expr {
                span,
                ty: Type::Boolean,
                kind: E::Binary(
                    ast::BinaryOp::EqEqEq,
                    Box::new(key.clone()),
                    Box::new(string(span, &field.name, self.types)),
                ),
            };
            dispatch = vec![S::If(test, vec![S::Set(value, serialized)], dispatch)];
        }
        // `property '<key>'` names the key in a circular-structure message.
        dispatch.insert(
            0,
            S::Set(
                described,
                concat(
                    span,
                    vec![
                        string(span, "property '", self.types),
                        key.clone(),
                        string(span, "'", self.types),
                    ],
                ),
            ),
        );
        let quoted = self.json_host_text(key.clone(), span)?;
        let quoted = Expr {
            span,
            ty: Type::String,
            kind: E::UnboxUnion(Box::new(quoted)),
        };
        let member = concat(
            span,
            vec![
                local_value(span, out, Type::String),
                local_value(span, separator, Type::String),
                quoted,
                string(span, ":", self.types),
                Expr {
                    span,
                    ty: Type::String,
                    kind: E::UnboxUnion(Box::new(local_value(span, value, text))),
                },
            ],
        );
        dispatch.push(S::If(
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::IsUndefined(Box::new(local_value(span, value, text))),
            },
            vec![],
            vec![
                S::Set(out, member),
                S::Set(separator, string(span, ",", self.types)),
            ],
        ));
        dispatch.push(S::Set(index, increment(span, index)));
        body.push(S::While(
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Binary(
                    ast::BinaryOp::Lt,
                    Box::new(local_value(span, index, Type::Float64)),
                    Box::new(Expr {
                        span,
                        ty: Type::Float64,
                        kind: E::Length(Box::new(local_value(span, key_slot, keys_ty))),
                    }),
                ),
            },
            dispatch,
        ));
        body.extend(self.json_leave(span)?);
        let closed = concat(
            span,
            vec![
                local_value(span, out, Type::String),
                string(span, "}", self.types),
            ],
        );
        let closed = unions::widen(closed, Some(text), self.types)?;
        body.push(S::Return(Some(closed)));
        Ok(body)
    }

    /// The instance layout of `class` and its own enumerable fields in
    /// own-key order: the declared fields, which every instance defines at
    /// construction in declaration order (base class first); private names
    /// are not properties. A class with an index signature (keys added at
    /// run time) is refused by name.
    pub(super) fn class_fields(
        &mut self,
        class: u32,
        span: swc_common::Span,
    ) -> Result<(Type, Vec<(u32, ir::RecordField)>)> {
        let shape = self.types.classes.shape(class);
        let fields = &self.types.records[shape as usize].fields;
        if fields
            .iter()
            .any(|field| field.name == super::classes::INDEX_FIELD)
        {
            return Err(self.source.locate(
                unsupported(&format!(
                    "a host read of a {} with an index signature that may hold an object with getters",
                    subtyping::describe(Type::Class(class), self.types)
                )),
                span,
            ));
        }
        let own = fields
            .iter()
            .enumerate()
            .filter(|(_, field)| !super::classes::is_private_name(&field.name))
            .map(|(slot, field)| (slot as u32, field.clone()))
            .collect();
        Ok((Type::Record(shape), own))
    }

    /// The values an `unknown` cell (local 0) may hold that may hold an
    /// object with accessors, each with the test that the cell holds it:
    /// records (objects with accessors included), class instances and
    /// dictionaries by their exact descriptors, and arrays as `unknown[]`.
    fn unknown_holders(
        &mut self,
        views: &BTreeSet<u32>,
        span: swc_common::Span,
    ) -> Result<Vec<(Expr, Expr)>> {
        let mut targets: Vec<Type> = Vec::new();
        for id in 0..self.types.records.len() as u32 {
            if self.types.records[id as usize].class.is_none()
                && reaches(Type::Record(id), views, self.types, &mut BTreeSet::new())
            {
                targets.push(Type::Record(id));
            }
        }
        for id in 0..self.types.dictionaries.len() as u32 {
            if reaches(
                Type::Dictionary(id),
                views,
                self.types,
                &mut BTreeSet::new(),
            ) {
                targets.push(Type::Dictionary(id));
            }
        }
        for class in self.types.classes.instantiable() {
            if reaches(Type::Class(class), views, self.types, &mut BTreeSet::new()) {
                targets.push(Type::Class(class));
            }
        }
        let mut holders = Vec::new();
        // An object with accessors itself, read as the type it is viewed as
        // (so its accessor properties call their getters).
        let mut seen = BTreeSet::new();
        for view in views
            .iter()
            .copied()
            .filter(|view| *view != UNKNOWN_REACHES)
        {
            for physical in accessor_shapes(view, self.types) {
                if physical == view || !seen.insert(physical) {
                    continue;
                }
                let test = self.cell_test(span, 0, Type::Unknown, Type::Record(physical), "")?;
                holders.push((
                    test,
                    Expr {
                        span,
                        ty: Type::Record(view),
                        kind: E::Reinterpret(Box::new(Expr {
                            span,
                            ty: Type::Record(physical),
                            kind: E::UnboxUnion(Box::new(local_value(span, 0, Type::Unknown))),
                        })),
                    },
                ));
            }
        }
        for target in targets {
            let test = match target {
                Type::Class(class) => {
                    let descriptor = self.hidden(Type::Number);
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Binary(
                            ast::BinaryOp::LogicalAnd,
                            Box::new(Expr {
                                span,
                                ty: Type::Boolean,
                                kind: E::IsUnionMember(
                                    Box::new(local_value(span, 0, Type::Unknown)),
                                    target,
                                ),
                            }),
                            Box::new(Expr {
                                span,
                                ty: Type::Boolean,
                                kind: E::IsClass {
                                    value: Box::new(Expr {
                                        span,
                                        ty: target,
                                        kind: E::UnboxUnion(Box::new(local_value(
                                            span,
                                            0,
                                            Type::Unknown,
                                        ))),
                                    }),
                                    shapes: vec![self.types.classes.shape(class)],
                                    slot: descriptor,
                                },
                            }),
                        ),
                    }
                }
                _ => self.cell_test(span, 0, Type::Unknown, target, "")?,
            };
            holders.push((
                test,
                Expr {
                    span,
                    ty: target,
                    kind: E::UnboxUnion(Box::new(local_value(span, 0, Type::Unknown))),
                },
            ));
        }
        let any = Self::unknown_array_type(self.types)?;
        let array = Expr {
            span,
            ty: Type::Boolean,
            kind: E::IsUnionMember(Box::new(local_value(span, 0, Type::Unknown)), any),
        };
        let elements = self.unknown_array(local_value(span, 0, Type::Unknown))?;
        holders.push((array, elements));
        Ok(holders)
    }

    /// The record layouts with other own keys that a value of view
    /// `shape` (local 0) may hold: the test that it holds one, and the object
    /// as that layout (an object with accessors as the type it is viewed as).
    fn wider_layouts(&mut self, shape: u32, span: swc_common::Span) -> Vec<(Expr, Expr)> {
        let names = |types: &ir::ValueTypes, id: u32| -> Vec<String> {
            types.records[id as usize]
                .fields
                .iter()
                .filter(|field| !hidden_field(&field.name))
                .map(|field| field.name.clone())
                .collect()
        };
        let own = names(self.types, shape);
        let mut layouts = Vec::new();
        for layout in self.types.subtyping.view_sources(shape) {
            let read = self.types.receivers.logical(layout).unwrap_or(layout);
            if layout == shape
                || read == shape
                || (names(self.types, read) == own
                    && snapshots::physical(layout, self.types).is_none())
            {
                continue;
            }
            let descriptor = self.hidden(Type::Number);
            layouts.push((
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsClass {
                        value: Box::new(local_value(span, 0, Type::Record(shape))),
                        shapes: vec![layout],
                        slot: descriptor,
                    },
                },
                Expr {
                    span,
                    ty: Type::Record(read),
                    kind: E::Reinterpret(Box::new(local_value(span, 0, Type::Record(shape)))),
                },
            ));
        }
        layouts
    }

    /// For each class derived from `class`: the test that the instance in
    /// local 0 is of exactly that class, and the instance as that class.
    fn subclass_tests(&mut self, class: u32, span: swc_common::Span) -> Result<Vec<(Expr, Expr)>> {
        let mut tests = Vec::new();
        for derived in self.types.classes.descendants(class) {
            let shape = self.types.classes.shape(derived);
            let descriptor = self.hidden(Type::Number);
            tests.push((
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsClass {
                        value: Box::new(local_value(span, 0, Type::Class(class))),
                        shapes: vec![shape],
                        slot: descriptor,
                    },
                },
                Expr {
                    span,
                    ty: Type::Class(derived),
                    kind: E::Reinterpret(Box::new(local_value(span, 0, Type::Class(class)))),
                },
            ));
        }
        Ok(tests)
    }

    /// `{"key":value,…}` of a class instance: its own fields in order, each
    /// read when reached; an absent optional field is no key.
    fn json_class(&mut self, class: u32, span: swc_common::Span) -> Result<Vec<S>> {
        let ty = Type::Class(class);
        let text = text_type(self.types)?;
        let name = self.types.classes.get(class).name.clone();
        // An instance of a subclass serializes its own fields.
        let mut body = Vec::new();
        for (test, derived) in self.subclass_tests(class, span)? {
            let index = serializer(derived.ty, self.types, self.generated, self.next_function)?;
            let (identities, path) = (identities(self.types)?, path_type(self.types)?);
            body.push(S::If(
                test,
                vec![S::Return(Some(Expr {
                    span,
                    ty: text,
                    kind: E::Call(
                        index,
                        vec![
                            derived,
                            local_value(span, JSON_IDENTITIES, identities),
                            local_value(span, JSON_PATH, path),
                            local_value(span, JSON_KEY, Type::String),
                        ],
                    ),
                }))],
                vec![],
            ));
        }
        body.extend(self.json_enter(ty, &name, span)?);
        let (record, fields) = self.class_fields(class, span)?;
        let (source, out, separator, value) = (
            self.hidden(record),
            self.hidden(Type::String),
            self.hidden(Type::String),
            self.hidden(text),
        );
        body.extend([
            S::Init(
                source,
                Expr {
                    span,
                    ty: record,
                    kind: E::Reinterpret(Box::new(local_value(span, 0, ty))),
                },
            ),
            S::Init(out, string(span, "{", self.types)),
            S::Init(separator, string(span, "", self.types)),
            S::Init(value, self.json_host_text(undefined(span), span)?),
        ]);
        for (slot, field) in fields {
            let read = Expr {
                span,
                ty: field.ty,
                kind: E::Field(Box::new(local_value(span, source, record)), slot),
            };
            let described = string(span, &format!("property '{}'", field.name), self.types);
            let serialized = self.json_value(read, described, span)?;
            let quoted = self.json_host_text(string(span, &field.name, self.types), span)?;
            let member = concat(
                span,
                vec![
                    local_value(span, out, Type::String),
                    local_value(span, separator, Type::String),
                    Expr {
                        span,
                        ty: Type::String,
                        kind: E::UnboxUnion(Box::new(quoted)),
                    },
                    string(span, ":", self.types),
                    Expr {
                        span,
                        ty: Type::String,
                        kind: E::UnboxUnion(Box::new(local_value(span, value, text))),
                    },
                ],
            );
            let emit = vec![
                S::Set(value, serialized),
                S::If(
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::IsUndefined(Box::new(local_value(span, value, text))),
                    },
                    vec![],
                    vec![
                        S::Set(out, member),
                        S::Set(separator, string(span, ",", self.types)),
                    ],
                ),
            ];
            if field.optional {
                body.push(S::If(
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::HasOwn(Box::new(local_value(span, source, record)), Some(slot)),
                    },
                    emit,
                    vec![],
                ));
            } else {
                body.extend(emit);
            }
        }
        body.extend(self.json_leave(span)?);
        let closed = concat(
            span,
            vec![
                local_value(span, out, Type::String),
                string(span, "}", self.types),
            ],
        );
        body.push(S::Return(Some(unions::widen(
            closed,
            Some(text),
            self.types,
        )?)));
        Ok(body)
    }

    /// A loop over the own keys of the record or dictionary in local 0, in
    /// own-key order; `step` gives the statements of one key.
    fn own_key_loop(
        &mut self,
        ty: Type,
        span: swc_common::Span,
        step: impl FnOnce(&mut Self, Expr) -> Result<Vec<S>>,
    ) -> Result<Vec<S>> {
        let keys = self.enumerate_value(
            local_value(span, 0, ty),
            super::object_enumeration::Kind::Keys,
            span,
        )?;
        let keys_ty = keys.ty;
        let (key_slot, index) = (self.hidden(keys_ty), self.hidden(Type::Float64));
        let key = Expr {
            span,
            ty: Type::String,
            kind: E::Index(
                Box::new(local_value(span, key_slot, keys_ty)),
                Box::new(local_value(span, index, Type::Float64)),
            ),
        };
        let mut each = step(self, key)?;
        each.push(S::Set(index, increment(span, index)));
        Ok(vec![
            S::Init(key_slot, keys),
            S::Init(index, float(span, 0.0)),
            S::While(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Binary(
                        ast::BinaryOp::Lt,
                        Box::new(local_value(span, index, Type::Float64)),
                        Box::new(Expr {
                            span,
                            ty: Type::Float64,
                            kind: E::Length(Box::new(local_value(span, key_slot, keys_ty))),
                        }),
                    ),
                },
                each,
            ),
        ])
    }

    /// `{"key":value,…}` of a dictionary: each own key in order, its value
    /// read when the key is reached (SerializeJSONObject).
    fn json_dictionary(&mut self, id: u32, span: swc_common::Span) -> Result<Vec<S>> {
        let ty = Type::Dictionary(id);
        let text = text_type(self.types)?;
        let mut body = self.json_enter(ty, "Object", span)?;
        let (out, separator, value) = (
            self.hidden(Type::String),
            self.hidden(Type::String),
            self.hidden(text),
        );
        body.extend([
            S::Init(out, string(span, "{", self.types)),
            S::Init(separator, string(span, "", self.types)),
            S::Init(value, self.json_host_text(undefined(span), span)?),
        ]);
        let walk = self.own_key_loop(ty, span, |this, key| {
            let described = concat(
                span,
                vec![
                    string(span, "property '", this.types),
                    key.clone(),
                    string(span, "'", this.types),
                ],
            );
            let read = this.dictionary_get(local_value(span, 0, ty), key.clone())?;
            let serialized = this.json_value(read, described, span)?;
            let quoted = this.json_host_text(key, span)?;
            let member = concat(
                span,
                vec![
                    local_value(span, out, Type::String),
                    local_value(span, separator, Type::String),
                    Expr {
                        span,
                        ty: Type::String,
                        kind: E::UnboxUnion(Box::new(quoted)),
                    },
                    string(span, ":", this.types),
                    Expr {
                        span,
                        ty: Type::String,
                        kind: E::UnboxUnion(Box::new(local_value(span, value, text))),
                    },
                ],
            );
            Ok(vec![
                S::Set(value, serialized),
                S::If(
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::IsUndefined(Box::new(local_value(span, value, text))),
                    },
                    vec![],
                    vec![
                        S::Set(out, member),
                        S::Set(separator, string(span, ",", this.types)),
                    ],
                ),
            ])
        })?;
        body.extend(walk);
        body.extend(self.json_leave(span)?);
        let closed = concat(
            span,
            vec![
                local_value(span, out, Type::String),
                string(span, "}", self.types),
            ],
        );
        body.push(S::Return(Some(unions::widen(
            closed,
            Some(text),
            self.types,
        )?)));
        Ok(body)
    }

    /// `[value,…]` with `null` for an element without JSON text.
    fn json_array(&mut self, id: u32, span: swc_common::Span) -> Result<Vec<S>> {
        let array = Type::Array(id);
        let element = self.types.arrays[id as usize];
        let text = text_type(self.types)?;
        let mut body = self.json_enter(array, "Array", span)?;
        let (index, out, separator, value) = (
            self.hidden(Type::Float64),
            self.hidden(Type::String),
            self.hidden(Type::String),
            self.hidden(text),
        );
        body.extend([
            S::Init(index, float(span, 0.0)),
            S::Init(out, string(span, "[", self.types)),
            S::Init(separator, string(span, "", self.types)),
        ]);
        let read = Expr {
            span,
            ty: element,
            kind: E::Index(
                Box::new(local_value(span, 0, array)),
                Box::new(local_value(span, index, Type::Float64)),
            ),
        };
        // `index <n>` names the element in a circular-structure message.
        let position =
            self.primitive_convert(local_value(span, index, Type::Float64), &[], false, span)?;
        let described = concat(span, vec![string(span, "index ", self.types), position]);
        let serialized = self.json_value(read, described, span)?;
        body.push(S::Init(value, self.json_host_text(undefined(span), span)?));
        let item = Expr {
            span,
            ty: Type::String,
            kind: E::Conditional(
                Box::new(Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsUndefined(Box::new(local_value(span, value, text))),
                }),
                Box::new(string(span, "null", self.types)),
                Box::new(Expr {
                    span,
                    ty: Type::String,
                    kind: E::UnboxUnion(Box::new(local_value(span, value, text))),
                }),
            ),
        };
        let appended = concat(
            span,
            vec![
                local_value(span, out, Type::String),
                local_value(span, separator, Type::String),
                item,
            ],
        );
        body.push(S::While(
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Binary(
                    ast::BinaryOp::Lt,
                    Box::new(local_value(span, index, Type::Float64)),
                    Box::new(Expr {
                        span,
                        ty: Type::Float64,
                        kind: E::Length(Box::new(local_value(span, 0, array))),
                    }),
                ),
            },
            vec![
                S::Set(value, serialized),
                S::Set(out, appended),
                S::Set(separator, string(span, ",", self.types)),
                S::Set(index, increment(span, index)),
            ],
        ));
        body.extend(self.json_leave(span)?);
        let closed = concat(
            span,
            vec![
                local_value(span, out, Type::String),
                string(span, "]", self.types),
            ],
        );
        body.push(S::Return(Some(unions::widen(
            closed,
            Some(text),
            self.types,
        )?)));
        Ok(body)
    }
}

/// The locals of a serializer after its value (local 0): the identities
/// of the objects being serialized, their keys and constructors (two
/// entries per object) and the key the value was reached by.
const JSON_IDENTITIES: u32 = 1;
const JSON_PATH: u32 = 2;
const JSON_KEY: u32 = 3;

/// `number[]`: the identities a walk has entered.
fn identities(types: &mut ir::ValueTypes) -> Result<Type> {
    arrays::intern(types.number(), types)
}

/// `string[]`: the keys and constructors of a JSON walk's stack.
fn path_type(types: &mut ir::ValueTypes) -> Result<Type> {
    arrays::intern(Type::String, types)
}

/// A binary numeric operation or comparison.
fn binary(span: swc_common::Span, op: ast::BinaryOp, left: Expr, right: Expr) -> Expr {
    use ast::BinaryOp::{Gt, GtEq, Lt, LtEq};
    let ty = if matches!(op, Gt | GtEq | Lt | LtEq) {
        Type::Boolean
    } else {
        Type::Float64
    };
    Expr {
        span,
        ty,
        kind: E::Binary(op, Box::new(left), Box::new(right)),
    }
}

/// `array.push(item)` as a statement.
fn push(span: swc_common::Span, array: Expr, item: Expr) -> S {
    let ty = array.ty;
    S::Expr(Expr {
        span,
        ty: Type::Float64,
        kind: E::ArrayPush(
            Box::new(array),
            Box::new(Expr {
                span,
                ty,
                kind: E::Array(vec![Some(item)]),
            }),
        ),
    })
}

/// `string | undefined`, the type of a serialized value.
fn text_type(types: &mut ir::ValueTypes) -> Result<Type> {
    unions::intern(vec![Type::String, Type::Undefined], types)
}
pub(super) fn local_value(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn float(span: swc_common::Span, value: f64) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Float64(value),
    }
}
fn undefined(span: swc_common::Span) -> Expr {
    Expr {
        span,
        ty: Type::Undefined,
        kind: E::Integer(0),
    }
}
fn string(span: swc_common::Span, text: &str, types: &ir::ValueTypes) -> Expr {
    Expr {
        span,
        ty: Type::String,
        kind: E::String(types.profile.string_bytes(text)),
    }
}
fn increment(span: swc_common::Span, slot: u32) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Binary(
            ast::BinaryOp::Add,
            Box::new(local_value(span, slot, Type::Float64)),
            Box::new(float(span, 1.0)),
        ),
    }
}
fn concat(span: swc_common::Span, parts: Vec<Expr>) -> Expr {
    let mut parts = parts.into_iter();
    let first = parts.next().unwrap_or_else(|| Expr {
        span,
        ty: Type::String,
        kind: E::String(Vec::new()),
    });
    parts.fold(first, |left, right| Expr {
        span,
        ty: Type::String,
        kind: E::Binary(ast::BinaryOp::Add, Box::new(left), Box::new(right)),
    })
}

/// The accessor-object shapes a value of record `shape` may be: those
/// viewed as it and the shape itself.
fn accessor_shapes(shape: u32, types: &ir::ValueTypes) -> Vec<u32> {
    let accessor = |id: u32| {
        types.records[id as usize]
            .fields
            .iter()
            .any(|field| accessor_field(&field.name).is_some())
    };
    let mut shapes: Vec<u32> = types
        .subtyping
        .view_sources(shape)
        .into_iter()
        .filter(|id| accessor(*id))
        .collect();
    if accessor(shape) {
        shapes.push(shape);
    }
    shapes
}

/// The structured refresh of `ty`, reserved with a placeholder body.
fn refresher(
    ty: Type,
    types: &mut ir::ValueTypes,
    generated: &mut BTreeMap<usize, ir::Function>,
    next: &mut usize,
) -> Result<usize> {
    if let Some((index, _)) = types.receivers.refreshers.get(&ty) {
        return Ok(*index);
    }
    let Some((source, span)) = types.receivers.origin.clone() else {
        return Err(CompilerError::Internal("refresh without origin".into()));
    };
    let index = *next;
    if index >= types.function_limit() {
        return Err(invalid("too many functions including structured refreshes"));
    }
    *next += 1;
    let seen = identities(types)?;
    generated.insert(
        index,
        ir::Function {
            shape: ir::ParameterShape::fixed(2),
            self_slot: None,
            receiver: None,
            cells: Default::default(),
            captures: vec![],
            environment: false,
            generator: None,
            source,
            span,
            params: vec![ty, seen],
            result: Type::Void,
            locals: vec![ty, seen],
            body: vec![S::Trap],
        },
    );
    types.receivers.refreshers.insert(ty, (index, false));
    Ok(index)
}

/// The serializer of `ty`, reserved with a placeholder body that `finish`
/// writes.
fn serializer(
    ty: Type,
    types: &mut ir::ValueTypes,
    generated: &mut BTreeMap<usize, ir::Function>,
    next: &mut usize,
) -> Result<usize> {
    if let Some((index, _)) = types.receivers.materializers.get(&ty) {
        return Ok(*index);
    }
    let Some((source, span)) = types.receivers.origin.clone() else {
        return Err(CompilerError::Internal("serializer without origin".into()));
    };
    let index = *next;
    if index >= types.function_limit() {
        return Err(invalid("too many functions including JSON serializers"));
    }
    *next += 1;
    let text = text_type(types)?;
    let params = json_params(ty, types)?;
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
            result: text,
            locals: params,
            body: vec![S::Trap],
        },
    );
    types.receivers.materializers.insert(ty, (index, false));
    Ok(index)
}

/// The parameters of the serializer of `ty`.
fn json_params(ty: Type, types: &mut ir::ValueTypes) -> Result<Vec<Type>> {
    Ok(vec![
        ty,
        identities(types)?,
        path_type(types)?,
        Type::String,
    ])
}

/// The host operations (by name prefix of every versioned form) that read
/// the properties of the values they receive.
const DEEP_READERS: [&str; 4] = ["ValueLog", "ValueFormat", "JsonStringify", "UnionLog"];

/// Hosts read records as data: an `unknown` cell or a host operation other
/// than the `JSON.stringify` serializers that may receive an object with
/// accessors is refused.
fn refuse_host_reads<'f>(
    views: &BTreeSet<u32>,
    types: &ir::ValueTypes,
    functions: impl Iterator<Item = &'f ir::Function>,
) -> Result<()> {
    for function in functions {
        let mut found = None;
        let mut body = function.body.clone();
        prune::statements(&mut body, &mut |expr| {
            // Hosts that read a value's properties (logging, formatting,
            // serializing); type tests and primitive conversions read none.
            if let E::Host(host, args) = &expr.kind
                && DEEP_READERS
                    .iter()
                    .any(|reader| format!("{host:?}").starts_with(reader))
            {
                for arg in args {
                    if reaches(arg.ty, views, types, &mut BTreeSet::new()) {
                        found.get_or_insert((format!("{host:?}"), arg.ty, arg.span));
                    }
                }
            }
            if let E::Json(value, _) = &expr.kind
                && reaches(value.ty, views, types, &mut BTreeSet::new())
            {
                found.get_or_insert(("JSON.stringify".to_owned(), value.ty, value.span));
            }
        });
        if let Some((host, ty, span)) = found {
            return Err(function.source.locate(
                unsupported(&format!(
                    "{host} of a {} that may hold an object with getters; only JSON.stringify reads accessor properties",
                    subtyping::describe(ty, types)
                )),
                span,
            ));
        }
    }
    Ok(())
}

/// The marker in the set of accessor views that an object with accessors
/// entered `unknown` (no record shape has this index).
const UNKNOWN_REACHES: u32 = u32::MAX;

/// Whether a value of `ty` may hold an object with accessors.
fn reaches(
    ty: Type,
    views: &BTreeSet<u32>,
    types: &ir::ValueTypes,
    seen: &mut BTreeSet<Type>,
) -> bool {
    if !seen.insert(ty) {
        return false;
    }
    match ty {
        Type::Record(id) => {
            views.contains(&id)
                || types.records[id as usize]
                    .fields
                    .iter()
                    .any(|field| reaches(field.ty, views, types, seen))
        }
        Type::Array(id) => reaches(types.arrays[id as usize], views, types, seen),
        Type::Union(id) | Type::ReferenceUnion(id) => types.unions[id as usize]
            .iter()
            .any(|member| reaches(*member, views, types, seen)),
        Type::Dictionary(id) => reaches(types.dictionaries[id as usize].value, views, types, seen),
        // A class instance's fields are those of its instance layout.
        Type::Class(class) => reaches(
            Type::Record(types.classes.get(class).shape),
            views,
            types,
            seen,
        ),
        // An `unknown` cell may hold one when one entered `unknown`.
        Type::Unknown => views.contains(&UNKNOWN_REACHES),
        _ => false,
    }
}

/// After every body is lowered: write each serializer's body (reserving the
/// serializers of the nested types it needs), then refuse every other host
/// read of an object with accessors.
pub(super) fn finish(
    types: &mut ir::ValueTypes,
    functions: &[ir::Function],
    generated: &mut BTreeMap<usize, ir::Function>,
    next: &mut usize,
) -> Result<()> {
    if !types.receivers.accessors {
        return Ok(());
    }
    let mut views = types.subtyping.accessor_views(types);
    // An object with accessors held by `unknown`: its physical shapes are
    // boxed too, so the JSON-object reads of `unknown` call its getters.
    let boxed: Vec<u32> = views
        .iter()
        .copied()
        .filter(|view| types.subtyping.is_boxed(*view))
        .collect();
    for view in &boxed {
        for physical in accessor_shapes(*view, types) {
            types.subtyping.note_boxed_shape(physical);
        }
    }
    if !boxed.is_empty() {
        views.insert(UNKNOWN_REACHES);
    }
    // Synthesized walks read each layout themselves (no key-copy notes).
    types.layouts.exact = true;
    if let Some((source, span)) = types.receivers.origin.clone() {
        let functions_map = BTreeMap::new();
        let mut effects = BTreeMap::new();
        loop {
            let refreshes: Vec<(Type, usize)> = types
                .receivers
                .refreshers
                .iter()
                .filter(|(_, (_, written))| !written)
                .map(|(ty, (index, _))| (*ty, *index))
                .collect();
            for (ty, index) in &refreshes {
                let function = {
                    let mut lower = Lower::root(
                        &source,
                        types,
                        &functions_map,
                        (generated, next, &mut effects),
                        *index,
                    );
                    lower.hidden(*ty);
                    let seen = identities(lower.types)?;
                    lower.hidden(seen);
                    let body = lower
                        .refresh_walk(*ty, &views, span)
                        .map_err(|error| source.locate(error, span))?;
                    lower.function(body, vec![*ty, seen], Type::Void, span)
                };
                generated.insert(*index, function);
                types.receivers.refreshers.insert(*ty, (*index, true));
            }
            let pending: Vec<(Type, usize)> = types
                .receivers
                .materializers
                .iter()
                .filter(|(_, (_, written))| !written)
                .map(|(ty, (index, _))| (*ty, *index))
                .collect();
            if pending.is_empty() && refreshes.is_empty() {
                break;
            }
            for (ty, index) in pending {
                let function = {
                    let mut lower = Lower::root(
                        &source,
                        types,
                        &functions_map,
                        (generated, next, &mut effects),
                        index,
                    );
                    let params = json_params(ty, lower.types)?;
                    for param in &params {
                        lower.hidden(*param);
                    }
                    let body = lower
                        .json_walk(ty, &views, span)
                        .map_err(|error| source.locate(error, span))?;
                    let text = text_type(lower.types)?;
                    lower.function(body, params, text, span)
                };
                generated.insert(index, function);
                types.receivers.materializers.insert(ty, (index, true));
            }
        }
    }
    let serializers: BTreeSet<usize> = types
        .receivers
        .materializers
        .values()
        .chain(types.receivers.refreshers.values())
        .map(|(index, _)| *index)
        .collect();
    let others = generated
        .iter()
        .filter(|(index, _)| !serializers.contains(index))
        .map(|(_, function)| function);
    types.layouts.exact = false;
    refuse_host_reads(&views, types, functions.iter().chain(others))
}

impl<'a> Lower<'a> {
    /// A lowering context for a synthesized function `index` outside every
    /// source function.
    pub(super) fn root(
        source: &'a Source,
        types: &'a mut ir::ValueTypes,
        functions: &'a BTreeMap<String, Signature>,
        (generated, next_function, call_effects): (
            &'a mut BTreeMap<usize, ir::Function>,
            &'a mut usize,
            &'a mut BTreeMap<usize, flow_calls::Impact>,
        ),
        index: usize,
    ) -> Self {
        Lower {
            source,
            types,
            functions,
            scopes: vec![BTreeMap::new()],
            next: 0,
            local_types: BTreeMap::new(),
            refinements: BTreeMap::new(),
            writes: Default::default(),
            call_writes: Default::default(),
            call_unknown: false,
            direct_writes: Default::default(),
            call_parameters: Default::default(),
            parameter_values: Default::default(),
            parameter_roots: Default::default(),
            known_calls: Default::default(),
            call_effects,
            reachable: true,
            control: Default::default(),
            guard_blocked: Default::default(),
            outer_narrows: Default::default(),
            result: Type::Void,
            infer_returns: false,
            entry: usize::MAX,
            loops: 0,
            labels: Vec::new(),
            next_label: 0,
            switches: 0,
            depth: 0,
            repeat_slots: None,
            generated,
            next_function,
            index,
            outer: Default::default(),
            captures: Default::default(),
            cells: Default::default(),
            guarded: Default::default(),
            generator: None,
            asynchronous: None,
            core: Default::default(),
        }
    }
    /// The synthesized function of this context's locals and `body`.
    pub(super) fn function(
        self,
        body: Vec<S>,
        params: Vec<Type>,
        result: Type,
        span: swc_common::Span,
    ) -> ir::Function {
        ir::Function {
            shape: ir::ParameterShape::fixed(params.len()),
            self_slot: None,
            receiver: None,
            cells: self.cells,
            captures: vec![],
            environment: false,
            generator: None,
            source: self.source.clone(),
            span,
            params,
            result,
            locals: (0..self.next)
                .map(|slot| self.local_types.get(&slot).copied().unwrap_or(Type::Number))
                .collect(),
            body,
        }
    }
}

/// Whether `name` is an accessor property of record `shape`.
pub(crate) fn is_accessor(shape: u32, name: &str, types: &ir::ValueTypes) -> bool {
    types.records[shape as usize]
        .fields
        .iter()
        .any(|field| accessor_field(&field.name).is_some_and(|(property, _)| property == name))
}

impl Lower<'_> {
    /// Read every field of the record in local `source` (record `shape`)
    /// into a new local, in the object's own-key order: the keys are
    /// enumerated first, then each value is read (a getter runs) when its
    /// key is reached. Returns the statements and the locals, by slot.
    pub(super) fn read_in_key_order(
        &mut self,
        source: u32,
        shape: u32,
        skip: &[String],
        span: swc_common::Span,
    ) -> Result<(Vec<S>, Vec<u32>)> {
        let record = Type::Record(shape);
        let fields = self.types.records[shape as usize].fields.clone();
        let mut body = Vec::new();
        let mut locals = Vec::with_capacity(fields.len());
        for field in &fields {
            let slot = self.hidden(field.ty);
            body.push(S::Init(
                slot,
                Expr {
                    span,
                    ty: field.ty,
                    kind: E::Zero,
                },
            ));
            locals.push(slot);
        }
        let keys = self.enumerate_value(
            local_value(span, source, record),
            super::object_enumeration::Kind::Keys,
            span,
        )?;
        let keys_ty = keys.ty;
        let (key_slot, index) = (self.hidden(keys_ty), self.hidden(Type::Float64));
        body.extend([S::Init(key_slot, keys), S::Init(index, float(span, 0.0))]);
        let key = Expr {
            span,
            ty: Type::String,
            kind: E::Index(
                Box::new(local_value(span, key_slot, keys_ty)),
                Box::new(local_value(span, index, Type::Float64)),
            ),
        };
        let mut dispatch = Vec::new();
        for (slot, field) in fields.iter().enumerate().rev() {
            // An excluded key (a rest pattern's named property) is not read.
            if skip.contains(&field.name) {
                continue;
            }
            let read = Expr {
                span,
                ty: field.ty,
                kind: E::Field(Box::new(local_value(span, source, record)), slot as u32),
            };
            let test = Expr {
                span,
                ty: Type::Boolean,
                kind: E::Binary(
                    ast::BinaryOp::EqEqEq,
                    Box::new(key.clone()),
                    Box::new(string(span, &field.name, self.types)),
                ),
            };
            dispatch = vec![S::If(test, vec![S::Set(locals[slot], read)], dispatch)];
        }
        dispatch.push(S::Set(index, increment(span, index)));
        body.push(S::While(
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Binary(
                    ast::BinaryOp::Lt,
                    Box::new(local_value(span, index, Type::Float64)),
                    Box::new(Expr {
                        span,
                        ty: Type::Float64,
                        kind: E::Length(Box::new(local_value(span, key_slot, keys_ty))),
                    }),
                ),
            },
            dispatch,
        ));
        Ok((body, locals))
    }
}
