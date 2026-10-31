//! V38 `codegen_v2`: fuel ceilings of the measured web workloads, the v37
//! baseline they improve on, unchanged v37 bytes, v37/v38 behavior parity
//! over the runtime case corpus, and collections during the new string paths.
//!
//! `codegen_v2_fuel_ceilings_hold` prints the benchmark table recorded in
//! docs/development-plan.md (`--nocapture`): fuel per iteration (the marginal
//! cost between two sizes) and the totals at the checked size.
use super::*;
use varta_common::{CapabilityGrant, CapabilityValue, language::LanguageProfile as P};

const V37: P = P::CheckedF64Utf16V37;
const V38: P = P::CheckedF64Utf16V38;

pub(crate) fn compile(source: &str, profile: P) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}

pub(crate) struct Run {
    pub lines: Vec<String>,
    pub fuel: u64,
    pub error: Option<String>,
}
/// Run with a test-only budget of `budgets` production fuel budgets.
pub(crate) fn run_with(bytes: &[u8], grants: &[CapabilityGrant], budgets: u64) -> Run {
    let runtime = VartaRuntime::new();
    let execution = execution::Execution::new(&runtime.engine, grants, None).unwrap();
    execute(&runtime, execution, bytes, budgets)
}
/// [`run_with`] charging the module's host work at the v38 fair rates
/// (`varta_common::fuel`) whatever its profile: the like-with-like fuel of
/// an earlier profile.
pub(crate) fn run_rated(bytes: &[u8], grants: &[CapabilityGrant], budgets: u64) -> Run {
    let runtime = VartaRuntime::new();
    let mut execution = execution::Execution::new(&runtime.engine, grants, None).unwrap();
    execution.rate_host_work(true);
    execute(&runtime, execution, bytes, budgets)
}
/// Run with test-only budgets (CLAUDE.md §11): `budgets` production fuel
/// budgets and host calls beyond the production per-turn count, so a
/// workload is measured by its guest work alone.
pub(crate) fn run_test_budgets(bytes: &[u8], budgets: u64) -> Run {
    run_budgets(bytes, budgets, false)
}
/// [`run_test_budgets`], with host work at the fair rates when `rated`.
pub(crate) fn run_budgets(bytes: &[u8], budgets: u64, rated: bool) -> Run {
    const HOST_CALLS: u32 = 1 << 20;
    let runtime = VartaRuntime::new();
    let mut execution = execution::Execution::with_test_budgets(
        &runtime.engine,
        varta_common::budget::Budgets {
            fuel_per_turn: budgets * abi::FUEL,
            host_calls_per_turn: HOST_CALLS,
            ..varta_common::budget::Budgets::PRODUCTION
        },
    )
    .unwrap();
    if rated {
        execution.rate_host_work(true);
    }
    execute(&runtime, execution, bytes, budgets)
}
/// Fuel of instantiation alone, under a test-only ceiling.
pub(crate) fn instantiation_fuel(bytes: &[u8]) -> u64 {
    const CEILING: u64 = 64 * abi::FUEL;
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(bytes).unwrap();
    let mut execution = execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(CEILING).unwrap();
    execution.instantiate_prepared(&module).unwrap();
    CEILING - execution.store.get_fuel().unwrap()
}

fn execute(
    runtime: &VartaRuntime,
    mut execution: execution::Execution,
    bytes: &[u8],
    budgets: u64,
) -> Run {
    let module = runtime.prepare_module(bytes).unwrap();
    let budget = budgets * abi::FUEL;
    execution.store.set_fuel(budget).unwrap();
    if budgets > 1 {
        execution.allow_test_wall_clock(std::time::Duration::from_secs(120));
    }
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
    let store = &mut execution.store;
    let error = result.err().map(|e| {
        // The root cause, without backtrace offsets that differ by build.
        let text = format!("{e:#}");
        text.lines().last().unwrap_or_default().trim().to_owned()
    });
    Run {
        lines: std::mem::take(&mut store.data_mut().output.lines),
        fuel: budget - store.get_fuel().unwrap(),
        error,
    }
}
fn output(source: &str) -> Vec<String> {
    let run = run_with(&compile(source, V38), &[], 20);
    assert_eq!(run.error, None, "{source}");
    run.lines
}
pub(crate) fn lines(expected: &[&str]) -> Vec<String> {
    expected.iter().map(|s| format!("varta log: {s}")).collect()
}

