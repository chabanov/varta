//! K4.4 step 5: timers on the runtime event loop (v36 `timers`).
//! Expected logs were produced by Node 24 for the same programs (each case
//! with `main()` appended); they are pinned here, so the tests need no Node
//! installation. Unless a test says otherwise it runs on the test-only virtual
//! clock, which advances loop time without sleeping, so no test depends on
//! wall-clock timing.
use super::*;
use varta_common::language::LanguageProfile;
use varta_common::scheduler::{MAX_PENDING_TIMERS, SchedulerError, TIMER_CAPABILITY};

const V36: LanguageProfile = LanguageProfile::CheckedF64Utf16V36;

fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, V36)
        .unwrap_or_else(|e| panic!("{e}\n{source}"))
}
fn grant(id: &str) -> CapabilityGrant {
    CapabilityGrant {
        id: id.into(),
        value: varta_common::CapabilityValue::Boolean(true),
    }
}
fn timer_grant() -> Vec<CapabilityGrant> {
    vec![grant(TIMER_CAPABILITY)]
}
fn execute(source: &str) -> Result<RunOutput, RuntimeError> {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    scheduler::with_virtual_clock(|| VartaRuntime::new().run_wasm_bytes(&bytes, &timer_grant()))
}
fn lines(expected: &[&str]) -> Vec<String> {
    expected.iter().map(|s| format!("varta log: {s}")).collect()
}
fn run(source: &str, expected: &[&str]) {
    let output = execute(source).unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    assert_eq!(output.lines, lines(expected), "{source}");
}
/// The log written before the execution ended, and how it ended. With
/// `collect`, the program is built with the test-only collector export and a
/// full collection is forced before every job.
fn observe(
    source: &str,
    grants: &[CapabilityGrant],
    collect: bool,
) -> (Vec<String>, Result<(), RuntimeError>) {
    use varta_compiler::suspension_test_hook as hook;
    let bytes = if collect {
        hook::compile_with_profile(source, V36).unwrap_or_else(|e| panic!("{e}\n{source}"))
    } else {
        compile(source)
    };
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&bytes).unwrap();
    let mut execution = execution::Execution::new(&runtime.engine, grants, None).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    let result = scheduler::with_virtual_clock(|| {
        let start = instance.get_func(&mut *store, "_start").unwrap();
        start.call(&mut *store, &[], &mut [])?;
        exceptions::finish(&instance, store)?;
        if !collect {
            return scheduler::checkpoint(&instance, store);
        }
        let collector = instance
            .get_typed_func::<(), ()>(&mut *store, hook::COLLECT)
            .unwrap();
        let resume = instance
            .get_typed_func::<(i64, i32, i64), i32>(
                &mut *store,
                varta_common::function::frame::RESUME_EXPORT,
            )
            .unwrap();
        loop {
            while let Some(token) = store.data_mut().scheduler.next().unwrap() {
                // Test-only budget: each forced full collection is paid for
                // outside the execution's production fuel.
                let remaining = store.get_fuel().unwrap();
                store.set_fuel(abi::FUEL).unwrap();
                collector.call(&mut *store, ()).unwrap();
                store.set_fuel(remaining).unwrap();
                resume.call(&mut *store, (token, 0, 0))?;
            }
            assert!(store.data().scheduler.unhandled_rejection().is_none());
            if store.data().scheduler.next_due().is_none() {
                return Ok(());
            }
            store.data_mut().scheduler.fire().unwrap();
        }
    })
    .map_err(scheduler::typed);
    (store.data().output.lines.clone(), result)
}

#[test]
fn timer_order_matches_node() {
    run(
        include_str!("timer_cases/ordering.ts"),
        &[
            "main start",
            "main end",
            "microtask of main",
            "timeout 0 first",
            "microtask of first",
            "timeout 0 second",
            "args 2",
            "timeout 10",
            "nested 10+5",
            "timeout 20",
            "nested 10+15",
        ],
    );
}

/// The conformance inventory's timer case (a known gap under its v35 pin)
/// runs under v36 with a `time.timer` grant.
#[test]
fn the_conformance_timer_case_runs_under_v36() {
    run(
        include_str!("../../../conformance/cases/timers-event-loop.ts"),
        &[
            "main end",
            "microtask of main",
            "timeout 0 first",
            "microtask of first",
            "timeout 0 second",
            "slept 5",
            "slept 1",
        ],
    );
}

