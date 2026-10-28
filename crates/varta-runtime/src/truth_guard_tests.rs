use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V24)
        .unwrap()
}
fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| format!("varta log: {s}")).collect()
}
#[test]
fn truth_guards_direct_locals_narrow_nullable_records_arrays_and_errors() {
    let source = "function record(value:{n:number}|null|undefined):number{if(value){return value.n;}else{return 0;}}function array(value:number[]|undefined):number{return value?value.length:0;}function error(value:Error|null):string{if(!value){return 'none';}else{return value.message;}}export function main():void{console.log(record({n:7}));console.log(record(null));console.log(record(undefined));console.log(array([1,2]));console.log(array(undefined));console.log(error(Error('yes')));console.log(error(null));}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["7", "0", "0", "2", "0", "yes", "none"])
    );
}
#[test]
fn truth_guards_false_branches_retain_zero_nan_empty_string_and_false() {
    let source = "function number(value:number|undefined):string{if(value){return value.toFixed(1);}else{return String(value);}}function text(value:string|undefined):string{if(!value){return String(value);}else{return value+'!';}}function boolean(value:boolean|undefined):string{return value?String(value):String(value);}export function main():void{console.log(number(0));console.log(number(-0));console.log(number(NaN));console.log(number(undefined));console.log(number(2));console.log(text(''));console.log(text(undefined));console.log(text('x'));console.log(boolean(false));console.log(boolean(undefined));console.log(boolean(true));}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&[
            "0",
            "0",
            "NaN",
            "undefined",
            "2.0",
            "",
            "undefined",
            "x!",
            "false",
            "undefined",
            "true"
        ])
    );
    for ty in ["number", "string", "boolean"] {
        let source = format!(
            "function missing(value:undefined):void{{}}export function main():void{{const value:{ty}|undefined=undefined;if(!value){{missing(value);}}}}"
        );
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &source,
                LanguageProfile::CheckedF64Utf16V24
            )
            .is_err(),
            "{ty}"
        );
    }
}
#[test]
fn truth_guards_short_circuit_rhs_uses_truth_false_and_nullish_information() {
    let source = "function missing(value:undefined):{n:number}{return {n:9};}function nullish(value:null|undefined):string{return 'missing';}function and(value:{n:number}|undefined):number|undefined{return value&&value.n;}function or(value:{n:number}|undefined):{n:number}{return value||missing(value);}function fallback(value:string|null|undefined):string{return value??nullish(value);}export function main():void{console.log(and({n:7}));console.log(and(undefined));console.log(or({n:2}).n);console.log(or(undefined).n);console.log(fallback(''));console.log(fallback(null));console.log(fallback(undefined));}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["7", "undefined", "2", "9", "", "missing", "missing"])
    );
}
#[test]
fn truth_guards_compound_true_and_false_paths_combine_existing_type_guards() {
    let source = "function and(value:string|{n:number}|undefined):number{if(value&&typeof value==='object'){return value.n;}else{return 0;}}function or(value:string|{n:number}|undefined):number{if(!value||typeof value==='string'){return 0;}else{return value.n;}}function text(value:string|{n:number}|undefined):number|boolean{return typeof value==='string'&&value.length;}export function main():void{console.log(and({n:7}));console.log(and(''));console.log(and(undefined));console.log(or({n:8}));console.log(or('x'));console.log(or(undefined));console.log(text('yes'));console.log(text({n:1}));}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["7", "0", "0", "8", "0", "0", "3", "0"])
    );
}
#[test]
fn truth_guards_while_and_for_refine_only_guarded_body_and_update() {
    let source = "function loop(value:{n:number}|undefined):number{let total=0;while(value){total+=value.n;break;}for(;value;total+=value.n){if(total>2){break;}}return total;}export function main():void{console.log(loop({n:2}));console.log(loop(undefined));}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["4", "0"])
    );
    for statement in [
        "if(value){}console.log(value.n);",
        "value&&value.n;console.log(value.n);",
        "while(value){break;}console.log(value.n);",
        "for(;value;){break;}console.log(value.n);",
        "do{console.log(value.n);}while(value);",
    ] {
        let source = format!(
            "export function main():void{{const value:{{n:number}}|undefined=undefined;{statement}}}"
        );
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &source,
                LanguageProfile::CheckedF64Utf16V24
            )
            .is_err(),
            "{statement}"
        );
    }
}
#[test]
fn truth_guards_redundant_tests_do_not_invent_mutable_capture_restrictions() {
    let source = "export function main():void{let value:string|number='x';const read=():string|number=>value;if(value){console.log(Boolean(value));}console.log(read());let object:{n:number}|number[]={n:3};const get=():{n:number}|number[]=>object;if(typeof object==='object'){console.log(Boolean(object));}console.log(Boolean(get()));}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["1", "x", "1", "1"])
    );
}
#[test]
fn truth_guards_mutations_and_captures_cannot_invalidate_narrowed_reads() {
    for statement in [
        "let value:{n:number}|undefined={n:1};const mutate=():void=>{value=undefined;};if(value){mutate();console.log(value.n);}",
        "let value:{n:number}|undefined={n:1};value&&(():number=>value.n)();",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &format!("export function main():void{{{statement}}}"),
                LanguageProfile::CheckedF64Utf16V24
            )
            .is_err(),
            "{statement}"
        );
    }
}
#[test]
fn truth_guards_numeric_reads_preserve_all_exponents_without_native_truth_calls() {
    use crate::{
        contract_tests::{export_index, rewrite},
        execution::Execution,
    };
    let bytes = compile(
        "function fixture(n:number):number{const value:number|undefined=n;if(value){return value;}else{return n;}}export function main():void{}",
    );
    let index = export_index(&bytes, "_start") - 1;
    let bytes = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(data[0] < 127 && index < 128);
            data[0] += 1;
            data.push(4);
            data.extend(b"test");
            data.extend([0, index]);
        }
        vec![data]
    });
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
                    let actual = function.call(&mut execution.store, n).unwrap();
                    if n.is_nan() {
                        assert!(actual.is_nan());
                    } else {
                        assert_eq!(actual.to_bits(), n.to_bits());
                    }
                }
            }
        }
        assert_eq!(execution.store.data().host_calls, abi::MAX_HOST_CALLS);
    }
}
#[test]
fn truth_guards_refined_strings_records_pairs_and_saved_returns_survive_collection() {
    use crate::execution::Execution;
    let source = "function churn():number{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}return 1;}function use(value:{text:string}):string{churn();return value.text;}function refined(value:string|{text:string}|undefined):string{if(value&&typeof value==='string'){churn();return value;}else{return 'missing';}}function saved(value:{text:string}|undefined):string|undefined{try{return value&&use(value);}finally{churn();String('overwrite');}}function keep(first:{text:string},pair:VartaResult<string>,last:string):string{churn();console.log(pair.status);return first.text+last;}export function main():void{console.log(refined('root'+'!'));console.log(refined(undefined));console.log(saved({text:'saved'+'!'}));console.log(saved(undefined));console.log(keep({text:'first'+'!'},varta.result.capture(varta.env.get('GUARD_PAIR_DENIED')),refined('last'+'!')));console.log(varta.result.status());}";
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
            "root!",
            "missing",
            "saved!",
            "undefined",
            "<capability denied: env> (GUARD_PAIR_DENIED)",
            "1",
            "first!last!",
            "1"
        ])
    );
}
#[test]
fn truth_guards_legacy_profiles_and_binding_identity_remain_explicit() {
    for statement in [
        "const value:{n:number}|undefined=undefined;if(value){console.log(value.n);}",
        "const value:{n:number}|undefined=undefined;value&&value.n;",
        "const value:string|undefined='';typeof value==='string'&&value.length;",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &format!("export function main():void{{{statement}}}"),
                LanguageProfile::CheckedF64Utf16V23
            )
            .is_err(),
            "{statement}"
        );
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &format!("export function main():void{{{statement}}}"),
                LanguageProfile::CheckedF64Utf16V24
            )
            .is_ok(),
            "{statement}"
        );
    }
    let source = "function show(value:{n:number}|undefined):number{if(value){const value={n:8};return value.n;}else{return 0;}}export function main():void{console.log(show({n:1}));console.log(show(undefined));}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["8", "0"])
    );
}
#[test]
fn truth_guards_rhs_throw_skips_later_effects_and_restores_outer_scope() {
    let source = "function fail(value:{n:number}):number{console.log(value.n);throw Error('stop');}function and(value:{n:number}|undefined):void{try{console.log(value&&fail(value));}catch({message}){console.log(message);}finally{console.log('done');}console.log(Boolean(value));}export function main():void{and({n:7});and(undefined);}";
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap()
            .lines,
        lines(&["7", "stop", "done", "1", "undefined", "done", "0"])
    );
}

