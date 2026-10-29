//! Continuation frames (K4.4 foundation). A suspendable function compiles to a
//! creator at its own index, a resume body and one branch of the closed exported
//! dispatcher. Saved bindings are binding cells reached through frame-private
//! environment segments; the frame itself is a separate managed kind retained by
//! a bounded permanent root table while it is live.
use super::*;
use crate::lower::promises::fields as promise_fields;
use frame::Completion;
use varta_common::function::{self as layout, frame};
use varta_common::scheduler::PromiseState;

/// Physical parameters of a resume body: `(frame:i64, completion:i32, value:i64)`.
const FRAME_PARAM: u32 = 0;
const COMPLETION_PARAM: u32 = 1;
const VALUE_PARAM: u32 = 2;
pub(super) const RESUME_PARAMS: u32 = 3;
const HIGH_WORD: u64 = size_of::<u32>() as u64;

#[derive(Clone, Copy)]
pub(super) struct Slot {
    pub local: u32,
    pub ty: Type,
    /// Captured locals keep their closure cell; others own a frame cell.
    pub shared: bool,
}
pub(super) struct Layout {
    pub id: u32,
    pub function: usize,
    pub slots: Vec<Slot>,
    /// Generator type id of a `function*` frame (v33).
    pub generator: Option<u32>,
}
impl Layout {
    /// Every physical local is saved except the state, completion and input
    /// locals, which each resumption re-initializes from the dispatcher.
    pub(super) fn new(
        id: u32,
        function: usize,
        ir: &ir::Function,
        machine: &ir::Suspendable,
    ) -> Self {
        let entry: BTreeSet<u32> = [machine.state, machine.completion]
            .into_iter()
            .chain(machine.inputs.values().copied())
            .collect();
        let slots = ir
            .locals
            .iter()
            .enumerate()
            .filter(|(local, ty)| ty.width() > 0 && !entry.contains(&(*local as u32)))
            .map(|(local, ty)| Slot {
                local: local as u32,
                ty: *ty,
                shared: ir.cells.contains(&(local as u32)),
            })
            .collect();
        Self {
            id,
            function,
            slots,
            generator: machine.generator,
        }
    }
    fn segments(&self) -> Vec<usize> {
        self.slots
            .chunks(frame::SEGMENT_BINDINGS)
            .map(<[Slot]>::len)
            .collect()
    }
    /// Every type a frame cell of this layout may hold.
    pub(super) fn binding_types(&self, machine: &ir::Suspendable, result: Type) -> Vec<Type> {
        self.slots
            .iter()
            .map(|slot| slot.ty)
            .chain(machine.points.iter().map(|point| point.yielded))
            .chain((result != Type::Void).then_some(result))
            .chain([machine.exception])
            .collect()
    }
    pub(super) fn catalog_row(
        &self,
        machine: &ir::Suspendable,
        result: Type,
        bindings: &BTreeMap<Type, u32>,
    ) -> layout::FrameLayout {
        let generator = self.generator.is_some();
        let input = |ty| match ty {
            _ if generator => frame::FrameInput::Guest,
            Type::Boolean => frame::FrameInput::Boolean,
            Type::Undefined | Type::Void => frame::FrameInput::Undefined,
            _ => frame::FrameInput::Float64,
        };
        layout::FrameLayout {
            code: self.function as u32,
            slots: self
                .slots
                .iter()
                .map(|slot| layout::FrameSlot {
                    binding: bindings[&slot.ty],
                    shared: slot.shared,
                })
                .collect(),
            states: machine
                .points
                .iter()
                .map(|point| layout::FrameState {
                    input: input(point.input),
                    yielded: bindings[&point.yielded],
                })
                .collect(),
            result: (result != Type::Void).then(|| bindings[&result]),
            return_input: machine.return_input.map(input),
            exception: bindings[&machine.exception],
            generator,
        }
    }
}

/// Module-wide frame resources, present only when a frame layout exists.
#[derive(Clone, Copy)]
pub(super) struct Resources {
    pub table: u32,
    pub generation: u32,
}
/// Emission context of one resume body.
#[derive(Clone, Copy)]
pub(super) struct Resume<'a> {
    pub layout: &'a Layout,
    pub machine: &'a ir::Suspendable,
}

fn word(field: u32, high: bool) -> wasm_encoder::MemArg {
    wasm_encoder::MemArg {
        offset: records::field(field).offset + if high { HIGH_WORD } else { 0 },
        ..strings::WORD
    }
}
fn table_end(resources: Resources) -> i32 {
    (resources.table as usize + frame::MAX_LIVE_FRAMES * abi::STRING_ROOT_SLOT_BYTES) as i32
}
fn binding_bytes(ty: Type) -> usize {
    value::RECORD_META_BYTES + ty.width() as usize * abi::STRING_ROOT_SLOT_BYTES
}
fn segment_bytes(count: usize) -> usize {
    value::RECORD_META_BYTES + count * abi::STRING_ROOT_SLOT_BYTES
}
/// Generations start above zero, so zeroed memory never names a live frame.
const GENERATION_STEP: i32 = 1;
/// Issue the next instance-wide generation; exhausting the counter is fatal.
fn next_generation(resources: Resources) -> [I<'static>; 9] {
    [
        I::GlobalGet(resources.generation),
        I::I32Const(u32::MAX as i32),
        I::I32Eq,
        I::If(BlockType::Empty),
        I::Unreachable,
        I::End,
        I::GlobalGet(resources.generation),
        I::I32Const(GENERATION_STEP),
        I::I32Add,
    ]
}
use varta_common::{abi, value};

/// Find the live-table slot holding `handle` (always present) and clear it.
fn release_slot(resources: Resources, cursor: u32, handle: u32) -> Vec<I<'static>> {
    vec![
        I::I32Const(resources.table as i32),
        I::LocalSet(cursor),
        I::Block(BlockType::Empty),
        I::Loop(BlockType::Empty),
        I::LocalGet(cursor),
        I::I32Const(table_end(resources)),
        I::I32GeU,
        I::If(BlockType::Empty),
        I::Unreachable,
        I::End,
        I::LocalGet(cursor),
        I::I64Load(strings::ROOT),
        I::LocalGet(handle),
        I::I64Eq,
        I::BrIf(1),
        I::LocalGet(cursor),
        I::I32Const(abi::STRING_ROOT_SLOT_BYTES as i32),
        I::I32Add,
        I::LocalSet(cursor),
        I::Br(0),
        I::End,
        I::End,
        I::LocalGet(cursor),
        I::I64Const(0),
        I::I64Store(strings::ROOT),
    ]
}

