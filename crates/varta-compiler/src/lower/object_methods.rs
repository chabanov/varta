//! V38 object-literal methods that use `this` (`expression_forms`).
//!
//! A method `m() { ... this ... }` of an object literal sees the receiver it
//! is called on, as in JavaScript: `o.m()` passes `o`, a copy made by an
//! object spread passes the copy, and an extracted method called alone
//! passes `undefined` (reading a property of that `this` throws
//! `TypeError`). Function values carry no receiver parameter, so the call
//! passes it through a register: every function-value call of a program
//! with such methods stores its receiver (the object of `o.m(...)`, held in
//! a local, or none) in a permanent root slot right before the dispatch, and
//! the method, a closure over a hidden `this` binding, takes and clears it
//! first thing (`receiver_prologue`). The receiver must be an object of the
//! literal's type, or of a type viewed as it (a spread copy with more
//! fields is noted as such a view); another object throws `TypeError`.
//!
//! The object's type is its contextual record type, or, without one, the
//! record of its literal or binding field values and annotated methods.
//! Getters and setters (accessor properties) of object literals stay
//! refused: a closed record has data fields only.
use super::*;
use swc_ecma_visit::{Visit, VisitMut, VisitMutWith, VisitWith};

/// The hidden binding of a method's receiver; `#` cannot start a source
/// identifier.
const SELF: &str = "#self";
/// The module-constant index of the receiver register while bodies are
/// lowered; `finish` places it behind the program's module constants.
const REGISTER: u32 = 1 << 29;
/// A marker shape in the receiver check (`IsClass`): the shapes viewed as
/// the first shape, filled in by `receiver_shapes` after `subtyping`.
const VIEW_SOURCES: u32 = u32::MAX;
/// The `TypeError` of a receiver that is not an object of the method's type.
const FOREIGN_RECEIVER: &str =
    "object literal method called on an object that is not of its literal type";

/// The receiver register of a program (v38 `expression_forms`).
#[derive(Default)]
pub(crate) struct Receivers {
    /// The program has an object-literal method that uses `this`.
    enabled: bool,
    /// The register's module-constant index once placed.
    register: Option<u32>,
    /// A method read the register while bodies were lowered.
    used: bool,
}
impl Receivers {
    /// The register's module-constant index, when the program has one.
    pub(crate) fn register(&self) -> Option<u32> {
        self.register
    }
    /// Record the register's index after module constants are renumbered.
    pub(crate) fn place(&mut self, index: Option<u32>) {
        self.register = index;
    }
}

/// A method that uses `this` without declaring a `this` parameter (one
/// that declares it keeps its receiver: `o.m()` passes the object).
fn bound(function: &ast::Function) -> bool {
    function.this_param.is_none() && records::uses_receiver(function)
}

/// Note whether any module has an object-literal method that uses `this`:
/// only such programs pass receivers through the register.
pub(super) fn scan(graph: &crate::modules::Graph, types: &mut ir::ValueTypes) {
    struct Methods(bool);
    impl Visit for Methods {
        fn visit_method_prop(&mut self, method: &ast::MethodProp) {
            self.0 |= bound(&method.function);
            method.visit_children_with(self);
        }
    }
    if !types.profile.expression_forms() {
        return;
    }
    let mut methods = Methods(false);
    for unit in graph.units.values() {
        unit.ast.visit_with(&mut methods);
    }
    types.receivers.enabled = methods.0;
}

/// After every body is lowered: place the register behind the `stored`
/// module constants.
pub(super) fn finish(
    types: &mut ir::ValueTypes,
    functions: &mut [ir::Function],
    generated: &mut BTreeMap<usize, ir::Function>,
    stored: &mut u32,
) {
    if !types.receivers.used {
        return;
    }
    let index = *stored;
    *stored += 1;
    for function in functions.iter_mut().chain(generated.values_mut()) {
        prune::each_constant(&mut function.body, &mut |constant| {
            if *constant == REGISTER {
                *constant = index;
            }
        });
    }
    types.receivers.register = Some(index);
}