/// One measured workload: its source for `n` iterations, the size it is
/// checked at, its v38 fuel ceiling there and the least v37/v38 fuel ratio.
struct Pattern {
    name: &'static str,
    source: fn(usize) -> String,
    size: usize,
    ceiling: u64,
    speedup: f64,
}

fn number_loop(n: usize) -> String {
    format!(
        "export function main(): void {{ let x = 0; for (let i = 0; i < {n}; i++) {{ x = x + i; }} console.log(String(x)); }}"
    )
}
fn string_compare(n: usize) -> String {
    format!(
        "export function main(): void {{ const a = \"abc\"; let b = \"ab\"; b = b + \"c\"; let n = 0; for (let i = 0; i < {n}; i++) {{ if (a === b) {{ n = n + 1; }} }} console.log(String(n)); }}"
    )
}
fn literal_compare(n: usize) -> String {
    format!(
        "export function main(): void {{ let kind = \"cl\"; kind += \"ick\"; let n = 0; for (let i = 0; i < {n}; i++) {{ if (kind === \"click\") {{ n++; }} else if (kind !== \"input\") {{ n--; }} }} console.log(String(n)); }}"
    )
}
fn string_concat(n: usize) -> String {
    format!(
        "export function main(): void {{ let s = \"\"; for (let i = 0; i < {n}; i++) {{ s = s + \"x\"; }} console.log(String(s.length)); }}"
    )
}
fn dictionary(n: usize) -> String {
    format!(
        "export function main(): void {{ const d: Record<string, number> = {{}}; for (let i = 0; i < {n}; i++) {{ d[\"k\" + String(i)] = i; }} let t = 0; for (let i = 0; i < {n}; i++) {{ const v = d[\"k\" + String(i)]; if (v !== undefined) {{ t = t + v; }} }} console.log(String(t)); }}"
    )
}
fn record_reads(n: usize) -> String {
    format!(
        "interface P {{ x: number; y: number }} export function main(): void {{ const p: P = {{ x: 1, y: 2 }}; let t = 0; for (let i = 0; i < {n}; i++) {{ t = t + p.x + p.y; }} console.log(String(t)); }}"
    )
}
fn function_calls(n: usize) -> String {
    format!(
        "function add(a: number, b: number): number {{ return a + b; }} export function main(): void {{ let t = 0; for (let i = 0; i < {n}; i++) {{ t = add(t, i); }} console.log(String(t)); }}"
    )
}
fn closure_calls(n: usize) -> String {
    format!(
        "export function main(): void {{ let k = 3; const f = (v: number): number => v + k; let t = 0; for (let i = 0; i < {n}; i++) {{ t = f(t); }} console.log(String(t)); }}"
    )
}
fn array_sum(n: usize) -> String {
    format!(
        "export function main(): void {{ const a: number[] = []; for (let i = 0; i < {n}; i++) {{ a.push(i); }} let t = 0; for (let i = 0; i < a.length; i++) {{ t = t + a[i]; }} console.log(String(t)); }}"
    )
}
/// Markdown-like scanning: headings, emphasis markers and line splitting
/// over a text of `n` repeated lines (`charAt` makes host calls before v38).
fn markdown_scan(n: usize) -> String {
    format!(
        "export function main(): void {{ let text = \"\"; for (let i = 0; i < {n}; i++) {{ text += \"# Title \" + String(i) + \"\\n*bold* and _em_ text\\n\"; }} let headings = 0; let marks = 0; let line = \"\"; for (let i = 0; i < text.length; i++) {{ const c = text.charAt(i); if (c === \"\\n\") {{ if (line.startsWith(\"#\")) {{ headings++; }} line = \"\"; }} else {{ line += c; if (c === \"*\" || c === \"_\") {{ marks++; }} }} }} console.log(String(headings) + \" \" + String(marks)); }}"
    )
}
/// Parse a JSON array of `n` records (about 2 KB at the checked size).
fn json_parse(n: usize) -> String {
    format!(
        "interface Item {{ id: number; name: string; tags: string[] }} export function main(): void {{ let text = \"[\"; for (let i = 0; i < {n}; i++) {{ if (i > 0) {{ text += \",\"; }} text += \"{{\\\"id\\\":\" + String(i) + \",\\\"name\\\":\\\"item\" + String(i) + \"\\\",\\\"tags\\\":[\\\"a\\\",\\\"b\\\"]}}\"; }} text += \"]\"; const items = JSON.parse(text) as Item[]; let t = 0; for (const item of items) {{ t = t + item.id + item.tags.length; }} console.log(String(text.length) + \" \" + String(t)); }}"
    )
}
/// A keyed-list diff of `n` rows: index the old keys, then classify each new
/// row as moved, changed or created.
fn keyed_diff(n: usize) -> String {
    format!(
        "interface Row {{ key: string; label: string }} export function main(): void {{ const before: Row[] = []; const after: Row[] = []; for (let i = 0; i < {n}; i++) {{ before.push({{ key: \"k\" + String(i), label: \"row \" + String(i) }}); }} for (let i = 0; i < {n}; i++) {{ const j = (i * 7) % {n}; after.push({{ key: \"k\" + String(j), label: j % 3 === 0 ? \"changed \" + String(j) : \"row \" + String(j) }}); }} const index: Record<string, number> = {{}}; for (let i = 0; i < before.length; i++) {{ index[before[i].key] = i; }} let moved = 0; let changed = 0; let created = 0; for (let i = 0; i < after.length; i++) {{ const row = after[i]; const old = index[row.key]; if (old === undefined) {{ created++; }} else {{ if (old !== i) {{ moved++; }} if (before[old].label !== row.label) {{ changed++; }} }} }} console.log(String(moved) + \" \" + String(changed) + \" \" + String(created)); }}"
    )
}

