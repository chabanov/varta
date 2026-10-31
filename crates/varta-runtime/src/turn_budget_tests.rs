//! Profile v38 turn budgets, long-lived application instances, host event
//! sources and permission broker v1 (`turn_budgets`, increment P5).
use super::*;
use crate::net_loopback::Loopback;
use std::sync::{Arc, Mutex};
use varta_common::CapabilityValue;
use varta_common::budget::{BudgetKind, BudgetRequest};
use varta_common::events::EventError;
use varta_common::language::LanguageProfile;
use varta_common::permission::{
    ApplicationDecisions, DecisionFile, DecisionRecord, DecisionSource, PermissionPrompt,
    PermissionState, PromptAnswer,
};
use varta_common::scheduler::{SchedulerError, TIMER_CAPABILITY};

const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;
const V37: LanguageProfile = LanguageProfile::CheckedF64Utf16V37;

fn compile_with(source: &str, profile: LanguageProfile) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}
fn compile(source: &str) -> Vec<u8> {
    compile_with(source, V38)
}
fn boolean(id: &str) -> CapabilityGrant {
    CapabilityGrant {
        id: id.into(),
        value: CapabilityValue::Boolean(true),
    }
}
fn timers() -> Vec<CapabilityGrant> {
    vec![boolean(TIMER_CAPABILITY)]
}
fn options(requested: Vec<CapabilityGrant>) -> RunOptions {
    RunOptions {
        requested,
        ..RunOptions::default()
    }
}
fn lines(expected: &[&str]) -> Vec<String> {
    expected.iter().map(|s| format!("varta log: {s}")).collect()
}
fn bounded(bytes: &[u8], options: RunOptions) -> RunReport {
    scheduler::with_virtual_clock(|| VartaRuntime::new().run_with(bytes, options))
}

/// A loop of `__N__` iterations in `main` and in each of two timers.
const SPREAD: &str = r#"
function work(n: number): number {
  let sum = 0;
  for (let i = 0; i < n; i++) {
    sum += i;
  }
  return sum;
}
export function main(): void {
  console.log(`main ${work(__N__)}`);
  setTimeout(() => { console.log(`first ${work(__N__)}`); }, 1);
  setTimeout(() => { console.log(`second ${work(__N__)}`); }, 2);
}
"#;
// Each turn of `SPREAD` fits the v38 production per-turn fuel
// (`budget::TURN_FUEL`) and the three together do not, under the v38 code
// generator (`codegen_v2`); with `V37_ITERATIONS` the three exceed the v37
// per-execution fuel (`abi::FUEL`) under the v37 code generator.
const ITERATIONS: &str = "240000";
const V37_ITERATIONS: &str = "6000";

#[test]
fn fuel_renews_for_every_turn_in_v38_and_not_before() {
    let source = SPREAD.replace("__N__", ITERATIONS);
    let report = bounded(&compile(&source), options(timers()));
    assert!(report.failure.is_none(), "{:?}", report.failure);
    let budget = report.budget.unwrap();
    assert!(budget.per_turn && !budget.deadline_per_turn);
    let kinds: Vec<TurnKind> = budget.turns.iter().map(|t| t.kind).collect();
    assert_eq!(kinds, [TurnKind::Main, TurnKind::Timer, TurnKind::Timer]);
    // Each turn fits the production budget; the three together do not.
    let turn = varta_common::budget::TURN_FUEL;
    let fuel: Vec<u64> = budget.turns.iter().map(|t| t.fuel).collect();
    assert!(fuel.iter().all(|f| *f < turn), "{fuel:?}");
    assert!(fuel.iter().sum::<u64>() > turn, "{fuel:?}");
    assert_eq!(budget.total_fuel, fuel.iter().sum::<u64>());
    assert_eq!(budget.turns[0].turn, 1);
    assert_eq!(budget.turns[2].turn, 3);
    assert!(
        budget
            .turns
            .iter()
            .all(|t| t.memory_bytes > 0 && t.host_calls >= 1)
    );

    // V37 keeps one allowance for the whole execution and fails.
    let source = SPREAD.replace("__N__", V37_ITERATIONS);
    let report = bounded(&compile_with(&source, V37), options(timers()));
    let failure = report.failure.expect("v37 exhausts its execution fuel");
    assert!(
        format!("{failure:?}").contains("OutOfFuel") || failure.to_string().contains("fuel"),
        "{failure}"
    );
    // The lines before the failure are kept, and the report has one entry.
    assert_eq!(
        report.output.lines.first().map(String::as_str),
        Some(lines(&["main 17997000"])[0].as_str())
    );
    let budget = report.budget.unwrap();
    assert!(!budget.per_turn);
    assert_eq!(budget.turns.len(), 1);
    assert_eq!(budget.turns[0].kind, TurnKind::Execution);
}

