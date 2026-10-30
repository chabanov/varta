//! V38 `strict_members`: names a closed type does not declare are compile
//! errors; v3 to v37 keep reading them as `undefined`. Per-union structural
//! member limits come from the profile registry. Expected logs of the
//! `strict_member_cases` programs and of the generated 200-member union were
//! produced by Node 24 (type stripping, `main()` appended) and are pinned
//! here; conversion failures, which Node does not check, are Varta's own.
use super::*;
use varta_common::language::LanguageProfile as P;
use varta_common::limits;

const V37: P = P::CheckedF64Utf16V37;
const V38: P = P::CheckedF64Utf16V38;

fn compile(source: &str, profile: P) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
/// Test-only per-turn fuel (CLAUDE.md §11): the 24-member fixture parses 25
/// events in one turn, which with JSON decoded in guest code takes about
/// 1.1M fuel (the development plan's per-turn fuel recommendation).
const TEST_FUEL_PER_TURN: u64 = 4 * abi::FUEL;
fn run(source: &str, profile: P) -> Vec<String> {
    let budgets = varta_common::budget::BudgetRequest {
        fuel_per_turn: Some(TEST_FUEL_PER_TURN),
        ..Default::default()
    };
    let report = VartaRuntime::new().run_with(
        &compile(source, profile),
        RunOptions {
            budgets,
            budget_ceiling: Some(budgets),
            ..RunOptions::default()
        },
    );
    if let Some(failure) = report.failure {
        panic!("{failure}\n{source}");
    }
    report.output.lines
}
fn logs(expected: &[&str]) -> Vec<String> {
    expected.iter().map(|l| format!("varta log: {l}")).collect()
}
fn rejected(source: &str, profile: P) -> String {
    varta_compiler::compile_typescript_with_profile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("{profile:?} accepted\n{source}"))
        .to_string()
}

/// The owner's repro: v37 prints `plain undefined`, v38 rejects the read
/// at the property with both names (TypeScript's TS2339).
#[test]
fn the_undeclared_read_runs_under_v37_and_is_rejected_under_v38() {
    let source = "interface CallPayload { name: string }\n\
                  export function main(): void {\n  \
                  const p: CallPayload = { name: \"now\" };\n  \
                  console.log(\"plain \" + p.tool_name);\n}\n";
    assert_eq!(run(source, V37), logs(&["plain undefined"]));
    let error = varta_compiler::compile_typescript_with_profile(source, V38).unwrap_err();
    let text = error.to_string();
    assert!(
        text.starts_with("input.ts:4:28: property tool_name is not declared by CallPayload"),
        "{text}"
    );
    // The typed category is the type-error category of the other member
    // diagnostics (`property … is not declared by every member …`), and the
    // range covers the property name.
    let diagnostic = error.diagnostic().expect("located diagnostic");
    assert_eq!(diagnostic.code, varta_common::DiagnosticCode::TypeError);
    assert_eq!(
        (diagnostic.range.start.column, diagnostic.range.end.column),
        (28, 37)
    );
}

