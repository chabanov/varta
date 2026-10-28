use super::*;
use varta_common::language::LanguageProfile;

fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V14)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| format!("varta log: {s}")).collect()
}

#[test]
fn logical_assignments_preserve_truthy_falsy_and_nullish_value_semantics() {
    let source = r"function rhs():number{console.log('rhs');return 7;}function test(value:number):void{let a=value;console.log(a&&=rhs());let b=value;console.log(b||=rhs());let c=value;console.log(c??=rhs());}export function main():void{test(0);test(-0);test(NaN);test(2);test(Infinity);let yes=true;console.log(yes&&=false);console.log(yes||=true);let text='';console.log(text&&='BAD');console.log(text||='kept');console.log(text??='BAD');let absent:number|null|undefined=undefined;console.log(absent??=3);absent=null;console.log(absent||=4);absent=undefined;console.log(absent&&=5);}";
    let mut expected = Vec::new();
    for value in ["0", "-0", "NaN"] {
        expected.extend([value, "rhs", "7", value]);
    }
    for value in ["2", "Infinity"] {
        expected.extend(["rhs", "7", value, value]);
    }
    expected.extend(["0", "1", "", "kept", "kept", "3", "4", "undefined"]);
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        let bytes = varta_compiler::compile_typescript_with_profile(source, profile).unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            lines(&expected),
            "{profile:?}"
        );
    }
}

#[test]
fn logical_assignments_unions_prune_only_proven_skipped_kinds() {
    let out = run(
        r"function number(value:number|undefined):number{return value??=2;}function truth(value:number|null|undefined):number{return value||=3;}function falsity(value:number|undefined):number|undefined{return value&&=4;}function texts(value:string|null):string{return value??='x';}export function main():void{console.log(number(undefined));console.log(number(-0));console.log(truth(null));console.log(truth(0));console.log(truth(5));console.log(falsity(undefined));console.log(falsity(0));console.log(falsity(2));console.log(texts(null));console.log(JSON.stringify(texts('')));const inputs:(number|undefined)[]=[undefined,1,0];console.log(JSON.stringify(inputs.map(value=>value??=8)));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "2",
            "-0",
            "3",
            "3",
            "5",
            "undefined",
            "0",
            "4",
            "x",
            "\"\"",
            "[8,1,0]"
        ])
    );
}

#[test]
fn logical_assignments_prepare_reference_once_and_keep_rhs_mutations_on_skips() {
    let out = run(
        r"function source(value:{n:number}):{n:number}{console.log('source');return value;}export function main():void{let value={n:1};const original=value;const key=():string=>{console.log('key');original.n=0;value={n:100};return 'n';};const rhs=():number=>{console.log('rhs');original.n=9;value={n:200};return 3;};console.log(source(value)[key()]||=rhs());console.log(original.n);console.log(value.n);const skip=():string=>{console.log('skip key');original.n=4;return 'n';};console.log(source(original)[skip()]||=rhs());console.log(original.n);let local=0;const change=():number=>{local=99;return 2;};console.log(local||=change());console.log(local);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "source", "key", "rhs", "3", "3", "200", "source", "skip key", "4", "4", "2", "2"
        ])
    );
}

#[test]
fn logical_assignments_optional_fields_preserve_absence_and_insertion_order() {
    let out = run(
        r"function rhs():number{console.log('rhs');return 2;}export function main():void{const value:{a?:number,b:number}={b:1};console.log(value.a&&=rhs());console.log(Object.hasOwn(value,'a'));console.log(value.a??=rhs());console.log(Object.hasOwn(value,'a'));console.log(value.a||=rhs());console.log(JSON.stringify(Object.keys(value)));value.a=undefined;console.log(value.a&&=rhs());console.log(Object.hasOwn(value,'a'));const nulls:{n:number|null}={n:null};console.log(nulls.n&&=rhs());console.log(nulls.n??=rhs());console.log(nulls.n);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "undefined",
            "0",
            "rhs",
            "2",
            "1",
            "2",
            "[\"b\",\"a\"]",
            "undefined",
            "1",
            "null",
            "rhs",
            "2",
            "2"
        ])
    );
}

