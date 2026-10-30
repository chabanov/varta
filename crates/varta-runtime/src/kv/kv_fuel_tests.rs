//! V38 fair fuel of the KV operations: the charges of a real program, the
//! typed fuel error before a commit's effect, the per-operation fuel table
//! and the calibration of the KV rates (`varta_common::fuel`).
use super::tests::{Tree, compile, read_write};
use crate::{exceptions, execution, scheduler, *};
use std::time::{Duration, Instant};
use varta_common::{
    CapabilityGrant,
    budget::{BudgetKind, Budgets},
    fuel,
};

/// One calibration point: its size, its host time as fuel and its charge.
type Point = (f64, f64, f64);

/// The marginal operations of a measured workload.
const OPERATIONS: usize = 40;

struct Run {
    lines: Vec<String>,
    fuel: u64,
    error: Option<String>,
    elapsed: Duration,
}

/// Run `bytes` with the KV grants over `tree` under test-only budgets of
/// `budgets` production fuel budgets (CLAUDE.md §11), its host work at the
/// fair rates when `rated`; `elapsed` covers instantiation and every turn.
fn run(bytes: &[u8], tree: &Tree, rated: bool, budgets: u64) -> Run {
    const HOST_CALLS: u32 = 1 << 20;
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(bytes).unwrap();
    let grants: Vec<CapabilityGrant> = read_write();
    let budget = budgets * abi::FUEL;
    let mut execution = execution::Execution::with_setup(
        &runtime.engine,
        execution::Setup {
            budgets: Budgets {
                fuel_per_turn: budget,
                host_calls_per_turn: HOST_CALLS,
                ..Budgets::PRODUCTION
            },
            ..execution::Setup::production(&grants, None, policy::FileMode::Virtual(tree.roots()))
        },
    )
    .unwrap();
    execution.rate_host_work(rated);
    execution.allow_test_wall_clock(Duration::from_secs(120));
    let started = Instant::now();
    let result = (|| {
        let instance = execution.instantiate_prepared(&module)?;
        let store = &mut execution.store;
        instance
            .get_func(&mut *store, "_start")
            .ok_or_else(|| wasmtime::Error::msg("missing _start"))?
            .call(&mut *store, &[], &mut [])?;
        exceptions::finish(&instance, store)?;
        scheduler::checkpoint(&instance, store)?;
        Ok::<(), RuntimeError>(())
    })();
    let elapsed = started.elapsed();
    let store = &mut execution.store;
    let error = result.err().map(|e| {
        let text = format!("{e:#}");
        text.lines().last().unwrap_or_default().trim().to_owned()
    });
    Run {
        lines: std::mem::take(&mut store.data_mut().output.lines),
        fuel: budget - store.get_fuel().unwrap(),
        error,
        elapsed,
    }
}

/// A KV workload: `setup` and `count` repetitions of `operation` (with `i`
/// in scope), in `main` with `kv` open on a fresh database.
fn workload(setup: &str, operation: &str, count: usize) -> String {
    format!(
        "interface Run {{ run_id: string; status: string; metrics: {{ input_tokens: number; output_tokens: number }}; error_code?: string; }}\n\
         export async function main(): Promise<void> {{\n\
           const kv = await varta.kv.open('/data/bench.kv');\n\
           const run: Run = {{ run_id: 'r-1', status: 'completed', metrics: {{ input_tokens: 120, output_tokens: 34 }} }};\n\
           let total = 0;\n\
           {setup}\n\
           for (let i = 0; i < {count}; i++) {{ {operation} }}\n\
           console.log(total === -1 ? 'never' : 'done');\n\
           kv.close();\n\
         }}\n"
    )
}

/// The marginal fuel and host-side wall time of one `operation` (rated or
/// not): the difference between `2 * OPERATIONS` and `OPERATIONS`
/// repetitions, each on a fresh database, the fastest of `rounds` runs.
fn marginal(setup: &str, operation: &str, rated: bool, rounds: usize) -> (u64, Duration) {
    let measure = |count: usize| {
        let bytes = compile(&workload(setup, operation, count));
        let mut best: Option<Run> = None;
        for _ in 0..rounds {
            let tree = Tree::new();
            let run = run(&bytes, &tree, rated, 5000);
            assert_eq!(run.error, None, "{operation}");
            assert_eq!(run.lines, ["varta log: done"], "{operation}");
            if best.as_ref().is_none_or(|best| run.elapsed < best.elapsed) {
                best = Some(run);
            }
        }
        best.unwrap()
    };
    let (one, two) = (measure(OPERATIONS), measure(2 * OPERATIONS));
    let per = |a: u64, b: u64| (b - a) / OPERATIONS as u64;
    (
        per(one.fuel, two.fuel),
        two.elapsed.saturating_sub(one.elapsed) / OPERATIONS as u32,
    )
}

