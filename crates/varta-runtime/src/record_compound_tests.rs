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
fn record_compounds_execute_all_operators_and_infer_computed_result_types() {
    let source = r"function double(value:{n:number,s:string},key:string):number{return value[key]*=2;}
        function add(value:{n:number,s:string},key:string,rhs:number|string):number|string{return value[key]+=rhs;}
        function narrowed(value:{n:number}|undefined,key:string|undefined):number{if(value!==undefined){if(key!==undefined){return value[key]+=1;}}return 0;}
        export function main():void{const value={n:5.5,s:'a'};let key='n';console.log(value[key]+=2);console.log(value[key]-=1);console.log(double(value,key));console.log(value[key]/=2);console.log(value[key]%=2);key='s';console.log(add(value,key,'b'));console.log(JSON.stringify(value));const nums={n:-0};console.log(nums['n'+'']*=1);console.log(nums['n'+'']/=0);console.log(nums['n'+'']+=Infinity);console.log(narrowed({n:4},'n'));console.log(narrowed(undefined,'n'));console.log(narrowed({n:4},undefined));}";
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
                "7.5",
                "6.5",
                "13",
                "6.5",
                "0.5",
                "ab",
                "{\"n\":0.5,\"s\":\"ab\"}",
                "-0",
                "NaN",
                "NaN",
                "5",
                "0",
                "0"
            ]),
            "{profile:?}"
        );
    }
}

#[test]
fn record_compounds_prepare_source_key_and_old_value_once_before_rhs() {
    let out = run(
        r"function source(value:{n:number,s:string}):{n:number,s:string}{console.log('source');return value;}
        export function main():void{let value={n:1,s:'old'};const original=value;let key='n';const select=():string=>{console.log('key');original.n=5;value={n:100,s:'new'};return key;};const rhs=():number=>{console.log('rhs');original.n=99;key='s';return 2;};console.log(source(value)[select()]+=rhs());console.log(JSON.stringify(original));console.log(JSON.stringify(value));console.log(key);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "source",
            "key",
            "rhs",
            "7",
            "{\"n\":7,\"s\":\"old\"}",
            "{\"n\":100,\"s\":\"new\"}",
            "s"
        ])
    );
}

#[test]
fn record_compounds_check_operand_kinds_after_rhs_without_coercion() {
    let out = run(
        r"function apply(value:{n:number,s:string,b:boolean,r:{x:number},a:number[],e:TypeError},key:string,rhs:number|string|boolean):number|string{return value[key]+=rhs;}
        export function main():void{const value={n:1,s:'a',b:true,r:{x:2},a:[3],e:TypeError('kept')};const bad=():number=>{console.log('rhs');return 2;};for(const key of ['b','r','a','e','s']){if(key!==undefined){try{apply(value,key,bad());}catch(error){console.log(error instanceof TypeError);}}}try{apply(value,'n',false);}catch(error){console.log(error instanceof TypeError);}console.log(JSON.stringify(value));console.log(value.e.message);console.log(apply(value,'n',2));console.log(apply(value,'s','b'));}",
    );
    let mut expected = Vec::new();
    for _ in 0..5 {
        expected.extend(["rhs", "1"]);
    }
    expected.extend([
        "1",
        "{\"n\":1,\"s\":\"a\",\"b\":true,\"r\":{\"x\":2},\"a\":[3],\"e\":{}}",
        "kept",
        "3",
        "ab",
    ]);
    assert_eq!(out.lines, lines(&expected));
}

#[test]
fn record_compounds_missing_values_keep_rhs_effects_and_original_exceptions() {
    let out = run(
        r"function fail():number{console.log('throw rhs');throw RangeError('original');}
        export function main():void{const value:{a?:number,n:number}={n:1};const fill=():number=>{console.log('fill');value.a=10;return 2;};try{value['a'+'']+=fill();}catch(error){console.log(error instanceof TypeError);}console.log(value.a);value.a=undefined;try{value['a'+'']+=fail();}catch({name,message}){console.log(name);console.log(message);}for(const key of ['missing','\ud800','a\u0000']){if(key!==undefined){try{value[key]+=2;}catch(error){console.log(error instanceof TypeError);}console.log(Object.hasOwn(value,key));}}console.log(JSON.stringify(Object.keys(value)));console.log(JSON.stringify(value));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "fill",
            "1",
            "10",
            "throw rhs",
            "RangeError",
            "original",
            "1",
            "0",
            "1",
            "0",
            "1",
            "0",
            "[\"n\",\"a\"]",
            "{\"n\":1}"
        ])
    );
}