impl<'a> Emitter<'a> {
    fn resume(&self) -> Result<Resume<'a>> {
        self.suspension
            .ok_or_else(|| CompilerError::Internal("suspension outside a resume body".into()))
    }
    /// Push the segment pointer holding saved slot `index` of the frame in `frame`.
    fn segment(&mut self, frame: u32, index: usize) {
        self.code.extend([
            I::LocalGet(frame),
            I::I32WrapI64,
            I::I64Load(records::field(frame::ENVIRONMENT_FIELD)),
            I::I32WrapI64,
            I::I64Load(records::field((index / frame::SEGMENT_BINDINGS) as u32)),
            I::I32WrapI64,
        ]);
    }
    fn entry(index: usize) -> wasm_encoder::MemArg {
        records::field((index % frame::SEGMENT_BINDINGS) as u32)
    }
    /// Allocate a block and write its managed header; `self.temporary` holds it.
    fn frame_block(
        &mut self,
        bytes: usize,
        flags: i32,
        link: i32,
        meta: i32,
        count: i32,
    ) -> Result<()> {
        self.code.push(I::I32Const(bytes as i32));
        self.string_helper(strings::Helper::Allocate)?;
        self.code.push(I::LocalSet(self.temporary));
        self.initialize_value(strings::ALLOCATED | flags, link, meta, count);
        Ok(())
    }
    fn zero_fields(&mut self, count: usize) {
        self.code.extend([
            I::LocalGet(self.temporary),
            I::I32Const(value::RECORD_META_BYTES as i32),
            I::I32Add,
            I::I32Const(0),
            I::I32Const((count * abi::STRING_ROOT_SLOT_BYTES) as i32),
            I::MemoryFill(strings::MEMORY),
        ]);
    }
    /// Allocate a binding cell for `ty` with the packed i64 payload in
    /// `self.string_temporary`; the cell handle is left on the stack.
    fn completion_cell(&mut self, ty: Type) -> Result<()> {
        let bytes = binding_bytes(ty);
        self.frame_block(
            bytes,
            layout::BINDING_FLAG,
            i32::from(ty.needs_root()),
            self.binding_types[&ty] as i32,
            ty.width() as i32,
        )?;
        self.code.extend([
            I::LocalGet(self.temporary),
            I::LocalGet(self.string_temporary),
            I::I64Store(records::field(0)),
        ]);
        self.pack_value(bytes);
        Ok(())
    }
    /// Consume a value of `ty` (already expression-rooted when it is a handle)
    /// and publish it as the frame's completion cell.
    fn store_completion(&mut self, ty: Type) -> Result<()> {
        if ty == Type::Void {
            self.code.extend([
                I::LocalGet(FRAME_PARAM),
                I::I32WrapI64,
                I::I64Const(0),
                I::I64Store(records::field(frame::COMPLETION_FIELD)),
            ]);
            return Ok(());
        }
        if ty.width() != 1 {
            return Err(CompilerError::Internal(
                "multi-slot frame completion".into(),
            ));
        }
        self.pack_slot(ty);
        self.code.push(I::LocalSet(self.string_temporary));
        self.completion_cell(ty)?;
        self.code.push(I::LocalSet(self.string_temporary));
        self.code.extend([
            I::LocalGet(FRAME_PARAM),
            I::I32WrapI64,
            I::LocalGet(self.string_temporary),
            I::I64Store(records::field(frame::COMPLETION_FIELD)),
        ]);
        Ok(())
    }
    /// Suspension: publish the yielded value, save every binding, record the
    /// state and return to the dispatcher with an empty exception channel.
    pub(super) fn suspend_point(&mut self, value: &Expr, state: u32) -> Result<()> {
        if self.resume()?.machine.scheduled {
            return self.await_point(value, state);
        }
        self.expr(value)?;
        self.store_completion(value.ty)?;
        self.save_frame()?;
        if let Some(globals) = self.exceptions {
            self.code.push(I::GlobalGet(globals.pending));
            self.trap_if();
        }
        self.code.extend([
            I::LocalGet(FRAME_PARAM),
            I::I32WrapI64,
            I::I32Const(state as i32),
            I::I32Store(word(frame::CONTROL_FIELD, true)),
            I::LocalGet(FRAME_PARAM),
            I::I32WrapI64,
            I::I32Const(frame::STATUS_SUSPENDED as i32),
            I::I32Store(word(frame::STATUS_FIELD, false)),
        ]);
        self.end_frame();
        self.code.push(I::Return);
        Ok(())
    }
    /// Scheduled suspension (v34 `await`): save every binding, issue the
    /// frame's next generation, then register its resume token with the
    /// scheduler. A pending promise records the token as a reaction under
    /// the promise's serial; a settled promise queues the resumption at once.
    /// Either way the promise becomes handled, and a rejection reported as
    /// unhandled is withdrawn. The awaited promise stays in a saved local, so
    /// the frame roots it until the resumption reads its settled state.
    fn await_point(&mut self, promise: &Expr, state: u32) -> Result<()> {
        let Type::Promise(id) = promise.ty else {
            return Err(CompilerError::Internal("await of a non-promise".into()));
        };
        let resources = self
            .frame_resources
            .ok_or_else(|| CompilerError::Internal("await without frame resources".into()))?;
        let shape = self.promises[id as usize].shape;
        let slot = |name: &str| -> Result<wasm_encoder::MemArg> {
            self.records[shape as usize]
                .fields
                .iter()
                .position(|field| field.name == name)
                .map(|slot| records::field(slot as u32))
                .ok_or_else(|| CompilerError::Internal("missing promise field".into()))
        };
        let (state_field, serial, tracked, handled) = (
            slot(promise_fields::STATE)?,
            slot(promise_fields::SERIAL)?,
            slot(promise_fields::TRACKED)?,
            slot(promise_fields::HANDLED)?,
        );
        self.save_frame()?;
        if let Some(globals) = self.exceptions {
            self.code.push(I::GlobalGet(globals.pending));
            self.trap_if();
        }
        self.code.extend([
            I::LocalGet(FRAME_PARAM),
            I::I32WrapI64,
            I::I32Const(state as i32),
            I::I32Store(word(frame::CONTROL_FIELD, true)),
            I::LocalGet(FRAME_PARAM),
            I::I32WrapI64,
            I::I32Const(frame::STATUS_SUSPENDED as i32),
            I::I32Store(word(frame::STATUS_FIELD, false)),
            I::LocalGet(FRAME_PARAM),
            I::I32WrapI64,
        ]);
        self.code.extend(next_generation(resources));
        self.code.extend([
            I::GlobalSet(resources.generation),
            I::GlobalGet(resources.generation),
            I::I32Store(word(frame::STATUS_FIELD, true)),
        ]);
        let token = [
            I::LocalGet(FRAME_PARAM),
            I::I64Const(i64::from(u32::MAX)),
            I::I64And,
            I::GlobalGet(resources.generation),
            I::I64ExtendI32U,
            I::I64Const(i64::from(u32::BITS)),
            I::I64Shl,
            I::I64Or,
        ];
        let host = |operation: Host| -> Result<u32> {
            self.hosts
                .get(&operation)
                .copied()
                .ok_or_else(|| CompilerError::Internal("missing scheduler host operation".into()))
        };
        let (react, enqueue, handled_operation) = (
            host(Host::PromiseReact)?,
            host(Host::SchedulerEnqueue)?,
            host(Host::PromiseHandled)?,
        );
        self.expr(promise)?;
        self.code.extend([
            I::I32WrapI64,
            I::LocalTee(self.temporary),
            I::F64Load(state_field),
            I::F64Const(f64::from(PromiseState::Pending as i32).into()),
            I::F64Eq,
            I::If(BlockType::Empty),
            I::LocalGet(self.temporary),
            I::LocalGet(self.temporary),
            I::F64Load(serial),
        ]);
        self.code.extend(token.clone());
        self.code
            .extend([I::Call(react), I::F64Store(serial), I::Else]);
        self.code.extend(token);
        self.code.extend([
            I::Call(enqueue),
            I::LocalGet(self.temporary),
            I::F64Load(tracked),
            I::F64Const(0.0.into()),
            I::F64Ne,
            I::If(BlockType::Empty),
            I::LocalGet(self.temporary),
            I::F64Load(tracked),
            I::Call(handled_operation),
            I::LocalGet(self.temporary),
            I::F64Const(0.0.into()),
            I::F64Store(tracked),
            I::End,
            I::End,
            I::LocalGet(self.temporary),
            I::I64Const(1),
            I::I64Store(handled),
        ]);
        self.end_frame();
        self.code.push(I::Return);
        Ok(())
    }
    /// `Expression::Schedule`: create the frame, then start it through the
    /// dispatcher (its first run up to a suspension) or queue its start. A
    /// function without a suspension point has no frame and is called.
    pub(super) fn schedule(
        &mut self,
        function: usize,
        args: &[Expr],
        mode: ir::ScheduleMode,
    ) -> Result<()> {
        for arg in args {
            self.expr(arg)?;
        }
        let index = self.functions_begin + function as u32;
        if !self.frame_functions.contains_key(&function) {
            if mode == ir::ScheduleMode::Enqueue {
                return Err(CompilerError::Internal(
                    "queued promise job without a frame".into(),
                ));
            }
            self.code.push(I::Call(index));
            return self.check_exception();
        }
        self.code.push(I::Call(index));
        match mode {
            ir::ScheduleMode::Start => {
                let dispatcher = self.dispatcher.ok_or_else(|| {
                    CompilerError::Internal("scheduled frame without a dispatcher".into())
                })?;
                self.code.extend([
                    I::I32Const(Completion::Normal as i32),
                    I::I64Const(0),
                    I::Call(dispatcher),
                    I::Drop,
                ]);
            }
            ir::ScheduleMode::Enqueue => {
                let enqueue = self
                    .hosts
                    .get(&Host::SchedulerEnqueue)
                    .copied()
                    .ok_or_else(|| {
                        CompilerError::Internal("missing scheduler host operation".into())
                    })?;
                self.code.push(I::Call(enqueue));
            }
        }
        Ok(())
    }
    fn save_frame(&mut self) -> Result<()> {
        let resume = self.resume()?;
        for (index, slot) in resume.layout.slots.iter().enumerate() {
            if slot.shared {
                self.segment(FRAME_PARAM, index);
                self.code.extend([
                    I::LocalGet(self.cell_slots[&slot.local]),
                    I::I64Store(Self::entry(index)),
                ]);
                continue;
            }
            let physical = self.local_slots[slot.local as usize];
            self.segment(FRAME_PARAM, index);
            self.code.extend([
                I::I64Load(Self::entry(index)),
                I::I32WrapI64,
                I::LocalGet(physical),
            ]);
            self.pack_slot(slot.ty.result_value().unwrap_or(slot.ty));
            self.code.push(I::I64Store(records::field(0)));
            if slot.ty.width() == 2 {
                self.segment(FRAME_PARAM, index);
                self.code.extend([
                    I::I64Load(Self::entry(index)),
                    I::I32WrapI64,
                    I::LocalGet(physical + 1),
                    I::I64ExtendI32U,
                    I::I64Store(records::field(1)),
                ]);
            }
        }
        Ok(())
    }
    /// Exact binding checks before any restored value is used: allocation,
    /// flags, reference mask, private type id, width and canonical payload.
    fn checked_cell(&mut self, physical: u32, ty: Type) {
        self.managed_guard(
            physical,
            binding_bytes(ty),
            layout::BINDING_FLAG,
            Some(i32::from(ty.needs_root())),
        );
        self.assert_word(layout::SIGNATURE_OFFSET, self.binding_types[&ty] as i32);
        self.assert_word(layout::RESERVED_OFFSETS[0], ty.width() as i32);
        let limit = match ty.result_value().unwrap_or(ty) {
            Type::Boolean => Some(1),
            Type::Null | Type::Undefined | Type::Void => Some(0),
            _ => None,
        };
        if let Some(limit) = limit {
            self.code.extend([
                I::LocalGet(self.temporary),
                I::I64Load(records::field(0)),
                I::I64Const(limit),
                I::I64GtU,
            ]);
            self.trap_if();
        }
        if ty.width() == 2 {
            self.code.extend([
                I::LocalGet(self.temporary),
                I::I64Load(records::field(1)),
                I::I64Const(strings::LENGTH_SHIFT as i64),
                I::I64ShrU,
                I::I64Eqz,
                I::I32Eqz,
            ]);
            self.trap_if();
        }
    }
    fn restore_frame(&mut self) -> Result<()> {
        let resume = self.resume()?;
        for (index, slot) in resume.layout.slots.iter().enumerate() {
            if slot.shared {
                let cell = self.cell_slots[&slot.local];
                self.segment(FRAME_PARAM, index);
                self.code.extend([
                    I::I64Load(Self::entry(index)),
                    I::LocalTee(cell),
                    I::I64Eqz,
                    I::I32Eqz,
                    I::If(BlockType::Empty),
                ]);
                self.checked_cell(cell, slot.ty);
                self.code.push(I::End);
            } else {
                let physical = self.local_slots[slot.local as usize];
                self.segment(FRAME_PARAM, index);
                self.code.extend([
                    I::I64Load(Self::entry(index)),
                    I::LocalSet(self.string_temporary),
                ]);
                self.checked_cell(self.string_temporary, slot.ty);
                self.code
                    .extend([I::LocalGet(self.temporary), I::I64Load(records::field(0))]);
                self.unpack_slot(slot.ty.result_value().unwrap_or(slot.ty));
                self.code.push(I::LocalSet(physical));
                if slot.ty.width() == 2 {
                    self.code.extend([
                        I::LocalGet(self.temporary),
                        I::I64Load(records::field(1)),
                        I::I32WrapI64,
                        I::LocalSet(physical + 1),
                    ]);
                }
            }
            self.store_root(slot.local);
        }
        Ok(())
    }
    /// Resume-body prologue: restore every saved binding, then the target state,
    /// completion kind and typed scalar input (already admitted by the dispatcher).
    pub(super) fn begin_resume(&mut self, function: &ir::Function) -> Result<()> {
        let resume = self.resume()?;
        self.begin_frame(function)?;
        self.restore_frame()?;
        let machine = resume.machine;
        self.code.extend([
            I::LocalGet(FRAME_PARAM),
            I::I32WrapI64,
            I::I32Load(word(frame::CONTROL_FIELD, true)),
            I::LocalSet(self.local_slots[machine.state as usize]),
            I::LocalGet(COMPLETION_PARAM),
            I::LocalSet(self.local_slots[machine.completion as usize]),
        ]);
        for (&ty, &slot) in &machine.inputs {
            self.code.push(I::LocalGet(VALUE_PARAM));
            self.unpack_slot(ty);
            self.code.push(I::LocalSet(self.local_slots[slot as usize]));
        }
        Ok(())
    }
    /// A return from a resume body publishes its value as the completion.
    pub(super) fn frame_return(&mut self) -> Result<()> {
        self.store_completion(self.result)?;
        self.end_frame();
        self.code.push(I::Return);
        Ok(())
    }
    /// The creator allocates and registers the frame, binds the arguments into
    /// frame cells and returns a resume token without entering the body.
    pub(super) fn frame_creator(
        &mut self,
        function: &ir::Function,
        layout: &Layout,
        resources: Resources,
    ) -> Result<()> {
        self.begin_frame(function)?;
        self.frame_block(
            frame::FRAME_BYTES,
            frame::FRAME_FLAG,
            frame::FRAME_MASK,
            layout.id as i32,
            frame::FRAME_FIELDS as i32,
        )?;
        for (field, initial) in [
            (frame::CONTROL_FIELD, i64::from(layout.function as u32)),
            (frame::STATUS_FIELD, i64::from(frame::STATUS_SUSPENDED)),
            (frame::ENVIRONMENT_FIELD, 0),
            (frame::COMPLETION_FIELD, 0),
            (frame::HANDLER_FIELD, 0),
        ] {
            self.code.extend([
                I::LocalGet(self.temporary),
                I::I64Const(initial),
                I::I64Store(records::field(field)),
            ]);
        }
        self.pack_value(frame::FRAME_BYTES);
        self.code.push(I::LocalSet(self.string_temporary));
        // Register before the next allocation so the frame is always rooted.
        self.code.extend([
            I::I32Const(resources.table as i32),
            I::LocalSet(self.temporary),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(self.temporary),
            I::I32Const(table_end(resources)),
            I::I32GeU,
            I::If(BlockType::Empty),
            I::Unreachable,
            I::End,
            I::LocalGet(self.temporary),
            I::I64Load(strings::ROOT),
            I::I64Eqz,
            I::BrIf(1),
            I::LocalGet(self.temporary),
            I::I32Const(abi::STRING_ROOT_SLOT_BYTES as i32),
            I::I32Add,
            I::LocalSet(self.temporary),
            I::Br(0),
            I::End,
            I::End,
            I::LocalGet(self.temporary),
            I::LocalGet(self.string_temporary),
            I::I64Store(strings::ROOT),
        ]);
        let segments = layout.segments();
        if !segments.is_empty() {
            let bytes = segment_bytes(segments.len());
            self.frame_block(
                bytes,
                layout::ENVIRONMENT_FLAG,
                value::field_mask(segments.len()) as i32,
                frame::FRAME_ENVIRONMENT_KIND,
                segments.len() as i32,
            )?;
            self.zero_fields(segments.len());
            self.code
                .extend([I::LocalGet(self.string_temporary), I::I32WrapI64]);
            self.pack_value(bytes);
            self.code
                .push(I::I64Store(records::field(frame::ENVIRONMENT_FIELD)));
        }
        for (index, count) in segments.iter().copied().enumerate() {
            let bytes = segment_bytes(count);
            self.frame_block(
                bytes,
                layout::ENVIRONMENT_FLAG,
                value::field_mask(count) as i32,
                frame::FRAME_SEGMENT_KIND,
                count as i32,
            )?;
            self.zero_fields(count);
            self.code.extend([
                I::LocalGet(self.string_temporary),
                I::I32WrapI64,
                I::I64Load(records::field(frame::ENVIRONMENT_FIELD)),
                I::I32WrapI64,
            ]);
            self.pack_value(bytes);
            self.code.push(I::I64Store(records::field(index as u32)));
        }
        let parameters = function.params.len() as u32;
        for (index, slot) in layout.slots.iter().enumerate() {
            let parameter = slot.local < parameters;
            if slot.shared && !parameter {
                continue;
            }
            let bytes = binding_bytes(slot.ty);
            self.frame_block(
                bytes,
                layout::BINDING_FLAG,
                i32::from(slot.ty.needs_root()),
                self.binding_types[&slot.ty] as i32,
                slot.ty.width() as i32,
            )?;
            self.code.push(I::LocalGet(self.temporary));
            if parameter {
                self.code
                    .push(I::LocalGet(self.local_slots[slot.local as usize]));
                self.pack_slot(slot.ty.result_value().unwrap_or(slot.ty));
            } else {
                self.code.push(I::I64Const(0));
            }
            self.code.push(I::I64Store(records::field(0)));
            if slot.ty.width() == 2 {
                self.code.push(I::LocalGet(self.temporary));
                if parameter {
                    self.code.extend([
                        I::LocalGet(self.local_slots[slot.local as usize] + 1),
                        I::I64ExtendI32U,
                    ]);
                } else {
                    self.code.push(I::I64Const(0));
                }
                self.code.push(I::I64Store(records::field(1)));
            }
            self.segment(self.string_temporary, index);
            self.pack_value(bytes);
            self.code.push(I::I64Store(Self::entry(index)));
        }
        self.code
            .extend([I::LocalGet(self.string_temporary), I::I32WrapI64]);
        self.code.extend(next_generation(resources));
        self.code.extend([
            I::GlobalSet(resources.generation),
            I::GlobalGet(resources.generation),
            I::I32Store(word(frame::STATUS_FIELD, true)),
        ]);
        if layout.generator.is_some() {
            // A generator frame belongs to its generator object: the table
            // slot rooted it only while the creator allocated its cells. It
            // leaves the table, so `varta_resume` can never reach it.
            self.code.extend(release_slot(
                resources,
                self.temporary,
                self.string_temporary,
            ));
            self.end_frame();
            self.code
                .extend([I::LocalGet(self.string_temporary), I::Return]);
            return Ok(());
        }
        self.end_frame();
        self.code.extend([
            I::LocalGet(self.string_temporary),
            I::I64Const(i64::from(u32::MAX)),
            I::I64And,
            I::GlobalGet(resources.generation),
            I::I64ExtendI32U,
            I::I64Const(i64::from(u32::BITS)),
            I::I64Shl,
            I::I64Or,
            I::Return,
        ]);
        Ok(())
    }
}