/// Render `n` keyed rows into an HTML string, the shape of a list view.
fn list_render(n: usize) -> String {
    format!(
        "interface Row {{ key: string; label: string }} function render(rows: Row[]): string {{ let html = \"\"; for (const row of rows) {{ html += \"<li id=\\\"\" + row.key + \"\\\">\" + row.label + \"</li>\"; }} return html; }} export function main(): void {{ const rows: Row[] = []; for (let i = 0; i < {n}; i++) {{ rows.push({{ key: \"k\" + String(i), label: \"row \" + String(i) }}); }} console.log(String(render(rows).length)); }}"
    )
}

/// A fine-grained reactive keyed list of `rows` rows (signal, effect and
/// keyed mapping, `codegen_v2_fixtures/keyed_view.ts`): mount them, or mount
/// them, then reverse their order and change every row's text.
fn keyed_view(rows: usize, update: bool) -> String {
    include_str!("codegen_v2_fixtures/keyed_view.ts")
        .replace("const ROWS = 10;", &format!("const ROWS = {rows};"))
        .replace(
            "const UPDATE = false;",
            &format!("const UPDATE = {update};"),
        )
}
/// A Markdown-like block parser over `replies` copies of a chat reply with a
/// heading, a list, a fenced block, a table and a paragraph, mostly ASCII with
/// a few non-ASCII characters (`codegen_v2_fixtures/markdown.ts`).
fn markdown_blocks(replies: usize) -> String {
    include_str!("codegen_v2_fixtures/markdown.ts")
        .replace("const REPEAT = 1;", &format!("const REPEAT = {replies};"))
}
fn view_mount(rows: usize) -> String {
    keyed_view(rows, false)
}
fn view_update(rows: usize) -> String {
    keyed_view(rows, true)
}

const PATTERNS: &[Pattern] = &[
    Pattern {
        name: "number loop",
        source: number_loop,
        size: 10_000,
        ceiling: 170_000,
        speedup: 5.0,
    },
    Pattern {
        name: "3-char string ===",
        source: string_compare,
        size: 4_000,
        ceiling: 400_000,
        speedup: 3.0,
    },
    Pattern {
        name: "string === literal",
        source: literal_compare,
        size: 4_000,
        ceiling: 150_000,
        speedup: 5.0,
    },
    Pattern {
        name: "s = s + \"x\"",
        source: string_concat,
        size: 800,
        ceiling: 140_000,
        speedup: 5.0,
    },
    Pattern {
        name: "dictionary 50+50",
        source: dictionary,
        size: 50,
        ceiling: 420_000,
        speedup: 1.5,
    },
    Pattern {
        name: "record field reads",
        source: record_reads,
        size: 1_000,
        ceiling: 50_000,
        speedup: 4.0,
    },
    Pattern {
        name: "function calls",
        source: function_calls,
        size: 1_000,
        ceiling: 40_000,
        speedup: 4.0,
    },
    Pattern {
        name: "closure calls",
        source: closure_calls,
        size: 1_000,
        ceiling: 340_000,
        speedup: 1.3,
    },
    Pattern {
        name: "array push + index sum",
        source: array_sum,
        size: 500,
        ceiling: 130_000,
        speedup: 5.0,
    },
    Pattern {
        name: "markdown-like scan",
        source: markdown_scan,
        size: 10,
        ceiling: 75_000,
        speedup: 4.5,
    },
    // Re-pinned for wave 3: v38 decodes JSON in guest code (421,398) and
    // v37's host decoding is charged at the fair rates (769,036).
    Pattern {
        name: "JSON.parse ~2 KB",
        source: json_parse,
        size: 40,
        ceiling: 485_000,
        speedup: 1.6,
    },
    Pattern {
        name: "list render 100 rows",
        source: list_render,
        size: 100,
        ceiling: 180_000,
        speedup: 15.0,
    },
    Pattern {
        name: "keyed diff 100 rows",
        source: keyed_diff,
        size: 100,
        ceiling: 1_600_000,
        speedup: 3.0,
    },
    Pattern {
        name: "keyed diff 500 rows",
        source: keyed_diff,
        size: 500,
        ceiling: 30_000_000,
        speedup: 0.0,
    },
    Pattern {
        name: "Markdown blocks (replies)",
        source: markdown_blocks,
        size: 1,
        ceiling: 66_000,
        speedup: 3.3,
    },
    Pattern {
        name: "keyed view mount 10 rows",
        source: view_mount,
        size: 10,
        ceiling: 95_000,
        speedup: 2.0,
    },
    Pattern {
        name: "keyed view update 10 rows",
        source: view_update,
        size: 10,
        ceiling: 180_000,
        speedup: 5.0,
    },
];

