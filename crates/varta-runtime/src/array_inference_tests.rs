//! Uncontextual array elements join without duplicating source effects or aliases.
use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V27)
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
fn array_inference_joins_primitives_in_both_orders_and_preserves_json_undefined() {
    run(
        "export function main():void{console.log(JSON.stringify([1,undefined]));console.log(JSON.stringify([undefined,1]));console.log(JSON.stringify([true,'x',null,2,undefined]));console.log(JSON.stringify([undefined,2,null,'x',true]));}",
        &[
            "[1,null]",
            "[null,1]",
            "[true,\"x\",null,2,null]",
            "[null,2,null,\"x\",true]",
        ],
    );
}
#[test]
fn array_inference_retains_sparse_presence_and_does_not_add_holes_to_the_element_type() {
    run(
        "export function main():void{const values=[,1,undefined,,];console.log(values.length);console.log(Object.hasOwn(values,0));console.log(Object.hasOwn(values,2));console.log(values.indexOf(undefined));console.log(JSON.stringify(values));const numbers=[,1];numbers.push(2);console.log(JSON.stringify(numbers));}",
        &["4", "0", "1", "2", "[null,1,null,null]", "[null,1,2]"],
    );
    assert!(
        varta_compiler::compile_typescript_with_profile(
            "export function main():void{const values=[,1];values.push(undefined);}",
            LanguageProfile::CheckedF64Utf16V27
        )
        .is_err()
    );
}
#[test]
fn array_inference_invariant_mutations_accept_only_original_members() {
    run(
        "export function main():void{const values=[1,'x'];values.push(2,'y');values[0]='changed';console.log(JSON.stringify(values));console.log(values.pop());console.log(JSON.stringify(values.slice().reverse()));}",
        &["[\"changed\",\"x\",2,\"y\"]", "y", "[2,\"x\",\"changed\"]"],
    );
    for body in [
        "const a=[1,'x'];a.push(true);",
        "const a=[1,undefined];a[0]='x';",
        "let a=[1,'x'];const b=[1];a=b;",
        "const a=[1,'x'];a.fill(false);",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &format!("export function main():void{{{body}}}"),
                LanguageProfile::CheckedF64Utf16V27
            )
            .is_err(),
            "{body}"
        );
    }
}
#[test]
fn array_inference_flattens_existing_unions_and_propagates_reads_and_guards() {
    run(
        "function choose(flag:boolean):number|string{return flag?1:'x';}export function main():void{const values=[choose(true),undefined,choose(false),true];for(const value of values){if(typeof value==='number'){console.log(value+1);}else if(typeof value==='string'){console.log(value.length);}else{console.log(value);}}}",
        &["2", "undefined", "1", "1"],
    );
}
#[test]
fn array_inference_joins_record_array_error_and_primitive_members_with_identity() {
    run(
        "export function main():void{const record={n:7};const child=[8];const error=TypeError('bad');const values=[record,child,error,1,undefined,'x',true,null];console.log(values[0]===record);console.log(values[1]===child);console.log(values[2]===error);const first=values[0];if(first&&typeof first==='object'&&!Array.isArray(first)&&!(first instanceof Error)){console.log(first.n);}const second=values[1];if(Array.isArray(second)){second.push(9);}console.log(child.length);}",
        &["1", "1", "1", "7", "2"],
    );
}
#[test]
fn array_inference_keeps_distinct_error_families_and_owned_messages() {
    run(
        "export function main():void{const values=[Error('base'),TypeError('type'),RangeError('range'),undefined];for(const value of values){if(value instanceof Error){console.log(value.message);}if(value instanceof TypeError){console.log(value.name);}}}",
        &["base", "type", "TypeError", "range"],
    );
}
#[test]
fn array_inference_empty_and_all_hole_peers_use_later_array_evidence() {
    run(
        "export function main():void{const values=[[],[1],([,,])];console.log(JSON.stringify(values));const first=values[0];if(first){first.push(2);}console.log(JSON.stringify(values));const mixed=[[],undefined,[1]];console.log(JSON.stringify(mixed));}",
        &[
            "[[],[1],[null,null]]",
            "[[2],[1],[null,null]]",
            "[[],null,[1]]",
        ],
    );
}
#[test]
fn array_inference_keeps_earlier_homogeneous_aggregate_context() {
    let source = "export function main():void{const values=[[1],[]];console.log(JSON.stringify(values));const record:{n:number|undefined}={n:1};console.log(JSON.stringify([record,{n:2}]));const optional:{n?:number}={n:7};console.log(JSON.stringify([optional,{}]));}";
    run(
        source,
        &["[[1],[]]", "[{\"n\":1},{\"n\":2}]", "[{\"n\":7},{}]"],
    );
    assert!(
        varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V26
        )
        .is_ok()
    );
}
#[test]
fn array_inference_explicit_context_still_controls_members_and_empty_values() {
    run(
        "function keep(values:(number|string)[]):(number|string)[]{return values;}export function main():void{const empty:(number|undefined)[]=[];empty.push(1,undefined);console.log(JSON.stringify(empty));console.log(JSON.stringify(keep([1,'x'])));const values:(number|undefined)[][]=[[],[1,undefined]];console.log(JSON.stringify(values));}",
        &["[1,null]", "[1,\"x\"]", "[[],[1,null]]"],
    );
    for body in [
        "const a:number[]=[1,undefined];",
        "const a:(number|string)[]=[1,true];",
        "const a:(number|undefined)[][]=[[true]];",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &format!("export function main():void{{{body}}}"),
                LanguageProfile::CheckedF64Utf16V27
            )
            .is_err()
        );
    }
}
#[test]
fn array_inference_deferred_empty_peers_do_not_reorder_or_duplicate_effectful_expressions() {
    run(
        "export function main():void{let calls=0;const make=():number[]=>{calls+=1;console.log(calls);return [calls];};const values=[[],make(),[],make()];console.log(calls);console.log(JSON.stringify(values));let value:{n:number}|undefined={n:7};if(value){const items=[value,value=undefined];console.log(JSON.stringify(items));console.log(Boolean(value));}}",
        &["1", "2", "2", "[[],[1],[],[2]]", "[{\"n\":7},null]", "0"],
    );
}
#[test]
fn array_inference_element_factories_execute_left_to_right_once() {
    run(
        "export function main():void{let n=0;const number=():number=>{n+=1;console.log(n);return n;};const text=():string=>{n+=1;console.log(n);return 'x';};console.log(JSON.stringify([number(),text(),number(),undefined]));console.log(n);}",
        &["1", "2", "3", "[1,\"x\",3,null]", "3"],
    );
}
#[test]
fn array_inference_throwing_element_skips_later_work_and_preserves_cleanup() {
    run(
        "function stop():string{throw 'stop';return '';}export function main():void{let n=0;try{const values=[1,stop(),n+=1];console.log('BAD');}catch(error){console.log(error);}finally{console.log(n);}console.log(JSON.stringify([undefined,2]));}",
        &["stop", "0", "[null,2]"],
    );
}
#[test]
fn array_inference_void_calls_contribute_undefined_after_their_effects() {
    run(
        "function effect():void{console.log('effect');}export function main():void{console.log(JSON.stringify([1,effect(),'x']));}",
        &["effect", "[1,null,\"x\"]"],
    );
}
#[test]
fn array_inference_callbacks_destructuring_and_spread_calls_keep_union_types() {
    run(
        "function first(...values:(number|string)[]):number|string|undefined{return values[0];}export function main():void{const values=[1,'x',undefined];console.log(JSON.stringify(values.map(value=>typeof value)));console.log(values.findIndex(value=>value===undefined));const [n,s,...tail]=values;console.log(n);console.log(s);console.log(JSON.stringify(tail));console.log(first(...[1,'x']));}",
        &[
            "[\"number\",\"string\",\"undefined\"]",
            "2",
            "1",
            "x",
            "[null]",
            "1",
        ],
    );
}
#[test]
fn array_inference_finalizer_saved_arrays_retain_identity_and_values() {
    run(
        "function make():(number|string)[]{const record=[1,'x'];try{return record;}finally{record[0]='changed';}}export function main():void{console.log(JSON.stringify(make()));}",
        &["[\"changed\",\"x\"]"],
    );
}
#[test]
fn array_inference_rejects_unrepresented_shapes_functions_pairs_and_untyped_empties() {
    for expression in [
        "[{n:1},{n:undefined}]",
        "[[1],['x']]",
        "[()=>1,undefined]",
        "[1,varta.result.capture(varta.env.get('PAIR'))]",
        "[]",
        "[,,]",
        "[[],undefined]",
        "[...[1,2]]",
    ] {
        let source = format!("export function main():void{{const values={expression};}}");
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &source,
                LanguageProfile::CheckedF64Utf16V27
            )
            .is_err(),
            "{expression}"
        );
    }
}
#[test]
fn array_inference_earlier_profiles_keep_homogeneous_literal_rules() {
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V26,
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{console.log(JSON.stringify([1,undefined]));}",
                profile
            )
            .is_err()
        );
        assert!(varta_compiler::compile_typescript_with_profile("export function main():void{const a:(number|undefined)[]=[1,undefined];console.log(JSON.stringify(a));}",profile).is_ok());
    }
}
#[test]
fn array_inference_preserves_array_and_function_compilation_budgets() {
    let mut sparse = vec![""; varta_common::value::MAX_ARRAY_ELEMENTS];
    sparse[0] = "1";
    sparse[1023] = "undefined";
    run(
        &format!(
            "export function main():void{{const values=[{}];console.log(values.length);console.log(values[0]);console.log(values[1023]);console.log(Object.hasOwn(values,512));}}",
            sparse.join(",")
        ),
        &["1024", "1", "undefined", "0"],
    );
    let dense = (0..128)
        .map(|i| if i % 2 == 0 { "1" } else { "undefined" })
        .collect::<Vec<_>>()
        .join(",");
    run(
        &format!(
            "export function main():void{{const values=[{dense}];console.log(values.length);}}"
        ),
        &["128"],
    );
    let wide = (0..1024)
        .map(|i| if i % 2 == 0 { "1" } else { "undefined" })
        .collect::<Vec<_>>()
        .join(",");
    for (profile, annotation) in [
        (LanguageProfile::CheckedF64Utf16V26, ":(number|undefined)[]"),
        (LanguageProfile::CheckedF64Utf16V27, ""),
    ] {
        let source = format!("export function main():void{{const values{annotation}=[{wide}];}}");
        let error = varta_compiler::compile_typescript_with_profile(&source, profile).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("function body exceeds compilation budget"),
            "{error}"
        );
    }
    let too_many = format!("export function main():void{{const values=[{wide},1];}}");
    let error = varta_compiler::compile_typescript_with_profile(
        &too_many,
        LanguageProfile::CheckedF64Utf16V27,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("array element count exceeds limit"),
        "{error}"
    );
}

