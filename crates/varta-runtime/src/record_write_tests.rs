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
fn computed_record_writes_check_selected_primitive_field_and_preserve_presence_order() {
    let source = r"function put(value:{n:number;s:string;a?:number},key:string,item:number|string|undefined):void{value[key]=item;}
        export function main():void {
            const value:{n:number;s:string;a?:number}={s:'old',n:1};
            put(value,'n',2.5);put(value,'s','new');put(value,'a',undefined);
            console.log(JSON.stringify(Object.keys(value)));console.log(JSON.stringify(value));console.log(Object.hasOwn(value,'a'));
            put(value,'a',3);console.log(JSON.stringify(Object.keys(value)));
            for(const key of ['s','n','missing']){if(key!==undefined){try{put(value,key,undefined);}catch(error){console.log(error instanceof TypeError);}}}
            console.log(JSON.stringify(value));
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
                "[\"s\",\"n\",\"a\"]",
                "{\"s\":\"new\",\"n\":2.5}",
                "1",
                "[\"s\",\"n\",\"a\"]",
                "1",
                "1",
                "1",
                "{\"s\":\"new\",\"n\":2.5,\"a\":3}"
            ]),
            "{profile:?}"
        );
    }
}

#[test]
fn computed_record_writes_accept_scalars_and_partial_unions_without_coercion() {
    let out = run(
        r"function put(value:{n:number;s:string;b:boolean;z:null;u:undefined;m:number|string},key:string,item:number|string|boolean|null|undefined):void{value[key]=item;}
        export function main():void {
            const value:{n:number;s:string;b:boolean;z:null;u:undefined;m:number|string}={n:1,s:'old',b:false,z:null,u:undefined,m:'old'};
            put(value,'n',-0);console.log(value['n'+ '']);put(value,'n',NaN);console.log(value.n);put(value,'n',Infinity);console.log(value.n);
            put(value,'s','\ud800');console.log(JSON.stringify(value.s));put(value,'b',true);put(value,'z',null);put(value,'u',undefined);
            put(value,'m',2.5);console.log(value['m'+ '']);put(value,'m','new');console.log(JSON.stringify(value));
            try{put(value,'m',true);}catch({message}){console.log(message);}console.log(value['m'+ '']);
            const key='n';value[key]=4;console.log(value.n);const nullable:{n:number}|null={n:1};const maybe:string|undefined='n';
            if(nullable!==null){if(maybe!==undefined){nullable[maybe]=5;console.log(nullable.n);}}
        }",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "-0",
            "NaN",
            "Infinity",
            "\"\\ud800\"",
            "2.5",
            "{\"n\":null,\"s\":\"\\ud800\",\"b\":true,\"z\":null,\"m\":\"new\"}",
            "Invalid computed record field value",
            "new",
            "4",
            "5"
        ])
    );
}

#[test]
fn computed_record_writes_preserve_exact_record_array_types_and_reference_identity() {
    let out = run(
        r"function put(value:{child:{n:number};items:number[];other:{s:string}},key:string,item:{n:number}|number[]|undefined):void{value[key]=item;}
        export function main():void {
            const value={child:{n:1},items:[2],other:{s:'old'}};const child={n:7};const items:number[]=[8];
            put(value,'child',child);put(value,'items',items);console.log(value.child===child);console.log(value.items===items);
            child.n=9;items.push(10);console.log(JSON.stringify(value));
            for(const key of ['child','other','items']){if(key!==undefined){try{put(value,key,key==='child'?items:child);}catch(error){console.log(error instanceof TypeError);}}}
            console.log(JSON.stringify(value));
            const literalKey='child';value[literalKey]={n:11};const arrayKey='items';value[arrayKey]=[];
            console.log(JSON.stringify(value));
            const optional:{child?:{n:number};items?:number[]}={};const key='child';optional[key]={n:12};optional['it'+'ems']=[];console.log(JSON.stringify(optional));
        }",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "1",
            "1",
            "{\"child\":{\"n\":9},\"items\":[8,10],\"other\":{\"s\":\"old\"}}",
            "1",
            "1",
            "1",
            "{\"child\":{\"n\":9},\"items\":[8,10],\"other\":{\"s\":\"old\"}}",
            "{\"child\":{\"n\":11},\"items\":[],\"other\":{\"s\":\"old\"}}",
            "{\"child\":{\"n\":12},\"items\":[]}"
        ])
    );
}