#[test]
fn logical_assignments_dynamic_keys_check_only_selected_stores_after_rhs() {
    let out = run(
        r"function rhs():number{console.log('rhs');return 7;}function apply(value:{n:number,s:string,a?:number},key:string):number|string|undefined{return value[key]||=rhs();}export function main():void{const value:{n:number,s:string,a?:number}={n:0,s:'kept'};console.log(apply(value,'s'));console.log(apply(value,'n'));value.s='';try{apply(value,'s');}catch(error){console.log(error instanceof TypeError);}console.log(value.s);console.log(value['missing'+'']&&=rhs());console.log(Object.hasOwn(value,'missing'));try{apply(value,'missing');}catch(error){console.log(error instanceof TypeError);}console.log(Object.hasOwn(value,'missing'));console.log(apply(value,'a'));console.log(JSON.stringify(Object.keys(value)));try{value['to'+'String']&&=rhs();}catch(error){console.log(error instanceof TypeError);}}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "kept",
            "rhs",
            "7",
            "rhs",
            "1",
            "",
            "undefined",
            "0",
            "rhs",
            "1",
            "0",
            "rhs",
            "7",
            "[\"n\",\"s\",\"a\"]",
            "1"
        ])
    );
}

#[test]
fn logical_assignments_reference_and_error_identity_and_arrow_context_are_preserved() {
    let out = run(
        r"export function main():void{const first={n:1};let record=first;const next={n:2};console.log((record||=next)===first);console.log((record&&=next)===next);console.log((record??=first)===next);const a=[1];let array=a;const b=[2];console.log((array||=b)===a);console.log((array&&=b)===b);let error:Error=TypeError('old');const saved=error;console.log((error||=RangeError('BAD'))===saved);console.log((error&&=RangeError('new')).name);let fn:(n:number)=>number=n=>n+1;const original=fn;console.log((fn||=(n=>n+2))===original);console.log((fn&&=(n=>n+3))(4));console.log((fn??=(n=>n+9))(4));const box:{r?:{n:number}}={};const result:{n:number}=(box.r??={n:8});console.log(result===box.r);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "1",
            "1",
            "1",
            "1",
            "1",
            "1",
            "RangeError",
            "1",
            "7",
            "7",
            "1"
        ])
    );
}

#[test]
fn logical_assignments_arrays_skip_invalid_stores_and_keep_profile_bounds() {
    let source = r"function rhs():number{console.log('rhs');return 9;}export function main():void{const a=[0,2];console.log(a[0]||=rhs());console.log(a[1]&&=rhs());console.log(a[9]&&=rhs());console.log(a[-1]&&=rhs());console.log(a[0]??=rhs());console.log(JSON.stringify(a));}";
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        let bytes = varta_compiler::compile_typescript_with_profile(source, profile).unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            lines(&[
                "rhs",
                "9",
                "rhs",
                "9",
                "undefined",
                "undefined",
                "9",
                "[9,9]"
            ])
        );
    }
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
    ] {
        let bytes=varta_compiler::compile_typescript_with_profile("function rhs():number{console.log('rhs');return 1;}export function main():void{const a=[0];try{a[2]??=rhs();}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",profile).unwrap();
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
        assert_eq!(execution.store.data().output.lines, lines(&["rhs"]));
    }
}