/// Every static access form on every closed owner kind is rejected under
/// v38 with the property's span and both names; v37 keeps its behaviour
/// (reads of ordinary names yield `undefined`, writes were already errors).
#[test]
fn every_static_access_form_names_the_property_and_its_owner() {
    let call = "interface Call { name: string; n: number }\n";
    let union = "type Ev = { type: \"a\"; payload: { name: string } } \
                 | { type: \"b\"; payload: { tool_name: string } };\n";
    let cases: [(String, &str, Option<&str>); 13] = [
        (
            format!("{call}export function main(): void {{ const p: Call = {{ name: \"x\", n: 1 }}; console.log(String(p.tool_name)); }}"),
            "property tool_name is not declared by Call",
            Some("undefined"),
        ),
        (
            format!("{call}export function main(): void {{ const p: Call = {{ name: \"x\", n: 1 }}; p.tool_name = \"y\"; }}"),
            "property tool_name is not declared by Call",
            None,
        ),
        (
            format!("{call}export function main(): void {{ const p: Call = {{ name: \"x\", n: 1 }}; p.count += 1; }}"),
            "property count is not declared by Call",
            None,
        ),
        (
            format!("{call}export function main(): void {{ const p: Call = {{ name: \"x\", n: 1 }}; p.count++; }}"),
            "property count is not declared by Call",
            None,
        ),
        (
            format!("{call}export function main(): void {{ const p: Call = {{ name: \"x\", n: 1 }}; p.label ??= \"z\"; }}"),
            "property label is not declared by Call",
            None,
        ),
        (
            format!("{call}export function main(): void {{ const p: Call | undefined = {{ name: \"x\", n: 1 }}; console.log(String(p?.tool_name)); }}"),
            "property tool_name is not declared by Call",
            Some("undefined"),
        ),
        (
            format!("{call}export function main(): void {{ const p: Call = {{ name: \"x\", n: 1 }}; const {{ tool_name }} = p; console.log(String(tool_name)); }}"),
            "property tool_name is not declared by Call",
            Some("undefined"),
        ),
        (
            format!("{call}export function main(): void {{ const p: Call = {{ name: \"x\", n: 1 }}; console.log(String(p[\"tool_name\"])); }}"),
            "property tool_name is not declared by Call",
            Some("undefined"),
        ),
        (
            "class Box { a = 1 }\nexport function main(): void { const b = new Box(); console.log(String(b.size)); }".into(),
            "property size is not declared by Box",
            None,
        ),
        (
            "interface Call { payload: { name: string } }\nexport function main(): void { const c = JSON.parse('{\"payload\":{\"name\":\"x\"}}') as Call; console.log(String(c.payload.tool_name)); }".into(),
            "property tool_name is not declared by { name: string }",
            Some("undefined"),
        ),
        (
            format!("{union}function f(e: Ev): string {{ switch (e.type) {{ case \"a\": return String(e.payload.tool_name); default: return e.payload.tool_name; }} }}\nexport function main(): void {{ console.log(f({{ type: \"a\", payload: {{ name: \"n\" }} }})); }}"),
            "property tool_name is not declared by { name: string }",
            Some("undefined"),
        ),
        (
            format!("{union}function f(e: Ev): string {{ if (e.type === \"a\") return String(e.payload.tool_name); return e.payload.tool_name; }}\nexport function main(): void {{ console.log(f({{ type: \"a\", payload: {{ name: \"n\" }} }})); }}"),
            "property tool_name is not declared by { name: string }",
            Some("undefined"),
        ),
        // A view: the parameter type, not the wider argument, decides.
        (
            "interface Wide { name: string; extra: number }\ninterface Narrow { name: string }\nfunction show(v: Narrow): string { return String(v.extra); }\nexport function main(): void { const w: Wide = { name: \"n\", extra: 1 }; console.log(show(w)); }".into(),
            "property extra is not declared by Narrow",
            None,
        ),
    ];
    for (source, expected, v37) in &cases {
        let error = rejected(source, V38);
        assert!(error.contains(expected), "{error}\n{source}");
        // The diagnostic points at the property name.
        let name = expected
            .strip_prefix("property ")
            .and_then(|rest| rest.split(' ').next())
            .unwrap();
        let caret = error.lines().nth(2).unwrap_or_default();
        assert!(caret.contains(&"~".repeat(name.len() - 1)), "{error}");
        if let Some(line) = v37 {
            assert_eq!(&run(source, V37), &logs(&[line]), "{source}");
        }
    }
}