#[test]
fn sleep_retry_backoff_and_intervals_match_node() {
    run(
        include_str!("timer_cases/sleep.ts"),
        &[
            "start",
            "slow,fast",
            "attempt 1 failed, retry in 75",
            "attempt 2 failed, retry in 150",
            "attempt 3 failed, retry in 188",
            "result 4",
            "tick 1",
            "tick 2",
            "tick 3",
            "ticks 3",
            "other interval",
            "other interval",
            "other interval",
            "async callback",
            "async callback resumed",
            "end",
        ],
    );
}

/// `AbortSignal.timeout` (engine prelude): Node's delay validation, a
/// `TimeoutError` reason once the delay elapsed, and a timer that does not
/// keep the event loop alive, so the program ends without waiting for the
/// last signal (whose delay is far beyond the deadline).
#[test]
fn abort_signal_timeout_matches_node() {
    run(
        include_str!("timer_cases/abort_timeout.ts"),
        &[
            "RangeError: The value of \"delay\" is out of range. It must be >= 0 && <= 4294967295. Received -1",
            "RangeError: The value of \"delay\" is out of range. It must be an integer. Received 1.5",
            "RangeError: The value of \"delay\" is out of range. It must be >= 0 && <= 4294967295. Received 4294967296",
            "aborted false",
            "at 10 false",
            "at 30 true",
            "TimeoutError The operation was aborted due to timeout 23 true",
            "thrown TimeoutError",
            "idle false",
        ],
    );
}

/// Callbacks, their captures and their extra arguments are reachable only
/// from the pending timers' frames; the log holds with a collection forced
/// before every job and through the ordinary event loop.
#[test]
fn timer_callbacks_survive_collections_forced_before_every_job() {
    let expected = [
        "item-1:a:item-0/item-2",
        "item-1:b:item-0/item-2",
        "item-2:a:item-0/item-2",
        "item-2:b:item-0/item-2",
        "a:item-0/item-2!item-93item-7?",
        "b:item-0/item-2!item-93item-7?",
    ];
    let source = include_str!("timer_cases/lifetime.ts");
    let (log, result) = observe(source, &timer_grant(), true);
    result.unwrap();
    assert_eq!(log, lines(&expected));
    run(source, &expected);
}

/// Node 24 prints "first" and "throws", then terminates with the uncaught
/// `RangeError: timer failed`; the third timer never runs. Varta reports the
/// exception like one escaping `main`.
#[test]
fn an_exception_escaping_a_timer_callback_ends_the_execution() {
    let (log, result) = observe(
        r#"
export function main(): void {
  setTimeout(() => console.log("first"), 1);
  setTimeout(() => {
    console.log("throws");
    throw new RangeError("timer failed");
  }, 2);
  setTimeout(() => console.log("never"), 3);
}
"#,
        &timer_grant(),
        false,
    );
    assert_eq!(log, lines(&["first", "throws"]));
    let error = result.unwrap_err();
    assert!(matches!(error, RuntimeError::Uncaught(_)), "{error:?}");
    assert!(error.to_string().contains("RangeError"), "{error}");
    assert!(error.to_string().contains("timer failed"), "{error}");
}

/// Node 24 prints "main", then terminates with `ERR_UNHANDLED_REJECTION`
/// for "late" before any timer runs: rejections are checked after every
/// microtask checkpoint, so a handler added by a later timer is too late.
#[test]
fn unhandled_rejections_are_checked_before_the_next_timer() {
    let (log, result) = observe(
        r#"
export function main(): void {
  const p = Promise.reject<number>("late");
  setTimeout(() => {
    p.catch(() => console.log("too late"));
  }, 1);
  console.log("main");
}
"#,
        &timer_grant(),
        false,
    );
    assert_eq!(log, lines(&["main"]));
    let Err(RuntimeError::Scheduler(SchedulerError::UnhandledRejection { count: 1, reason })) =
        result
    else {
        panic!("{result:?}");
    };
    assert_eq!(reason.to_string(), "\"late\"");
    // A rejection made by a timer callback ends the execution after that
    // callback's microtasks, before the next timer.
    let (log, result) = observe(
        r#"
export function main(): void {
  setTimeout(() => {
    console.log("timer rejects");
    Promise.reject<number>("in timer");
    Promise.resolve(1).then(() => console.log("its microtask"));
  }, 1);
  setTimeout(() => console.log("never"), 2);
}
"#,
        &timer_grant(),
        false,
    );
    assert_eq!(log, lines(&["timer rejects", "its microtask"]));
    assert!(
        matches!(
            result,
            Err(RuntimeError::Scheduler(
                SchedulerError::UnhandledRejection { .. }
            ))
        ),
        "{result:?}"
    );
}

