//! K4.4 foundation: compiler-internal suspension points executed in Wasmtime
//! through frame creators, forced collections and the closed `varta_resume`
//! dispatcher. Source reaches suspension only through the test lowering hook.
use crate::lower::suspension::hook;
use varta_common::function::frame::{self, Completion, FrameView, Outcome, ResumeToken};
use varta_common::language::LanguageProfile;
use varta_common::value::ValueKind;
use wasmtime::{Caller, Engine, Instance, Linker, Memory, Module, Store, Val};

const PROFILE: LanguageProfile = LanguageProfile::CheckedF64Utf16V31;
const CREATE: &str = "varta.test.frame.0";

pub(crate) fn compile(source: &str) -> Result<Vec<u8>, crate::CompilerError> {
    hook::with(|| crate::compile_typescript_with_profile(source, PROFILE))
}

#[derive(Default)]
struct Host {
    /// Token re-entered from the guest's `console.log(number)` callback.
    reenter: Option<i64>,
    /// Frame status observed inside that callback, and whether re-entry trapped.
    observed: Option<(u32, bool)>,
}
struct Frames {
    store: Store<Host>,
    instance: Instance,
    memory: Memory,
}
impl Frames {
    fn new(source: &str) -> Self {
        let bytes = compile(source).unwrap();
        let engine = Engine::default();
        let module = Module::new(&engine, &bytes).unwrap();
        let mut linker = Linker::new(&engine);
        linker
            .func_wrap(
                "varta",
                varta_common::HostOperation::LogFloatV2.name(),
                |mut caller: Caller<'_, Host>, _value: f64| {
                    let Some(token) = caller.data().reenter else {
                        return;
                    };
                    let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
                    let status = FrameView::read_envelope(
                        memory.data(&caller),
                        ResumeToken::from_bits(token).frame,
                    )
                    .unwrap()
                    .status;
                    let resume = caller
                        .get_export(frame::RESUME_EXPORT)
                        .unwrap()
                        .into_func()
                        .unwrap()
                        .typed::<(i64, i32, i64), i32>(&caller)
                        .unwrap();
                    let trapped = resume
                        .call(&mut caller, (token, Completion::Normal as i32, 0))
                        .is_err();
                    caller.data_mut().observed = Some((status, trapped));
                },
            )
            .unwrap();
        linker.define_unknown_imports_as_traps(&module).unwrap();
        let mut store = Store::new(&engine, Host::default());
        let instance = linker.instantiate(&mut store, &module).unwrap();
        let memory = instance.get_memory(&mut store, "memory").unwrap();
        Self {
            store,
            instance,
            memory,
        }
    }
    fn create(&mut self, args: &[Val]) -> i64 {
        self.create_layout(CREATE, args)
    }
    fn create_layout(&mut self, name: &str, args: &[Val]) -> i64 {
        let func = self.instance.get_func(&mut self.store, name).unwrap();
        let mut result = [Val::I64(0)];
        func.call(&mut self.store, args, &mut result).unwrap();
        result[0].unwrap_i64()
    }
    fn resume(&mut self, token: i64, kind: Completion, value: u64) -> wasmtime::Result<Outcome> {
        self.resume_raw(token, kind as i32, value)
    }
    fn resume_raw(&mut self, token: i64, kind: i32, value: u64) -> wasmtime::Result<Outcome> {
        let resume = self
            .instance
            .get_typed_func::<(i64, i32, i64), i32>(&mut self.store, frame::RESUME_EXPORT)?;
        let code = resume.call(&mut self.store, (token, kind, value as i64))?;
        Ok(Outcome::from_code(code).unwrap())
    }
    fn next(&mut self, token: i64, value: f64) -> Outcome {
        self.resume(token, Completion::Normal, value.to_bits())
            .unwrap()
    }
    fn view(&self, token: i64) -> FrameView {
        FrameView::read_envelope(
            self.memory.data(&self.store),
            ResumeToken::from_bits(token).frame,
        )
        .unwrap()
    }
    /// The token for the frame's current suspension generation.
    fn token(&self, token: i64) -> i64 {
        let frame = ResumeToken::from_bits(token).frame;
        self.view(token).token(frame).to_bits()
    }
    fn collect(&mut self) {
        self.instance
            .get_typed_func::<(), ()>(&mut self.store, crate::emit::TEST_COLLECT)
            .unwrap()
            .call(&mut self.store, ())
            .unwrap();
    }
    fn completion(&self, token: i64) -> u64 {
        self.view(token).completion.unwrap().value
    }
    fn number(&self, token: i64) -> f64 {
        f64::from_bits(self.completion(token))
    }
    fn word(&self, at: usize) -> u32 {
        u32::from_le_bytes(
            self.memory.data(&self.store)[at..at + 4]
                .try_into()
                .unwrap(),
        )
    }
    fn wide(&self, at: usize) -> u64 {
        u64::from_le_bytes(
            self.memory.data(&self.store)[at..at + 8]
                .try_into()
                .unwrap(),
        )
    }
    fn string(&self, handle: u64) -> String {
        let (ptr, len) = (handle as u32 as usize, (handle >> 32) as usize);
        let units: Vec<u16> = self.memory.data(&self.store)[ptr..ptr + len]
            .chunks(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        String::from_utf16(&units).unwrap()
    }
    /// Kind and payload of a boxed exception-union completion.
    fn thrown(&self, token: i64) -> (u32, u64) {
        let union = self.completion(token) as u32 as usize;
        (self.word(union), self.wide(union + 8))
    }
    fn pending(&mut self) -> i32 {
        self.instance
            .get_global(&mut self.store, varta_common::exceptions::PENDING)
            .unwrap()
            .get(&mut self.store)
            .unwrap_i32()
    }
}