/// Test-only fuel budgets of a measured run: enough for every v38 workload
/// and for v37 below the 500-row keyed diff, which runs out at this budget.
const MEASURED_BUDGETS: u64 = 40;

/// Fuel of `source(n)` under `profile`, with generous test-only budgets
/// (fuel and host calls, so v37's host calls do not end a scan early).
/// V37 is measured with its host work at the v38 fair rates, so both
/// profiles pay for the same work (v38 charges them anyway).
fn fuel(pattern: &Pattern, n: usize, profile: P) -> Run {
    run_budgets(
        &compile(&(pattern.source)(n), profile),
        MEASURED_BUDGETS,
        profile == V37,
    )
}

#[test]
fn codegen_v2_fuel_ceilings_hold() {
    println!("| workload | size | v37 fuel | v38 fuel | ratio | v37 /iter | v38 /iter |");
    for pattern in PATTERNS {
        let (before, after) = (
            fuel(pattern, pattern.size, V37),
            fuel(pattern, pattern.size, V38),
        );
        assert_eq!(after.error, None, "{}", pattern.name);
        // Identical output; v37 may exhaust its host-call budget instead.
        if before.error.is_none() {
            assert_eq!(before.lines, after.lines, "{}", pattern.name);
        }
        assert!(
            after.fuel <= pattern.ceiling,
            "{}: v38 fuel {} exceeds its ceiling {}",
            pattern.name,
            after.fuel,
            pattern.ceiling
        );
        let ratio = before.fuel as f64 / after.fuel as f64;
        if before.error.is_none() {
            assert!(
                ratio >= pattern.speedup,
                "{}: v37 {} / v38 {} = {ratio:.2} < {}",
                pattern.name,
                before.fuel,
                after.fuel,
                pattern.speedup
            );
        }
        // The marginal cost of one iteration, between half and full size.
        let half = pattern.size / 2;
        let per = |profile, full: &Run| {
            let small = fuel(pattern, half, profile);
            (small.error.is_none() && full.error.is_none())
                .then(|| (full.fuel - small.fuel) as f64 / (pattern.size - half) as f64)
        };
        let (old, new) = (per(V37, &before), per(V38, &after));
        let show = |value: Option<f64>| value.map_or_else(|| "-".to_owned(), |v| format!("{v:.1}"));
        println!(
            "| {} | {} | {} | {} | {} | {} | {} |",
            pattern.name,
            pattern.size,
            before
                .error
                .as_ref()
                .map_or_else(|| before.fuel.to_string(), |e| format!("fails: {e}")),
            after.fuel,
            if before.error.is_none() {
                format!("{ratio:.2}x")
            } else {
                "-".into()
            },
            show(old),
            show(new),
        );
    }
}

