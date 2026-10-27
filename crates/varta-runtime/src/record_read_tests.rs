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
fn computed_record_reads_infer_primitive_unions_and_missing_undefined() {
    let source = r"function read(value:{n:number;s:string;b:boolean;z:null;u:undefined},key:string):number|string|boolean|null|undefined{return value[key];}
        export function main():void {
            const value={n:1.5,s:'text',b:true,z:null,u:undefined};
            console.log(JSON.stringify(['n','s','b','z','u','missing'].map(key=>read(value,key))));
            const empty={};const key='missing';console.log(empty[key]);
            console.log(value['n']);console.log(value[('n')]);
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
                "[1.5,\"text\",true,null,null,null]",
                "undefined",
                "1.5",
                "1.5"
            ]),
            "{profile:?}"
        );
    }
}

#[test]
fn computed_record_reads_preserve_optional_presence_json_and_numeric_bits() {
    let out = run(r#"export function main():void {
        const value=varta.json.parse<{a?:number;b:number}>('{"b":2.5}');const key='a';
        console.log(value[key]);console.log(Object.hasOwn(value,key));
        value.a=undefined;console.log(value[key]);console.log(Object.hasOwn(value,key));
        value.a=-0;const found=value[key];if(found!==undefined){console.log(1/found);}
        console.log(JSON.stringify(Object.keys(value)));
        const edges={negative:-0,nan:NaN,infinite:Infinity};
        console.log(edges['nega'+'tive']);console.log(edges['n'+'an']);console.log(edges['infi'+'nite']);
        const {b,...rest}=value;console.log(rest[key]);
    }"#);
    assert_eq!(
        out.lines,
        lines(&[
            "undefined",
            "0",
            "undefined",
            "1",
            "-Infinity",
            "[\"b\",\"a\"]",
            "-0",
            "NaN",
            "Infinity",
            "-0"
        ])
    );
}

#[test]
fn computed_record_reads_use_exact_utf16_names_and_decision_order() {
    let out = run(r"export function main():void {
        const value={'\ue000':1,'𐀀':2,'😀':3,'\uffff':4,'':5,'a\u0000b':6,'é':7,'Привіт':8,'2':9,'01':10};
        console.log(JSON.stringify(['\ue000','𐀀','😀','\uffff','','a\u0000b','é','Привіт','2','01','\ud800','\udc00','é','привіт','a','a\u0000','02'].map(key=>value[key])));
    }");
    assert_eq!(
        out.lines,
        lines(&["[1,2,3,4,5,6,7,8,9,10,null,null,null,null,null,null,null]"])
    );
}

#[test]
fn computed_record_reads_keep_reference_identity_arrays_and_error_tags() {
    let out = run(r"export function main():void {
        const child={n:1};const array:number[]=[2];const source={a:child,b:child,list:array,n:0};
        const key='a';const selected=source[key];
        if(typeof selected==='object'){if(!Array.isArray(selected)){selected.n=7;console.log(selected===child);}}
        const list=source['li'+'st'];if(Array.isArray(list)){list.push(3);console.log(list===array);}
        console.log(JSON.stringify(child));console.log(JSON.stringify(array));
        const error=TypeError('owned');const errors={a:error,b:error};const found=errors[key];
        if(found instanceof TypeError){console.log(found.message);console.log(found===error);}
        const text={s:'\ud800',u:undefined};console.log(JSON.stringify(text['s'+'' ]));
    }");
    assert_eq!(
        out.lines,
        lines(&["1", "1", "{\"n\":7}", "[2,3]", "owned", "1", "\"\\ud800\""])
    );
}