#[test]
fn an_exhausted_turn_names_its_turn_and_keeps_earlier_output() {
    let source = r#"
export function main(): void {
  console.log('main');
  setTimeout(() => {
    console.log('timer');
    let i = 0;
    while (i >= 0) { i++; }
  }, 1);
}
"#;
    let report = bounded(&compile(source), options(timers()));
    assert_eq!(report.output.lines, lines(&["main", "timer"]));
    match report.failure {
        Some(RuntimeError::Budget(exhausted)) => {
            assert_eq!(
                exhausted,
                BudgetExhausted {
                    turn: 2,
                    kind: TurnKind::Timer,
                    budget: BudgetKind::Fuel,
                    limit: varta_common::budget::TURN_FUEL,
                }
            );
            assert!(
                exhausted
                    .to_string()
                    .contains("turn 2 (timer) exhausted its fuel budget")
            );
        }
        other => panic!("{other:?}"),
    }
    // The earlier entry points still return only the error.
    let error = scheduler::with_virtual_clock(|| {
        VartaRuntime::new().run_wasm_bytes(&compile(source), &timers())
    })
    .unwrap_err();
    assert!(matches!(error, RuntimeError::Budget(_)), "{error:?}");
}

#[test]
fn manifest_budgets_are_capped_by_the_host_ceiling() {
    let source = SPREAD.replace("__N__", ITERATIONS);
    let bytes = compile(&source);
    // A package may lower its own fuel per turn.
    let mut lowered = options(timers());
    lowered.budgets.fuel_per_turn = Some(100_000);
    let report = bounded(&bytes, lowered);
    assert!(
        matches!(
            report.failure,
            Some(RuntimeError::Budget(BudgetExhausted {
                turn: 1,
                budget: BudgetKind::Fuel,
                limit: 100_000,
                ..
            }))
        ),
        "{:?}",
        report.failure
    );
    // A raise beyond the default needs the ceiling; otherwise it is capped.
    let mut raised = options(timers());
    raised.budgets.fuel_per_turn = Some(40_000_000);
    let report = bounded(&bytes, raised.clone());
    let budget = report.budget.unwrap();
    assert_eq!(
        budget.budgets.fuel_per_turn,
        varta_common::budget::TURN_FUEL
    );
    assert_eq!(budget.capped.len(), 1);
    assert_eq!(budget.capped[0].requested, 40_000_000);
    raised.budget_ceiling = Some(BudgetRequest {
        fuel_per_turn: Some(30_000_000),
        ..BudgetRequest::default()
    });
    let budget = bounded(&bytes, raised).budget.unwrap();
    assert_eq!(budget.budgets.fuel_per_turn, 30_000_000);
    assert_eq!(budget.capped[0].allowed, 30_000_000);
    // A tight host-call ceiling applies per turn.
    let mut tight = options(timers());
    tight.budget_ceiling = Some(BudgetRequest {
        host_calls_per_turn: Some(1),
        ..BudgetRequest::default()
    });
    let report = bounded(&bytes, tight);
    assert!(
        matches!(
            report.failure,
            Some(RuntimeError::Budget(BudgetExhausted {
                budget: BudgetKind::HostCalls,
                limit: 1,
                ..
            }))
        ),
        "{:?}",
        report.failure
    );
    // Earlier profiles ignore requested budgets entirely.
    let mut ignored = options(timers());
    ignored.budgets.fuel_per_turn = Some(100);
    let report = bounded(
        &compile_with("export function main(): void { console.log('ok'); }", V37),
        ignored,
    );
    assert!(report.failure.is_none());
    assert_eq!(
        report.budget.unwrap().budgets,
        varta_common::budget::Budgets::PRODUCTION
    );
    // Malformed budget requests fail before any guest effect.
    let mut zero = options(timers());
    zero.budgets.output_bytes = Some(0);
    let report = bounded(&bytes, zero);
    assert!(
        matches!(report.failure, Some(RuntimeError::Policy(_))),
        "{:?}",
        report.failure
    );
    assert!(report.output.lines.is_empty());
}

