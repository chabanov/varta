//! Predicate index/reverse searches use snapshot ranges and live indexed values.
use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V25)
        .unwrap_or_else(|e| panic!("{e}\n{source}"))
}
fn run(source: &str, expected: &[&str]) {
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
}
#[test]
fn predicate_search_dense_results_and_empty_sentinels() {
    run(
        "export function main():void{const values=[2,4,6,4];console.log(values.findIndex(v=>v===4));console.log(values.findLastIndex(v=>v===4));console.log(values.findLast(v=>v===4));console.log(values.findIndex(v=>v===9));console.log(values.findLastIndex(v=>v===9));console.log(values.findLast(v=>v===9));const empty:number[]=[];console.log(empty.findIndex(()=>true));console.log(empty.findLastIndex(()=>true));console.log(empty.findLast(()=>true));}",
        &[
            "1",
            "3",
            "4",
            "-1",
            "-1",
            "undefined",
            "-1",
            "-1",
            "undefined",
        ],
    );
}
#[test]
fn predicate_search_traverses_in_each_direction_and_stops_at_first_match() {
    run(
        "export function main():void{const values=[1,2,3,4];console.log(values.findLastIndex((v,i)=>{console.log(i);return i===1;}));console.log(values.findIndex((v,i)=>{console.log(i);return i===1;}));console.log(values.findLast((v,i)=>{if(i!==3)throw 'late';return true;}));}",
        &["3", "2", "1", "1", "0", "1", "1", "4"],
    );
}
#[test]
fn predicate_search_visits_holes_and_preserves_absence() {
    run(
        "export function main():void{const values=[1,,3];console.log(values.findIndex(v=>v===undefined));console.log(values.findLastIndex(v=>v===undefined));console.log(values.findLast((v,i)=>i===1));console.log(Object.hasOwn(values,1));const holes:number[]=[];holes.length=4;console.log(holes.findIndex(v=>v===undefined));console.log(holes.findLastIndex(v=>v===undefined));}",
        &["1", "1", "undefined", "0", "0", "3"],
    );
}
#[test]
fn predicate_search_explicit_this_and_lexical_arrow_receiver() {
    run(
        "export function main():void{const values=[1,2,3,2];const predicate=function(this:{n:number},v:number|undefined):boolean{return v===this.n;};console.log(values.findIndex(predicate,{n:2}));console.log(values.findLastIndex(predicate,{n:2}));console.log(values.findLast(predicate,{n:2}));const factory=function(this:{n:number}):(v:number|undefined)=>boolean{return(v:number|undefined):boolean=>v===this.n;};const arrow=factory.call({n:2});console.log(values.findLastIndex(arrow,{n:99}));}",
        &["1", "3", "2", "3"],
    );
}
#[test]
fn predicate_search_callback_arity_defaults_and_original_value() {
    run(
        "export function main():void{const values=[1,2];console.log(values.findIndex(()=>true));console.log(values.findLastIndex(()=>true));console.log(values.findLast(()=>true));console.log(values.findIndex((v:number|undefined,i:number,a:number[],extra:number=7):boolean=>extra===7));const holes:number[]=[];holes.length=1;console.log(holes.findLast((v:number=42):boolean=>v===42));console.log(holes.findLastIndex((v:number=42):boolean=>v===42));}",
        &["0", "1", "2", "0", "undefined", "0"],
    );
}
#[test]
fn predicate_search_ignores_appended_elements_after_snapshot() {
    run(
        "export function main():void{const first=[1,2];console.log(first.findIndex((v,i,a)=>{if(i===0)a.push(9);console.log(i);return v===9;}));console.log(first.length);const last=[1,2];console.log(last.findLastIndex((v,i,a)=>{if(i===1)a.push(9);console.log(i);return v===9;}));console.log(last.length);}",
        &["0", "1", "-1", "3", "1", "0", "-1", "3"],
    );
}
#[test]
fn predicate_search_reads_live_replacements() {
    run(
        "export function main():void{const first=[1,2,3];console.log(first.findIndex((v,i,a)=>{if(i===0)a[1]=9;return v===9;}));const last=[1,2,3];console.log(last.findLastIndex((v,i,a)=>{if(i===2)a[1]=9;return v===9;}));}",
        &["1", "1"],
    );
}
#[test]
fn predicate_search_visits_deleted_unseen_positions_as_undefined() {
    run(
        "export function main():void{const first=[1,2,3];console.log(first.findIndex((v,i,a)=>{if(i===0)delete a[1];return v===undefined;}));const last=[1,2,3];console.log(last.findLastIndex((v,i,a)=>{if(i===2)delete a[1];return v===undefined;}));}",
        &["1", "1"],
    );
}
#[test]
fn predicate_search_truncation_does_not_shorten_the_snapshot_range() {
    run(
        "export function main():void{const first=[1,2,3];console.log(first.findIndex((v,i,a)=>{if(i===0)a.length=1;return i===2&&v===undefined;}));const last=[1,2,3];console.log(last.findLastIndex((v,i,a)=>{if(i===2)a.length=0;return v===undefined;}));const cached=[1,2,3];console.log(cached.findLast((v,i,a)=>{a.length=0;return true;}));}",
        &["2", "1", "3"],
    );
}
#[test]
fn predicate_search_regrowth_observes_new_values_and_holes() {
    run(
        "export function main():void{const values=[1,2,3];console.log(values.findLastIndex((v,i,a)=>{if(i===2){a.length=0;a.length=3;a[1]=8;}return v===8;}));console.log(Object.hasOwn(values,0));console.log(Object.hasOwn(values,1));console.log(Object.hasOwn(values,2));}",
        &["1", "0", "1", "0"],
    );
}
#[test]
fn predicate_search_found_reference_is_cached_before_callback_mutation() {
    run(
        "export function main():void{const values=[{n:1},{n:2}];const found=values.findLast((v,i,a)=>{delete a[i];return true;});if(found){console.log(found.n);}console.log(Object.hasOwn(values,1));}",
        &["2", "0"],
    );
}
#[test]
fn predicate_search_uses_guest_truth_for_ordinary_predicate_results() {
    run(
        "export function main():void{const values=[1];const empty:number[]=[];console.log(values.findIndex(()=>({n:1})));console.log(values.findLastIndex(()=>empty));console.log(values.findLastIndex(()=>Error('truth')));console.log(values.findIndex(()=>(()=>1)));console.log(values.findIndex(()=>0/0));console.log(values.findIndex(()=>''));console.log(values.findLastIndex(()=>false));console.log(values.findLastIndex(()=>undefined));console.log(values.findIndex(()=>null));console.log(values.findIndex(()=>-0));console.log(values.findIndex(()=>{console.log('void');}));}",
        &[
            "0", "0", "0", "0", "-1", "-1", "-1", "-1", "-1", "-1", "void", "-1",
        ],
    );
}
#[test]
fn predicate_search_callback_throws_and_finalizers_preserve_recovery() {
    run(
        "export function main():void{const values=[1,2];try{values.findLastIndex((v,i)=>{try{throw 'stop';}finally{console.log(i);}});}catch(e){console.log(e);}console.log(values.findIndex(()=>true));}",
        &["1", "stop", "0"],
    );
}
#[test]
fn predicate_search_snapshots_length_after_ignored_argument_effects() {
    run(
        "export function main():void{const first=[1];console.log(first.findIndex(v=>v===2,undefined,first.push(2)));const last=[1];console.log(last.findLastIndex(v=>v===2,undefined,last.push(2)));console.log(last.findLast(v=>v===3,undefined,last.push(3)));}",
        &["1", "1", "3"],
    );
}
#[test]
fn predicate_search_caches_source_callback_and_this_before_ignored_writes() {
    run(
        "export function main():void{let values=[1];let receiver={n:1};let predicate=function(this:{n:number},v:number|undefined):boolean{return v===this.n;};console.log(values.findIndex(predicate,receiver,predicate=function(this:{n:number},v:number|undefined):boolean{return false;},receiver={n:2},values=[9]));console.log(values.length);console.log(receiver.n);}",
        &["0", "1", "2"],
    );
}
#[test]
fn predicate_search_receiver_is_cached_before_callback_factory_reassignment() {
    run(
        "export function main():void{let values=[1,2];const factory=():(v:number|undefined)=>boolean=>{values=[9];return(v:number|undefined):boolean=>v===2;};console.log(values.findIndex(factory()));console.log(values.length);}",
        &["1", "1"],
    );
}
#[test]
fn predicate_search_existing_find_checks_callback_before_this_argument() {
    let source = "function make(value:{n:number}):(n:number|undefined)=>boolean{console.log(value.n);return(n:number|undefined):boolean=>true;}export function main():void{let value:{n:number}|undefined={n:7};const values=[1];if(value){console.log(values.find(make(value),value=undefined));console.log(Boolean(value));}}";
    let wasm = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V24,
    )
    .unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&wasm, &[])
            .unwrap()
            .lines,
        vec!["varta log: 7", "varta log: 1", "varta log: 0"]
    );
    let unsafe_source = "export function main():void{let value:{n:number}|undefined={n:7};const values=[1];const make=():(n:number|undefined)=>boolean=>{value=undefined;return(n:number|undefined):boolean=>true;};if(value){values.find(make(),value.n);}}";
    assert!(
        varta_compiler::compile_typescript_with_profile(
            unsafe_source,
            LanguageProfile::CheckedF64Utf16V24
        )
        .is_err()
    );
}
#[test]
fn predicate_search_default_budgets_cover_maximum_snapshot_length() {
    for (search, expected) in [
        ("findIndex((v,i)=>i===1023)", "1023"),
        ("findLastIndex((v,i)=>i===0)", "0"),
        ("findLastIndex(()=>true)", "1023"),
    ] {
        run(
            &format!(
                "export function main():void{{const values:number[]=[];values.length=1024;console.log(values.{search});}}"
            ),
            &[expected],
        );
    }
}

