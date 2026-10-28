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
fn compound_assignments_execute_arithmetic_statements_values_and_number_edges() {
    let source = r"export function main():void{let n=1;console.log(n+=2);console.log(n-=1);console.log(n*=2);console.log(n/=4);console.log(n%=1.5);let large=2147483647;console.log(large+=1);let zero=0;console.log(zero*=-1);console.log(zero/=0);let infinite=1;console.log(infinite/=0);let remainder=-5.5;console.log(remainder%=2);let total=0;for(let i=0;i<3;i+=1){total+=i;}console.log(total);}";
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
                "3",
                "2",
                "4",
                "1",
                "1",
                "2147483648",
                "-0",
                "NaN",
                "Infinity",
                "-1.5",
                "3"
            ]),
            "{profile:?}"
        );
    }
}

#[test]
fn compound_assignments_concat_exact_utf16_and_return_computed_strings() {
    let out = run(
        r"function append(value:{s?:string}):string{return value.s+='\ud800\u0000';}export function main():void{let text='A😀';console.log(JSON.stringify(text+='\udc00'));const value:{s?:string}={s:'x'};const result:string=append(value);console.log(JSON.stringify(result));console.log(JSON.stringify(value));const strings=['a'];console.log(strings[0]+='b');console.log(JSON.stringify(strings));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "\"A😀\\udc00\"",
            "\"x\\ud800\\u0000\"",
            "{\"s\":\"x\\ud800\\u0000\"}",
            "ab",
            "[\"ab\"]"
        ])
    );
}

#[test]
fn compound_assignments_snapshot_old_values_before_rhs_mutates_bindings_and_receivers() {
    let out = run(
        r"export function main():void{let n=1;const effect=():number=>{n=99;return 2;};console.log(n+=effect());console.log(n);let record={n:2};const first=record;const change=():number=>{first.n=50;record={n:100};return 3;};console.log(record.n*=change());console.log(first.n);console.log(record.n);let current=[4];const original=current;let index=0;const replace=():number=>{original[0]=80;original.push(9);current=[100];index=1;return 2;};console.log(current[index]-=replace());console.log(JSON.stringify(original));console.log(JSON.stringify(current));}",
    );
    assert_eq!(
        out.lines,
        lines(&["3", "3", "6", "6", "100", "2", "[2,9]", "[100]"])
    );
}

#[test]
fn compound_assignments_read_after_key_effects_and_evaluate_reference_once() {
    let out = run(
        r"function source(value:number[]):number[]{console.log('source');return value;}export function main():void{const a=[1];const key=():number=>{console.log('key');a[0]=5;return 0;};const rhs=():number=>{console.log('rhs');a[0]=99;return 2;};console.log(source(a)[key()]+=rhs());console.log(a[0]);}",
    );
    assert_eq!(out.lines, lines(&["source", "key", "rhs", "7", "7"]));
}

#[test]
fn compound_assignments_nested_chains_retain_each_old_value_and_computed_result() {
    let out = run(
        r"export function main():void{let n=1;console.log(n+=n+=2);console.log(n);let local=1;const record={n:2};const a=[4];const result:number=(local+=record.n*=a[0]+=3);console.log(result);console.log(local);console.log(record.n);console.log(a[0]);let text='a';console.log(text+=text+='b');console.log(text);}",
    );
    assert_eq!(
        out.lines,
        lines(&["4", "4", "15", "15", "14", "7", "aab", "aab"])
    );
}

#[test]
fn compound_assignments_checked_unions_match_kinds_without_coercion() {
    let out = run(
        r"function apply(value:{v:number|string|boolean},item:number|string|boolean):number|string{return value.v+=item;}export function main():void{const value:{v:number|string|boolean}={v:1};console.log(apply(value,2));value.v='a';console.log(apply(value,'b'));value.v='kept';try{apply(value,2);}catch(error){console.log(error instanceof TypeError);}console.log(value.v);value.v=false;try{apply(value,2);}catch(error){console.log(error instanceof TypeError);}console.log(value.v);value.v=5;try{apply(value,true);}catch(error){console.log(error instanceof TypeError);}console.log(value.v);}",
    );
    assert_eq!(
        out.lines,
        lines(&["3", "ab", "1", "kept", "1", "0", "1", "5"])
    );
}