pub(crate) fn catalog(bytes: &[u8]) -> varta_common::function::FunctionCatalog {
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let wasmparser::Payload::CustomSection(section) = payload.unwrap()
            && section.name() == varta_common::function::CATALOG_SECTION
        {
            return varta_common::function::FunctionCatalog::from_frame_bytes(section.data())
                .unwrap();
        }
    }
    panic!("missing function catalog");
}

fn program(body: &str) -> String {
    format!("{body}\nexport function main(): void {{}}")
}

#[test]
fn suspension_round_trips_values_and_completes() {
    let mut frames = Frames::new(&program(
        "function task(a: number, b: number): number {
           const first = __varta_suspend(a + b);
           const second = __varta_suspend(first * 2);
           return first + second + a;
         }",
    ));
    let token = frames.create(&[Val::F64(2f64.to_bits()), Val::F64(3f64.to_bits())]);
    let view = frames.view(token);
    assert_eq!(
        (view.state, view.status, view.completion),
        (frame::INITIAL_STATE, frame::STATUS_SUSPENDED, None)
    );
    assert_eq!(frames.next(token, 99.0), Outcome::Suspended);
    assert_eq!(frames.number(token), 5.0);
    let token = frames.token(token);
    assert_eq!(frames.view(token).state, 1);
    assert_eq!(frames.next(token, 10.0), Outcome::Suspended);
    assert_eq!(frames.number(token), 20.0);
    let token = frames.token(token);
    assert_eq!(frames.next(token, 7.0), Outcome::Returned);
    assert_eq!(frames.number(token), 19.0);
    assert_eq!(frames.view(token).status, frame::STATUS_COMPLETED);
    assert_eq!(frames.pending(), 0);
}

impl Frames {
    /// Resume with each input in turn; returns yields (as f64), the final
    /// outcome and the token whose frame holds the final completion.
    fn drive(&mut self, mut token: i64, inputs: &[u64]) -> (Vec<f64>, Outcome, i64) {
        let mut yields = Vec::new();
        for input in inputs {
            let outcome = self.resume(token, Completion::Normal, *input).unwrap();
            if outcome != Outcome::Suspended {
                return (yields, outcome, token);
            }
            assert_eq!(self.pending(), 0, "the scheduler holds an empty channel");
            yields.push(self.number(token));
            token = self.token(token);
        }
        (yields, Outcome::Suspended, token)
    }
}
fn numbers(values: &[f64]) -> Vec<u64> {
    values.iter().map(|value| value.to_bits()).collect()
}

