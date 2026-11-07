//! Assignments through class setters (v38 `declaration_forms`). A setter
//! `set x(v)` is the method [`forms::setter_key`]`("x")`; an assignment
//! `o.x = v` evaluates `o`, then `v`, then calls the setter with them and
//! yields `v`. A compound assignment or update reads through the getter of
//! the pair first, as ECMAScript's `GetValue`/`PutValue` do. The receiver
//! is held in a rooted local across the right-hand side, which may
//! allocate.
use super::*;

struct Accessor {
    name: String,
    setter: Method,
    getter: Option<Method>,
}

fn local(span: Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}

impl Lower<'_> {
    /// The setter `member` assigns through, if the receiver is a class
    /// instance whose class declares one.
    fn accessor(&mut self, member: &ast::MemberExpr, base: &Expr) -> Result<Option<Accessor>> {
        if !self.types.profile.declaration_forms() {
            return Ok(None);
        }
        let Type::Class(class) = base.ty else {
            return Ok(None);
        };
        self.class_functions()?;
        let Ok(name) = self.member_key(&member.prop) else {
            return Ok(None);
        };
        let info = self.types.classes.get(class);
        let Some(setter) = info.methods.get(&forms::setter_key(&name)).cloned() else {
            return Ok(None);
        };
        let getter = info.methods.get(&name).filter(|m| m.getter).cloned();
        self.check_access(
            setter.owner,
            setter.access,
            class,
            &name,
            member.prop.span(),
        )?;
        Ok(Some(Accessor {
            name,
            setter,
            getter,
        }))
    }
    /// `o.x = v` through a setter; `Err(base)` hands back a receiver whose
    /// member is not a setter.
    pub(in crate::lower) fn accessor_assignment(
        &mut self,
        member: &ast::MemberExpr,
        base: Expr,
        right: &ast::Expr,
        span: Span,
    ) -> Result<std::result::Result<Expr, Expr>> {
        // A static field through a class value (`static_store`).
        if let Some(field) = self.static_field(member, &base)? {
            return self.static_assignment(field, base, right, span).map(Ok);
        }
        let Some(accessor) = self.accessor(member, &base)? else {
            return Ok(Err(base));
        };
        let param = self.setter_parameter(&accessor)?;
        let value = self.expr_expected(right, Some(param))?;
        self.accessor_store(&accessor, base, value, span).map(Ok)
    }
    /// `o.x op= v` through a getter/setter pair.
    pub(in crate::lower) fn accessor_compound(
        &mut self,
        member: &ast::MemberExpr,
        base: Expr,
        right: &ast::Expr,
        op: ast::BinaryOp,
        span: Span,
    ) -> Result<std::result::Result<Expr, Expr>> {
        if let Some(field) = self.static_field(member, &base)? {
            return self.static_compound(field, base, right, op, span).map(Ok);
        }
        let Some(accessor) = self.accessor(member, &base)? else {
            return Ok(Err(base));
        };
        let receiver = self.hidden(base.ty);
        let old = self.getter_read(&accessor, receiver, span)?;
        let value = self.compound_operation(old, right, op, span)?;
        self.accessor_store_at(&accessor, receiver, base, value, None, span)
            .map(Ok)
    }
    /// `o.x++` and friends through a getter/setter pair.
    pub(in crate::lower) fn accessor_update(
        &mut self,
        member: &ast::MemberExpr,
        base: Expr,
        update: &ast::UpdateExpr,
    ) -> Result<std::result::Result<Expr, Expr>> {
        if let Some(field) = self.static_field(member, &base)? {
            return self.static_update(field, base, update).map(Ok);
        }
        let Some(accessor) = self.accessor(member, &base)? else {
            return Ok(Err(base));
        };
        let span = update.span;
        let receiver = self.hidden(base.ty);
        let old = self.getter_read(&accessor, receiver, span)?;
        let old = self.checked_update_numeric(old)?;
        let old_ty = old.ty;
        let previous = self.hidden(old_ty);
        let next = self.update_next(span, previous, old_ty, update.op)?;
        let prefix = update.prefix;
        let first = (previous, old);
        self.accessor_store_at(
            &accessor,
            receiver,
            base,
            next,
            Some((first, !prefix)),
            span,
        )
        .map(Ok)
    }
    /// A logical assignment of a static field through a class value
    /// (`static_store`); one through a setter is not admitted.
    pub(in crate::lower) fn accessor_logical(
        &mut self,
        member: &ast::MemberExpr,
        base: Expr,
        right: &ast::Expr,
        op: ast::AssignOp,
        span: Span,
    ) -> Result<std::result::Result<Expr, Expr>> {
        if let Some(field) = self.static_field(member, &base)? {
            return self.static_logical(field, base, right, op, span).map(Ok);
        }
        if self.accessor(member, &base)?.is_some() {
            return Err(self.source.locate(
                unsupported("logical assignment to a class accessor"),
                member.span,
            ));
        }
        Ok(Err(base))
    }
    fn setter_parameter(&self, accessor: &Accessor) -> Result<Type> {
        match accessor.setter.signature.params.as_slice() {
            [param] if !accessor.setter.signature.shape.extended() => Ok(*param),
            _ => Err(unsupported(
                "class setter without exactly one plain parameter",
            )),
        }
    }
    fn getter_read(&mut self, accessor: &Accessor, receiver: u32, span: Span) -> Result<Expr> {
        let Some(getter) = &accessor.getter else {
            return Err(self.source.locate(
                unsupported(&format!(
                    "compound assignment or update of the setter-only accessor {}",
                    accessor.name
                )),
                span,
            ));
        };
        let (target, owner) = getter.target();
        self.drop_paths();
        Ok(Expr {
            span,
            ty: getter.signature.result,
            kind: E::Call(target, vec![local(span, receiver, Type::Class(owner))]),
        })
    }
    fn accessor_store(
        &mut self,
        accessor: &Accessor,
        base: Expr,
        value: Expr,
        span: Span,
    ) -> Result<Expr> {
        let receiver = self.hidden(base.ty);
        self.accessor_store_at(accessor, receiver, base, value, None, span)
    }
    /// Evaluate the receiver into `receiver` (and, for an update, the old
    /// value into its local), then the value, call the setter and yield the
    /// value, or the old value when `previous.1` is set.
    fn accessor_store_at(
        &mut self,
        accessor: &Accessor,
        receiver: u32,
        base: Expr,
        value: Expr,
        previous: Option<((u32, Expr), bool)>,
        span: Span,
    ) -> Result<Expr> {
        let param = self.setter_parameter(accessor)?;
        let value = unions::widen(value, Some(param), self.types)?;
        if value.ty != param {
            let (expected, found) = self.mismatch(param, value.ty);
            return Err(self.source.locate(
                type_error(format!(
                    "setter {} expects {expected}, found {found}",
                    accessor.name
                )),
                value.span,
            ));
        }
        let output = self.hidden(param);
        let marker = self.hidden(Type::Number);
        let (target, owner) = accessor.setter.target();
        let mut body = vec![S::Init(receiver, base)];
        let mut roots = vec![receiver, output];
        let mut result = output;
        let mut old_ty = None;
        if let Some(((slot, old), postfix)) = previous {
            old_ty = Some(old.ty);
            body.push(S::Init(slot, old));
            roots.push(slot);
            if postfix {
                result = slot;
            }
        }
        body.push(S::Init(output, value));
        body.push(S::Expr(Expr {
            span,
            ty: Type::Void,
            kind: E::Call(
                target,
                vec![
                    local(span, receiver, Type::Class(owner)),
                    local(span, output, param),
                ],
            ),
        }));
        // A setter runs code (v37 `paths`).
        self.drop_paths();
        if result == output {
            return Ok(Expr {
                span,
                ty: param,
                kind: E::ValueBlock {
                    output,
                    marker,
                    roots,
                    body,
                },
            });
        }
        // A postfix update yields the old value.
        let ty = old_ty.unwrap_or(param);
        let yielded = self.hidden(ty);
        body.push(S::Init(yielded, local(span, result, ty)));
        roots.push(yielded);
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output: yielded,
                marker,
                roots,
                body,
            },
        })
    }
}