#[test]
fn array_inference_local_cell_effects_are_not_lost_during_type_collection() {
    let source = "export function main():void{let value:{n:number}|undefined={n:7};const clear=():string=>{value=undefined;return 'cleared';};if(value){const values=[value,clear()];console.log(value.n);}}";
    assert!(
        varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V27
        )
        .is_err()
    );
}
#[test]
fn array_inference_pure_boxing_preserves_permission_denials_and_system_status() {
    run(
        "export function main():void{const values=[1,varta.env.get('INFERRED_DENIED'),undefined];console.log(varta.result.status());console.log(JSON.stringify(values));console.log(varta.result.status());}",
        &[
            "<capability denied: env> (INFERRED_DENIED)",
            "1",
            "[1,\"\",null]",
            "1",
        ],
    );
}
#[test]
fn array_inference_earlier_union_values_and_children_survive_later_collection() {
    let source = "function churn():string{let text='x';for(let i=0;i<14;i++)text=text+text;for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}return 'later';}export function main():void{let value:{text:string}|undefined={text:'kept'+'!'};const clear=():string=>{value=undefined;return churn();};if(value){const values=[value,clear(),undefined];churn();const first=values[0];if(first&&typeof first==='object'){console.log(first.text);}console.log(JSON.stringify(values));}}";
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
        [
            "varta log: kept!",
            "varta log: [{\"text\":\"kept!\"},\"later\",null]"
        ]
    );
}