/// Inputs of the closed dispatcher, all resolved by emission.
pub(super) struct Dispatch<'a> {
    pub layouts: &'a [Layout],
    pub machines: &'a BTreeMap<usize, ir::Suspendable>,
    pub bodies: &'a [u32],
    pub resources: Resources,
    pub exceptions: exceptions::Globals,
    pub allocate: u32,
    pub bindings: &'a BTreeMap<Type, u32>,
}

/// `varta_resume(token, completion, value) -> outcome`: closed direct calls by
/// layout, after checking channel emptiness, frame ownership, envelope,
/// status/reentrancy, generation, completion kind, layout/code/state agreement
/// and input admissibility. Uncaught exceptions move into the frame.
pub(super) fn dispatcher(dispatch: &Dispatch<'_>) -> Result<wasm_encoder::Function> {
    const TOKEN: u32 = 0;
    const KIND: u32 = 1;
    const VALUE: u32 = 2;
    const PTR: u32 = 3;
    const CURSOR: u32 = 4;
    const STATE: u32 = 5;
    const OUTCOME: u32 = 6;
    const CELL: u32 = 7;
    const HANDLE: u32 = 8;
    let resources = dispatch.resources;
    let globals = dispatch.exceptions;
    let mut code: Vec<I<'static>> = Vec::new();
    let trap_if = |code: &mut Vec<I<'static>>| {
        code.extend([I::If(BlockType::Empty), I::Unreachable, I::End]);
    };
    let header = |code: &mut Vec<I<'static>>, memarg: wasm_encoder::MemArg| {
        code.extend([
            I::LocalGet(PTR),
            I::I32Const(abi::STRING_HEADER_BYTES as i32),
            I::I32Sub,
            I::I32Load(memarg),
        ]);
    };
    // The scheduler only ever holds control with an empty exception channel.
    code.push(I::GlobalGet(globals.pending));
    trap_if(&mut code);
    code.extend([
        I::LocalGet(TOKEN),
        I::I32WrapI64,
        I::LocalTee(PTR),
        I::I32Eqz,
    ]);
    trap_if(&mut code);
    // Ownership: the frame must be live in this instance's root table.
    code.extend([
        I::I32Const(resources.table as i32),
        I::LocalSet(CURSOR),
        I::Block(BlockType::Empty),
        I::Loop(BlockType::Empty),
        I::LocalGet(CURSOR),
        I::I32Const(table_end(resources)),
        I::I32GeU,
        I::If(BlockType::Empty),
        I::Unreachable,
        I::End,
        I::LocalGet(CURSOR),
        I::I64Load(strings::ROOT),
        I::LocalTee(HANDLE),
        I::I32WrapI64,
        I::LocalGet(PTR),
        I::I32Eq,
        I::BrIf(1),
        I::LocalGet(CURSOR),
        I::I32Const(abi::STRING_ROOT_SLOT_BYTES as i32),
        I::I32Add,
        I::LocalSet(CURSOR),
        I::Br(0),
        I::End,
        I::End,
        I::LocalGet(HANDLE),
        I::I64Const(i64::from(u32::BITS)),
        I::I64ShrU,
        I::I64Const(frame::FRAME_BYTES as i64),
        I::I64Ne,
    ]);
    trap_if(&mut code);
    header(&mut code, strings::FLAG);
    code.extend([
        I::I32Const(strings::ALLOCATED | frame::FRAME_FLAG),
        I::I32Ne,
    ]);
    trap_if(&mut code);
    header(&mut code, strings::LINK);
    code.extend([I::I32Const(frame::FRAME_MASK), I::I32Ne]);
    trap_if(&mut code);
    code.extend([
        I::LocalGet(PTR),
        I::I32Load(wasm_encoder::MemArg {
            offset: layout::RESERVED_OFFSETS[0],
            ..strings::WORD
        }),
        I::I32Const(frame::FRAME_FIELDS as i32),
        I::I32Ne,
        I::LocalGet(PTR),
        I::I64Load(records::field(frame::HANDLER_FIELD)),
        I::I64Eqz,
        I::I32Eqz,
        I::I32Or,
    ]);
    trap_if(&mut code);
    // Completed and running (reentrant) frames are rejected before any effect.
    code.extend([
        I::LocalGet(PTR),
        I::I32Load(word(frame::STATUS_FIELD, false)),
        I::I32Const(frame::STATUS_SUSPENDED as i32),
        I::I32Ne,
        I::LocalGet(PTR),
        I::I32Load(word(frame::STATUS_FIELD, true)),
        I::LocalGet(TOKEN),
        I::I64Const(i64::from(u32::BITS)),
        I::I64ShrU,
        I::I32WrapI64,
        I::I32Ne,
        I::I32Or,
        I::LocalGet(KIND),
        I::I32Const(frame::Completion::Return as i32),
        I::I32GtU,
        I::I32Or,
    ]);
    trap_if(&mut code);
    code.extend([
        I::LocalGet(PTR),
        I::I32Load(word(frame::CONTROL_FIELD, true)),
        I::LocalSet(STATE),
        I::Block(BlockType::Empty),
    ]);
    for (layout, body) in dispatch.layouts.iter().zip(dispatch.bodies) {
        let machine = &dispatch.machines[&layout.function];
        if layout.generator.is_some() {
            // Generator frames are resumed only by their typed step
            // functions and never enter the live table outside their
            // creator, so ownership already fails. The branch still traps
            // before any effect; the unreachable call pairs the body with
            // its layout for admission.
            code.extend([
                I::LocalGet(PTR),
                I::I32Load(strings::WORD),
                I::I32Const(layout.id as i32),
                I::I32Eq,
                I::If(BlockType::Empty),
                I::Unreachable,
                I::LocalGet(HANDLE),
                I::LocalGet(KIND),
                I::LocalGet(VALUE),
                I::Call(*body),
                I::End,
            ]);
            continue;
        }
        code.extend([
            I::LocalGet(PTR),
            I::I32Load(strings::WORD),
            I::I32Const(layout.id as i32),
            I::I32Eq,
            I::If(BlockType::Empty),
            I::LocalGet(PTR),
            I::I32Load(word(frame::CONTROL_FIELD, false)),
            I::I32Const(layout.function as i32),
            I::I32Ne,
            I::LocalGet(STATE),
            I::I32Const(machine.points.len() as i32),
            I::I32GtU,
            I::I32Or,
        ]);
        trap_if(&mut code);
        let limit = |ty: Type| match ty {
            Type::Boolean => Some(1),
            Type::Undefined | Type::Void => Some(0),
            _ => None,
        };
        for (state, point) in (1..).zip(&machine.points) {
            if let Some(limit) = limit(point.input) {
                code.extend([
                    I::LocalGet(KIND),
                    I::I32Eqz,
                    I::LocalGet(STATE),
                    I::I32Const(state),
                    I::I32Eq,
                    I::I32And,
                    I::LocalGet(VALUE),
                    I::I64Const(limit),
                    I::I64GtU,
                    I::I32And,
                ]);
                trap_if(&mut code);
            }
        }
        code.extend([
            I::LocalGet(KIND),
            I::I32Const(frame::Completion::Return as i32),
            I::I32Eq,
        ]);
        match machine.return_input {
            None => {}
            Some(ty) => match limit(ty) {
                None => code.extend([I::Drop, I::I32Const(0)]),
                Some(limit) => {
                    code.extend([I::LocalGet(VALUE), I::I64Const(limit), I::I64GtU, I::I32And])
                }
            },
        }
        trap_if(&mut code);
        code.extend([
            I::LocalGet(PTR),
            I::I32Const(frame::STATUS_RUNNING as i32),
            I::I32Store(word(frame::STATUS_FIELD, false)),
            I::LocalGet(PTR),
            I::I64Const(0),
            I::I64Store(records::field(frame::COMPLETION_FIELD)),
            I::LocalGet(HANDLE),
            I::LocalGet(KIND),
            I::LocalGet(VALUE),
            I::Call(*body),
        ]);
        if machine.scheduled {
            // A scheduled body (v34) settles its promise on every completion,
            // so no exception escapes it, and its suspension point already
            // issued the generation of the token it registered.
            code.push(I::GlobalGet(globals.pending));
            trap_if(&mut code);
            code.extend([
                I::LocalGet(PTR),
                I::I32Load(word(frame::STATUS_FIELD, false)),
                I::I32Const(frame::STATUS_SUSPENDED as i32),
                I::I32Eq,
                I::If(BlockType::Empty),
                I::I32Const(frame::Outcome::Suspended as i32),
                I::Return,
                I::End,
                I::LocalGet(PTR),
                I::I32Const(frame::STATUS_COMPLETED as i32),
                I::I32Store(word(frame::STATUS_FIELD, false)),
                I::LocalGet(CURSOR),
                I::I64Const(0),
                I::I64Store(strings::ROOT),
                I::I32Const(frame::Outcome::Returned as i32),
                I::Return,
                I::End,
            ]);
            continue;
        }
        code.extend([I::Br(1), I::End]);
    }
    code.extend([I::Unreachable, I::End]);
    // Uncaught exceptions leave the channel and become the frame completion.
    let exception = dispatch.machines.values().next().map(|m| m.exception);
    let exception = exception
        .ok_or_else(|| CompilerError::Internal("dispatcher without frame layouts".into()))?;
    let bytes = binding_bytes(exception);
    code.extend([
        I::GlobalGet(globals.pending),
        I::If(BlockType::Empty),
        I::I32Const(bytes as i32),
        I::Call(dispatch.allocate),
        I::LocalSet(CELL),
        I::LocalGet(CELL),
        I::I32Const(abi::STRING_HEADER_BYTES as i32),
        I::I32Sub,
        I::I32Const(strings::ALLOCATED | layout::BINDING_FLAG),
        I::I32Store(strings::FLAG),
        I::LocalGet(CELL),
        I::I32Const(abi::STRING_HEADER_BYTES as i32),
        I::I32Sub,
        I::I32Const(i32::from(exception.needs_root())),
        I::I32Store(strings::LINK),
        I::LocalGet(CELL),
        I::I32Const(dispatch.bindings[&exception] as i32),
        I::I32Store(strings::WORD),
        I::LocalGet(CELL),
        I::I32Const(exception.width() as i32),
        I::I32Store(wasm_encoder::MemArg {
            offset: layout::RESERVED_OFFSETS[0],
            ..strings::WORD
        }),
        I::LocalGet(CELL),
        I::GlobalGet(globals.value),
        I::I64Store(records::field(0)),
        I::LocalGet(PTR),
        I::LocalGet(CELL),
        I::I64ExtendI32U,
        I::I64Const((bytes as i64) << strings::LENGTH_SHIFT),
        I::I64Or,
        I::I64Store(records::field(frame::COMPLETION_FIELD)),
        I::I32Const(0),
        I::GlobalSet(globals.pending),
        I::I64Const(0),
        I::GlobalSet(globals.value),
        I::I32Const(varta_common::exceptions::ROOT as i32),
        I::I64Const(0),
        I::I64Store(strings::ROOT),
        I::I32Const(frame::Outcome::Threw as i32),
        I::LocalSet(OUTCOME),
        I::Else,
        I::LocalGet(PTR),
        I::I32Load(word(frame::STATUS_FIELD, false)),
        I::I32Const(frame::STATUS_SUSPENDED as i32),
        I::I32Eq,
        I::If(BlockType::Empty),
        I::LocalGet(PTR),
    ]);
    code.extend(next_generation(resources));
    code.extend([
        I::GlobalSet(resources.generation),
        I::GlobalGet(resources.generation),
        I::I32Store(word(frame::STATUS_FIELD, true)),
        I::I32Const(frame::Outcome::Suspended as i32),
        I::LocalSet(OUTCOME),
        I::Else,
        I::I32Const(frame::Outcome::Returned as i32),
        I::LocalSet(OUTCOME),
        I::End,
        I::End,
        // A completed frame leaves the root table; its completion stays
        // readable until the next guest allocation may reclaim it.
        I::LocalGet(OUTCOME),
        I::I32Const(frame::Outcome::Suspended as i32),
        I::I32Ne,
        I::If(BlockType::Empty),
        I::LocalGet(PTR),
        I::I32Const(frame::STATUS_COMPLETED as i32),
        I::I32Store(word(frame::STATUS_FIELD, false)),
        I::LocalGet(CURSOR),
        I::I64Const(0),
        I::I64Store(strings::ROOT),
        I::End,
        I::LocalGet(OUTCOME),
        I::End,
    ]);
    let mut function =
        wasm_encoder::Function::new([(HANDLE - PTR, ValType::I32), (1, ValType::I64)]);
    for instruction in &code {
        function.instruction(instruction);
    }
    Ok(function)
}