#[test]
fn normalization_preserves_operand_order_and_already_evaluated_values() {
    let mut frames = Frames::new(&program(
        "function add3(a: number, b: number, c: number): number { return a * 100 + b * 10 + c; }
         function task(): number {
           const trace: number[] = [];
           let x = 1;
           const mark = (n: number): number => { trace.push(n); return n; };
           const set = (n: number): number => { x = n; return mark(n); };
           const r = add3(mark(x), __varta_suspend(set(2)), mark(x));
           let code = 0;
           for (const n of trace) { code = code * 10 + n; }
           return r * 10000 + code;
         }",
    ));
    let token = frames.create(&[]);
    let (yields, outcome, token) = frames.drive(token, &numbers(&[0.0, 7.0]));
    assert_eq!((yields, outcome), (vec![2.0], Outcome::Returned));
    // add3(1, 7, 2): the first argument kept its pre-suspension value, the
    // third was evaluated after resumption; marks ran once each, in order.
    assert_eq!(frames.number(token), 1_720_122.0);
}

#[test]
fn normalization_preserves_an_already_evaluated_callee() {
    let mut frames = Frames::new(&program(
        "function task(): number {
           let op = (a: number, b: number): number => a + b;
           const swap = (): number => { op = (a: number, b: number): number => a * b; return 0; };
           const r = op(3, __varta_suspend(swap()));
           return r + op(3, 4) * 1000;
         }",
    ));
    let token = frames.create(&[]);
    let (_, outcome, token) = frames.drive(token, &numbers(&[0.0, 4.0]));
    assert_eq!(outcome, Outcome::Returned);
    assert_eq!(frames.number(token), 12_007.0);
}

#[test]
fn short_circuit_and_conditional_operands_suspend_only_on_their_branch() {
    let source = program(
        "function task(flag: boolean): number {
           const both = flag && __varta_suspend(true, true);
           const either = flag || __varta_suspend(false, true);
           const picked = flag ? __varta_suspend(1) : 2;
           return (both ? 100 : 0) + (either ? 1000 : 0) + picked * 10;
         }",
    );
    let mut frames = Frames::new(&source);
    let token = frames.create(&[Val::I32(0)]);
    // `false && ...` skips; `false || ...` suspends; `?:` takes the else arm.
    let (yields, outcome, token) = frames.drive(token, &[0, 1, 0]);
    assert_eq!((yields, outcome), (vec![0.0], Outcome::Returned));
    assert_eq!(frames.number(token), 1020.0);
    let token = frames.create(&[Val::I32(1)]);
    let (yields, outcome, token) = frames.drive(token, &[0, 1, 9.0f64.to_bits()]);
    assert_eq!(yields.len(), 2);
    assert_eq!(outcome, Outcome::Returned);
    assert_eq!(frames.number(token), 1190.0);
}

#[test]
fn nullish_operand_suspends_only_for_nullish_left_values() {
    let mut frames = Frames::new(&program(
        "function task(flag: boolean): number {
           let missing: number | undefined = undefined;
           if (flag) { missing = 4; }
           const value = missing ?? __varta_suspend(3);
           return value;
         }",
    ));
    let token = frames.create(&[Val::I32(0)]);
    let (yields, outcome, token) = frames.drive(token, &numbers(&[0.0, 6.0]));
    assert_eq!((yields, outcome), (vec![3.0], Outcome::Returned));
    assert_eq!(frames.number(token), 6.0);
    let token = frames.create(&[Val::I32(1)]);
    let (yields, outcome, token) = frames.drive(token, &numbers(&[0.0]));
    assert_eq!((yields, outcome), (vec![], Outcome::Returned));
    assert_eq!(frames.number(token), 4.0);
}

#[test]
fn compound_assignments_prepare_receiver_and_old_value_before_suspension() {
    let mut frames = Frames::new(&program(
        "function task(): number {
           const o = { n: 1 };
           let count = 0;
           let s = 2;
           const touch = (): { n: number } => { count = count + 1; return o; };
           const store = (n: number): number => { o.n = n; return n; };
           const reset = (n: number): number => { s = n; return n; };
           touch().n += __varta_suspend(store(10));
           s *= __varta_suspend(reset(100));
           return o.n * 100000 + s * 10 + count;
         }",
    ));
    let token = frames.create(&[]);
    let (yields, outcome, token) = frames.drive(token, &numbers(&[0.0, 5.0, 3.0]));
    assert_eq!((yields, outcome), (vec![10.0, 100.0], Outcome::Returned));
    // o.n = 1 + 5 (old value read before the RHS store); s = 2 * 3.
    assert_eq!(frames.number(token), 600_061.0);
}