const SET_RUN: &str = "await kv.set(['run', i], run);";
const GET_RUN: &str = "const e = await kv.get<Run>(['run', i % 10]); if (e.value !== null) { total = total + e.value.metrics.input_tokens; }";
const SEED_RUNS: &str = "for (let k = 0; k < 10; k++) { await kv.set(['run', k], run); }";

/// Each KV operation pays the published rates on top of its guest work:
/// under v38 a `get` and a `set` cost more fuel than the same
/// program with its host work unrated, by at least the KV charges.
#[test]
fn kv_operations_pay_the_published_rates() {
    let (rated_get, _) = marginal(SEED_RUNS, GET_RUN, true, 1);
    let (free_get, _) = marginal(SEED_RUNS, GET_RUN, false, 1);
    // A get: one read returning one entry, its key and value decoded.
    assert!(
        rated_get >= free_get + fuel::KV_OPERATION + fuel::KV_ENTRY,
        "{rated_get} vs {free_get}"
    );
    let (rated_set, _) = marginal("", SET_RUN, true, 1);
    let (free_set, _) = marginal("", SET_RUN, false, 1);
    assert!(
        rated_set >= free_set + fuel::KV_OPERATION,
        "{rated_set} vs {free_set}"
    );
}

/// A commit whose charge the remaining fuel cannot cover ends the turn with
/// the typed fuel-budget error before anything is submitted: the database
/// file holds no row of it.
#[test]
fn an_unaffordable_commit_is_the_typed_fuel_error_and_writes_nothing() {
    let units = varta_common::kv::MAX_VALUE_BYTES / 2;
    let source = format!(
        "export async function main(): Promise<void> {{\n\
           const kv = await varta.kv.open('/data/denied.kv');\n\
           await kv.set(['big'], 'x'.repeat({units}));\n\
           console.log('committed');\n\
           kv.close();\n\
         }}\n"
    );
    let bytes = compile(&source);
    // The whole program at the fair rates; its commit charge alone is at
    // least `KV_BYTE` per value unit, and little guest work follows it.
    let full = run(&bytes, &Tree::new(), true, 20);
    assert_eq!(full.lines, ["varta log: committed"]);
    let budget = full.fuel - fuel::work(units, fuel::KV_BYTE) / 2;
    let tree = Tree::new();
    let report = VartaRuntime::new().run_with(
        &bytes,
        RunOptions {
            requested: read_write(),
            file_roots: tree.roots(),
            budgets: varta_common::budget::BudgetRequest {
                fuel_per_turn: Some(budget),
                ..Default::default()
            },
            ..RunOptions::default()
        },
    );
    let failure = report.failure.expect("the commit charge exhausts the fuel");
    assert!(
        matches!(&failure, RuntimeError::Budget(exhausted) if exhausted.budget == BudgetKind::Fuel),
        "{failure:?}"
    );
    assert!(report.output.lines.is_empty(), "{:?}", report.output.lines);
    assert!(super::tests::rows(&tree.root().join("denied.kv")).is_empty());
}

/// The per-operation fuel of `get`, `set` and `list` (one `Run` record per
/// entry) under v38 at the fair rates, and without the host work charged:
/// `cargo test -p varta-runtime --lib kv_operation_fuel -- --ignored --nocapture`.
#[test]
#[ignore = "measurement"]
fn kv_operation_fuel() {
    for (name, setup, operation) in [
        ("get (1 record)", SEED_RUNS, GET_RUN),
        ("set (1 record)", "", SET_RUN),
        (
            "list (10 records)",
            SEED_RUNS,
            "for await (const e of kv.list<Run>({ prefix: ['run'] })) { total = total + e.value.metrics.output_tokens; }",
        ),
    ] {
        let (fair, _) = marginal(setup, operation, true, 1);
        let (free, _) = marginal(setup, operation, false, 1);
        println!("{name}: {fair} fuel at the fair rates, {free} guest only");
    }
}