/// Declared optional fields, index signatures, `Record<string, T>`,
/// `unknown`, `object`, `in` tests and inherited names keep their semantics.
#[test]
fn optional_fields_dictionaries_unknown_and_in_tests_are_unchanged() {
    let source = r#"interface Opt { name: string; tag?: string }
interface Mixed { id: string; [key: string]: string }
export function main(): void {
  const o: Opt = { name: "n" };
  const t: string | undefined = o.tag;
  const m: Mixed = { id: "1", other: "x" };
  const d: Record<string, number> = { a: 1 };
  const u: unknown = JSON.parse('{"k":2}');
  let k = "none";
  if (typeof u === "object" && u !== null && "k" in u) k = "has k";
  console.log(String(t) + " " + m.other + " " + String(m["missing"]) + " " + String(d.a) + " " + String(d["b"]) + " " + k);
}
"#;
    let expected = logs(&["undefined x undefined 1 undefined has k"]);
    assert_eq!(run(source, V38), expected);
    assert_eq!(run(source, V37), expected);
    // `in` tests an undeclared name without reading it; a view may hold an
    // object that has it (Node prints the same).
    let tests = r#"interface Opt { name: string; tag?: string }
interface Wide { name: string; extra: number }
function has(o: Opt): string { return ("tag" in o ? "true" : "false") + " " + ("extra" in o ? "true" : "false"); }
export function main(): void {
  const w: Wide = { name: "w", extra: 1 };
  console.log(has({ name: "n" }) + " " + has(w));
}
"#;
    assert_eq!(run(tests, V38), logs(&["false false false true"]));
    // Inherited `Object.prototype` names keep their earlier errors.
    let inherited = "interface C { name: string }\nexport function main(): void { const p: C = { name: \"n\" }; console.log(String(p.toString === undefined)); }";
    for profile in [V37, V38] {
        assert!(
            rejected(inherited, profile).contains("unknown or inherited object property: toString"),
            "{profile:?}"
        );
    }
}

/// Every engine prelude compiles under v38's strict members (they declare
/// every name they read), and a program using them runs.
#[test]
fn engine_preludes_declare_every_name_they_read() {
    let source = r#"
export async function main(): Promise<void> {
  const h = new Headers({ "a": "1" });
  const c = new AbortController();
  const s = AbortSignal.timeout(1000);
  const bytes = new TextEncoder().encode("hé");
  const text = new TextDecoder().decode(bytes);
  const m = new Map<string, number>([["k", 1]]);
  const set = new Set<string>(["x"]);
  const u = new Uint8Array(2);
  const b = btoa("hi");
  const d = new Date(0);
  const r = /a+/g;
  const url = new URL("https://example.com/p?q=1");
  const q = new URLSearchParams("a=1");
  const e = encodeURIComponent("a b");
  const padded = "7".padStart(3, "0");
  const req = new Request("https://example.com/");
  const res = new Response("body");
  const body = await res.text();
  console.log([h.get("a"), String(c.signal.aborted), String(s.aborted), text, String(m.get("k")),
    String(set.has("x")), String(u.length), atob(b), String(d.getTime()), String(r.test("aa")),
    url.pathname, String(q.get("a")), e, padded, req.method, body, String.fromCharCode(65)].join(" "));
}
"#;
    // `AbortSignal.timeout` arms an (unreferenced) timer.
    let grants = [varta_common::CapabilityGrant {
        id: "time.timer".into(),
        value: varta_common::CapabilityValue::Boolean(true),
    }];
    let lines = VartaRuntime::new()
        .run_wasm_bytes(&compile(source, V38), &grants)
        .unwrap()
        .lines;
    assert_eq!(
        lines,
        logs(&["1 false false hé 1 true 2 hi 0 true /p 1 a%20b 007 GET body A"])
    );
}

/// The 24-member discriminated record union (a generic event stream):
/// `JSON.parse(text) as Event` selects each member by `type`, a `switch` on
/// it is exhaustive, and v37 rejects the union at its frozen limit.
#[test]
fn a_24_member_discriminated_union_parses_and_switches_like_node() {
    let source = include_str!("strict_member_cases/events.ts");
    assert_eq!(run(source, V38), logs(&EVENTS_LOG));
    let error = rejected(source, V37);
    assert!(
        error.contains(
            "union member count exceeds limit: union has 24 structural members; limit 16"
        ),
        "{error}"
    );
    assert!(error.starts_with("input.ts:28:1:"), "{error}");
}

const EVENTS_LOG: [&str; 25] = [
    "created 0 t0",
    "delta 11 15 -",
    "notice 22 g0+g1",
    "metric 9 165 r15",
    "completed 20 t20",
    "queued 7 3 n1",
    "tool_call 18 g0",
    "retry 5 121 none",
    "log 16 t16",
    "failed 3 63 -",
    "started 14 g0+g1+g2",
    "tool_result 1 77 r7",
    "paused 12 t12",
    "heartbeat 23 51 n17",
    "archived 10 g0+g1",
    "progress 21 33 none",
    "message 8 t8",
    "resumed 19 39 -",
    "cancelled 6 g0",
    "deleted 17 253 r23",
    "chunk 4 t4",
    "warning 15 27 n9",
    "checkpoint 2 g0+g1+g2",
    "timeout 13 209 none",
    r#"276 timeout {"seq":13,"detail":{"code":209,"reason":null},"type":"timeout"}"#,
];

