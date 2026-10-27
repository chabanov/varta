//! Named targets use stable enclosing block indices and existing finalizer routing.
use super::*;
impl Emitter<'_> {
    pub(super) fn named_label(&mut self, id: u32, iteration: bool, body: &[S]) -> Result<()> {
        let target = self.push_label(I::Block(BlockType::Empty), false, false);
        self.named_breaks.insert(id, target);
        if iteration {
            self.pending_labels.push(id);
        }
        self.statements(body)?;
        if self.pending_labels.contains(&id) {
            return Err(CompilerError::Internal(
                "iteration label did not reach its loop".into(),
            ));
        }
        self.named_breaks.remove(&id);
        self.end();
        Ok(())
    }
    pub(super) fn bind_continues(&mut self, names: &[u32], target: usize) {
        for id in names {
            self.named_continues.insert(*id, target);
        }
    }
    pub(super) fn unbind_continues(&mut self, names: &[u32]) {
        for id in names {
            self.named_continues.remove(id);
        }
    }
    pub(super) fn named_branch(&mut self, id: u32, continuing: bool) -> Result<()> {
        let targets = if continuing {
            &self.named_continues
        } else {
            &self.named_breaks
        };
        let target = *targets
            .get(&id)
            .ok_or_else(|| CompilerError::Internal("missing named branch target".into()))?;
        self.complete_branch(target)
    }
}
