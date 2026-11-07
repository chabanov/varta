//! Class values (v38 `declaration_forms`, owner decision of 2026-11-06).
//!
//! A class named where a value is expected, and a binding of type `typeof
//! C`, is a class object: [`Type::ClassValue`]`(C)`, the identity of `C` or
//! of one of its subclasses (the class id, an `i32` erased to `Number`
//! before emission). Classes are a closed world, so every operation on a
//! class value dispatches over the subclasses of `C` known at link time:
//!
//! - `new v(args)` evaluates `v` and the arguments once (against `C`'s
//!   constructor) and constructs the concrete class whose id `v` holds; a
//!   subclass whose constructor does not accept `C`'s parameters is not a
//!   `typeof C` ([`assignable`]), so it never reaches the dispatch;
//! - `v.m(args)` calls the own or inherited static method `m` of that class
//!   (one direct call when every subclass shares the implementation);
//! - `v.name` is the class's name, `typeof v` is `"function"`, and `===`
//!   compares identities.
//!
//! A class value converts to a constructor type (`new (…) => T`) as a
//! closure that constructs through the dispatch. It never enters a union,
//! an aggregate or `unknown` (it has no value tag), and static fields are
//! not read through it yet (they are module variables of each class).
use super::*;

/// The constructor parameters and shape of `class`, when it has one.
fn constructor(types: &ir::ValueTypes, class: u32) -> Option<(&[Type], &ir::ParameterShape)> {
    let init = types.classes.get(class).init.as_ref()?;
    Some((&init.params, &init.shape))
}

/// Whether a class object of `actual` is a `typeof expected`: `actual` is
/// `expected` or a subclass whose constructor accepts every argument list
/// of `expected`'s constructor (TypeScript's construct signature
/// assignability, required for `new` to dispatch soundly).
pub(super) fn assignable(types: &ir::ValueTypes, actual: u32, expected: u32) -> bool {
    if actual == expected {
        return true;
    }
    if !types.classes.descends(actual, expected) || types.classes.get(actual).is_template() {
        return false;
    }
    match (constructor(types, actual), constructor(types, expected)) {
        (Some((params, shape)), Some((wanted, wanted_shape))) => {
            params.len() == wanted.len()
                && shape == wanted_shape
                && params
                    .iter()
                    .zip(wanted)
                    .all(|(param, wanted)| arguments::accepts(*param, *wanted, types))
        }
        // Before layout every class constructor is assumed compatible; the
        // dispatch checks again.
        _ => true,
    }
}

/// The classes a class object of `class` may hold: `class` and every
/// subclass, in id order (a static method's receiver is any subclass, as in
/// TypeScript, whatever its constructor).
fn family(types: &ir::ValueTypes, class: u32) -> Vec<u32> {
    (0..types.classes.count())
        .filter(|id| {
            let info = types.classes.get(*id);
            !info.prelude && !info.is_template() && types.classes.descends(*id, class)
        })
        .collect()
}

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}

/// `identity === class` on the physical class id.
fn is_class(span: swc_common::Span, identity: u32, class: u32) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Binary(
            ast::BinaryOp::EqEqEq,
            Box::new(local(span, identity, Type::Number)),
            Box::new(Expr {
                span,
                ty: Type::Number,
                kind: E::Integer(class as i32),
            }),
        ),
    }
}

/// `if (identity === c1) …1 else if (identity === c2) …2 … else trap`: the
/// last arm of a closed dispatch is unreachable.
fn dispatch(span: swc_common::Span, identity: u32, arms: Vec<(u32, Vec<S>)>) -> Vec<S> {
    let mut chain = vec![S::Trap];
    for (class, body) in arms.into_iter().rev() {
        chain = vec![S::If(is_class(span, identity, class), body, chain)];
    }
    chain
}

