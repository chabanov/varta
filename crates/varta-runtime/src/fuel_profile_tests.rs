//! Fuel profiling (`varta dev --profile-fuel`): exact refunds, attribution,
//! budgets and the separation of profile builds from ordinary artifacts.
use super::*;
use varta_common::language::LanguageProfile as P;

const V38: P = P::CheckedF64Utf16V38;

fn source(text: &str) -> varta_compiler::ModuleSource {
    varta_compiler::ModuleSource {
        path: "main.ts".into(),
        text: text.into(),
    }
}
struct NoImports;
impl varta_compiler::ModuleLoader for NoImports {
    fn resolve(
        &self,
        _: &std::path::Path,
        specifier: &str,
    ) -> Result<std::path::PathBuf, varta_compiler::CompilerError> {
        Err(varta_compiler::CompilerError::Invalid(format!(
            "unexpected import {specifier}"
        )))
    }
    fn load(&self, _: &std::path::Path) -> Result<String, varta_compiler::CompilerError> {
        Err(varta_compiler::CompilerError::Invalid(
            "unexpected load".into(),
        ))
    }
}
pub(crate) fn profiled(text: &str, profile: P) -> Vec<u8> {
    let bytes = varta_compiler::compile_modules_for_fuel_profile(source(text), &NoImports, profile)
        .unwrap_or_else(|e| panic!("{e}\n{text}"))
        .wasm;
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
fn plain(text: &str, profile: P) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(text, profile)
        .unwrap_or_else(|e| panic!("{e}\n{text}"))
}
/// Fuel of an ordinary run under the production budget.
fn consumed(bytes: &[u8]) -> u64 {
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(bytes).unwrap();
    let mut execution = execution::Execution::new(&runtime.engine, &[], None).unwrap();
    // The production fuel of the module's profile: one v38 turn or one
    // earlier execution.
    let budget = production_fuel(module.profile);
    execution.store.set_fuel(budget).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    let result = instance
        .get_func(&mut *store, "_start")
        .unwrap()
        .call(&mut *store, &[], &mut [])
        .map_err(RuntimeError::from)
        .and_then(|()| exceptions::finish(&instance, store))
        .and_then(|()| scheduler::checkpoint(&instance, store));
    drop(result);
    budget - store.get_fuel().unwrap()
}
fn production_fuel(profile: P) -> u64 {
    if profile.turn_budgets() {
        varta_common::budget::TURN_FUEL
    } else {
        abi::FUEL
    }
}

const PROGRAM: &str = r#"
function fib(n: number): number {
  if (n < 2) { return n; }
  return fib(n - 1) + fib(n - 2);
}
function label(n: number): string { return "fib " + String(n); }
export function main(): void {
  let text = "";
  for (let i = 0; i < 12; i++) { text += label(fib(i)); }
  console.log(String(text.length));
}
"#;

#[test]
fn probes_refund_their_cost_and_attribute_every_unit() {
    for profile in [V38, P::CheckedF64Utf16V37] {
        let bytes = profiled(PROGRAM, profile);
        let run = VartaRuntime::new()
            .run_fuel_profile(&bytes, &[], None)
            .unwrap();
        assert!(run.error.is_none(), "{:?}", run.error);
        assert_eq!(run.output.lines, ["varta log: 65"]);
        let profile_fuel = &run.profile;
        // The instrumented run consumes exactly the uninstrumented fuel.
        assert_eq!(profile_fuel.consumed, consumed(&plain(PROGRAM, profile)));
        assert_eq!(profile_fuel.budget, production_fuel(profile));
        assert_eq!(profile_fuel.unfinished, 0);
        let attributed: u64 = profile_fuel.functions.iter().map(|f| f.self_fuel).sum();
        // Instantiation (data segments, initial memory) runs no function.
        assert!(profile_fuel.outside > 0);
        assert_eq!(attributed + profile_fuel.outside, profile_fuel.consumed);
        let row = |prefix: &str| {
            profile_fuel
                .functions
                .iter()
                .find(|f| f.name.contains(prefix))
                .unwrap_or_else(|| panic!("{prefix}: {:?}", profile_fuel.functions))
        };
        let main = row("function main(): void");
        assert_eq!(main.calls, 1);
        assert_eq!(
            main.total_fuel,
            profile_fuel.consumed - profile_fuel.outside
        );
        let fib = row("function fib");
        // fib(0..12) makes sum(2 fib(i+1) - 1) activations.
        assert_eq!(fib.calls, 740);
        assert!(fib.total_fuel >= fib.self_fuel && fib.total_fuel < main.total_fuel);
        assert_eq!(row("function label").calls, 12);
        assert!(row("<helper Allocate>").calls > 0);
        assert!(main.name.starts_with("main.ts:7:8 "), "{}", main.name);
        // Rows are sorted by descending self fuel.
        assert!(
            profile_fuel
                .functions
                .windows(2)
                .all(|w| w[0].self_fuel >= w[1].self_fuel)
        );
    }
}