/// The resume body's Wasm locals: every IR local (parameters included), cells,
/// scratch and expression root marks, after the three resume parameters.
pub(super) fn resume_locals(function: &ir::Function, marks: usize) -> Vec<ValType> {
    function
        .locals
        .iter()
        .copied()
        .flat_map(wasm_types)
        .chain(std::iter::repeat_n(ValType::I64, function.cells.len()))
        .chain([ValType::I32, ValType::I64, ValType::F64])
        .chain(std::iter::repeat_n(ValType::I32, marks))
        .collect()
}

/// Emit the resume body from the creator's emitter: identical module context,
/// with every IR local moved behind the three resume parameters.
pub(super) fn resume_body<'a>(
    creator: Emitter<'a>,
    function: &ir::Function,
    resume: Resume<'a>,
    needs_arena: bool,
) -> Result<wasm_encoder::Function> {
    let mut next = RESUME_PARAMS;
    let local_slots = function
        .locals
        .iter()
        .map(|ty| {
            let slot = next;
            next += ty.width();
            slot
        })
        .collect();
    let cell_slots = function
        .cells
        .iter()
        .map(|slot| {
            let physical = next;
            next += 1;
            (*slot, physical)
        })
        .collect();
    let mut emitter = Emitter {
        code: Vec::new(),
        local_slots,
        temporary: next,
        string_temporary: next + 1,
        float_temporary: next + 2,
        frame_base: next + 3,
        frame_end: next + 4,
        root_mark_base: next + 5,
        expr_depth: 0,
        max_expr_depth: 0,
        temporary_floor: None,
        exception_jumps: BTreeMap::new(),
        cell_slots,
        suspension: Some(resume),
        ..creator
    };
    emitter.begin_resume(function)?;
    emitter
        .statements(&function.body)
        .map_err(|e| function.source.locate(e, function.span))?;
    emitter.end_frame();
    emitter.resolve_exception_jumps()?;
    let marks = if needs_arena {
        2 + emitter.max_expr_depth as usize
    } else {
        0
    };
    let mut body = wasm_encoder::Function::new_with_locals_types(resume_locals(function, marks));
    for instruction in &emitter.code {
        body.instruction(instruction);
    }
    body.instruction(&I::End);
    if body.byte_len() > abi::MAX_WASM_FUNCTION_BODY_BYTES {
        return Err(function.source.locate(
            CompilerError::Invalid("resume body exceeds compilation budget".into()),
            function.span,
        ));
    }
    Ok(body)
}

