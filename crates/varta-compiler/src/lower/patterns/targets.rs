//! Prepared references retain receiver/key before reads, defaults and rest copying.
use super::*;
pub(in crate::lower) struct Target {
    ty: Type,
    span: swc_common::Span,
    kind: Place,
    roots: Vec<u32>,
    body: Vec<S>,
}
enum Place {
    Local(u32),
    Field { base: u32, ty: Type, index: u32 },
    Index { base: u32, ty: Type, key: u32 },
    Length { base: u32, ty: Type },
}
impl Lower<'_> {
    pub(in crate::lower) fn prepare_pattern_target(
        &mut self,
        pattern: &ast::Pat,
    ) -> Result<Option<Target>> {
        let pattern = if let ast::Pat::Assign(assign) = pattern {
            &*assign.left
        } else {
            pattern
        };
        let span = pattern.span();
        if let ast::Pat::Ident(id) = pattern {
            let binding = self.assignment_binding(id.id.sym.as_ref(), span)?;
            return Ok(Some(Target {
                ty: binding
                    .ty
                    .ok_or_else(|| invalid("missing assignment target type"))?,
                span,
                kind: Place::Local(binding.slot),
                roots: vec![],
                body: vec![],
            }));
        }
        let ast::Pat::Expr(expr) = pattern else {
            return Ok(None);
        };
        let ast::Expr::Member(member) = &**expr else {
            return Err(self
                .source
                .locate(unsupported("non-member destructuring target"), span));
        };
        let field = self
            .place(member)
            .map_err(|e| self.source.locate(e, span))?;
        Ok(Some(self.prepare_member_reference(field, span)?))
    }
    pub(in crate::lower) fn prepare_member_reference(
        &mut self,
        field: Expr,
        span: swc_common::Span,
    ) -> Result<Target> {
        let mut roots = Vec::new();
        let mut body = Vec::new();
        let mut save = |this: &mut Lower<'_>, value: Expr| {
            let ty = value.ty;
            let slot = this.hidden(ty);
            roots.push(slot);
            body.push(S::Init(slot, value));
            (slot, ty)
        };
        let kind = match field.kind {
            E::Field(base, index) => {
                let (base, ty) = save(self, *base);
                Place::Field { base, ty, index }
            }
            E::Index(base, key) => {
                let (base, ty) = save(self, *base);
                let (key, _) = save(self, *key);
                Place::Index { base, ty, key }
            }
            E::Length(base) if self.types.profile.array_mutations() => {
                let (base, ty) = save(self, *base);
                Place::Length { base, ty }
            }
            _ => {
                return Err(self
                    .source
                    .locate(unsupported("immutable assignment member target"), span));
            }
        };
        Ok(Target {
            ty: field.ty,
            span,
            kind,
            roots,
            body,
        })
    }
    pub(in crate::lower) fn prepared_assignment_value(
        &mut self,
        target: Target,
        rhs: &ast::Expr,
        span: swc_common::Span,
    ) -> Result<Expr> {
        // Non-union context can infer arrows and aggregate literals without
        // widening a scalar result. Union context may select one aggregate
        // literal member, but never pre-widens the original RHS to the target.
        let expression = stmt::unparen(rhs);
        let context = if !target.ty.is_union() {
            Some(target.ty)
        } else if matches!(expression, ast::Expr::Object(_) | ast::Expr::Array(_)) {
            self.context(expression, Some(target.ty))
        } else {
            None
        };
        let value = self.expr_expected(rhs, context)?;
        self.finish_prepared_value(target, value, span)
    }
    pub(in crate::lower) fn prepared_compound_value(
        &mut self,
        target: Target,
        rhs: &ast::Expr,
        op: ast::BinaryOp,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let read_span = target.span;
        let read = match target.kind {
            Place::Local(slot) => local(read_span, slot, target.ty),
            Place::Field { base, ty, index } => Expr {
                span: read_span,
                ty: target.ty,
                kind: E::Field(Box::new(local(span, base, ty)), index),
            },
            Place::Index { base, ty, key } => Expr {
                span: read_span,
                ty: unions::intern(vec![target.ty, Type::Undefined], self.types)?,
                kind: E::MaybeIndex(
                    Box::new(local(span, base, ty)),
                    Box::new(local(span, key, Type::Float64)),
                ),
            },
            Place::Length { base, ty } => Expr {
                span: read_span,
                ty: self.types.number(),
                kind: E::Length(Box::new(local(span, base, ty))),
            },
        };
        let value = self.compound_operation(read, rhs, op, span)?;
        self.finish_prepared_value(target, value, span)
    }
    fn finish_prepared_value(
        &mut self,
        mut target: Target,
        value: Expr,
        span: swc_common::Span,
    ) -> Result<Expr> {
        if !arguments::accepts(target.ty, value.ty, self.types) {
            return Err(self.source.locate(
                type_error(format!(
                    "assignment type mismatch: expected {}, found {}",
                    target.ty, value.ty
                )),
                value.span,
            ));
        }
        let ty = value.ty;
        let input = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        target.body.push(S::Init(input, value));
        let store = self.store_pattern_target(
            &target,
            unions::widen(local(span, input, ty), Some(target.ty), self.types)?,
        )?;
        target.body.push(store);
        target.roots.push(input);
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output: input,
                marker,
                roots: target.roots,
                body: target.body,
            },
        })
    }
    pub(super) fn finish_pattern_target(
        &mut self,
        mut target: Target,
        mut value: Expr,
        default: Option<&ast::Expr>,
        mut prefix: Vec<S>,
    ) -> Result<Vec<S>> {
        let mut body = std::mem::take(&mut target.body);
        body.append(&mut prefix);
        let mut roots = std::mem::take(&mut target.roots);
        if let Some(default) = default {
            let (resolved, staged_roots, staged_body) =
                self.pattern_default_value(value, default, Some(target.ty))?;
            roots.extend(staged_roots);
            body.extend(staged_body);
            value = resolved;
        }
        body.extend(self.assign_prepared_target(&target, value)?);
        Ok(vec![S::Scope(roots, body)])
    }
    fn assign_prepared_target(&mut self, target: &Target, value: Expr) -> Result<Vec<S>> {
        let expected = target.ty;
        if arguments::accepts(expected, value.ty, self.types) {
            return Ok(vec![self.store_pattern_target(
                target,
                unions::widen(value, Some(expected), self.types)?,
            )?]);
        }
        let members = unions::members(value.ty, self.types);
        let present: Vec<_> = members
            .iter()
            .copied()
            .filter(|ty| *ty != Type::Undefined)
            .collect();
        if value.ty.is_union() && members.contains(&Type::Undefined) && !present.is_empty() {
            let ty = unions::intern(present, self.types)?;
            if arguments::accepts(expected, ty, self.types) {
                let span = value.span;
                let input = self.hidden(value.ty);
                let input_ty = value.ty;
                let test = Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsUndefined(Box::new(local(span, input, input_ty))),
                };
                let present = if ty.is_union() {
                    local(span, input, ty)
                } else {
                    Expr {
                        span,
                        ty,
                        kind: E::UnboxUnion(Box::new(local(span, input, input_ty))),
                    }
                };
                let error = self.array_error(
                    varta_common::value::ErrorClass::TypeError,
                    "Invalid destructuring assignment value",
                    span,
                )?;
                let store = self.store_pattern_target(
                    target,
                    unions::widen(present, Some(expected), self.types)?,
                )?;
                return Ok(vec![S::Scope(
                    vec![input],
                    vec![
                        S::Init(input, value),
                        S::If(test, vec![S::Throw(error)], vec![store]),
                    ],
                )]);
            }
        }
        Err(self.source.locate(
            type_error(format!(
                "destructuring assignment type mismatch: expected {expected}, found {}",
                value.ty
            )),
            target.span,
        ))
    }
    fn store_pattern_target(&mut self, target: &Target, value: Expr) -> Result<S> {
        let span = target.span;
        Ok(match target.kind {
            Place::Local(slot) => S::Set(slot, value),
            Place::Field { base, ty, index } => S::SetField(local(span, base, ty), index, value),
            Place::Index { base, ty, key } => S::SetIndex {
                array: local(span, base, ty),
                index: local(span, key, Type::Float64),
                value,
                slots: self.types.profile.sparse_arrays().then(|| {
                    (
                        self.hidden(ty),
                        self.hidden(if self.types.profile.array_mutations() {
                            Type::Float64
                        } else {
                            Type::Number
                        }),
                    )
                }),
            },
            Place::Length { base, ty } => S::ArrayLength {
                array: local(span, base, ty),
                length: value,
                error: self.array_error(
                    varta_common::value::ErrorClass::RangeError,
                    "Invalid array length",
                    span,
                )?,
            },
        })
    }
}