#[test]
fn computed_record_writes_distinguish_exact_error_families_before_unboxing() {
    let out = run(
        r"function put(value:{type:TypeError;range:RangeError;base:Error},key:string,item:Error|number):void{value[key]=item;}
        export function main():void {
            const value:{type:TypeError;range:RangeError;base:Error}={type:TypeError('t'),range:RangeError('r'),base:Error('e')};
            const type=TypeError('new');put(value,'type',type);console.log(value.type===type);
            put(value,'range',RangeError('new range'));console.log(value.range.message);
            const errors:Error[]=[Error('base'),TypeError('type'),RangeError('range'),SyntaxError('syntax')];
            for(const item of errors) {
                if(item!==undefined){put(value,'base',item);console.log(value.base.name);try{put(value,'type',item);console.log('accepted');}catch(error){console.log('rejected');}}
            }
            try{put(value,'type',0);}catch(error){console.log(error instanceof TypeError);}console.log(value.type.message);
        }",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "1",
            "new range",
            "Error",
            "rejected",
            "TypeError",
            "accepted",
            "RangeError",
            "rejected",
            "SyntaxError",
            "rejected",
            "1",
            "type"
        ])
    );
}

#[test]
fn computed_record_writes_do_not_merge_or_erase_distinct_array_element_types() {
    let out = run(
        r"function put(value:{nums:number[];strings:string[]},name:string,item:number[]|undefined):void{value[name]=item;}
        export function main():void{const value={nums:[1],strings:['old']};const nums=[2];put(value,'nums',nums);console.log(value.nums===nums);try{put(value,'strings',nums);}catch(error){console.log(error instanceof TypeError);}value['str'+'ings']=['new'];console.log(JSON.stringify(value));}",
    );
    assert_eq!(
        out.lines,
        lines(&["1", "1", "{\"nums\":[2],\"strings\":[\"new\"]}"])
    );
}

#[test]
fn computed_record_writes_retype_only_proven_primitive_subsets_of_reference_unions() {
    let out = run(
        r"function put(value:{subset:number|string;all:number|string|number[]},name:string,item:number|string|number[]):void{value[name]=item;}
        export function main():void{const value:{subset:number|string;all:number|string|number[]}={subset:0,all:0};put(value,'subset',-0);console.log(value.subset);put(value,'subset','owned'+'!');console.log(JSON.stringify(value));const array=[1];put(value,'all',array);const saved=value.all;if(Array.isArray(saved)){console.log(saved===array);}try{put(value,'subset',array);}catch(error){console.log(error instanceof TypeError);}console.log(JSON.stringify(value));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "-0",
            "{\"subset\":\"owned!\",\"all\":0}",
            "1",
            "1",
            "{\"subset\":\"owned!\",\"all\":[1]}"
        ])
    );
}

#[test]
fn computed_record_writes_compare_exact_utf16_and_retain_original_shape_slots() {
    let out = run(
        r"function put(value:{'\ue000':number;'𐀀':number;'😀':number;'\uffff':number;'':number;'a\u0000b':number;'é':number},key:string):void{value[key]=9;}
        export function main():void {
            const value={'\ue000':1,'𐀀':2,'😀':3,'\uffff':4,'':5,'a\u0000b':6,'é':7};
            for(const key of Object.keys(value)){if(key!==undefined){put(value,key);}}
            console.log(JSON.stringify(Object.values(value)));
            for(const key of ['\ud800','\udc00','é','a','a\u0000','É']){if(key!==undefined){try{put(value,key);}catch(error){console.log(error instanceof TypeError);}}}
            console.log(JSON.stringify(Object.values(value)));
        }",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "[9,9,9,9,9,9,9]",
            "1",
            "1",
            "1",
            "1",
            "1",
            "1",
            "[9,9,9,9,9,9,9]"
        ])
    );
}

