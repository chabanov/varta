//! Route abrupt completions through one emitted copy of each finalizer.
use super::*;
const NORMAL_COMPLETION: i32 = 0;
const THROW_COMPLETION: i32 = 1;
const ROUTE_BASE: usize = 2;
#[derive(Clone, Copy)]
pub(super) struct Slots {
    pub marker: u32,
    pub completion: u32,
    pub exception: u32,
    pub result: Option<u32>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Completion {
    Return,
    Branch(usize),
}
pub(super) struct Finalizer {
    target: usize,
    slots: Slots,
    scope_depth: usize,
    label_depth: usize,
    routes: Vec<Completion>,
}
impl Emitter<'_> {
    fn save_result(&mut self, slot: u32) {
        for offset in (0..self.result.width()).rev() {
            self.code
                .push(I::LocalSet(self.local_slots[slot as usize] + offset));
        }
        self.store_root(slot);
    }
    fn load_result(&mut self, slot: u32) {
        for offset in 0..self.result.width() {
            self.code
                .push(I::LocalGet(self.local_slots[slot as usize] + offset));
        }
    }
    fn route_completion(&mut self, completion: Completion) -> Result<()> {
        let finalizer = self
            .finalizers
            .last_mut()
            .ok_or_else(|| CompilerError::Internal("missing finalizer for completion".into()))?;
        let route = if let Some(index) = finalizer.routes.iter().position(|c| *c == completion) {
            index
        } else {
            finalizer.routes.push(completion);
            finalizer.routes.len() - 1
        };
        let target = finalizer.target;
        let depth = finalizer.scope_depth;
        let slots = finalizer.slots;
        if completion == Completion::Return
            && let Some(slot) = slots.result
        {
            self.save_result(slot);
        }
        self.clear_scopes(depth);
        self.code.extend([
            I::I32Const((route + ROUTE_BASE) as i32),
            I::LocalSet(self.local_slots[slots.completion as usize]),
        ]);
        self.exception_jumps.insert(self.code.len(), target);
        self.code.push(I::Br(0));
        Ok(())
    }
    pub(super) fn complete_return(&mut self) -> Result<()> {
        if self.finalizers.is_empty() && self.suspension.is_some() {
            self.frame_return()
        } else if self.finalizers.is_empty() {
            self.end_frame();
            self.code.push(I::Return);
            Ok(())
        } else {
            self.route_completion(Completion::Return)
        }
    }
    pub(super) fn complete_branch(&mut self, index: usize) -> Result<()> {
        if self
            .finalizers
            .last()
            .is_some_and(|f| index < f.label_depth)
        {
            self.route_completion(Completion::Branch(index))
        } else {
            self.clear_scopes(self.labels[index].scope_depth);
            self.code.push(I::Br(self.depth(index)));
            Ok(())
        }
    }
    pub(super) fn try_finally(&mut self, body: &[S], finalizer: &[S], slots: Slots) -> Result<()> {
        let memory = self
            .memory
            .ok_or_else(|| CompilerError::Internal("missing finalizer roots".into()))?;
        let globals = self
            .exceptions
            .ok_or_else(|| CompilerError::Internal("missing finalizer channel".into()))?;
        self.code.extend([
            I::GlobalGet(memory.roots),
            I::LocalSet(self.local_slots[slots.marker as usize]),
            I::I32Const(NORMAL_COMPLETION),
            I::LocalSet(self.local_slots[slots.completion as usize]),
        ]);
        let label_depth = self.labels.len();
        let target = self.code.len();
        let landing = self.push_label(I::Block(BlockType::Empty), false, false);
        let throw_target = self.code.len();
        self.push_label(I::Block(BlockType::Empty), false, false);
        self.handlers.push(exceptions::Handler {
            target: throw_target,
            marker: slots.marker,
            scope_depth: self.scopes.len(),
        });
        self.finalizers.push(Finalizer {
            target,
            slots,
            label_depth,
            scope_depth: self.scopes.len(),
            routes: Vec::new(),
        });
        self.statements(body)?;
        let protected = self
            .finalizers
            .pop()
            .ok_or_else(|| CompilerError::Internal("missing protected finalizer".into()))?;
        self.handlers.pop();
        self.code.push(I::Br(self.depth(landing)));
        self.end();
        self.code.extend([
            I::GlobalGet(globals.value),
            I::LocalSet(self.local_slots[slots.exception as usize]),
        ]);
        self.store_root(slots.exception);
        self.clear_exception(globals);
        self.code.extend([
            I::I32Const(THROW_COMPLETION),
            I::LocalSet(self.local_slots[slots.completion as usize]),
        ]);
        self.end();
        // Clear temporaries abandoned by return/branches as well as exceptions.
        self.code.extend([
            I::LocalGet(self.local_slots[slots.marker as usize]),
            I::GlobalSet(memory.roots),
        ]);
        self.statements(finalizer)?;
        self.completion_test(slots.completion, THROW_COMPLETION);
        self.code.extend([
            I::LocalGet(self.local_slots[slots.exception as usize]),
            I::GlobalSet(globals.value),
            I::I32Const(varta_common::exceptions::ROOT as i32),
            I::GlobalGet(globals.value),
            I::I64Store(strings::ROOT),
            I::I32Const(1),
            I::GlobalSet(globals.pending),
        ]);
        self.propagate_exception()?;
        self.end();
        for (index, completion) in protected.routes.into_iter().enumerate() {
            self.completion_test(slots.completion, (index + ROUTE_BASE) as i32);
            match completion {
                Completion::Return => {
                    if let Some(slot) = slots.result {
                        self.load_result(slot);
                    }
                    self.complete_return()?;
                }
                Completion::Branch(target) => self.complete_branch(target)?,
            }
            self.end();
        }
        Ok(())
    }
    fn completion_test(&mut self, slot: u32, value: i32) {
        self.code.extend([
            I::LocalGet(self.local_slots[slot as usize]),
            I::I32Const(value),
            I::I32Eq,
        ]);
        self.push_label(I::If(BlockType::Empty), false, false);
    }
}
