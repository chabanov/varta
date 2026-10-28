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
fn updates_execute_prefix_postfix_statements_and_for_updates_in_all_admitted_profiles() {
    let source = r"export function main():void{let n=1;console.log(n++);console.log(++n);console.log(n--);console.log(--n);(n)++;++(n);console.log(n);console.log(JSON.stringify([n++,++n,n--,--n]));let sum=0;for(let i=0;i<4;i++){if(i===1){continue;}sum+=i;}console.log(sum);for(let i=3;i>0;--i){sum+=i;}console.log(sum);}";
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
            lines(&["1", "3", "3", "1", "3", "[3,5,5,3]", "5", "11"]),
            "{profile:?}"
        );
    }
}

#[test]
fn updates_preserve_binary64_edges_and_postfix_signed_zero() {
    let out = run(
        r"export function main():void{let zero=-0;console.log(1/zero++);console.log(zero);zero=0;console.log(--zero);let fraction=1.5;console.log(fraction++);console.log(--fraction);let nan=NaN;console.log(nan++);console.log(--nan);let infinite=Infinity;console.log(infinite--);console.log(++infinite);let negative=-Infinity;console.log(negative++);console.log(--negative);let large=9007199254740992;console.log(large++);console.log(large);let boundary=2147483647;console.log(++boundary);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "-Infinity",
            "1",
            "-1",
            "1.5",
            "1.5",
            "NaN",
            "NaN",
            "Infinity",
            "Infinity",
            "-Infinity",
            "-Infinity",
            "9007199254740992",
            "9007199254740992",
            "2147483648"
        ])
    );
}

#[test]
fn updates_check_closed_union_kinds_and_return_exact_number_types_without_coercion() {
    let out = run(
        r"function increment(value:number|string|boolean|null|undefined):number{return value++;}function decrement(value:{v:number|{n:number}|number[]|TypeError},key:string):number{return --value[key];}export function main():void{console.log(increment(4));for(const value of ['1','']){try{increment(value);}catch(error){console.log(error instanceof TypeError);}}try{increment(true);}catch(error){console.log(error instanceof TypeError);}try{increment(null);}catch(error){console.log(error instanceof TypeError);}try{increment(undefined);}catch(error){console.log(error instanceof TypeError);}const value:{v:number|{n:number}|number[]|TypeError}={v:3};console.log(decrement(value,'v'));console.log(value.v);value.v={n:7};try{decrement(value,'v');}catch(error){console.log(error instanceof TypeError);}console.log(JSON.stringify(value));value.v=[8];try{decrement(value,'v');}catch(error){console.log(error instanceof TypeError);}value.v=TypeError('kept');try{decrement(value,'v');}catch(error){console.log(error instanceof TypeError);}console.log(JSON.stringify(value));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "4",
            "1",
            "1",
            "1",
            "1",
            "1",
            "2",
            "2",
            "1",
            "{\"v\":{\"n\":7}}",
            "1",
            "1",
            "{\"v\":{}}"
        ])
    );
}

#[test]
fn updates_evaluate_source_key_once_before_read_and_keep_the_original_reference() {
    let out = run(
        r"function source(value:{n:number}):{n:number}{console.log('source');return value;}export function main():void{let value={n:1};const original=value;const key=():string=>{console.log('key');original.n=5;value={n:100};return 'n';};console.log(source(value)[key()]++);console.log(original.n);console.log(value.n);const a=[5,10];let index=0;console.log(a[index++]++);console.log(index);console.log(JSON.stringify(a));const b=[1,9];console.log(b[b[0]++]++);console.log(JSON.stringify(b));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "source", "key", "5", "6", "100", "5", "1", "[6,10]", "9", "[2,10]"
        ])
    );
}

#[test]
fn updates_dynamic_keys_preserve_exact_utf16_names_and_all_field_bits() {
    let out = run(
        r"export function main():void{const value={'\ue000':1,'𐀀':2,'😀':3,'':4,'a\u0000b':5,'é':6,'2':7,'01':8};console.log(JSON.stringify(['\ue000','𐀀','😀','','a\u0000b','é','2','01'].map(key=>value[key]++)));console.log(JSON.stringify(Object.values(value)));const own={constructor:1,toString:2,valueOf:3};console.log(JSON.stringify(['constructor','toString','valueOf'].map(key=>++own[key])));}",
    );
    assert_eq!(
        out.lines,
        lines(&["[1,2,3,4,5,6,7,8]", "[8,2,3,4,5,6,7,9]", "[2,3,4]"])
    );
    let fields = (0..32)
        .map(|i| format!("f{i}:{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let out = run(&format!(
        "export function main():void{{const value={{{fields}}};const key='f31';console.log(value[key]++);console.log(++value[key]);console.log(Object.hasOwn(value,key));console.log(Object.keys(value).length);}}"
    ));
    assert_eq!(out.lines, lines(&["31", "33", "1", "32"]));
}

