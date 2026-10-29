//! K4.4 step 3: promises and the microtask scheduler (v34 `promises`).
//! Expected logs were produced by Node 24 for the same programs; they are
//! pinned here, so the tests need no Node installation (differential oracle).
use super::*;
use varta_common::language::LanguageProfile;

const V34: LanguageProfile = LanguageProfile::CheckedF64Utf16V34;

pub(crate) fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, V34)
        .unwrap_or_else(|e| panic!("{e}\n{source}"))
}
pub(crate) fn run(source: &str, expected: &[&str]) {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>(),
        "{source}"
    );
}

/// Fuel a program consumes through its whole checkpoint (debug helper).
pub(crate) fn fuel(source: &str) -> u64 {
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&compile(source)).unwrap();
    let mut execution = execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    let start = instance.get_func(&mut *store, "_start").unwrap();
    start.call(&mut *store, &[], &mut []).unwrap();
    scheduler::checkpoint(&instance, store).unwrap();
    abi::FUEL - store.get_fuel().unwrap()
}
/// Replace one byte string by another of the same length (section sizes stay).
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

/// The scheduler imports exist only under the v34 host ABI, take no
/// capability, and are matched by exact name and signature.
#[test]
fn scheduler_imports_require_the_v34_host_abi_and_no_capability() {
    use varta_common::HostOperation as H;
    for operation in [
        H::PromiseReact,
        H::PromiseSettle,
        H::SchedulerEnqueue,
        H::PromiseUnhandled,
        H::PromiseHandled,
    ] {
        assert_eq!(operation.minimum_abi(), abi::SCHEDULED_FRAMES_VERSION);
        assert_eq!(operation.capability(), None);
        assert!(operation.scheduler());
        assert!(!operation.returns_string() && !operation.needs_result_globals());
        assert_eq!(H::from_name(operation.name()), Some(operation));
        assert_eq!(V34.host_operation(operation), operation);
    }
    let bytes = compile(
        "export async function main(): Promise<void> {\n  await Promise.reject<number>(1).catch(() => 2);\n}",
    );
    let runtime = VartaRuntime::new();
    runtime.validate_module(&bytes).unwrap();
    assert!(
        runtime
            .run_wasm_bytes(&bytes, &[])
            .unwrap()
            .lines
            .is_empty()
    );
    // Downgrading the declared profile and host ABI to v33 rejects the import
    // before compilation or any guest effect.
    let v33 = LanguageProfile::CheckedF64Utf16V33;
    let mut section = abi::SECTION.as_bytes().to_vec();
    section.extend(abi::SCHEDULED_FRAMES_VERSION.to_le_bytes());
    let mut older = abi::SECTION.as_bytes().to_vec();
    older.extend(v33.host_abi().to_le_bytes());
    let mut values = varta_common::value::VALUE_SECTION.as_bytes().to_vec();
    values.extend(V34.value_abi().to_le_bytes());
    let mut older_values = varta_common::value::VALUE_SECTION.as_bytes().to_vec();
    older_values.extend(v33.value_abi().to_le_bytes());
    let downgraded = patched(
        &patched(
            &patched(&bytes, b"checked-f64-utf16-v34", b"checked-f64-utf16-v33"),
            &section,
            &older,
        ),
        &values,
        &older_values,
    );
    let error = runtime
        .validate_module(&downgraded)
        .unwrap_err()
        .to_string();
    assert!(error.contains("invalid host signature"), "{error}");
    // An unknown scheduler operation name is not a host import.
    let unknown = patched(&bytes, b"promise_react", b"promise_reacx");
    let error = runtime.validate_module(&unknown).unwrap_err().to_string();
    assert!(error.contains("unknown host import"), "{error}");
}

fn failure(source: &str) -> RuntimeError {
    let bytes = compile(source);
    VartaRuntime::new()
        .run_wasm_bytes(&bytes, &[])
        .expect_err("the execution must terminate")
}

