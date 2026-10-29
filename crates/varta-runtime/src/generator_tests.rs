//! K4.4 step 2: synchronous generators (v33 `generators`) through the runtime.
//! Expected logs were produced by Node 24 for the same programs; they are
//! pinned here, so the tests need no Node installation (differential oracle).
use super::*;
use varta_common::language::LanguageProfile;

const V33: LanguageProfile = LanguageProfile::CheckedF64Utf16V33;

fn compile_with(source: &str, profile: LanguageProfile) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{e}\n{source}"))
}
/// Every program runs with the v33 cell frames and the v35 liveness frames
/// (`frame_liveness`); both must produce the pinned Node output.
fn run(source: &str, expected: &[&str]) {
    for profile in [V33, LanguageProfile::CheckedF64Utf16V35] {
        let bytes = compile_with(source, profile);
        wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap_or_else(|e| panic!("{profile:?} {e:?}\n{source}"))
                .lines,
            expected
                .iter()
                .map(|s| format!("varta log: {s}"))
                .collect::<Vec<_>>(),
            "{profile:?}\n{source}"
        );
    }
}

// Pinned Node 24 outputs (generated).
#[test]
fn generator_protocol_matches_node_in_every_state() {
    run(
        include_str!("generator_cases/protocol.ts"),
        &[
            "n0 false first",
            "n1 false got x",
            "n2 false got y",
            "n3 true 7",
            "n4 true undefined",
            "r5 true 9",
            "start-return true early",
            "after true undefined",
            "t0 false 1",
            "t-return true mid",
            "t1 true undefined",
            "not started threw boom",
            "u0 true undefined",
            "v0 false 1",
            "uncaught in generator at yield",
            "v1 true undefined",
            "completed threw 42",
        ],
    );
}
#[test]
fn generator_try_catch_finally_ordering_matches_node() {
    run(
        include_str!("generator_cases/finally.ts"),
        &[
            "false:a",
            "false:in finally",
            "true:undefined",
            "true:undefined",
            "enter,cleanup,finally done",
            "false:try",
            "false:caught x",
            "false:still in catch",
            "false:after",
            "true:undefined",
            "false:try",
            "false:caught y",
            "true:undefined",
            "true:undefined",
            "false:1",
            "false:2",
            "true:undefined",
            "inner finally,outer catch e1,outer finally",
        ],
    );
}
#[test]
fn generator_for_of_closing_matches_node() {
    run(
        include_str!("generator_cases/for_of.ts"),
        &[
            "sum 5 yield 0,yield 1,yield 2,yield 3,closed",
            "break yield 0,yield 1,yield 2,closed",
            "return 2 yield 0,yield 1,yield 2,closed",
            "throw body failure 1 yield 0,yield 1,closed",
            "generator threw inner failure saw 1,about to fail",
            "labels yield 0,yield 0,yield 1,closed,yield 1,yield 0,yield 1,closed,yield 2,yield 0,closed,closed",
            "nested 00,01,10,11 yield 0,yield 0,yield 1,closed,yield 1,yield 0,yield 1,closed,closed",
        ],
    );
}
#[test]
fn generator_references_switch_closures_generics_and_reentry_match_node() {
    run(
        include_str!("generator_cases/values.ts"),
        &[
            "p0+q0,p2+q2,p4+q4 pages 450",
            "switch 1 one,two,end",
            "switch 2 two,end",
            "switch 3 other,end",
            "closures 1,11,11",
            "generic hi,hi 3",
            "a",
            "TypeError Generator is already running",
            "true",
        ],
    );
}
#[test]
fn generator_frames_outlive_the_live_table_and_are_collected() {
    run(
        include_str!("generator_cases/lifetime.ts"),
        &["dropped 100", "nested 134", "a-3:0:56 b-3:1:56 b-3:2:56"],
    );
}
#[test]
fn generator_language_guide_examples_match_node() {
    run(
        include_str!("generator_cases/guide.ts"),
        &[
            "page 0", "page 2", "closed", "true 9", "true", "0", "1", "cleanup",
        ],
    );
}
