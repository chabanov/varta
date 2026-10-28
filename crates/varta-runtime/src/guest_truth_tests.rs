use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V23)
        .unwrap()
}
fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| format!("varta log: {s}")).collect()
}
#[test]
fn guest_truth_primitive_conditions_ternaries_and_negation_share_boolean_rules() {
    let source = r"function show(value:number|string|boolean|null|undefined):void{console.log(JSON.stringify([!value,!!value,value?true:false]));}export function main():void{show(0);show(-0);show(NaN);show(Infinity);show(-Infinity);show('');show('0');show('\u0000');show('\uD800');show(false);show(true);show(null);show(undefined);}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "[true,false,false]",
            "[true,false,false]",
            "[true,false,false]",
            "[false,true,true]",
            "[false,true,true]",
            "[true,false,false]",
            "[false,true,true]",
            "[false,true,true]",
            "[false,true,true]",
            "[true,false,false]",
            "[false,true,true]",
            "[true,false,false]",
            "[true,false,false]"
        ])
    );
}
#[test]
fn guest_truth_references_are_true_in_all_control_flow_positions() {
    let source = "function show(value:string|null|undefined|{n:number}|number[]|Error):void{if(value){console.log('yes');}else{console.log('no');}console.log(!value);}export function main():void{let count=0;while({n:0}){count++;break;}do{count++;}while('');for(;[1];){count++;break;}console.log(count);console.log((():number=>0)?'function':'BAD');console.log(!({n:0}));console.log(![1]);console.log(!Error(''));show({n:0});show([]);show(Error(''));show('');show(null);show(undefined);}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "3", "function", "0", "0", "0", "yes", "0", "yes", "0", "yes", "0", "no", "1", "no",
            "1", "no", "1"
        ])
    );
}
#[test]
fn guest_truth_logical_operators_return_original_values_and_identities() {
    let source = "function fallback():string{console.log('fallback');return 'right';}function fail():string{throw Error('BAD');}export function main():void{console.log(''&&fail());console.log('left'||fail());console.log(''||fallback());console.log('left'&&fallback());console.log(1/(-0&&7));console.log(NaN||5);console.log(1/(NaN&&0));console.log(''??'fallback');console.log(0??5);console.log(false??true);console.log(NaN??1);const a={n:1};const b={n:2};console.log((a||b)===a);console.log((a&&b)===b);const f=():number=>1;const g=():number=>2;console.log((f||g)===f);console.log((f&&g)===g);console.log((undefined??g)());console.log((f??g)());console.log((null||f)());console.log(JSON.stringify(0||'text'));console.log(JSON.stringify('text'&&3));}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "",
            "left",
            "fallback",
            "right",
            "fallback",
            "right",
            "-Infinity",
            "5",
            "NaN",
            "",
            "0",
            "0",
            "NaN",
            "1",
            "1",
            "1",
            "1",
            "2",
            "1",
            "1",
            "\"text\"",
            "3"
        ])
    );
}
#[test]
fn guest_truth_short_circuit_effects_and_exceptions_execute_once_in_order() {
    let source = "function left(value:string|null):string|null{console.log('left');return value;}function right():string{console.log('right');throw Error('stop');}export function main():void{console.log(left('')&&right());console.log(left('yes')||right());console.log(left('')??right());try{left('yes')&&right();}catch({message}){console.log(message);}finally{console.log('done');}try{left(null)??right();}catch({message}){console.log(message);}let count=0;const first=():number=>{count++;return 0;};console.log(first()||9);console.log(count);}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "left", "", "left", "yes", "left", "", "left", "right", "stop", "done", "left",
            "right", "stop", "9", "1"
        ])
    );
}
fn fixture(source: &str) -> Vec<u8> {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(source);
    let index = export_index(&bytes, "_start") - 1;
    rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(data[0] < 127 && index < 128);
            data[0] += 1;
            data.push(4);
            data.extend(b"test");
            data.extend([0, index]);
        }
        vec![data]
    })
}
#[test]
fn guest_truth_control_flow_and_assignment_are_host_free_for_all_numeric_exponents() {
    use crate::execution::Execution;
    let bytes = fixture(
        "function fixture(n:number):number{let value:number|string|undefined=n;let output=!value?1:0;if(value){output+=2;}output+=(value||3)?4:8;output+=(value&&2)?16:32;output+=(value??1)?64:128;value||=3;value&&=4;if(value){output+=256;}return output;}export function main():void{}",
    );
    let runtime = VartaRuntime::new();
    assert!(runtime.inspect_module(&bytes).unwrap().imports.is_empty());
    let module = runtime.validate_module(&bytes).unwrap();
    for batch in 0..64_u64 {
        let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        execution.store.data_mut().host_calls = abi::MAX_HOST_CALLS;
        let function = instance
            .get_typed_func::<f64, f64>(&mut execution.store, "test")
            .unwrap();
        for exponent in batch * 32..(batch + 1) * 32 {
            for fraction in [0, 1, 0xf_ffff_ffff_ffff] {
                let n = f64::from_bits((exponent << 52) | fraction);
                for n in [n, -n] {
                    let expected = if n != 0. && !n.is_nan() { 342. } else { 421. };
                    assert_eq!(
                        function.call(&mut execution.store, n).unwrap(),
                        expected,
                        "{n:?}"
                    );
                }
            }
        }
        assert_eq!(execution.store.data().host_calls, abi::MAX_HOST_CALLS);
    }
}
#[test]
fn guest_truth_logical_numeric_results_preserve_signed_zero_and_nonfinite_values() {
    use crate::execution::Execution;
    let runtime = VartaRuntime::new();
    for (expression, mode) in [("n||-0", 0), ("n&&3", 1), ("n??3", 2)] {
        let bytes = fixture(&format!(
            "function fixture(n:number):number{{return {expression};}}export function main():void{{}}"
        ));
        assert!(runtime.inspect_module(&bytes).unwrap().imports.is_empty());
        let module = runtime.validate_module(&bytes).unwrap();
        let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let function = instance
            .get_typed_func::<f64, f64>(&mut execution.store, "test")
            .unwrap();
        for n in [
            0.,
            -0.,
            1.,
            -1.,
            f64::from_bits(1),
            f64::MAX,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
        ] {
            let truth = n != 0. && !n.is_nan();
            let want = match mode {
                0 => {
                    if truth {
                        n
                    } else {
                        -0.
                    }
                }
                1 => {
                    if truth {
                        3.
                    } else {
                        n
                    }
                }
                _ => n,
            };
            let actual = function.call(&mut execution.store, n).unwrap();
            if want.is_nan() {
                assert!(actual.is_nan());
            } else {
                assert_eq!(actual.to_bits(), want.to_bits(), "{expression}/{n:?}");
            }
        }
        assert_eq!(execution.store.data().host_calls, 0);
    }
}
#[test]
fn guest_truth_logical_assignments_and_array_predicates_use_guest_conversion() {
    let source = "function rhs():string{console.log('rhs');return 'next';}export function main():void{let value:string|undefined='';console.log(value||=rhs());console.log(value&&=rhs());console.log(value??=rhs());console.log(!value);const nums=[1,2];console.log(JSON.stringify(nums.filter((n:number):string=>n==1?'':'x')));console.log(nums.some((n:number):string=>n==2?'x':''));console.log(nums.every((n:number):{n:number}=>({n})));console.log(JSON.stringify(nums.find((n:number|undefined):string=>n===2?'x':'')));}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "rhs", "next", "rhs", "next", "next", "0", "[2]", "1", "1", "2"
        ])
    );
    let bytes = fixture(
        "function fixture():boolean{const nums=[1,2];return nums.some((n:number):string=>n==2?'x':'');}export function main():void{}",
    );
    assert!(
        VartaRuntime::new()
            .inspect_module(&bytes)
            .unwrap()
            .imports
            .is_empty()
    );
}
#[test]
fn guest_truth_cached_operands_pairs_and_saved_results_survive_collection_and_finalizers() {
    use crate::execution::Execution;
    let source = "function churn():number{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}return 1;}function make():string{churn();return 'fallback'+'!';}function keep(first:{text:string},pair:VartaResult<string>,last:string):string{churn();console.log(pair.status);return first.text+last;}function saved(value:string|undefined):string|undefined{try{return value||make();}finally{churn();String('overwrite');}}export function main():void{const record={text:'root'+'!'};let value:string|undefined='';console.log(keep(record,varta.result.capture(varta.env.get('TRUTH_PAIR_DENIED')),value||make()));console.log(saved('saved'+'!'));console.log(saved(undefined));console.log(varta.result.status());const a={n:1};const selected=a||{other:'no'};console.log(selected===a);console.log(JSON.stringify((null??a)));console.log((value||'x')&&(undefined??'yes'));}";
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
        lines(&[
            "<capability denied: env> (TRUTH_PAIR_DENIED)",
            "1",
            "root!fallback!",
            "saved!",
            "fallback!",
            "1",
            "1",
            "{\"n\":1}",
            "yes"
        ])
    );
}
#[test]
fn guest_truth_json_helper_purity_retains_live_rhs_effects_and_legacy_imports() {
    use crate::contract_tests::{export_index, rewrite};
    let runtime = VartaRuntime::new();
    for (profile, expression, allowed) in [
        (LanguageProfile::CheckedF64Utf16V23, "value||'x'", true),
        (LanguageProfile::CheckedF64Utf16V22, "value||'x'", false),
        (
            LanguageProfile::CheckedF64Utf16V23,
            "value||varta.env.get('X')",
            false,
        ),
    ] {
        let source = format!(
            "function fake(input:boolean):boolean{{const value:string|undefined='';if({expression}){{return input;}}return false;}}export function main():void{{varta.json.parse<number>('1');}}"
        );
        let bytes = varta_compiler::compile_typescript_with_profile(&source, profile).unwrap();
        let index = export_index(&bytes, "_start") - 1;
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
                assert!(index < 128 && data[at] < 128);
                data[at] = index;
            }
            vec![data]
        });
        if allowed {
            runtime.validate_module(&bad).unwrap();
        } else {
            assert!(
                runtime
                    .validate_module(&bad)
                    .unwrap_err()
                    .to_string()
                    .contains("cannot call host imports")
            );
        }
    }
}
#[test]
fn guest_truth_profiles_opaque_pairs_and_unrepresentable_unions_keep_explicit_boundaries() {
    for statement in [
        "if({n:0}){}",
        "console.log(!({n:0}));",
        "const a='x'&&'y';",
        "const f=():number=>1;const g=f||f;",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &format!("export function main():void{{{statement}}}"),
                LanguageProfile::CheckedF64Utf16V22
            )
            .is_err(),
            "{statement}"
        );
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &format!("export function main():void{{{statement}}}"),
                LanguageProfile::CheckedF64Utf16V23
            )
            .is_ok(),
            "{statement}"
        );
    }
    for statement in [
        "if(varta.result.capture(varta.env.get('X'))){}",
        "const value=varta.result.capture(varta.env.get('X'))||'x';",
        "const value='x'&&varta.result.capture(varta.env.get('X'));",
        "const f=():number=>1;let n=1;const result=n||f;",
        "const first:string|{n:number}='';const result=first||{other:1};",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &format!("export function main():void{{{statement}}}"),
                LanguageProfile::CheckedF64Utf16V23
            )
            .is_err(),
            "{statement}"
        );
    }
}

#[test]
fn guest_truth_result_types_exclude_unreachable_nullish_or_reference_members() {
    let source = "function literalAnd():string{return true&&'literal';}function orText(value:string|undefined):string{return value||'fallback';}function nullishText(value:string|null|undefined):string{return value??'fallback';}function andNumber(value:{n:number}|undefined):number|undefined{return value&&7;}export function main():void{console.log(orText(undefined));console.log(orText('yes'));console.log(nullishText(''));console.log(nullishText(null));console.log(andNumber({n:0}));console.log(andNumber(undefined));const f=():number=>9;const selected=f||{n:0};console.log(selected());console.log(literalAnd());const literalSelected=0||f;console.log(literalSelected());}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "fallback",
            "yes",
            "",
            "fallback",
            "7",
            "undefined",
            "9",
            "literal",
            "9"
        ])
    );
}