/// After `subtyping::resolve`: every receiver check admits the method's own
/// object type and the types viewed as it.
pub(super) fn receiver_shapes(program: &mut ir::Program) {
    if !program.types.receivers.used {
        return;
    }
    let types = &program.types;
    let viewed = |shape: u32| -> Vec<u32> {
        types
            .subtyping
            .sources_of(shape)
            .iter()
            .filter_map(|source| match *source {
                Type::Record(id) => Some(id),
                Type::Class(id) => Some(types.classes.shape(id)),
                _ => None,
            })
            .collect()
    };
    for function in &mut program.functions {
        prune::statements(&mut function.body, &mut |expr| {
            if let E::IsClass { shapes, .. } = &mut expr.kind
                && shapes.last() == Some(&VIEW_SOURCES)
            {
                shapes.pop();
                if let Some(&shape) = shapes.first() {
                    shapes.extend(viewed(shape));
                }
            }
        });
    }
}

/// Replaces `this` (outside nested functions and classes) by `#self!`.
struct Receiver {
    sup: Option<swc_common::Span>,
}
impl VisitMut for Receiver {
    fn visit_mut_expr(&mut self, expr: &mut ast::Expr) {
        if let ast::Expr::This(this) = expr {
            let span = this.span;
            *expr = ast::Expr::TsNonNull(ast::TsNonNullExpr {
                span,
                expr: Box::new(ast::Expr::Ident(ast::Ident::new(
                    SELF.into(),
                    span,
                    Default::default(),
                ))),
            });
            return;
        }
        expr.visit_mut_children_with(self);
    }
    fn visit_mut_super(&mut self, node: &mut ast::Super) {
        self.sup.get_or_insert(node.span);
    }
    fn visit_mut_function(&mut self, _: &mut ast::Function) {}
    fn visit_mut_class(&mut self, _: &mut ast::Class) {}
}