/// Fuel of console-shaped turns under v38 (fair host fuel, JSON decoded in
/// guest code), for the per-turn fuel table of the development
/// plan: `cargo test -p varta-runtime --lib console_turn_fuel -- --ignored
/// --nocapture`.
#[test]
#[ignore]
fn console_turn_fuel() {
    let measure = |label: &str, source: &str| {
        let run = run_test_budgets(&compile(source, V38), MEASURED_BUDGETS);
        assert_eq!(run.error, None, "{label}");
        println!("| {label} | {} |", run.fuel);
    };
    println!("| turn | v38 fuel |");
    measure("keyed view mount, 50 rows", &view_mount(50));
    measure("keyed view update, 50 rows", &view_update(50));
    measure("Markdown blocks, 3 replies", &markdown_blocks(3));
    measure(
        "24-member union events, 25 parses",
        include_str!("strict_member_cases/events.ts"),
    );
    // A response of `n` records (about 45 bytes each) parsed and summed.
    for records in [45, 225, 450] {
        measure(
            &format!("response of {} KB parsed", records * 45 / 1000),
            &json_parse(records),
        );
    }
}

/// Row counts of the keyed view that one frozen production budget (`abi::FUEL`, 1024
/// host calls) runs: v38 mounts and updates more rows than before the
/// hotspot work (74 and 35 rows at cb6795d) and than v37 (19 and 11).
#[test]
fn keyed_views_fit_one_production_budget() {
    const MOUNTED: usize = 120;
    const UPDATED: usize = 50;
    for (source, rows) in [
        (view_mount(MOUNTED), MOUNTED),
        (view_update(UPDATED), UPDATED),
    ] {
        let run = run_with(&compile(&source, V38), &[], 1);
        assert_eq!(run.error, None, "{rows} rows");
        assert!(
            run.lines[0].starts_with(&format!("varta log: {rows} ")),
            "{:?}",
            run.lines
        );
    }
}

/// The most keyed-view rows one v38 production turn (`budget::TURN_FUEL`,
/// 1024 host calls) mounts and updates (bisection):
/// `cargo test -p varta-runtime --lib keyed_view_rows_per_v38_turn -- --ignored --nocapture`.
#[test]
#[ignore]
fn keyed_view_rows_per_v38_turn() {
    const LARGEST: usize = 1 << 14;
    let budgets = varta_common::budget::TURN_FUEL / abi::FUEL;
    for (name, view) in [
        ("mount", view_mount as fn(usize) -> String),
        ("update", view_update),
    ] {
        let fits = |n: usize| run_with(&compile(&view(n), V38), &[], budgets).error;
        let (mut low, mut high) = (0, 1);
        while high < LARGEST && fits(high).is_none() {
            low = high;
            high *= 2;
        }
        while high - low > 1 {
            let middle = (low + high) / 2;
            if fits(middle).is_none() {
                low = middle;
            } else {
                high = middle;
            }
        }
        println!(
            "keyed view {name}: {low} rows (at {high}: {:?})",
            fits(high)
        );
    }
}

