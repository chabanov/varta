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
        for statement in statements {
            if let ast::Stmt::Decl(ast::Decl::Var(var)) = statement {
                self.predeclare(var)
                    .map_err(|e| self.source.locate(e, var.span()))?;
            }
        }
        let mut result = statements
            .iter()
            .map(|s| self.statement(s))
            .collect::<Result<Vec<_>>>()?;
        if scope {
            let slots = self
                .scopes
                .pop()
                .ok_or_else(|| invalid("missing lexical scope"))?
                .values()
                .map(|binding| binding.slot)
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
        self.statement_inner(statement)
            .map_err(|e| self.source.locate(e, statement.span()))
    }
    fn statement_inner(&mut self, statement: &ast::Stmt) -> Result<S> {
        Ok(match statement {
            ast::Stmt::Empty(_) => S::Block(vec![]),
            ast::Stmt::Labeled(statement) => self.labeled_statement(statement)?,
            ast::Stmt::Throw(t) => self.throw_statement(t)?,
            ast::Stmt::Try(t) => self.try_statement(t)?,
            ast::Stmt::Decl(ast::Decl::Var(var)) => S::Block(self.variables(var)?),
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
                if value.as_ref().map_or(Type::Void, |e| e.ty) != self.result {
                    return Err(self.source.locate(
                        type_error(format!(
                            "return type mismatch: expected {}, found {}",
                            self.result,
                            value.as_ref().map_or(Type::Void, |v| v.ty)
                        )),
                        value.as_ref().map_or(ret.span, |v| v.span),
                    ));
                }
                S::Return(value)
            }
            ast::Stmt::If(branch) => {
                let test = self.condition(&branch.test)?;
                let saved = self.refinements.clone();
                self.guard(&branch.test, true)?;
                let yes = self.single(&branch.cons)?;
                self.refinements = saved.clone();
                self.guard(&branch.test, false)?;
                let no = branch
                    .alt
                    .as_ref()
                    .map(|s| self.single(s))
                    .transpose()?
                    .unwrap_or_default();
                self.refinements = saved;
                S::If(test, yes, no)
            }
            ast::Stmt::DoWhile(statement) => {
                if !self.types.profile.closures() {
                    return Err(unsupported("do-while requires checked-f64-utf16-v12"));
                }
                self.loops += 1;
                let body = self.single(&statement.body)?;
                self.loops -= 1;
                let test = self.condition(&statement.test)?;
                S::DoWhile(test, body)
            }
            ast::Stmt::While(statement) => {
                let test = self.condition(&statement.test)?;
                self.loops += 1;
                let body = self.single(&statement.body)?;
                self.loops -= 1;
                S::While(test, body)
            }
            ast::Stmt::ForOf(statement) => self.for_of(statement)?,
            ast::Stmt::For(statement) => {
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
                let mut update = statement
                    .update
                    .as_deref()
                    .map(|e| self.expression_statement(e))
                    .transpose()?
                    .unwrap_or_default();
                self.loops += 1;
                let body = self.single(&statement.body)?;
                self.loops -= 1;
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
            ast::Stmt::Switch(statement) => {
                let value = self.expr(&statement.discriminant)?;
                scalar(value.ty).map_err(|e| self.source.locate(e, value.span))?;
                let discriminant = self.next;
                let selected = self.next + 1;
                self.local_types.insert(discriminant, value.ty);
                self.next += 2;
                let mut cases = Vec::new();
                let mut default = false;
                self.switches += 1;
                for case in &statement.cases {
                    let test = case.test.as_deref().map(|e| self.expr(e)).transpose()?;
                    if test.as_ref().is_some_and(|e| e.ty != value.ty) {
                        return Err(self
                            .source
                            .locate(type_error("switch case type mismatch"), case.span));
                    }
                    if test.is_none() {
                        if default {
                            return Err(self
                                .source
                                .locate(invalid("duplicate default"), case.span));
                        }
                        default = true;
                    }
                    if case.cons.iter().any(|s| matches!(s, ast::Stmt::Decl(_))) {
                        return Err(unsupported("case declaration without a block"));
                    }
                    cases.push((test, self.block(&case.cons, true)?));
                }
                self.switches -= 1;
                S::Switch {
                    value,
                    discriminant,
                    selected,
                    cases,
                }
            }
            ast::Stmt::Break(statement) => {
                if let Some(label) = &statement.label {
                    return self.named_branch(label, false);
                }
                if self.loops + self.switches == 0 {
                    return Err(unsupported("break target"));
                }
                S::Break
            }
            ast::Stmt::Continue(statement) => {
                if let Some(label) = &statement.label {
                    return self.named_branch(label, true);
                }
                if self.loops == 0 {
                    return Err(unsupported("continue target"));
                }
                S::Continue
            }
            _ => return Err(unsupported("statement")),
        })
    }
    fn expression_statement(&mut self, expression: &ast::Expr) -> Result<Vec<S>> {
        Ok(vec![
            if let ast::Expr::Assign(assign) = unparen(expression) {
                self.assignment(assign)?
            } else {
                S::Expr(self.expr(expression)?)
            },
        ])
    }
    fn assignment(&mut self, assign: &ast::AssignExpr) -> Result<S> {
        self.assignment_inner(assign)
            .map_err(|e| self.source.locate(e, assign.span))
    }
    fn assignment_inner(&mut self, assign: &ast::AssignExpr) -> Result<S> {
        if assign.op != ast::AssignOp::Assign {
            return Err(unsupported("compound assignment"));
        }
        if let ast::AssignTarget::Pat(pattern) = &assign.left {
            return self.destructuring_assignment(assign, pattern);
        }
        if let ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(member)) = &assign.left {
            let base = self.expr(&member.obj)?;
            if self.types.profile.closures()
                && let Type::Record(shape) = base.ty
                && let ast::MemberProp::Computed(key) = &member.prop
                && !matches!(
                    &*key.expr,
                    ast::Expr::Lit(ast::Lit::Str(_) | ast::Lit::Num(_))
                )
            {
                return self.dynamic_record_write(assign, base, &key.expr, shape);
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
        if !binding.mutable || !binding.initialized {
            return Err(invalid("assignment to constant or uninitialized binding"));
        }
        if self.refinements.contains_key(&binding.slot) {
            return Err(unsupported("assignment to a guard-refined union binding"));
        }
        let value = self.expr_expected(&assign.right, binding.ty)?;
        if Some(value.ty) != binding.ty {
            return Err(self.source.locate(
                type_error(format!(
                    "assignment type mismatch: expected {}, found {}",
                    binding.ty.unwrap_or(Type::Void),
                    value.ty
                )),
                value.span,
            ));
        }
        Ok(S::Set(binding.slot, value))
    }
}