/// Arming a timer requires the exact `time.timer` grant; without it (or
/// when a host ceiling withholds it) the execution ends at the call, before
/// any timer exists and before any later guest effect. Clearing needs no
/// grant, and `time.now` does not authorize timers.
#[test]
fn timers_are_denied_by_default_before_any_effect() {
    let source = r#"
export function main(): void {
  console.log("before");
  setTimeout(() => console.log("callback"), 1);
  console.log("after");
}
"#;
    for grants in [vec![], vec![grant("time.now")]] {
        let (log, result) = observe(source, &grants, false);
        assert_eq!(log, lines(&["before"]));
        let error = result.unwrap_err();
        assert!(
            matches!(error, RuntimeError::Scheduler(SchedulerError::TimerDenied)),
            "{error:?}"
        );
        assert!(
            error
                .to_string()
                .starts_with("<capability denied: time.timer>"),
            "{error}"
        );
    }
    let bytes = compile(source);
    let runtime = VartaRuntime::new();
    // A host ceiling without the grant denies what the package requested.
    let error = runtime
        .run_wasm_bytes_with_policy(&bytes, &timer_grant(), &[])
        .unwrap_err();
    assert!(
        matches!(error, RuntimeError::Scheduler(SchedulerError::TimerDenied)),
        "{error:?}"
    );
    // Both the package request and the ceiling grant it.
    let output = scheduler::with_virtual_clock(|| {
        runtime.run_wasm_bytes_with_policy(&bytes, &timer_grant(), &timer_grant())
    })
    .unwrap();
    assert_eq!(output.lines, lines(&["before", "after", "callback"]));
    // Clearing is bookkeeping and takes no capability.
    let output = VartaRuntime::new()
        .run_wasm_bytes(
            &compile("export function main(): void {\n  clearTimeout(1);\n  clearInterval(undefined);\n  console.log(\"cleared\");\n}"),
            &[],
        )
        .unwrap();
    assert_eq!(output.lines, lines(&["cleared"]));
}

/// A timer due at or beyond the execution deadline can never fire: the
/// execution ends at once with a budget error instead of waiting. One due
/// just before the deadline still fires, and an interval that is never
/// cleared runs until its next firing would pass the deadline.
#[test]
fn timers_beyond_the_deadline_end_the_execution_with_a_budget_error() {
    let deadline = abi::DEADLINE_SECONDS * 1000;
    let (log, result) = observe(
        &format!(
            "export function main(): void {{\n  setTimeout(() => console.log(\"late\"), {deadline});\n  setTimeout(() => console.log(\"in time\"), {});\n}}",
            deadline - 1
        ),
        &timer_grant(),
        false,
    );
    assert_eq!(log, lines(&["in time"]));
    let error = result.unwrap_err();
    assert!(
        matches!(
            error,
            RuntimeError::Scheduler(SchedulerError::TimerDeadline { due_ms, deadline_seconds })
                if due_ms == deadline && deadline_seconds == abi::DEADLINE_SECONDS
        ),
        "{error:?}"
    );
    assert!(
        error.to_string().starts_with("execution deadline exceeded"),
        "{error}"
    );
    let (log, result) = observe(
        "export function main(): void {\n  let n = 0;\n  setInterval(() => {\n    n = n + 1;\n    console.log(\"tick \" + String(n));\n  }, 1500);\n}",
        &timer_grant(),
        false,
    );
    assert_eq!(log, lines(&["tick 1", "tick 2", "tick 3"]));
    assert!(
        matches!(
            result,
            Err(RuntimeError::Scheduler(SchedulerError::TimerDeadline {
                due_ms: 6000,
                ..
            }))
        ),
        "{result:?}"
    );
}

