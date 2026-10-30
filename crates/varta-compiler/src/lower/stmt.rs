use super::*;
pub(super) fn unparen(mut expr: &ast::Expr) -> &ast::Expr {
    while let ast::Expr::Paren(p) = expr {
        expr = &p.expr;
    }
    expr
}

impl Lower<'_> {
    pub(super) fn block(&mut self, statements: &[ast::Stmt], scope: bool) -> Result<Vec<S>> {
        self.depth += 1;
        if self.depth > 64 {
            return Err(invalid("nesting exceeds 64 scopes"));
        }
        if scope {
            self.scopes.push(BTreeMap::new());
        }
        let core = self.types.profile.language_core();
        for statement in statements {
            match statement {
                ast::Stmt::Decl(ast::Decl::Var(var)) => self
                    .predeclare(var)
                    .map_err(|e| self.source.locate(e, var.span()))?,
                ast::Stmt::Decl(ast::Decl::Fn(function)) if core => self
                    .predeclare_function(function)
                    .map_err(|e| self.source.locate(e, function.span()))?,
                _ => {}
            }
        }
        let early = if core {
            self.reserve_early(statements)?
        } else {
            Vec::new()
        };
        let mut result = if core {
            self.hoisted_functions(statements)?
        } else {
            Vec::new()
        };
        for statement in statements {
            result.push(self.statement(statement)?);
        }
        if core {
            let bindings: Vec<Binding> = self
                .scopes
                .last()
                .ok_or_else(|| invalid("missing lexical scope"))?
                .values()
                .filter(|binding| early.contains(&binding.slot))
                .cloned()
                .collect();
            let span = statements
                .first()
                .map_or_else(Default::default, |s| s.span());
            let prologue = self.tdz_prologue(&bindings, span);
            result.splice(0..0, prologue);
        }
        if scope {
            let slots = self
                .scopes
                .pop()
                .ok_or_else(|| invalid("missing lexical scope"))?
                .values()
                .flat_map(|binding| {
                    std::iter::once(binding.slot).chain(
                        binding
                            .tdz
                            .filter(|_| self.core.early.contains(&binding.slot)),
                    )
                })
                .collect();
            result = vec![S::Scope(slots, result)];
        }
        self.depth -= 1;
        Ok(result)
    }
    pub(super) fn single(&mut self, statement: &ast::Stmt) -> Result<Vec<S>> {
        Ok(vec![self.statement(statement)?])
    }
    fn statement(&mut self, statement: &ast::Stmt) -> Result<S> {
        let result = self
            .statement_inner(statement)
            .map_err(|e| self.source.locate(e, statement.span()));
        if self.types.profile.refinement_writes()
            && let Ok(statement) = &result
            && !completion::normal(std::slice::from_ref(statement))
        {
            self.reachable = false;
        }
        result
    }
    fn statement_inner(&mut self, statement: &ast::Stmt) -> Result<S> {
        Ok(match statement {
            ast::Stmt::Empty(_) => S::Block(vec![]),
            ast::Stmt::Labeled(statement) => self.labeled_statement(statement)?,
            ast::Stmt::Throw(t) => self.throw_statement(t)?,
            ast::Stmt::Try(t) => self.try_statement(t)?,
            ast::Stmt::Decl(ast::Decl::Var(var)) => S::Block(self.variables(var)?),
            // Hoisted to the block entry (v37 `language_core`).
            ast::Stmt::Decl(ast::Decl::Fn(_)) if self.types.profile.language_core() => {
                S::Block(vec![])
            }
            ast::Stmt::Decl(ast::Decl::Class(_)) if self.types.profile.classes() => {
                return Err(unsupported("class declaration outside module top level"));
            }
            ast::Stmt::Block(block) => S::Block(self.block(&block.stmts, true)?),
            ast::Stmt::Expr(statement) => {
                if let ast::Expr::Assign(assign) = unparen(&statement.expr) {
                    self.assignment(assign)?
                } else {
                    S::Expr(self.expr(&statement.expr)?)
                }
            }
            ast::Stmt::Return(ret) => {
                if self.infer_returns {
                    return Ok(S::Return(
                        ret.arg.as_deref().map(|expr| self.expr(expr)).transpose()?,
                    ));
                }
                let mut value = ret
                    .arg
                    .as_deref()
                    .map(|expr| self.expr_expected(expr, Some(self.result)))
                    .transpose()?;
                // `return e` in an async generator awaits `e` (v36).
                if self.async_body().is_some() {
                    value = value
                        .map(|value| self.async_return_value(value))
                        .transpose()?;
                }
                if self.types.profile.has_undefined() {
                    if self.result == Type::Void
                        && value.as_ref().is_some_and(|v| v.ty == Type::Undefined)
                    {
                        return Ok(S::Block(vec![
                            S::Expr(
                                value
                                    .take()
                                    .ok_or_else(|| invalid("missing return value"))?,
                            ),
                            S::Return(None),
                        ]));
                    }
                    if value.is_none()
                        && (self.result == Type::Undefined
                            || self.types.profile.missing()
                                && unions::members(self.result, self.types)
                                    .contains(&Type::Undefined))
                    {
                        value = Some(unions::widen(
                            Expr {
                                span: ret.span,
                                ty: Type::Undefined,
                                kind: E::Integer(0),
                            },
                            Some(self.result),
                            self.types,
                        )?);
                    }
                }
                // An async body may also return a promise to adopt (v34).
                let adopted = self
                    .asynchronous
                    .is_some_and(|id| value.as_ref().is_some_and(|v| v.ty == Type::Promise(id)));
                if !adopted && value.as_ref().map_or(Type::Void, |e| e.ty) != self.result {
                    let (expected, found) =
                        self.mismatch(self.result, value.as_ref().map_or(Type::Void, |v| v.ty));
                    return Err(self.source.locate(
                        type_error(format!(
                            "return type mismatch: expected {expected}, found {found}"
                        )),
                        value.as_ref().map_or(ret.span, |v| v.span),
                    ));
                }
                S::Return(value)
            }
            ast::Stmt::If(branch) => {
                let test = self.condition(&branch.test)?;
                let saved = self.flow_snapshot();
                self.guard(&branch.test, true)?;
                self.flow_edge(flow_control::truth(&branch.test) != Some(false));
                let yes = self.single(&branch.cons)?;
                let yes_state = self.flow_snapshot();
                self.flow_alternate(&saved);
                self.guard(&branch.test, false)?;
                self.flow_edge(flow_control::truth(&branch.test) != Some(true));
                let no = branch
                    .alt
                    .as_ref()
                    .map(|s| self.single(s))
                    .transpose()?
                    .unwrap_or_default();
                let mut paths = vec![];
                if completion::normal(&yes) {
                    paths.push(yes_state);
                }
                if completion::normal(&no) {
                    paths.push(self.flow_snapshot());
                }
                self.flow_join(&saved, &paths)?;
                S::If(test, yes, no)
            }
            ast::Stmt::DoWhile(statement) => {
                if self.types.profile.refinement_writes() {
                    return self.flow_do_while(statement);
                }
                if !self.types.profile.closures() {
                    return Err(unsupported("do-while requires checked-f64-utf16-v12"));
                }
                let saved = self.flow_snapshot();
                self.loops += 1;
                let body = self.single(&statement.body)?;
                self.loops -= 1;
                if self.types.profile.refinement_writes() {
                    // Continue can skip a later body store on the way to test.
                    self.flow_restore(&saved);
                }
                let test = self.condition(&statement.test)?;
                S::DoWhile(test, body)
            }
            ast::Stmt::While(statement) => {
                if self.types.profile.refinement_writes() {
                    return self.flow_while(statement);
                }
                let test = self.condition(&statement.test)?;
                let saved = self.flow_snapshot();
                if self.types.profile.truth_guards() {
                    self.guard(&statement.test, true)?;
                }
                self.loops += 1;
                let body = self.single(&statement.body)?;
                self.loops -= 1;
                self.flow_restore(&saved);
                S::While(test, body)
            }
            ast::Stmt::ForOf(statement) => self.for_of(statement)?,
            ast::Stmt::ForIn(statement) if self.types.profile.mixed_index_records() => {
                self.for_in(statement)?
            }
            ast::Stmt::For(statement) => {
                if self.types.profile.refinement_writes() {
                    return self.flow_for(statement);
                }
                self.scopes.push(BTreeMap::new());
                let mut init = match &statement.init {
                    Some(ast::VarDeclOrExpr::VarDecl(var)) => {
                        self.predeclare(var)
                            .map_err(|e| self.source.locate(e, var.span()))?;
                        self.variables(var)?
                    }
                    Some(ast::VarDeclOrExpr::Expr(expr)) => self.expression_statement(expr)?,
                    None => vec![],
                };
                let test = statement
                    .test
                    .as_deref()
                    .map(|e| self.condition(e))
                    .transpose()?;
                let saved = self.flow_snapshot();
                if self.types.profile.truth_guards()
                    && let Some(test) = &statement.test
                {
                    self.guard(test, true)?;
                }
                let mut update = statement
                    .update
                    .as_deref()
                    .map(|e| self.expression_statement(e))
                    .transpose()?
                    .unwrap_or_default();
                self.loops += 1;
                let body = self.single(&statement.body)?;
                self.loops -= 1;
                self.flow_restore(&saved);
                if self.types.profile.closures()
                    && matches!(&statement.init, Some(ast::VarDeclOrExpr::VarDecl(var)) if var.kind == ast::VarDeclKind::Let)
                {
                    let slots: Vec<_> = self
                        .scopes
                        .last()
                        .ok_or_else(|| invalid("missing loop scope"))?
                        .values()
                        .map(|binding| binding.slot)
                        .collect();
                    // The first iteration also owns cells distinct from the
                    // initializer environment captured by default/RHS closures.
                    init.push(S::Rebind(slots.clone()));
                    update.insert(0, S::Rebind(slots));
                }
                let slots = self
                    .scopes
                    .pop()
                    .ok_or_else(|| invalid("missing loop scope"))?
                    .values()
                    .map(|binding| binding.slot)
                    .collect();
                S::Scope(slots, vec![S::For(init, test, update, body)])
            }
            ast::Stmt::Switch(statement) => return self.switch_statement(statement),
            ast::Stmt::Break(statement) => {
                if let Some(label) = &statement.label {
                    return self.named_branch(label, false);
                }
                if self.loops + self.switches == 0 {
                    return Err(unsupported("break target"));
                }
                self.flow_branch(None, false)?;
                S::Break
            }
            ast::Stmt::Continue(statement) => {
                if let Some(label) = &statement.label {
                    return self.named_branch(label, true);
                }
                if self.loops == 0 {
                    return Err(unsupported("continue target"));
                }
                self.flow_branch(None, true)?;
                S::Continue
            }
            _ => return Err(unsupported("statement")),
        })
    }
    pub(super) fn expression_statement(&mut self, expression: &ast::Expr) -> Result<Vec<S>> {
        Ok(vec![
            if let ast::Expr::Assign(assign) = unparen(expression) {
                self.assignment(assign)?
            } else {
                S::Expr(self.expr(expression)?)
            },
        ])
    }
    fn assignment(&mut self, assign: &ast::AssignExpr) -> Result<S> {
        let statement = self
            .assignment_inner(assign)
            .map_err(|e| self.source.locate(e, assign.span))?;
        // A property or pattern store may change any path (v37 `paths`).
        if !matches!(
            assign.left,
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(_))
        ) {
            self.drop_paths();
        }
        Ok(statement)
    }
    fn assignment_inner(&mut self, assign: &ast::AssignExpr) -> Result<S> {
        if assign.op != ast::AssignOp::Assign {
            return Ok(S::Expr(self.compound_assignment_value(assign)?));
        }
        if let ast::AssignTarget::Pat(pattern) = &assign.left {
            return self.destructuring_assignment(assign, pattern);
        }
        if let ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(member)) = &assign.left {
            let base = self.write_member_base(member)?;
            if matches!(base.ty, Type::Dictionary(_)) {
                return Ok(S::Expr(self.dictionary_write(
                    member,
                    base,
                    &assign.right,
                )?));
            }
            if matches!(base.ty, Type::Collection(ir::CollectionKind::Uint8Array, _)) {
                return Ok(S::Expr(self.bytes_write(member, base, &assign.right)?));
            }
            if let Type::Tuple(_) | Type::Collection(..) = base.ty {
                return Err(unsupported(&format!(
                    "assignment to a {} element or property",
                    base.ty
                )));
            }
            if self.types.profile.closures()
                && let Type::Record(shape) = base.ty
                && let ast::MemberProp::Computed(key) = &member.prop
                && !matches!(
                    &*key.expr,
                    ast::Expr::Lit(ast::Lit::Str(_) | ast::Lit::Num(_))
                )
            {
                return Ok(S::Expr(
                    self.dynamic_record_write_value(assign, base, &key.expr, shape)?,
                ));
            }
            let field = self.place_value(member, base)?;
            if self.types.profile.array_mutations()
                && let E::Length(array) = field.kind
            {
                let length = self.expr_expected(&assign.right, Some(self.types.number()))?;
                if length.ty != self.types.number() {
                    return Err(self.source.locate(
                        type_error("array length requires number; coercion is unsupported"),
                        length.span,
                    ));
                }
                let error = self.array_error(
                    varta_common::value::ErrorClass::RangeError,
                    "Invalid array length",
                    assign.span,
                )?;
                return Ok(S::ArrayLength {
                    array: *array,
                    length,
                    error,
                });
            }
            if !matches!(field.kind, E::Field(_, _) | E::Index(_, _)) {
                return Err(unsupported(
                    "assignment to immutable result property or array length",
                ));
            }
            let value = self.expr_expected(&assign.right, Some(field.ty))?;
            if value.ty != field.ty {
                return Err(self.source.locate(
                    type_error(format!(
                        "object field type mismatch: expected {}, found {}",
                        field.ty, value.ty
                    )),
                    value.span,
                ));
            }
            return Ok(match field.kind {
                E::Field(object, index) => S::SetField(*object, index, value),
                E::Index(object, index) => {
                    let slots = self.types.profile.sparse_arrays().then(|| {
                        (
                            self.hidden(object.ty),
                            self.hidden(if self.types.profile.array_mutations() {
                                Type::Float64
                            } else {
                                Type::Number
                            }),
                        )
                    });
                    S::SetIndex {
                        array: *object,
                        index: *index,
                        value,
                        slots,
                    }
                }
                _ => return Err(invalid("missing mutable property")),
            });
        }
        let ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(name)) = &assign.left else {
            return Err(unsupported("assignment target"));
        };
        let binding = self.resolve_binding(name.id.sym.as_ref())?;
        if binding.global.is_some() {
            return Ok(S::Expr(self.global_assignment_value(
                &binding,
                &assign.right,
                assign.span,
            )?));
        }
        // A store through a closure that captured the binding before its
        // initialization (v37 `tdz`) checks the flag after the right side.
        let early = binding.mutable
            && !binding.initialized
            && binding.tdz.is_some()
            && binding.identity.0 != self.index
            && self.types.profile.language_core();
        if (!binding.mutable || !binding.initialized) && !early {
            return Err(invalid("assignment to constant or uninitialized binding"));
        }
        if self.refinements.contains_key(&binding.slot) && !self.types.profile.refinement_writes() {
            return Err(unsupported("assignment to a guard-refined union binding"));
        }
        let expected = binding
            .ty
            .ok_or_else(|| invalid("missing assignment target type"))?;
        let flow = self.types.profile.refinement_writes();
        let value = if flow {
            self.assignment_rhs_value(expected, &assign.right)?
        } else {
            self.expr_expected(&assign.right, Some(expected))?
        };
        if if flow {
            !arguments::accepts(expected, value.ty, self.types)
        } else {
            value.ty != expected
        } {
            let (expected, found) = self.mismatch(binding.ty.unwrap_or(Type::Void), value.ty);
            return Err(self.source.locate(
                type_error(format!(
                    "assignment type mismatch: expected {expected}, found {found}"
                )),
                value.span,
            ));
        }
        self.flow_assigned(binding.slot, value.ty)?;
        let value = unions::widen(value, Some(expected), self.types)?;
        if early {
            return self.tdz_write(name.id.sym.as_ref(), &binding, value);
        }
        Ok(S::Set(binding.slot, value))
    }
}
