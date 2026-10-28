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
fn bitwise_compounds_execute_all_operators_and_number_results_in_admitted_profiles() {
    let source = r"function unsigned(value:number|undefined):number{return value>>>=1;}export function main():void{let n=13;console.log(n&=7);console.log(n|=8);console.log(n^=3);console.log(n<<=2);console.log(n>>=3);console.log(n>>>=1);console.log(n);console.log(unsigned(-1));let total=0;for(let bit=1;bit<8;bit<<=1){total|=bit;}console.log(total);}";
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
            lines(&["5", "13", "14", "56", "7", "3", "3", "2147483647", "7"]),
            "{profile:?}"
        );
    }
}

#[test]
fn bitwise_compounds_preserve_full_range_conversions_unsigned_storage_and_positive_zero() {
    let out = run(
        r"export function main():void{let n=NaN;console.log(n|=1);n=Infinity;console.log(n>>>=0);n=-1;console.log(n>>>=0);console.log(n);n=4294967297;console.log(n|=0);n=2147483648;console.log(n>>=0);n=1;console.log(n<<=-1);n=1;console.log(n<<=33);n=3;console.log(n>>=NaN);console.log(n<<=Infinity);n=-0;console.log(1/(n&=0));n=1.9;console.log(n&=3.9);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "1",
            "0",
            "4294967295",
            "4294967295",
            "1",
            "-2147483648",
            "-2147483648",
            "2",
            "3",
            "3",
            "Infinity",
            "1"
        ])
    );
}

#[test]
fn bitwise_compounds_prepare_source_key_and_old_value_before_rhs_mutation() {
    let out = run(
        r"function source(value:{n:number}):{n:number}{console.log('source');return value;}export function main():void{let n=5;const change=():number=>{n=99;return 2;};console.log(n|=change());console.log(n);let value={n:1};const original=value;let name='n';const key=():string=>{console.log('key');original.n=9;value={n:100};return name;};const rhs=():number=>{console.log('rhs');original.n=99;value={n:200};name='other';return 3;};console.log(source(value)[key()]^=rhs());console.log(original.n);console.log(value.n);let current=[4];const old=current;let index=0;const replace=():number=>{old[0]=99;old.push(1);current=[100];index=1;return 1;};console.log(current[index]<<=replace());console.log(JSON.stringify(old));console.log(JSON.stringify(current));console.log(index);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "7", "7", "source", "key", "rhs", "10", "10", "200", "8", "[8,1]", "[100]", "1"
        ])
    );
}

#[test]
fn bitwise_compounds_match_actual_number_tags_after_rhs_without_coercion() {
    let out = run(
        r"function apply(value:{n:number,s:string,b:boolean,r:{x:number},a:number[],e:TypeError},key:string,rhs:number|string|boolean):number{return value[key]|=rhs;}function effect():number{console.log('rhs');return 2;}export function main():void{const value={n:1,s:'1',b:true,r:{x:2},a:[3],e:TypeError('kept')};for(const key of ['s','b','r','a','e']){if(key!==undefined){try{apply(value,key,effect());}catch(error){console.log(error instanceof TypeError);}}}try{apply(value,'n',true);}catch(error){console.log(error instanceof TypeError);}console.log(apply(value,'n',2));console.log(JSON.stringify(value));}",
    );
    let mut expected = Vec::new();
    for _ in 0..5 {
        expected.extend(["rhs", "1"]);
    }
    expected.extend([
        "1",
        "3",
        "{\"n\":3,\"s\":\"1\",\"b\":true,\"r\":{\"x\":2},\"a\":[3],\"e\":{}}",
    ]);
    assert_eq!(out.lines, lines(&expected));
}

#[test]
fn bitwise_compounds_absence_and_inherited_errors_keep_rhs_effects_and_original_exceptions() {
    let out = run(
        r"function fail():number{console.log('throw rhs');throw RangeError('original');}function rhs():number{console.log('rhs');return 2;}export function main():void{const value:{a?:number,n:number}={n:1};const fill=():number=>{console.log('fill');value.a=9;return 2;};try{value.a|=fill();}catch({message}){console.log(message);}console.log(value.a);value.a=undefined;try{value['a'+'']<<=fail();}catch({name,message}){console.log(name);console.log(message);}for(const key of ['missing','toString']){if(key!==undefined){try{value[key]^=rhs();}catch({message}){console.log(message);}console.log(Object.hasOwn(value,key));}}console.log(JSON.stringify(Object.keys(value)));console.log(JSON.stringify(value));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "fill",
            "Invalid compound bitwise operands",
            "9",
            "throw rhs",
            "RangeError",
            "original",
            "rhs",
            "Invalid compound bitwise operands",
            "0",
            "Inherited record property access is unsupported",
            "0",
            "[\"n\",\"a\"]",
            "{\"n\":1}"
        ])
    );
}