/// Timers past one turn's deadline: a bounded run ends at the deadline, an
/// application instance renews it for every turn. Real clock.
#[test]
fn application_instances_renew_the_deadline_per_turn() {
    let source = r#"
export function main(): void {
  console.log('main');
  setTimeout(() => { console.log('late'); }, 300);
  setTimeout(() => { console.log('later'); }, 600);
}
"#;
    let bytes = compile(source);
    let mut short = options(timers());
    short.budgets.deadline_per_turn_ms = Some(200);
    let report = VartaRuntime::new().run_with(&bytes, short.clone());
    assert!(
        matches!(
            report.failure,
            Some(RuntimeError::Scheduler(
                SchedulerError::TimerDeadline { .. }
            ))
        ),
        "{:?}",
        report.failure
    );
    assert_eq!(report.output.lines, lines(&["main"]));
    let report = VartaRuntime::new().run_application(
        &bytes,
        ApplicationOptions {
            run: short,
            ..ApplicationOptions::default()
        },
    );
    assert!(report.failure.is_none(), "{:?}", report.failure);
    assert_eq!(report.output.lines, lines(&["main", "late", "later"]));
    let budget = report.budget.unwrap();
    assert!(budget.deadline_per_turn);
    assert_eq!(budget.total_turns, 3);
    // Earlier profiles cannot run as application instances.
    let error = VartaRuntime::new()
        .start_application(
            &compile_with("export function main(): void {}", V37),
            ApplicationOptions::default(),
        )
        .err()
        .unwrap();
    assert!(matches!(
        error,
        RuntimeError::Application(ApplicationError::Profile)
    ));
}

const LISTENER: &str = r#"
let count = 0;
export function main(): void {
  console.log('init');
  const id: number = varta.events.listen('ui', (record: string) => {
    count += 1;
    console.log(`event ${count} ${record}`);
    if (record === 'stop') {
      varta.events.close(id);
    }
    if (record === 'boom') {
      throw new Error('listener failed');
    }
  });
  try {
    varta.events.listen('ui', (record: string) => { console.log(record); });
  } catch (error) {
    console.log(`duplicate ${error instanceof TypeError}`);
  }
}
"#;

fn event(record: &str, coalesce: bool) -> HostEvent {
    HostEvent {
        source: "ui".into(),
        record: record.into(),
        gesture: false,
        coalesce,
    }
}

#[test]
fn host_events_run_as_turns_of_a_long_lived_instance() {
    let bytes = compile(LISTENER);
    let runtime = VartaRuntime::new();
    let mut app = runtime
        .start_application(&bytes, ApplicationOptions::default())
        .unwrap();
    assert_eq!(app.state(), InstanceState::Running);
    assert_eq!(app.take_output(), lines(&["init", "duplicate true"]));
    // Only an event source keeps it alive: nothing can arrive by itself.
    assert_eq!(app.step(None), Step::Idle);
    assert_eq!(app.deliver(event("a", false)), Ok(Delivery::Queued));
    assert_eq!(app.deliver(event("m1", true)), Ok(Delivery::Queued));
    assert_eq!(app.deliver(event("m2", true)), Ok(Delivery::Coalesced));
    assert!(matches!(
        app.deliver(HostEvent {
            source: "missing".into(),
            ..event("x", false)
        }),
        Err(ApplicationError::Event(EventError::NoListener(_)))
    ));
    let Step::Turn(first) = app.step(None) else {
        panic!("a turn");
    };
    assert_eq!((first.turn, first.kind), (2, TurnKind::HostEvent));
    assert!(matches!(app.step(None), Step::Turn(_)));
    assert_eq!(app.take_output(), lines(&["event 1 a", "event 2 m2"]));
    // Closing the source ends the instance once nothing else is pending.
    app.deliver(event("stop", false)).unwrap();
    assert!(matches!(app.step(None), Step::Turn(_)));
    assert_eq!(app.step(None), Step::Finished { exit_code: 0 });
    assert_eq!(app.take_output(), lines(&["event 3 stop"]));
    assert_eq!(
        app.deliver(event("x", false)),
        Err(ApplicationError::Terminated)
    );
    assert_eq!(app.budget_report().total_turns, 4);
}