#[test]
fn updates_missing_and_inherited_reads_fail_without_publishing_presence_or_results() {
    let out = run(
        r"function later():number{console.log('BAD_LATER');return 1;}function take(value:number,next:number):void{console.log('BAD_BODY');}export function main():void{const value:{a?:number,n:number}={n:1};try{take(value.a++,later());}catch({message}){console.log(message);}console.log(Object.hasOwn(value,'a'));value.a=undefined;try{++value.a;}catch(error){console.log(error instanceof TypeError);}console.log(Object.hasOwn(value,'a'));console.log(JSON.stringify(Object.keys(value)));for(const key of ['missing','\ud800','toString']){if(key!==undefined){try{take(value[key]++,later());}catch({message}){console.log(message);}console.log(Object.hasOwn(value,key));}}console.log(JSON.stringify(value));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "Invalid update operand",
            "0",
            "1",
            "1",
            "[\"n\",\"a\"]",
            "Invalid update operand",
            "0",
            "Invalid update operand",
            "0",
            "Inherited record property access is unsupported",
            "0",
            "{\"n\":1}"
        ])
    );
}

#[test]
fn updates_sparse_and_invalid_array_reads_do_not_create_or_extend_elements() {
    let source = r"export function main():void{const a=[1];for(const index of [-1,1.5,NaN,Infinity,1,1024]){if(index!==undefined){try{a[index]++;}catch(error){console.log(error instanceof TypeError);}}}console.log(JSON.stringify(a));console.log(a.length);}";
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
            lines(&["1", "1", "1", "1", "1", "1", "[1]", "1"])
        );
    }
    for profile in [
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        let bytes=varta_compiler::compile_typescript_with_profile("export function main():void{const a:number[]=[1,,3];try{++a[1];}catch(error){console.log(error instanceof TypeError);}console.log(Object.hasOwn(a,1));console.log(JSON.stringify(a));const b:(number|undefined)[]=[undefined];try{b[0]--;}catch(error){console.log(error instanceof TypeError);}console.log(Object.hasOwn(b,0));}",profile).unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            lines(&["1", "0", "[1,null,3]", "1", "1"])
        );
    }
}

#[test]
fn updates_v14_length_returns_old_or_new_only_after_checked_storage() {
    let out = run(
        r"export function main():void{const a=[1,2];console.log(a.length++);console.log(JSON.stringify(a));console.log(--a['length']);console.log(JSON.stringify(a));a.length=0;let returned=99;try{returned=a.length--;}catch(error){console.log(error instanceof RangeError);}console.log(returned);console.log(a.length);console.log(++a.length);console.log(JSON.stringify(a));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "2",
            "[1,2,null]",
            "2",
            "[1,2]",
            "1",
            "99",
            "0",
            "1",
            "[null]"
        ])
    );
}

#[test]
fn updates_mixed_chains_keep_successful_inner_writes_and_skip_later_work_on_failure() {
    let out = run(
        r"function partial():void{let local=1;const absent:{n?:number}={};try{console.log(local++ + absent.n++);}catch(error){console.log(error instanceof TypeError);}console.log(local);console.log(Object.hasOwn(absent,'n'));const outer={s:'x',n:0};const inner={n:1};try{outer['s'+'']+=inner['n'+'']++;}catch(error){console.log(error instanceof TypeError);}console.log(inner.n);console.log(outer.s);}export function main():void{partial();let count=0;const value={n:1};console.log(value.n||=count++);console.log(count);console.log(value.n+=++count);console.log(count);}",
    );
    assert_eq!(
        out.lines,
        lines(&["1", "2", "0", "1", "2", "x", "1", "0", "2", "1"])
    );
}

#[test]
fn updates_source_key_exceptions_keep_original_values_and_finalizers() {
    let out = run(
        r"function source():{n:number}{console.log('source');throw TypeError('source failure');}function key():string{console.log('key');throw RangeError('key failure');}function later():number{console.log('BAD_LATER');return 1;}function take(value:number,next:number):void{console.log('BAD_BODY');}export function main():void{const value={n:1};try{take(source()[key()]++,later());}catch({message}){console.log(message);}finally{console.log('source final');}try{take(++value[key()],later());}catch({message}){console.log(message);}finally{console.log('key final');}console.log(value.n);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "source",
            "source failure",
            "source final",
            "key",
            "key failure",
            "key final",
            "1"
        ])
    );
}

