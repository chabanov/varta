use super::*;
/// The catch-binding union: every value the exception channel can carry, and
/// the type of a frame's exception completion. With v32 Error subclasses it
/// also holds the Error root class, which every source error class extends.
pub(super) fn exception_union(types: &mut ir::ValueTypes) -> Result<Type> {
    let mut members = vec![
        Type::Float64,
        Type::Boolean,
        Type::String,
        Type::Null,
        Type::Undefined,
    ];
    if types.profile.bigint_values() {
        members.push(Type::BigInt);
    }
    if types.profile.errors() {
        members.extend(errors::classes(types.profile).map(Type::from_error_class));
    }
    if let Some(root) = types
        .classes
        .error_root(varta_common::value::ErrorClass::Error)
    {
        members.push(Type::Class(root));
    }
    unions::intern(members, types)
}
impl Lower<'_> {
    pub(super) fn exception_type(&mut self) -> Result<Type> {
        exception_union(self.types)
    }
    pub(super) fn throw_statement(&mut self, statement: &ast::ThrowStmt) -> Result<S> {
        if !self.types.profile.exceptions() {
            return Err(unsupported("throw requires checked-f64-utf16-v7"));
        }
        let value = self.expr(&statement.arg)?;
        Ok(S::Throw(self.thrown(value)?))
    }
    /// Check a thrown value and widen it to the exception-channel union.
    pub(super) fn thrown(&mut self, value: Expr) -> Result<Expr> {
        if unions::members(value.ty, self.types).iter().any(|ty| {
            (!unions::primitive(*ty)
                && !(self.types.profile.bigint_values() && *ty == Type::BigInt))
                && !(self.types.profile.errors() && ty.is_error())
                && !matches!(ty, Type::Class(class) if self.types.classes.is_error(*class))
        }) {
            return Err(self.source.locate(
                unsupported("exception value other than primitives or supported Error"),
                value.span,
            ));
        }
        let ty = self.exception_type()?;
        unions::widen(value, Some(ty), self.types)
    }
    pub(super) fn try_statement(&mut self, statement: &ast::TryStmt) -> Result<S> {
        if !self.types.profile.exceptions() {
            return Err(unsupported("try/catch requires checked-f64-utf16-v7"));
        }
        let entry = self.flow_snapshot();
        let exits = self.flow_exit_mark();
        let mut body = self.block(&statement.block.stmts, true)?;
        let mut paths = vec![];
        if completion::normal(&body) {
            paths.push(self.flow_snapshot());
        }
        if let Some(catch) = &statement.handler {
            if self.types.profile.refinement_writes() {
                // A throw can precede any store in the try body. The handler
                // starts with entry facts minus every potentially changed slot.
                self.flow_alternate(&entry);
                self.flow_block_effects(&statement.block);
            }
            body = vec![self.catch_statement(body, catch)?];
            // Treat handler completion conservatively; its checked facts are
            // still valid whenever it reaches the following statement.
            paths.push(self.flow_snapshot());
        }
        if let Some(finalizer) = &statement.finalizer {
            let pending = self.flow_exit_mark();
            if self.types.profile.refinement_writes() {
                self.flow_alternate(&entry);
                self.flow_try_prefix(statement);
            }
            let marker = self.hidden(Type::Number);
            let completion = self.hidden(Type::Number);
            let ty = self.exception_type()?;
            let exception = self.hidden(ty);
            let result = (self.result != Type::Void).then(|| self.hidden(self.result));
            let before = self.flow_snapshot();
            let finalizer = self.block(&finalizer.stmts, true)?;
            let paths = self.flow_finally(exits, pending, &before, paths);
            self.flow_join(&entry, &paths)?;
            let mut roots = vec![exception];
            roots.extend(result);
            Ok(S::Scope(
                roots,
                vec![S::TryFinally {
                    body,
                    finalizer,
                    marker,
                    completion,
                    exception,
                    result,
                }],
            ))
        } else if statement.handler.is_some() {
            self.flow_join(&entry, &paths)?;
            Ok(S::Block(body))
        } else {
            Err(unsupported("try without catch or finally"))
        }
    }
    pub(super) fn hidden(&mut self, ty: Type) -> u32 {
        let slot = self.next;
        self.next += 1;
        self.local_types.insert(slot, ty);
        slot
    }
    fn catch_statement(&mut self, body: Vec<S>, catch: &ast::CatchClause) -> Result<S> {
        let marker = self.next;
        self.next += 1;
        self.local_types.insert(marker, Type::Number);
        self.scopes.push(BTreeMap::new());
        let mut prefix = Vec::new();
        let mut hidden_root = None;
        let binding = if let Some(param) = &catch.param {
            let ty = self.exception_type()?;
            if let ast::Pat::Ident(id) = param {
                if id.type_ann.is_some() {
                    return Err(self
                        .source
                        .locate(unsupported("catch binding annotation"), id.span()));
                }
                let slot = self.next;
                self.declare(id.id.sym.as_ref(), Some(ty), true, true)?;
                Some(slot)
            } else {
                if !self.types.profile.closures() {
                    return Err(self.source.locate(
                        unsupported("catch destructuring requires checked-f64-utf16-v12"),
                        param.span(),
                    ));
                }
                let ast::Pat::Object(pattern) = param else {
                    return Err(self.source.locate(
                        unsupported("catch binding other than identifier or object pattern"),
                        param.span(),
                    ));
                };
                if pattern.type_ann.is_some() {
                    return Err(self
                        .source
                        .locate(unsupported("catch binding annotation"), pattern.span));
                }
                for (name, span) in patterns::names(std::slice::from_ref(param))
                    .map_err(|(e, span)| self.source.locate(e, span))?
                {
                    self.declare(&name, None, false, true)
                        .map_err(|e| self.source.locate(e, span))?;
                }
                let slot = self.hidden(ty);
                hidden_root = Some(slot);
                prefix = self.catch_binding_pattern(pattern, slot, ty)?;
                Some(slot)
            }
        } else {
            None
        };
        prefix.extend(self.block(&catch.body.stmts, false)?);
        let mut slots: Vec<_> = self
            .scopes
            .pop()
            .ok_or_else(|| invalid("missing catch scope"))?
            .values()
            .map(|b| b.slot)
            .collect();
        slots.extend(hidden_root);
        Ok(S::TryCatch {
            body,
            binding,
            handler: vec![S::Scope(slots, prefix)],
            marker,
        })
    }
}
