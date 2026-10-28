//! Capture metadata belongs to one prepared module and one execution.
use super::*;
use crate::execution::Execution;
use std::sync::Arc;
use varta_common::language::LanguageProfile;

fn prepare(runtime: &VartaRuntime, source: &str) -> PreparedModule {
    let bytes = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V30,
    )
    .unwrap();
    runtime.prepare_module(&bytes).unwrap()
}
fn execution(runtime: &VartaRuntime) -> Execution {
    Execution::new(&runtime.engine, &[], Some(&[])).unwrap()
}
#[test]
fn prepared_modules_retain_distinct_catalogs_across_clone_drop_and_execution() {
    let runtime = VartaRuntime::new();
    let first = prepare(
        &runtime,
        "export function main():void{const n=7;const f=():number=>n;console.log(f());}",
    );
    let second = prepare(
        &runtime,
        "export function main():void{const s='other';const f=():string=>s;console.log(f());}",
    );
    let first_owner = Arc::downgrade(first.function_catalog.as_ref().unwrap());
    let second_owner = Arc::downgrade(second.function_catalog.as_ref().unwrap());
    let clone = first.clone();
    drop(first);
    let mut a = execution(&runtime);
    let mut b = execution(&runtime);
    let ia = a.instantiate_prepared(&clone).unwrap();
    let ib = b.instantiate_prepared(&second).unwrap();
    assert!(Arc::ptr_eq(
        a.store.data().function_catalog.as_ref().unwrap(),
        &first_owner.upgrade().unwrap()
    ));
    assert!(Arc::ptr_eq(
        b.store.data().function_catalog.as_ref().unwrap(),
        &second_owner.upgrade().unwrap()
    ));
    assert!(!Arc::ptr_eq(
        a.store.data().function_catalog.as_ref().unwrap(),
        b.store.data().function_catalog.as_ref().unwrap()
    ));
    // A rejected second module must leave the original owner intact.
    assert!(a.instantiate_prepared(&second).is_err());
    assert!(Arc::ptr_eq(
        a.store.data().function_catalog.as_ref().unwrap(),
        &first_owner.upgrade().unwrap()
    ));
    drop(clone);
    drop(second);
    for (instance, run, expected) in [
        (ia, &mut a, "varta log: 7"),
        (ib, &mut b, "varta log: other"),
    ] {
        instance
            .get_typed_func::<(), ()>(&mut run.store, "_start")
            .unwrap()
            .call(&mut run.store, ())
            .unwrap();
        assert_eq!(run.store.data().output.lines, [expected]);
    }
    drop(a);
    drop(b);
    assert!(first_owner.upgrade().is_none());
    assert!(second_owner.upgrade().is_none());
}
#[test]
fn prepared_failed_instantiation_clears_catalog_and_cannot_be_retried() {
    let runtime = VartaRuntime::new();
    let prepared = prepare(
        &runtime,
        "export function main():void{const f=():number=>7;console.log(f());}",
    );
    let mut run = execution(&runtime);
    // Deny the module's initial memory allocation with a tighter test ceiling.
    run.store.data_mut().limits = wasmtime::StoreLimitsBuilder::new().memory_size(0).build();
    assert!(run.instantiate_prepared(&prepared).is_err());
    assert!(run.store.data().function_catalog.is_none());
    assert!(run.instantiate_prepared(&prepared).is_err());
    assert!(run.store.data().function_catalog.is_none());
}
#[test]
fn prepared_legacy_and_raw_modules_do_not_inherit_capture_metadata() {
    let runtime = VartaRuntime::new();
    let bytes = varta_compiler::compile_typescript_with_profile(
        "export function main():void{const f=():number=>7;console.log(f());}",
        LanguageProfile::CheckedF64Utf16V29,
    )
    .unwrap();
    let prepared = runtime.prepare_module(&bytes).unwrap();
    assert!(prepared.function_catalog().is_none());
    let mut run = execution(&runtime);
    run.instantiate_prepared(&prepared).unwrap();
    assert!(run.store.data().function_catalog.is_none());
    let v30 = prepare(
        &runtime,
        "export function main():void{const f=():number=>7;console.log(f());}",
    );
    let mut raw = execution(&runtime);
    raw.instantiate(v30.module()).unwrap();
    assert!(raw.store.data().function_catalog.is_none());
    assert!(raw.instantiate_prepared(&v30).is_err());
    assert!(raw.store.data().function_catalog.is_none());
}