#[test]
fn logical_assignments_v14_sparse_current_storage_and_length_validation() {
    let out = run(
        r"export function main():void{const a=[1];delete a[0];console.log(a[0]&&=2);console.log(Object.hasOwn(a,0));console.log(a[0]??=3);console.log(Object.hasOwn(a,0));console.log(a[3]||=4);console.log(JSON.stringify(a));const shrink=():number=>{a.length=0;return 8;};console.log(a[0]&&=shrink());console.log(JSON.stringify(a));console.log(a.length??=1.5);try{a.length&&=1.5;}catch(error){console.log(error instanceof RangeError);}a.length=0;console.log(a.length&&=1.5);console.log(a.length||=2);console.log(JSON.stringify(a));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "undefined",
            "0",
            "3",
            "1",
            "4",
            "[3,null,null,4]",
            "8",
            "[8]",
            "1",
            "1",
            "0",
            "2",
            "[null,null]"
        ])
    );
}

#[test]
fn logical_assignments_nested_chains_skip_complete_inner_references_and_keep_partial_writes() {
    let out = run(
        r"function key():string{console.log('key');return 'n';}function rhs():number{console.log('rhs');return 5;}function nested():void{let value=1;const record={n:0};console.log(value||=record[key()]||=rhs());value=0;console.log(value||=record[key()]||=rhs());console.log(value);console.log(record.n);}function partial():void{const outer={s:'' ,n:0};const inner={n:0};try{outer['s'+'']||=inner['n'+'']||=3;}catch(error){console.log(error instanceof TypeError);}console.log(inner.n);console.log(outer.s);const missing:{n?:number}={};let local:number|undefined=0;console.log(local||=missing['n'+'']&&=rhs());}export function main():void{nested();partial();}",
    );
    assert_eq!(
        out.lines,
        lines(&["1", "key", "rhs", "5", "5", "5", "1", "3", "", "undefined"])
    );
}

#[test]
fn logical_assignments_source_key_rhs_exceptions_and_finalizers_stop_later_arguments() {
    let out = run(
        r"function source():{n:number}{console.log('source');throw TypeError('source');}function key():string{console.log('key');throw RangeError('key');}function rhs():number{console.log('rhs');throw SyntaxError('rhs');}function later():number{console.log('BAD_LATER');return 1;}function take(value:number|undefined,next:number):void{console.log('BAD_BODY');}export function main():void{const value={n:0};try{take(source()[key()]||=rhs(),later());}catch({message}){console.log(message);}try{take(value[key()]||=rhs(),later());}catch({message}){console.log(message);}try{take(value.n||=rhs(),later());}catch({message}){console.log(message);}finally{console.log('final');}console.log(value.n);}",
    );
    assert_eq!(
        out.lines,
        lines(&["source", "source", "key", "key", "rhs", "rhs", "final", "0"])
    );
}

#[test]
fn logical_assignments_reject_invalid_targets_types_and_legacy_profiles() {
    for source in [
        "export function main():void{const n=1;n||=2;}",
        "export function main():void{n??=2;let n=1;}",
        "export function main():void{let n:number|undefined=1;if(n!==undefined){n||=2;}}",
        "export function main():void{let n=1;n||='wrong';}",
        "export function main():void{const a=[1];a['0']||=2;}",
        "export function main():void{const a=[1];a[false]??=2;}",
        "export function main():void{const r={n:1};const key=0;r[key]??=2;}",
        "export function main():void{const r={n:1};const key:string|undefined='n';r[key]||=2;}",
        "export function main():void{let result=varta.result.capture(varta.env.get('X'));result||=varta.result.capture(varta.env.get('X'));}",
        "export function main():void{const e=Error('x');e.message||='y';}",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V14,
        )
        .unwrap_err();
        assert!(error.diagnostic().is_some(), "{source}: {error}");
    }
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf16V11,
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{let n=0;n||=2;}",
                profile
            )
            .is_err()
        );
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{const a=[1];a.length||=2;}",
                LanguageProfile::CheckedF64Utf16V13
            )
            .is_err()
        );
    }
}

