//! The frontend's sealed-block SSA view of integer constants. Bulk-op checks
//! depend on whether the size is an Iconst *at translation*, before optimizer
//! folding. A literal copied through locals is still constant; an unsealed
//! loop phi is not. Track identities, not just equal numeric values.
use std::collections::{BTreeMap, VecDeque};
use wasmparser::{BlockType, FuncType, FuncValidator, Operator as O, ValidatorResources};
type Id = usize;
#[derive(Clone)]
enum Value {
    Other,
    Integer(u32),
    Phi(Option<Vec<Id>>),
    Alias(Id),
}
#[derive(Default)]
struct Block {
    sealed: bool,
    preds: Vec<Id>,
    vars: BTreeMap<Id, Id>,
}
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Function,
    Block,
    Loop,
    If,
}
struct Frame {
    kind: Kind,
    base: usize,
    end: Id,
    target: Id,
    labels: Vec<Id>,
    results: Vec<Id>,
    params: Vec<Id>,
    otherwise: Option<Id>,
}
pub(super) struct Flow {
    values: Vec<Value>,
    users: Vec<Vec<Id>>,
    blocks: Vec<Block>,
    frames: Vec<Frame>,
    current: Option<Id>,
    stack: Vec<Id>,
    next_var: Id,
}
impl Flow {
    pub fn new(params: usize, locals: &[(u32, wasmparser::ValType)], results: usize) -> Self {
        let mut flow = Self {
            values: Vec::new(),
            users: Vec::new(),
            blocks: vec![Block {
                sealed: true,
                ..Default::default()
            }],
            frames: Vec::new(),
            current: Some(0),
            stack: Vec::new(),
            next_var: 0,
        };
        for _ in 0..params {
            let id = flow.value(Value::Other);
            flow.blocks[0].vars.insert(flow.next_var, id);
            flow.next_var += 1;
        }
        let zero = flow.value(Value::Integer(0));
        let other = flow.value(Value::Other);
        for (count, ty) in locals {
            for _ in 0..*count {
                flow.blocks[0].vars.insert(
                    flow.next_var,
                    if *ty == wasmparser::ValType::I32 {
                        zero
                    } else {
                        other
                    },
                );
                flow.next_var += 1;
            }
        }
        let end = flow.block();
        let vars = flow.vars(results);
        flow.frames.push(Frame {
            kind: Kind::Function,
            base: 0,
            end,
            target: end,
            labels: vars.clone(),
            results: vars,
            params: Vec::new(),
            otherwise: None,
        });
        flow
    }
    fn value(&mut self, value: Value) -> Id {
        let id = self.values.len();
        self.values.push(value);
        self.users.push(Vec::new());
        id
    }
    fn block(&mut self) -> Id {
        let id = self.blocks.len();
        self.blocks.push(Block::default());
        id
    }
    fn vars(&mut self, count: usize) -> Vec<Id> {
        let first = self.next_var;
        self.next_var += count;
        (first..self.next_var).collect()
    }
    fn root(&self, mut value: Id) -> Id {
        while let Value::Alias(next) = self.values[value] {
            value = next;
        }
        value
    }
    fn simplify(&mut self, initial: Id) {
        let mut pending = VecDeque::from([initial]);
        while let Some(id) = pending.pop_front() {
            let Value::Phi(Some(args)) = &self.values[id] else {
                continue;
            };
            let mut same = None;
            let mut different = false;
            for arg in args {
                let arg = self.root(*arg);
                if arg == id {
                    continue;
                }
                if same.is_some_and(|old| old != arg) {
                    different = true;
                    break;
                }
                same = Some(arg);
            }
            if different {
                continue;
            }
            let replacement = same.unwrap_or_else(|| self.value(Value::Other));
            self.values[id] = Value::Alias(replacement);
            let users = std::mem::take(&mut self.users[id]);
            for user in users {
                if user != replacement {
                    self.users[replacement].push(user);
                    pending.push_back(user);
                }
            }
        }
    }
    fn binding(&mut self, block: Id, var: Id, pending: &mut Vec<(Id, Id, Id)>) -> Id {
        if let Some(value) = self.blocks[block].vars.get(&var) {
            return *value;
        }
        let phi = self.value(Value::Phi(None));
        self.blocks[block].vars.insert(var, phi);
        if self.blocks[block].sealed {
            pending.push((block, var, phi));
        }
        phi
    }
    fn fill(&mut self, mut pending: Vec<(Id, Id, Id)>) {
        // Explicit work stack: flat Wasm can contain arbitrarily long chains
        // of single-predecessor blocks without nesting the native stack.
        while let Some((block, var, phi)) = pending.pop() {
            let mut args = Vec::new();
            for pred in self.blocks[block].preds.clone() {
                let value = self.binding(pred, var, &mut pending);
                args.push(value);
                let root = self.root(value);
                self.users[root].push(phi);
            }
            self.values[phi] = Value::Phi(Some(args));
            self.simplify(phi);
        }
    }
    fn read(&mut self, block: Id, var: Id) -> Id {
        let mut pending = Vec::new();
        let id = self.binding(block, var, &mut pending);
        self.fill(pending);
        self.root(id)
    }
    fn seal(&mut self, block: Id) {
        self.blocks[block].sealed = true;
        let pending = self.blocks[block]
            .vars
            .iter()
            .filter_map(|(var, id)| {
                matches!(self.values[*id], Value::Phi(None)).then_some((block, *var, *id))
            })
            .collect();
        self.fill(pending);
    }
    fn edge(&mut self, from: Id, to: Id) {
        if !self.blocks[to].preds.contains(&from) {
            self.blocks[to].preds.push(from);
        }
    }
    fn assign(&mut self, block: Id, vars: &[Id], values: &[Id]) {
        for (var, value) in vars.iter().zip(values) {
            self.blocks[block].vars.insert(*var, *value);
        }
    }
    fn jump(&mut self, depth: u32) {
        if let Some(from) = self.current {
            let frame = &self.frames[self.frames.len() - 1 - depth as usize];
            let target = frame.target;
            let vars = frame.labels.clone();
            let values = self.stack[self.stack.len() - vars.len()..].to_vec();
            self.assign(from, &vars, &values);
            self.edge(from, target);
        }
    }
    pub fn trap(&mut self) {
        self.current = None;
    }
    pub fn reachable(&self) -> bool {
        self.current.is_some()
    }
    pub fn sync(&mut self, height: usize) {
        self.stack.truncate(height);
        while self.stack.len() < height {
            let id = self.value(Value::Other);
            self.stack.push(id);
        }
    }
    pub fn top(&self) -> Option<u32> {
        match self.stack.last().map(|id| &self.values[self.root(*id)]) {
            Some(Value::Integer(n)) => Some(*n),
            _ => None,
        }
    }
    pub fn op(
        &mut self,
        op: &O<'_>,
        check: &FuncValidator<ValidatorResources>,
        types: &[FuncType],
    ) {
        match op {
            O::Block { blockty } | O::Loop { blockty } | O::If { blockty } => {
                let kind = match op {
                    O::Loop { .. } => Kind::Loop,
                    O::If { .. } => Kind::If,
                    _ => Kind::Block,
                };
                if kind == Kind::If {
                    self.stack.pop();
                }
                let (params, results) = match blockty {
                    BlockType::Empty => (0, 0),
                    BlockType::Type(_) => (0, 1),
                    BlockType::FuncType(index) => {
                        let ty = &types[*index as usize];
                        (ty.params().len(), ty.results().len())
                    }
                };
                let base = self.stack.len().saturating_sub(params);
                let args = self.stack[base..].to_vec();
                let end = self.block();
                let result_vars = self.vars(results);
                let mut target = end;
                let mut labels = result_vars.clone();
                let mut otherwise = None;
                if kind == Kind::Loop {
                    target = self.block();
                    labels = self.vars(params);
                    if let Some(from) = self.current {
                        self.assign(from, &labels, &args);
                        self.edge(from, target);
                        self.current = Some(target);
                        self.stack.truncate(base);
                        for var in &labels {
                            let value = self.read(target, *var);
                            self.stack.push(value);
                        }
                    }
                } else if kind == Kind::If {
                    let then = self.block();
                    let other = self.block();
                    otherwise = Some(other);
                    if let Some(from) = self.current {
                        self.edge(from, then);
                        self.edge(from, other);
                        self.current = Some(then);
                    }
                    self.seal(then);
                    self.seal(other);
                }
                self.frames.push(Frame {
                    kind,
                    base,
                    end,
                    target,
                    labels,
                    results: result_vars,
                    params: args,
                    otherwise,
                });
            }
            O::Else => {
                let frame = self.frames.last().unwrap();
                let end = frame.end;
                let base = frame.base;
                let vars = frame.results.clone();
                let args = frame.params.clone();
                let other = frame.otherwise;
                if let Some(from) = self.current {
                    let values = self.stack[self.stack.len() - vars.len()..].to_vec();
                    self.assign(from, &vars, &values);
                    self.edge(from, end);
                }
                self.current = other.filter(|id| !self.blocks[*id].preds.is_empty());
                self.frames.last_mut().unwrap().otherwise = None;
                self.stack.truncate(base);
                self.stack.extend(args);
            }
            O::End => {
                let frame = self.frames.pop().unwrap();
                if let Some(from) = self.current {
                    let values = self.stack[self.stack.len() - frame.results.len()..].to_vec();
                    self.assign(from, &frame.results, &values);
                    self.edge(from, frame.end);
                }
                if let Some(other) = frame.otherwise {
                    self.assign(other, &frame.results, &frame.params);
                    if !self.blocks[other].preds.is_empty() {
                        self.edge(other, frame.end);
                    }
                }
                if frame.kind == Kind::Loop {
                    self.seal(frame.target);
                }
                self.seal(frame.end);
                self.current = (!self.blocks[frame.end].preds.is_empty()).then_some(frame.end);
                self.stack.truncate(frame.base);
                for var in frame.results {
                    let value = self.read(frame.end, var);
                    self.stack.push(value);
                }
            }
            O::Br { relative_depth } => {
                self.jump(*relative_depth);
                self.current = None;
            }
            O::BrIf { relative_depth } => {
                self.stack.pop();
                self.jump(*relative_depth);
                if let Some(from) = self.current {
                    let block = self.block();
                    self.edge(from, block);
                    self.seal(block);
                    self.current = Some(block);
                }
            }
            O::BrTable { targets } => {
                self.stack.pop();
                for target in targets.targets() {
                    self.jump(target.expect("validated branch"));
                }
                self.jump(targets.default());
                self.current = None;
            }
            O::Return | O::Unreachable | O::ReturnCall { .. } | O::ReturnCallIndirect { .. } => {
                self.current = None
            }
            _ if self.current.is_none() => {}
            O::LocalGet { local_index } => {
                let value = self.read(self.current.unwrap(), *local_index as usize);
                self.stack.push(value);
            }
            O::LocalSet { local_index } | O::LocalTee { local_index } => {
                let value = self.stack.pop().unwrap();
                self.blocks[self.current.unwrap()]
                    .vars
                    .insert(*local_index as usize, value);
                if matches!(op, O::LocalTee { .. }) {
                    self.stack.push(value);
                }
            }
            O::I32Const { value } => {
                let value = self.value(Value::Integer(*value as u32));
                self.stack.push(value);
            }
            _ => {
                let (pops, pushes) = op.operator_arity(check).expect("validated operator arity");
                self.stack.truncate(self.stack.len() - pops as usize);
                for _ in 0..pushes {
                    let id = self.value(Value::Other);
                    self.stack.push(id);
                }
            }
        }
    }
}