/// Node 24 terminates with `ERR_UNHANDLED_REJECTION` for the reason "second",
/// the earliest rejection still unhandled after the checkpoint.
#[test]
fn unhandled_rejections_terminate_after_the_checkpoint_with_the_first_reason() {
    let error = failure(
        r#"
async function boom(): Promise<number> {
  await null;
  throw new TypeError("lost in async");
}
export function main(): void {
  boom();
  Promise.reject<number>("second");
  console.log("sync");
}
"#,
    );
    let RuntimeError::Scheduler(varta_common::scheduler::SchedulerError::UnhandledRejection {
        count,
        reason,
    }) = &error
    else {
        panic!("{error:?}");
    };
    assert_eq!(*count, 2);
    assert_eq!(reason.to_string(), "\"second\"");
    assert!(
        error
            .to_string()
            .starts_with("unhandled promise rejection: \"second\" (2 rejected promise(s)"),
        "{error}"
    );
    let error = failure(
        "export async function main(): Promise<void> {\n  await null;\n  throw new RangeError(\"main failed\");\n}",
    );
    assert!(
        error
            .to_string()
            .starts_with("unhandled promise rejection: RangeError: \"main failed\""),
        "{error}"
    );
}

/// Node 24 prints `sync`, then `handled late`, and exits normally: a handler
/// attached by a later job of the same checkpoint withdraws the rejection.
#[test]
fn a_handler_added_during_the_checkpoint_withdraws_the_rejection() {
    run(
        r#"
export function main(): void {
  const p: Promise<number> = Promise.reject<number>(new Error("late"));
  Promise.resolve(0)
    .then(() => 1)
    .then(() => {
      p.catch((e) => {
        if (e instanceof Error) {
          console.log("handled " + e.message);
        }
        return 0;
      });
    });
  console.log("sync");
}
"#,
        &["sync", "handled late"],
    );
}

/// Exhaustion inside a job is fatal: it never becomes a rejection a later
/// handler could observe, and `await` never renews the execution's budget.
#[test]
fn fuel_exhaustion_inside_jobs_is_fatal_and_await_never_renews_it() {
    for source in [
        r#"
export function main(): void {
  Promise.resolve(1)
    .then(() => {
      let n = 0;
      while (n >= 0) {
        n = n + 1;
      }
      return n;
    })
    .catch(() => {
      console.log("never a rejection");
      return 0;
    });
}
"#,
        "export async function main(): Promise<void> {\n  for (;;) {\n    await null;\n  }\n}",
    ] {
        let error = failure(source);
        assert!(
            matches!(&error, RuntimeError::Execution(e) if format!("{e:#}").contains("fuel")),
            "{error:?}"
        );
    }
    // Every job draws on the one execution budget: more awaits, more fuel.
    let awaits = |count: usize| {
        fuel(&format!(
            "export async function main(): Promise<void> {{\n  for (let i = 0; i < {count}; i++) {{\n    await i;\n  }}\n}}"
        ))
    };
    let (few, many) = (awaits(5), awaits(10));
    assert!(many > few && many - few >= 5, "{few} {many}");
}

/// Finished frames leave the live-frame table, so sequential async calls may
/// outnumber it; more pending reactions than it holds at once are fatal.
#[test]
fn live_frames_are_released_and_bounded() {
    let count = varta_common::function::frame::MAX_LIVE_FRAMES + 6;
    run(
        &format!(
            "async function tiny(n: number): Promise<number> {{\n  await null;\n  return n;\n}}\nexport async function main(): Promise<void> {{\n  let total = 0;\n  for (let i = 0; i < {count}; i++) {{\n    total = total + (await tiny(1));\n  }}\n  console.log(String(total));\n}}"
        ),
        &[&count.to_string()],
    );
    let error = failure(&format!(
        "export function main(): void {{\n  const never = new Promise<number>(() => {{}});\n  for (let i = 0; i < {count}; i++) {{\n    never.then((v) => v + 1);\n  }}\n}}"
    ));
    assert!(
        matches!(&error, RuntimeError::Execution(e) if format!("{e:#}").contains("unreachable")),
        "{error:?}"
    );
}