#[test]
fn a_failed_turn_terminates_the_instance_without_a_restart() {
    let bytes = compile(LISTENER);
    let runtime = VartaRuntime::new();
    let mut app = runtime
        .start_application(&bytes, ApplicationOptions::default())
        .unwrap();
    app.deliver(event("one", false)).unwrap();
    app.deliver(event("boom", false)).unwrap();
    app.deliver(event("never", false)).unwrap();
    assert!(matches!(app.step(None), Step::Turn(_)));
    assert_eq!(app.step(None), Step::Failed);
    assert!(
        matches!(
            app.failure(),
            Some(RuntimeError::Scheduler(
                SchedulerError::UnhandledRejection { .. }
            ))
        ),
        "{:?}",
        app.failure()
    );
    // The lines of the failed turn are kept; no later turn runs.
    assert_eq!(
        app.take_output(),
        lines(&["init", "duplicate true", "event 1 one", "event 2 boom"])
    );
    assert_eq!(app.step(None), Step::Failed);
    assert_eq!(
        app.deliver(event("x", false)),
        Err(ApplicationError::Terminated)
    );
    // Only an explicit new instance starts again, from main.
    let mut fresh = runtime
        .start_application(&bytes, ApplicationOptions::default())
        .unwrap();
    fresh.deliver(event("again", false)).unwrap();
    assert!(matches!(fresh.step(None), Step::Turn(_)));
    assert_eq!(
        fresh.take_output(),
        lines(&["init", "duplicate true", "event 1 again"])
    );

    // A budget exhausted in an event turn ends the instance the same way.
    let looping = compile(
        r#"export function main(): void {
  varta.events.listen('ui', (record: string) => { let i = 0; while (i >= 0) { i++; } });
}"#,
    );
    let mut app = runtime
        .start_application(&looping, ApplicationOptions::default())
        .unwrap();
    app.deliver(event("go", false)).unwrap();
    assert_eq!(app.step(None), Step::Failed);
    assert!(matches!(
        app.failure(),
        Some(RuntimeError::Budget(BudgetExhausted {
            turn: 2,
            kind: TurnKind::HostEvent,
            budget: BudgetKind::Fuel,
            ..
        }))
    ));
}

/// A fetch from a timer turn and a host event turn, against the loopback
/// server, in one long-lived instance. Real clock.
#[test]
fn application_instances_drive_timers_and_loopback_requests() {
    let server = Loopback::start();
    let source = r#"
async function load(label: string): Promise<void> {
  const response = await fetch('__BASE__/hello');
  console.log(`${label} ${response.status} ${await response.text()}`);
}
export function main(): void {
  console.log('init');
  setTimeout(() => { load('timer'); }, 20);
  const id: number = varta.events.listen('ui', (record: string) => {
    load(record);
    varta.events.close(id);
  });
}
"#
    .replace("__BASE__", &server.base);
    let requested = vec![
        CapabilityGrant {
            id: varta_common::net::REQUEST_CAPABILITY.into(),
            value: CapabilityValue::Requests(varta_common::RequestScope {
                urls: vec![format!("{}/**", server.base)],
                methods: None,
                response_headers: None,
            }),
        },
        boolean(TIMER_CAPABILITY),
    ];
    let mut app = VartaRuntime::new()
        .start_application(
            &compile(&source),
            ApplicationOptions {
                run: options(requested),
                ..ApplicationOptions::default()
            },
        )
        .unwrap();
    app.deliver(event("event", false)).unwrap();
    assert_eq!(app.run(), Step::Finished { exit_code: 0 });
    let mut output = app.take_output();
    output.sort();
    assert_eq!(
        output,
        lines(&["event 200 hello world", "init", "timer 200 hello world"])
    );
    let report = app.budget_report();
    assert!(report.turns.iter().any(|t| t.kind == TurnKind::Network));
    assert!(report.turns.iter().any(|t| t.kind == TurnKind::HostEvent));
    assert_eq!(server.accepted(), 2);
}