#[test]
fn computed_record_writes_evaluate_source_key_rhs_once_and_store_to_prepared_receiver() {
    let out = run(
        r"function source():{n:number}{console.log('source');return {n:1};}
        function key():string{console.log('key');return 'n';}function rhs():number{console.log('rhs');return 2;}
        export function main():void {
            source()[key()]=rhs();let value:{a?:number;b:number}={b:1};const original=value;let name='a';
            const choose=():string=>{console.log('choose');value.a=1;return name;};
            const item=():number=>{console.log('item');name='b';value.a=2;value={b:10};return 3;};
            value[choose()]=item();console.log(JSON.stringify(original));console.log(JSON.stringify(value));
            const nums={n:0};for(nums['n'+ '']=1;nums.n<3;nums['n'+ '']=nums.n+1){console.log(nums.n);}
            (nums['n'+ '']=4);console.log(nums.n);
        }",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "source",
            "key",
            "rhs",
            "choose",
            "item",
            "{\"b\":1,\"a\":3}",
            "{\"b\":10}",
            "1",
            "2",
            "4"
        ])
    );
}

#[test]
fn computed_record_writes_unknown_and_inherited_keys_fail_after_rhs_without_new_properties() {
    let out = run(r"function item():number{console.log('rhs');return 2;}
        export function main():void {
            const value={n:1};for(const key of ['missing','constructor','toString','__proto__']) {
                if(key!==undefined){try{value[key]=item();}catch({message}){console.log(message);}}
            }
            const empty={};const key='n';try{empty[key]=item();}catch(error){console.log(error instanceof TypeError);}
            const own={constructor:1,toString:2};own['con'+'structor']=3;own['to'+'String']=4;
            console.log(JSON.stringify(value));console.log(JSON.stringify(empty));console.log(JSON.stringify(own));
        }");
    assert_eq!(
        out.lines,
        lines(&[
            "rhs",
            "Unknown computed record write key",
            "rhs",
            "Unknown computed record write key",
            "rhs",
            "Unknown computed record write key",
            "rhs",
            "Unknown computed record write key",
            "rhs",
            "1",
            "{\"n\":1}",
            "{}",
            "{\"constructor\":3,\"toString\":4}"
        ])
    );
}

#[test]
fn computed_record_writes_exceptions_stop_later_effects_and_leave_requested_store_unperformed() {
    let out = run(
        r"function badSource():{n:number}{console.log('source');throw TypeError('source failure');}
        function badKey():string{console.log('key');throw RangeError('key failure');}
        function rhs():number{console.log('rhs');return 2;}function key():string{console.log('good key');return 'n';}
        export function main():void {
            const value:{n:number;a?:string}={n:1};
            try{badSource()[badKey()]=rhs();}catch({message}){console.log(message);}
            try{value[badKey()]=rhs();}catch({message}){console.log(message);}
            const bad=():number=>{value.n=5;throw SyntaxError('rhs failure');};
            try{value[key()]=bad();console.log('BAD_AFTER');}catch({message}){console.log(message);}finally{console.log('finally');}
            console.log(JSON.stringify(value));
            try{value['a'+ '']=rhs();}catch(error){console.log(error instanceof TypeError);}console.log(Object.hasOwn(value,'a'));console.log(JSON.stringify(value));
        }",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "source",
            "source failure",
            "key",
            "key failure",
            "good key",
            "rhs failure",
            "finally",
            "{\"n\":5}",
            "rhs",
            "1",
            "0",
            "{\"n\":5}"
        ])
    );
}

#[test]
fn computed_record_writes_cover_all_presence_bits_and_default_budgets() {
    let fields = (0..varta_common::value::MAX_RECORD_FIELDS)
        .map(|i| format!("f{i:02}?:number"))
        .collect::<Vec<_>>()
        .join(";");
    let names = (0..varta_common::value::MAX_RECORD_FIELDS)
        .map(|i| format!("'f{i:02}'"))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        "export function main():void{{const value:{{{fields}}}={{}};const keys=[{names}];let n=0;for(const key of keys){{if(key!==undefined){{value[key]=n;n=n+1;}}}}console.log(Object.keys(value).length);console.log(value.f00);console.log(value.f31);const key='f31';value[key]=undefined;console.log(Object.hasOwn(value,key));console.log(value[key]);console.log(JSON.stringify(Object.keys(value))===JSON.stringify(keys));}}"
    );
    assert_eq!(
        run(&source).lines,
        lines(&["32", "0", "31", "1", "undefined", "1"])
    );
}