/// A full collection before every job: each parsed member, its nested
/// records and arrays survive while the converter and the scheduler hold
/// them, and the output is unchanged.
#[test]
fn the_24_member_union_survives_forced_collections() {
    use varta_compiler::suspension_test_hook as hook;
    let bytes =
        hook::compile_with_profile(include_str!("strict_member_cases/events.ts"), V38).unwrap();
    let runtime = crate::VartaRuntime::new();
    let module = runtime.prepare_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(TEST_FUEL_PER_TURN).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    let start = instance.get_func(&mut *store, "_start").unwrap();
    start.call(&mut *store, &[], &mut []).unwrap();
    let collect = instance
        .get_typed_func::<(), ()>(&mut *store, hook::COLLECT)
        .unwrap();
    let resume = instance
        .get_typed_func::<(i64, i32, i64), i32>(
            &mut *store,
            varta_common::function::frame::RESUME_EXPORT,
        )
        .unwrap();
    let mut jobs = 0;
    while let Some(token) = store.data_mut().scheduler.next().unwrap() {
        // Test-only budget: each forced full collection is paid for outside
        // the execution's production fuel, which the jobs alone consume.
        let remaining = store.get_fuel().unwrap();
        store.set_fuel(abi::FUEL).unwrap();
        collect.call(&mut *store, ()).unwrap();
        store.set_fuel(remaining).unwrap();
        resume.call(&mut *store, (token, 0, 0)).unwrap();
        jobs += 1;
    }
    assert!(jobs >= 24, "{jobs}");
    assert!(store.data().scheduler.unhandled_rejection().is_none());
    assert_eq!(store.data().output.lines, logs(&EVENTS_LOG));
}

/// Unknown discriminants, missing discriminants and undeclared fields are
/// catchable `TypeError`s (the owner's strict checked JSON); Node keeps any
/// parsed object, so these expectations are Varta's.
#[test]
fn unmatched_discriminants_and_unknown_fields_are_catchable_errors() {
    let union = include_str!("strict_member_cases/events.ts")
        .split("const inputs")
        .next()
        .unwrap()
        .to_owned();
    let source = format!(
        "{union}function attempt(text: string): string {{
  try {{
    return describe(JSON.parse(text) as Event);
  }} catch (e) {{
    return e instanceof TypeError ? \"TypeError \" + e.message : \"other\";
  }}
}}
export function main(): void {{
  console.log(attempt('{{\"type\":\"created\",\"seq\":1,\"text\":\"ok\"}}'));
  console.log(attempt('{{\"type\":\"unknown_kind\",\"seq\":1,\"text\":\"t\"}}'));
  console.log(attempt('{{\"seq\":1,\"text\":\"t\"}}'));
  console.log(attempt('{{\"type\":\"created\",\"seq\":1,\"text\":\"t\",\"extra\":true}}'));
  console.log(attempt('{{\"type\":\"progress\",\"seq\":1,\"detail\":{{\"code\":1,\"reason\":null,\"more\":1}}}}'));
  console.log(attempt('[1]'));
  console.log(attempt('{{\"type\":\"deleted\",\"seq\":2,\"detail\":{{\"code\":5,\"reason\":\"gone\"}}}}'));
}}
"
    );
    let lines = run(&source, V38);
    assert_eq!(lines[0], "varta log: created 1 ok");
    assert_eq!(lines[6], "varta log: deleted 2 5 gone");
    for line in &lines[1..6] {
        assert!(line.starts_with("varta log: TypeError "), "{lines:?}");
    }
}

