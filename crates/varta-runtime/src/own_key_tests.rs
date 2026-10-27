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
fn dynamic_own_keys_distinguish_presence_from_payload_and_do_not_change_order() {
    let source = "function own(value:{a?:number;b:number},key:string):boolean{return Object.hasOwn(value,key);}export function main():void{const value:{a?:number;b:number}={b:1};console.log(own(value,'a'));console.log(own(value,'b'));console.log(own(value,'missing'));value.a=undefined;console.log(own(value,'a'));value.a=2;console.log(own(value,'a'));console.log(JSON.stringify(Object.keys(value)));console.log(JSON.stringify(Object.values(value)));}";
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
            lines(&["0", "1", "0", "1", "1", "[\"b\",\"a\"]", "[1,2]"]),
            "{profile:?}"
        );
    }
}
#[test]
fn dynamic_own_keys_preserve_exact_utf16_and_only_match_declared_own_names() {
    let out = run(
        "function key(text:string):string{return text;}export function main():void{const value={'Привіт':1,'😀':2,'a\\u0000b':3,'':4,'2':5,'01':6,constructor:7};console.log(Object.hasOwn(value,key('Привіт')));console.log(Object.hasOwn(value,key('😀')));console.log(Object.hasOwn(value,key('a\\u0000b')));console.log(Object.hasOwn(value,key('')));console.log(Object.hasOwn(value,key('2')));console.log(Object.hasOwn(value,key('01')));console.log(Object.hasOwn(value,key('constructor')));console.log(Object.hasOwn(value,key('toString')));console.log(Object.hasOwn(value,key('__proto__')));console.log(Object.hasOwn(value,key('\\ud800')));console.log(Object.hasOwn(value,'\\ud800'));console.log(Object.hasOwn(value,'__proto__'));console.log(Object.hasOwn(value,key('привіт')));const other={'é':1};console.log(Object.hasOwn(other,key('é')));console.log(Object.hasOwn(value,2));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "1", "1", "1", "1", "1", "1", "1", "0", "0", "0", "0", "0", "0", "0", "1"
        ])
    );
}
#[test]
fn dynamic_own_keys_evaluate_source_and_key_once_in_order_even_for_empty_shapes() {
    let out = run(
        "function source():{n:number}{console.log('source');return {n:1};}function key():string{console.log('key');return 'n';}function empty():{}{console.log('empty');return {};}export function main():void{console.log(Object.hasOwn(source(),key()));console.log(Object.hasOwn(empty(),key()));console.log(Object.hasOwn(source(),key()+'missing'));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "source", "key", "1", "empty", "key", "0", "source", "key", "0"
        ])
    );
}
#[test]
fn dynamic_own_keys_read_current_presence_after_key_effects_on_the_original_receiver() {
    let out = run(
        "export function main():void{let value:{a?:number}={};const original=value;const name=():string=>{value.a=undefined;value={};return 'a';};console.log(Object.hasOwn(value,name()));console.log(Object.hasOwn(value,'a'));console.log(Object.hasOwn(original,'a'));const check=(key:string):boolean=>Object.hasOwn(original,key);console.log(check('a'));}",
    );
    assert_eq!(out.lines, lines(&["1", "0", "1", "1"]));
}
#[test]
fn dynamic_own_keys_key_throws_skip_later_arguments_and_run_finalizers() {
    let out = run(
        "function source():{n:number}{console.log('source');return {n:1};}function bad():string{console.log('key');throw TypeError('key failure');}function later():number{console.log('BAD_LATER');return 0;}function consume(own:boolean,value:number):void{console.log('BAD_BODY');}export function main():void{try{consume(Object.hasOwn(source(),bad()),later());}catch({message}){console.log(message);}finally{console.log('finally');}} ",
    );
    assert_eq!(
        out.lines,
        lines(&["source", "key", "key failure", "finally"])
    );
}
#[test]
fn dynamic_own_keys_support_typed_json_rest_narrowing_and_callback_inference() {
    let out = run(
        "export function main():void{const value=varta.json.parse<{a?:number;b:number}>('{\"b\":1}');const {b,...rest}=value;console.log(JSON.stringify(['a','b'].map(key=>Object.hasOwn(value,key))));console.log(JSON.stringify(['a','b'].map(key=>Object.hasOwn(rest,key))));const key:string|undefined='b';if(key!==undefined){console.log(Object.hasOwn(value,key));}const source:{b:number}|null={b:1};if(source!==null){const key='b';console.log(Object.hasOwn(source,key));}} ",
    );
    assert_eq!(
        out.lines,
        lines(&["[false,true]", "[false,false]", "1", "1"])
    );
}
#[test]
fn dynamic_own_keys_cover_all_presence_bits_under_default_budgets() {
    let fields = (0..varta_common::value::MAX_RECORD_FIELDS)
        .map(|i| format!("f{i:02}?:number"))
        .collect::<Vec<_>>()
        .join(";");
    let source = format!(
        "export function main():void{{const value:{{{fields}}}={{f31:31,f00:0}};const keys=['f00','f30','f31','missing'];console.log(JSON.stringify(keys.map(key=>Object.hasOwn(value,key))));let count=0;for(let i=0;i<100;i=i+1){{for(const key of keys){{if(key!==undefined){{if(Object.hasOwn(value,key)){{count=count+1;}}}}}}}}console.log(count);}}"
    );
    assert_eq!(
        run(&source).lines,
        lines(&["[true,false,true,false]", "200"])
    );
}
#[test]
fn dynamic_own_keys_reject_unchecked_kinds_shadowing_and_old_profiles() {
    for source in [
        "export function main():void{const key=1;Object.hasOwn({n:1},key);}",
        "export function main():void{const key:string|undefined='n';Object.hasOwn({n:1},key);}",
        "export function main():void{Object.hasOwn({n:1},true);}",
        "export function main():void{const value:{n:number}|null={n:1};const key='n';Object.hasOwn(value,key);}",
        "export function main():void{const Object={hasOwn:1};const key='n';Object.hasOwn({n:1},key);}",
        "export function main():void{const source={n:1};const key=Object.hasOwn(source,key);}",
        "export function main():void{Object.hasOwn<number>({n:1},'n');}",
        "export function main():void{const key='n';Object.hasOwn(...[{n:1}],key);}",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V14,
        )
        .unwrap_err();
        assert!(error.diagnostic().is_some(), "{source}: {error}");
    }
    for profile in [
        LanguageProfile::CheckedF64Utf16V3,
        LanguageProfile::CheckedF64Utf16V11,
    ] {
        let source = "export function main():void{const key='n';Object.hasOwn({n:1},key);}";
        assert!(varta_compiler::compile_typescript_with_profile(source, profile).is_err());
        let literal = varta_compiler::compile_typescript_with_profile(
            "export function main():void{console.log(Object.hasOwn({n:1},'n'));}",
            profile,
        )
        .unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&literal, &[])
                .unwrap()
                .lines,
            lines(&["1"])
        );
    }
}
fn run_with_gc_budget(source: &str) -> RunOutput {
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
    std::mem::take(&mut execution.store.data_mut().output)
}
#[test]
fn dynamic_own_keys_gc_keeps_receivers_keys_earlier_operands_pairs_and_saved_booleans() {
    let out = run_with_gc_budget(
        "function churn():void{let text='x';for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+'!',text+'?'];garbage.slice();}}function source():{nested:{text:string};maybe?:number}{return {nested:{text:'owned'+'!'}};}function key():string{churn();return 'nest'+'ed';}function keep(first:{text:string},pair:VartaResult<string>,own:boolean):boolean{churn();console.log(first.text);console.log(pair.status);return own;}function saved():boolean{try{return Object.hasOwn(source(),key());}finally{churn();}}export function main():void{console.log(keep({text:'earlier'+'!'},varta.result.capture(varta.env.get('VARTA_OWN_KEY_GC')),Object.hasOwn(source(),key())));console.log(saved());const object:{alpha?:string}={alpha:'value'+'!'};let count=0;for(let i=0;i<7000;i=i+1){if(Object.hasOwn(object,'al'+'pha')){count=count+1;}}churn();console.log(count);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "<capability denied: env> (VARTA_OWN_KEY_GC)",
            "earlier!",
            "1",
            "1",
            "1",
            "7000"
        ])
    );
}
#[test]
fn dynamic_own_keys_do_not_read_or_serialize_payloads_or_coerce_unknown_keys() {
    let out = run(
        "function key(text:string):string{return text;}export function main():void{const value={error:TypeError('opaque'),number:NaN,array:[1,2],text:'\\ud800'};console.log(Object.hasOwn(value,key('error')));console.log(Object.hasOwn(value,key('number')));console.log(Object.hasOwn(value,key('array')));console.log(Object.hasOwn(value,key('text')));console.log(Object.hasOwn(value,key('a'.repeat(1024))));console.log(Object.hasOwn(value,'a'.repeat(1024)));}",
    );
    assert_eq!(out.lines, lines(&["1", "1", "1", "1", "0", "0"]));
}
#[test]
fn dynamic_own_keys_fatal_key_limits_bypass_catch_and_finally() {
    for receiver in ["{n:1}", "[1]", "Error('x')"] {
        let bytes = compile(&format!(
            "function forever():string{{while(true){{}}return 'n';}}export function main():void{{try{{Object.hasOwn({receiver},forever());}}catch(e){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"
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

#[test]
fn dynamic_own_keys_source_and_key_effects_remain_visible_to_helper_validation() {
    use crate::contract_tests::{export_index, rewrite};
    for (ty, value) in [
        ("{n:number}", "{n:1}"),
        ("number[]", "[1]"),
        ("TypeError", "TypeError('x')"),
    ] {
        let bytes = compile(&format!(
            "function source():{ty}{{console.log('source');return {value};}}function key():string{{console.log('key');return 'n';}}function fake(value:boolean):boolean{{Object.hasOwn(source(),key());return value;}}export function main():void{{varta.json.parse<number>('1');}}"
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
fn dynamic_own_keys_decision_order_uses_utf16_instead_of_utf8_metadata() {
    let out = run(
        r"export function main():void{const value={'\ue000':1,'𐀀':2,'😀':3,'\uffff':4};console.log(JSON.stringify(['\ue000','𐀀','😀','\uffff','\ud800'].map(key=>Object.hasOwn(value,key))));}",
    );
    assert_eq!(out.lines, lines(&["[true,true,true,true,false]"]));
}

#[test]
fn array_string_own_keys_are_canonical_and_presence_aware_in_sparse_profiles() {
    let source = r"export function main():void{
        const a:(number|undefined)[]=[1,,undefined];
        console.log(JSON.stringify(['0','1','2','3','length'].map(key=>Object.hasOwn(a,key))));
        console.log(JSON.stringify(['','00','01','-0','-1','+0','0.0','1e0',' 0','0 ','0x0','١','０','0\u0000','\ud800','__proto__','constructor','toString','Length','length\u0000','4294967294','4294967295','999999999999999999999999999999999999'].map(key=>Object.hasOwn(a,key))));
        console.log(Object.hasOwn(a,'\ud800'));console.log(Object.hasOwn(a,'__proto__'));
        console.log(Object.hasOwn(a,-0));console.log(Object.hasOwn(a,NaN));console.log(Object.hasOwn(a,Infinity));console.log(Object.hasOwn(a,-Infinity));console.log(Object.hasOwn(a,0.5));
        console.log(JSON.stringify(Object.keys(a)));console.log(JSON.stringify(a));
        const empty:number[]=[];const key='length';console.log(Object.hasOwn(empty,key));
    }";
    for profile in [
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
                "[true,false,true,false,true]",
                "[false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false,false]",
                "0",
                "0",
                "1",
                "0",
                "0",
                "0",
                "0",
                "[\"0\",\"2\"]",
                "[1,null,null]",
                "1"
            ]),
            "{profile:?}"
        );
    }
}
#[test]
fn array_string_own_keys_cover_bitmap_edges_and_maximum_index() {
    let out = run(r"export function main():void{
        const a:number[]=[];a.length=1024;
        a[0]=0;a[7]=7;a[8]=8;a[31]=31;a[32]=32;a[255]=255;a[256]=256;a[1023]=1023;
        console.log(JSON.stringify(['0','7','8','31','32','255','256','1023','1024','9999','0000','length'].map(key=>Object.hasOwn(a,key))));
        delete a[8];a.length=32;a.length=1024;
        console.log(JSON.stringify(['7','8','31','32','255','256','1023','length'].map(key=>Object.hasOwn(a,key))));
        a[1023]=1;console.log(Object.hasOwn(a,'102'+'3'));
    }");
    assert_eq!(
        out.lines,
        lines(&[
            "[true,true,true,true,true,true,true,true,false,false,false,true]",
            "[true,false,true,false,false,false,false,true]",
            "1"
        ])
    );
}
#[test]
fn array_string_own_keys_observe_key_mutation_and_retain_original_identity() {
    let out = run(r"export function main():void{
        let a:(number|undefined)[]=[1,,3];const original=a;
        const key=():string=>{a[1]=undefined;a=[9];return '1';};
        console.log(Object.hasOwn(a,key()));console.log(Object.hasOwn(a,'1'+''));console.log(Object.hasOwn(original,'1'+''));
        const shrink=():string=>{original.length=0;return '1';};console.log(Object.hasOwn(original,shrink()));
        console.log(JSON.stringify(['length','0'].map(k=>Object.hasOwn(original,k))));
    }");
    assert_eq!(out.lines, lines(&["1", "0", "1", "0", "[true,false]"]));
}
#[test]
fn error_string_own_keys_cover_classes_presence_and_exact_names() {
    let source = r"function own(e:Error|TypeError|RangeError|SyntaxError,key:string):boolean{return Object.hasOwn(e,key);}
    export function main():void{
        console.log(own(Error(),'message'));console.log(own(Error(undefined),'message'));console.log(own(Error(''),'message'));console.log(own(Error('x'),'message'));
        console.log(own(TypeError(),'message'));console.log(own(TypeError(''),'message'));console.log(own(RangeError(),'message'));console.log(own(RangeError('x'),'message'));console.log(own(SyntaxError(),'message'));console.log(own(SyntaxError('x'),'message'));
        const e=TypeError('message');console.log(JSON.stringify(['message','name','stack','cause','constructor','__proto__','toString','','Message','message\u0000','\ud800','messagé'].map(key=>Object.hasOwn(e,key))));
        console.log(Object.hasOwn(e,'\ud800'));console.log(Object.hasOwn(e,'__proto__'));console.log(e.name);console.log(e.message);
        try{throw RangeError('caught');}catch(e){if(e instanceof Error){const key='message';console.log(Object.hasOwn(e,key));console.log(e.name);}}
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
                "0",
                "0",
                "1",
                "1",
                "0",
                "1",
                "0",
                "1",
                "0",
                "1",
                "[true,false,false,false,false,false,false,false,false,false,false,false]",
                "0",
                "0",
                "TypeError",
                "message",
                "1",
                "RangeError"
            ]),
            "{profile:?}"
        );
    }
}
#[test]
fn array_and_error_own_keys_evaluate_arguments_once_and_propagate_key_throws() {
    let out = run(
        r"function array():number[]{console.log('array');return [1];}
    function error():TypeError{console.log('error');return TypeError('x');}
    function key(s:string):string{console.log(s);return s;}
    function fail():string{console.log('key throws');throw SyntaxError('failed');}
    function later():number{console.log('BAD_LATER');return 0;}
    function consume(a:boolean,b:number):void{console.log('BAD_BODY');}
    export function main():void{
        console.log(Object.hasOwn(array(),key('0')));console.log(Object.hasOwn(error(),key('message')));
        try{consume(Object.hasOwn(array(),fail()),later());}catch({message}){console.log(message);}finally{console.log('array finally');}
        try{consume(Object.hasOwn(error(),fail()),later());}catch({message}){console.log(message);}finally{console.log('error finally');}
    }",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "array",
            "0",
            "1",
            "error",
            "message",
            "1",
            "array",
            "key throws",
            "failed",
            "array finally",
            "error",
            "key throws",
            "failed",
            "error finally"
        ])
    );
}
#[test]
fn array_and_error_own_keys_reject_coercion_and_preserve_profile_boundaries() {
    for receiver in ["[1]", "Error('x')"] {
        for key in [
            "true",
            "null",
            "undefined",
            "({key:'0'})",
            "(()=> '0')",
            "(1 as number|string)",
        ] {
            let source = format!("export function main():void{{Object.hasOwn({receiver},{key});}}");
            let error = varta_compiler::compile_typescript_with_profile(
                &source,
                LanguageProfile::CheckedF64Utf16V14,
            )
            .unwrap_err();
            assert!(error.diagnostic().is_some(), "{source}: {error}");
        }
        let source = format!(
            "export function main():void{{const key:string|undefined='0';Object.hasOwn({receiver},key);}}"
        );
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &source,
                LanguageProfile::CheckedF64Utf16V14
            )
            .is_err()
        );
    }
    for profile in [
        LanguageProfile::CheckedF64Utf16V11,
        LanguageProfile::CheckedF64Utf16V12,
    ] {
        let source = "export function main():void{const key='0';Object.hasOwn([1],key);}";
        assert!(varta_compiler::compile_typescript_with_profile(source, profile).is_err());
    }
    let source = "export function main():void{const key='message';Object.hasOwn(Error('x'),key);}";
    assert!(
        varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V11
        )
        .is_err()
    );
    let literal=varta_compiler::compile_typescript_with_profile("export function main():void{console.log(Object.hasOwn(Error('x'),'message'));console.log(Object.hasOwn(Error(),'message'));console.log(Object.hasOwn(Error('x'),'name'));}",LanguageProfile::CheckedF64Utf16V11).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&literal, &[])
            .unwrap()
            .lines,
        lines(&["1", "0", "0"])
    );
}
#[test]
fn array_and_error_own_keys_keep_receivers_and_earlier_values_alive_across_gc() {
    let out = run_with_gc_budget(
        r"function churn():void{let text='x';for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+'!',text+'?'];garbage.slice();}}
    function array():string[]{return ['owned'+'!',,'tail'];}function error():TypeError{return TypeError('owned'+'!');}
    function key(s:string):string{churn();return s+'';}
    function keep(first:{text:string},pair:VartaResult<string>,a:boolean,e:boolean):boolean{churn();console.log(first.text);console.log(pair.status);console.log(a);return e;}
    function saved():boolean{try{return Object.hasOwn(error(),key('message'));}finally{churn();}}
    export function main():void{console.log(keep({text:'earlier'+'!'},varta.result.capture(varta.env.get('VARTA_OWN_GC')),Object.hasOwn(array(),key('0')),Object.hasOwn(error(),key('message'))));console.log(saved());
        let total=0;for(let i=0;i<7000;i=i+1){if(Object.hasOwn(array(),'0'+'')){total=total+1;}if(Object.hasOwn(error(),'mess'+'age')){total=total+1;}}churn();console.log(total);
    }",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "<capability denied: env> (VARTA_OWN_GC)",
            "earlier!",
            "1",
            "1",
            "1",
            "1",
            "14000"
        ])
    );
}

#[test]
fn array_and_error_own_keys_reject_maximum_length_names_without_scanning_them() {
    let out = run_with_gc_budget(
        "export function main():void{const key='0'.repeat(524288);console.log(Object.hasOwn([1],key));console.log(Object.hasOwn(Error('x'),key));}",
    );
    assert_eq!(out.lines, lines(&["0", "0"]));
}