/// Programs with checked JSON export the host-free value ABI helpers, whose
/// call graph (allocator, collector, root push) stays uninstrumented so the
/// profile build passes validation; their fuel counts toward the caller and
/// the profile still sums to the uninstrumented run's fuel.
#[test]
fn json_programs_profile_without_probes_in_host_free_helpers() {
    const JSON: &str = r#"
interface Item { id: number; name: string; tags: string[] }
function total(items: Item[]): number {
  let t = 0;
  for (const item of items) { t = t + item.id + item.tags.length; }
  return t;
}
export function main(): void {
  let text = "[";
  for (let i = 0; i < 30; i++) {
    if (i > 0) { text += ","; }
    text += "{\"id\":" + String(i) + ",\"name\":\"item\",\"tags\":[\"a\",\"b\"]}";
  }
  text += "]";
  const items = JSON.parse(text) as Item[];
  console.log(String(total(items)) + " " + JSON.stringify(items[1]));
}
"#;
    // Serialization alone also exports the value ABI helpers.
    const STRINGIFY: &str = "export function main(): void { console.log(JSON.stringify(\"x\")); }";
    for profile in [V38, P::CheckedF64Utf16V37] {
        let run = VartaRuntime::new()
            .run_fuel_profile(&profiled(STRINGIFY, profile), &[], None)
            .unwrap();
        assert!(run.error.is_none(), "{profile:?}: {:?}", run.error);
        assert_eq!(run.output.lines, [r#"varta log: "x""#]);
        assert_eq!(run.profile.consumed, consumed(&plain(STRINGIFY, profile)));
        let attributed: u64 = run.profile.functions.iter().map(|f| f.self_fuel).sum();
        assert_eq!(attributed + run.profile.outside, run.profile.consumed);
    }
    for profile in [V38, P::CheckedF64Utf16V37] {
        // The profile run validates the build with every ordinary rule,
        // including the host-free value ABI call graph.
        let bytes = profiled(JSON, profile);
        let run = VartaRuntime::new()
            .run_fuel_profile(&bytes, &[], None)
            .unwrap();
        assert!(run.error.is_none(), "{profile:?}: {:?}", run.error);
        assert_eq!(
            run.output.lines,
            [r#"varta log: 495 {"id":1,"name":"item","tags":["a","b"]}"#]
        );
        let profile_fuel = &run.profile;
        assert_eq!(profile_fuel.consumed, consumed(&plain(JSON, profile)));
        assert_eq!(profile_fuel.unfinished, 0);
        let attributed: u64 = profile_fuel.functions.iter().map(|f| f.self_fuel).sum();
        assert_eq!(attributed + profile_fuel.outside, profile_fuel.consumed);
        let names: Vec<_> = profile_fuel.functions.iter().map(|f| &f.name).collect();
        assert!(
            names.iter().any(|name| name.contains("function total")),
            "{names:?}"
        );
        // V37's allocator is a host-free export the host calls, without
        // probes; v38 links its serializer, which calls the allocator as
        // ordinary guest code.
        assert_eq!(
            names
                .iter()
                .any(|name| name.as_str() == "<helper Allocate>"),
            profile == V38,
            "{names:?}"
        );
    }
}

#[test]
fn exhausted_budgets_stop_at_the_same_point_and_keep_the_profile() {
    let text = "function spin(n: number): number { let t = 0; while (true) { t = t + n; } return t; }\nexport function main(): void { console.log(\"start\"); console.log(String(spin(1))); }";
    let run = VartaRuntime::new()
        .run_fuel_profile(&profiled(text, V38), &[], None)
        .unwrap();
    let error = run.error.expect("budget exhausted");
    // V38 (`turn_budgets`) names the exhausted budget and its turn.
    assert!(
        matches!(&error, RuntimeError::Budget(exhausted) if exhausted.budget == varta_common::budget::BudgetKind::Fuel),
        "{error:?}"
    );
    assert_eq!(run.output.lines, ["varta log: start"]);
    assert_eq!(run.profile.unfinished, 2);
    assert_eq!(run.profile.consumed, consumed(&plain(text, V38)));
    let spin = &run.profile.functions[0];
    assert!(spin.name.contains("function spin"), "{spin:?}");
    assert!(spin.self_fuel > abi::FUEL * 9 / 10, "{spin:?}");
}

#[test]
fn probes_are_not_host_operations_and_profile_builds_are_not_artifacts() {
    // Thousands of probe calls stay outside the host-call budget.
    let text = "function one(n: number): number { return n + 1; }\nexport function main(): void { let t = 0; for (let i = 0; i < 3000; i++) { t = one(t); } console.log(String(t)); }";
    let bytes = profiled(text, V38);
    let run = VartaRuntime::new()
        .run_fuel_profile(&bytes, &[], None)
        .unwrap();
    assert!(run.error.is_none(), "{:?}", run.error);
    assert_eq!(run.output.lines, ["varta log: 3000"]);
    // An ordinary execution, validation or inspection rejects the probes.
    let runtime = VartaRuntime::new();
    for error in [
        runtime.run_wasm_bytes(&bytes, &[]).unwrap_err().to_string(),
        runtime.validate_module(&bytes).unwrap_err().to_string(),
        runtime.inspect_module(&bytes).unwrap_err().to_string(),
    ] {
        assert!(
            error.contains("unknown host import: varta_profile.enter"),
            "{error}"
        );
    }
    // The profiling entry point accepts only profile builds.
    let error = runtime
        .run_fuel_profile(&plain(text, V38), &[], None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("not a fuel profile build"), "{error}");
}

#[test]
fn guest_exits_are_not_profile_errors() {
    let text = "export function main(): void { console.log(\"bye\"); varta.process.exit(3); }";
    let bytes = profiled(text, V38);
    let grants = [varta_common::CapabilityGrant {
        id: "process.exit".into(),
        value: varta_common::CapabilityValue::Boolean(true),
    }];
    let run = VartaRuntime::new()
        .run_fuel_profile(&bytes, &grants, None)
        .unwrap();
    assert!(run.error.is_none(), "{:?}", run.error);
    assert_eq!(run.output.exit_code, 3);
    assert_eq!(run.profile.unfinished, 1);
}

/// The profiler and the v38 per-turn budget report observe one run: fuel
/// renewed for every timer turn keeps being attributed, the profile's
/// consumed fuel is the sum of the turns' fuel and its budget the sum of
/// their allowances.
#[test]
fn profiles_attribute_every_turn_of_the_budget_report() {
    let text = r#"
function work(n: number): number { let t = 0; for (let i = 0; i < n; i++) { t = t + i; } return t; }
export function main(): void {
  setTimeout(() => { console.log(String(work(200))); }, 1);
  setTimeout(() => { console.log(String(work(300))); }, 2);
  console.log(String(work(100)));
}
"#;
    let options = RunOptions {
        requested: vec![varta_common::CapabilityGrant {
            id: varta_common::scheduler::TIMER_CAPABILITY.into(),
            value: varta_common::CapabilityValue::Boolean(true),
        }],
        fuel_profile: true,
        ..RunOptions::default()
    };
    let bytes = profiled(text, V38);
    let report = scheduler::with_virtual_clock(|| VartaRuntime::new().run_with(&bytes, options));
    assert!(report.failure.is_none(), "{:?}", report.failure);
    assert_eq!(
        report.output.lines,
        ["varta log: 4950", "varta log: 19900", "varta log: 44850"]
    );
    let budget = report.budget.unwrap();
    let profile = report.fuel_profile.unwrap();
    assert_eq!(budget.total_turns, 3);
    assert_eq!(profile.consumed, budget.total_fuel);
    assert_eq!(profile.budget, budget.budgets.fuel_per_turn * 3);
    let work = profile
        .functions
        .iter()
        .find(|row| row.name.contains("work"))
        .unwrap();
    assert_eq!(work.calls, 3);
    assert!(work.self_fuel * 2 > profile.consumed, "{profile:?}");
}
