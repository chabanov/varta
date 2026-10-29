//! v35 `frame_liveness`: per-state saved sets, liveness segments with exact
//! GC maps, and their checks, executed in Wasmtime through the test hook.
use crate::suspension_tests::{Frames, catalog, compile, program, trapped, with_profile};
use varta_common::function::frame::{self, Completion, Outcome, ResumeToken};
use varta_common::function::{CaptureType, FunctionCatalog};
use varta_common::language::LanguageProfile;
use wasmtime::Val;

const V34: LanguageProfile = LanguageProfile::CheckedF64Utf16V34;
const V35: LanguageProfile = LanguageProfile::CheckedF64Utf16V35;

fn v35<T>(test: impl FnOnce() -> T) -> T {
    let mut result = None;
    with_profile(V35, || result = Some(test()));
    result.unwrap()
}
fn kind(catalog: &FunctionCatalog, binding: u32) -> &CaptureType {
    &catalog.types[catalog.bindings[binding as usize].ty as usize]
}

/// `t` is needed only after the first point and `u` only after the second;
/// the parameters are dead after entry, so each state saves one value.
const NARROW: &str = "function task(a: number, s: string): number {
   const t = s + \"!\";
   const first = __varta_suspend(a);
   const u = t.length + first;
   const second = __varta_suspend(u);
   return u + second;
 }";

#[test]
fn each_state_saves_only_the_locals_live_across_it() {
    let catalog = v35(|| catalog(&compile(&program(NARROW)).unwrap()));
    assert_eq!(catalog.version, frame::FRAME_LIVENESS_CATALOG_VERSION);
    let layout = &catalog.frames[0];
    // State 0 holds the parameters the creator binds.
    assert_eq!(layout.slots.len(), 2);
    let saved: Vec<Vec<&CaptureType>> = layout
        .states
        .iter()
        .map(|state| {
            state
                .saved
                .iter()
                .map(|s| kind(&catalog, s.binding))
                .collect()
        })
        .collect();
    assert!(
        matches!(saved.as_slice(), [one, two]
            if matches!(one.as_slice(), [CaptureType::String])
            && matches!(two.as_slice(), [CaptureType::Float64])),
        "{saved:?}"
    );
    assert_eq!(layout.capacity, Some(2));
    // The cell layout of the same function saves every local at every point.
    let cells = catalog_for(&program(NARROW), V34);
    assert_eq!(cells.version, frame::FRAME_CATALOG_VERSION);
    assert!(cells.frames[0].slots.len() > 4);
    assert!(cells.frames[0].states.iter().all(|s| s.saved.is_empty()));
    assert_eq!(cells.frames[0].capacity, None);
}

fn catalog_for(source: &str, profile: LanguageProfile) -> FunctionCatalog {
    let mut result = None;
    with_profile(profile, || {
        result = Some(catalog(&compile(source).unwrap()))
    });
    result.unwrap()
}

#[test]
fn liveness_segments_publish_the_gc_map_of_the_suspended_state_only() {
    v35(|| {
        let mut frames = Frames::new(&program(NARROW));
        let token = frames.create(&[
            Val::F64(2f64.to_bits()),
            Val::I64(0), // the static empty string
        ]);
        let view = frames.view(token);
        assert!(view.direct);
        // State 0: the String parameter is the only reference entry.
        assert_eq!(view.edges, [0b10]);
        assert_eq!(frames.next(token, 0.0), Outcome::Suspended);
        let token = frames.token(token);
        // State 1 saves `t` (a String) in entry 0.
        assert_eq!(frames.view(token).edges, [0b1]);
        frames.collect();
        assert_eq!(frames.next(token, 5.0), Outcome::Suspended);
        let token = frames.token(token);
        // State 2 saves `u` (a Number): no reference is retained.
        assert_eq!(frames.view(token).edges, [0]);
        assert_eq!(frames.view(token).state, 2);
        assert_eq!(frames.next(token, 10.0), Outcome::Returned);
        // `t` was "!" (length 1): u = 1 + 5, result = 6 + 10.
        assert_eq!(frames.number(token), 16.0);
        // A completed frame retains nothing through its segments.
        let view = frames.view(token);
        assert_eq!(view.status, frame::STATUS_COMPLETED);
        assert!(view.edges.iter().all(|mask| *mask == 0));
    });
}

