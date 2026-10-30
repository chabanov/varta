//! V38 `data_limits` JSON: `JSON.stringify` through the owned serializer
//! (`json_stringify_owned`, past the 1 MiB host result buffer) and checked
//! parsing of large, wide and deep values, with the v37 ceilings unchanged.
//! Expected logs of the `json_limit_cases` programs were produced by Node 24
//! (type stripping, `main()` appended, `varta.json.parse` shimmed as
//! `JSON.parse`) and are pinned here.
use super::*;
use varta_common::language::LanguageProfile as P;

const V37: P = P::CheckedF64Utf16V37;
const V38: P = P::CheckedF64Utf16V38;

fn compile(source: &str, profile: P) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    // Emitted modules must also pass backend validation, not only compilation.
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}

/// What one budgeted run produced.
struct Run {
    result: Result<Vec<String>, RuntimeError>,
    fuel: u64,
    pages: u64,
}

/// Run under an explicit test-only fuel budget of `fuel` production budgets
/// (CLAUDE.md §11); memory stays at the production 8 MiB budget.
fn run(source: &str, profile: P, fuel: u64) -> Run {
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&compile(source, profile)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.allow_test_wall_clock(std::time::Duration::from_secs(300));
    execution.store.set_fuel(fuel * abi::FUEL).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    let result = (|| {
        let start = instance.get_func(&mut *store, "_start").unwrap();
        start.call(&mut *store, &[], &mut [])?;
        exceptions::finish(&instance, store)?;
        scheduler::checkpoint(&instance, store)?;
        Ok::<(), RuntimeError>(())
    })()
    .map_err(|error| resources::classify(&instance, store, scheduler::typed(error)));
    let fuel = fuel * abi::FUEL - store.get_fuel().unwrap();
    let pages = instance
        .get_memory(&mut *store, "memory")
        .unwrap()
        .size(&*store);
    Run {
        result: result.map(|()| std::mem::take(&mut store.data_mut().output.lines)),
        fuel,
        pages,
    }
}
fn logs(run: Run, source: &str) -> Vec<String> {
    run.result.unwrap_or_else(|e| panic!("{e:?}\n{source}"))
}
fn lines(expected: &[&str]) -> Vec<String> {
    expected.iter().map(|s| format!("varta log: {s}")).collect()
}
fn failure(run: Run) -> String {
    format!("{:#}", run.result.expect_err("expected a failure"))
}

const ROUND_TRIP: &str = include_str!("json_limit_cases/round_trip.ts");
const LARGE_STRING: &str = include_str!("json_limit_cases/large_string.ts");
const DICTIONARY: &str = include_str!("json_limit_cases/dictionary.ts");
const DEPTH: &str = include_str!("json_limit_cases/depth.ts");

#[test]
fn v38_number_arrays_round_trip_past_1024_elements() {
    assert_eq!(
        logs(run(ROUND_TRIP, V38, 50), ROUND_TRIP),
        lines(&["10000 10000 40003 true", "100000 100000 400003 true"])
    );
    // V37 keeps its 64 KiB JSON input ceiling (a fatal limit).
    assert!(failure(run(ROUND_TRIP, V37, 50)).contains("checked JSON input byte limit"));
}

#[test]
fn v38_serializes_strings_past_the_host_result_buffer() {
    assert_eq!(
        logs(run(LARGE_STRING, V38, 50), LARGE_STRING),
        lines(&["655360 786434 655360 true"])
    );
}