const STATES: &str = r#"
async function state(name: string, resource: string): Promise<string> {
  if (resource === '') {
    return (await varta.permissions.query({ name: name })).state;
  }
  return (await varta.permissions.query({ name: name, resource: resource })).state;
}
async function show(name: string, resource: string): Promise<void> {
  try {
    console.log(`${name} ${await state(name, resource)}`);
  } catch (error) {
    console.log(`${name} ${error instanceof TypeError ? 'TypeError' : 'other'}`);
  }
}
export async function main(): Promise<void> {
  await show('time.timer', '');
  await show('env.get', 'APP_MODE');
  await show('env.get', 'OTHER');
  await show('time.now', '');
  await show('net.unknown', '');
  await show('time.timer', 'x');
  const asked = await varta.permissions.request({ name: 'env.get' });
  console.log(`request ${asked.state}`);
}
"#;

fn state_grants() -> Vec<CapabilityGrant> {
    vec![
        boolean(TIMER_CAPABILITY),
        CapabilityGrant {
            id: "env.get".into(),
            value: CapabilityValue::PathGlobs(vec!["APP_*".into()]),
        },
    ]
}
fn optional_env() -> BTreeMap<String, Consent> {
    BTreeMap::from([(
        "env.get".to_owned(),
        Consent {
            required: false,
            purpose: Some("Read the mode".into()),
        },
    )])
}

#[test]
fn queries_report_broker_states_and_decisions_are_keyed_by_artifact() {
    let bytes = compile(STATES);
    let mut headless = options(state_grants());
    headless.consents = optional_env();
    let report = bounded(&bytes, headless.clone());
    assert!(report.failure.is_none(), "{:?}", report.failure);
    // Headless hosts never prompt: the optional grant is denied.
    assert_eq!(
        report.output.lines,
        lines(&[
            "time.timer granted",
            "env.get denied",
            "env.get denied",
            "time.now denied",
            "net.unknown TypeError",
            "time.timer TypeError",
            "request denied",
        ])
    );
    assert!(report.audit.iter().any(|e| e.operation == "time.timer"
        && e.source == DecisionSource::Policy
        && e.outcome == PermissionState::Granted));
    assert!(
        report
            .artifact
            .starts_with(varta_common::permission::ARTIFACT_PREFIX)
    );
    assert_eq!(report.artifact, artifact_identity(&bytes));

    // A stored allow for this artifact grants the optional permission; a
    // decision for another artifact is ignored.
    let file = |artifact: &str, decision| DecisionFile {
        schema_version: 1,
        applications: vec![ApplicationDecisions {
            artifact: artifact.into(),
            decisions: vec![DecisionRecord {
                id: "env.get".into(),
                decision,
            }],
        }],
    };
    let mut allowed = headless.clone();
    allowed.decisions = Some(file(&report.artifact, StoredDecision::Allow));
    let lines_allowed = bounded(&bytes, allowed).output.lines;
    assert_eq!(lines_allowed[1], lines(&["env.get granted"])[0]);
    assert_eq!(
        lines_allowed[2],
        lines(&["env.get denied"])[0],
        "never beyond the request"
    );
    assert_eq!(lines_allowed[6], lines(&["request granted"])[0]);
    let other = format!(
        "{}{}",
        varta_common::permission::ARTIFACT_PREFIX,
        "0".repeat(64)
    );
    let mut elsewhere = headless.clone();
    elsewhere.decisions = Some(file(&other, StoredDecision::Allow));
    assert_eq!(
        bounded(&bytes, elsewhere).output.lines[1],
        lines(&["env.get denied"])[0]
    );
    // A stored denial of a required grant: the application cannot run.
    let mut refused = options(state_grants());
    refused.decisions = Some(DecisionFile {
        schema_version: 1,
        applications: vec![ApplicationDecisions {
            artifact: report.artifact.clone(),
            decisions: vec![DecisionRecord {
                id: TIMER_CAPABILITY.into(),
                decision: StoredDecision::Deny,
            }],
        }],
    });
    let report = bounded(&bytes, refused);
    assert!(
        matches!(report.failure, Some(RuntimeError::Permission(_))),
        "{:?}",
        report.failure
    );
    assert!(report.output.lines.is_empty());
    // The host ceiling still wins over a stored allow.
    let mut ceiling = headless;
    ceiling.ceiling = Some(vec![boolean(TIMER_CAPABILITY)]);
    ceiling.decisions = Some(file(&artifact_identity(&bytes), StoredDecision::Allow));
    assert_eq!(
        bounded(&bytes, ceiling).output.lines[1],
        lines(&["env.get denied"])[0]
    );
}

