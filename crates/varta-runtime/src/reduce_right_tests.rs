//! Reverse folds retain live sparse presence and owned accumulator graphs.
use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V26)
        .unwrap_or_else(|error| panic!("{error}\n{source}"))
}
fn run(source: &str, expected: &[&str]) {
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap_or_else(|error| panic!("{error}\n{source}"))
            .lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
}
#[test]
fn reduce_right_dense_direction_and_accumulator_order() {
    run(
        "export function main():void{const a=[1,2,3];console.log(a.reduceRight((s,v)=>s-v));console.log(a.reduceRight((s,v)=>s-v,10));console.log(a.reduce((s,v)=>s-v));console.log(['a','b','c'].reduceRight((s,v)=>s+v));}",
        &["0", "4", "-4", "cba"],
    );
}
#[test]
fn reduce_right_callback_arguments_are_accumulator_value_index_original_array() {
    run(
        "export function main():void{const values=[1,2,3];console.log(values.reduceRight((s:string,v:number,i:number,a:number[]):string=>{console.log(i);console.log(a===values);return s+String(v);},''));}",
        &["2", "1", "1", "1", "0", "1", "321"],
    );
}
#[test]
fn reduce_right_implicit_seed_is_rightmost_present_and_not_a_callback_input() {
    run(
        "export function main():void{const a:number[]=[];a.length=6;a[1]=2;a[4]=5;console.log(a.reduceRight((s,v,i)=>{console.log(i);console.log(s);return s+v;}));console.log(Object.hasOwn(a,5));console.log(Object.hasOwn(a,4));}",
        &["1", "5", "7", "0", "1"],
    );
}
#[test]
fn reduce_right_empty_and_all_holes_throw_only_without_initial_value() {
    run(
        "export function main():void{const empty:number[]=[];try{empty.reduceRight((s,v)=>s+v);}catch(error){console.log(error instanceof TypeError);}const holes:number[]=[];holes.length=4;try{holes.reduceRight((s,v)=>s+v);}catch(error){console.log(error instanceof TypeError);}console.log(empty.reduceRight((s,v)=>{throw 'BAD';return s+v;},7));console.log(holes.reduceRight((s,v)=>{throw 'BAD';return s+v;},9));}",
        &["1", "1", "7", "9"],
    );
}
#[test]
fn reduce_right_explicit_undefined_is_an_initial_value() {
    run(
        "export function main():void{const empty:number[]=[];console.log(empty.reduceRight((s:undefined,v:number):undefined=>{throw 'BAD';return undefined;},undefined));console.log([1].reduceRight((s:undefined,v:number):undefined=>{console.log(s);console.log(v);return undefined;},undefined));}",
        &["undefined", "undefined", "1", "undefined"],
    );
}
#[test]
fn reduce_right_present_undefined_is_visited_while_holes_are_skipped() {
    run(
        "export function main():void{const a:(number|undefined)[]=[undefined,,2];console.log(a.reduceRight((s:number|undefined,v:number|undefined):number|undefined=>{console.log(v);return s;}));console.log(a.reduceRight((s:number|undefined,v:number|undefined):number|undefined=>{console.log(v);return s;},2));}",
        &["undefined", "2", "2", "undefined", "2"],
    );
}
#[test]
fn reduce_right_single_present_element_never_calls_the_callback_without_seed() {
    run(
        "export function main():void{const a:number[]=[];a.length=4;a[1]=8;console.log(a.reduceRight((s,v)=>{throw 'BAD';return s+v;}));const ref={n:9};console.log([ref].reduceRight((s,v)=>{throw 'BAD';return s;})===ref);}",
        &["8", "1"],
    );
}
#[test]
fn reduce_right_skips_deleted_positions_and_observes_replacements() {
    run(
        "export function main():void{const a=[1,2,3,4];console.log(a.reduceRight((s,v,i,source)=>{console.log(i);if(i===3){delete source[2];source[1]=9;source.push(99);}return s+v;},0));console.log(a.length);}",
        &["3", "1", "0", "14", "5"],
    );
}
#[test]
fn reduce_right_truncation_skips_positions_and_regrowth_is_live() {
    run(
        "export function main():void{const a=[1,2,3,4];console.log(a.reduceRight((s,v,i,source)=>{console.log(i);if(i===3)source.length=1;return s+v;},0));const b=[1,2,3,4];console.log(b.reduceRight((s,v,i,source)=>{if(i===3){source.length=1;source.push(8,9,10,99);}return s+v;},0));}",
        &["3", "0", "5", "22"],
    );
}
#[test]
fn reduce_right_evaluates_source_callback_seed_and_ignored_tails_before_snapshot() {
    run(
        "export function main():void{let values=[1];const source=():number[]=>{console.log('source');return values;};const callback=():((s:number,v:number)=>number)=>{console.log('callback');values.push(2);return(s:number,v:number):number=>s+v;};const seed=():number=>{console.log('seed');values.push(3);return 0;};const extra=():void=>{console.log('extra');values.push(4);};console.log(source().reduceRight(callback(),seed(),extra()));}",
        &["source", "callback", "seed", "extra", "10"],
    );
}
#[test]
fn reduce_right_caches_effective_operands_before_ignored_reassignments() {
    run(
        "export function main():void{let values=[1,2];const original=values;let fn=(s:number,v:number):number=>s+v;let seed={n:0};const result=values.reduceRight(fn,0,values=[99],fn=(s:number,v:number):number=>999);console.log(result);console.log(original.length);console.log(values[0]);const output=original.reduceRight((s:{n:number},v:number):{n:number}=>{s.n+=v;return s;},seed,seed={n:99});console.log(output.n);console.log(seed.n);}",
        &["3", "2", "99", "3", "99"],
    );
}
#[test]
fn reduce_right_infers_array_record_and_scalar_seeds_from_context() {
    run(
        "export function main():void{const a=[1,2,3];const out=a.reduceRight((s:number[],v:number):number[]=>{s.push(v);return s;},[]);console.log(JSON.stringify(out));const seed={n:0};const record=a.reduceRight((s:{n:number},v:number):{n:number}=>{s.n+=v;return s;},seed);console.log(record===seed);console.log(record.n);console.log(a.reduceRight((s,v)=>s+String(v),''));}",
        &["[3,2,1]", "1", "6", "321"],
    );
}
#[test]
fn reduce_right_callback_arity_defaults_and_lexical_receivers() {
    run(
        "export function main():void{console.log([1,2].reduceRight(()=>7,0));console.log([1,2].reduceRight((s:number,v:number,i:number,a:number[],extra:number=9):number=>s+v+extra,0));const factory=function(this:{n:number}):(s:number,v:number)=>number{return(s:number,v:number):number=>s+v+this.n;};console.log([1,2].reduceRight(factory.call({n:3}),0));const fn=function(this:undefined,s:number,v:number):number{console.log(this);return s+v;};console.log([1].reduceRight(fn,0));}",
        &["7", "21", "9", "undefined", "1"],
    );
}
#[test]
fn reduce_right_exceptions_and_finalizers_preserve_accumulator_and_cleanup() {
    run(
        "export function main():void{const a=[1,2,3];try{a.reduceRight((s,v,i)=>{try{throw 'stop';}finally{console.log(i);}},0);}catch(error){console.log(error);}console.log(a.reduceRight((s,v)=>{try{return s+v;}finally{return 7;}},0));console.log(a.reduceRight((s,v)=>s+v));}",
        &["2", "stop", "7", "6"],
    );
}
#[test]
fn reduce_right_ignored_throw_prevents_invocation_and_restores_outer_roots() {
    run(
        "function extra():number{throw 'stop';return 0;}export function main():void{const values=[1,2];try{values.reduceRight((s,v)=>{console.log('BAD');return s+v;},0,extra());}catch(error){console.log(error);}finally{console.log('finally');}console.log(values.reduceRight((s,v)=>s+v));}",
        &["stop", "finally", "3"],
    );
}
#[test]
fn reduce_right_maximum_dense_and_sparse_ranges_fit_default_budgets() {
    let input = format!("[{}]", vec!["1"; 1024].join(","));
    for seeded in [false, true] {
        let seed = if seeded { ",0" } else { "" };
        run(
            &format!(
                "export function main():void{{const a=varta.json.parse<number[]>('{input}');console.log(a.reduceRight((s,v)=>s+v{seed}));}}"
            ),
            &["1024"],
        );
    }
    run(
        "export function main():void{const a:number[]=[];a.length=1024;console.log(a.reduceRight((s,v)=>{throw 'BAD';return s+v;},7));try{a.reduceRight((s,v)=>s+v);}catch(error){console.log(error instanceof TypeError);}}",
        &["7", "1"],
    );
}
#[test]
fn reduce_right_rejects_invalid_callbacks_receivers_and_preserves_earlier_profiles() {
    for body in [
        "a.reduceRight()",
        "a.reduceRight(1)",
        "a.reduceRight((s:string,v:number):string=>s)",
        "a.reduceRight((s:number,v:number):string=>'bad',0)",
        "a.reduceRight(function(this:{n:number},s:number,v:number):number{return s+v;},0)",
        "a.reduceRight((s:number,v:number,i:number,a:number[],required:number):number=>s+v,0)",
    ] {
        let source = format!("export function main():void{{const a=[1,2];{body};}}");
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &source,
                LanguageProfile::CheckedF64Utf16V26
            )
            .is_err(),
            "{source}"
        );
    }
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V25,
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{console.log([1,2].reduceRight((s,v)=>s+v));}",
                profile
            )
            .is_err()
        );
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{console.log([1,2].reduce((s,v)=>s+v));}",
                profile
            )
            .is_ok()
        );
    }
}
#[test]
fn reduce_right_ignored_pairs_and_callbacks_preserve_permission_denials_and_status() {
    run(
        "export function main():void{console.log([1].reduceRight((s,v)=>{varta.env.get('FOLD_CALLBACK');return s+v;},0,varta.result.capture(varta.env.get('FOLD_EXTRA'))));console.log(varta.result.status());}",
        &[
            "<capability denied: env> (FOLD_EXTRA)",
            "<capability denied: env> (FOLD_CALLBACK)",
            "1",
            "1",
        ],
    );
}
#[test]
fn reduce_right_fatal_fuel_bypasses_guest_catch_and_finally() {
    let bytes = compile(
        "export function main():void{console.log('before');try{[1].reduceRight((s,v)=>{console.log('callback');while(true){}return s+v;},0);}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
    );
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    assert!(
        instance
            .get_typed_func::<(), ()>(&mut execution.store, "_start")
            .unwrap()
            .call(&mut execution.store, ())
            .is_err()
    );
    assert_eq!(
        execution.store.data().output.lines,
        ["varta log: before", "varta log: callback"]
    );
}
#[test]
fn reduce_right_cached_accumulator_and_current_value_survive_collection() {
    let source = "function churn():void{let text='x';for(let i=0;i<14;i++)text=text+text;for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}}export function main():void{const values=[{text:'one'+'!'},{text:'two'+'!'}];const seed={text:'seed'+'!'};const found=values.reduceRight((s:{text:string},v:{text:string},i:number,a:{text:string}[]):{text:string}=>{a.length=0;churn();return {text:s.text+v.text};},seed,churn());churn();console.log(found.text);console.log(seed.text);console.log(values.length);}";
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
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
        ["varta log: seed!two!", "varta log: seed!", "varta log: 0"]
    );
}

