//! Idle collection between the turns of v38 application instances under the
//! maintenance budget (`incremental_collection`, wave 3 (f)).
use super::*;
use varta_common::budget::{
    BudgetKind, BudgetRequest, MAINTENANCE_FUEL, MAINTENANCE_STEP_FUEL, TURN_FUEL,
};
use varta_common::collector::IDLE_EXPORT;
use varta_common::language::LanguageProfile;

const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;
const V37: LanguageProfile = LanguageProfile::CheckedF64Utf16V37;

fn compile_with(source: &str, profile: LanguageProfile) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}

/// Every event builds `__ROWS__` rows of strings and arrays, keeps the last
/// batch reachable from a module binding and drops the batch before.
const CHURN: &str = r#"
class Row { constructor(public label: string, public cells: string[]) {} }
let kept: Row[] = [];
let total = 0;
export function main(): void {
  varta.events.listen('ui', (record: string) => {
    const rows: Row[] = [];
    for (let i = 0; i < __ROWS__; i++) {
      const cells: string[] = [];
      for (let j = 0; j < 4; j++) { cells.push(`${record}-${i}-${j}`); }
      rows.push(new Row(`row ${i}`, cells));
    }
    kept = rows;
    let length = 0;
    for (const row of kept) { length += row.label.length + row.cells[3].length; }
    total += length;
    console.log(`${record} ${kept.length} ${length} ${total}`);
  });
}
"#;

fn churn(rows: usize) -> Vec<u8> {
    compile_with(&CHURN.replace("__ROWS__", &rows.to_string()), V38)
}

fn event(record: &str) -> HostEvent {
    HostEvent {
        source: "ui".into(),
        record: record.into(),
        gesture: false,
        coalesce: false,
    }
}

/// Deliver `events` events one at a time, stepping until idle after each.
fn session(
    bytes: &[u8],
    options: RunOptions,
    events: usize,
) -> (Vec<String>, BudgetReport, Option<String>) {
    let mut app = VartaRuntime::new()
        .start_application(
            bytes,
            ApplicationOptions {
                run: options,
                ..ApplicationOptions::default()
            },
        )
        .unwrap();
    for index in 0..events {
        if app.state() != InstanceState::Running {
            break;
        }
        let _ = app.deliver(event(&format!("e{index}")));
        while let Step::Turn(_) = app.step(None) {}
    }
    let failure = app.failure().map(ToString::to_string);
    (app.take_output(), app.budget_report(), failure)
}

/// The report without wall-clock fields, which are the only nondeterministic
/// part of a session.
fn deterministic(report: &BudgetReport) -> BudgetReport {
    let mut report = report.clone();
    for turn in &mut report.turns {
        turn.wall = std::time::Duration::ZERO;
    }
    for period in &mut report.maintenance {
        period.wall = std::time::Duration::ZERO;
    }
    report
}

#[test]
fn idle_collection_runs_between_turns_under_the_maintenance_budget() {
    let bytes = churn(400);
    let (lines, report, failure) = session(&bytes, RunOptions::default(), 12);
    assert_eq!(failure, None);
    assert_eq!(lines.len(), 12, "{lines:?}");
    assert_eq!(report.budgets.maintenance_fuel, MAINTENANCE_FUEL);
    assert!(
        report.total_maintenance_periods > 0,
        "the churn leaves garbage for idle collection: {:?}",
        report.lines()
    );
    for period in &report.maintenance {
        assert!(period.steps > 0 && period.fuel > 0 && period.fuel <= MAINTENANCE_FUEL);
        assert!(period.after_turn >= 1 && period.after_turn <= report.total_turns);
    }
    let listed: u64 = report.maintenance.iter().map(|p| p.fuel).sum();
    assert!(listed <= report.total_maintenance_fuel);
    // Maintenance is never charged to a turn.
    let turns: u64 = report.turns.iter().map(|t| t.fuel).sum();
    assert_eq!(turns, report.total_fuel);
    let text = report.lines();
    assert!(
        text.iter()
            .any(|line| line.starts_with("varta budget: maintenance after turn ")),
        "{text:?}"
    );
    assert!(
        text.iter()
            .any(|line| line.starts_with("varta budget: maintenance total: ")),
        "{text:?}"
    );
    // A maintenance line follows the line of the turn it comes after.
    let first = &report.maintenance[0];
    let at = text
        .iter()
        .position(|line| line.starts_with(&format!("varta budget: turn {} ", first.after_turn)))
        .unwrap();
    assert!(text[at + 1].starts_with(&format!(
        "varta budget: maintenance after turn {}: ",
        first.after_turn
    )));
}