#[test]
fn record_compounds_inherited_reads_fail_before_rhs_and_keep_declared_own_names() {
    let out = run(
        r"function rhs():number{console.log('rhs');return 2;}export function main():void{const value={n:1};for(const key of ['constructor','__proto__','toString']){if(key!==undefined){try{value[key]+=rhs();}catch(error){console.log(error instanceof TypeError);}}}const own={constructor:1,toString:2,valueOf:3};for(const key of ['constructor','toString','valueOf']){if(key!==undefined){console.log(own[key]+=rhs());}}console.log(JSON.stringify(own));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "1",
            "1",
            "1",
            "rhs",
            "3",
            "rhs",
            "4",
            "rhs",
            "5",
            "{\"constructor\":3,\"toString\":4,\"valueOf\":5}"
        ])
    );
}

#[test]
fn record_compounds_nested_mixed_chains_keep_snapshots_and_partial_stores() {
    let out = run(
        r"function nested(value:{n:number,s:string},key:string):void{console.log(value[key]+=value[key]+=2);console.log(value.n);}function mixed(value:{n:number,s:string},key:string):void{const array=[4];let local=1;console.log(local+=value[key]*=array[0]+=3);console.log(JSON.stringify(value));console.log(JSON.stringify(array));}function strings(value:{n:number,s:string}):void{const key='s';console.log(value[key]+=value[key]+='b');}function partial(value:{n:number,s:string},key:string):void{const absent:{n?:number}={};try{absent[key]+=value[key]+=1;}catch(error){console.log(error instanceof TypeError);}console.log(value.n);console.log(Object.hasOwn(absent,key));try{value[key]+=absent[key]+=1;}catch(error){console.log(error instanceof TypeError);}console.log(value.n);}export function main():void{const value={n:1,s:'a'};nested(value,'n');mixed(value,'n');strings(value);partial(value,'n');}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "4",
            "4",
            "29",
            "{\"n\":28,\"s\":\"a\"}",
            "[7]",
            "aab",
            "1",
            "29",
            "0",
            "1",
            "29"
        ])
    );
}

#[test]
fn record_compounds_exact_utf16_keys_and_all_field_bits_preserve_presence_order() {
    let out = run(
        r"export function main():void{const value={'\ue000':1,'𐀀':2,'😀':3,'':4,'a\u0000b':5,'é':6,'2':7,'01':8};console.log(JSON.stringify(['\ue000','𐀀','😀','','a\u0000b','é','2','01'].map(key=>value[key]+=1)));console.log(JSON.stringify(Object.keys(value)));const text={s:'A😀'};console.log(JSON.stringify(text['s'+'']+='\ud800\u0000'));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "[2,3,4,5,6,7,8,9]",
            "[\"2\",\"\",\"𐀀\",\"😀\",\"\",\"a\\u0000b\",\"é\",\"01\"]",
            "\"A😀\\ud800\\u0000\""
        ])
    );
    let fields = (0..32)
        .map(|i| format!("f{i}:{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let out = run(&format!(
        "export function main():void{{const value={{{fields}}};const key='f31';console.log(value[key]+=2);console.log(value.f31);console.log(Object.hasOwn(value,key));console.log(Object.keys(value).length);}}"
    ));
    assert_eq!(out.lines, lines(&["33", "33", "1", "32"]));
}

#[test]
fn record_compounds_throws_stop_later_operands_and_preserve_callback_finalizers() {
    let out = run(
        r"function source():{n:number}{console.log('source');throw TypeError('source failure');}function key():string{console.log('key');throw RangeError('key failure');}function rhs():number{console.log('rhs');throw SyntaxError('rhs failure');}function later():number{console.log('BAD_LATER');return 1;}function consume(value:number,other:number):void{console.log('BAD_BODY');}
        export function main():void{const value={n:1};try{consume(source()[key()]+=rhs(),later());}catch({message}){console.log(message);}finally{console.log('source final');}try{consume(value[key()]+=rhs(),later());}catch({message}){console.log(message);}try{consume(value['n'+'']+=rhs(),later());}catch({message}){console.log(message);}console.log(JSON.stringify(['n','n'].map(name=>value[name]+=1)));console.log(value.n);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "source",
            "source failure",
            "source final",
            "key",
            "key failure",
            "rhs",
            "rhs failure",
            "[2,3]",
            "3"
        ])
    );
}