#[test]
fn v38_dictionaries_round_trip_past_1024_entries() {
    assert_eq!(
        logs(run(DICTIONARY, V38, 5000), DICTIONARY),
        lines(&[r#"90013 90013 {"3":3,"7":7,"aaaa": 1,"jjji":1,"jjjj":1}"#])
    );
    assert!(failure(run(DICTIONARY, V37, 50)).contains("checked JSON input byte limit"));
}

#[test]
fn v38_values_nest_past_depth_64() {
    assert_eq!(
        logs(run(DEPTH, V38, 5000), DEPTH),
        lines(&["50 true", "500 true", "3000 true", "55894 2999 true"])
    );
    assert!(failure(run(DEPTH, V37, 200)).contains("checked JSON depth limit"));
}

/// Nesting beyond the host's JSON depth ceiling is a reported JSON limit
/// error for both directions, never a crashed host stack.
#[test]
fn v38_json_nesting_stops_at_the_host_depth_ceiling() {
    use varta_common::limits::V38_MAX_JSON_DEPTH;
    // `levels` objects-with-arrays nest 2 * levels - 1 deep. The host builds
    // them top-down; `wrap` adds one more level in the guest. (A guest loop
    // building such a chain is far costlier: each collection marks a chain
    // to its fixed point one level per heap pass.)
    let nested = |levels: usize, wrap: bool| {
        let wrapped = if wrap { "{c:[v]}" } else { "v" };
        let extra = if wrap { 8 } else { 0 };
        format!(
            "interface Node{{c:Node[]}}export function main():void{{\
             const t='{{\"c\":['.repeat({levels})+']}}'.repeat({levels});\
             const v=varta.json.parse<Node>(t);const w:Node={wrapped};\
             const same=JSON.stringify(w).length===t.length+{extra};console.log(`${{same}}`);}}"
        )
    };
    let within = V38_MAX_JSON_DEPTH.div_ceil(2);
    assert_eq!(
        logs(run(&nested(within, false), V38, 500), "within"),
        lines(&["true"])
    );
    let error = failure(run(&nested(within, true), V38, 500));
    assert!(
        error.contains("nesting exceeds the host depth ceiling of 10000"),
        "{error}"
    );
    // V37 keeps its depth of 64 and its message.
    let error = failure(run(
        "interface Node{c:Node[]}export function main():void{let v:Node={c:[]};\
         for(let i=0;i<40;i++){v={c:[v]};}console.log(JSON.stringify(v).length);}",
        V37,
        50,
    ));
    assert!(
        error.contains("JSON serialization: depth or node limit"),
        "{error}"
    );
    let levels = within + 1;
    let parse = format!(
        "interface Node{{c:Node[]}}export function main():void{{\
         const t='{{\"c\":['.repeat({levels})+']}}'.repeat({levels});\
         console.log(varta.json.parse<Node>(t).c.length);}}"
    );
    let error = failure(run(&parse, V38, 500));
    assert!(error.contains("checked JSON depth limit"), "{error}");
}

/// The owned serializer allocates through the module's value ABI helpers,
/// which keep their host-free validation; the v38 module binds
/// `json_stringify_owned` and never the v37 result-buffer import.
#[test]
fn v38_owned_serializer_binds_host_free_value_helpers() {
    use crate::contract_tests::export_index;
    let source = "export function main():void{console.log(JSON.stringify([1]));}";
    let bytes = compile(source, V38);
    let inspection = VartaRuntime::new().inspect_module(&bytes).unwrap();
    let names: Vec<_> = inspection.imports.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"json_stringify_owned"), "{names:?}");
    assert!(!names.contains(&"json_stringify_v17"), "{names:?}");
    assert!(
        inspection
            .exports
            .iter()
            .any(|e| e == varta_common::value::VALUE_ALLOCATE)
    );
    let v37 = VartaRuntime::new()
        .inspect_module(&compile(source, V37))
        .unwrap();
    assert!(v37.imports.iter().any(|i| i.name == "json_stringify_v17"));
    assert!(
        !v37.exports
            .iter()
            .any(|e| e == varta_common::value::VALUE_ALLOCATE)
    );
    // The allocator keeps its checked signature (and host-free call graph).
    let start = export_index(&bytes, "_start");
    let tampered = crate::contract_tests::rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            let name = varta_common::value::VALUE_ALLOCATE;
            let at = data
                .windows(name.len())
                .position(|s| s == name.as_bytes())
                .unwrap();
            data[at + name.len() + 1] = start;
        }
        vec![data]
    });
    let error = VartaRuntime::new().validate_module(&tampered).unwrap_err();
    assert!(error.to_string().contains("signature mismatch"), "{error}");
}

/// Strings past 1 MiB exist only under v38; v37 keeps its per-string ceiling.
#[test]
fn v37_keeps_its_string_and_result_ceilings() {
    let error = failure(run(LARGE_STRING, V37, 50));
    assert!(error.contains("unreachable"), "{error}");
}

/// Fuel and committed memory of JSON round trips under v38 (deterministic).
/// The JSON operations are measured as the difference to the same program
/// that only builds its input text.
#[test]
fn v38_json_round_trip_costs() {
    let program = |count: usize, json: bool| {
        format!(
            "export function main():void{{const t='['+'1.5,'.repeat({})+'-2e-7]';{}}}",
            count - 1,
            if json {
                "const v=varta.json.parse<number[]>(t);console.log(JSON.stringify(v).length);"
            } else {
                "console.log(t.length);"
            }
        )
    };
    for count in [10_000, 100_000] {
        let base = run(&program(count, false), V38, 50);
        let (base_fuel, base_pages) = (base.fuel, base.pages);
        assert!(base.result.is_ok());
        let full = run(&program(count, true), V38, 50);
        let (fuel, pages) = (full.fuel, full.pages);
        assert_eq!(
            logs(full, "round trip"),
            lines(&[&(count * 4 + 3).to_string()])
        );
        println!(
            "JSON round trip of {count} numbers: {} fuel ({} with input), {pages} pages ({base_pages} input only)",
            fuel - base_fuel,
            fuel
        );
        // A round trip stays far inside one production fuel budget per
        // 10k elements and inside the 8 MiB memory budget.
        assert!(
            fuel - base_fuel < abi::FUEL * (count as u64 / 10_000),
            "{fuel}"
        );
        assert!(pages * abi::WASM_PAGE_BYTES as u64 <= abi::MAX_MEMORY_BYTES as u64);
    }
}