/// Test-only entry points: frame creators by layout id and the collector, so
/// tests can create frames and force collections between resumptions.
#[cfg(any(test, feature = "suspension-test-hook"))]
pub(crate) const TEST_CREATE_PREFIX: &str = "varta.test.frame.";
#[cfg(any(test, feature = "suspension-test-hook"))]
pub(crate) const TEST_COLLECT: &str = "varta.test.collect";
#[cfg(any(test, feature = "suspension-test-hook"))]
pub(super) fn test_exports(
    exports: &mut wasm_encoder::ExportSection,
    layouts: &[Layout],
    functions_begin: u32,
    helpers: &BTreeMap<strings::Helper, u32>,
) {
    for layout in layouts {
        exports.export(
            &format!("{TEST_CREATE_PREFIX}{}", layout.id),
            wasm_encoder::ExportKind::Func,
            functions_begin + layout.function as u32,
        );
    }
    if let Some(collect) = helpers.get(&strings::Helper::Collect) {
        exports.export(TEST_COLLECT, wasm_encoder::ExportKind::Func, *collect);
    }
}

/// The closed step function of generator type `generator` (v33):
/// `(generator:i64, completion:i32, value:i64) -> outcome`. It applies the
/// ECMAScript generator state rules (executing, completed, not started) before
/// any effect, then resumes the frame by a direct call chosen by its layout id
/// among that type's generator layouts. An uncaught exception stays in the
/// channel for the caller to propagate; the frame is completed either way.
pub(super) fn step_function(
    generator: u32,
    layouts: &[Layout],
    bodies: &[u32],
    pending: u32,
) -> wasm_encoder::Function {
    use ir::GeneratorOutcome as Out;
    const HANDLE: u32 = 0;
    const KIND: u32 = 1;
    const VALUE: u32 = 2;
    const PTR: u32 = 3;
    let status = |code: &mut Vec<I<'static>>, value: u32| {
        code.extend([
            I::LocalGet(PTR),
            I::I32Const(value as i32),
            I::I32Store(word(frame::STATUS_FIELD, false)),
        ]);
    };
    let done_without_body = |code: &mut Vec<I<'static>>| {
        code.extend([
            I::LocalGet(KIND),
            I::I32Const(Completion::Normal as i32),
            I::I32Eq,
            I::If(BlockType::Empty),
            I::I32Const(Out::DoneUndefined as i32),
            I::Return,
            I::End,
            I::LocalGet(KIND),
            I::I32Const(Completion::Return as i32),
            I::I32Eq,
            I::If(BlockType::Result(ValType::I32)),
            I::I32Const(Out::DoneInput as i32),
            I::Else,
            I::I32Const(Out::ThrowInput as i32),
            I::End,
            I::Return,
        ]);
    };
    let mut code: Vec<I<'static>> = vec![
        I::LocalGet(HANDLE),
        I::I32WrapI64,
        I::LocalSet(PTR),
        // Executing: re-entry is a TypeError in the caller.
        I::LocalGet(PTR),
        I::I32Load(word(frame::STATUS_FIELD, false)),
        I::I32Const(frame::STATUS_RUNNING as i32),
        I::I32Eq,
        I::If(BlockType::Empty),
        I::I32Const(Out::Running as i32),
        I::Return,
        I::End,
        I::LocalGet(PTR),
        I::I32Load(word(frame::STATUS_FIELD, false)),
        I::I32Const(frame::STATUS_COMPLETED as i32),
        I::I32Eq,
        I::If(BlockType::Empty),
    ];
    done_without_body(&mut code);
    code.extend([
        I::End,
        // Not started: `return`/`throw` complete it without entering the body.
        I::LocalGet(PTR),
        I::I32Load(word(frame::CONTROL_FIELD, true)),
        I::I32Const(frame::INITIAL_STATE as i32),
        I::I32Eq,
        I::LocalGet(KIND),
        I::I32Const(Completion::Normal as i32),
        I::I32Ne,
        I::I32And,
        I::If(BlockType::Empty),
    ]);
    status(&mut code, frame::STATUS_COMPLETED);
    done_without_body(&mut code);
    code.push(I::End);
    status(&mut code, frame::STATUS_RUNNING);
    code.extend([
        I::LocalGet(PTR),
        I::I64Const(0),
        I::I64Store(records::field(frame::COMPLETION_FIELD)),
        I::Block(BlockType::Empty),
    ]);
    for (layout, body) in layouts.iter().zip(bodies) {
        if layout.generator != Some(generator) {
            continue;
        }
        code.extend([
            I::LocalGet(PTR),
            I::I32Load(strings::WORD),
            I::I32Const(layout.id as i32),
            I::I32Eq,
            I::LocalGet(PTR),
            I::I32Load(word(frame::CONTROL_FIELD, false)),
            I::I32Const(layout.function as i32),
            I::I32Eq,
            I::I32And,
            I::If(BlockType::Empty),
            I::LocalGet(HANDLE),
            I::LocalGet(KIND),
            I::LocalGet(VALUE),
            I::Call(*body),
            I::Br(1),
            I::End,
        ]);
    }
    // A frame of another type or layout never reaches this step function.
    code.extend([
        I::Unreachable,
        I::End,
        I::GlobalGet(pending),
        I::If(BlockType::Empty),
    ]);
    status(&mut code, frame::STATUS_COMPLETED);
    code.extend([
        I::I32Const(Out::Returned as i32),
        I::Return,
        I::End,
        I::LocalGet(PTR),
        I::I32Load(word(frame::STATUS_FIELD, false)),
        I::I32Const(frame::STATUS_SUSPENDED as i32),
        I::I32Eq,
        I::If(BlockType::Empty),
        I::I32Const(Out::Yielded as i32),
        I::Return,
        I::End,
    ]);
    status(&mut code, frame::STATUS_COMPLETED);
    code.push(I::I32Const(Out::Returned as i32));
    let mut function = wasm_encoder::Function::new([(1, ValType::I32)]);
    for instruction in &code {
        function.instruction(instruction);
    }
    function.instruction(&I::End);
    function
}

impl Emitter<'_> {
    /// Push a generator step: the generator handle stays expression-rooted
    /// while the frame runs; the input travels as one packed slot.
    pub(super) fn generator_step(
        &mut self,
        generator: &Expr,
        completion: Completion,
        value: &Expr,
    ) -> Result<()> {
        let Type::Generator(id) = generator.ty else {
            return Err(CompilerError::Internal("step of a non-generator".into()));
        };
        let step = *self
            .generator_steps
            .get(id as usize)
            .ok_or_else(|| CompilerError::Internal("missing generator step".into()))?;
        self.expr(generator)?;
        self.code.push(I::I32Const(completion as i32));
        self.expr(value)?;
        self.pack_slot(value.ty);
        self.code.push(I::Call(step));
        self.check_exception()
    }
    /// The value in the frame's completion cell after the latest step.
    pub(super) fn generator_value(&mut self, generator: &Expr, ty: Type) -> Result<()> {
        self.expr(generator)?;
        self.code.extend([
            I::I32WrapI64,
            I::I64Load(records::field(frame::COMPLETION_FIELD)),
            I::I32WrapI64,
            I::I64Load(records::field(0)),
        ]);
        self.unpack_slot(ty);
        Ok(())
    }
}
