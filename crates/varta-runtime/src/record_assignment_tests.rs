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
fn record_assignment_values_keep_exact_rhs_types_bits_and_literal_checks() {
    let source = r"function dynamic(value:{n?:number},name:string):number{return value[name]=-0;}
        function literal(value:{n?:number}):number{return value['n']=2.5;}
        export function main():void{
            const value:{n?:number}={};console.log(1/dynamic(value,'n'));console.log(literal(value)+1);
            const n:number=(value['n'+ '']=3);console.log(n+1);const missing:undefined=(value['n']=undefined);console.log(missing);console.log(Object.hasOwn(value,'n'));
            const named={'0':1};console.log((named[0]=7)+1);
            const primitive:{b?:boolean;z?:null}={};console.log(!(primitive['b']=true));const z:null=(primitive['z']=null);console.log(JSON.stringify(z));
            const errors:{e?:Error}={};const e:TypeError=(errors['e']=TypeError('exact'));console.log(e.message);console.log(errors['e'+ '']===e);
        }";
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
                "-Infinity",
                "3.5",
                "4",
                "undefined",
                "1",
                "8",
                "0",
                "null",
                "exact",
                "1"
            ]),
            "{profile:?}"
        );
    }
}

#[test]
fn record_assignment_values_return_original_union_without_narrowing_to_selected_field() {
    let out = run(
        r"function put(value:{n:number;s:string},name:string,item:number|string):number|string{return value[name]=item;}
        export function main():void{const value={n:1,s:'old'};const n=put(value,'n',2.5);const s=put(value,'s','new');console.log(typeof n);console.log(typeof s);console.log(JSON.stringify([n,s]));console.log(JSON.stringify(value));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "number",
            "string",
            "[2.5,\"new\"]",
            "{\"n\":2.5,\"s\":\"new\"}"
        ])
    );
}

#[test]
fn record_assignment_values_preserve_reference_identity_and_aggregate_context() {
    let out = run(r"export function main():void{
        const target:{child?:{n:number};items?:number[]}={};const child={n:1};const name='child';const result:{n:number}=(target[name]=child);console.log(result===child);result.n=9;console.log(JSON.stringify(target));
        const a:number[]=(target['items']=[]);a.push(2);console.log(JSON.stringify(target));console.log(target['items'+ '']===a);
        const c=(target['chi'+'ld']={n:7});console.log(c.n);console.log(c===target.child);
        const optional:{child?:{n?:number}}={};const d=(optional['child']={});d.n=3;console.log(JSON.stringify(optional));
    }");
    assert_eq!(
        out.lines,
        lines(&[
            "1",
            "{\"child\":{\"n\":9}}",
            "{\"child\":{\"n\":9},\"items\":[2]}",
            "1",
            "7",
            "1",
            "{\"child\":{\"n\":3}}"
        ])
    );
}

#[test]
fn record_assignment_values_prepare_chained_receivers_and_keys_before_single_rhs() {
    let out = run(
        r"function outer():{n:number}{console.log('outer source');return {n:0};}
        function inner():{n:number}{console.log('inner source');return {n:0};}
        function key(label:string):string{console.log(label);return 'n';}function rhs():number{console.log('rhs');return 7;}
        export function main():void{
            console.log(outer()[key('outer key')]=inner()[key('inner key')]=rhs());
            const a={n:1};const b={n:2};console.log(a['n'+ '']=b['n']=3);console.log(JSON.stringify([a.n,b.n]));
            let source={n:4};const original=source;let name='n';
            const right=():number=>{source={n:10};name='unknown';return 5;};
            const result:number=(source[name]=right());console.log(result);console.log(original.n);console.log(source.n);
        }",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "outer source",
            "outer key",
            "inner source",
            "inner key",
            "rhs",
            "7",
            "3",
            "[3,3]",
            "5",
            "5",
            "10"
        ])
    );
}

#[test]
fn record_assignment_values_work_in_receivers_keys_callbacks_and_short_circuits() {
    let out = run(
        r"function sum(value:number,next:number):number{return value+next;}function later():number{console.log('later');return 4;}
        export function main():void{
            const value={n:0};console.log(sum((value['n'+ '']=2),later()));console.log((value['n']=3)+1);
            const keys={name:'old'};console.log(value[(keys['name']='n')]=5);console.log(keys.name);
            const holder={child:{n:0}};const child={n:1};console.log((holder['child']=child)['n']=6);console.log(child.n);
            console.log(false&&((value['n']=99)===99));console.log(value.n);
            console.log(true?(value['n']=7):(value['n']=8));console.log(JSON.stringify([1,2].map(item=>{return value['n'+ '']=item;})));console.log(value.n);
            let total=0;for(let i=0;i<3;i=i+1){total=total+(value['n'+ '']=i);}console.log(total);
        }",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "later", "6", "4", "5", "n", "6", "6", "0", "5", "7", "[1,2]", "2", "3"
        ])
    );
}

