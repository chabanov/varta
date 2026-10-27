//! Prepared references retain receiver/key before reads, defaults and rest copying.
use super::*;
pub(super) struct Target {
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
    pub(super) fn prepare_pattern_target(&mut self, pattern: &ast::Pat) -> Result<Option<Target>> {
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
                    .locate(unsupported("immutable destructuring member target"), span));
            }
        };
        Ok(Some(Target {
            ty: field.ty,
            span,
            kind,
            roots,
            body,
        }))
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