impl Lower<'_> {
    /// The receiver version of the static method `name` that `class`
    /// inherits from a class declared in another module (no copy of a
    /// `this`-using method exists in `class`, `desugar::statics`): it runs
    /// with the class object of `class` as `this`.
    pub(super) fn receiver_method(
        &self,
        class: u32,
        name: &str,
    ) -> Result<Option<classes::Method>> {
        if !self.types.profile.declaration_forms() {
            return Ok(None);
        }
        let statics = &self.types.classes.get(class).statics;
        let Some(method) = statics.get(name).filter(|method| method.owner != class) else {
            return Ok(None);
        };
        let declared = |key: String| statics.get(&key).filter(|m| m.owner == method.owner);
        if let Some(receiver) = declared(crate::desugar::receiver_key(name)) {
            return Ok(Some(receiver.clone()));
        }
        if declared(crate::desugar::unshared_key(name)).is_some() {
            return Err(unsupported(&format!(
                "static method {name} of class {} called with the subclass {} as this: it writes a static field through this (an own property of the subclass) or calls super",
                self.types.classes.get(method.owner).name,
                self.types.classes.get(class).name
            )));
        }
        Ok(None)
    }
    /// `C.m(args)` of a static method that `C` inherits from another
    /// module's class through its receiver version.
    pub(super) fn receiver_static_call(
        &mut self,
        call: &ast::CallExpr,
        class: u32,
        receiver: &classes::Method,
    ) -> Result<Expr> {
        let span = call.span;
        if call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(unsupported("spread to a class method"));
        }
        let signature = &receiver.signature;
        let (Some(this), Some((_, optional))) = (
            signature.params.first().copied(),
            signature.shape.optional.split_first(),
        ) else {
            return Err(CompilerError::Internal("missing static receiver".into()));
        };
        let params = signature.params[1..].to_vec();
        let shape = ir::ParameterShape {
            optional: optional.to_vec(),
            rest: signature.shape.rest,
        };
        let values = self.call_arguments(&call.args, &params, &shape)?;
        let mut args = vec![Expr {
            span,
            ty: this,
            kind: E::Integer(class as i32),
        }];
        args.extend(self.bind_arguments(values, &params, &shape, span)?);
        Ok(Expr {
            span,
            ty: signature.result,
            kind: E::Call(signature.index, args),
        })
    }
    /// A class-object expression (`C`, `new v(…)`, `v.m`, `C.name`, `v ===
    /// w`), lowered outside `expr_lowered` so the recursive expression
    /// lowering keeps a small native stack frame.
    #[inline(never)]
    pub(super) fn class_value_expression(&mut self, expression: &ast::Expr) -> Option<Result<Expr>> {
        if !self.types.profile.declaration_forms() {
            return None;
        }
        match expression {
            ast::Expr::Ident(id) => {
                let class = self.class_value_name(expression)?;
                Some(self.class_value(id, class))
            }
            ast::Expr::New(new) => {
                let class = self.class_value_path(&new.callee)?;
                Some(self.class_value_new(new, class))
            }
            ast::Expr::Member(member) if matches!(member.prop, ast::MemberProp::Ident(_)) => {
                let class = self.class_value_path(&member.obj).or_else(|| {
                    self.class_value_name(&member.obj).filter(
                        |_| matches!(&member.prop, ast::MemberProp::Ident(id) if id.sym == "name"),
                    )
                })?;
                Some(self.class_value_member(member, class))
            }
            ast::Expr::Bin(binary) => self.class_value_equality(binary).transpose(),
            _ => None,
        }
    }
    /// The class a value-position identifier names (not shadowed by a
    /// binding or function), when class values are admitted.
    pub(super) fn class_value_name(&self, expression: &ast::Expr) -> Option<u32> {
        if !self.types.profile.declaration_forms() {
            return None;
        }
        // An engine class keeps its own value rules (`typeof Blob`).
        self.class_callee(expression)
            .filter(|class| !self.types.classes.get(*class).prelude)
    }
    /// The class object of the class `id` names.
    pub(super) fn class_value(&mut self, id: &ast::Ident, class: u32) -> Result<Expr> {
        let info = self.types.classes.get(class);
        if info.is_template() {
            return Err(self.source.locate(
                unsupported(
                    "generic class as a value (a generic class has no single class object)",
                ),
                id.span,
            ));
        }
        Ok(Expr {
            span: id.span,
            ty: Type::ClassValue(class),
            kind: E::Integer(class as i32),
        })
    }
    /// The class of a class-object path (`v`, `this.k`) without lowering it.
    pub(super) fn class_value_path(&self, expression: &ast::Expr) -> Option<u32> {
        match self.peek_type(expression, 0) {
            Some(Type::ClassValue(class)) => Some(class),
            _ => None,
        }
    }
    /// A class object where a constructor type is expected: the closure
    /// `(a0, …) => new v(a0, …)` over the binding `v`, which constructs
    /// through the dispatch.
    pub(super) fn class_value_conversion(
        &mut self,
        value: Expr,
        expected: Option<Type>,
        expression: &ast::Expr,
    ) -> Result<Expr> {
        let (Type::ClassValue(class), Some(construct @ Type::Function(function))) =
            (value.ty, expected)
        else {
            return Ok(value);
        };
        if !self.types.functions[function as usize].construct {
            return Ok(value);
        }
        let ast::Expr::Ident(id) = stmt::unparen(expression) else {
            return Err(unsupported(
                "class value expression where a constructor type is expected; bind it to a const first",
            ));
        };
        self.class_constructor_value(id, (class, construct))
    }
    /// `new v(args)` of a class object `v`.
    pub(super) fn class_value_new(&mut self, new: &ast::NewExpr, class: u32) -> Result<Expr> {
        let span = new.span;
        if new.type_args.is_some() {
            return Err(unsupported("type arguments to new of a class value"));
        }
        self.reject_abstract_new(class, span)?;
        let args = new.args.as_deref().unwrap_or_default();
        if args.iter().any(|arg| arg.spread.is_some()) {
            return Err(unsupported("spread to a class constructor"));
        }
        self.class_functions()?;
        let callee = self.expr(&new.callee)?;
        let signature = self
            .types
            .classes
            .get(class)
            .init
            .clone()
            .ok_or_else(|| CompilerError::Internal("missing class initializer".into()))?;
        let values = self.call_arguments(args, &signature.params, &signature.shape)?;
        let values = self.bind_arguments(values, &signature.params, &signature.shape, span)?;
        let identity = self.hidden(Type::Number);
        let mut body = vec![S::Init(
            identity,
            Expr {
                ty: Type::Number,
                ..callee
            },
        )];
        let mut saved = Vec::new();
        for value in values {
            let slot = self.hidden(value.ty);
            saved.push((slot, value.ty));
            body.push(S::Init(slot, value));
        }
        let result = Type::Class(class);
        let output = self.hidden(result);
        let mut arms = Vec::new();
        for target in family(self.types, class) {
            if self.types.classes.get(target).forms.is_abstract {
                continue;
            }
            if !assignable(self.types, target, class) {
                return Err(self.source.locate(
                    type_error(format!(
                        "the constructor of class {} does not accept the arguments of the constructor of class {}, which new of a class value passes",
                        self.types.classes.get(target).name,
                        self.types.classes.get(class).name
                    )),
                    new.callee.span(),
                ));
            }
            let values = saved
                .iter()
                .map(|(slot, ty)| local(span, *slot, *ty))
                .zip(
                    self.types
                        .classes
                        .get(target)
                        .init
                        .as_ref()
                        .map_or(Vec::new(), |init| init.params.clone()),
                )
                .map(|(value, param)| unions::widen(value, Some(param), self.types))
                .collect::<Result<Vec<_>>>()?;
            let instance = self.construct(span, target, values)?;
            let instance = unions::widen(instance, Some(result), self.types)?;
            arms.push((target, vec![S::Init(output, instance)]));
        }
        body.extend(dispatch(span, identity, arms));
        let marker = self.hidden(Type::Number);
        let roots = saved
            .iter()
            .filter(|(_, ty)| ty.needs_root())
            .map(|(slot, _)| *slot)
            .collect();
        Ok(Expr {
            span,
            ty: result,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }
    /// `v.m(args)`: the static method `m` of the class `v` holds.
    pub(super) fn class_value_static_call(
        &mut self,
        call: &ast::CallExpr,
        member: &ast::MemberExpr,
        class: u32,
    ) -> Result<Expr> {
        let span = call.span;
        let name = self.member_key(&member.prop)?;
        if call.type_args.is_some()
            || self
                .types
                .classes
                .get(class)
                .generic_statics
                .contains_key(&name)
        {
            return Err(unsupported(
                "generic static method call through a class value",
            ));
        }
        if call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(unsupported("spread to a class method"));
        }
        self.class_functions()?;
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
        if self
            .types
            .overloads
            .method(method.owner, &name, true)
            .is_some()
        {
            return Err(unsupported(
                "overloaded static method call through a class value",
            ));
        }
        let signature = method.signature;
        let callee = self.expr(&member.obj)?;
        let values = self.call_arguments(&call.args, &signature.params, &signature.shape)?;
        let values = self.bind_arguments(values, &signature.params, &signature.shape, span)?;
        let identity = self.hidden(Type::Number);
        let mut body = vec![S::Init(
            identity,
            Expr {
                ty: Type::Number,
                ..callee
            },
        )];
        let mut saved = Vec::new();
        for value in values {
            let slot = self.hidden(value.ty);
            saved.push((slot, value.ty));
            body.push(S::Init(slot, value));
        }
        // The implementation each class of the family calls.
        // (class, implementation, whether it takes the class as receiver).
        let mut targets: Vec<(u32, Signature, bool)> = Vec::new();
        for target in family(self.types, class) {
            let Some(own) = self.types.classes.get(target).statics.get(&name).cloned() else {
                continue;
            };
            let compatible = own.signature.params.len() == signature.params.len()
                && own.signature.shape == signature.shape
                && own
                    .signature
                    .params
                    .iter()
                    .zip(&signature.params)
                    .all(|(param, wanted)| arguments::accepts(*param, *wanted, self.types))
                && arguments::accepts(signature.result, own.signature.result, self.types);
            if !compatible {
                return Err(self.source.locate(
                    type_error(format!(
                        "static method {name} of class {} is not compatible with that of class {} (TS2417)",
                        self.types.classes.get(target).name,
                        self.types.classes.get(class).name
                    )),
                    member.prop.span(),
                ));
            }
            match self
                .receiver_method(target, &name)
                .map_err(|e| self.source.locate(e, member.prop.span()))?
            {
                Some(receiver) => targets.push((target, receiver.signature, true)),
                None => targets.push((target, own.signature, false)),
            }
        }
        let result = signature.result;
        let call_of =
            |this: &mut Self, (class, target, receiver): &(u32, Signature, bool)| -> Result<Expr> {
                let params = &target.params[usize::from(*receiver)..];
                let mut args: Vec<Expr> = target
                    .params
                    .first()
                    .filter(|_| *receiver)
                    .map(|ty| Expr {
                        span,
                        ty: *ty,
                        kind: E::Integer(*class as i32),
                    })
                    .into_iter()
                    .collect();
                for (value, param) in saved
                    .iter()
                    .map(|(slot, ty)| local(span, *slot, *ty))
                    .zip(params)
                {
                    args.push(unions::widen(value, Some(*param), this.types)?);
                }
                let value = Expr {
                    span,
                    ty: target.result,
                    kind: E::Call(target.index, args),
                };
                unions::widen(value, Some(result), this.types)
            };
        let roots: Vec<u32> = saved
            .iter()
            .filter(|(_, ty)| ty.needs_root())
            .map(|(slot, _)| *slot)
            .collect();
        let shared = targets.first().map(|(_, first, _)| {
            targets
                .iter()
                .all(|(_, s, receiver)| !receiver && s.index == first.index)
        });
        let marker = self.hidden(Type::Number);
        // A void call's block has no output (its width is zero).
        let output = self.hidden(if result == Type::Void {
            Type::Number
        } else {
            result
        });
        let store = |value: Expr| {
            if result == Type::Void {
                S::Expr(value)
            } else {
                S::Init(output, value)
            }
        };
        if shared == Some(true) {
            let value = call_of(self, &targets[0])?;
            body.push(store(value));
        } else {
            let mut arms = Vec::new();
            for target in &targets {
                let value = call_of(self, target)?;
                arms.push((target.0, vec![store(value)]));
            }
            body.extend(dispatch(span, identity, arms));
        }
        Ok(Expr {
            span,
            ty: result,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }
    /// `v.name`: the name of the class `v` holds.
    pub(super) fn class_value_member(
        &mut self,
        member: &ast::MemberExpr,
        class: u32,
    ) -> Result<Expr> {
        let span = member.span;
        let name = self.member_key(&member.prop)?;
        if name != "name" {
            return Err(self.source.locate(
                unsupported(&format!(
                    "member {name} of a class value (only name, new and static method calls are dispatched; a static field is read through its class)"
                )),
                member.prop.span(),
            ));
        }
        let callee = self.expr(&member.obj)?;
        let identity = self.hidden(Type::Number);
        let output = self.hidden(Type::String);
        let mut arms = Vec::new();
        for target in family(self.types, class) {
            let text =
                crate::desugar::display_name(&self.types.classes.get(target).name).to_owned();
            arms.push((
                target,
                vec![S::Init(
                    output,
                    Expr {
                        span,
                        ty: Type::String,
                        kind: E::String(self.types.profile.string_bytes(&text)),
                    },
                )],
            ));
        }
        let mut body = vec![S::Init(
            identity,
            Expr {
                ty: Type::Number,
                ..callee
            },
        )];
        body.extend(dispatch(span, identity, arms));
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty: Type::String,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![],
                body,
            },
        })
    }
    /// `a === b` (or `!==`, `==`, `!=`) of two class objects: their
    /// identities.
    pub(super) fn class_value_equality(&mut self, binary: &ast::BinExpr) -> Result<Option<Expr>> {
        let negated = match binary.op {
            ast::BinaryOp::EqEqEq | ast::BinaryOp::EqEq => false,
            ast::BinaryOp::NotEqEq | ast::BinaryOp::NotEq => true,
            _ => return Ok(None),
        };
        let class_operand = |this: &Self, operand: &ast::Expr| {
            this.class_value_name(operand).is_some() || this.class_value_path(operand).is_some()
        };
        if !class_operand(self, &binary.left) && !class_operand(self, &binary.right) {
            return Ok(None);
        }
        let left = self.expr(&binary.left)?;
        let right = self.expr(&binary.right)?;
        if !matches!(left.ty, Type::ClassValue(_)) || !matches!(right.ty, Type::ClassValue(_)) {
            return Err(type_error(format!(
                "comparison of a class value with {} (TS2367)",
                if matches!(left.ty, Type::ClassValue(_)) {
                    right.ty
                } else {
                    left.ty
                }
            )));
        }
        let span = binary.span;
        let compare = Expr {
            span,
            ty: Type::Boolean,
            kind: E::Binary(
                ast::BinaryOp::EqEqEq,
                Box::new(Expr {
                    ty: Type::Number,
                    ..left
                }),
                Box::new(Expr {
                    ty: Type::Number,
                    ..right
                }),
            ),
        };
        Ok(Some(if negated {
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Not(Box::new(compare)),
            }
        } else {
            compare
        }))
    }
}