#[test]
fn loops_resume_inside_their_bodies_and_keep_per_iteration_cells() {
    let mut frames = Frames::new(&program(
        "function task(limit: number): number {
           let sum = 0;
           let first: () => number = () => 99;
           let last: () => number = () => 99;
           for (let i = 0; i < limit; i = i + 1) {
             if (i === 0) { first = () => i; }
             last = () => i;
             sum += __varta_suspend(i);
           }
           let j = 0;
           while (__varta_suspend(j * 100) > 0) { j = j + 1; }
           return sum * 100 + j * 10 + first() + last();
         }",
    ));
    let token = frames.create(&[Val::F64(3f64.to_bits())]);
    let inputs = numbers(&[0.0, 10.0, 20.0, 30.0, 1.0, 1.0, 0.0]);
    let (yields, outcome, token) = frames.drive(token, &inputs);
    assert_eq!(
        (yields, outcome),
        (vec![0.0, 1.0, 2.0, 0.0, 100.0, 200.0], Outcome::Returned)
    );
    // Each `let i` iteration kept its own captured cell across suspensions.
    assert_eq!(frames.number(token), 6022.0);
}

#[test]
fn captured_cells_are_shared_between_closures_and_the_frame() {
    let mut frames = Frames::new(&program(
        "function task(start: number): number {
           let count = start;
           const inc = (): number => { count = count + 1; return count; };
           inc();
           const step = __varta_suspend(count);
           inc();
           return count * 10 + step;
         }",
    ));
    let token = frames.create(&[Val::F64(4f64.to_bits())]);
    let (yields, outcome, token) = frames.drive(token, &numbers(&[0.0, 3.0]));
    assert_eq!((yields, outcome), (vec![5.0], Outcome::Returned));
    assert_eq!(frames.number(token), 63.0);
    let view = frames.view(token);
    let catalog = catalog(
        &compile(&program(
            "function task(start: number): number {
           let count = start;
           const inc = (): number => { count = count + 1; return count; };
           inc();
           const step = __varta_suspend(count);
           inc();
           return count * 10 + step;
         }",
        ))
        .unwrap(),
    );
    let layout = &catalog.frames[0];
    assert_eq!(layout.slots.len(), view.bindings.len());
    // `count` keeps its closure cell; the parameter and temporaries own theirs.
    assert!(!layout.slots[0].shared);
    assert_eq!(layout.slots.iter().filter(|slot| slot.shared).count(), 1);
}

#[test]
fn thrown_resumption_reaches_the_handler_active_at_the_suspension_point() {
    let source = program(
        "function task(fail: boolean): number {
           try {
             const value = __varta_suspend(1);
             if (fail) { throw new Error(\"inner\"); }
             return value;
           } catch (e) {
             return 7;
           }
         }",
    );
    let mut frames = Frames::new(&source);
    let token = frames.create(&[Val::I32(0)]);
    assert_eq!(frames.next(token, 0.0), Outcome::Suspended);
    let token = frames.token(token);
    let outcome = frames
        .resume(token, Completion::Throw, 42f64.to_bits())
        .unwrap();
    assert_eq!((outcome, frames.number(token)), (Outcome::Returned, 7.0));
    assert_eq!(frames.pending(), 0);
    let token = frames.create(&[Val::I32(1)]);
    let (_, outcome, token) = frames.drive(token, &numbers(&[0.0, 2.0]));
    assert_eq!((outcome, frames.number(token)), (Outcome::Returned, 7.0));
}

#[test]
fn uncaught_exceptions_move_into_the_frame_and_leave_the_channel_empty() {
    let mut frames = Frames::new(&program(
        "function task(): number {
           const value = __varta_suspend(1);
           return value;
         }",
    ));
    let token = frames.create(&[]);
    assert_eq!(frames.next(token, 0.0), Outcome::Suspended);
    let token = frames.token(token);
    let outcome = frames
        .resume(token, Completion::Throw, 42f64.to_bits())
        .unwrap();
    assert_eq!(outcome, Outcome::Threw);
    assert_eq!(frames.pending(), 0);
    assert_eq!(
        frames.thrown(token),
        (ValueKind::Float64 as u32, 42f64.to_bits())
    );
    assert_eq!(frames.view(token).status, frame::STATUS_COMPLETED);
    // A frame that never started completes without entering its body.
    let token = frames.create(&[]);
    let outcome = frames
        .resume(token, Completion::Throw, 5f64.to_bits())
        .unwrap();
    assert_eq!(
        (outcome, frames.thrown(token).1),
        (Outcome::Threw, 5f64.to_bits())
    );
    assert_eq!(frames.view(token).state, frame::INITIAL_STATE);
    let token = frames.create(&[]);
    let outcome = frames
        .resume(token, Completion::Return, 8f64.to_bits())
        .unwrap();
    assert_eq!((outcome, frames.number(token)), (Outcome::Returned, 8.0));
}