fn export_fixture(bytes: &[u8]) -> Vec<u8> {
    use crate::contract_tests::{export_index, rewrite};
    let index = export_index(bytes, "_start") - 1;
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
fn reduce_right_empty_seed_and_single_element_preserve_all_number_bits_without_host_calls() {
    for body in [
        "const a:number[]=[];return a.reduceRight((s,v)=>s+v,n);",
        "const a=[n];return a.reduceRight((s,v)=>s+v);",
    ] {
        let bytes = export_fixture(&compile(&format!(
            "function fixture(n:number):number{{{body}}}export function main():void{{}}"
        )));
        let runtime = VartaRuntime::new();
        assert!(runtime.inspect_module(&bytes).unwrap().imports.is_empty());
        let module = runtime.validate_module(&bytes).unwrap();
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
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
            assert_eq!(
                function
                    .call(&mut execution.store, f64::from_bits(bits))
                    .unwrap()
                    .to_bits(),
                bits
            );
        }
        assert_eq!(execution.store.data().host_calls, abi::MAX_HOST_CALLS);
    }
}
#[test]
fn reduce_right_empty_arrays_validate_function_handles_even_without_invocation() {
    use crate::contract_tests::{export_index, rewrite};
    for seed in ["", ",7"] {
        let bytes = compile(&format!(
            "function make():(s:number,v:number)=>number{{return(s:number,v:number):number=>{{console.log('BAD');return s+v;}};}}function fixture(fn:(s:number,v:number)=>number):number{{const a:number[]=[];try{{return a.reduceRight(fn{seed});}}catch(error){{return -1;}}}}export function main():void{{}}"
        ));
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
            let mut execution =
                crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
            let instance = execution.instantiate(&module).unwrap();
            let handle = instance
                .get_typed_func::<(), i64>(&mut execution.store, "make")
                .unwrap()
                .call(&mut execution.store, ())
                .unwrap();
            let value = instance
                .get_typed_func::<i64, f64>(&mut execution.store, "fixture")
                .unwrap()
                .call(&mut execution.store, if corrupt { 0 } else { handle });
            assert_eq!(value.is_ok(), !corrupt);
            if let Ok(value) = value {
                assert_eq!(value, if seed.is_empty() { -1.0 } else { 7.0 });
            }
            assert!(execution.store.data().output.lines.is_empty());
        }
    }
}
#[test]
fn reduce_right_wrappers_cannot_hide_callback_host_effects_in_managed_helpers() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function fake(value:boolean):boolean{[value].reduceRight((s,v)=>{console.log(v);return s;},value);return value;}export function main():void{varta.json.parse<number>('1');}",
    );
    let fake = export_index(&bytes, "_start") - 1;
    let name = varta_common::value::VALUE_ALLOCATE;
    let bytes = rewrite(&bytes, |id, data| {
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
    let error = VartaRuntime::new().validate_module(&bytes).unwrap_err();
    assert!(
        error.to_string().contains("cannot call host imports"),
        "{error}"
    );
}
#[test]
fn reduce_right_ignored_tail_is_not_limited_by_physical_callback_parameter_count() {
    let extra = "0,".repeat(abi::MAX_WASM_PARAMETERS + 8);
    run(
        &format!(
            "export function main():void{{console.log([1,2].reduceRight((s,v)=>s+v,0,{extra}));}}"
        ),
        &["3"],
    );
}
#[test]
fn reduce_right_opaque_callback_writes_cannot_preserve_stale_refinements() {
    let source = "export function main():void{let value:{n:number}|undefined={n:7};if(value){[1].reduceRight((s,v)=>{value=undefined;return s+v;},0);console.log(value.n);}}";
    assert!(
        varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V26
        )
        .is_err()
    );
}