#[test]
fn record_compounds_reject_unsupported_keys_types_and_legacy_profiles() {
    for source in [
        "export function main():void{const value={n:1};const key=1;value[key]+=2;}",
        "export function main():void{const value={n:1};const key:string|undefined='n';value[key]+=2;}",
        "export function main():void{const value:{n:number}|undefined={n:1};const key='n';value[key]+=2;}",
        "export function main():void{const value={s:'x'};const key='s';value[key]-=2;}",
        "export function main():void{const value={n:1};const key='n';value[key]+='x';}",
        "export function main():void{const value={a:{n:1},b:{s:'x'},n:1};const key='n';value[key]+=2;}",
        "export function main():void{const value={a:[1],b:['x'],n:1};const key='n';value[key]+=2;}",
        "export function main():void{const value={f:():number=>1,n:1};const key='n';value[key]+=2;}",
        "export function main():void{const value={n:1};const key='n';value[key]**=2;}",
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
                "export function main():void{const value={n:1};const key='n';value[key]+=2;}",
                profile
            )
            .is_err()
        );
    }
}

#[test]
fn record_compounds_gc_retains_prepared_reference_old_values_and_saved_pairs() {
    let source = r"function churn():number{let text='x';for(let i=0;i<14;i+=1){text=text+text;}for(let i=0;i<260;i+=1){const garbage=[text+'!',text+'?'];garbage.slice();}return 1;}
        function keep(first:{text:string},result:VartaResult<string>,value:string,after:number):string{churn();console.log(result.value);console.log(result.status);return first.text+value;}
        function work():string{let current={s:'old'+'!'};const rhs=():string=>{current={s:'new'};churn();return 'rhs'+'!';};try{return current['s'+'']+=rhs();}finally{churn();}}
        export function main():void{let current={s:'field'+'!'};let key='s'+''.repeat(1);const rhs=():string=>{current={s:'changed'};key='missing';churn();return 'append'+'!';};console.log(keep({text:'earlier'+'!'},varta.result.capture(varta.env.get('VARTA_RECORD_COMPOUND_GC')),current[key]+=rhs(),churn()));console.log(work());const broad:{v:number|{text:string}}={v:{text:'detached'+'!'}};const replace=():number=>{broad.v=1;churn();return 2;};try{broad['v'+'']+=replace();}catch(error){console.log(error instanceof TypeError);}console.log(broad.v);const count={n:0};for(let i=0;i<4000;i+=1){count['n'+'']+=1;}churn();console.log(count.n);}";
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
            "<capability denied: env> (VARTA_RECORD_COMPOUND_GC)",
            "",
            "1",
            "earlier!field!append!",
            "old!rhs!",
            "1",
            "1",
            "4000"
        ])
    );
}

#[test]
fn record_compounds_helper_validation_visits_every_operand_effect() {
    use crate::contract_tests::{export_index, rewrite};
    for (source, key, rhs) in [
        (
            "console.log('source');return {n:1};",
            "return 'n';",
            "return 2;",
        ),
        (
            "return {n:1};",
            "console.log('key');return 'n';",
            "return 2;",
        ),
        (
            "return {n:1};",
            "return 'n';",
            "console.log('rhs');return 2;",
        ),
    ] {
        let bytes = compile(&format!(
            "function source():{{n:number}}{{{source}}}function key():string{{{key}}}function rhs():number{{{rhs}}}function fake(value:boolean):boolean{{const result=(source()[key()]+=rhs());return value;}}export function main():void{{varta.json.parse<number>('1');}}"
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
fn record_compounds_resource_limits_remain_fatal_before_any_requested_store() {
    for (key, rhs) in [
        ("while(true){}return 'n';", "return 2;"),
        ("return 'n';", "while(true){}return 2;"),
    ] {
        let bytes = compile(&format!(
            "function key():string{{{key}}}function rhs():number{{{rhs}}}export function main():void{{const value={{n:1}};try{{console.log(value[key()]+=rhs());}}catch(error){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"
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
        "export function main():void{{const value={{s:'x'.repeat({units})}};console.log('before');try{{value['s'+'']+='x';}}catch(error){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"
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
}
