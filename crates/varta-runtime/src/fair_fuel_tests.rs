//! V38 fair host fuel: the published charges, their absence before v38, and
//! exhaustion by a charge as the ordinary typed fuel-budget error.
use crate::codegen_v2_tests::{compile, run_rated, run_with};
use crate::*;
use varta_common::{fuel, language::LanguageProfile as P};

const V37: P = P::CheckedF64Utf16V37;
const V38: P = P::CheckedF64Utf16V38;

/// `count` logs of a string of `units` UTF-16 units.
fn logs(count: usize, units: usize) -> String {
    format!(
        "export function main(): void {{ const s = \"{}\"; for (let i = 0; i < {count}; i++) {{ console.log(s); }} }}",
        "x".repeat(units)
    )
}

/// A log call costs `CALL` plus `BYTE` per argument byte under v38, and
/// nothing beyond its guest instructions under v37 (unless rated).
#[test]
fn v38_host_calls_pay_the_published_rates() {
    let fuel = |profile, count, units, rated: bool| {
        let bytes = compile(&logs(count, units), profile);
        let run = if rated {
            run_rated(&bytes, &[], 1)
        } else {
            run_with(&bytes, &[], 1)
        };
        assert_eq!(run.error, None);
        run.fuel
    };
    // The marginal fuel of 10 more calls of a 100-unit string.
    let call = fuel::CALL + fuel::work(100 * size_of::<u16>(), fuel::BYTE);
    let guest = fuel(V37, 20, 100, false) - fuel(V37, 10, 100, false);
    let v38 = fuel(V38, 20, 100, false) - fuel(V38, 10, 100, false);
    let rated = fuel(V37, 20, 100, true) - fuel(V37, 10, 100, true);
    assert_eq!(rated, guest + 10 * call, "v37 rated");
    // V38's loop instructions differ from v37's; the charge is exact.
    let v38_guest = {
        let bytes = compile(&logs(20, 100), V38);
        let small = compile(&logs(10, 100), V38);
        let runtime = VartaRuntime::new();
        let unrated = |bytes: &[u8]| {
            let mut execution = execution::Execution::new(&runtime.engine, &[], None).unwrap();
            execution.rate_host_work(false);
            let module = runtime.prepare_module(bytes).unwrap();
            execution.store.set_fuel(abi::FUEL).unwrap();
            let instance = execution.instantiate_prepared(&module).unwrap();
            let store = &mut execution.store;
            instance
                .get_func(&mut *store, "_start")
                .unwrap()
                .call(&mut *store, &[], &mut [])
                .unwrap();
            abi::FUEL - store.get_fuel().unwrap()
        };
        unrated(&bytes) - unrated(&small)
    };
    assert_eq!(v38, v38_guest + 10 * call, "v38");
}

/// A charge the remaining fuel cannot cover ends the turn with the typed
/// fuel-budget error, before the call's effect (no log line is written).
#[test]
fn an_unaffordable_charge_is_the_typed_fuel_budget_error() {
    // One log of an argument longer than the whole per-turn fuel.
    let units = (abi::FUEL as usize / size_of::<u16>()) + 1;
    let source = format!(
        "export function main(): void {{ const s = \"x\".repeat({units}); console.log(s); }}"
    );
    let report = VartaRuntime::new().run_with(&compile(&source, V38), RunOptions::default());
    let failure = report.failure.expect("the charge exhausts the fuel");
    assert!(
        matches!(&failure, RuntimeError::Budget(exhausted) if exhausted.budget == varta_common::budget::BudgetKind::Fuel),
        "{failure:?}"
    );
    assert!(report.output.lines.is_empty(), "{:?}", report.output.lines);
}

/// The Wasm SIMD proposal (`simd128`, fixed-width) validates in the
/// production engine configuration: a function with `v128.const` and
/// `v128.any_true` (checked in Node 24 with `WebAssembly.validate` too).
#[test]
fn simd128_validates_in_the_production_engine() {
    const MODULE: [u8; 45] = [
        0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0, 3, 2, 1, 0, 10, 25, 1, 23, 0, 253, 12, 0,
        0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 253, 83, 26, 11,
    ];
    wasmtime::Module::new(&VartaRuntime::new().engine, MODULE).unwrap();
}
