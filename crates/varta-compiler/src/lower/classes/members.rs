//! Class expressions: construction, field access, method calls, `super` and
//! `instanceof`. Accessibility and `readonly` are compile-time checks only.
use super::*;

fn local(span: Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn boolean(span: Span, value: bool) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Integer(i32::from(value)),
    }
}

impl Lower<'_> {
    fn accessible(&self, owner: u32, access: Access, receiver: u32) -> bool {
        let classes = &self.types.classes;
        let current = classes.context.map(|c| c.class);
        match access {
            Access::Public => true,
            Access::Private => current == Some(owner),
            // TypeScript: only within subclasses, through their own instances.
            Access::Protected => current.is_some_and(|current| {
                classes.descends(current, owner) && classes.descends(receiver, current)
            }),
        }
    }
    fn check_access(
        &self,
        owner: u32,
        access: Access,
        receiver: u32,
        name: &str,
        span: Span,
    ) -> Result<()> {
        if self.accessible(owner, access, receiver) {
            return Ok(());
        }
        let kind = if access == Access::Private {
            "private"
        } else {
            "protected"
        };
        Err(self.source.locate(
            type_error(format!(
                "class member {name} is {kind} to {}",
                self.types.classes.get(owner).name
            )),
            span,
        ))
    }
    /// A missing member; engine prelude classes (v35) explain the members
    /// they name but do not implement yet.
    fn unknown_member(&self, class: u32, name: &str, kind: &str) -> CompilerError {
        match self
            .types
            .classes
            .pending_member(class, name, self.types.profile)
        {
            Some(reason) => CompilerError::Unsupported {
                kind: reason.into(),
                detail: "not implemented by the checked-f64-utf16-v35 engine Web types".into(),
            },
            None => invalid(format!("unknown {kind}: {name}")),
        }
    }
    fn member_key(&self, prop: &ast::MemberProp) -> Result<String> {
        match prop {
            ast::MemberProp::Ident(id) => Ok(id.sym.to_string()),
            ast::MemberProp::PrivateName(_) => Err(unsupported("#private class member")),
            ast::MemberProp::Computed(key) => match stmt::unparen(&key.expr) {
                ast::Expr::Lit(ast::Lit::Str(_)) => records::expression_key(&key.expr),
                _ => Err(unsupported("computed class member access")),
            },
        }
    }
    /// Static class of a call receiver, determined without lowering it, so a
    /// member call can select class dispatch before builtin method names.
    pub(in crate::lower) fn class_receiver(
        &self,
        expression: &ast::Expr,
        depth: usize,
    ) -> Option<u32> {
        if depth >= 64 || !self.types.profile.classes() {
            return None;
        }
        let ty = match stmt::unparen(expression) {
            ast::Expr::This(_) => self.binding("this").ok()?.ty?,
            ast::Expr::Ident(id) => {
                let binding = self.binding(id.sym.as_ref()).ok()?;
                self.refinements
                    .get(&binding.slot)
                    .copied()
                    .or(binding.ty)?
            }
            ast::Expr::New(new) => return self.class_callee(&new.callee),
            ast::Expr::Member(member) => {
                let class = self.class_receiver(&member.obj, depth + 1)?;
                let name = self.member_key(&member.prop).ok()?;
                match self.types.classes.field(class, &name) {
                    Some((_, field)) => field.ty,
                    None => {
                        let method = self.types.classes.get(class).methods.get(&name)?;
                        if !method.getter {
                            return None;
                        }
                        method.signature.result
                    }
                }
            }
            ast::Expr::Call(call) => match &call.callee {
                ast::Callee::Expr(callee) => match stmt::unparen(callee) {
                    ast::Expr::Member(member) if self.class_callee(&member.obj).is_some() => {
                        let class = self.class_callee(&member.obj)?;
                        let name = self.member_key(&member.prop).ok()?;
                        self.types
                            .classes
                            .get(class)
                            .statics
                            .get(&name)?
                            .signature
                            .result
                    }
                    ast::Expr::Member(member) => {
                        let class = self.class_receiver(&member.obj, depth + 1)?;
                        let name = self.member_key(&member.prop).ok()?;
                        self.types
                            .classes
                            .get(class)
                            .methods
                            .get(&name)?
                            .signature
                            .result
                    }
                    ast::Expr::Ident(id) if self.binding(id.sym.as_ref()).is_err() => {
                        self.functions.get(id.sym.as_ref())?.result
                    }
                    _ => return None,
                },
                _ => return None,
            },
            _ => return None,
        };
        match ty {
            Type::Class(class) => Some(class),
            _ => None,
        }
    }
    /// Whether a member call's receiver is a value rather than a namespace.
    pub(in crate::lower) fn class_late_receiver(&self, member: &ast::MemberExpr) -> bool {
        if !self.types.profile.classes()
            || matches!(&member.prop, ast::MemberProp::Ident(id) if id.sym == "toString")
        {
            return false;
        }
        let mut root = &*member.obj;
        while let ast::Expr::Member(inner) = stmt::unparen(root) {
            root = &inner.obj;
        }
        match stmt::unparen(root) {
            ast::Expr::Ident(id) => {
                self.binding(id.sym.as_ref()).is_ok() || self.outer.contains_key(id.sym.as_ref())
            }
            _ => true,
        }
    }
    /// `new C(...)`: allocate every slot with a placeholder (zero handles are
    /// never traced and never observed), then run the initializer.
    pub(in crate::lower) fn class_new(&mut self, new: &ast::NewExpr, class: u32) -> Result<Expr> {
        if new.type_args.is_some() {
            return Err(unsupported("generic class construction"));
        }
        // An engine prelude constructs its own instances (v35 Web types).
        if !crate::modules::engine::is_prelude_source(self.source)
            && self
                .types
                .classes
                .pending_member(
                    class,
                    crate::modules::engine::CONSTRUCTOR,
                    self.types.profile,
                )
                .is_some()
        {
            return Err(self.source.locate(
                self.unknown_member(class, crate::modules::engine::CONSTRUCTOR, "constructor"),
                new.callee.span(),
            ));
        }
        let args = new.args.as_deref().unwrap_or_default();
        if args.iter().any(|arg| arg.spread.is_some()) {
            return Err(unsupported("spread to a class constructor"));
        }
        let info = self.types.classes.get(class);
        let shape = info.shape;
        let field_types: Vec<Type> = info.fields.iter().map(|f| f.ty).collect();
        let signature = info
            .init
            .clone()
            .ok_or_else(|| CompilerError::Internal("missing class initializer".into()))?;
        if !signature.shape.extended() && args.len() != signature.params.len() {
            return Err(invalid(format!(
                "class {} constructor expects {} arguments",
                info.name,
                signature.params.len()
            )));
        }
        let values = self.call_arguments(args, &signature.params, &signature.shape)?;
        let values = self.bind_arguments(values, &signature.params, &signature.shape, new.span)?;
        let span = new.span;
        let mut zeros: BTreeMap<Type, u32> = BTreeMap::new();
        let mut defaults = Vec::new();
        for (slot, ty) in field_types.into_iter().enumerate() {
            // A literal-typed field (v32 `literal_types`) takes the placeholder
            // of its erased base primitive.
            let kind = match ty.erased() {
                Type::Float64 => E::Float64(0.0),
                Type::String => E::String(Vec::new()),
                _ if !ty.is_handle() => E::Integer(0),
                // A never-written local is a zero handle of the right width.
                _ => E::Local(*zeros.entry(ty).or_insert_with(|| self.hidden(ty))),
            };
            defaults.push((slot as u32, Expr { span, ty, kind }));
        }
        let instance = self.hidden(Type::Class(class));
        let marker = self.hidden(Type::Number);
        let mut call_args = vec![local(span, instance, Type::Class(class))];
        call_args.extend(values);
        let body = vec![
            S::Init(
                instance,
                Expr {
                    span,
                    ty: Type::Record(shape),
                    kind: E::Record(defaults),
                },
            ),
            S::Expr(Expr {
                span,
                ty: Type::Void,
                kind: E::Call(signature.index, call_args),
            }),
        ];
        self.class_call_effects(&signature);
        Ok(Expr {
            span,
            ty: Type::Class(class),
            kind: E::ValueBlock {
                output: instance,
                marker,
                roots: vec![instance],
                body,
            },
        })
    }
    /// A constructor/method reaches caller cells only through function values.
    fn class_call_effects(&mut self, signature: &Signature) {
        if !self.types.profile.refinement_writes()
            || !signature
                .params
                .iter()
                .any(|ty| flow_calls::callbacks(*ty, self.types, 0))
        {
            return;
        }
        self.call_unknown = true;
        for slot in self.mutable_cells() {
            self.flow_write(slot);
        }
    }
    /// Field read (`read`) or assignment place of a class instance.
    pub(in crate::lower) fn class_member(
        &mut self,
        member: &ast::MemberExpr,
        value: Expr,
        read: bool,
    ) -> Result<Expr> {
        let Type::Class(class) = value.ty else {
            return Err(invalid("missing class instance"));
        };
        let name = self.member_key(&member.prop)?;
        let Some((slot, field)) = self.types.classes.field(class, &name) else {
            let method = self.types.classes.get(class).methods.get(&name).cloned();
            return match method {
                Some(method) if method.getter && read => {
                    self.check_access(
                        method.owner,
                        method.access,
                        class,
                        &name,
                        member.prop.span(),
                    )?;
                    let (target, receiver) = method.target();
                    Ok(Expr {
                        span: member.span,
                        ty: method.signature.result,
                        kind: E::Call(
                            target,
                            vec![Expr {
                                ty: Type::Class(receiver),
                                ..value
                            }],
                        ),
                    })
                }
                Some(method) => Err(self.source.locate(
                    if method.getter {
                        type_error(format!("class getter {name} has no setter"))
                    } else {
                        unsupported("class method value without a call")
                    },
                    member.prop.span(),
                )),
                None => Err(self.source.locate(
                    self.unknown_member(class, &name, "class member"),
                    member.prop.span(),
                )),
            };
        };
        let field = field.clone();
        self.check_access(field.owner, field.access, class, &name, member.prop.span())?;
        if !read && field.readonly {
            let context = self.types.classes.context;
            let constructor = context.is_some_and(|c| {
                (c.class == field.owner || Some(c.class) == field.redeclared)
                    && c.init == Some(self.index)
                    && matches!(stmt::unparen(&member.obj), ast::Expr::This(_))
            });
            if !constructor {
                return Err(self.source.locate(
                    type_error(format!(
                        "cannot assign to readonly class field {name} outside its constructor"
                    )),
                    member.span,
                ));
            }
        }
        Ok(Expr {
            span: member.span,
            ty: field.ty,
            kind: E::Field(Box::new(value), slot),
        })
    }
    /// `receiver.m(...)`, through the overridden method's dispatcher if any.
    pub(in crate::lower) fn class_method_call(
        &mut self,
        call: &ast::CallExpr,
        member: &ast::MemberExpr,
        value: Option<Expr>,
    ) -> Result<Expr> {
        let value = match value {
            Some(value) => value,
            None => self.expr(&member.obj)?,
        };
        let Type::Class(class) = value.ty else {
            return Err(unsupported("call"));
        };
        let name = self.member_key(&member.prop)?;
        if name == error_classes::TO_STRING && self.types.classes.is_error(class) {
            return self.class_error_to_string(call, value);
        }
        let Some(method) = self.types.classes.get(class).methods.get(&name).cloned() else {
            return Err(self.source.locate(
                if self.types.classes.field(class, &name).is_some() {
                    type_error(format!("class field {name} is not callable"))
                } else {
                    self.unknown_member(class, &name, "class method")
                },
                member.prop.span(),
            ));
        };
        if method.getter {
            return Err(self.source.locate(
                type_error(format!("class getter {name} is not callable")),
                member.prop.span(),
            ));
        }
        self.check_access(
            method.owner,
            method.access,
            class,
            &name,
            member.prop.span(),
        )?;
        let (target, receiver) = method.target();
        let receiver = Expr {
            ty: Type::Class(receiver),
            ..value
        };
        self.class_call(call, &method.signature, target, receiver)
    }
    fn class_call(
        &mut self,
        call: &ast::CallExpr,
        signature: &Signature,
        target: usize,
        receiver: Expr,
    ) -> Result<Expr> {
        if call.type_args.is_some() {
            return Err(unsupported("generic class method call"));
        }
        if call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(unsupported("spread to a class method"));
        }
        if !signature.shape.extended() && call.args.len() != signature.params.len() {
            return Err(invalid(format!(
                "class method expects {} arguments",
                signature.params.len()
            )));
        }
        let values = self.call_arguments(&call.args, &signature.params, &signature.shape)?;
        let mut args = vec![receiver];
        args.extend(self.bind_arguments(values, &signature.params, &signature.shape, call.span)?);
        Ok(Expr {
            span: call.span,
            ty: signature.result,
            kind: E::Call(target, args),
        })
    }
    /// `C.m(...)`: a direct call of an own or inherited static method.
    pub(in crate::lower) fn class_static_call(
        &mut self,
        call: &ast::CallExpr,
        member: &ast::MemberExpr,
        class: u32,
    ) -> Result<Expr> {
        let name = self.member_key(&member.prop)?;
        let Some(method) = self.types.classes.get(class).statics.get(&name).cloned() else {
            return Err(self.source.locate(
                self.unknown_member(class, &name, "static class method"),
                member.prop.span(),
            ));
        };
        self.check_access(
            method.owner,
            method.access,
            class,
            &name,
            member.prop.span(),
        )?;
        if call.type_args.is_some() {
            return Err(unsupported("generic class method call"));
        }
        if call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(unsupported("spread to a class method"));
        }
        let signature = method.signature;
        if !signature.shape.extended() && call.args.len() != signature.params.len() {
            return Err(invalid(format!(
                "class method expects {} arguments",
                signature.params.len()
            )));
        }
        let values = self.call_arguments(&call.args, &signature.params, &signature.shape)?;
        let args = self.bind_arguments(values, &signature.params, &signature.shape, call.span)?;
        Ok(Expr {
            span: call.span,
            ty: signature.result,
            kind: E::Call(signature.index, args),
        })
    }
    pub(super) fn this_value(&mut self, span: Span) -> Result<Expr> {
        self.expr(&ast::Expr::This(ast::ThisExpr { span }))
    }
    /// `super(...)` in a derived constructor runs the base initializer.
    pub(in crate::lower) fn super_call(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        let context = self.types.classes.context;
        let Some(class) = context
            .filter(|c| c.init == Some(self.index))
            .and_then(|c| self.types.classes.get(c.class).parent)
        else {
            return Err(unsupported(
                "super(...) outside a derived class constructor",
            ));
        };
        if self.types.classes.root_family(class).is_some() {
            return self.error_super_call(call, class);
        }
        let signature = self
            .types
            .classes
            .get(class)
            .init
            .clone()
            .ok_or_else(|| CompilerError::Internal("missing base initializer".into()))?;
        let this = self.this_value(call.span)?;
        let receiver = Expr {
            ty: Type::Class(class),
            ..this
        };
        let value = self.class_call(call, &signature, signature.index, receiver)?;
        self.class_call_effects(&signature);
        Ok(value)
    }
    /// `super.m(...)` calls the inherited implementation without dispatch.
    pub(in crate::lower) fn super_method_call(
        &mut self,
        call: &ast::CallExpr,
        property: &ast::SuperPropExpr,
    ) -> Result<Expr> {
        let Some(parent) = self
            .types
            .classes
            .context
            .and_then(|c| self.types.classes.get(c.class).parent)
        else {
            return Err(unsupported("super property outside a derived class"));
        };
        let ast::SuperProp::Ident(name) = &property.prop else {
            return Err(unsupported("computed super property"));
        };
        let name = name.sym.to_string();
        let Some(method) = self.types.classes.get(parent).methods.get(&name).cloned() else {
            return Err(self.source.locate(
                invalid(format!("unknown inherited class method: {name}")),
                property.span,
            ));
        };
        let current = self.types.classes.context.map_or(parent, |c| c.class);
        self.check_access(method.owner, method.access, current, &name, property.span)?;
        let this = self.this_value(property.span)?;
        let receiver = Expr {
            ty: Type::Class(method.owner),
            ..this
        };
        self.class_call(call, &method.signature, method.signature.index, receiver)
    }
    /// A union holds at most one record-tagged member, which may be a class.
    fn union_instance_of(&mut self, value: Expr, class: u32, span: Span) -> Result<Expr> {
        let union = value.ty;
        let member = unions::members(union, self.types)
            .into_iter()
            .find_map(|ty| match ty {
                Type::Class(member) => Some(member),
                _ => None,
            });
        let classes = &self.types.classes;
        let kind = match member {
            Some(member) if classes.descends(member, class) => {
                E::IsUnionMember(Box::new(value), Type::Class(member))
            }
            Some(member) if classes.descends(class, member) => {
                let shapes = classes.family_shapes(class);
                let saved = self.hidden(union);
                let result = self.hidden(Type::Boolean);
                let marker = self.hidden(Type::Number);
                let slot = self.hidden(Type::Number);
                let test = Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Conditional(
                        Box::new(Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::IsUnionMember(
                                Box::new(local(span, saved, union)),
                                Type::Class(member),
                            ),
                        }),
                        Box::new(Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::IsClass {
                                value: Box::new(Expr {
                                    span,
                                    ty: Type::Class(member),
                                    kind: E::UnboxUnion(Box::new(local(span, saved, union))),
                                }),
                                shapes,
                                slot,
                            },
                        }),
                        Box::new(boolean(span, false)),
                    ),
                };
                E::ValueBlock {
                    output: result,
                    marker,
                    roots: vec![saved],
                    body: vec![S::Init(saved, value), S::Init(result, test)],
                }
            }
            _ => E::Sequence(Box::new(value), Box::new(boolean(span, false))),
        };
        Ok(Expr {
            span,
            ty: Type::Boolean,
            kind,
        })
    }
    /// `x instanceof C` narrows a union binding to its class member when that
    /// member is `C` or a subclass; downcasts are tested but do not narrow.
    pub(in crate::lower) fn refine_class(
        &mut self,
        name: &str,
        class: u32,
        yes: bool,
    ) -> Result<()> {
        let Ok(binding) = self.binding(name).or_else(|_| self.resolve_binding(name)) else {
            return Ok(());
        };
        let Some(ty) = self.refinements.get(&binding.slot).copied().or(binding.ty) else {
            return Ok(());
        };
        if ty == Type::Unknown {
            // The descriptor test proves the class family (v34 `unknown`).
            return self.refine_with(name, |_, _| yes.then(|| vec![Type::Class(class)]));
        }
        let member = unions::members(ty, self.types).into_iter().find(
            |member| matches!(member, Type::Class(m) if self.types.classes.descends(*m, class)),
        );
        match member {
            Some(member) => self.refine(name, references::Guard::Exact(member), yes),
            None if yes && self.types.classes.is_error(class) => {
                self.refine_error_downcast(name, class)
            }
            None => Ok(()),
        }
    }
    /// `value instanceof C` by static nominal subtyping, or a runtime
    /// descriptor test when `C` is a proper subclass of the static class.
    pub(in crate::lower) fn class_instance_of(
        &mut self,
        binary: &ast::BinExpr,
        class: u32,
    ) -> Result<Expr> {
        let value = self.expr(&binary.left)?;
        self.class_test(value, class, binary.span)
    }
    /// `value instanceof C` for an already lowered operand.
    pub(in crate::lower) fn class_test(
        &mut self,
        value: Expr,
        class: u32,
        span: Span,
    ) -> Result<Expr> {
        let classes = &self.types.classes;
        let kind = match value.ty {
            Type::Class(actual) if classes.descends(actual, class) => {
                E::Sequence(Box::new(value), Box::new(boolean(span, true)))
            }
            Type::Class(actual) if classes.descends(class, actual) => {
                let shapes = classes.family_shapes(class);
                E::IsClass {
                    value: Box::new(value),
                    shapes,
                    slot: self.hidden(Type::Number),
                }
            }
            Type::Unknown => return self.unknown_instance_of(value, Type::Class(class), span),
            ty if ty.is_union() => return self.union_instance_of(value, class, span),
            // Class instances never inhabit unrelated classes, records,
            // arrays, primitives or Error values.
            ty if ty.value_kind().is_some() => {
                E::Sequence(Box::new(value), Box::new(boolean(span, false)))
            }
            _ => return Err(unsupported("instanceof operand type")),
        };
        Ok(Expr {
            span,
            ty: Type::Boolean,
            kind,
        })
    }
}