#[test]
fn truth_guards_v23_keeps_preexisting_truth_branch_mutation_admission() {
    let source = "export function main():void{let value:{n:number}|undefined={n:1};if(value){value=undefined;}console.log(Boolean(value));}";
    let bytes = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V23,
    )
    .unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap()
            .lines,
        lines(&["0"])
    );
    let bytes = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V24,
    )
    .unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap()
            .lines,
        lines(&["0"])
    );
}
#[test]
fn truth_guards_metadata_requires_supported_profile_with_shared_abi26() {
    use crate::contract_tests::rewrite;
    let bytes = compile(
        "export function main():void{const value:{n:number}|undefined={n:1};if(value){console.log(value.n);}}",
    );
    let runtime = VartaRuntime::new();
    runtime.validate_module(&bytes).unwrap();
    for action in 0..5 {
        let rewritten = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 0
                && data.get(1..1 + abi::SECTION.len()) == Some(abi::SECTION.as_bytes())
                && action == 0
            {
                data[1 + abi::SECTION.len()..]
                    .copy_from_slice(&abi::NUMBER_FORMAT_VERSION.to_le_bytes());
            }
            if id == 0
                && data.get(1..1 + varta_common::language::SECTION.len())
                    == Some(varta_common::language::SECTION.as_bytes())
            {
                if action == 3 {
                    return vec![];
                }
                if matches!(action, 1 | 2 | 4) {
                    let name = b"checked-f64-utf16-v24";
                    let at =
                        data.windows(name.len()).position(|s| s == name).unwrap() + name.len() - 1;
                    if action == 1 {
                        // An invalid profile family stays unknown as versions are added.
                        data[at - 2] = b'x';
                    } else {
                        data[at] = if action == 4 { b'5' } else { b'3' };
                    }
                }
            }
            vec![data]
        });
        if matches!(action, 2 | 4) {
            runtime.validate_module(&rewritten).unwrap();
        } else {
            assert!(runtime.validate_module(&rewritten).is_err(), "{action}");
        }
    }
}
