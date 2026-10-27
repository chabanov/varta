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
fn object_keys_returns_fresh_dense_string_arrays_with_js_key_order() {
    let out = run(
        "export function main():void{const value={z:1,'10':2,'2':3,a:4,'01':5,'4294967295':6};const keys=Object.keys(value);console.log(JSON.stringify(keys));console.log(keys===Object.keys(value));console.log(Object.hasOwn(keys,0));console.log(Object.hasOwn(keys,5));console.log(Object.hasOwn(keys,6));console.log(JSON.stringify(Object.keys({})));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "[\"2\",\"10\",\"z\",\"a\",\"01\",\"4294967295\"]",
            "0",
            "1",
            "1",
            "0",
            "[]"
        ])
    );
}
#[test]
fn object_keys_distinguishes_absence_from_undefined_and_tracks_new_field_order() {
    let out = run(
        "export function main():void{const value:{a?:number;b?:number;'1'?:number;'2'?:number;c:number}={c:0};console.log(JSON.stringify(Object.keys(value)));value.b=undefined;value['2']=2;value.a=1;value['1']=1;console.log(JSON.stringify(Object.keys(value)));value.b=3;console.log(JSON.stringify(Object.keys(value)));console.log(JSON.stringify(value));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "[\"c\"]",
            "[\"1\",\"2\",\"c\",\"b\",\"a\"]",
            "[\"1\",\"2\",\"c\",\"b\",\"a\"]",
            "{\"1\":1,\"2\":2,\"c\":0,\"b\":3,\"a\":1}"
        ])
    );
}
#[test]
fn object_keys_evaluates_source_once_and_returns_an_independent_snapshot() {
    let out = run(
        "export function main():void{const value:{a?:number;b:number}={b:0};const source=():{a?:number;b:number}=>{console.log('source');value.a=1;return value;};const keys=Object.keys(source());value.b=2;keys[0]='changed';keys.push('extra');console.log(JSON.stringify(keys));console.log(JSON.stringify(Object.keys(value)));const other=Object.keys(value);other.length=0;other.push('safe');console.log(JSON.stringify(other));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "source",
            "[\"changed\",\"a\",\"extra\"]",
            "[\"b\",\"a\"]",
            "[\"safe\"]"
        ])
    );
}
#[test]
fn object_keys_preserves_unicode_keys_without_serializing_or_reading_values() {
    let out = run(
        "export function main():void{const value={'Привіт':NaN,'😀':Infinity,'a\\u0000b':'\\ud800'};console.log(JSON.stringify(Object.keys(value)));const own={toString:1,constructor:2};console.log(JSON.stringify(Object.keys(own)));const rows:{a?:number;b:string}[]=[{b:'value'}];const {...rest}=rows[0];console.log(JSON.stringify(Object.keys(rest)));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "[\"Привіт\",\"😀\",\"a\\u0000b\"]",
            "[\"toString\",\"constructor\"]",
            "[\"b\"]"
        ])
    );
}
#[test]
fn object_keys_handles_nullable_records_and_throws_before_enclosing_calls() {
    let out = run(
        "function source(present:boolean):{a:number}|null|undefined{console.log('source');if(present){return {a:1};}return null;}function consume(keys:string[]):void{console.log('BAD');}export function main():void{console.log(JSON.stringify(Object.keys(source(true))));try{consume(Object.keys(source(false)));}catch(e){if(e instanceof TypeError){console.log(e.message);}}finally{console.log('finally');}}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "source",
            "[\"a\"]",
            "source",
            "Cannot convert undefined or null to object",
            "finally"
        ])
    );
}
#[test]
fn object_keys_clears_unused_payloads_and_bits_before_result_growth_or_collection() {
    let out = run(
        "export function main():void{const value:{a?:number;b?:number;c?:number;d?:number;e?:number}={d:4};const keys=Object.keys(value);console.log(keys.length);console.log(JSON.stringify(keys));console.log(Object.hasOwn(keys,1));keys.length=5;console.log(JSON.stringify(keys));console.log(Object.hasOwn(keys,4));keys[4]='last';console.log(JSON.stringify(keys));keys.length=0;keys.push('new');console.log(JSON.stringify(keys));const empty:{a?:number;b?:number}={};const none=Object.keys(empty);none.push('first');console.log(JSON.stringify(none));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "1",
            "[\"d\"]",
            "0",
            "[\"d\",null,null,null,null]",
            "0",
            "[\"d\",null,null,null,\"last\"]",
            "[\"new\"]",
            "[\"first\"]"
        ])
    );
}
#[test]
fn object_keys_rejects_shadowed_intrinsics_unsupported_sources_and_legacy_profiles() {
    for source in [
        "export function main():void{Object.keys();}",
        "export function main():void{Object.keys({},{a:1});}",
        "export function main():void{Object.keys([1]);}",
        "export function main():void{Object.keys('text');}",
        "export function main():void{Object.keys(1);}",
        "export function main():void{Object.keys(Error('x'));}",
        "export function main():void{Object.keys(varta.result.capture(varta.env.get('x')));}",
        "export function main():void{const Object={keys:1};Object.keys({a:1});}",
        "function Object():void{}export function main():void{Object.keys({a:1});}",
        "export function main():void{Object.keys<number>({a:1});}",
        "export function main():void{Object.keys(...[{a:1}]);}",
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
                "export function main():void{Object.keys({a:1});}",
                profile
            )
            .is_err()
        );
    }
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
    ] {
        let bytes=varta_compiler::compile_typescript_with_profile("export function main():void{const value:{a?:number;b:number}={b:1};console.log(JSON.stringify(Object.keys(value)));}",profile).unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            lines(&["[\"b\"]"])
        );
    }
}
#[test]
fn object_keys_handles_all_presence_bits_and_maximum_shapes_at_default_limits() {
    let fields = (0..varta_common::value::MAX_RECORD_FIELDS)
        .map(|n| format!("f{n:02}?:number"))
        .collect::<Vec<_>>()
        .join(";");
    let source = format!(
        "export function main():void{{const value:{{{fields}}}={{f31:31,f00:0}};const keys=Object.keys(value);console.log(JSON.stringify(keys));console.log(Object.hasOwn(keys,31));keys.push('tail');console.log(JSON.stringify(keys));value.f30=30;console.log(JSON.stringify(Object.keys(value)));let count=0;for(let i=0;i<200;i=i+1){{const current=Object.keys(value);count=count+current.length;}}console.log(count);}}"
    );
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        let source = if profile == LanguageProfile::CheckedF64Utf16V12 {
            source.replace(
                "console.log(Object.hasOwn(keys,31));",
                "console.log(false);",
            )
        } else {
            source.clone()
        };
        let bytes = varta_compiler::compile_typescript_with_profile(&source, profile).unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            lines(&[
                "[\"f31\",\"f00\"]",
                "0",
                "[\"f31\",\"f00\",\"tail\"]",
                "[\"f31\",\"f00\",\"f30\"]",
                "600"
            ]),
            "{profile:?}"
        );
    }
}
#[test]
fn object_keys_result_context_composes_with_callbacks_nested_calls_and_assignment_values() {
    let out = run(
        "function names():string[]{return Object.keys({n:1});}export function main():void{console.log(JSON.stringify([1,2].map(n=>{return Object.keys({n:n});})));console.log(JSON.stringify(Object.keys({n:1}).map(key=>key+'!')));console.log(JSON.stringify(names()));let a=0;console.log(JSON.stringify(Object.keys(({n:a}={n:1,extra:2}))));console.log(a);console.log(Object.keys({n:1}).length);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "[[\"n\"],[\"n\"]]",
            "[\"n!\"]",
            "[\"n\"]",
            "[\"n\",\"extra\"]",
            "1",
            "1"
        ])
    );
}
#[test]
fn object_keys_value_blocks_keep_earlier_operands_result_pairs_and_return_values_rooted() {
    let source = "function churn():number{let text='x';for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+'!',text+'?'];garbage.slice();}return 1;}function source():{a?:number;b:number}{churn();return {b:1};}function keep(first:{text:string},pair:VartaResult<string>,keys:string[]):string{churn();console.log(pair.status);return first.text+JSON.stringify(keys);}function delayed(keys:string[],unused:number):string{return JSON.stringify(keys);}function result():string[]{try{return Object.keys(source());}finally{churn();}}export function main():void{console.log(keep({text:'kept'+'!'},varta.result.capture(varta.env.get('VARTA_KEYS_GC')),Object.keys(source())));console.log(delayed(Object.keys(source()),churn()));console.log(JSON.stringify(result()));console.log(('prefix'+'!')+JSON.stringify(Object.keys(source())));}";
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
            "<capability denied: env> (VARTA_KEYS_GC)",
            "1",
            "kept![\"b\"]",
            "[\"b\"]",
            "[\"b\"]",
            "prefix![\"b\"]"
        ])
    );
}
#[test]
fn object_keys_argument_effects_stay_visible_to_host_free_helper_validation() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function source():{n:number}{console.log('effect');return {n:1};}function fake(value:boolean):boolean{Object.keys(source());return value;}export function main():void{varta.json.parse<number>('1');}",
    );
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
#[test]
fn object_keys_typed_json_sources_keep_presence_and_schema_enumeration_order() {
    let out = run(
        r#"export function main():void{const value=varta.json.parse<{a?:number;b:number;c?:number}>('{"b":1,"c":2}');console.log(JSON.stringify(Object.keys(value)));value.a=undefined;console.log(JSON.stringify(Object.keys(value)));const {b,...rest}=value;console.log(JSON.stringify(Object.keys(rest)));}"#,
    );
    assert_eq!(
        out.lines,
        lines(&["[\"b\",\"c\"]", "[\"b\",\"c\",\"a\"]", "[\"c\",\"a\"]"])
    );
}
#[test]
fn object_keys_argument_resource_traps_bypass_guest_handlers() {
    let bytes = compile(
        "function source():{n:number}{while(true){}return {n:1};}export function main():void{try{Object.keys(source());}catch(e){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
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