#[test]
fn return_resumption_runs_enclosing_finally_blocks() {
    let mut frames = Frames::new(&program(
        "function task(fail: boolean): number {
           try {
             __varta_suspend(1);
             return 0;
           } finally {
             if (fail) { throw 5; }
           }
         }",
    ));
    let token = frames.create(&[Val::I32(0)]);
    assert_eq!(frames.next(token, 0.0), Outcome::Suspended);
    let token = frames.token(token);
    let outcome = frames
        .resume(token, Completion::Return, 9f64.to_bits())
        .unwrap();
    assert_eq!((outcome, frames.number(token)), (Outcome::Returned, 9.0));
    let token = frames.create(&[Val::I32(1)]);
    assert_eq!(frames.next(token, 0.0), Outcome::Suspended);
    let token = frames.token(token);
    let outcome = frames
        .resume(token, Completion::Return, 9f64.to_bits())
        .unwrap();
    assert_eq!(outcome, Outcome::Threw);
    assert_eq!(
        frames.thrown(token),
        (ValueKind::Float64 as u32, 5f64.to_bits())
    );
}

const RETAINED: &str = "function task(): number {
   const kept = \"ab\".repeat(3);
   const list = [1, 2, 3];
   const error = new Error(\"kept \" + kept);
   let junk = \"\";
   for (let i = 0; i < 32; i = i + 1) { junk = junk + \"garbage!\"; }
   const got = __varta_suspend(kept);
   let more = \"\";
   for (let i = 0; i < 32; i = i + 1) { more = more + \"realloc!\"; }
   if (got > 0) { throw error; }
   return (kept === \"ababab\" ? 1 : 0) + list[2] * 10 + (error.message === \"kept ababab\" ? 100 : 0);
 }
 function churn(): number {
   let junk = \"\";
   for (let i = 0; i < 64; i = i + 1) { junk = junk + \"overwrite\"; }
   return __varta_suspend(junk.length);
 }";

#[test]
fn frames_keep_exact_roots_across_forced_collections() {
    let mut frames = Frames::new(&program(RETAINED));
    let token = frames.create(&[]);
    assert_eq!(frames.next(token, 0.0), Outcome::Suspended);
    let token = frames.token(token);
    let yielded = frames.completion(token);
    // Only the frame retains these values now; reclaim garbage and reuse it.
    frames.collect();
    let other = frames.create_layout(&format!("{}1", crate::emit::TEST_CREATE_PREFIX), &[]);
    assert_eq!(frames.next(other, 0.0), Outcome::Suspended);
    frames.collect();
    assert_eq!(frames.string(yielded), "ababab");
    assert_eq!(frames.next(token, 0.0), Outcome::Returned);
    assert_eq!(frames.number(token), 131.0);
}

#[test]
fn pending_exception_completion_survives_collection_until_read() {
    let mut frames = Frames::new(&program(RETAINED));
    let token = frames.create(&[]);
    assert_eq!(frames.next(token, 0.0), Outcome::Suspended);
    let token = frames.token(token);
    frames.collect();
    assert_eq!(frames.next(token, 1.0), Outcome::Threw);
    let (kind, error) = frames.thrown(token);
    assert_eq!(kind, ValueKind::Error as u32);
    let message = frames.wide(error as u32 as usize + 8);
    assert_eq!(frames.string(message), "kept ababab");
}

const TWO_STEPS: &str = "function task(flag: boolean): string {
   const first = __varta_suspend(1, true);
   const second = __varta_suspend(2, undefined);
   return first && flag ? \"both\" : \"one\";
 }";

fn trapped(result: wasmtime::Result<Outcome>) -> bool {
    result
        .unwrap_err()
        .downcast_ref::<wasmtime::Trap>()
        .is_some_and(|trap| *trap == wasmtime::Trap::UnreachableCodeReached)
}

#[test]
fn dispatcher_rejects_stale_completed_and_foreign_tokens() {
    let mut frames = Frames::new(&program(TWO_STEPS));
    let token = frames.create(&[Val::I32(1)]);
    assert_eq!(frames.next(token, 0.0), Outcome::Suspended);
    // A late completion for an earlier generation cannot resume the frame.
    assert!(trapped(frames.resume(token, Completion::Normal, 1)));
    let mut frames = Frames::new(&program(TWO_STEPS));
    let token = frames.create(&[Val::I32(1)]);
    let (_, outcome, done) = frames.drive(token, &[0, 1, 0]);
    assert_eq!(outcome, Outcome::Returned);
    assert_eq!(frames.string(frames.completion(done)), "both");
    let stale = frames.token(done);
    assert!(trapped(frames.resume(stale, Completion::Normal, 0)));
    let mut frames = Frames::new(&program(TWO_STEPS));
    let token = frames.create(&[Val::I32(1)]);
    let forged = ResumeToken {
        frame: ResumeToken::from_bits(token).frame + 8,
        generation: ResumeToken::from_bits(token).generation,
    };
    assert!(trapped(frames.resume(
        forged.to_bits(),
        Completion::Normal,
        0
    )));
    let mut frames = Frames::new(&program(TWO_STEPS));
    assert!(trapped(frames.resume(0, Completion::Normal, 0)));
}

#[test]
fn dispatcher_rejects_inadmissible_completions_and_inputs_before_effects() {
    for (kind, value, at_suspension) in [
        (3, 0, false),
        (-1, 0, false),
        // A Boolean input must be canonical, an undefined input zero.
        (Completion::Normal as i32, 2, true),
        // A string result admits no Return completion input.
        (Completion::Return as i32, 0, true),
    ] {
        let mut frames = Frames::new(&program(TWO_STEPS));
        let mut token = frames.create(&[Val::I32(1)]);
        if at_suspension {
            assert_eq!(frames.next(token, 0.0), Outcome::Suspended);
            token = frames.token(token);
        }
        let before = frames.view(token);
        assert!(
            trapped(frames.resume_raw(token, kind, value)),
            "{kind} {value}"
        );
        assert_eq!(frames.view(token), before, "rejection precedes any effect");
    }
    let mut frames = Frames::new(&program(TWO_STEPS));
    let token = frames.create(&[Val::I32(1)]);
    let (_, outcome, token) = frames.drive(token, &[0, 1]);
    assert_eq!(outcome, Outcome::Suspended);
    assert!(trapped(frames.resume(token, Completion::Normal, 1)));
}

#[test]
fn dispatcher_requires_an_empty_exception_channel() {
    let mut frames = Frames::new(&program(TWO_STEPS));
    let token = frames.create(&[Val::I32(1)]);
    let pending = frames
        .instance
        .get_global(&mut frames.store, varta_common::exceptions::PENDING)
        .unwrap();
    pending.set(&mut frames.store, Val::I32(1)).unwrap();
    assert!(trapped(frames.resume(token, Completion::Normal, 0)));
}

#[test]
fn running_frames_reject_reentrant_resumption() {
    let mut frames = Frames::new(&program(
        "function task(): number {
           const value = __varta_suspend(1);
           console.log(value);
           return value + 1;
         }",
    ));
    let token = frames.create(&[]);
    assert_eq!(frames.next(token, 0.0), Outcome::Suspended);
    let token = frames.token(token);
    frames.store.data_mut().reenter = Some(token);
    assert_eq!(frames.next(token, 4.0), Outcome::Returned);
    assert_eq!(
        frames.store.data().observed,
        Some((frame::STATUS_RUNNING, true))
    );
    assert_eq!(frames.number(token), 5.0);
}

#[test]
fn malformed_frames_and_saved_bindings_fail_closed() {
    let source = program(
        "function task(a: number): number {
           const kept = \"ab\".repeat(2);
           const value = __varta_suspend(a);
           return value + kept.length;
         }",
    );
    // Frame envelope: the exact edge mask is part of the managed kind.
    let mut frames = Frames::new(&source);
    let token = frames.create(&[Val::F64(1f64.to_bits())]);
    let ptr = ResumeToken::from_bits(token).frame as usize;
    let link = ptr - varta_common::abi::STRING_HEADER_BYTES + varta_common::value::HEAP_LINK_OFFSET;
    frames.memory.data_mut(&mut frames.store)[link] ^= 1;
    assert!(trapped(frames.resume(token, Completion::Normal, 0)));
    // Saved binding: a wrong private type id is caught before restoration.
    let mut frames = Frames::new(&source);
    let token = frames.create(&[Val::F64(1f64.to_bits())]);
    let cell = frames.view(token).bindings[0] as u32 as usize;
    frames.memory.data_mut(&mut frames.store)[cell] ^= 1;
    assert!(trapped(frames.resume(token, Completion::Normal, 0)));
    // Saved state outside the layout's states is rejected by the dispatcher.
    let mut frames = Frames::new(&source);
    let token = frames.create(&[Val::F64(1f64.to_bits())]);
    let state = ptr + varta_common::value::RECORD_META_BYTES + 4;
    frames.memory.data_mut(&mut frames.store)[state] = 9;
    assert!(trapped(frames.resume(token, Completion::Normal, 0)));
}

#[test]
fn live_frames_have_their_own_budget() {
    let mut frames = Frames::new(&program(TWO_STEPS));
    for _ in 0..frame::MAX_LIVE_FRAMES {
        frames.create(&[Val::I32(0)]);
    }
    let create = frames.instance.get_func(&mut frames.store, CREATE).unwrap();
    let mut result = [Val::I64(0)];
    let error = create
        .call(&mut frames.store, &[Val::I32(0)], &mut result)
        .unwrap_err();
    assert_eq!(
        error.downcast_ref::<wasmtime::Trap>(),
        Some(&wasmtime::Trap::UnreachableCodeReached)
    );
}

#[test]
fn completed_frames_release_their_root_table_slot() {
    let mut frames = Frames::new(&program(
        "function task(): number { return __varta_suspend(1); }",
    ));
    for _ in 0..frame::MAX_LIVE_FRAMES * 2 {
        let token = frames.create(&[]);
        let outcome = frames
            .resume(token, Completion::Return, 3f64.to_bits())
            .unwrap();
        assert_eq!((outcome, frames.number(token)), (Outcome::Returned, 3.0));
        frames.collect();
    }
}

#[test]
fn frame_catalog_is_versioned_and_older_readers_reject_it() {
    let bytes = compile(&program(TWO_STEPS)).unwrap();
    let catalog = catalog(&bytes);
    assert_eq!(catalog.version, frame::FRAME_CATALOG_VERSION);
    let layout = &catalog.frames[0];
    let inputs: Vec<_> = layout.states.iter().map(|state| state.input).collect();
    assert_eq!(
        inputs,
        [frame::FrameInput::Boolean, frame::FrameInput::Undefined]
    );
    assert_eq!(layout.return_input, None);
    assert!(layout.result.is_some());
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        if let wasmparser::Payload::CustomSection(section) = payload.unwrap()
            && section.name() == varta_common::function::CATALOG_SECTION
        {
            assert!(varta_common::function::FunctionCatalog::from_bytes(section.data()).is_err());
        }
    }
    // Ordinary modules keep their exact earlier surface.
    let plain = compile(&program("function task(): number { return 1; }")).unwrap();
    let plain_without_hook = crate::compile_typescript_with_profile(
        &program("function task(): number { return 1; }"),
        PROFILE,
    )
    .unwrap();
    assert_eq!(plain, plain_without_hook);
    let module = Module::new(&Engine::default(), &plain).unwrap();
    assert!(module.get_export(frame::RESUME_EXPORT).is_none());
}
