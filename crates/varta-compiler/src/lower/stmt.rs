use super::*;

impl Lower<'_> {
    fn predeclare(&mut self, declaration: &ast::VarDecl) -> Result<()> {
        if declaration.declare || declaration.kind == ast::VarDeclKind::Var {
            return Err(unsupported("var or ambient local"));
        }
        for variable in &declaration.decls {
            let ast::Pat::Ident(binding) = &variable.name else {
                return Err(unsupported("binding pattern"));
            };
            self.declare(
                binding.id.sym.as_ref(),
                annotation(binding.type_ann.as_deref()).map_err(|e| {
                    self.source.locate(
                        e,
                        binding
                            .type_ann
                            .as_ref()
                            .map_or(binding.span(), |a| a.type_ann.span()),
                    )
                })?,
                false,
                declaration.kind != ast::VarDeclKind::Const,
            )
            .map_err(|e| self.source.locate(e, binding.span()))?;
        }
        Ok(())
    }
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
        let result = statements
            .iter()
            .map(|s| self.statement(s))
            .collect::<Result<Vec<_>>>()?;
        if scope {
            self.scopes.pop();
        }
        self.depth -= 1;
        Ok(result)
    }
    fn single(&mut self, statement: &ast::Stmt) -> Result<Vec<S>> {
        Ok(vec![self.statement(statement)?])
    }
    fn variables(&mut self, declaration: &ast::VarDecl) -> Result<Vec<S>> {
        let mut output = Vec::new();
        for variable in &declaration.decls {
            let ast::Pat::Ident(binding) = &variable.name else {
                return Err(unsupported("binding pattern"));
            };
            let name = binding.id.sym.as_ref();
            let old = self.binding(name)?;
            let value = self.expr(
                variable
                    .init
                    .as_deref()
                    .ok_or_else(|| unsupported("uninitialized variable"))?,
            )?;
            value_type(value.ty).map_err(|e| self.source.locate(e, value.span))?;
            if old.ty.is_some_and(|ty| ty != value.ty) {
                return Err(self.source.locate(
                    type_error(format!(
                        "type mismatch for {name}: expected {}, found {}",
                        old.ty.unwrap_or(Type::Void),
                        value.ty
                    )),
                    value.span,
                ));
            }
            self.update(name, value.ty)?;
            output.push(S::Set(old.slot, value));
        }
        Ok(output)
    }
    fn statement(&mut self, statement: &ast::Stmt) -> Result<S> {
        self.statement_inner(statement)
            .map_err(|e| self.source.locate(e, statement.span()))
    }
    fn statement_inner(&mut self, statement: &ast::Stmt) -> Result<S> {
        Ok(match statement {
            ast::Stmt::Empty(_) => S::Block(vec![]),
            ast::Stmt::Decl(ast::Decl::Var(var)) => S::Block(self.variables(var)?),
            ast::Stmt::Block(block) => S::Block(self.block(&block.stmts, true)?),
            ast::Stmt::Expr(statement) => {
                if let ast::Expr::Assign(assign) = &*statement.expr {
                    self.assignment(assign)?
                } else {
                    S::Expr(self.expr(&statement.expr)?)
                }
            }
            ast::Stmt::Return(ret) => {
                let value = ret.arg.as_deref().map(|expr| self.expr(expr)).transpose()?;
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
                let yes = self.single(&branch.cons)?;
                let no = branch
                    .alt
                    .as_ref()
                    .map(|s| self.single(s))
                    .transpose()?
                    .unwrap_or_default();
                S::If(test, yes, no)
            }
            ast::Stmt::While(statement) => {
                let test = self.condition(&statement.test)?;
                self.loops += 1;
                let body = self.single(&statement.body)?;
                self.loops -= 1;
                S::While(test, body)
            }
            ast::Stmt::For(statement) => {
                self.scopes.push(BTreeMap::new());
                let init = match &statement.init {
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
                let update = statement
                    .update
                    .as_deref()
                    .map(|e| self.expression_statement(e))
                    .transpose()?
                    .unwrap_or_default();
                self.loops += 1;
                let body = self.single(&statement.body)?;
                self.loops -= 1;
                self.scopes.pop();
                S::For(init, test, update, body)
            }
            ast::Stmt::Switch(statement) => {
                let value = self.expr(&statement.discriminant)?;
                scalar(value.ty).map_err(|e| self.source.locate(e, value.span))?;
                let discriminant = self.next;
                let selected = self.next + 1;
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
                if statement.label.is_some() || self.loops + self.switches == 0 {
                    return Err(unsupported("break target"));
                }
                S::Break
            }
            ast::Stmt::Continue(statement) => {
                if statement.label.is_some() || self.loops == 0 {
                    return Err(unsupported("continue target"));
                }
                S::Continue
            }
            _ => return Err(unsupported("statement")),
        })
    }
    fn expression_statement(&mut self, expression: &ast::Expr) -> Result<Vec<S>> {
        Ok(vec![if let ast::Expr::Assign(assign) = expression {
            self.assignment(assign)?
        } else {
            S::Expr(self.expr(expression)?)
        }])
    }
    fn assignment(&mut self, assign: &ast::AssignExpr) -> Result<S> {
        self.assignment_inner(assign)
            .map_err(|e| self.source.locate(e, assign.span))
    }
    fn assignment_inner(&mut self, assign: &ast::AssignExpr) -> Result<S> {
        if assign.op != ast::AssignOp::Assign {
            return Err(unsupported("compound assignment"));
        }
        let ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(name)) = &assign.left else {
            return Err(unsupported("assignment target"));
        };
        let binding = self.binding(name.id.sym.as_ref())?;
        if !binding.mutable || !binding.initialized {
            return Err(invalid("assignment to constant or uninitialized binding"));
        }
        let value = self.expr(&assign.right)?;
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