#[test]
fn idle_collection_is_deterministic() {
    let bytes = churn(300);
    let runs: Vec<_> = (0..3)
        .map(|_| {
            let (lines, report, failure) = session(&bytes, RunOptions::default(), 10);
            (lines, deterministic(&report), failure)
        })
        .collect();
    assert!(runs[0].1.total_maintenance_periods > 0);
    for run in &runs[1..] {
        assert_eq!(run, &runs[0]);
    }
}

/// Idle collection keeps the program's results: the same output as a
/// session whose maintenance never runs (a maintenance budget below one
/// step's bound).
#[test]
fn idle_collection_preserves_live_data() {
    let bytes = churn(400);
    let (with, report, failure) = session(&bytes, RunOptions::default(), 12);
    assert_eq!(failure, None);
    assert!(report.total_maintenance_periods > 0);
    let mut options = RunOptions::default();
    options.budgets.maintenance_fuel = Some(MAINTENANCE_STEP_FUEL - 1);
    let (without, skipped, failure) = session(&bytes, options, 12);
    assert_eq!(failure, None);
    assert_eq!(skipped.total_maintenance_periods, 0);
    assert_eq!(skipped.total_maintenance_fuel, 0);
    assert_eq!(with, without);
}

#[test]
fn maintenance_budgets_are_requested_and_capped_like_turn_budgets() {
    let bytes = churn(50);
    let mut options = RunOptions::default();
    options.budgets.maintenance_fuel = Some(2 * MAINTENANCE_FUEL);
    let (_, report, _) = session(&bytes, options.clone(), 1);
    assert_eq!(report.budgets.maintenance_fuel, MAINTENANCE_FUEL);
    assert_eq!(report.capped.len(), 1);
    assert_eq!(report.capped[0].kind, BudgetKind::MaintenanceFuel);
    options.budget_ceiling = Some(BudgetRequest {
        maintenance_fuel: Some(TURN_FUEL),
        ..BudgetRequest::default()
    });
    let (_, report, _) = session(&bytes, options, 1);
    assert_eq!(report.budgets.maintenance_fuel, TURN_FUEL);
    assert!(report.capped.is_empty());
}

/// A step that exhausts the maintenance budget (here a test-only step size
/// far beyond the reference runtime's) terminates the instance like an
/// exhausted turn: typed, after the output of every earlier turn.
#[test]
fn an_exhausted_maintenance_budget_terminates_the_instance() {
    let bytes = churn(400);
    let mut options = RunOptions::default();
    options.budgets.maintenance_fuel = Some(MAINTENANCE_STEP_FUEL);
    let (lines, report, failure) =
        crate::maintenance::with_step_units(u32::MAX >> 1, || session(&bytes, options, 12));
    let failure = failure.expect("the maintenance budget is exhausted");
    assert!(
        failure.contains("(maintenance) exhausted its maintenance fuel budget"),
        "{failure}"
    );
    assert!(
        failure.contains("instructions per idle period"),
        "{failure}"
    );
    assert!(!lines.is_empty() && lines.len() < 12, "{lines:?}");
    assert_eq!(report.maintenance.last().map(|p| p.completed), Some(false));
}