/// The largest size of every workload that one production budget runs
/// (bisection; the "Max size" column of the development-plan table):
/// `cargo test -p varta-runtime --lib codegen_v2_largest_sizes -- --ignored
/// --nocapture`.
#[test]
#[ignore]
fn codegen_v2_largest_sizes() {
    const LARGEST: usize = 1 << 20;
    for pattern in PATTERNS {
        let mut line = format!("| {} |", pattern.name);
        for profile in [V37, V38] {
            let fits = |n: usize| {
                run_with(&compile(&(pattern.source)(n), profile), &[], 1)
                    .error
                    .is_none()
            };
            let (mut low, mut high) = (0, 1);
            while high < LARGEST && fits(high) {
                low = high;
                high *= 2;
            }
            while high - low > 1 {
                let middle = (low + high) / 2;
                if fits(middle) {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            line += &format!(" {low} |");
        }
        println!("{line}");
    }
}

#[cfg(not(feature = "codegen-v2-all-profiles"))]
fn digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// Digests of these case programs under v37, produced by the compiler before
/// the codegen-v2 stream (8fc9965).
#[cfg(not(feature = "codegen-v2-all-profiles"))]
const FROZEN_V37: &[(&str, u64)] = &[
    (
        include_str!("collection_cases/strings.ts"),
        0xa352_8a5b_07e2_d111,
    ),
    (
        include_str!("language_core_cases/switches.ts"),
        0x4c79_e490_dec1_6b13,
    ),
    (
        include_str!("regexp_cases/pilot_text.ts"),
        0x6be6_9415_4a8f_6473,
    ),
    (
        include_str!("v37_integration_cases/sse_lines.ts"),
        0xb8b5_6295_cfdb_2f63,
    ),
    (
        include_str!("value15_cases/dictionary_json.ts"),
        0x0fcc_b71e_841d_5b17,
    ),
    (
        include_str!("web_cases/headers_workload.ts"),
        0x89d2_88f2_5bd8_f9bf,
    ),
    (
        include_str!("promise_cases/ordering.ts"),
        0xa7ca_91ce_ef3c_cedc,
    ),
    (
        include_str!("generator_cases/protocol.ts"),
        0xda53_b10a_94a6_f5cc,
    ),
    (
        include_str!("class_generic_cases/classes.ts"),
        0x4f35_5869_6543_9cf9,
    ),
    (
        include_str!("v34_cross_cases/json_tree.ts"),
        0x4dbe_117e_d97a_ba7d,
    ),
];

/// The test-only `codegen-v2-all-profiles` feature changes these bytes on
/// purpose.
#[test]
#[cfg(not(feature = "codegen-v2-all-profiles"))]
fn v37_modules_keep_their_bytes() {
    for (source, expected) in FROZEN_V37 {
        let bytes = varta_compiler::compile_typescript_with_profile(source, V37).unwrap();
        assert_eq!(digest(&bytes), *expected, "{source}");
    }
}

/// Case directories whose programs need the lifted v38 data limits.
const V38_LIMIT_CASES: [&str; 2] = ["data_limit_cases", "json_limit_cases"];

/// Case programs whose v38 run costs more fuel than v37's guest code with
/// its host work at the fair rates: (file, v37 fuel without its data-segment
/// copy, v38 fuel), pinned exactly. V38 runs Number formatting of
/// non-integers and BigInt arithmetic and conversion as linked guest code,
/// which costs more than v37's host calls at the fair rates (`String(1.5)`:
/// 530 against 4485; `String(7n * 2n)`: 1425 against 9558). Copy-on-write
/// memory images hid both on macOS, where v37 paid a 16 KiB page for its
/// data-segment copy.
const COSTLIER_UNDER_V38: [(&str, u64, u64); 2] = [
    ("unknown_cases/assertions.ts", 24_938, 26_166),
    ("value15_cases/values.ts", 73_371, 77_988),
];

/// Every runtime case program behaves identically under v37 and v38: same
/// acceptance, output lines and failure cause, and never more fuel like with
/// like (except [`COSTLIER_UNDER_V38`]): v38 charges no more for guest
/// instructions than v37 does, with the host work of both charged at the
/// published fair rates (`varta_common::fuel`; v37 is rated by a test-only
/// switch) and instantiation's data-segment copy, which v38 does not charge,
/// left out of v37's fuel. HTTP services are not started; network requests
/// are denied (no grant), timers granted.
#[test]
fn case_corpus_behaves_identically_under_v38() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().ends_with("_cases"))
        // The data-limits cases deliberately differ: v38 lifts the ceilings
        // at which v37 fails (`data_limit_tests`, `json_limit_tests`).
        .filter(|entry| !V38_LIMIT_CASES.contains(&&*entry.file_name().to_string_lossy()))
        .flat_map(|dir| std::fs::read_dir(dir.path()).unwrap().flatten())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "ts"))
        .filter(|path| !crate::frozen_profile_tests::deviates_under_v38(path))
        .collect();
    files.sort();
    let grants = [CapabilityGrant {
        id: "time.timer".into(),
        value: CapabilityValue::Boolean(true),
    }];
    let (mut compared, mut costlier) = (0, 0);
    for path in &files {
        let source = std::fs::read_to_string(path).unwrap();
        let old = varta_compiler::compile_typescript_with_profile(&source, V37);
        let new = varta_compiler::compile_typescript_with_profile(&source, V38);
        let (Ok(old), Ok(new)) = (old.as_ref(), new.as_ref()) else {
            assert!(
                old.is_err(),
                "{}: v38 rejects a v37 program: {:?}",
                path.display(),
                new.as_ref().err().map(ToString::to_string)
            );
            continue;
        };
        wasmtime::Module::new(&VartaRuntime::new().engine, new).unwrap();
        if http_contract::read(old).unwrap().is_some() {
            continue;
        }
        let (before, after) = (run_rated(old, &grants, 1), run_with(new, &grants, 1));
        let guest = before.fuel - instantiation_fuel(old);
        // Fuel, or the BigInt work budget derived from it.
        let exhausted = |run: &Run| {
            run.error
                .as_ref()
                .is_some_and(|e| e.contains("fuel") || e.contains("work budget"))
        };
        if let Some((_, old_fuel, new_fuel)) = COSTLIER_UNDER_V38
            .iter()
            .find(|(case, ..)| path.ends_with(case))
        {
            assert_eq!(
                (guest, after.fuel),
                (*old_fuel, *new_fuel),
                "{}",
                path.display()
            );
            costlier += 1;
        } else {
            // Both may run out of the same budget (a fair charge empties it).
            assert!(
                after.fuel <= guest || exhausted(&before) && exhausted(&after),
                "{}: v38 {} > v37 {} ({:?} / {:?})",
                path.display(),
                after.fuel,
                guest,
                after.error,
                before.error
            );
        }
        match &before.error {
            // V38 may finish where v37 ran out of fuel or host calls.
            Some(error)
                if error.contains("fuel")
                    || error.contains("work budget")
                    || error.contains("host operation budget") =>
            {
                assert!(after.lines.starts_with(&before.lines), "{}", path.display());
            }
            _ => {
                assert_eq!(before.lines, after.lines, "{}", path.display());
                assert_eq!(before.error, after.error, "{}", path.display());
            }
        }
        compared += 1;
    }
    assert!(compared >= 120, "only {compared} case programs compared");
    assert_eq!(costlier, COSTLIER_UNDER_V38.len());
}