#[test]
fn record_assignment_values_stop_outer_stores_and_later_arguments_after_inner_failure() {
    let out = run(
        r"function take(value:number,next:number):void{console.log('BAD_BODY');}function later():number{console.log('BAD_LATER');return 0;}
        export function main():void{
            const outer={n:0};const inner={n:0,s:'old'};
            try{take((outer['n'+ '']=inner['s'+ '']=3),later());}catch(error){console.log(error instanceof TypeError);}finally{console.log('finally');}
            console.log(JSON.stringify(outer));console.log(JSON.stringify(inner));
            try{take((outer['missing'+ '']=inner['n'+ '']=4),later());}catch({message}){console.log(message);}
            console.log(outer.n);console.log(inner.n);
            const fail=():number=>{inner.n=8;throw RangeError('rhs');};
            try{take((outer['n'+ '']=inner['n'+ '']=fail()),later());}catch({message}){console.log(message);}console.log(outer.n);console.log(inner.n);
        }",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "1",
            "finally",
            "{\"n\":0}",
            "{\"n\":0,\"s\":\"old\"}",
            "Unknown computed record write key",
            "0",
            "4",
            "rhs",
            "0",
            "8"
        ])
    );
}

#[test]
fn record_assignment_values_compose_with_destructuring_results_and_saved_returns() {
    let out = run(
        r"function saved(value:{n:number}):number{try{return value['n'+ '']=2;}finally{value.n=9;}}
        export function main():void{let n=0;const target={child:{n:0}};const original={n:3};const result=({n}=target['child']=original);console.log(result===original);console.log(n);const assigned=(target['child']=({n}=original));console.log(assigned===original);const value={n:0};console.log(saved(value));console.log(value.n);console.log(JSON.stringify([1,2].map(item=>{try{return value['n'+ '']=item;}finally{value.n=10;}})));console.log(value.n);}",
    );
    assert_eq!(out.lines, lines(&["1", "3", "1", "2", "9", "[1,2]", "10"]));
}

#[test]
fn record_assignment_values_reject_unsupported_targets_bad_results_and_old_profiles() {
    for source in [
        "export function main():void{const n=0;const result=(n=2);}",
        "export function main():void{const value={n:1};const result=(value.n='wrong');}",
        "export function main():void{const value=[1];const result=(value[0]='wrong');}",
        "export function main():void{const value={n:1};const key='n';const result=(value[key]**=2);}",
        "export function main():void{const value={n:1};const key=1;const result=(value[key]=2);}",
        "export function main():void{const value:{n:number}|null={n:1};const key='n';const result=(value[key]=2);}",
        "export function main():void{const value={n:1};const key:string|undefined='n';const result=(value[key]=2);}",
        "export function main():void{const value={n:1};const result=(value['missing']=2);}",
        "export function main():void{const value={n:1};const result=(value['n']='wrong');}",
        "export function main():void{const value={n:1};const result=(value['__proto__']=2);}",
        "function put(value:{n:number;s:string},name:string,item:number|string):number{return value[name]=item;}export function main():void{}",
        "export function main():void{const value={n:1};const name='n';const result:string=(value[name]=2);}",
        "export function main():void{const value={n:1};const result=(value['n']=()=>2);}",
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
        for key in ["'n'", "name"] {
            let source = format!(
                "export function main():void{{const value={{n:1}};const name='n';const result=(value[{key}]=2);}}"
            );
            assert!(varta_compiler::compile_typescript_with_profile(&source, profile).is_err());
        }
    }
}

#[test]
fn record_assignment_values_keep_all_slots_and_exact_number_results_at_default_limits() {
    let fields = (0..varta_common::value::MAX_RECORD_FIELDS)
        .map(|i| format!("f{i:02}?:number"))
        .collect::<Vec<_>>()
        .join(";");
    let names = (0..varta_common::value::MAX_RECORD_FIELDS)
        .map(|i| format!("'f{i:02}'"))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        "function put(value:{{{fields}}},name:string,item:number):number{{return value[name]=item;}}export function main():void{{const value:{{{fields}}}={{}};const keys=[{names}];const results=keys.map((name,index)=>{{return put(value,name,index);}});console.log(results.length);console.log(results[31]);console.log(value.f31);console.log(Object.keys(value).length);console.log(value['f31']=undefined);console.log(Object.hasOwn(value,'f31'));}}"
    );
    assert_eq!(
        run(&source).lines,
        lines(&["32", "31", "31", "32", "undefined", "1"])
    );
}