#[test]
fn bitwise_compounds_exact_utf16_keys_and_high_field_bit_preserve_present_own_storage() {
    let out = run(
        r"export function main():void{const value={'\ue000':1,'𐀀':2,'😀':3,'':4,'a\u0000b':5,'é':6,'2':7,'01':8};console.log(JSON.stringify(['\ue000','𐀀','😀','','a\u0000b','é','2','01'].map(key=>value[key]|=16)));console.log(JSON.stringify(Object.values(value)));const own={constructor:1,toString:2};console.log(own['construc'+'tor']<<=1);console.log(own['to'+'String']^=3);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "[17,18,19,20,21,22,23,24]",
            "[23,17,18,19,20,21,22,24]",
            "2",
            "1"
        ])
    );
    let fields = (0..32)
        .map(|i| format!("f{i}:{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let out = run(&format!(
        "export function main():void{{const value={{{fields}}};const key='f31';console.log(value[key]|=32);console.log(value.f31);console.log(Object.hasOwn(value,key));console.log(Object.keys(value).length);}}"
    ));
    assert_eq!(out.lines, lines(&["63", "63", "1", "32"]));
}

#[test]
fn bitwise_compounds_nested_mixed_chains_retain_each_snapshot_and_partial_store() {
    let out = run(
        r"function nested():void{let local=1;const record={n:2};const values=[4];const key='n';console.log(local|=record[key]<<=values[0]^=1);console.log(record.n);console.log(JSON.stringify(values));let same=1;console.log(same<<=same|=2);console.log(same);}function partial():void{const outer={n:0,s:'kept'};const inner={n:2};try{outer['s'+'']|=inner.n^=1;}catch(error){console.log(error instanceof TypeError);}console.log(inner.n);console.log(outer.s);let dest=8;const absent:{n?:number}={};try{dest>>=absent.n|=2;}catch(error){console.log(error instanceof TypeError);}console.log(dest);console.log(Object.hasOwn(absent,'n'));}export function main():void{nested();partial();}",
    );
    assert_eq!(
        out.lines,
        lines(&["65", "64", "[5]", "8", "8", "1", "3", "kept", "1", "8", "0"])
    );
}

#[test]
fn bitwise_compounds_sparse_and_invalid_reads_fail_after_rhs_without_creating_elements() {
    let source = r"function rhs():number{console.log('rhs');return 2;}export function main():void{const a=[1];for(const index of [-1,1.5,NaN,Infinity,1,1024]){if(index!==undefined){try{a[index]|=rhs();}catch(error){console.log(error instanceof TypeError);}}}console.log(JSON.stringify(a));console.log(a.length);}";
    let mut expected = Vec::new();
    for _ in 0..6 {
        expected.extend(["rhs", "1"]);
    }
    expected.extend(["[1]", "1"]);
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
            lines(&expected)
        );
    }
    for profile in [
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        let bytes=varta_compiler::compile_typescript_with_profile("export function main():void{const a:number[]=[1,,3];try{a[1]^=2;}catch(error){console.log(error instanceof TypeError);}console.log(Object.hasOwn(a,1));console.log(JSON.stringify(a));const b:(number|undefined)[]=[undefined];try{b[0]>>>=1;}catch(error){console.log(error instanceof TypeError);}console.log(Object.hasOwn(b,0));}",profile).unwrap();
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
fn bitwise_compounds_resolve_current_array_backing_and_keep_old_profile_fatal_bounds() {
    let out = run(
        r"export function main():void{const a=[1];const shrink=():number=>{a.pop();return 1;};console.log(a[0]|=shrink());console.log(JSON.stringify(a));console.log(Object.hasOwn(a,0));const grow=():number=>{for(let i=0;i<100;i++){a.push(i);}a[0]=99;return 3;};console.log(a[0]^=grow());console.log(a[0]);console.log(a.length);}",
    );
    assert_eq!(out.lines, lines(&["1", "[1]", "1", "2", "2", "101"]));
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
    ] {
        let bytes=varta_compiler::compile_typescript_with_profile("export function main():void{const a=[1];const rhs=():number=>{a.pop();console.log('rhs');return 1;};try{console.log(a[0]|=rhs());}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",profile).unwrap();
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
fn bitwise_compounds_v14_length_keeps_range_validation_after_numeric_conversion() {
    let out = run(
        r"export function main():void{const a=[1,2,3];console.log(a.length>>=1);console.log(JSON.stringify(a));console.log(a['length']>>=-1);console.log(a.length>>>=0);console.log(a.length|=3);console.log(JSON.stringify(a));let returned=99;try{returned=(a.length^=2147483648);}catch(error){console.log(error instanceof RangeError);}console.log(returned);console.log(a.length);console.log(a.length|=NaN);console.log(a.length&=-1);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "1",
            "[1]",
            "0",
            "0",
            "3",
            "[null,null,null]",
            "1",
            "99",
            "3",
            "3",
            "3"
        ])
    );
}