/// Builder appends keep immutable semantics: aliases keep their length,
/// appends after an alias copy, and builders survive collections forced by
/// garbage between appends.
#[test]
fn string_builders_keep_aliases_and_survive_collections() {
    assert_eq!(
        output(
            r#"export function main(): void {
  let s = "ab";
  const t = s;
  s += "c";
  const u = s;
  s += "d";
  let w = u;
  w += "X";
  s += "e";
  console.log(t + " " + u + " " + s + " " + w);
  let empty = "";
  empty += "";
  console.log(String(empty.length));
}"#
        ),
        lines(&["ab abc abcde abcX", "0"])
    );
    // Collections run while builder blocks are live and between appends
    // (enough garbage for several geometric heap growth steps, v38
    // `data_limits`).
    let collected = r#"export function main(): void {
  let s = "";
  let kept = "";
  let bad = 0;
  for (let i = 0; i < 1500; i++) {
    s += "0123456789".charAt(i % 10);
    let garbage = "g";
    // Stable operands: both reads stay rooted by their binding while the
    // concatenation allocates (and collects).
    for (let j = 0; j < 8; j++) { garbage = garbage + garbage; }
    if (garbage.length !== 256 || garbage.charAt(255) !== "g") { bad++; }
    if (i % 500 === 499) { kept = kept + s.slice(s.length - 3); }
  }
  let check = bad;
  for (let i = 0; i < s.length; i++) { if (s.charCodeAt(i) !== 48 + (i % 10)) { check++; } }
  console.log(String(s.length) + " " + String(check) + " " + kept);
}"#;
    assert_eq!(output(collected), lines(&["1500 0 789789789"]));
    // Test-only budgets (CLAUDE.md §11) through the manifest request and
    // host ceiling: the same run as `output`, profiled to its end.
    let budgets = varta_common::budget::BudgetRequest {
        fuel_per_turn: Some(20 * abi::FUEL),
        ..Default::default()
    };
    let report = VartaRuntime::new().run_with(
        &crate::fuel_profile_tests::profiled(collected, V38),
        RunOptions {
            budgets,
            budget_ceiling: Some(budgets),
            fuel_profile: true,
            ..RunOptions::default()
        },
    );
    assert!(report.failure.is_none(), "{:?}", report.failure);
    let profile = report.fuel_profile.unwrap();
    let collections = profile
        .functions
        .iter()
        .find(|row| row.name == "<helper Collect>")
        .map_or(0, |row| row.calls);
    assert!(collections >= 2, "{collections} collections");
    // A loop variable rebound each iteration starts a fresh builder.
    assert_eq!(
        output(
            r##"export function main(): void {
  const all: string[] = [];
  for (let i = 0; i < 3; i++) { let line = "#"; line += String(i); line += "!"; all.push(line); }
  console.log(all.join(","));
}"##
        ),
        lines(&["#0!,#1!,#2!"])
    );
}