#[test]
fn record_assignment_values_gc_keeps_nested_floors_earlier_operands_pairs_and_results() {
    let source = r"function churn():number{let text='x';for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+'!',text+'?'];garbage.slice();}return 1;}
        function source():{text?:string}{churn();return {};}
        function name():string{churn();return 'te'+'xt';}function rhs():string{churn();return 'owned'+'!';}
        function keep(first:{text:string},captured:VartaResult<string>,value:string,after:number):string{churn();console.log(captured.status);return first.text+value;}
        function saved():string{try{return source()[name()]=rhs();}finally{churn();}}
        function pair():VartaResult<string>{try{return varta.result.capture(varta.env.get('VARTA_RECORD_ASSIGN_PAIR'));}finally{console.log(source()[name()]=rhs());churn();}}
        function make():{child?:{text:string}}{return {};}
        function child():{text:string}{churn();return {text:'child'+'!'};}
        export function main():void{
            console.log(keep({text:'earlier'+'!'},varta.result.capture(varta.env.get('VARTA_RECORD_ASSIGN')),source()[name()]=source()['text']=rhs(),churn()));
            console.log(saved());const p=pair();console.log(p.status);console.log(p.value);
            console.log(('prefix'+'!')+(source()[name()]=rhs()));
            const graph=(make()['child']=child());churn();console.log(graph.text);
            const value:{text?:string}={};let result='';for(let i=0;i<7000;i=i+1){result=(value['te'+'xt']='retained'+'!');}
            churn();console.log(result);console.log(value.text);
            const inner={n:0};const outer={n:0};for(let i=0;i<1000;i=i+1){try{console.log(outer['missing'+ '']=inner['n'+ '']=i);}catch(error){}}
            churn();console.log(inner.n);console.log(outer.n);
        }";
    let bytes = compile(source);
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
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
            "<capability denied: env> (VARTA_RECORD_ASSIGN)",
            "1",
            "earlier!owned!",
            "owned!",
            "<capability denied: env> (VARTA_RECORD_ASSIGN_PAIR)",
            "owned!",
            "1",
            "",
            "prefix!owned!",
            "child!",
            "retained!",
            "retained!",
            "999",
            "0"
        ])
    );
}

#[test]
fn record_assignment_values_source_and_key_throws_skip_rhs_and_enclosing_calls() {
    let out = run(
        r"function source():{n:number}{console.log('source');throw TypeError('source failure');}
        function key():string{console.log('key');throw RangeError('key failure');}
        function rhs():number{console.log('BAD_RHS');return 2;}
        function take(value:number,next:number):void{console.log('BAD_BODY');}function later():number{console.log('BAD_LATER');return 1;}
        export function main():void{
            const value={n:0};try{take((source()[key()]=rhs()),later());}catch({message}){console.log(message);}
            try{take((value[key()]=rhs()),later());}catch({message}){console.log(message);}finally{console.log('finally');}console.log(value.n);
        }",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "source",
            "source failure",
            "key",
            "key failure",
            "finally",
            "0"
        ])
    );
}

#[test]
fn record_assignment_values_helper_validation_visits_prepared_operands_and_rhs() {
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
        for target in ["key()", "'n'"] {
            let bytes = compile(&format!(
                "function source():{{n:number}}{{{source}}}function key():string{{{key}}}function rhs():number{{{rhs}}}function fake(value:boolean):boolean{{const n=(source()[{target}]=rhs());return value;}}export function main():void{{varta.json.parse<number>('1');}}"
            ));
            // The key-effect case applies only to the computed-key path.
            if target == "'n'" && key.contains("console.log") {
                continue;
            }
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
}

#[test]
fn record_assignment_values_fatal_rhs_and_key_limits_bypass_later_work_and_handlers() {
    for expression in ["value[foreverKey()]=2", "value['n']=foreverRhs()"] {
        let bytes = compile(&format!(
            "function foreverKey():string{{while(true){{}}return 'n';}}function foreverRhs():number{{while(true){{}}return 2;}}function take(value:number,next:number):void{{console.log('BAD_BODY');}}function later():number{{console.log('BAD_LATER');return 1;}}export function main():void{{const value={{n:1}};try{{take(({expression}),later());}}catch(error){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"
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