#[test]
fn computed_record_writes_reject_unchecked_types_targets_and_older_profiles() {
    for source in [
        "export function main():void{const key=1;const value={n:1};value[key]=2;}",
        "export function main():void{const value={n:1};value[true]=2;}",
        "export function main():void{const key:string|undefined='n';const value={n:1};value[key]=2;}",
        "export function main():void{const value:{n:number}|null={n:1};const key='n';value[key]=2;}",
        "export function main():void{const key='n';const value={n:1};value[key]='wrong';}",
        "export function main():void{const key='n';const value={n:1};value[key]=()=>2;}",
        "export function main():void{const key='n';const value={n:1};value[key]=varta.result.capture(varta.time.now());}",
        "export function main():void{const key='n';const value={n:1};value[key]**=2;}",
        "export function main():void{const value={n:1};const result=(value.n='wrong');}",
        "export function main():void{const key='n';const value={n:1};[value[key]]=[2];}",
        "export function main():void{const key='n';const value={n:1};delete value[key];}",
        "export function main():void{const key='child';const value={child:{n:1}};value[key]={s:'wrong'};}",
        "export function main():void{const key='items';const value={items:[1]};value[key]=['wrong'];}",
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
                "export function main():void{const key='n';const value={n:1};value[key]=2;}",
                profile
            )
            .is_err()
        );
        let bytes = varta_compiler::compile_typescript_with_profile(
            "export function main():void{const value={n:1};value['n']=2;console.log(value.n);}",
            profile,
        )
        .unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            lines(&["2"])
        );
    }
}

#[test]
fn computed_record_writes_gc_retains_prepared_inputs_child_graphs_and_saved_pairs() {
    let source = r"function churn():void{let text='x';for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+'!',text+'?'];garbage.slice();}}
        function make():{child:{text:string};maybe?:{text:string}}{return {child:{text:'old'+'!'}};}
        function key():string{churn();return 'may'+'be';}
        function rhs():{text:string}{churn();const fresh={text:'new'+'!'};churn();return fresh;}
        function saved():{child:{text:string};maybe?:{text:string}}{const value=make();try{return value;}finally{value[key()]=rhs();churn();}}
        function pair():VartaResult<string>{try{return varta.result.capture(varta.env.get('VARTA_RECORD_WRITE_GC'));}finally{const value=make();value[key()]=rhs();churn();console.log(JSON.stringify(value));}}
        export function main():void {
            const result=saved();churn();console.log(JSON.stringify(result));const captured=pair();console.log(captured.status);console.log(captured.value);
            const value:{s?:string}={};for(let i=0;i<7000;i=i+1){value['s'+'']='retained'+'!';}
            churn();console.log(value.s);console.log(Object.hasOwn(value,'s'));
            for(let i=0;i<1000;i=i+1){try{value['miss'+'ing']='x';}catch(error){}}
            churn();console.log(value.s);
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
            "{\"child\":{\"text\":\"old!\"},\"maybe\":{\"text\":\"new!\"}}",
            "<capability denied: env> (VARTA_RECORD_WRITE_GC)",
            "{\"child\":{\"text\":\"old!\"},\"maybe\":{\"text\":\"new!\"}}",
            "1",
            "",
            "retained!",
            "1",
            "retained!"
        ])
    );
}

#[test]
fn computed_record_writes_visit_all_operand_effects_in_helper_purity_validation() {
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
            "function source():{{n:number}}{{{source}}}function key():string{{{key}}}function rhs():number{{{rhs}}}function fake(value:boolean):boolean{{source()[key()]=rhs();return value;}}export function main():void{{varta.json.parse<number>('1');}}"
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
fn computed_record_writes_fatal_key_and_rhs_budgets_bypass_catch_and_finally() {
    for assignment in ["value[foreverKey()]=rhs();", "value['n'+'']=foreverRhs();"] {
        let bytes = compile(&format!(
            "function foreverKey():string{{while(true){{}}return 'n';}}function foreverRhs():number{{while(true){{}}return 2;}}function rhs():number{{console.log('BAD_RHS');return 2;}}export function main():void{{const value={{n:1}};try{{{assignment}}}catch(error){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"
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
fn computed_record_writes_bound_maximum_keys_and_preserve_fatal_string_limits() {
    let units = abi::MAX_STRING_BYTES / size_of::<u16>();
    let bytes = compile(&format!(
        "function item():number{{console.log('rhs');return 2;}}export function main():void{{const value={{n:1}};const key='x'.repeat({units});try{{value[key]=item();}}catch(error){{console.log(error instanceof TypeError);}}console.log(value.n);try{{value[key+'x']=item();}}catch(error){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"
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
    assert_eq!(
        execution.store.data().output.lines,
        lines(&["rhs", "1", "1"])
    );
}