/// A generated union of `count` record members over three shapes,
/// discriminated by `kind`, with `body` as the body of `main`.
fn generated_union(count: usize, body: &str) -> String {
    let mut source = String::new();
    for i in 0..count {
        source.push_str(&match i % 3 {
            0 => format!("interface M{i} {{ kind: \"k{i}\"; v: number }}\n"),
            1 => format!("interface M{i} {{ kind: \"k{i}\"; s: string; n?: number }}\n"),
            _ => format!("interface M{i} {{ kind: \"k{i}\"; items: {{ id: number }}[] }}\n"),
        });
    }
    source.push_str("type U =\n");
    for i in 0..count {
        source.push_str(&format!("  | M{i}\n"));
    }
    source.push_str(";\nfunction show(u: U): string {\n  switch (u.kind) {\n");
    for i in 0..count {
        source.push_str(&match i % 3 {
            0 => format!("    case \"k{i}\": return `k{i} ${{u.v}}`;\n"),
            1 => format!("    case \"k{i}\": return `k{i} ${{u.s}} ${{u.n ?? -1}}`;\n"),
            _ => format!(
                "    case \"k{i}\": return `k{i} ${{u.items.map((x) => x.id).join(\",\")}}`;\n"
            ),
        });
    }
    source.push_str("  }\n}\n");
    source.push_str(&format!("export function main(): void {{\n{body}\n}}\n"));
    source
}

const GENERATED_BODY: &str = r#"  const texts: string[] = [
    '{"kind":"k0","v":0}',
    '{"v":3,"kind":"k3"}',
    '{"kind":"k100","s":"hundred","n":7}',
    '{"kind":"k101","items":[{"id":1},{"id":2}]}',
    '{"kind":"k198","v":198}',
    '{"s":"last","kind":"k196"}',
  ];
  for (const text of texts) console.log(show(JSON.parse(text) as U));
  const u = JSON.parse('{"kind":"k197","items":[]}') as U;
  console.log(u.kind + " " + JSON.stringify(u));"#;

const GENERATED_LOG: [&str; 7] = [
    "k0 0",
    "k3 3",
    "k100 hundred 7",
    "k101 1,2",
    "k198 198",
    "k196 last -1",
    r#"k197 {"kind":"k197","items":[]}"#,
];

/// A 200-member union under v38: checked conversion picks by discriminant,
/// an unknown discriminant is a catchable error, and v37 rejects it.
#[test]
fn a_200_member_discriminated_union_is_admitted_under_v38() {
    const COUNT: usize = 200;
    const { assert!(limits::FROZEN.union_members < COUNT && COUNT < limits::WASM.union_members) };
    let source = generated_union(COUNT, GENERATED_BODY);
    assert_eq!(run(&source, V38), logs(&GENERATED_LOG));
    // V37 stops earlier, at its frozen 128 record shapes.
    let error = rejected(&source, V37);
    assert!(
        error.contains("object shape count exceeds limit"),
        "{error}"
    );
    let failing = generated_union(
        COUNT,
        r#"  for (const text of ['{"kind":"k200","v":1}', '{"kind":"k0","v":1,"w":2}', '{"kind":"k1","s":1}']) {
    try {
      console.log(show(JSON.parse(text) as U));
    } catch (e) {
      console.log(e instanceof TypeError ? "TypeError" : "other");
    }
  }
  console.log(show(JSON.parse('{"kind":"k198","v":8}') as U));"#,
    );
    assert_eq!(
        run(&failing, V38),
        logs(&["TypeError", "TypeError", "TypeError", "k198 8"])
    );
}

/// Selecting a member costs one discriminant lookup and a compare chain:
/// the fuel of a conversion grows with the member's position by a small
/// per-member step, not by a key lookup per member.
#[test]
fn discriminant_dispatch_costs_one_lookup_and_a_compare_chain() {
    let fuel = |count: usize, body: &str| {
        let bytes = compile(&generated_union(count, body), V38);
        let report = VartaRuntime::new().run_with(&bytes, RunOptions::default());
        assert!(
            report.failure.is_none(),
            "{:?}",
            report.failure.map(|e| e.to_string())
        );
        report.budget.expect("budget report").total_fuel
    };
    let parse = |member: usize| {
        format!(
            "  const u = JSON.parse('{{\"kind\":\"k{member}\",\"v\":1}}') as U;\n  console.log(\"k\");"
        )
    };
    let read = "  const u: U = { kind: \"k0\", v: 1 };\n  console.log(u.kind);";
    let baseline = "  console.log(\"k\");";
    let mut measured = Vec::new();
    for count in [24usize, 200] {
        let base = fuel(count, baseline);
        let first = fuel(count, &parse(0)) - base;
        // The last member with the `v` shape.
        let index = (count - 1) / 3 * 3;
        let last = fuel(count, &parse(index)) - base;
        let field = fuel(count, read) - base;
        let step = (last - first) / index as u64;
        println!(
            "{count} members: conversion selecting the first {first}, the last {last} \
             ({step} per member); field read through the union {field}"
        );
        measured.push((first, step, field));
    }
    let [(first, step, field), (wide_first, wide_step, wide_field)] = measured[..] else {
        unreachable!("two measurements")
    };
    // Each member before the match costs one literal test (about 55 fuel,
    // a short string comparison); each member after it one flag test.
    // Neither depends on the member count, and no member repeats the key
    // lookup.
    assert!(step < 100 && wide_step <= step, "{step} {wide_step}");
    assert!((wide_first - first) / 176 < 20, "{first} {wide_first}");
    assert!((wide_field - field) / 176 < 20, "{field} {wide_field}");
}

