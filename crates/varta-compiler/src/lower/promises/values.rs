//! Promise values (v35 `promise_values`): promises tagged `ValueKind::Promise`
//! inside unions, `unknown`, arrays, records and dictionaries. A cell that may
//! hold a promise is awaited or resolved by its tag: a promise member is used
//! itself (PromiseResolve returns it), any other member is fulfilled into a new
//! promise. A promise held by `unknown` has no static `T` and is never adopted
//! silently: awaiting or fulfilling with it throws a `TypeError`.
use super::*;

/// Message of the `TypeError` thrown instead of adopting a promise whose
/// value type is unknown.
const UNKNOWN_PROMISE: &str =
    "a promise held by unknown cannot be adopted; keep it typed as Promise<T>";

impl Lower<'_> {
    /// The promise member of a union that may hold one, and its only other
    /// member. `None` when the type holds no promise.
    pub(super) fn promise_member(
        &self,
        ty: Type,
        span: swc_common::Span,
    ) -> Result<Option<(Type, Type)>> {
        if !self.types.profile.promise_values() || !ty.is_union() {
            return Ok(None);
        }
        let members = unions::members(ty, self.types);
        let Some(&promise) = members.iter().find(|m| matches!(m, Type::Promise(_))) else {
            return Ok(None);
        };
        let rest: Vec<Type> = members
            .iter()
            .copied()
            .filter(|m| !matches!(m, Type::Promise(_)))
            .collect();
        let [rest] = rest[..] else {
            return Err(self.source.locate(
                rejected(
                    "awaiting or resolving a union of a promise and several other members; \
                     narrow it first",
                ),
                span,
            ));
        };
        Ok(Some((promise, rest)))
    }
    /// Evaluate `value` once into a hidden local; the block's value is unused.
    fn bind_once(&mut self, value: Expr) -> (Expr, u32) {
        let span = value.span;
        let ty = value.ty;
        let slot = self.hidden(ty);
        let block = Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output: slot,
                marker: self.hidden(Type::Number),
                roots: vec![],
                body: vec![S::Init(slot, value)],
            },
        };
        (block, slot)
    }
    /// `await` of a union that may hold a promise: a promise member is
    /// awaited itself, the other member through a fulfilled promise, so each
    /// takes exactly one job as in ECMAScript.
    pub(super) fn await_union(
        &mut self,
        span: swc_common::Span,
        value: Expr,
    ) -> Result<Option<Expr>> {
        let Some((promise, rest)) = self.promise_member(value.ty, value.span)? else {
            return Ok(None);
        };
        let Type::Promise(id) = promise else {
            return Err(CompilerError::Internal("missing promise member".into()));
        };
        let fulfilled = stored(self.types.promises[id as usize].value);
        let result = unions::intern(vec![fulfilled, rest], self.types)
            .map_err(|error| self.source.locate(error, span))?;
        let cell_ty = value.ty;
        let (bind, cell) = self.bind_once(value);
        let read = |ty: Type| Expr {
            span,
            ty,
            kind: E::UnboxUnion(Box::new(local(span, cell, cell_ty))),
        };
        let test = Expr {
            span,
            ty: Type::Boolean,
            kind: E::IsUnionMember(Box::new(local(span, cell, cell_ty)), promise),
        };
        let own = self.await_promise(span, read(promise))?;
        let own = unions::widen(own, Some(result), self.types)?;
        let rest_promise = match intern(rest, self.types) {
            Ok(Type::Promise(rest_id)) => self.promise_object(span, rest_id, Some(read(rest)))?,
            Ok(_) => return Err(CompilerError::Internal("missing promise".into())),
            Err(error) => return Err(self.source.locate(error, span)),
        };
        let other = self.await_promise(span, rest_promise)?;
        let other = unions::widen(other, Some(result), self.types)?;
        Ok(Some(Expr {
            span,
            ty: result,
            kind: E::Sequence(
                Box::new(bind),
                Box::new(Expr {
                    span,
                    ty: result,
                    kind: E::Conditional(Box::new(test), Box::new(own), Box::new(other)),
                }),
            ),
        }))
    }
    /// `Promise.resolve(v)` of a union that may hold a promise of the same
    /// `T` as its other member: the promise itself, or a fulfilled one.
    pub(super) fn resolve_union(
        &mut self,
        span: swc_common::Span,
        value: Expr,
        target: Option<Type>,
    ) -> Result<Option<Expr>> {
        let Some((promise, rest)) = self.promise_member(value.ty, value.span)? else {
            return Ok(None);
        };
        let Type::Promise(id) = promise else {
            return Err(CompilerError::Internal("missing promise member".into()));
        };
        let fulfilled = stored(self.types.promises[id as usize].value);
        if target.is_some_and(|t| intern(t, self.types).ok() != Some(promise))
            || !arguments::accepts(fulfilled, rest, self.types)
        {
            return Err(self.source.locate(
                type_error(format!(
                    "Promise.resolve of {} requires its other member to be a {fulfilled}",
                    value.ty
                )),
                value.span,
            ));
        }
        let cell_ty = value.ty;
        let (bind, cell) = self.bind_once(value);
        let read = |ty: Type| Expr {
            span,
            ty,
            kind: E::UnboxUnion(Box::new(local(span, cell, cell_ty))),
        };
        let test = Expr {
            span,
            ty: Type::Boolean,
            kind: E::IsUnionMember(Box::new(local(span, cell, cell_ty)), promise),
        };
        let other = unions::widen(read(rest), Some(fulfilled), self.types)?;
        let other = self.promise_object(span, id, Some(other))?;
        Ok(Some(Expr {
            span,
            ty: promise,
            kind: E::Sequence(
                Box::new(bind),
                Box::new(Expr {
                    span,
                    ty: promise,
                    kind: E::Conditional(Box::new(test), Box::new(read(promise)), Box::new(other)),
                }),
            ),
        }))
    }
    /// Statements that throw the adoption `TypeError` when the `unknown` in
    /// `slot` holds a promise (v35); nothing for other types and profiles.
    pub(super) fn unknown_promise_guard(
        &mut self,
        span: swc_common::Span,
        slot: u32,
        ty: Type,
    ) -> Result<Vec<S>> {
        if !self.types.profile.promise_values() || ty != Type::Unknown {
            return Ok(Vec::new());
        }
        // Every promise shares the one tag; any promise type names it.
        let any = intern(Type::Void, self.types)?;
        let error = self.array_error(ErrorClass::TypeError, UNKNOWN_PROMISE, span)?;
        Ok(vec![S::If(
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::IsUnionMember(Box::new(local(span, slot, ty)), any),
            },
            vec![S::Throw(error)],
            vec![],
        )])
    }
    /// `value` (an `unknown`) checked by [`Self::unknown_promise_guard`].
    pub(super) fn guarded_unknown(&mut self, value: Expr) -> Result<Expr> {
        if !self.types.profile.promise_values() || value.ty != Type::Unknown {
            return Ok(value);
        }
        let span = value.span;
        let slot = self.hidden(Type::Unknown);
        let mut body = vec![S::Init(slot, value)];
        body.extend(self.unknown_promise_guard(span, slot, Type::Unknown)?);
        Ok(Expr {
            span,
            ty: Type::Unknown,
            kind: E::ValueBlock {
                output: slot,
                marker: self.hidden(Type::Number),
                roots: vec![slot],
                body,
            },
        })
    }
}