/// Equality fast paths agree with the ordering comparison at every length
/// around the word and chunk boundaries, including surrogates.
#[test]
fn string_equality_and_literals_agree_across_lengths() {
    let source = r#"export function main(): void {
  const unit = "a\ud83d\ude00b";
  let failures = 0;
  let checked = 0;
  for (let n = 0; n < 20; n++) {
    let x = "";
    let y = "";
    for (let i = 0; i < n; i++) { x += unit.charAt(i % 4); y += unit.charAt(i % 4); }
    for (let k = 0; k <= n; k++) {
      const z = k < n ? x.slice(0, k) + "Z" + x.slice(k + 1) : x + "Z";
      const same = x === y;
      const differs = x !== z;
      const ordered = (x < z || x > z) && !(x < y) && !(x > y);
      if (!same || !differs || !ordered) { failures++; }
      checked++;
    }
  }
  const c = unit.charAt(1);
  console.log(String(failures) + " " + String(checked) + " " + String(c === "\ud83d") + String(c !== "\ud83d") + String("" === unit.slice(0, 0)) + String(unit.slice(0, 1) === "a") + String("ab" === "ab") + String(unit === "a\ud83d\ude00b") + String(unit.slice(0, 3) === "a\ud83d\ude00") + String(unit.slice(0, 3) === "a\ud83d\ude01"));
}"#;
    assert_eq!(
        output(source),
        lines(&["0 210 truefalsetruetruetruetruetruefalse"])
    );
}

/// `charAt`/`charCodeAt` run in the guest with `ToIntegerOrInfinity` indices
/// and make no host call, so long scans fit the host-call budget.
#[test]
fn char_reads_follow_to_integer_or_infinity_without_host_calls() {
    let source = r#"export function main(): void {
  const s = "aé😀z";
  const at = (i: number): string => s.charAt(i);
  console.log(at(0) + "|" + at(-0.5) + "|" + at(1.9) + "|" + at(NaN) + "|" + at(-1) + "|" + at(5) + "|" + at(Infinity) + "|" + String(at(2).length));
  console.log(String(s.charCodeAt(1)) + " " + String(s.charCodeAt(2)) + " " + String(s.charCodeAt(3)) + " " + String(s.charCodeAt(-1)) + " " + String(s.charCodeAt(9)) + " " + String(s.charCodeAt(NaN)) + " " + String(s.charCodeAt(0.5)));
  let text = "";
  for (let i = 0; i < 300; i++) { text += "ab\n"; }
  let breaks = 0;
  for (let i = 0; i < text.length; i++) { if (text.charAt(i) === "\n" && text.charCodeAt(i) === 10) { breaks++; } }
  console.log(String(breaks));
}"#;
    let run = run_with(&compile(source, V38), &[], 1);
    assert_eq!(run.error, None);
    assert_eq!(
        run.lines,
        lines(&["a|a|é|a||||1", "233 55357 56832 NaN NaN 97 97", "300"])
    );
    // V37 reaches the same characters through host calls.
    let old = run_with(&compile(source, V37), &[], 1);
    assert!(
        old.error
            .as_ref()
            .is_some_and(|e| e.contains("host operation budget")),
        "{:?}",
        old.error
    );
    assert_eq!(old.lines[..2], run.lines[..2]);
}

/// Quiet loop bodies after a string `switch` reset no temporary roots, so the
/// switch must not leave its discriminant rooted across iterations.
#[test]
fn quiet_bodies_do_not_accumulate_roots() {
    let source = r#"export function main(): void {
  let total = 0;
  let key = "a";
  for (let i = 0; i < 20000; i++) {
    switch (key) {
      case "a": total = total + 1; break;
      case "b": total = total + 2; break;
      default: total = total + 3;
    }
    if (key === "z") { total = total - 1; }
  }
  console.log(String(total));
}"#;
    assert_eq!(output(source), lines(&["20000"]));
}

/// Number `%` is exact for safe integers in the guest and identical to the
/// host remainder for every other operand pair.
#[test]
fn guest_remainders_match_the_host_remainder() {
    let source = "export function main(): void { const xs: number[] = [7, -7, 4, -4, 0, -0, 5.5, -5.5, 1e300, 9007199254740993, Infinity, -Infinity, NaN, 3]; const ys: number[] = [3, -3, 2, -2, 0, -0, 0.5, Infinity, -Infinity, NaN, 1e-300, 9007199254740991]; let out = \"\"; for (const x of xs) { for (const y of ys) { const r = x % y; out += (r === 0 ? (1 / r > 0 ? \"+0\" : \"-0\") : String(r)) + \",\"; } } console.log(out); let n = 0; for (let i = 0; i < 3000; i++) { n = n + (i % 7); } console.log(String(n)); }";
    let new = run_with(&compile(source, V38), &[], 1);
    assert_eq!(new.error, None);
    let old = run_with(&compile(source, V37), &[], 1);
    // V37 spends one host call per `%`; its first line covers every pair.
    assert_eq!(old.lines.first(), new.lines.first());
    assert_eq!(new.lines[1], "varta log: 8994");
}