/// A checked JSON schema union with eight members of distinct kinds (one
/// more than the frozen seven) is admitted for host-schema parsing in v38.
#[test]
fn schema_unions_beyond_seven_members_parse_under_v38() {
    let source = r#"interface Item { id: number }
type Any = null | undefined | boolean | number | string | bigint | Item | Item[];
export function main(): void {
  const values: Any[] = [];
  for (const text of ['null', 'true', '2.5', '"s"', '{"id":4}', '[{"id":5}]']) values.push(JSON.parse(text) as Any);
  console.log(values.map((v) => typeof v + ":" + JSON.stringify(v)).join(" "));
}
"#;
    assert_eq!(
        run(source, V38),
        logs(&[
            r#"object:null boolean:true number:2.5 string:"s" object:{"id":4} object:[{"id":5}]"#
        ])
    );
    let error = rejected(source, V37);
    assert!(
        error.contains("union member count: union has 8 members; limit 7"),
        "{error}"
    );
}

/// A v38 module whose union exceeds the frozen limits fails before any
/// guest effect when relabelled as v37: at its host ABI section, and with
/// that downgraded too, before instantiation at the frozen checks.
#[test]
fn a_v38_module_with_wide_unions_fails_as_v37_before_effects() {
    use crate::language_tests::{host_abi, replace};
    use varta_common::language::{LanguageContract, SCHEMA_VERSION};
    let bytes = compile(include_str!("strict_member_cases/events.ts"), V38);
    let v37 = LanguageContract {
        schema_version: SCHEMA_VERSION,
        profile: V37,
    }
    .to_bytes()
    .unwrap();
    let relabelled = replace(&bytes, &v37);
    let runtime = VartaRuntime::new();
    assert!(matches!(
        runtime.validate_module(&relabelled),
        Err(RuntimeError::Language(_))
    ));
    assert!(matches!(
        runtime.run_wasm_bytes(&relabelled, &[]),
        Err(RuntimeError::Language(_))
    ));
    let downgraded = host_abi(&relabelled, Some(abi::NET_REQUEST_VERSION));
    assert!(runtime.validate_module(&downgraded).is_err());
    assert!(runtime.run_wasm_bytes(&downgraded, &[]).is_err());
    assert_eq!(
        run(include_str!("strict_member_cases/events.ts"), V38),
        logs(&EVENTS_LOG)
    );
}

/// A `switch` over a discriminant path whose clauses cover every member is
/// exhaustive without `default` (interfaces, inline object types, nested
/// paths), each clause keeps its narrowing, and v37 keeps rejecting the
/// missing final return.
#[test]
fn switches_over_discriminant_paths_are_exhaustive_like_typescript() {
    let source = include_str!("strict_member_cases/switches.ts");
    assert_eq!(
        run(source, V38),
        logs(&["b1 a", "12 9 0", "x:hi y#4", "first second 2", "z#9"])
    );
    let error = rejected(source, V37);
    assert!(
        error.starts_with("input.ts:7:1: value-returning function must return on every path\n"),
        "{error}"
    );
}

