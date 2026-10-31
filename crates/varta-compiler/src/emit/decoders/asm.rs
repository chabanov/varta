//! A small structured-control assembler for the decoder bodies: blocks and
//! loops are opened under a name and branched to by name, so no branch depth
//! is written by hand.
use std::borrow::Cow;
use wasm_encoder::{BlockType, Function, Instruction as I, MemArg, ValType};

/// Unit (`u16`) loads of the input text, two-byte aligned.
pub(super) const UNIT: MemArg = MemArg {
    offset: 0,
    align: 1,
    memory_index: super::super::strings::MEMORY,
};
/// Words of heap blocks (four-byte aligned).
pub(super) const WORD: MemArg = MemArg {
    offset: 0,
    align: 2,
    memory_index: super::super::strings::MEMORY,
};
/// Slots of heap blocks and tape entries (eight-byte aligned).
pub(super) const SLOT: MemArg = MemArg {
    offset: 0,
    align: 3,
    memory_index: super::super::strings::MEMORY,
};
/// Wide reads of input text (two-byte aligned).
pub(super) const TEXT: MemArg = UNIT;

pub(super) fn at(memarg: MemArg, offset: u64) -> MemArg {
    MemArg { offset, ..memarg }
}

/// One function body under construction.
pub(super) struct Asm {
    code: Vec<I<'static>>,
    labels: Vec<String>,
    params: u32,
    locals: Vec<ValType>,
}

impl Asm {
    /// A body with `params` parameters (locals `0..params`).
    pub(super) fn new(params: u32) -> Self {
        Self {
            code: Vec::new(),
            labels: Vec::new(),
            params,
            locals: Vec::new(),
        }
    }
    /// A fresh local of `ty`.
    pub(super) fn local(&mut self, ty: ValType) -> u32 {
        self.locals.push(ty);
        self.params + self.locals.len() as u32 - 1
    }
    pub(super) fn i(&mut self, instruction: I<'static>) -> &mut Self {
        self.code.push(instruction);
        self
    }
    pub(super) fn block(&mut self, name: &str) -> &mut Self {
        self.labels.push(name.to_owned());
        self.i(I::Block(BlockType::Empty))
    }
    pub(super) fn loop_(&mut self, name: &str) -> &mut Self {
        self.labels.push(name.to_owned());
        self.i(I::Loop(BlockType::Empty))
    }
    pub(super) fn if_(&mut self) -> &mut Self {
        self.labels.push(String::new());
        self.i(I::If(BlockType::Empty))
    }
    pub(super) fn if_result(&mut self, ty: ValType) -> &mut Self {
        self.labels.push(String::new());
        self.i(I::If(BlockType::Result(ty)))
    }
    pub(super) fn else_(&mut self) -> &mut Self {
        self.i(I::Else)
    }
    pub(super) fn end(&mut self) -> &mut Self {
        self.labels.pop();
        self.i(I::End)
    }
    fn depth(&self, name: &str) -> u32 {
        let position = self
            .labels
            .iter()
            .rposition(|label| *label == name)
            .unwrap_or_else(|| unreachable!("decoder label {name} is not open"));
        (self.labels.len() - 1 - position) as u32
    }
    pub(super) fn br(&mut self, name: &str) -> &mut Self {
        let depth = self.depth(name);
        self.i(I::Br(depth))
    }
    pub(super) fn br_if(&mut self, name: &str) -> &mut Self {
        let depth = self.depth(name);
        self.i(I::BrIf(depth))
    }
    /// `br_table` over `targets` (by name), `default` otherwise.
    pub(super) fn br_table(&mut self, targets: &[String], default: &str) -> &mut Self {
        let depths: Vec<u32> = targets.iter().map(|name| self.depth(name)).collect();
        let default = self.depth(default);
        self.i(I::BrTable(Cow::Owned(depths), default))
    }
    pub(super) fn get(&mut self, local: u32) -> &mut Self {
        self.i(I::LocalGet(local))
    }
    pub(super) fn set(&mut self, local: u32) -> &mut Self {
        self.i(I::LocalSet(local))
    }
    pub(super) fn tee(&mut self, local: u32) -> &mut Self {
        self.i(I::LocalTee(local))
    }
    pub(super) fn int(&mut self, value: i32) -> &mut Self {
        self.i(I::I32Const(value))
    }
    pub(super) fn long(&mut self, value: i64) -> &mut Self {
        self.i(I::I64Const(value))
    }
    /// `local += value` (i32).
    pub(super) fn add(&mut self, local: u32, value: i32) -> &mut Self {
        self.get(local).int(value).i(I::I32Add).set(local)
    }
    /// The finished function; every label must be closed.
    pub(super) fn finish(mut self) -> Function {
        debug_assert!(self.labels.is_empty(), "unclosed decoder labels");
        self.code.push(I::End);
        let mut function = Function::new_with_locals_types(self.locals);
        for instruction in &self.code {
            function.instruction(instruction);
        }
        function
    }
}
