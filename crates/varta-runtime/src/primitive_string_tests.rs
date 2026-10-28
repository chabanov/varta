use super::*;
use crate::execution::Execution;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V19)
        .unwrap()
}
fn lines(text: &[&str]) -> Vec<String> {
    text.iter().map(|s| format!("varta log: {s}")).collect()
}
#[test]
fn primitive_string_converts_all_primitives_and_unions_without_losing_utf16() {
    let bytes = compile(
        "function show(v:number|boolean|string|null|undefined):void{console.log(String(v));}export function main():void{console.log(String());show(-0);show(1e21);show(Number.MIN_VALUE);show(NaN);show(Infinity);show(-Infinity);show(true);show(false);show(null);show(undefined);show('text');const text=String('A\\uD800\\uDC00\\uDFFF\\u0000Z');console.log(text.length);console.log(text.charCodeAt(3));console.log(text.charCodeAt(4));console.log(String(console.log('void')));}",
    );
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap()
            .lines,
        lines(&[
            "",
            "0",
            "1e+21",
            "5e-324",
            "NaN",
            "Infinity",
            "-Infinity",
            "true",
            "false",
            "null",
            "undefined",
            "text",
            "6",
            "57343",
            "0",
            "void",
            "undefined"
        ])
    );
}
#[test]
fn primitive_string_caches_first_value_before_ignored_arguments_and_converts_after_them() {
    let bytes = compile(
        "function later():string{console.log('later');return 'ignored';}function fail():number{throw RangeError('argument');}export function main():void{let n=1e21;console.log(String(n,n=2,later(),{n:1},[1],varta.result.capture(varta.env.get('STRING_ORDER_DENIED'))));console.log(n);console.log(varta.result.status());try{String(3,fail(),later());}catch({message}){console.log(message);}}",
    );
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
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
            "later",
            "<capability denied: env> (STRING_ORDER_DENIED)",
            "1e+21",
            "2",
            "1",
            "argument"
        ])
    );
    // Five visible logs, one denied env call and one successful conversion.
    assert_eq!(execution.store.data().host_calls, 7);
}
#[test]
fn primitive_string_preserves_lexical_functions_and_rejects_unimplemented_object_conversion() {
    for source in [
        "export function main():void{const String=(n:number):string=>'local';console.log(String(1));}",
        "function String(n:number):string{return 'local';}export function main():void{console.log(String(1));}",
    ] {
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&compile(source), &[])
                .unwrap()
                .lines,
            lines(&["local"])
        );
    }
    for statement in [
        "String({n:1});",
        "String([1]);",
        "String(Error('edge'));",
        "const n:number|{n:number}=1;String(n);",
        "String(...[1]);",
        "String<number>(1);",
        "new String(1);",
        "String(1);const String=(n:number):string=>'late';",
        "const f=String;f(1);",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                &format!("export function main():void{{{statement}}}"),
                LanguageProfile::CheckedF64Utf16V19
            )
            .is_err(),
            "{statement}"
        );
    }
    for &profile in varta_common::language::SUPPORTED_PROFILES
        .iter()
        .filter(|p| !p.primitive_string())
    {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{String(1);}",
                profile
            )
            .is_err(),
            "{profile:?}"
        );
    }
}
#[test]
fn primitive_string_host_limits_are_fatal_and_argument_throws_skip_conversion() {
    let bytes = compile(
        "export function main():void{try{String(1);}catch(error){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
    );
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    for expired in [false, true] {
        let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        if expired {
            execution.store.data_mut().deadline = Instant::now();
        } else {
            execution.store.data_mut().host_calls = abi::MAX_HOST_CALLS;
        }
        assert!(
            instance
                .get_typed_func::<(), ()>(&mut execution.store, "_start")
                .unwrap()
                .call(&mut execution.store, ())
                .is_err()
        );
        assert!(execution.store.data().output.lines.is_empty());
    }
    let bytes = compile(
        "function fail():number{throw Error('first');}export function main():void{try{String(fail(),console.log('BAD_LATER'));}catch({message}){console.log(message);}}",
    );
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    assert_eq!(execution.store.data().host_calls, 1);
    assert_eq!(execution.store.data().output.lines, lines(&["first"]));
}
#[test]
fn primitive_string_retains_roots_pairs_first_argument_and_saved_returns_through_gc() {
    let source = "function churn():number{let text='x';for(let i=0;i<14;i++){text=text+text;}for(let i=0;i<260;i++){const trash=[text+'!',text+'?'];trash.slice();}return 1;}function keep(first:{text:string},pair:VartaResult<string>,last:string):string{churn();console.log(pair.status);return first.text+last;}function saved():string{try{return String(1e21);}finally{String(churn());}}export function main():void{const first:string|number='saved'+'!';console.log(String(first,churn(),varta.result.capture(varta.env.get('STRING_GC_DENIED'))));console.log(keep({text:'root'+'!'},varta.result.capture(varta.env.get('STRING_PAIR_DENIED')),String(Number.MIN_VALUE,churn())));console.log(saved());console.log(varta.result.status());}";
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
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
            "<capability denied: env> (STRING_GC_DENIED)",
            "saved!",
            "<capability denied: env> (STRING_PAIR_DENIED)",
            "1",
            "root!5e-324",
            "1e+21",
            "1"
        ])
    );
}
#[test]
fn primitive_string_exact_maximum_string_is_owned_across_result_buffer_reuse() {
    let bytes = compile(
        "export function main():void{const input='x'.repeat(524288);const result=String(input);console.log(String(result.length));String('overwrite');console.log(result===input);console.log(result.charAt(result.length-1));}",
    );
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(1000 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    assert_eq!(
        execution.store.data().output.lines,
        lines(&["524288", "1", "x"])
    );
}
#[test]
fn primitive_string_rejects_downgraded_metadata_wrong_signatures_and_helper_reachability() {
    use crate::contract_tests::{export_index, rewrite};
    use varta_common::language::{LanguageContract, SCHEMA_VERSION};
    let bytes = compile("export function main():void{String(1);}");
    let runtime = VartaRuntime::new();
    for action in 0..4 {
        let bad = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 0
                && data.get(1..1 + abi::SECTION.len()) == Some(abi::SECTION.as_bytes())
                && action < 2
            {
                data[1 + abi::SECTION.len()..]
                    .copy_from_slice(&abi::NUMBER_TEXT_VERSION.to_le_bytes());
            }
            if id == 0
                && data.get(1..1 + varta_common::language::SECTION.len())
                    == Some(varta_common::language::SECTION.as_bytes())
                && action == 0
            {
                data.truncate(1 + varta_common::language::SECTION.len());
                data.extend(
                    LanguageContract {
                        schema_version: SCHEMA_VERSION,
                        profile: LanguageProfile::CheckedF64Utf16V18,
                    }
                    .to_bytes()
                    .unwrap(),
                );
            }
            if id == 1 && action == 2 {
                let signature = [0x60, 1, 0x7e, 1, 0x7f];
                let at = data
                    .windows(signature.len())
                    .position(|s| s == signature)
                    .unwrap();
                data[at + 2] = 0x7f;
            }
            if id == 2 && action == 3 {
                let name = b"primitive_to_string";
                let at = data.windows(name.len()).position(|s| s == name).unwrap();
                data[at] = b'P';
            }
            vec![data]
        });
        assert!(runtime.validate_module(&bad).is_err(), "action {action}");
    }
    let bytes = compile(
        "function fake(value:boolean):boolean{String(1);return value;}export function main():void{varta.json.parse<number>('1');}",
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
    assert!(
        runtime
            .validate_module(&bad)
            .unwrap_err()
            .to_string()
            .contains("cannot call host imports")
    );
}