/// The calibration of the KV rates: the host-side time of each KV operation
/// (wall time minus its guest fuel at the measured guest rate), fitted to a
/// fixed part per operation, a part per key/value byte and a part per entry,
/// converted to fuel at the guest's fuel rate. Release build:
/// `cargo test --release -p varta-runtime --lib kv_fuel_calibration -- --ignored --nocapture`.
#[test]
#[ignore = "calibration"]
fn kv_fuel_calibration() {
    const ROUNDS: usize = 5;
    // The guest's fuel per nanosecond on this host (a plain loop).
    let rate = {
        let source = |n: usize| {
            format!(
                "export function main(): void {{ let s = 0; for (let i = 0; i < {n}; i++) {{ s = (s + i * 3) % 1000003; }} console.log(s === -1 ? 'never' : 'done'); }}"
            )
        };
        let best = |n: usize| {
            let bytes = compile(&source(n));
            (0..ROUNDS)
                .map(|_| run(&bytes, &Tree::new(), false, 200))
                .min_by_key(|run| run.elapsed)
                .unwrap()
        };
        let (one, two) = (best(1_000_000), best(2_000_000));
        (two.fuel - one.fuel) as f64 / (two.elapsed - one.elapsed).as_nanos() as f64
    };
    println!("guest rate: {rate:.3} fuel/ns");
    // One point: the host time of `operation` as fuel at the guest rate
    // (wall time minus the guest's own instructions), and the fuel the fair
    // rates charge for it (rated minus unrated marginal fuel).
    let point = |x: usize, setup: &str, operation: &str| {
        let (free, elapsed) = marginal(setup, operation, false, ROUNDS);
        let (rated, _) = marginal(setup, operation, true, 1);
        let host = (elapsed.as_nanos() as f64 - free as f64 / rate).max(0.0) * rate;
        (x as f64, host, (rated - free) as f64)
    };
    let string = |units: usize| format!("const v = 'x'.repeat({units});");
    let read = "const e = await kv.get<string>(['s', i % 10]); if (e.value !== null) { total = total + e.value.length; }";
    // A commit whose check fails: staged and checked, never written.
    let refused = "const r = await kv.atomic().check({ key: ['s', i], versionstamp: '00000000000000010000' }).set(['s', i], v).commit(); if (r.ok) { total = -1; }";
    let mut series: Vec<(&str, &str, Vec<Point>)> = Vec::new();
    for (name, unit) in [
        ("get", "value unit"),
        ("set", "value unit"),
        ("refused commit", "value unit"),
    ] {
        let mut points = Vec::new();
        for units in [16, 4096, 32768] {
            let seed = format!(
                "{} for (let k = 0; k < 10; k++) {{ await kv.set(['s', k], v); }}",
                string(units)
            );
            points.push(match name {
                "get" => point(units, &seed, read),
                "set" => point(units, &string(units), "await kv.set(['s', i], v);"),
                _ => point(units, &string(units), refused),
            });
        }
        series.push((name, unit, points));
    }
    let mut list = Vec::new();
    for entries in [10, 100, 400] {
        let seed = format!(
            "{} {{ let op = kv.atomic(); for (let k = 0; k < {entries}; k++) {{ op = op.set(['l', k], v); }} await op.commit(); }}",
            string(16)
        );
        let read = "for await (const e of kv.list<string>({ prefix: ['l'] }, { batchSize: 500 })) { total = total + e.value.length; }";
        list.push(point(entries, &seed, read));
    }
    series.push(("list", "entry", list));
    // Opening (and closing) the database again, one point per size.
    let reopen = "const again = await varta.kv.open('/data/bench.kv'); again.close();";
    let opens = [10, 1000]
        .into_iter()
        .map(|rows| {
            let seed = format!(
                "{} {{ let op = kv.atomic(); for (let k = 0; k < {rows}; k++) {{ op = op.set(['o', k], v); }} await op.commit(); kv.close(); }}",
                string(16)
            );
            point(rows, &seed, reopen)
        })
        .collect();
    series.push(("open", "row", opens));
    let fit = |points: &[(f64, f64)]| {
        let n = points.len() as f64;
        let (sx, sy) = points
            .iter()
            .fold((0.0, 0.0), |(sx, sy), (x, y)| (sx + x, sy + y));
        let (mx, my) = (sx / n, sy / n);
        let (sxy, sxx) = points.iter().fold((0.0, 0.0), |(sxy, sxx), (x, y)| {
            (sxy + (x - mx) * (y - my), sxx + (x - mx) * (x - mx))
        });
        let slope = sxy / sxx;
        (my - slope * mx, slope)
    };
    for (name, unit, points) in &series {
        for (x, host, charged) in points {
            println!(
                "{name} {x}: host {host:.0} fuel-equivalent, charged {charged:.0} ({:.2}x)",
                charged / host.max(1.0)
            );
        }
        let host: Vec<(f64, f64)> = points.iter().map(|(x, h, _)| (*x, *h)).collect();
        let charged: Vec<(f64, f64)> = points.iter().map(|(x, _, c)| (*x, *c)).collect();
        let ((hf, hs), (cf, cs)) = (fit(&host), fit(&charged));
        println!(
            "{name}: host fixed {hf:.0} + {hs:.2}/{unit}; charged fixed {cf:.0} + {cs:.2}/{unit}"
        );
    }
}