#[test]
fn predicate_search_ignored_tails_do_not_become_wasm_parameters() {
    let source = format!(
        "export function main():void{{console.log([1].findIndex(()=>true,undefined{}));}}",
        ",0".repeat(varta_common::abi::MAX_WASM_PARAMETERS + 8)
    );
    run(&source, &["0"]);
}
#[test]
fn predicate_search_earlier_profiles_keep_their_source_boundary() {
    for method in ["findIndex", "findLast", "findLastIndex"] {
        let source = format!("export function main():void{{console.log([1].{method}(()=>true));}}");
        for profile in [
            LanguageProfile::CheckedF64Utf16V12,
            LanguageProfile::CheckedF64Utf16V24,
        ] {
            assert!(varta_compiler::compile_typescript_with_profile(&source, profile).is_err());
        }
        compile(&source);
    }
}
#[test]
fn predicate_search_requires_checked_callback_and_undefined_capable_value_argument() {
    for expression in [
        "[1].findIndex()",
        "[1].findLast(1)",
        "[1].findLastIndex((v:number):boolean=>true)",
        "'abc'.findIndex(()=>true)",
    ] {
        let source = format!("export function main():void{{console.log({expression});}}");
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &source,
                LanguageProfile::CheckedF64Utf16V25
            )
            .is_err(),
            "{expression}"
        );
    }
}
fn export_fixture(bytes: &[u8], offset: u8) -> Vec<u8> {
    use crate::contract_tests::{export_index, rewrite};
    let index = export_index(bytes, "_start") - offset;
    rewrite(bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(data[0] < 127 && index < 128);
            data[0] += 1;
            data.push(7);
            data.extend(b"fixture");
            data.extend([0, index]);
        }
        vec![data]
    })
}
#[test]
fn predicate_search_value_bits_and_index_zero_survive_guest_only_paths() {
    use crate::execution::Execution;
    let runtime = VartaRuntime::new();
    let bytes = export_fixture(
        &compile(
            "function fixture(n:number):number{const values=[n];return values.findLast(()=>true)??0;}export function main():void{}",
        ),
        1,
    );
    assert!(runtime.inspect_module(&bytes).unwrap().imports.is_empty());
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    execution.store.data_mut().host_calls = abi::MAX_HOST_CALLS;
    let function = instance
        .get_typed_func::<f64, f64>(&mut execution.store, "fixture")
        .unwrap();
    for bits in [
        0,
        1,
        0x8000_0000_0000_0000,
        0x8000_0000_0000_0001,
        0x7fef_ffff_ffff_ffff,
        0xffef_ffff_ffff_ffff,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
        0x7ff8_1234_5678_9abc,
    ] {
        let n = f64::from_bits(bits);
        let actual = function.call(&mut execution.store, n).unwrap();
        assert_eq!(actual.to_bits(), n.to_bits());
    }
    assert_eq!(execution.store.data().host_calls, abi::MAX_HOST_CALLS);
    let bytes = export_fixture(
        &compile(
            "function fixture(n:number):number{const values=[1];if(n){return values.findIndex(()=>false);}return values.findLastIndex(()=>true);}export function main():void{}",
        ),
        1,
    );
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    let function = instance
        .get_typed_func::<f64, f64>(&mut execution.store, "fixture")
        .unwrap();
    assert_eq!(
        function.call(&mut execution.store, 0.0).unwrap().to_bits(),
        0_f64.to_bits()
    );
    assert_eq!(
        function.call(&mut execution.store, 1.0).unwrap().to_bits(),
        (-1_f64).to_bits()
    );
}
#[test]
fn predicate_search_empty_arrays_still_reject_corrupt_callback_handles() {
    use crate::{
        contract_tests::{export_index, rewrite},
        execution::Execution,
    };
    for method in ["findIndex", "findLast", "findLastIndex"] {
        let source = format!(
            "function make():(v:number|undefined)=>boolean{{return(v:number|undefined):boolean=>{{console.log('BAD');return true;}};}}function fixture(fn:(v:number|undefined)=>boolean):number{{const values:number[]=[];return values.{method}(fn)??-1;}}export function main():void{{}}"
        );
        let bytes = compile(&source);
        let main = export_index(&bytes, "_start");
        let bytes = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 7 {
                assert!(data[0] < 126 && main < 128);
                data[0] += 2;
                for (name, index) in [("make", main - 2), ("fixture", main - 1)] {
                    data.push(name.len() as u8);
                    data.extend(name.as_bytes());
                    data.extend([0, index]);
                }
            }
            vec![data]
        });
        let runtime = VartaRuntime::new();
        let module = runtime.validate_module(&bytes).unwrap();
        for corrupt in [false, true] {
            let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
            let instance = execution.instantiate(&module).unwrap();
            let handle = instance
                .get_typed_func::<(), i64>(&mut execution.store, "make")
                .unwrap()
                .call(&mut execution.store, ())
                .unwrap();
            let callback = if corrupt { 0 } else { handle };
            let result = instance
                .get_typed_func::<i64, f64>(&mut execution.store, "fixture")
                .unwrap()
                .call(&mut execution.store, callback);
            assert_eq!(result.is_ok(), !corrupt, "{method}");
            if let Ok(value) = result {
                assert_eq!(value, -1.0);
            }
            assert!(execution.store.data().output.lines.is_empty());
        }
    }
}
#[test]
fn predicate_search_cached_graphs_survive_collection_and_ignored_argument_effects() {
    use crate::execution::Execution;
    let source = "function churn():void{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}}export function main():void{const values=[{text:'kept'+'!'}];const found=values.findLast((v,i,a)=>{a.pop();churn();return true;},undefined,churn());churn();if(found){console.log(found.text);}const input=['old'+'!'];console.log(input.findLastIndex((v,i,a)=>{a.length=0;churn();return true;}));console.log(input.length);}";
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(1000 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    crate::exceptions::finish(&instance, &mut execution.store).unwrap();
    assert_eq!(
        execution.store.data().output.lines,
        vec!["varta log: kept!", "varta log: 0", "varta log: 0"]
    );
}
#[test]
fn predicate_search_ignored_pairs_and_callbacks_preserve_denial_status() {
    run(
        "export function main():void{const values=[1,2];console.log(values.findIndex(()=>true,undefined,varta.result.capture(varta.env.get('SEARCH_EXTRA'))));console.log(varta.result.status());console.log(values.findLastIndex(v=>{varta.env.get('SEARCH_CALLBACK');return true;}));console.log(varta.result.status());}",
        &[
            "<capability denied: env> (SEARCH_EXTRA)",
            "0",
            "1",
            "<capability denied: env> (SEARCH_CALLBACK)",
            "1",
            "1",
        ],
    );
}
#[test]
fn predicate_search_fatal_fuel_bypasses_guest_catches_and_finalizers() {
    use crate::execution::Execution;
    let source = "export function main():void{console.log('before');try{[1].findLastIndex((v:number|undefined):boolean=>{console.log('callback');while(true){}return true;});}catch(e){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}";
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    let error = instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap_err();
    assert!(format!("{error:#}").contains("fuel"), "{error:#}");
    assert_eq!(
        execution.store.data().output.lines,
        vec!["varta log: before", "varta log: callback"]
    );
}
#[test]
fn predicate_search_wrappers_do_not_hide_host_effects_from_managed_helpers() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function fake(value:boolean):boolean{[value].findLastIndex(item=>{console.log(item);return true;});return value;}export function main():void{varta.json.parse<number>('1');}",
    );
    let fake = export_index(&bytes, "_start") - 1;
    let name = varta_common::value::VALUE_ALLOCATE;
    let bad = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            let at = data
                .windows(name.len())
                .position(|s| s == name.as_bytes())
                .unwrap()
                + name.len()
                + 1;
            assert!(fake < 128 && data[at] < 128);
            data[at] = fake;
        }
        vec![data]
    });
    let error = VartaRuntime::new().validate_module(&bad).unwrap_err();
    assert!(
        error.to_string().contains("cannot call host imports"),
        "{error}"
    );
}
