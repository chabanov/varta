//! The language items of the v38 web-bytes stream (`web_bytes`): the typed
//! global object, `typeof` of an engine global or a class method,
//! `new Uint8Array(buffer)` over an `ArrayBuffer`, and type assertions that
//! are structural upcasts. The Web types themselves are the `fetch`, blob and
//! global engine preludes.
use super::*;
use crate::lower::expr::callee_name;
use crate::modules::engine::{
    ARRAY_BUFFER_CLASS, BLOB, BLOB_VIEW, ENVIRONMENT_VALUE, GLOBAL, GLOBAL_OBJECT, GLOBAL_THIS,
    PROCESS_CLASS,
};
use crate::modules::engine_globals;

impl Lower<'_> {
    /// `globalThis`, unless the module binds the name: the global prelude's
    /// `GlobalObject`, whose getters are the typed global members.
    pub(super) fn global_object(
        &mut self,
        name: &str,
        span: swc_common::Span,
    ) -> Result<Option<Expr>> {
        if !self.types.profile.web_bytes()
            || name != GLOBAL_THIS
            || self.binding(name).is_ok()
            || self.functions.contains_key(name)
        {
            return Ok(None);
        }
        self.engine_apply((&GLOBAL, GLOBAL_OBJECT, &[]), vec![], span)
            .map(Some)
    }
    /// `typeof NAME` of an engine global identifier that no binding, function
    /// or generic function of the program shadows: the global's constant
    /// JavaScript `typeof` string (`"function"` for a class or function,
    /// `"object"` for a namespace or object), so feature detection such as
    /// `typeof Blob !== 'undefined'` compiles. Nothing is evaluated and no
    /// prelude is loaded for the operand. A shadowed name keeps the ordinary
    /// `typeof` of its binding; any other identifier stays unknown.
    pub(super) fn global_typeof(&mut self, unary: &ast::UnaryExpr) -> Option<Expr> {
        if unary.op != ast::UnaryOp::TypeOf {
            return None;
        }
        let ast::Expr::Ident(name) = stmt::unparen(&unary.arg) else {
            return None;
        };
        let name = name.sym.as_ref();
        if self.binding(name).is_ok()
            || self.functions.contains_key(name)
            || self.generic_name(name).is_some()
        {
            return None;
        }
        let kind = engine_globals::kind(name, self.types.profile)?;
        Some(Expr {
            span: unary.span,
            ty: Type::String,
            kind: E::String(self.types.profile.string_bytes(kind.type_of())),
        })
    }
    /// `typeof x.m` of a class method `m`: always `"function"`, as a method is
    /// never an absent member (feature detection such as
    /// `typeof crypto.randomUUID === 'function'`). Fields and getters keep the
    /// ordinary `typeof` of their value.
    pub(super) fn method_typeof(&mut self, unary: &ast::UnaryExpr) -> Result<Option<Expr>> {
        if !self.types.profile.web_bytes() || unary.op != ast::UnaryOp::TypeOf {
            return Ok(None);
        }
        let ast::Expr::Member(member) = stmt::unparen(&unary.arg) else {
            return Ok(None);
        };
        let ast::MemberProp::Ident(property) = &member.prop else {
            return Ok(None);
        };
        // A receiver without calls (a binding, `this` or a member path) is
        // lowered here and again by the ordinary `typeof` when it is no
        // class instance, which is free of effects.
        let mut root = stmt::unparen(&member.obj);
        while let ast::Expr::Member(inner) = root {
            if !matches!(inner.prop, ast::MemberProp::Ident(_)) {
                return Ok(None);
            }
            root = stmt::unparen(&inner.obj);
        }
        if !matches!(root, ast::Expr::Ident(_) | ast::Expr::This(_))
            || !self.class_late_receiver(member)
        {
            return Ok(None);
        }
        let receiver = self.expr(&member.obj)?;
        let Type::Class(class) = receiver.ty else {
            return Ok(None);
        };
        let name = property.sym.as_ref();
        let info = self.types.classes.get(class);
        let method = info.methods.get(name).is_some_and(|method| !method.getter)
            || info.generic_methods.contains_key(name);
        if !method {
            return Ok(None);
        }
        Ok(Some(Expr {
            span: unary.span,
            ty: Type::String,
            kind: E::Sequence(
                Box::new(receiver),
                Box::new(Expr {
                    span: unary.span,
                    ty: Type::String,
                    kind: E::String(self.types.profile.string_bytes("function")),
                }),
            ),
        }))
    }
    /// The storage of `new Uint8Array(buffer)` when `value` is an
    /// `ArrayBuffer`: the buffer's own storage, so the view shares it.
    pub(super) fn buffer_view(&mut self, value: Expr) -> Result<std::result::Result<Expr, Expr>> {
        let Type::Class(class) = value.ty else {
            return Ok(Err(value));
        };
        let info = self.types.classes.get(class);
        if !self.types.profile.web_bytes() || !info.prelude || info.name != ARRAY_BUFFER_CLASS {
            return Ok(Err(value));
        }
        let span = value.span;
        self.engine_apply((&BLOB, BLOB_VIEW, &[]), vec![value], span)
            .map(Ok)
    }
    /// `x as T` where a value of `x`'s type is structurally assignable to `T`
    /// (a class instance or record asserted to a wider object type): the
    /// coercion through a view, exactly as an assignment would make it.
    pub(super) fn structural_upcast(
        &mut self,
        value: Expr,
        target: Type,
    ) -> Result<std::result::Result<Expr, Expr>> {
        if !self.types.profile.web_bytes() || !subtyping::assignable(target, value.ty, self.types) {
            return Ok(Err(value));
        }
        subtyping::coerce(value, Some(target), self.types).map(Ok)
    }
    /// `await Promise.race([...])` with an expected type (v38 `web_bytes`):
    /// the race takes `Promise<expected>` as its context, so promises of
    /// different types race into the declared union (TypeScript infers the
    /// call from the awaited context). Other awaited operands keep their own
    /// inference.
    pub(super) fn awaited_race(
        &mut self,
        expression: &ast::AwaitExpr,
        expected: Option<Type>,
    ) -> Result<Option<Expr>> {
        let Some(expected) = expected else {
            return Ok(None);
        };
        if !self.types.profile.web_bytes()
            || self.asynchronous.is_none() && self.async_body().is_none()
        {
            return Ok(None);
        }
        if !self.races(&expression.arg) {
            return Ok(None);
        }
        let Ok(promise) = promises::intern(expected, self.types) else {
            return Ok(None);
        };
        self.expr_expected(&expression.arg, Some(promise)).map(Some)
    }
    /// Whether `expression` is a call of the global `Promise.race`.
    fn races(&self, expression: &ast::Expr) -> bool {
        let ast::Expr::Call(call) = stmt::unparen(expression) else {
            return false;
        };
        let ast::Callee::Expr(callee) = &call.callee else {
            return false;
        };
        callee_name(callee).as_deref() == Some("Promise.race") && self.binding("Promise").is_err()
    }
    /// Whether `expression` is `await Promise.race(...)` under v38
    /// (`web_bytes`), which an assignment to a union types from its target.
    pub(super) fn awaits_race(&self, expression: &ast::Expr) -> bool {
        self.types.profile.web_bytes()
            && matches!(stmt::unparen(expression), ast::Expr::Await(awaited) if self.races(&awaited.arg))
    }
    /// `x as T` from a union to a union of some of its members (v38
    /// `web_bytes`, e.g. a `BodyInit` asserted to a `BlobPart`): the cell is
    /// tested against each member of `T` and kept as it is, or the assertion
    /// throws `TypeError(message)`. Members of either union may be classes,
    /// records, arrays, a `Uint8Array` or primitives; the cell representation
    /// of a member is the same in both unions.
    pub(super) fn union_downcast(
        &mut self,
        value: Expr,
        target: Type,
        message: &str,
    ) -> Result<std::result::Result<Expr, Expr>> {
        if !self.types.profile.web_bytes()
            || !value.ty.is_union()
            || !target.is_union()
            || !value.ty.is_cell()
            || !target.is_cell()
        {
            return Ok(Err(value));
        }
        let sources = unions::members(value.ty, self.types);
        let targets = unions::members(target, self.types);
        if !targets.iter().all(|member| sources.contains(member)) {
            return Ok(Err(value));
        }
        let span = value.span;
        let saved = self.hidden(value.ty);
        let output = self.hidden(target);
        let marker = self.hidden(Type::Number);
        let local = |slot: u32, ty: Type| Expr {
            span,
            ty,
            kind: E::Local(slot),
        };
        let mut test = Expr {
            span,
            ty: Type::Boolean,
            kind: E::Integer(0),
        };
        for member in targets.into_iter().rev() {
            let next = Expr {
                span,
                ty: Type::Boolean,
                kind: E::IsUnionMember(Box::new(local(saved, value.ty)), member),
            };
            test = Expr {
                span,
                ty: Type::Boolean,
                kind: E::Conditional(
                    Box::new(next),
                    Box::new(Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Integer(1),
                    }),
                    Box::new(test),
                ),
            };
        }
        let text = unions::intern(vec![Type::String, Type::Undefined], self.types)?;
        let text = unions::widen(
            Expr {
                span,
                ty: Type::String,
                kind: E::String(self.types.profile.string_bytes(message)),
            },
            Some(text),
            self.types,
        )?;
        let error = Expr {
            span,
            ty: Type::from_error_class(varta_common::value::ErrorClass::TypeError),
            kind: E::NewError(varta_common::value::ErrorClass::TypeError, Box::new(text)),
        };
        let thrown = self.exception_type()?;
        let throw = S::Throw(unions::widen(error, Some(thrown), self.types)?);
        let source = value.ty;
        Ok(Ok(Expr {
            span,
            ty: target,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![saved],
                body: vec![
                    S::Init(saved, value),
                    S::If(
                        Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::Not(Box::new(test)),
                        },
                        vec![throw],
                        vec![],
                    ),
                    S::Init(
                        output,
                        Expr {
                            span,
                            ty: target,
                            kind: E::Reinterpret(Box::new(local(saved, source))),
                        },
                    ),
                ],
            },
        }))
    }
    /// `o.p instanceof C ? o.p : other` (v38 `web_bytes`): the property of a
    /// record or dictionary read once, tested against the class member of its
    /// union and read narrowed in the consequent, as `typeof` tests do in
    /// [`Lower::narrowed_path_conditional`]. `None` when the conditional is
    /// not this idiom.
    pub(super) fn instanceof_path_conditional(
        &mut self,
        branch: &ast::CondExpr,
        expected: Option<Type>,
    ) -> Result<Option<Expr>> {
        if !self.types.profile.web_bytes() {
            return Ok(None);
        }
        let ast::Expr::Bin(test) = stmt::unparen(&branch.test) else {
            return Ok(None);
        };
        if test.op != ast::BinaryOp::InstanceOf {
            return Ok(None);
        }
        let ast::Expr::Member(member) = stmt::unparen(&test.left) else {
            return Ok(None);
        };
        if !declared_fields::same_path(&test.left, &branch.cons)
            || mixed_records::static_key(member).is_none()
        {
            return Ok(None);
        }
        let Some(class) = self.class_callee(&test.right) else {
            return Ok(None);
        };
        // The path is identifiers, `this` and static keys: lowering its object
        // here and again on the fallback has no effect.
        let base = self.expr(&member.obj)?;
        let value = match base.ty {
            Type::Record(_) => self.record_field(member, base, true)?,
            Type::Dictionary(_) => self.dictionary_read(member, base)?,
            _ => return Ok(None),
        };
        let target = Type::Class(class);
        let source = value.ty;
        if !source.is_cell() || !unions::members(source, self.types).contains(&target) {
            return Ok(None);
        }
        let span = branch.span;
        let name = self.types.classes.get(class).name.clone();
        let saved = self.hidden(source);
        let check = self.cell_test(span, saved, source, target, &name)?;
        let proven = Expr {
            span,
            ty: target,
            kind: E::UnboxUnion(Box::new(Expr {
                span,
                ty: source,
                kind: E::Local(saved),
            })),
        };
        let entry = self.flow_snapshot();
        let other = self.expr_contextual(&branch.alt, expected, None)?;
        let after = self.flow_snapshot();
        self.flow_join(&entry, &[entry.clone(), after])?;
        let (ty, proven, other) = if proven.ty == other.ty {
            (proven.ty, proven, other)
        } else {
            self.join(proven, other)?
        };
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        Ok(Some(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![saved],
                body: vec![
                    S::Init(saved, value),
                    S::Init(
                        output,
                        Expr {
                            span,
                            ty,
                            kind: E::Conditional(
                                Box::new(check),
                                Box::new(proven),
                                Box::new(other),
                            ),
                        },
                    ),
                ],
            },
        }))
    }
    /// `f as F` (or `F | undefined`) of a named function `f` whose
    /// parameters accept `F`'s and whose result `F` accepts (v38
    /// `web_bytes`, e.g. `globalThis.fetch as FetchLike | undefined`): a
    /// synthesized adapter of `F`'s signature that calls `f` with its
    /// arguments widened to `f`'s parameter types (missing optional
    /// parameters are `undefined`). The adapter is a fresh function value, so
    /// it is not `===` to `f`.
    pub(super) fn function_adapter(
        &mut self,
        value: Expr,
        target: Type,
    ) -> Result<std::result::Result<Expr, Expr>> {
        let (
            Type::Function(source),
            E::FunctionRef {
                index, captures, ..
            },
        ) = (value.ty, &value.kind)
        else {
            return Ok(Err(value));
        };
        if !self.types.profile.web_bytes() || !captures.is_empty() {
            return Ok(Err(value));
        }
        let callee = *index;
        let wanted = unions::members(target, self.types)
            .into_iter()
            .filter(|member| *member != Type::Undefined)
            .collect::<Vec<_>>();
        let [Type::Function(wanted)] = wanted.as_slice() else {
            return Ok(Err(value));
        };
        let wanted = *wanted;
        let from = self.types.functions[source as usize].clone();
        let into = self.types.functions[wanted as usize].clone();
        if from.receiver.is_some()
            || into.receiver.is_some()
            || from.shape.rest
            || into.shape.rest
            || into.shape.optional.iter().any(|optional| *optional)
            || into.params.len() > from.params.len()
            || from.params[into.params.len()..]
                .iter()
                .zip(&from.shape.optional[into.params.len()..])
                .any(|(_, optional)| !optional)
            || !into
                .params
                .iter()
                .zip(&from.params)
                .all(|(given, taken)| arguments::accepts(*taken, *given, self.types))
            || !(into.result == from.result
                || arguments::accepts(into.result, from.result, self.types))
        {
            return Ok(Err(value));
        }
        let span = value.span;
        let adapter = self.reserve_function("too many functions including function adapters")?;
        let params = into.params.clone();
        let result = into.result;
        self.define_function(adapter, span, params.clone(), result, |child, args| {
            let mut forwarded = Vec::new();
            for (position, taken) in from.params.iter().enumerate() {
                let argument = match args.get(position) {
                    Some(given) => given.clone(),
                    None => Expr {
                        span,
                        ty: Type::Undefined,
                        kind: E::Integer(0),
                    },
                };
                forwarded.push(unions::widen(argument, Some(*taken), child.types)?);
            }
            let call = Expr {
                span,
                ty: from.result,
                kind: E::Call(callee, forwarded),
            };
            let call = unions::widen(call, Some(result), child.types)?;
            Ok(vec![S::Return(Some(call))])
        })?;
        let signature = Signature {
            shape: ir::ParameterShape::fixed(params.len()),
            receiver: None,
            index: adapter,
            names: vec![None; params.len()],
            params,
            result,
        };
        let reference = self.function_reference(&signature, span)?;
        unions::widen(reference, Some(target), self.types).map(Ok)
    }
    /// `p.env.NAME` or `p.env['NAME']` of a `Process` (v38 `web_bytes`): one
    /// `env.get` of the static key when it runs, so a wildcard `env.get`
    /// grant reads the variable by name without enumerating the environment.
    /// The receiver path has no calls (lowered here and again otherwise).
    pub(super) fn environment_read(&mut self, member: &ast::MemberExpr) -> Result<Option<Expr>> {
        if !self.types.profile.web_bytes() {
            return Ok(None);
        }
        let Some(name) = mixed_records::static_key(member) else {
            return Ok(None);
        };
        let ast::Expr::Member(env) = stmt::unparen(&member.obj) else {
            return Ok(None);
        };
        if !matches!(&env.prop, ast::MemberProp::Ident(id) if id.sym == "env") {
            return Ok(None);
        }
        let mut root = stmt::unparen(&env.obj);
        while let ast::Expr::Member(inner) = root {
            if mixed_records::static_key(inner).is_none() {
                return Ok(None);
            }
            root = stmt::unparen(&inner.obj);
        }
        if !matches!(root, ast::Expr::Ident(_) | ast::Expr::This(_))
            || !self.class_late_receiver(env)
        {
            return Ok(None);
        }
        let receiver = self.expr(&env.obj)?;
        let Type::Class(class) = receiver.ty else {
            return Ok(None);
        };
        let info = self.types.classes.get(class);
        if !info.prelude || info.name != PROCESS_CLASS {
            return Ok(None);
        }
        let span = member.span;
        let key = Expr {
            span,
            ty: Type::String,
            kind: E::String(self.types.profile.string_bytes(&name)),
        };
        let read = self.engine_apply((&GLOBAL, ENVIRONMENT_VALUE, &[]), vec![key], span)?;
        Ok(Some(Expr {
            span,
            ty: read.ty,
            kind: E::Sequence(Box::new(receiver), Box::new(read)),
        }))
    }
}
