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
        if let Some(spread) = object.props.iter().find_map(|prop| match prop {
            ast::PropOrSpread::Spread(spread) => Some(spread.dot3_token),
            ast::PropOrSpread::Prop(_) => None,
        }) {
            return Err(self.source.locate(
                unsupported("spread in an object literal with getters or setters"),
                spread,
            ));
        }
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
        for prop in &object.props {
            let ast::PropOrSpread::Prop(prop) = prop else {
                continue;
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
            if values.iter().any(|(existing, _)| *existing == name) {
                return Err(self.source.locate(
                    unsupported(&format!("object literal member {name} defined twice")),
                    span,
                ));
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
            let provided =
                values.iter().any(|(name, _)| *name == field.name) || getters.contains(&field.name);
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
                    optional: false,
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
        Ok(Expr {
            span,
            ty,
            kind: E::Reinterpret(Box::new(Expr {
                span,
                ty: physical,
                kind: E::Record(fields),
            })),
        })
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
            let ast::PropOrSpread::Prop(prop) = prop else {
                continue;
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
    /// The value `JSON.stringify` serializes (v38): a record or array that
    /// may hold an object with accessors goes through its materializer,
    /// which copies it into plain objects whose accessor properties hold
    /// their getters' values; the host then serializes data fields only.
    pub(super) fn json_materialized(&mut self, value: Expr) -> Result<Expr> {
        if !self.types.receivers.accessors || !matches!(value.ty, Type::Record(_) | Type::Array(_))
        {
            return Ok(value);
        }
        let span = value.span;
        self.types
            .receivers
            .origin
            .get_or_insert_with(|| (self.source.clone(), span));
        let index = materializer(value.ty, self.types, self.generated, self.next_function)?;
        Ok(Expr {
            span,
            ty: value.ty,
            kind: E::Call(index, vec![value]),
        })
    }
}

/// The materializer of `ty`, reserved with a placeholder body.
fn materializer(
    ty: Type,
    types: &mut ir::ValueTypes,
    generated: &mut BTreeMap<usize, ir::Function>,
    next: &mut usize,
) -> Result<usize> {
    if let Some((index, _)) = types.receivers.materializers.get(&ty) {
        return Ok(*index);
    }
    let Some((source, span)) = types.receivers.origin.clone() else {
        return Err(CompilerError::Internal(
            "materializer without origin".into(),
        ));
    };
    let index = *next;
    if index >= types.function_limit() {
        return Err(invalid("too many functions including JSON materializers"));
    }
    *next += 1;
    generated.insert(
        index,
        ir::Function {
            shape: ir::ParameterShape::fixed(1),
            self_slot: None,
            receiver: None,
            cells: Default::default(),
            captures: vec![],
            environment: false,
            generator: None,
            source,
            span,
            params: vec![ty],
            result: ty,
            locals: vec![ty],
            body: vec![S::Trap],
        },
    );
    types.receivers.materializers.insert(ty, (index, false));
    Ok(index)
}

/// Hosts read records as data and skip the accessor functions, so an
/// object with accessors may reach the host only through a `JSON.stringify`
/// materializer: a host operation or an `unknown` cell that may receive one
/// is refused (it would silently lose the accessor properties).
fn refuse_host_reads(
    views: &BTreeSet<u32>,
    types: &ir::ValueTypes,
    functions: &[ir::Function],
    generated: &BTreeMap<usize, ir::Function>,
) -> Result<()> {
    if let Some(shape) = views.iter().find(|shape| types.subtyping.is_boxed(**shape)) {
        return Err(unsupported(&format!(
            "a {} held by unknown may be an object with getters, whose accessor properties are not data",
            subtyping::describe(Type::Record(*shape), types)
        )));
    }
    let materializers: BTreeSet<usize> = types
        .receivers
        .materializers
        .values()
        .map(|(index, _)| *index)
        .collect();
    for function in functions.iter().chain(generated.values()) {
        let mut found = None;
        let mut body = function.body.clone();
        prune::statements(&mut body, &mut |expr| {
            if let E::Host(host, args) = &expr.kind {
                for arg in args {
                    let materialized =
                        matches!(&arg.kind, E::Call(index, _) if materializers.contains(index));
                    if !materialized && reaches(arg.ty, views, types, &mut BTreeSet::new()) {
                        found.get_or_insert((*host, arg.ty, arg.span));
                    }
                }
            }
        });
        if let Some((host, ty, span)) = found {
            return Err(function.source.locate(
                unsupported(&format!(
                    "{host:?} of a {} that may hold an object with getters; only JSON.stringify reads accessor properties",
                    subtyping::describe(ty, types)
                )),
                span,
            ));
        }
    }
    Ok(())
}

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
        _ => false,
    }
}

/// After every body is lowered: write each materializer's body, reserving
/// the materializers of nested records and arrays it needs.
pub(super) fn finish(
    types: &mut ir::ValueTypes,
    functions: &[ir::Function],
    generated: &mut BTreeMap<usize, ir::Function>,
    next: &mut usize,
) -> Result<()> {
    if !types.receivers.accessors {
        return Ok(());
    }
    let views = types.subtyping.accessor_views(types);
    refuse_host_reads(&views, types, functions, generated)?;
    if types.receivers.materializers.is_empty() {
        return Ok(());
    }
    let Some((source, span)) = types.receivers.origin.clone() else {
        return Ok(());
    };
    loop {
        let pending: Vec<(Type, usize)> = types
            .receivers
            .materializers
            .iter()
            .filter(|(_, (_, written))| !written)
            .map(|(ty, (index, _))| (*ty, *index))
            .collect();
        if pending.is_empty() {
            return Ok(());
        }
        for (ty, index) in pending {
            let (body, locals) = materializer_body(ty, &views, types, generated, next, span)
                .map_err(|error| source.locate(error, span))?;
            let function = generated
                .get_mut(&index)
                .ok_or_else(|| CompilerError::Internal("missing materializer".into()))?;
            function.body = body;
            function.locals = locals;
            types.receivers.materializers.insert(ty, (index, true));
        }
    }
}

fn materializer_body(
    ty: Type,
    views: &BTreeSet<u32>,
    types: &mut ir::ValueTypes,
    generated: &mut BTreeMap<usize, ir::Function>,
    next: &mut usize,
    span: swc_common::Span,
) -> Result<(Vec<S>, Vec<Type>)> {
    let local = |slot: u32, ty: Type| Expr {
        span,
        ty,
        kind: E::Local(slot),
    };
    let mut locals = vec![ty];
    if !reaches(ty, views, types, &mut BTreeSet::new()) {
        return Ok((vec![S::Return(Some(local(0, ty)))], locals));
    }
    // A nested value: its own materializer, or itself.
    let mut nested = |value: Expr, types: &mut ir::ValueTypes| -> Result<Expr> {
        if !reaches(value.ty, views, types, &mut BTreeSet::new()) {
            return Ok(value);
        }
        if !matches!(value.ty, Type::Record(_) | Type::Array(_)) {
            return Err(unsupported(&format!(
                "JSON.stringify of a {} that may hold an object with getters (only records and arrays are copied through their getters)",
                subtyping::describe(value.ty, types)
            )));
        }
        let index = materializer(value.ty, types, generated, next)?;
        Ok(Expr {
            span,
            ty: value.ty,
            kind: E::Call(index, vec![value]),
        })
    };
    match ty {
        Type::Record(id) => {
            let fields = types.records[id as usize].fields.clone();
            if let Some(field) = fields.iter().find(|field| field.optional) {
                return Err(unsupported(&format!(
                    "JSON.stringify of an object with getters whose type has the optional member {}",
                    field.name
                )));
            }
            let mut values = Vec::with_capacity(fields.len());
            for (slot, field) in fields.iter().enumerate() {
                let read = Expr {
                    span,
                    ty: field.ty,
                    kind: E::Field(Box::new(local(0, ty)), slot as u32),
                };
                values.push((slot as u32, nested(read, types)?));
            }
            Ok((
                vec![S::Return(Some(Expr {
                    span,
                    ty,
                    kind: E::Record(values),
                }))],
                locals,
            ))
        }
        Type::Array(id) => {
            let element = types.arrays[id as usize];
            let number = types.number();
            let (copy, index, count) = (1, 2, 3);
            locals.extend([ty, number, number]);
            let read = Expr {
                span,
                ty: element,
                kind: E::Index(Box::new(local(0, ty)), Box::new(local(index, number))),
            };
            let value = nested(read, types)?;
            let constant = |value: f64| Expr {
                span,
                ty: number,
                kind: E::Float64(value),
            };
            let binary = |op: ast::BinaryOp, left: Expr, right: Expr, ty: Type| Expr {
                span,
                ty,
                kind: E::Binary(op, Box::new(left), Box::new(right)),
            };
            Ok((
                vec![
                    S::Init(
                        copy,
                        Expr {
                            span,
                            ty,
                            kind: E::Array(Vec::new()),
                        },
                    ),
                    S::Init(
                        count,
                        Expr {
                            span,
                            ty: number,
                            kind: E::Length(Box::new(local(0, ty))),
                        },
                    ),
                    S::Init(index, constant(0.0)),
                    S::While(
                        binary(
                            ast::BinaryOp::Lt,
                            local(index, number),
                            local(count, number),
                            Type::Boolean,
                        ),
                        vec![
                            S::Expr(Expr {
                                span,
                                ty: number,
                                kind: E::ArrayPush(
                                    Box::new(local(copy, ty)),
                                    Box::new(Expr {
                                        span,
                                        ty,
                                        kind: E::Array(vec![Some(value)]),
                                    }),
                                ),
                            }),
                            S::Set(
                                index,
                                binary(
                                    ast::BinaryOp::Add,
                                    local(index, number),
                                    constant(1.0),
                                    number,
                                ),
                            ),
                        ],
                    ),
                    S::Return(Some(local(copy, ty))),
                ],
                locals,
            ))
        }
        other => Err(unsupported(&format!(
            "JSON.stringify of a {} that may hold an object with getters",
            subtyping::describe(other, types)
        ))),
    }
}