#[test]
fn logical_assignments_gc_preserves_detached_sources_keys_old_values_and_saved_results() {
    let source = r"function churn():number{let text='x';for(let i=0;i<14;i+=1){text=text+text;}for(let i=0;i<260;i+=1){const trash=[text+'!',text+'?'];trash.slice();}return 1;}function keep(first:{text:string},result:VartaResult<string>,value:string|undefined,last:number):string{churn();console.log(result.status);if(value!==undefined){return first.text+value;}return 'BAD';}function work():string{let value:{s?:string}={};const rhs=():string=>{value={s:'changed'};churn();return 'saved'+'!';};try{return value['s'+'']??=rhs();}finally{churn();}}export function main():void{let value={s:'old'+'!'};let key='s'+''.repeat(1);const rhs=():string=>{value={s:'new'};key='other';churn();return 'rhs'+'!';};console.log(keep({text:'earlier'+'!'},varta.result.capture(varta.env.get('VARTA_LOGICAL_GC')),value[key]&&=rhs(),churn()));console.log(work());const graph:{v:{text:string}|undefined}={v:{text:'kept'+'!'}};const selected=(graph.v||={text:'BAD'});graph.v=undefined;churn();console.log(selected.text);const count={n:1};for(let i=0;i<4000;i+=1){count.n&&=i+1;}churn();console.log(count.n);}";
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
        lines(&[
            "<capability denied: env> (VARTA_LOGICAL_GC)",
            "1",
            "earlier!rhs!",
            "saved!",
            "kept!",
            "4000"
        ])
    );
}

#[test]
fn logical_assignments_helper_validation_sees_even_skipped_rhs_effects() {
    use crate::contract_tests::{export_index, rewrite};
    for (expression, source, key, rhs, has_host_effect) in [
        (
            "source()[key()]??=rhs()",
            "return {n:0};",
            "return 'n';",
            "return 2;",
            false,
        ),
        (
            "local||=rhs()",
            "return {n:0};",
            "return 'n';",
            "console.log('rhs');return 2;",
            true,
        ),
        (
            "local??=rhs()",
            "return {n:0};",
            "return 'n';",
            "console.log('rhs');return 2;",
            true,
        ),
        (
            "source()[key()]??=rhs()",
            "console.log('source');return {n:0};",
            "return 'n';",
            "return 2;",
            true,
        ),
        (
            "source()[key()]??=rhs()",
            "return {n:0};",
            "console.log('key');return 'n';",
            "return 2;",
            true,
        ),
        (
            "source()[key()]??=rhs()",
            "return {n:0};",
            "return 'n';",
            "console.log('rhs');return 2;",
            true,
        ),
    ] {
        let bytes = compile(&format!(
            "function source():{{n:number}}{{{source}}}function key():string{{{key}}}function rhs():number{{{rhs}}}function fake(value:boolean):boolean{{let local=1;const result=({expression});return value;}}export function main():void{{varta.json.parse<number>('1');}}"
        ));
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
        let result = VartaRuntime::new().validate_module(&bad);
        if has_host_effect {
            let error = result.unwrap_err();
            assert!(
                error.to_string().contains("cannot call host imports"),
                "{error}"
            );
        } else {
            result.unwrap();
        }
    }
}

#[test]
fn logical_assignments_skipped_fatal_rhs_does_not_run_and_selected_limits_stay_fatal() {
    let out = run(
        r"function forever():number{while(true){}return 2;}export function main():void{let yes=1;console.log(yes||=forever());console.log(yes??=forever());let zero=0;console.log(zero&&=forever());const a=[1];console.log(a[-1]&&=forever());}",
    );
    assert_eq!(out.lines, lines(&["1", "1", "0", "undefined"]));
    for expression in ["n||=rhs()", "record['n'+'']??=rhs()", "array[0]||=rhs()"] {
        let bytes = compile(&format!(
            "function rhs():number{{while(true){{}}return 2;}}export function main():void{{let n=0;const record:{{n?:number}}={{}};const array=[0];try{{console.log({expression});}}catch(error){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"
        ));
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
        assert!(execution.store.data().output.lines.is_empty());
    }
}