/// At most `MAX_PENDING_TIMERS` timers are pending at once; clearing a timer
/// releases its slot once its frame ran in the next microtask checkpoint.
#[test]
fn pending_timers_are_bounded() {
    let (_, result) = observe(
        &format!(
            "export function main(): void {{\n  for (let i = 0; i <= {MAX_PENDING_TIMERS}; i++) {{\n    setTimeout(() => console.log(\"x\"), 1);\n  }}\n}}"
        ),
        &timer_grant(),
        false,
    );
    assert!(
        matches!(
            result,
            Err(RuntimeError::Scheduler(SchedulerError::TimerBudget))
        ),
        "{result:?}"
    );
    // A loop that sets and clears one timer per iteration, awaiting between
    // iterations, never holds more than one.
    run(
        &format!(
            "export async function main(): Promise<void> {{\n  let fired = 0;\n  for (let i = 0; i < {}; i++) {{\n    const t = setTimeout(() => {{ fired = fired + 1; }}, 1000);\n    clearTimeout(t);\n    await null;\n  }}\n  setTimeout(() => console.log(\"fired \" + String(fired)), 1);\n}}",
            MAX_PENDING_TIMERS * 4
        ),
        &["fired 0"],
    );
}

/// Timer callbacks run guest code under the execution's single fuel budget;
/// waiting is free, but callbacks are not.
#[test]
fn timer_callbacks_consume_the_execution_fuel() {
    let error = execute(
        "export function main(): void {\n  setInterval(() => {\n    let x = 0;\n    for (let i = 0; i < 20000; i++) {\n      x = x + i;\n    }\n  }, 1);\n}",
    )
    .unwrap_err();
    assert!(error.to_string().contains("fuel"), "{error}");
}

/// `process.exit` inside a timer callback ends the execution with its code;
/// later timers never run.
#[test]
fn process_exit_inside_a_timer_callback_ends_the_execution() {
    let bytes = compile(
        "export function main(): void {\n  setTimeout(() => {\n    console.log(\"exiting\");\n    varta.process.exit(3);\n  }, 1);\n  setTimeout(() => console.log(\"never\"), 2);\n}",
    );
    let mut grants = timer_grant();
    grants.push(grant("process.exit"));
    let output =
        scheduler::with_virtual_clock(|| VartaRuntime::new().run_wasm_bytes(&bytes, &grants))
            .unwrap();
    assert_eq!(output.exit_code, 3);
    assert_eq!(output.lines, lines(&["exiting"]));
}

/// Production waits for the wall clock: a timer never fires before its
/// delay elapsed (a lower bound only, so the test cannot flake).
#[test]
fn the_production_clock_waits_for_the_delay() {
    let bytes = compile(
        "export function main(): void {\n  setTimeout(() => console.log(\"waited\"), 40);\n}",
    );
    let started = std::time::Instant::now();
    let output = VartaRuntime::new()
        .run_wasm_bytes(&bytes, &timer_grant())
        .unwrap();
    assert!(started.elapsed() >= std::time::Duration::from_millis(40));
    assert_eq!(output.lines, lines(&["waited"]));
}

/// The timer imports exist only under host ABI 32, which only v36 declares.
/// A v36 module relabelled as v35 fails before any guest effect; earlier
/// profiles keep their diagnostics for the timer globals.
#[test]
fn timer_imports_require_the_v36_host_abi() {
    use varta_common::HostOperation as H;
    let bytes = compile(
        "export function main(): void {\n  const t = setTimeout(() => console.log(\"x\"), 1);\n  clearTimeout(t);\n}",
    );
    let runtime = VartaRuntime::new();
    let inspection = runtime.inspect_module(&bytes).unwrap();
    for operation in [H::TimerStart, H::TimerClear, H::TimerTake, H::TimerUncaught] {
        assert!(
            inspection
                .imports
                .iter()
                .any(|import| import.name == operation.name()),
            "{operation:?}"
        );
        assert_eq!(operation.minimum_abi(), abi::TIMERS_VERSION);
    }
    let mut section = abi::SECTION.as_bytes().to_vec();
    section.extend(abi::TIMERS_VERSION.to_le_bytes());
    let mut older = abi::SECTION.as_bytes().to_vec();
    older.extend(abi::PROMISE_VALUES_VERSION.to_le_bytes());
    let relabelled = patched(
        &patched(&bytes, b"checked-f64-utf16-v36", b"checked-f64-utf16-v35"),
        &section,
        &older,
    );
    let error = runtime
        .validate_module(&relabelled)
        .unwrap_err()
        .to_string();
    assert!(error.contains("invalid host signature: timer_"), "{error}");
    let unknown = patched(&bytes, b"timer_start", b"timer_stary");
    let error = runtime.validate_module(&unknown).unwrap_err().to_string();
    assert!(error.contains("unknown host import"), "{error}");
    for profile in [
        LanguageProfile::CheckedF64Utf16V35,
        LanguageProfile::CheckedF64Utf16V34,
    ] {
        let error = varta_compiler::compile_typescript_with_profile(
            "export function main(): void {\n  setTimeout(() => console.log(\"x\"), 1);\n}",
            profile,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("unsupported call"), "{profile:?}: {error}");
    }
}