#[test]
fn array_inference_boxed_number_payloads_keep_exact_bits_without_host_calls() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function fixture(n:number):number{const values=[n,undefined];return values[0]??0;}export function main():void{}",
    );
    let fixture = export_index(&bytes, "_start") - 1;
    let bytes = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(data[0] < 127 && fixture < 128);
            data[0] += 1;
            data.push(7);
            data.extend(b"fixture");
            data.extend([0, fixture]);
        }
        vec![data]
    });
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
#[test]
fn array_inference_cannot_hide_initializer_hosts_in_managed_helpers() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function fake(value:boolean):boolean{const values=[value,console.log('BAD')];return value;}export function main():void{varta.json.parse<number>('1');}",
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
#[test]
fn array_inference_fatal_initializer_skips_later_elements_and_guest_handlers() {
    let bytes = compile(
        "function spin():string{console.log('middle');while(true){}return '';}export function main():void{console.log('before');try{const values=[1,spin(),console.log('BAD_LATE')];}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
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
        ["varta log: before", "varta log: middle"]
    );
}

#[test]
fn array_inference_retains_context_for_calls_conditionals_and_pattern_results() {
    let source = "export function main():void{const peer=[9];const data=[1,2];const values=[peer,data.reduce((out,value)=>{out.push(value);return out;},[])];console.log(JSON.stringify(values));const record:{items:number[]}={items:[7]};const records=[record,true?{items:[]}:{items:[2]}];console.log(JSON.stringify(records));let n:number|undefined=1;const assignments=[peer,([n]=[])];console.log(n);console.log(JSON.stringify(assignments));}";
    let legacy = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V26,
    )
    .unwrap();
    let expected = [
        "varta log: [[9],[1,2]]",
        "varta log: [{\"items\":[7]},{\"items\":[]}]",
        "varta log: undefined",
        "varta log: [[9],[]]",
    ];
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&legacy, &[])
            .unwrap()
            .lines,
        expected
    );
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        expected
    );
}

#[test]
fn array_inference_aggregate_hints_do_not_constrain_inferred_conditional_members() {
    let prefix =
        "function choose(flag:boolean):{items:number[]}|number[]{return flag?{items:[7]}:[9];}";
    let shared = "const first=choose(true);const data=[1,2];console.log(JSON.stringify([first,data.reduce((out,value)=>{out.push(value);return out;},[])]));console.log(JSON.stringify([first,true?{items:[]}:[]]));";
    let source = format!("{prefix}export function main():void{{{shared}}}");
    let legacy = varta_compiler::compile_typescript_with_profile(
        &source,
        LanguageProfile::CheckedF64Utf16V26,
    )
    .unwrap();
    let expected = [
        "varta log: [{\"items\":[7]},[1,2]]",
        "varta log: [{\"items\":[7]},{\"items\":[]}]",
    ];
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&legacy, &[])
            .unwrap()
            .lines,
        expected
    );
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(&source), &[])
            .unwrap()
            .lines,
        expected
    );
    run(
        &format!(
            "{prefix}export function main():void{{const first=choose(true);const values=[first,true?1:'text'];values.push('later');console.log(JSON.stringify(values));}}"
        ),
        &["[{\"items\":[7]},1,\"later\"]"],
    );
}
