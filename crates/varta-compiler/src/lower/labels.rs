//! Labels live in a control namespace local to the current function.
use super::*;
fn iteration(mut statement: &ast::Stmt) -> bool {
    while let ast::Stmt::Labeled(label) = statement {
        statement = &label.body;
    }
    matches!(
        statement,
        ast::Stmt::While(_) | ast::Stmt::DoWhile(_) | ast::Stmt::For(_) | ast::Stmt::ForOf(_)
    )
}
impl Lower<'_> {
    pub(super) fn labeled_statement(&mut self, statement: &ast::LabeledStmt) -> Result<S> {
        if !self.types.profile.closures() {
            return Err(unsupported(
                "statement labels require checked-f64-utf16-v12",
            ));
        }
        let name = statement.label.sym.to_string();
        if self.labels.iter().any(|(old, _, _)| *old == name) {
            return Err(self.source.locate(
                invalid(format!("duplicate active label: {name}")),
                statement.label.span,
            ));
        }
        self.depth += 1;
        if self.depth > 64 {
            return Err(invalid("nesting exceeds 64 scopes"));
        }
        let id = self.next_label;
        self.next_label = id
            .checked_add(1)
            .ok_or_else(|| invalid("too many statement labels"))?;
        let iteration = iteration(&statement.body);
        self.labels.push((name, id, iteration));
        let target = self.flow_target(flow_control::Kind::Label(id, iteration));
        let saved = self.flow_snapshot();
        let body = self.single(&statement.body)?;
        if self.types.profile.refinement_writes() {
            let mut paths = self.flow_end(target);
            paths.push(self.flow_snapshot());
            self.flow_join(&saved, &paths)?;
        }
        self.labels.pop();
        self.depth -= 1;
        Ok(S::Label {
            id,
            iteration,
            body,
        })
    }
    pub(super) fn named_branch(&mut self, label: &ast::Ident, continuing: bool) -> Result<S> {
        let (_, id, iteration) = self
            .labels
            .iter()
            .rev()
            .find(|(name, _, _)| name == label.sym.as_ref())
            .ok_or_else(|| {
                self.source
                    .locate(invalid(format!("unknown label: {}", label.sym)), label.span)
            })?;
        if continuing && !iteration {
            return Err(self.source.locate(
                invalid(format!(
                    "continue requires an iteration label: {}",
                    label.sym
                )),
                label.span,
            ));
        }
        let id = *id;
        self.flow_branch(Some(id), continuing)?;
        Ok(if continuing {
            S::ContinueLabel(id)
        } else {
            S::BreakLabel(id)
        })
    }
}
