//! Loop test/update entries join only paths reaching that particular edge.
use super::*;
use flow::Snapshot;
impl Lower<'_> {
    fn flow_false_exit(&mut self, test: &ast::Expr, entry: &Snapshot) -> Result<Option<Snapshot>> {
        self.flow_alternate(entry);
        let exit = if flow_control::truth(test) != Some(true) {
            self.guard(test, false)?;
            Some(self.flow_snapshot())
        } else {
            None
        };
        self.flow_alternate(entry);
        self.guard(test, true)?;
        self.flow_edge(flow_control::truth(test) != Some(false));
        Ok(exit)
    }
    pub(super) fn flow_while(&mut self, statement: &ast::WhileStmt) -> Result<S> {
        if flow_control::truth(&statement.test) != Some(false) {
            self.flow_while_effects(statement);
        }
        let test = self.condition(&statement.test)?;
        let entry = self.flow_snapshot();
        let exit = self.flow_false_exit(&statement.test, &entry)?;
        let target = self.flow_iteration();
        self.loops += 1;
        let body = self.single(&statement.body)?;
        self.loops -= 1;
        self.flow_take(target, true);
        let mut paths = self.flow_end(target);
        paths.extend(exit);
        self.flow_join(&entry, &paths)?;
        Ok(S::While(test, body))
    }
    pub(super) fn flow_do_while(&mut self, statement: &ast::DoWhileStmt) -> Result<S> {
        if flow_control::truth(&statement.test) != Some(false) {
            self.flow_do_effects(statement);
        }
        let entry = self.flow_snapshot();
        let target = self.flow_iteration();
        self.loops += 1;
        let body = self.single(&statement.body)?;
        self.loops -= 1;
        let mut tests = self.flow_take(target, true);
        tests.push(self.flow_snapshot());
        self.flow_join(&entry, &tests)?;
        let test = self.condition(&statement.test)?;
        let tested = self.flow_snapshot();
        let exit = self.flow_false_exit(&statement.test, &tested)?;
        let mut paths = self.flow_end(target);
        paths.extend(exit);
        self.flow_join(&entry, &paths)?;
        Ok(S::DoWhile(test, body))
    }
    pub(super) fn flow_for(&mut self, statement: &ast::ForStmt) -> Result<S> {
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
        // The initializer runs once. Its facts cannot bypass mutations on the
        // repeated body/update/test edges, including the second test evaluation.
        if statement.test.as_deref().and_then(flow_control::truth) != Some(false) {
            self.flow_for_effects(statement);
        }
        let test = statement
            .test
            .as_deref()
            .map(|e| self.condition(e))
            .transpose()?;
        let entry = self.flow_snapshot();
        let exit = statement
            .test
            .as_deref()
            .map(|e| self.flow_false_exit(e, &entry))
            .transpose()?
            .flatten();
        let target = self.flow_iteration();
        self.loops += 1;
        let body = self.single(&statement.body)?;
        self.loops -= 1;
        let mut updates = self.flow_take(target, true);
        updates.push(self.flow_snapshot());
        self.flow_join(&entry, &updates)?;
        let mut update = statement
            .update
            .as_deref()
            .map(|e| self.expression_statement(e))
            .transpose()?
            .unwrap_or_default();
        let mut paths = self.flow_end(target);
        paths.extend(exit);
        self.flow_join(&entry, &paths)?;
        if matches!(&statement.init, Some(ast::VarDeclOrExpr::VarDecl(var)) if var.kind == ast::VarDeclKind::Let)
        {
            let slots: Vec<_> = self
                .scopes
                .last()
                .ok_or_else(|| invalid("missing loop scope"))?
                .values()
                .map(|b| b.slot)
                .collect();
            init.push(S::Rebind(slots.clone()));
            update.insert(0, S::Rebind(slots));
        }
        let slots = self
            .scopes
            .pop()
            .ok_or_else(|| invalid("missing loop scope"))?
            .values()
            .map(|b| b.slot)
            .collect();
        Ok(S::Scope(slots, vec![S::For(init, test, update, body)]))
    }
}
