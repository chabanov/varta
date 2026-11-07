//! V38 object-literal methods that use `this` (`expression_forms`).
//!
//! A method `m() { ... this ... }` of an object literal is called as
//! `o.m()` with `this` = `o`. Function fields have no receiver, so the
//! literal binds its methods to the object it creates: a hidden binding
//! receives the object right after it is built, and every method is an
//! arrow over that binding (`this` reads it with a checked non-null
//! assertion). This is JavaScript's behaviour for every call `o.m(...)`
//! through the object; a method value copied to another object keeps its
//! original receiver, so spreads and `Object.assign` of such objects are
//! refused (`subtyping::spread_views`), and an extracted method called alone sees the
//! object instead of `undefined`.
//!
//! The object's type is its contextual record type, or, without one, the
//! record of its literal or binding field values and annotated methods.
//! Getters and setters (accessor properties) of object literals stay
//! refused: a closed record has data fields only.
use super::*;
use swc_ecma_visit::{VisitMut, VisitMutWith};

/// Prefix of the hidden binding that receives the object; `#` cannot start
/// a source identifier.
const SELF: &str = "#self";

/// Replaces `this` (outside nested functions and classes) by `#selfN!`.
struct Receiver<'a> {
    name: &'a str,
    sup: Option<swc_common::Span>,
}
impl VisitMut for Receiver<'_> {
    fn visit_mut_expr(&mut self, expr: &mut ast::Expr) {
        if let ast::Expr::This(this) = expr {
            let span = this.span;
            *expr = ast::Expr::TsNonNull(ast::TsNonNullExpr {
                span,
                expr: Box::new(ast::Expr::Ident(ast::Ident::new(
                    self.name.into(),
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

/// A method that uses `this` without declaring a `this` parameter (one
/// that declares it keeps its receiver: `o.m()` passes the object).
fn bound(function: &ast::Function) -> bool {
    function.this_param.is_none() && records::uses_receiver(function)
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
        let name = format!("{SELF}{}", self.next);
        let holder = unions::intern(vec![ty, Type::Undefined], self.types)?;
        self.declare(&name, Some(holder), true, true)?;
        let binding = self.binding(&name)?;
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
            **prop = self.bound_method(method, &name)?;
        }
        let value = self.record(&rewritten, Some(ty))?;
        let value = unions::widen(value, Some(ty), self.types)?;
        if let Type::Record(shape) = value.ty {
            self.types.subtyping.note_bound(shape);
        }
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        let undefined = Expr {
            span,
            ty: Type::Undefined,
            kind: E::Integer(0),
        };
        let stored = Expr {
            span,
            ty,
            kind: E::Local(output),
        };
        Ok(Some(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![],
                body: vec![
                    S::Init(
                        binding.slot,
                        unions::widen(undefined, Some(holder), self.types)?,
                    ),
                    S::Init(output, value),
                    S::Set(
                        binding.slot,
                        unions::widen(stored, Some(holder), self.types)?,
                    ),
                ],
            },
        }))
    }

    /// `m(params): R { body }` as `m: (params): R => { body }` with `this`
    /// replaced by the hidden binding `name`.
    fn bound_method(&self, method: &ast::MethodProp, name: &str) -> Result<ast::Prop> {
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
        let mut visitor = Receiver { name, sup: None };
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