#[test]
fn computed_record_reads_evaluate_once_and_observe_key_mutation_on_original_source() {
    let out = run(
        r"function source():{n:number}{console.log('source');return {n:1};}
        function key():string{console.log('key');return 'n';}
        function empty():{}{console.log('empty');return {};}
        export function main():void {
            console.log(source()[key()]);console.log(empty()[key()]);
            let value:{a?:number}={};const original=value;
            const mutate=():string=>{value.a=9;value={};return 'a';};
            console.log(value[mutate()]);console.log(value['a'+ '']);console.log(original['a'+ '']);
            const nullable:{n:number}|undefined={n:5};const maybeKey:string|undefined='n';
            if(nullable!==undefined){if(maybeKey!==undefined){console.log(nullable[maybeKey]);}}
        }",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "source",
            "key",
            "1",
            "empty",
            "key",
            "undefined",
            "9",
            "undefined",
            "9",
            "5"
        ])
    );
}

#[test]
fn computed_record_reads_reject_inherited_lookups_without_hiding_real_own_fields() {
    let out = run(r"function read(key:string):undefined{return {}[key];}
        export function main():void {
            for(const key of ['constructor','__defineGetter__','__defineSetter__','hasOwnProperty','__lookupGetter__','__lookupSetter__','isPrototypeOf','propertyIsEnumerable','toLocaleString','toString','valueOf','__proto__']) {
                if(key!==undefined){try{read(key);}catch(error){console.log(error instanceof TypeError);}}
            }
            const own={constructor:1,toString:undefined,valueOf:'owned'};
            console.log(JSON.stringify(['constructor','toString','valueOf','missing'].map(key=>own[key])));
            const key='constructor';console.log(Object.hasOwn({},key));
        }");
    let mut expected = vec!["1"; 12];
    expected.extend(["[1,null,\"owned\",null]", "0"]);
    assert_eq!(out.lines, lines(&expected));
}

#[test]
fn computed_record_reads_throws_skip_later_work_and_preserve_finalizers() {
    let out = run(
        r"function source():{n:number}{console.log('source');return {n:1};}
        function bad():string{console.log('key');throw RangeError('key failure');}
        function fail():{n:number}{console.log('receiver');throw TypeError('source failure');}
        function later():number{console.log('BAD_LATER');return 0;}
        function consume(value:number|undefined,other:number):void{console.log('BAD_BODY');}
        export function main():void {
            try{consume(source()[bad()],later());}catch({message}){console.log(message);}finally{console.log('finally');}
            try{consume(fail()[bad()],later());}catch({message}){console.log(message);}
            try{consume(source()['to'+'String'],later());}catch({message}){console.log(message);}finally{console.log('inherited finally');}
        }",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "source",
            "key",
            "key failure",
            "finally",
            "receiver",
            "source failure",
            "source",
            "Inherited record property access is unsupported",
            "inherited finally"
        ])
    );
}

#[test]
fn computed_record_reads_cover_all_slots_and_presence_bit_31_under_default_limits() {
    let fields = (0..varta_common::value::MAX_RECORD_FIELDS)
        .map(|i| format!("f{i:02}?:number"))
        .collect::<Vec<_>>()
        .join(";");
    let names = (0..varta_common::value::MAX_RECORD_FIELDS)
        .map(|i| format!("'f{i:02}'"))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        "export function main():void{{const value:{{{fields}}}={{f00:0,f31:31}};const keys=Object.keys(value);console.log(JSON.stringify(keys.map(key=>value[key])));let total=0;for(const key of [{names}]){{if(key!==undefined){{const got=value[key];if(got!==undefined){{total=total+got;}}}}}}console.log(total);const key='f31';value.f31=undefined;console.log(value[key]);console.log(Object.hasOwn(value,key));}}"
    );
    assert_eq!(
        run(&source).lines,
        lines(&["[0,31]", "31", "undefined", "1"])
    );
}