/// The dispatcher checks the generation in every token: a token resumed once
/// is stale even while its frame stays suspended, and a completed frame
/// rejects every token, so a late or duplicate resumption traps.
#[test]
fn late_and_duplicate_resumptions_are_rejected_by_generation() {
    let runtime = VartaRuntime::new();
    let module = runtime
        .prepare_module(&compile(
            "export async function main(): Promise<void> {\n  await null;\n  await null;\n}",
        ))
        .unwrap();
    let mut execution = execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    let start = instance.get_func(&mut *store, "_start").unwrap();
    start.call(&mut *store, &[], &mut []).unwrap();
    let resume = instance
        .get_typed_func::<(i64, i32, i64), i32>(
            &mut *store,
            varta_common::function::frame::RESUME_EXPORT,
        )
        .unwrap();
    let first = store.data_mut().scheduler.next().unwrap().unwrap();
    assert_eq!(resume.call(&mut *store, (first, 0, 0)).unwrap(), 0);
    let second = store.data_mut().scheduler.next().unwrap().unwrap();
    assert_ne!(first, second);
    assert_eq!(first as u32, second as u32, "same frame, new generation");
    // A stale generation on a suspended frame traps before any effect.
    assert!(resume.call(&mut *store, (first, 0, 0)).is_err());
}
#[test]
fn completed_frames_reject_their_last_token() {
    let runtime = VartaRuntime::new();
    let module = runtime
        .prepare_module(&compile(
            "export async function main(): Promise<void> {\n  await null;\n}",
        ))
        .unwrap();
    let mut execution = execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    let start = instance.get_func(&mut *store, "_start").unwrap();
    start.call(&mut *store, &[], &mut []).unwrap();
    let resume = instance
        .get_typed_func::<(i64, i32, i64), i32>(
            &mut *store,
            varta_common::function::frame::RESUME_EXPORT,
        )
        .unwrap();
    let token = store.data_mut().scheduler.next().unwrap().unwrap();
    assert_eq!(resume.call(&mut *store, (token, 0, 0)).unwrap(), 1);
    assert!(store.data_mut().scheduler.next().unwrap().is_none());
    assert!(resume.call(&mut *store, (token, 0, 0)).is_err());
}
#[test]
fn smoke_then_chain() {
    run(
        r#"
export function main(): void {
  const p: Promise<number> = Promise.resolve(1);
  p.then((v) => { console.log("a " + String(v)); return v + 1; })
   .then((v) => { console.log("b " + String(v)); });
  console.log("sync");
}
"#,
        &["sync", "a 1", "b 2"],
    );
}

// Pinned Node 24 outputs (generated by running each case with `main()` appended).
#[test]
fn promise_job_order_matches_node() {
    run(
        include_str!("promise_cases/ordering.ts"),
        &[
            "script start",
            "executor runs synchronously",
            "script end",
            "a1 1",
            "b1 2",
            "a-second",
            "a2 10",
            "c3 5",
            "after resolveLater",
            "b2 20",
            "pending got late",
            "pending again late",
        ],
    );
}
#[test]
fn promise_rejections_and_resolution_match_node() {
    run(
        include_str!("promise_cases/rejections.ts"),
        &[
            "guarded try",
            "value sync",
            "once 1",
            "executor TypeError executor failed",
            "settled kept",
            "finally sees no value",
            "finally on rejection",
            "cleanup finally",
            "passed through plain",
            "finally overrides",
            "cycle TypeError: Chaining cycle detected for promise #<Promise>",
            "guarded caught inner",
            "guarded finally",
            "cleanup after await in finally",
            "direct RangeError direct",
            "after catch -1",
            "guarded recovered",
            "cleanup 10",
            "finally passes 5",
            "finally rethrows kept reason",
        ],
    );
}