/// A clause missing from such a `switch` is rejected with the members it
/// leaves unhandled; v37 keeps its earlier text.
#[test]
fn a_missing_discriminant_case_names_the_unhandled_members() {
    let interfaces = "interface A { kind: \"a\" }\ninterface B { kind: \"b\"; n: number }\n\
                      interface C { kind: \"c\"; s: string }\ntype U = A | B | C;\n\
                      function f(u: U): string {\n  switch (u.kind) {\n    case \"a\": return \"a\";\n  }\n}\n\
                      export function main(): void { console.log(f({ kind: \"a\" })); }\n";
    let error = rejected(interfaces, V38);
    assert!(
        error.starts_with(
            "input.ts:5:1: value-returning function must return on every path; \
             the switch at 6:3 does not handle B | C"
        ),
        "{error}"
    );
    let v37 = rejected(interfaces, V37);
    assert!(
        v37.starts_with("input.ts:5:1: value-returning function must return on every path\n"),
        "{v37}"
    );
    let nested = "interface E { payload: { type: \"x\"; a: number } | { type: \"y\"; b: string } }\n\
                  function g(e: E): string {\n  switch (e.payload.type) {\n    case \"y\": return e.payload.b;\n  }\n}\n\
                  export function main(): void { console.log(g({ payload: { type: \"y\", b: \"q\" } })); }\n";
    let error = rejected(nested, V38);
    assert!(
        error.contains("the switch at 3:3 does not handle { a: number; type: 'x' }"),
        "{error}"
    );
    // A statement after the switch is what the missing return follows.
    let later = interfaces.replace("  }\n}\n", "  }\n  const x = 1;\n}\n");
    let error = rejected(&later, V38);
    assert!(
        error.starts_with("input.ts:5:1: value-returning function must return on every path\n"),
        "{error}"
    );
}

/// Strict members cover the wave-2 web-bytes paths: a class merged with an
/// interface declares the merged members and nothing else, and
/// `process.env` stays a dictionary-like view whose static names read
/// through `env.get` instead of failing as undeclared.
#[test]
fn merged_classes_are_strict_and_process_env_keeps_its_dictionary_rule() {
    let merged = |read: &str| {
        format!(
            "interface R {{ a: number; b: string }} interface C extends R {{}}
             class C {{ constructor() {{ Object.assign(this, {{ a: 1, b: 'y' }}); }} }}
             export function main(): void {{ const c = new C(); console.log({read}); }}"
        )
    };
    assert_eq!(run(&merged("String(c.a) + c.b"), V38), logs(&["1y"]));
    let error = rejected(&merged("String(c.zzz)"), V38);
    assert!(
        error.contains("property zzz is not declared by C"),
        "{error}"
    );
    let env = "export function main(): void {
       console.log(`${globalThis.process.env.VARTA_STRICT_UNSET} ${globalThis.process.env.VARTA_STRICT_OTHER}`);
     }";
    // Without a grant each static read is the recoverable `env.get` denial.
    assert_eq!(
        run(env, V38),
        logs(&[
            "<capability denied: env> (VARTA_STRICT_UNSET)",
            "<capability denied: env> (VARTA_STRICT_OTHER)",
            "undefined undefined",
        ])
    );
}

/// A union of 200 record members without discriminants (`web_bytes` strict
/// matching) admitted by the registry's member limit: a cell holding a
/// member is tested by a flat sequence, an object converts to its one exact
/// match, and an object that matches none is a catchable `TypeError`
/// (every member is tried, so one conversion costs about 200 trial builds).
#[test]
fn a_200_member_undiscriminated_union_converts_by_strict_matching() {
    const COUNT: usize = 200;
    let mut source = String::new();
    for i in 0..COUNT {
        source.push_str(&format!("interface M{i} {{ f{i}: number }}\n"));
    }
    source.push_str("type U =\n");
    for i in 0..COUNT {
        source.push_str(&format!("  | M{i}\n"));
    }
    source.push_str(
        r#";
function convert(text: string): string {
  try {
    const u = JSON.parse(text) as U;
    const again: unknown = u;
    return JSON.stringify(again as U);
  } catch (e) {
    return e instanceof TypeError ? "TypeError" : "other";
  }
}
export function main(): void {
  for (const text of ['{"f199":3}', '{}']) {
    console.log(convert(text));
  }
}
"#,
    );
    assert_eq!(run(&source, V38), logs(&[r#"{"f199":3}"#, "TypeError"]));
}