#[test]
fn computed_record_reads_reject_coercion_invalid_writes_unrepresentable_unions_and_old_profiles() {
    for source in [
        "export function main():void{const key=1;({n:1})[key];}",
        "export function main():void{({n:1})[true];}",
        "export function main():void{const key:string|undefined='n';({n:1})[key];}",
        "export function main():void{const source:{n:number}|null={n:1};const key='n';source[key];}",
        "export function main():void{const key='n';const value={n:1};value[key]='wrong';}",
        "export function main():void{const key='n';const value={n:1};value[key]+=2;}",
        "export function main():void{const key='n';const value={n:1};[value[key]]=[2];}",
        "export function main():void{const key='n';const value={n:1};delete value[key];}",
        "export function main():void{const key='a';({a:{n:1},b:{s:'x'}})[key];}",
        "export function main():void{const key='a';({a:[1],b:['x']})[key];}",
        "export function main():void{const key='n';const value=({n:1})[key];const n:number=value;}",
        "export function main():void{const key=({n:1})[key];}",
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
                "export function main():void{const key='n';({n:1})[key];}",
                profile
            )
            .is_err()
        );
        let bytes=varta_compiler::compile_typescript_with_profile("export function main():void{const value={n:2};console.log(value['n']);value['n']=3;console.log(value.n);}",profile).unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            lines(&["2", "3"])
        );
    }
}

#[test]
fn computed_record_reads_gc_keeps_payloads_inputs_pairs_and_saved_returns() {
    let bytes = compile(
        r"function churn():void{let text='x';for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+'!',text+'?'];garbage.slice();}}
        function source():{text:string}{return {text:'owned'+'!'};}
        function key():string{churn();return 'te'+'xt';}
        function saved():string|undefined{try{return source()[key()];}finally{churn();}}
        function children():{child:{text:string};array:string[]}{return {child:{text:'child'+'!'},array:['array'+'!']};}
        function savedChild(name:string):{text:string}|string[]|undefined{try{return children()[name];}finally{churn();}}
        function keep(first:{text:string},pair:VartaResult<string>,value:string|undefined):string|undefined{churn();console.log(first.text);console.log(pair.status);return value;}
        export function main():void {
            console.log(keep({text:'earlier'+'!'},varta.result.capture(varta.env.get('VARTA_RECORD_READ_GC')),source()[key()]));
            console.log(saved());const value={text:'retained'+'!'};const lookup=():string|undefined=>value['te'+'xt'];
            let result:string|undefined=undefined;for(let i=0;i<7000;i=i+1){result=lookup();}churn();console.log(result);
            const child=savedChild('chi'+'ld');const array=savedChild('arr'+'ay');churn();console.log(JSON.stringify(child));console.log(JSON.stringify(array));
        }",
    );
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
            "<capability denied: env> (VARTA_RECORD_READ_GC)",
            "earlier!",
            "1",
            "owned!",
            "owned!",
            "retained!",
            "{\"text\":\"child!\"}",
            "[\"array!\"]"
        ])
    );
}

#[test]
fn computed_record_reads_fatal_key_budgets_bypass_catch_and_finally() {
    let bytes = compile(
        "function forever():string{while(true){}return 'n';}export function main():void{try{console.log(({n:1})[forever()]);}catch(e){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
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
    assert!(execution.store.data().output.lines.is_empty());
}

#[test]
fn computed_record_reads_source_and_key_effects_remain_visible_to_helper_validation() {
    use crate::contract_tests::{export_index, rewrite};
    for (source, key) in [
        ("console.log('source');return {n:1};", "return 'n';"),
        ("return {n:1};", "console.log('key');return 'n';"),
    ] {
        let bytes = compile(&format!(
            "function source():{{n:number}}{{{source}}}function key():string{{{key}}}function fake(value:boolean):boolean{{source()[key()];return value;}}export function main():void{{varta.json.parse<number>('1');}}"
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
fn computed_record_reads_bound_maximum_keys_and_preserve_fatal_string_limits() {
    let units = abi::MAX_STRING_BYTES / size_of::<u16>();
    let bytes = compile(&format!(
        "export function main():void{{const value={{n:1}};const key='x'.repeat({units});console.log(value[key]);try{{console.log(value[key+'x']);}}catch(e){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"
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
    assert_eq!(execution.store.data().output.lines, lines(&["undefined"]));
}