#[test]
fn compound_assignments_missing_values_check_after_rhs_and_preserve_original_exceptions() {
    let out = run(
        r"function take(value:number,next:number):void{console.log('BAD_BODY');}function later():number{console.log('BAD_LATER');return 1;}export function main():void{const value:{n?:number}={};const fill=():number=>{console.log('rhs');value.n=10;return 2;};try{take((value.n+=fill()),later());}catch(error){console.log(error instanceof TypeError);}console.log(value.n);const missing:{n?:number}={};const fail=():number=>{throw RangeError('original');};try{take((missing.n+=fail()),later());}catch({message}){console.log(message);}console.log(Object.hasOwn(missing,'n'));let n=1;const mutate=():number=>{n=99;throw SyntaxError('rhs failure');};try{take((n+=mutate()),later());}catch({message}){console.log(message);}console.log(n);}",
    );
    assert_eq!(
        out.lines,
        lines(&["rhs", "1", "10", "original", "0", "rhs failure", "99"])
    );
}

#[test]
fn compound_assignments_sparse_and_invalid_reads_do_not_create_properties() {
    let out = run(
        r"function rhs():number{console.log('rhs');return 2;}export function main():void{const a:(number|undefined)[]=[1,,undefined];for(const index of [1,2,9,-1,0.5,NaN,Infinity]){if(index!==undefined){try{console.log(a[index]+=rhs());}catch(error){console.log(error instanceof TypeError);}}}console.log(JSON.stringify(a));console.log(a.length);console.log(Object.hasOwn(a,1));console.log(Object.hasOwn(a,2));console.log(a[0]+=2);}",
    );
    let mut expected = Vec::new();
    for _ in 0..7 {
        expected.extend(["rhs", "1"]);
    }
    expected.extend(["[1,null,null]", "3", "0", "1", "3"]);
    assert_eq!(out.lines, lines(&expected));
}

#[test]
fn compound_assignments_resolve_current_backing_after_rhs_and_keep_profile_bounds() {
    let out = run(
        r"export function main():void{const a=[1,2,3];const resize=():number=>{a.length=0;a.push(10);return 4;};console.log(a[2]+=resize());console.log(JSON.stringify(a));const b=[1];const shrink=():number=>{b.pop();return 2;};console.log(b[0]+=shrink());console.log(JSON.stringify(b));}",
    );
    assert_eq!(out.lines, lines(&["7", "[10,null,7]", "3", "[3]"]));
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
    ] {
        let source = "export function main():void{const a=[1];const shrink=():number=>{a.pop();console.log('rhs');return 2;};try{console.log(a[0]+=shrink());}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}";
        let bytes = varta_compiler::compile_typescript_with_profile(source, profile).unwrap();
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
fn compound_assignments_v14_length_uses_old_length_and_retains_range_errors() {
    let out = run(
        r"export function main():void{const a=[1,2,3];const reset=():number=>{a.length=0;return 1;};console.log(a.length+=reset());console.log(JSON.stringify(a));try{console.log(a.length-=5);}catch(error){console.log(error instanceof RangeError);}console.log(a.length);console.log(a['length']/=2);console.log(a.length%=1);const result:number=(a.length*=-1);console.log(1/result);console.log(a.length);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "4",
            "[null,null,null,null]",
            "1",
            "4",
            "2",
            "0",
            "-Infinity",
            "0"
        ])
    );
}

#[test]
fn compound_assignments_reject_other_operators_targets_static_mismatches_and_old_profiles() {
    for source in [
        "export function main():void{const n=1;console.log(n+=2);}",
        "export function main():void{console.log(n+=2);let n=1;}",
        "export function main():void{let n:number|string=1;if(typeof n==='number'){n+=2;}}",
        "export function main():void{let n=1;console.log(n+='x');}",
        "export function main():void{let s='x';console.log(s-=2);}",
        "export function main():void{let n=true;console.log(n+=2);}",
        "export function main():void{let n=1;console.log(n**=2);}",
        "export function main():void{let n=1;console.log(n<<=2);}",
        "export function main():void{let n=1;console.log(n&&=2);}",
        "export function main():void{let n=1;console.log(n??=2);}",
        "export function main():void{let n=1;console.log(n++);}",
        "export function main():void{const value={n:1};const key='n';console.log(value[key]**=2);}",
        "export function main():void{const value={n:1};console.log(value.missing+=2);}",
        "export function main():void{const a=[1];console.log(a['0']+=2);}",
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
        LanguageProfile::CheckedF64Utf16V3,
        LanguageProfile::CheckedF64Utf16V11,
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{let n=1;n+=2;}",
                profile
            )
            .is_err()
        );
    }
}