/// Timer imports are declared only when used: a program that uses
/// `AbortSignal` without `AbortSignal.timeout` loads no timer code and needs
/// no `time.timer` grant.
#[test]
fn programs_without_timers_import_no_timer_operation() {
    let bytes = compile(
        "export function main(): void {\n  const controller = new AbortController();\n  controller.abort();\n  console.log(String(controller.signal.aborted));\n}",
    );
    let runtime = VartaRuntime::new();
    let inspection = runtime.inspect_module(&bytes).unwrap();
    assert!(
        !inspection
            .imports
            .iter()
            .any(|import| import.name.starts_with("timer_")),
        "{:?}",
        inspection.imports
    );
    assert_eq!(
        runtime.run_wasm_bytes(&bytes, &[]).unwrap().lines,
        lines(&["true"])
    );
}

/// Delays and arguments of literal types (module constants, `as const`)
/// erase to their primitive, and an `undefined` delay is evaluated.
#[test]
fn literal_typed_delays_and_arguments_are_admitted() {
    run(
        "const RETRY_DELAY = 20;\nconst LABEL = \"constant\" as const;\nfunction none(): undefined {\n  console.log(\"delay evaluated\");\n  return undefined;\n}\nexport function main(): void {\n  setTimeout((label: string) => console.log(label), RETRY_DELAY, LABEL);\n  setTimeout(() => console.log(\"undefined delay\"), none());\n}",
        &["delay evaluated", "undefined delay", "constant"],
    );
}

/// Outside the admitted subset, timer calls fail with construct-specific
/// diagnostics; a shadowing declaration is an ordinary function.
#[test]
fn timer_calls_outside_the_subset_are_rejected_by_name() {
    for (source, expected) in [
        ("setTimeout()", "setTimeout requires a callback function"),
        (
            "setTimeout(1, 2)",
            "setTimeout requires a typed callback function",
        ),
        (
            "setTimeout(() => {}, \"1\")",
            "setTimeout delay must be a number",
        ),
        (
            "setTimeout<number>(() => {}, 1)",
            "unsupported generic call",
        ),
        (
            "const a: number[] = [];\n  setTimeout(() => {}, ...a)",
            "spread setTimeout arguments",
        ),
        (
            "setTimeout((x: number) => {}, 1)",
            "setTimeout callback parameters do not accept the arguments it is called with",
        ),
        (
            "const f = (x: number): void => {};\n  setInterval(f, 1, \"text\")",
            "setInterval callback parameters do not accept",
        ),
        (
            "clearTimeout(1, 2)",
            "clearTimeout with more than one argument",
        ),
        (
            "clearInterval([1])",
            "clearInterval requires a timer id number",
        ),
        ("varta.web.unrefTimeout(() => {}, 1)", "unsupported call"),
    ] {
        let program = format!("export function main(): void {{\n  {source};\n}}");
        let error = varta_compiler::compile_typescript_with_profile(&program, V36)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{source}: {error}");
    }
    // A program's own declaration shadows the global.
    let output = VartaRuntime::new()
        .run_wasm_bytes(
            &compile(
                "function setTimeout(f: () => void, ms: number): number {\n  f();\n  return ms;\n}\nexport function main(): void {\n  console.log(setTimeout(() => console.log(\"now\"), 7));\n}",
            ),
            &[],
        )
        .unwrap();
    assert_eq!(output.lines, lines(&["now", "7"]));
    // A void context discards an arrow's expression value only under v36.
    let discard =
        "export function main(): void {\n  const p = new Promise<void>((resolve) => 5);\n}";
    compile(discard);
    let error = varta_compiler::compile_typescript_with_profile(
        discard,
        LanguageProfile::CheckedF64Utf16V35,
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("arrow expression result mismatch"),
        "{error}"
    );
}

/// Replace one byte string by another of the same length.
fn patched(bytes: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    assert_eq!(from.len(), to.len());
    let at = bytes
        .windows(from.len())
        .position(|window| window == from)
        .unwrap_or_else(|| panic!("missing {}", String::from_utf8_lossy(from)));
    let mut bytes = bytes.to_vec();
    bytes[at..at + to.len()].copy_from_slice(to);
    bytes
}
