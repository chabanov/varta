//! Lexical declarations predeclare all leaves before evaluating any initializer.
use super::*;
impl Lower<'_> {
    fn binding_annotation(&mut self, pattern: &ast::Pat) -> Result<Option<Type>> {
        let ann = match pattern {
            ast::Pat::Ident(p) => p.type_ann.as_deref(),
            ast::Pat::Object(p) => p.type_ann.as_deref(),
            ast::Pat::Array(p) => p.type_ann.as_deref(),
            _ => {
                return Err(self
                    .source
                    .locate(unsupported("lexical binding pattern"), pattern.span()));
            }
        };
        annotation(ann, self.types).map_err(|e| {
            self.source
                .locate(e, ann.map_or(pattern.span(), |a| a.type_ann.span()))
        })
    }
    pub(super) fn predeclare(&mut self, declaration: &ast::VarDecl) -> Result<()> {
        if declaration.declare || declaration.kind == ast::VarDeclKind::Var {
            return Err(unsupported("var or ambient local"));
        }
        let patterns: Vec<_> = declaration.decls.iter().map(|v| v.name.clone()).collect();
        // One shared budget covers the declaration, including names in separate
        // declarators. Names remain in TDZ until their own value is ready.
        patterns::names(&patterns).map_err(|(e, span)| self.source.locate(e, span))?;
        for variable in &declaration.decls {
            let ty = self.binding_annotation(&variable.name)?;
            let mutable = declaration.kind == ast::VarDeclKind::Let;
            if let ast::Pat::Ident(id) = &variable.name {
                self.declare(id.id.sym.as_ref(), ty, false, mutable)
                    .map_err(|e| self.source.locate(e, id.span()))?;
            } else {
                if !self.types.profile.closures() {
                    return Err(self.source.locate(
                        unsupported("local destructuring requires checked-f64-utf16-v12"),
                        variable.name.span(),
                    ));
                }
                for (name, span) in patterns::names(std::slice::from_ref(&variable.name))
                    .map_err(|(e, span)| self.source.locate(e, span))?
                {
                    self.declare(&name, None, false, mutable)
                        .map_err(|e| self.source.locate(e, span))?;
                }
            }
        }
        Ok(())
    }
    pub(super) fn variables(&mut self, declaration: &ast::VarDecl) -> Result<Vec<S>> {
        let mut output = Vec::new();
        for variable in &declaration.decls {
            let ast::Pat::Ident(binding) = &variable.name else {
                let init = variable.init.as_deref().ok_or_else(|| {
                    self.source.locate(
                        unsupported("lexical destructuring without an initializer"),
                        variable.name.span(),
                    )
                })?;
                let expected = self.binding_annotation(&variable.name)?;
                let value = self.expr_expected(init, expected)?;
                if let Some(expected) = expected
                    && value.ty != expected
                {
                    return Err(self.source.locate(
                        type_error(format!(
                            "binding initializer mismatch: expected {expected}, found {}",
                            value.ty
                        )),
                        value.span,
                    ));
                }
                output.extend(self.initialize_binding_pattern(
                    &variable.name,
                    value,
                    None,
                    None,
                )?);
                continue;
            };
            let name = binding.id.sym.as_ref();
            let old = self.binding(name)?;
            let (init, readonly) = match variable.init.as_deref() {
                Some(init) => {
                    let (init, readonly) = self.const_assertion(init);
                    (Some(init), readonly)
                }
                None => (None, false),
            };
            let value = if let Some(init) = init {
                if old.ty.is_none() && self.types.profile.literal_types() {
                    // An inferred binding keeps a literal-typed initializer's type.
                    self.expr_preserving(init)?
                } else {
                    self.expr_expected(init, old.ty)?
                }
            } else if self.types.profile.missing()
                && declaration.kind == ast::VarDeclKind::Let
                && old.ty.is_some_and(|ty| {
                    unions::members(ty, self.types).contains(&Type::Undefined)
                        // `let x: unknown;` (v36) starts as `undefined`, as in TypeScript.
                        || ty == Type::Unknown && self.types.profile.checked_declared_fields()
                })
            {
                unions::widen(
                    Expr {
                        span: variable.span,
                        ty: Type::Undefined,
                        kind: E::Integer(0),
                    },
                    old.ty,
                    self.types,
                )?
            } else if self.types.profile.language_core()
                && declaration.kind == ast::VarDeclKind::Let
                && let Some(ty) = old.ty
            {
                output.push(self.deferred(name, &old, ty, variable.span)?);
                continue;
            } else {
                return Err(unsupported(
                    "uninitialized variable without an undefined-capable let annotation",
                ));
            };
            value_type(value.ty).map_err(|e| self.source.locate(e, value.span))?;
            if old.ty.is_some_and(|ty| ty != value.ty) {
                let (expected, found) = self.mismatch(old.ty.unwrap_or(Type::Void), value.ty);
                return Err(self.source.locate(
                    type_error(format!(
                        "type mismatch for {name}: expected {expected}, found {found}"
                    )),
                    value.span,
                ));
            }
            self.update(name, value.ty)?;
            self.mark_const_assertion(name, readonly)?;
            self.flow_known_binding(&old, &value);
            let never = value.ty == Type::Never;
            output.extend(self.initialize(&old, value));
            // A `never` binding (v37) is unreachable: fail safe if reached.
            if never {
                output.push(S::Trap);
            }
        }
        Ok(output)
    }
}