impl Lower<'_> {
    /// The object of a literal with a method that uses `this`, or `None`
    /// when no method does.
    pub(super) fn receiver_literal(
        &mut self,
        object: &ast::ObjectLit,
        expected: Option<Type>,
    ) -> Result<Option<Expr>> {
        if let Some(span) = object.props.iter().find_map(|prop| match prop {
            ast::PropOrSpread::Prop(prop) => match &**prop {
                ast::Prop::Getter(getter) => Some(getter.span),
                ast::Prop::Setter(setter) => Some(setter.span),
                _ => None,
            },
            ast::PropOrSpread::Spread(_) => None,
        }) {
            return Err(self.source.locate(
                unsupported(
                    "getter or setter in an object literal (a closed record has data fields only; declare a class)",
                ),
                span,
            ));
        }
        let receiver = |prop: &ast::PropOrSpread| {
            matches!(prop, ast::PropOrSpread::Prop(prop)
                if matches!(&**prop, ast::Prop::Method(method) if bound(&method.function)))
        };
        if !object.props.iter().any(receiver) {
            return Ok(None);
        }
        let span = object.span;
        if let Some(spread) = object.props.iter().find_map(|prop| match prop {
            ast::PropOrSpread::Spread(spread) => Some(spread.dot3_token),
            ast::PropOrSpread::Prop(_) => None,
        }) {
            return Err(self.source.locate(
                unsupported("spread in an object literal whose methods use this"),
                spread,
            ));
        }
        let ty = match expected {
            Some(ty @ Type::Record(_)) => ty,
            None => self.receiver_type(object)?,
            Some(other) => {
                return Err(self.source.locate(
                    unsupported(&format!(
                        "object literal whose methods use this in a {other} context (expected a closed object type)"
                    )),
                    span,
                ));
            }
        };
        let mut rewritten = object.clone();
        for prop in &mut rewritten.props {
            let ast::PropOrSpread::Prop(prop) = prop else {
                continue;
            };
            let ast::Prop::Method(method) = &**prop else {
                continue;
            };
            if !bound(&method.function) {
                continue;
            }
            let function = &method.function;
            self.core.method_receivers.insert(
                (function.span.lo.0, function.span.hi.0),
                (SELF.to_owned(), ty),
            );
            **prop = self.bound_method(method)?;
        }
        let value = self.record(&rewritten, Some(ty))?;
        Ok(Some(unions::widen(value, Some(ty), self.types)?))
    }

    /// `m(params): R { body }` as `m: (params): R => { body }` with `this`
    /// replaced by the method's hidden receiver binding.
    fn bound_method(&self, method: &ast::MethodProp) -> Result<ast::Prop> {
        let function = &method.function;
        if function.is_generator {
            return Err(self.source.locate(
                unsupported("generator method that uses this in an object literal"),
                function.span,
            ));
        }
        let mut params: Vec<ast::Pat> = function.params.iter().map(|p| p.pat.clone()).collect();
        let mut body = function.body.clone().ok_or_else(|| {
            self.source.locate(
                invalid("object literal method without a body"),
                function.span,
            )
        })?;
        let mut visitor = Receiver { sup: None };
        for param in &mut params {
            param.visit_mut_with(&mut visitor);
        }
        body.visit_mut_with(&mut visitor);
        if let Some(span) = visitor.sup {
            return Err(self
                .source
                .locate(unsupported("super in an object literal method"), span));
        }
        let arrow = ast::ArrowExpr {
            span: function.span,
            ctxt: Default::default(),
            params,
            body: Box::new(ast::ArrowFunctionBody::FunctionBody(body)),
            is_async: function.is_async,
            is_generator: false,
            type_params: function.type_params.clone(),
            return_type: function.return_type.clone(),
        };
        Ok(ast::Prop::KeyValue(ast::KeyValueProp {
            key: method.key.clone(),
            value: Box::new(ast::Expr::Arrow(arrow)),
        }))
    }

    /// The entry of an object-literal method closure (in the closure's own
    /// lowering): declare its `this` binding and take the receiver from the
    /// register. `undefined` stays `undefined`; an object of the method's
    /// type (or one viewed as it) is the receiver; any other object throws.
    pub(super) fn receiver_prologue(
        &mut self,
        name: &str,
        ty: Type,
        span: swc_common::Span,
    ) -> Result<Vec<S>> {
        let holder = unions::intern(vec![ty, Type::Undefined], self.types)?;
        self.declare(name, Some(holder), true, true)?;
        let binding = self.binding(name)?;
        let raw = self.hidden(ty);
        let descriptor = self.hidden(Type::Number);
        self.types.receivers.used = true;
        let local = |slot| Expr {
            span,
            ty,
            kind: E::Local(slot),
        };
        let zero = Expr {
            span,
            ty,
            kind: E::Zero,
        };
        let undefined = Expr {
            span,
            ty: Type::Undefined,
            kind: E::Integer(0),
        };
        let foreign = self.array_error(
            varta_common::value::ErrorClass::TypeError,
            FOREIGN_RECEIVER,
            span,
        )?;
        let Type::Record(shape) = ty else {
            return Err(CompilerError::Internal(
                "object literal method receiver is not a record".into(),
            ));
        };
        Ok(vec![
            S::Init(
                binding.slot,
                unions::widen(undefined, Some(holder), self.types)?,
            ),
            S::Init(
                raw,
                Expr {
                    span,
                    ty,
                    kind: E::ModuleConstant(REGISTER),
                },
            ),
            S::InitModuleConstant(REGISTER, zero.clone()),
            S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Binary(ast::BinaryOp::NotEqEq, Box::new(local(raw)), Box::new(zero)),
                },
                vec![S::If(
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::IsClass {
                            value: Box::new(local(raw)),
                            shapes: vec![shape, VIEW_SOURCES],
                            slot: descriptor,
                        },
                    },
                    vec![S::Set(
                        binding.slot,
                        unions::widen(local(raw), Some(holder), self.types)?,
                    )],
                    vec![S::Throw(foreign)],
                )],
                vec![],
            ),
        ])
    }

    /// Whether `o.m(...)` passes `o` to the function in field `m`: in a
    /// program with object-literal methods that use `this`, for a plain
    /// function field of a record or class instance.
    pub(super) fn receiver_call(&self, member: &ast::MemberExpr) -> bool {
        if !self.types.receivers.enabled || !matches!(member.prop, ast::MemberProp::Ident(_)) {
            return false;
        }
        let object = self.peek_type(&member.obj, 0);
        if !matches!(object, Some(Type::Record(_) | Type::Class(_))) {
            return false;
        }
        matches!(
            self.peek_type(&ast::Expr::Member(member.clone()), 0),
            Some(Type::Function(id)) if {
                let function = &self.types.functions[id as usize];
                function.receiver.is_none()
                    && function.generic.is_none()
                    && function.overloads.is_none()
            }
        )
    }

    /// `o.m(args)` of a function field in a program with object-literal
    /// methods that use `this`: `o` is evaluated once into a local, the
    /// field read from it and the call passes it as the receiver.
    pub(super) fn method_call(
        &mut self,
        call: &ast::CallExpr,
        member: &ast::MemberExpr,
    ) -> Result<Option<Expr>> {
        if !self.types.receivers.enabled {
            return Ok(None);
        }
        let base = self.expr(&member.obj)?;
        if !matches!(base.ty, Type::Record(_) | Type::Class(_)) {
            return Err(CompilerError::Internal(
                "method call on a base that is not an object".into(),
            ));
        }
        let span = call.span;
        let name = format!("#receiver{}", self.next);
        self.declare(&name, Some(base.ty), true, false)?;
        let binding = self.binding(&name)?;
        let mut field = member.clone();
        field.obj = Box::new(ast::Expr::Ident(ast::Ident::new(
            name.as_str().into(),
            member.obj.span(),
            Default::default(),
        )));
        let value = self.expr(&ast::Expr::Member(field))?;
        let Type::Function(id) = value.ty else {
            return Err(self.source.locate(
                type_error(format!(
                    "called field may not be a function ({}); narrow it or call it with ?.()",
                    value.ty
                )),
                value.span,
            ));
        };
        let signature = self.types.functions[id as usize].clone();
        if signature.generic.is_some() || signature.receiver.is_some() {
            return Err(CompilerError::Internal(
                "method call of a generic or receiver function field".into(),
            ));
        }
        if !signature.shape.extended() && call.args.len() != signature.params.len() {
            return Err(invalid("callback argument count mismatch"));
        }
        let args = self.call_arguments(&call.args, &signature.params, &signature.shape)?;
        let receiver = Expr {
            span: member.obj.span(),
            ty: base.ty,
            kind: E::Local(binding.slot),
        };
        let call = self.invoke_receiver(value, Some(receiver), args, span)?;
        let output = self.hidden(Type::Boolean);
        let marker = self.hidden(Type::Number);
        let taken = Expr {
            span,
            ty: Type::Boolean,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![],
                body: vec![
                    S::Init(binding.slot, base),
                    S::Init(
                        output,
                        Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::Integer(1),
                        },
                    ),
                ],
            },
        };
        Ok(Some(Expr {
            span,
            ty: call.ty,
            kind: E::Sequence(Box::new(taken), Box::new(call)),
        }))
    }

    /// The record type of a literal without a context: literal and binding
    /// field values, and the annotated signatures of its methods.
    fn receiver_type(&mut self, object: &ast::ObjectLit) -> Result<Type> {
        let mut fields = Vec::with_capacity(object.props.len());
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
                _ => return Err(unsupported("object literal member form")),
            };
            fields.push(ir::RecordField {
                name,
                ty,
                optional: false,
            });
        }
        records::intern(fields, self.types)
    }
    fn evident_type(&self, value: &ast::Expr, span: swc_common::Span) -> Result<Type> {
        parameters::literal(value, self.types)
            .or_else(|| self.peek_type(value, 0))
            .map(|ty| literals::erased(ty, self.types))
            .ok_or_else(|| {
                self.source.locate(
                    unsupported(
                        "field value of an unannotated object literal whose methods use this (annotate the object's type)",
                    ),
                    span,
                )
            })
    }
    fn method_type(&mut self, method: &ast::MethodProp) -> Result<Type> {
        let function = &method.function;
        let span = function.span;
        let Some(result) = function.return_type.clone() else {
            return Err(self.source.locate(
                unsupported(
                    "method without a return type annotation in an unannotated object literal whose methods use this",
                ),
                span,
            ));
        };
        if function.type_params.is_some() {
            return Err(self.source.locate(
                unsupported("generic method in an object literal whose methods use this"),
                span,
            ));
        }
        let mut params = Vec::with_capacity(function.params.len());
        for param in &function.params {
            params.push(
                field_arrows::parameter(&param.pat)
                    .map_err(|e| self.source.locate(e, param.span))?,
            );
        }
        let signature = ast::TsFnType {
            span,
            params,
            type_params: None,
            type_ann: result,
        };
        functions::type_annotation(&signature, self.types).map_err(|e| self.source.locate(e, span))
    }
}