#[test]
fn updates_captured_bindings_and_fresh_for_iteration_cells_keep_identity() {
    let out = run(
        r"function counter():()=>number{let n=0;return ()=>n++;}export function main():void{const next=counter();console.log(next());console.log(next());let first:()=>number=()=>-1;let second:()=>number=()=>-1;for(let i=0;i<2;i++){if(i===0){first=()=>i;}else{second=()=>i;}}console.log(first());console.log(second());let value=0;const increment=():number=>++value;console.log(increment());console.log(value);}",
    );
    assert_eq!(out.lines, lines(&["0", "1", "0", "1", "1", "1"]));
}

#[test]
fn updates_gc_keeps_receivers_keys_earlier_graphs_pairs_and_saved_return_values() {
    let source = r"function churn():number{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}return 1;}function source():{n:number|undefined}{return {n:1};}function key():string{const name='n'+''.repeat(1);churn();return name;}function keep(first:{text:string},result:VartaResult<string>,value:number,last:number):string{churn();console.log(result.status);console.log(value);return first.text;}function work(value:{n?:number}):number{try{return value.n++;}finally{churn();}}export function main():void{console.log(keep({text:'earlier'+'!'},varta.result.capture(varta.env.get('VARTA_UPDATE_GC')),source()[key()]++,churn()));const value:{n?:number}={n:-0};console.log(1/work(value));console.log(value.n);const count:{n:number|undefined}={n:0};for(let i=0;i<4000;i++){count.n++;}churn();console.log(count.n);}";
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
            "<capability denied: env> (VARTA_UPDATE_GC)",
            "1",
            "1",
            "earlier!",
            "-Infinity",
            "1",
            "4000"
        ])
    );
}

#[test]
fn updates_helper_validation_checks_source_and_key_effects_without_new_host_arithmetic() {
    use crate::contract_tests::{export_index, rewrite};
    for (source, key, prefix, has_host) in [
        ("return {n:1};", "return 'n';", false, false),
        ("return {n:1};", "return 'n';", true, false),
        (
            "console.log('source');return {n:1};",
            "return 'n';",
            false,
            true,
        ),
        (
            "return {n:1};",
            "console.log('key');return 'n';",
            true,
            true,
        ),
    ] {
        let expression = if prefix {
            "++source()[key()]"
        } else {
            "source()[key()]++"
        };
        let bytes = compile(&format!(
            "function source():{{n:number}}{{{source}}}function key():string{{{key}}}function fake(value:boolean):boolean{{const result=({expression});return value;}}export function main():void{{varta.json.parse<number>('1');}}"
        ));
        let fake = export_index(&bytes, "_start") - 1;
        let name = varta_common::value::VALUE_ALLOCATE;
        let rewritten = rewrite(&bytes, |id, data| {
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
        let result = VartaRuntime::new().validate_module(&rewritten);
        if has_host {
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
fn updates_reject_invalid_types_targets_legacy_profiles_and_keep_fatal_limits() {
    for source in [
        "export function main():void{const n=1;n++;}",
        "export function main():void{n++;let n=1;}",
        "export function main():void{let n:number|undefined=1;if(n!==undefined){++n;}}",
        "export function main():void{let n='1';n++;}",
        "export function main():void{let n=true;--n;}",
        "export function main():void{const a=[1];a['0']++;}",
        "export function main():void{const a=[1];++a[false];}",
        "export function main():void{const r={n:1};const key=0;r[key]++;}",
        "export function main():void{const r={n:1};const key:string|undefined='n';r[key]++;}",
        "export function main():void{const r:{n:number}|undefined={n:1};r.n++;}",
        "export function main():void{let fn:()=>number=()=>1;fn++;}",
        "export function main():void{let result=varta.result.capture(varta.env.get('X'));result++;}",
        "export function main():void{const r={a:{n:1},b:{s:'x'},n:1};const key='n';r[key]++;}",
        "export function main():void{const e=Error('x');e.message++;}",
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
                "export function main():void{let n=1;n++;}",
                profile
            )
            .is_err()
        );
    }
    assert!(
        varta_compiler::compile_typescript_with_profile(
            "export function main():void{const a=[1];a.length++;}",
            LanguageProfile::CheckedF64Utf16V13
        )
        .is_err()
    );
    for source in [
        "function source():{n:number}{while(true){}return {n:1};}export function main():void{try{console.log(source().n++);}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
        "function key():string{while(true){}return 'n';}export function main():void{const value={n:1};try{console.log(++value[key()]);}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
        "export function main():void{const a:number[]=[];a.length=1024;try{console.log(a.length++);}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
    ] {
        let runtime = VartaRuntime::new();
        let module = runtime.validate_module(&compile(source)).unwrap();
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