/// Bounded runs and earlier profiles run no maintenance, and only v38
/// modules export the idle collection (every v38 module has a managed heap).
#[test]
fn only_v38_application_instances_collect_between_turns() {
    let source = "export function main(): void { const xs: string[] = []; for (let i = 0; i < 3; i++) { xs.push(`x${i}`); } console.log(xs.join(',')); }";
    let has_export = |bytes: &[u8]| {
        wasmtime::Module::new(&wasmtime::Engine::default(), bytes)
            .unwrap()
            .get_export(IDLE_EXPORT)
            .is_some()
    };
    assert!(has_export(&compile_with(source, V38)));
    assert!(!has_export(&compile_with(source, V37)));
    let report = VartaRuntime::new().run_with(&churn(400), RunOptions::default());
    let budget = report.budget.unwrap();
    assert_eq!(budget.total_maintenance_fuel, 0);
    assert!(budget.maintenance.is_empty());
}

/// The idle export is validated: its signature, and that nothing it
/// reaches calls a host import.
#[test]
fn the_idle_export_is_validated() {
    use crate::contract_tests::{export_index, rewrite};
    // `noisy` is `(i32) -> i32` (booleans are i32) and logs.
    let bytes = compile_with(
        r#"function noisy(flag: boolean): boolean { console.log('effect'); return !flag; }
export function main(): void { const xs: string[] = ['a']; xs.push('b'); if (noisy(xs.length > 1)) { console.log('no'); } }"#,
        V38,
    );
    VartaRuntime::new().validate_module(&bytes).unwrap();
    let retarget = |target: u8| {
        rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 7
                && let Some(start) = data
                    .windows(IDLE_EXPORT.len())
                    .position(|s| s == IDLE_EXPORT.as_bytes())
            {
                let slot = start + IDLE_EXPORT.len() + 1;
                assert!(data[slot] < 128 && target < 128);
                data[slot] = target;
            }
            vec![data]
        })
    };
    let start = export_index(&bytes, "_start");
    // `_start` has the wrong signature.
    let error = VartaRuntime::new()
        .validate_module(&retarget(start))
        .unwrap_err();
    assert!(
        error.to_string().contains("invalid idle collection export"),
        "{error}"
    );
    // `noisy`, defined before `main`, reaches a host import.
    let error = VartaRuntime::new()
        .validate_module(&retarget(start - 1))
        .unwrap_err();
    assert!(
        error.to_string().contains("cannot call host imports"),
        "{error}"
    );
}

/// One idle step of `collector::IDLE_STEP_UNITS` units fits
/// `budget::MAINTENANCE_STEP_FUEL`: with exactly that maintenance fuel every
/// idle period that has work runs one step and never exhausts it (rows of
/// strings and arrays, and a linked list traced through deep chains; a step
/// may also start a cycle, shading every root and zeroing its bitmaps).
#[test]
fn idle_step_fuel_stays_within_its_bound() {
    const LIST: &str = r#"
interface Node { label: string; parts: number[]; next: Node | undefined }
let head: Node | undefined = undefined;
let count = 0;
export function main(): void {
  varta.events.listen('ui', (record: string) => {
    for (let i = 0; i < 300; i++) {
      head = { label: `${record}${i}`, parts: [i, i + 1], next: head };
      const dropped: number[] = [i, i];
      count += dropped.length;
    }
    console.log(`${record} ${count}`);
  });
}
"#;
    for bytes in [churn(400), compile_with(LIST, V38)] {
        let mut options = RunOptions::default();
        options.budgets.maintenance_fuel = Some(MAINTENANCE_STEP_FUEL);
        let (lines, report, failure) = session(&bytes, options, 16);
        assert_eq!(failure, None);
        assert_eq!(lines.len(), 16);
        assert!(report.maintenance.iter().all(|period| period.steps == 1));
        assert!(!report.maintenance.is_empty());
        assert!(
            report
                .maintenance
                .iter()
                .all(|period| period.fuel <= MAINTENANCE_STEP_FUEL)
        );
    }
}