/// `label` is read only by the handler and `tag` only by the finalizer, so
/// they are live across the points through the throw and return
/// continuations, and survive a forced collection there.
const HANDLERS: &str = "function caught(): number {
   const label = \"h\".repeat(3);
   try {
     const x = __varta_suspend(1);
     return x;
   } catch (e) {
     return label.length * 100;
   }
 }
 function finished(): number {
   const tag = \"ab\".repeat(3);
   try {
     return __varta_suspend(1);
   } finally {
     if (tag.length !== 6) {
       throw new Error(\"lost\");
     }
   }
 }";

#[test]
fn handler_and_finalizer_values_stay_live_across_points() {
    v35(|| {
        let mut frames = Frames::new(&program(HANDLERS));
        let token = frames.create(&[]);
        assert_eq!(frames.next(token, 0.0), Outcome::Suspended);
        let token = frames.token(token);
        frames.collect();
        let outcome = frames
            .resume(token, Completion::Throw, 7f64.to_bits())
            .unwrap();
        assert_eq!((outcome, frames.number(token)), (Outcome::Returned, 300.0));
        let finished = format!("{}1", crate::emit::TEST_CREATE_PREFIX);
        let token = frames.create_layout(&finished, &[]);
        assert_eq!(frames.next(token, 0.0), Outcome::Suspended);
        let token = frames.token(token);
        frames.collect();
        let outcome = frames
            .resume(token, Completion::Return, 9f64.to_bits())
            .unwrap();
        assert_eq!((outcome, frames.number(token)), (Outcome::Returned, 9.0));
    });
    let catalog = catalog_for(&program(HANDLERS), V35);
    for layout in &catalog.frames {
        let saved = &layout.states[0].saved;
        assert!(
            saved
                .iter()
                .any(|slot| matches!(kind(&catalog, slot.binding), CaptureType::String)),
            "{layout:?}"
        );
    }
}

#[test]
fn closure_cells_stay_shared_with_liveness_frames() {
    let source = program(
        "function task(start: number): number {
           let count = start;
           const inc = (): number => { count = count + 1; return count; };
           inc();
           const step = __varta_suspend(count);
           inc();
           return count * 10 + step;
         }",
    );
    v35(|| {
        let mut frames = Frames::new(&source);
        let token = frames.create(&[Val::F64(4f64.to_bits())]);
        let (yields, outcome, token) =
            frames.drive(token, &crate::suspension_tests::numbers(&[0.0, 3.0]));
        assert_eq!((yields, outcome), (vec![5.0], Outcome::Returned));
        assert_eq!(frames.number(token), 63.0);
    });
    let catalog = catalog_for(&source, V35);
    // The point saves `count`'s closure cell (one shared entry) and the closure.
    let saved = &catalog.frames[0].states[0].saved;
    assert_eq!(
        saved.iter().filter(|slot| slot.shared).count(),
        1,
        "{saved:?}"
    );
    assert!(!catalog.frames[0].slots[0].shared);
}

#[test]
fn tampered_liveness_segments_fail_closed() {
    let source = program(
        "function task(a: number, flag: boolean): number {
           const kept = \"ab\".repeat(2);
           const value = __varta_suspend(a);
           return flag ? value + kept.length : value;
         }",
    );
    v35(|| {
        let segment = |frames: &Frames, token: i64| -> usize {
            let ptr = ResumeToken::from_bits(token).frame as usize;
            let table = frames.wide(ptr + varta_common::value::RECORD_META_BYTES + 16) as u32;
            frames.wide(table as usize + varta_common::value::RECORD_META_BYTES) as u32 as usize
        };
        let suspended = |frames: &mut Frames| -> i64 {
            let token = frames.create(&[Val::F64(1f64.to_bits()), Val::I32(1)]);
            assert_eq!(frames.next(token, 0.0), Outcome::Suspended);
            frames.token(token)
        };
        let header = varta_common::abi::STRING_HEADER_BYTES;
        // The edge mask must equal the state's GC map.
        let mut frames = Frames::new(&source);
        let token = suspended(&mut frames);
        let link = segment(&frames, token) - header + varta_common::value::HEAP_LINK_OFFSET;
        frames.memory.data_mut(&mut frames.store)[link] ^= 0b100;
        assert!(trapped(frames.resume(token, Completion::Normal, 0)));
        // The segment kind is exact.
        let mut frames = Frames::new(&source);
        let token = suspended(&mut frames);
        let at = segment(&frames, token);
        frames.memory.data_mut(&mut frames.store)[at] = frame::FRAME_SEGMENT_KIND as u8;
        assert!(trapped(frames.resume(token, Completion::Normal, 0)));
        // A saved Boolean must stay canonical.
        let mut frames = Frames::new(&source);
        let token = suspended(&mut frames);
        let entries = segment(&frames, token) + varta_common::value::RECORD_META_BYTES;
        let flag = (0..2)
            .map(|index| entries + index * 8)
            .find(|at| frames.wide(*at) == 1)
            .expect("the saved flag");
        frames.memory.data_mut(&mut frames.store)[flag] = 2;
        assert!(trapped(frames.resume(token, Completion::Normal, 0)));
        // Untouched, the same frame completes.
        let mut frames = Frames::new(&source);
        let token = suspended(&mut frames);
        frames.collect();
        assert_eq!(frames.next(token, 3.0), Outcome::Returned);
        assert_eq!(frames.number(token), 7.0);
    });
}

#[test]
fn liveness_modules_validate_and_keep_the_frame_budget_per_state() {
    // Many locals, few live at once: the cell layout would need a slot for
    // every local, a liveness frame only for the widest state.
    let mut body = String::from("function task(a: number): number {\n let total = 0;\n");
    for index in 0..300 {
        body.push_str(&format!(
            " const v{index} = a + {index};\n total = total + v{index};\n"
        ));
    }
    body.push_str(" const resumed = __varta_suspend(total);\n return resumed + total;\n}");
    let source = program(&body);
    let bytes = v35(|| compile(&source).unwrap());
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    let catalog = catalog(&bytes);
    assert!(catalog.frames[0].capacity.unwrap() <= 2);
    let mut cells = None;
    with_profile(V34, || cells = Some(compile(&source)));
    let error = cells.unwrap().unwrap_err().to_string();
    assert!(error.contains("continuation frame budget"), "{error}");
}