#[test]
fn compound_assignments_gc_retains_old_strings_union_graphs_operands_and_saved_returns() {
    let source = r"function churn():number{let text='x';for(let i=0;i<14;i+=1){text=text+text;}for(let i=0;i<260;i+=1){const garbage=[text+'!',text+'?'];garbage.slice();}return 1;}
        function work():string{let text='old'+'!';const change=():string=>{text='changed';churn();return 'rhs'+'!';};try{return text+=change();}finally{churn();}}
        function keep(first:{text:string},captured:VartaResult<string>,value:string,after:number):string{churn();console.log(captured.status);return first.text+value;}
        export function main():void{const record:{s?:string}={s:'field'+'!'};const rhs=():string=>{record.s='changed';churn();return 'append'+'!';};console.log(keep({text:'earlier'+'!'},varta.result.capture(varta.env.get('VARTA_COMPOUND_GC')),record.s+=rhs(),churn()));console.log(work());let n:number|string=0;for(let i=0;i<7000;i+=1){n+=1;}churn();console.log(n);const broad:(number|{text:string})[]=[{text:'unrooted'+'!'}];const replace=():number=>{broad[0]=1;churn();return 2;};try{console.log(broad[0]+=replace());}catch(error){console.log(error instanceof TypeError);}console.log(JSON.stringify(broad));}";
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
            "<capability denied: env> (VARTA_COMPOUND_GC)",
            "1",
            "earlier!field!append!",
            "old!rhs!",
            "7000",
            "1",
            "[1]"
        ])
    );
}

#[test]
fn compound_assignments_helper_validation_visits_source_key_and_rhs_effects() {
    use crate::contract_tests::{export_index, rewrite};
    for (source, key, rhs, expression) in [
        (
            "console.log('source');return [1];",
            "return 0;",
            "return 2;",
            "source()[key()]+=rhs()",
        ),
        (
            "return [1];",
            "console.log('key');return 0;",
            "return 2;",
            "source()[key()]+=rhs()",
        ),
        (
            "return [1];",
            "return 0;",
            "console.log('rhs');return 2;",
            "source()[key()]+=rhs()",
        ),
        (
            "return [1];",
            "return 0;",
            "console.log('rhs');return 2;",
            "local+=rhs()",
        ),
    ] {
        let bytes = compile(&format!(
            "function source():number[]{{{source}}}function key():number{{{key}}}function rhs():number{{{rhs}}}function fake(value:boolean):boolean{{let local=1;const result=({expression});return value;}}export function main():void{{varta.json.parse<number>('1');}}"
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
        let error = VartaRuntime::new().validate_module(&bad).unwrap_err();
        assert!(
            error.to_string().contains("cannot call host imports"),
            "{error}"
        );
    }
}

#[test]
fn compound_assignments_resource_exhaustion_bypasses_handlers_and_success_results() {
    for target in ["n", "record.n", "array[0]", "array.length"] {
        let bytes = compile(&format!(
            "function rhs():number{{while(true){{}}return 2;}}function take(value:number,next:number):void{{console.log('BAD_BODY');}}function later():number{{console.log('BAD_LATER');return 1;}}export function main():void{{let n=1;const record={{n:1}};const array=[1];try{{take(({target}+=rhs()),later());}}catch(error){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"
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
    let units = abi::MAX_STRING_BYTES / size_of::<u16>();
    let bytes = compile(&format!(
        "export function main():void{{let text='x'.repeat({units});console.log('before');try{{console.log(text+='x');}}catch(error){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"
    ));
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(100 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    assert!(
        instance
            .get_typed_func::<(), ()>(&mut execution.store, "_start")
            .unwrap()
            .call(&mut execution.store, ())
            .is_err()
    );
    assert_eq!(execution.store.data().output.lines, lines(&["before"]));
    let bytes = compile(
        "function rhs():number{console.log('rhs');return 1;}export function main():void{const a:number[]=[];a.length=1024;try{console.log(a.length+=rhs());}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
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
    assert_eq!(execution.store.data().output.lines, lines(&["rhs"]));
}