#[derive(Clone, Default)]
struct Recorder {
    answers: Arc<Mutex<Vec<PromptAnswer>>>,
    shown: Arc<Mutex<Vec<PermissionPrompt>>>,
}
impl PermissionPrompter for Recorder {
    fn prompt(&mut self, prompt: &PermissionPrompt) -> PromptAnswer {
        self.shown.lock().unwrap().push(prompt.clone());
        self.answers.lock().unwrap().remove(0)
    }
}

const GESTURE: &str = r#"
export function main(): void {
  varta.events.listen('ui', (record: string) => {
    varta.permissions.request({ name: 'env.get' }).then((status) => {
      console.log(`${record} ${status.state}`);
    });
  });
}
"#;

#[test]
fn requests_prompt_only_in_user_gesture_turns() {
    let bytes = compile(GESTURE);
    let recorder = Recorder::default();
    // The required timer is asked once at first run, then env.get on use.
    recorder
        .answers
        .lock()
        .unwrap()
        .extend([PromptAnswer::Allow, PromptAnswer::Allow]);
    let mut run = options(state_grants());
    run.consents = optional_env();
    let mut app = VartaRuntime::new()
        .start_application(
            &bytes,
            ApplicationOptions {
                run,
                prompter: Some(Box::new(recorder.clone())),
                ..ApplicationOptions::default()
            },
        )
        .unwrap();
    // A background turn does not prompt: the state stays `prompt`.
    app.deliver(event("background", false)).unwrap();
    assert!(matches!(app.step(None), Step::Turn(_)));
    assert_eq!(app.take_output(), lines(&["background prompt"]));
    // A gesture turn opens the prompt; the answer arrives in a later turn.
    app.deliver(HostEvent {
        gesture: true,
        ..event("click", false)
    })
    .unwrap();
    assert!(matches!(app.step(None), Step::Turn(_)));
    assert!(app.take_output().is_empty());
    let Step::Turn(answer) = app.step(None) else {
        panic!("the answer turn");
    };
    assert_eq!(answer.kind, TurnKind::Permission);
    assert_eq!(app.take_output(), lines(&["click granted"]));
    let shown = recorder.shown.lock().unwrap();
    assert_eq!(shown.len(), 2);
    assert_eq!(shown[0].capability, TIMER_CAPABILITY);
    assert!(shown[0].required);
    assert_eq!(shown[1].capability, "env.get");
    assert_eq!(shown[1].scope, ["APP_*"]);
    assert_eq!(shown[1].purpose.as_deref(), Some("Read the mode"));
    assert!(!shown[1].required);
    drop(shown);
    assert_eq!(app.decisions()["env.get"], StoredDecision::Allow);
    let (audit, _) = app.audit();
    assert!(audit.iter().any(|e| e.source == DecisionSource::Prompt
        && e.operation == "env.get"
        && e.outcome == PermissionState::Granted));
    // Revocation affects new operations.
    assert!(app.revoke("env.get"));
    app.deliver(event("after", false)).unwrap();
    assert!(matches!(app.step(None), Step::Turn(_)));
    assert_eq!(app.take_output(), lines(&["after denied"]));
}

#[test]
fn instance_imports_need_host_abi_34() {
    use varta_common::HostOperation as H;
    for operation in [
        H::PermissionQuery,
        H::PermissionRequest,
        H::EventSourceOpen,
        H::EventWatch,
        H::EventTake,
        H::EventText,
        H::EventSourceClose,
    ] {
        assert_eq!(operation.minimum_abi(), abi::PROGRAM_LIMITS_VERSION);
        assert_eq!(operation.capability(), None);
        assert_eq!(H::from_name(operation.name()), Some(operation));
    }
    // V37 programs cannot name the v38 APIs, and programs never reach the
    // engine-internal intrinsics.
    for source in [
        "export function main(): void { varta.permissions.query({ name: 'time.now' }); }",
        "export function main(): void { varta.events.listen('ui', (r: string) => {}); }",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, V37).is_err(),
            "{source}"
        );
    }
    for source in [
        "export function main(): void { varta.events.open('ui'); }",
        "export function main(): void { varta.permissions.state('x'); }",
        "export function main(): void { varta.events.watch(1, () => {}); }",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, V38).is_err(),
            "{source}"
        );
    }
}