#[test]
fn bitwise_compounds_operand_throws_skip_later_arguments_and_keep_finalizers() {
    let out = run(
        r"function source():{n:number}{console.log('source');throw TypeError('source');}function key():string{console.log('key');throw RangeError('key');}function rhs():number{console.log('rhs');throw SyntaxError('rhs');}function later():number{console.log('BAD_LATER');return 1;}function take(value:number,next:number):void{console.log('BAD_BODY');}export function main():void{const value={n:1};try{take(source()[key()]|=rhs(),later());}catch({message}){console.log(message);}try{take(value[key()]<<=rhs(),later());}catch({message}){console.log(message);}try{take(value.n>>>=rhs(),later());}catch({message}){console.log(message);}finally{console.log('final');}console.log(value.n);}",
    );
    assert_eq!(
        out.lines,
        lines(&["source", "source", "key", "key", "rhs", "rhs", "final", "1"])
    );
}

#[test]
fn bitwise_compounds_gc_retains_prepared_inputs_old_graphs_pairs_and_saved_returns() {
    let source = r"function churn():number{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}return 2;}function keep(first:{text:string},result:VartaResult<string>,value:number,last:number):string{churn();console.log(result.status);console.log(value);return first.text;}function work(value:{n?:number}):number{try{return value.n>>>=1;}finally{churn();}}export function main():void{let value={n:5};let key='n'+''.repeat(1);const rhs=():number=>{value={n:0};key='missing';churn();return 2;};console.log(keep({text:'earlier'+'!'},varta.result.capture(varta.env.get('VARTA_BITWISE_COMPOUND_GC')),value[key]|=rhs(),churn()));console.log(work({n:-1}));const graph:{v:number|{text:string}}={v:{text:'detached'+'!'}};const replace=():number=>{graph.v=1;churn();return 2;};try{graph.v|=replace();}catch(error){console.log(error instanceof TypeError);}console.log(graph.v);const count:{n:number|undefined}={n:0};for(let i=0;i<4000;i++){count.n^=i;}churn();console.log(count.n);}";
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
            "<capability denied: env> (VARTA_BITWISE_COMPOUND_GC)",
            "1",
            "7",
            "earlier!",
            "2147483647",
            "1",
            "1",
            "0"
        ])
    );
}

#[test]
fn bitwise_compounds_helper_purity_controls_pass_and_source_key_rhs_effects_are_visible() {
    use crate::contract_tests::{export_index, rewrite};
    for (source, key, rhs, has_host) in [
        ("return {n:1};", "return 'n';", "return 2;", false),
        (
            "console.log('source');return {n:1};",
            "return 'n';",
            "return 2;",
            true,
        ),
        (
            "return {n:1};",
            "console.log('key');return 'n';",
            "return 2;",
            true,
        ),
        (
            "return {n:1};",
            "return 'n';",
            "console.log('rhs');return 2;",
            true,
        ),
    ] {
        let bytes = compile(&format!(
            "function source():{{n:number}}{{{source}}}function key():string{{{key}}}function rhs():number{{{rhs}}}function fake(value:boolean):boolean{{const result=(source()[key()]|=rhs());return value;}}export function main():void{{varta.json.parse<number>('1');}}"
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
fn bitwise_compounds_reject_invalid_targets_types_and_keep_fatal_exhaustion_domains() {
    for source in [
        "export function main():void{const n=1;n|=2;}",
        "export function main():void{n&=2;let n=1;}",
        "export function main():void{let n:number|undefined=1;if(n!==undefined){n^=2;}}",
        "export function main():void{let n='1';n|=2;}",
        "export function main():void{let n=1;n|='2';}",
        "export function main():void{let n=1;n<<=false;}",
        "export function main():void{const a=[1];a['0']>>=2;}",
        "export function main():void{const value={n:1};const key=0;value[key]|=2;}",
        "export function main():void{const value={n:1};const key:string|undefined='n';value[key]|=2;}",
        "export function main():void{let fn:()=>number=()=>1;fn|=2;}",
        "export function main():void{const error=Error('x');error.message|=2;}",
        "export function main():void{let n=1;n**=2;}",
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
                "export function main():void{let n=1;n|=2;}",
                profile
            )
            .is_err()
        );
    }
    assert!(
        varta_compiler::compile_typescript_with_profile(
            "export function main():void{const a=[1];a.length|=2;}",
            LanguageProfile::CheckedF64Utf16V13
        )
        .is_err()
    );
    for source in [
        "function rhs():number{while(true){}return 2;}export function main():void{const value:{n?:number}={};try{console.log(value.n|=rhs());}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
        "function key():string{while(true){}return 'n';}export function main():void{const value={n:1};try{console.log(value[key()]<<=1);}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
        "export function main():void{const a=[1];try{console.log(a.length<<=11);}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
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
