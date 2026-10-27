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
fn string_ranges_normalize_omitted_undefined_fractional_nan_and_infinite_bounds() {
    let cases = [
        ("", "abcdef", "abcdef"),
        ("3", "def", "def"),
        ("-2", "ef", "abcdef"),
        ("2,4", "cd", "cd"),
        ("4,2", "", "cd"),
        ("-4,-1", "cde", ""),
        ("-4,4", "cd", "abcd"),
        ("2,-1", "cde", "ab"),
        ("-100,100", "abcdef", "abcdef"),
        ("Infinity", "", ""),
        ("-Infinity", "abcdef", "abcdef"),
        ("NaN", "abcdef", "abcdef"),
        ("undefined,undefined", "abcdef", "abcdef"),
        ("3,undefined", "def", "def"),
        ("undefined,NaN", "", ""),
        ("undefined,2", "ab", "ab"),
        ("1.9,4.9", "bcd", "bcd"),
        ("-1.9", "f", "abcdef"),
        ("-0.9,2.9", "ab", "ab"),
        ("Infinity,-Infinity", "", "abcdef"),
        ("1e300,-1e300", "", "abcdef"),
        ("NaN,Infinity", "abcdef", "abcdef"),
        ("-Infinity,-1", "abcde", ""),
        ("2,NaN", "", "ab"),
    ];
    let mut source = String::from("export function main():void{");
    let mut expected = Vec::new();
    for (args, slice, substring) in cases {
        source.push_str(&format!(
            "console.log('abcdef'.slice({args}));console.log('abcdef'.substring({args}));"
        ));
        expected.extend([
            format!("varta log: {slice}"),
            format!("varta log: {substring}"),
        ]);
    }
    source.push('}');
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        let bytes = varta_compiler::compile_typescript_with_profile(&source, profile).unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            expected,
            "{profile:?}"
        );
    }
}
#[test]
fn string_ranges_preserve_utf16_code_units_lone_surrogates_and_nuls() {
    let out = run(
        "export function main():void{const s='A😀B\\u0000\\ud800Z';console.log(s.length);console.log(JSON.stringify(s.slice(1,2)));console.log(JSON.stringify(s.substring(2,1)));console.log(JSON.stringify(s.slice(2,3)));console.log(JSON.stringify(s.substring(1,3)));console.log(JSON.stringify(s.slice(4,6)));console.log(JSON.stringify(s.substring(6,4)));console.log(JSON.stringify(s.slice(-2)));console.log(''.slice(-Infinity,Infinity).length);console.log(''.substring(Infinity,-Infinity).length);console.log(s.slice(1,2).charCodeAt(0));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "7",
            "\"\\ud83d\"",
            "\"\\ud83d\"",
            "\"\\ude00\"",
            "\"😀\"",
            "\"\\u0000\\ud800\"",
            "\"\\u0000\\ud800\"",
            "\"\\ud800Z\"",
            "0",
            "0",
            "55357"
        ])
    );
}
#[test]
fn string_ranges_evaluate_receiver_start_end_once_in_order_even_for_empty_results() {
    let out = run(
        "function source():string{console.log('receiver');return 'abcdef';}function bound(label:string,n:number):number{console.log(label);return n;}function missing(label:string):undefined{console.log(label);return undefined;}export function main():void{console.log(source().slice(bound('start',4),bound('end',2)));console.log(source().substring(bound('start',4),bound('end',2)));console.log(''.slice(missing('empty start'),missing('empty end')));console.log(source().slice(missing('missing start'),missing('missing end')));console.log(source().substring(bound('outer start',source().slice(1,3).length),bound('outer end',5)));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "receiver",
            "start",
            "end",
            "",
            "receiver",
            "start",
            "end",
            "cd",
            "empty start",
            "empty end",
            "",
            "receiver",
            "missing start",
            "missing end",
            "abcdef",
            "receiver",
            "receiver",
            "outer start",
            "outer end",
            "cde"
        ])
    );
}
#[test]
fn string_ranges_union_bounds_narrowed_receivers_and_callback_inference_keep_types() {
    let out = run(
        "function source():string|null{return 'abcdef';}function end(present:boolean):number|undefined{if(present){return 4;}return undefined;}export function main():void{const text=source();if(text!==null){console.log(text.slice(undefined,end(true)));console.log(text.substring(2,end(false)));}console.log(JSON.stringify(['abc','def'].map(s=>s.slice(1))));console.log(JSON.stringify(['abc','def'].map(s=>s.substring(2,1))));console.log('abc'.slice(void 1));}",
    );
    assert_eq!(
        out.lines,
        lines(&["abcd", "cdef", "[\"bc\",\"ef\"]", "[\"b\",\"e\"]", "abc"])
    );
}
#[test]
fn string_ranges_argument_throws_skip_later_arguments_and_enclosing_effects() {
    let out = run(
        "function source():string{console.log('receiver');return 'abc';}function bad():number{console.log('bad start');throw TypeError('bound');}function later():number{console.log('BAD_END');return 2;}function consume(first:string,second:string):void{console.log('BAD_BODY');}function after():string{console.log('BAD_AFTER');return '';}export function main():void{try{consume(source().slice(bad(),later()),after());}catch({message}){console.log(message);}finally{console.log('finally');}try{source().substring(0,varta.json.parse<number>('{'));}catch({name}){console.log(name);}}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "receiver",
            "bad start",
            "bound",
            "finally",
            "receiver",
            "SyntaxError"
        ])
    );
}
#[test]
fn string_ranges_shared_slice_dispatch_preserves_array_semantics_and_effects() {
    let out = run(
        "export function main():void{const source:number[]=[1,2,3];const read=():number[]=>{console.log('array source');return source;};console.log(JSON.stringify(read().slice(-2)));console.log('abcdef'.slice(-2));console.log(JSON.stringify(source.slice(2,1)));console.log('abcdef'.substring(2,1));const sparse:number[]=[1,,3];console.log(Object.hasOwn(sparse.slice(),1));console.log(JSON.stringify([source].map(a=>a.slice(1))));}",
    );
    assert_eq!(
        out.lines,
        lines(&["array source", "[2,3]", "ef", "[]", "b", "0", "[[2,3]]"])
    );
    let legacy = varta_compiler::compile_typescript_with_profile(
        "export function main():void{console.log(JSON.stringify([1,2,3].slice(1)));}",
        LanguageProfile::CheckedF64Utf16V6,
    )
    .unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&legacy, &[])
            .unwrap()
            .lines,
        lines(&["[2,3]"])
    );
}
#[test]
fn string_ranges_maximum_owned_outputs_fit_existing_byte_limits() {
    let out = run_with_budget(
        "export function main():void{const text='x'.repeat(524288);const first=text.slice();const second=first.substring(Infinity,-Infinity);console.log(first.length);console.log(second.length);console.log(second.charAt(524287));console.log(second.slice(524288).length);console.log(second.substring(524287,524288));}",
        10 * abi::FUEL,
    );
    assert_eq!(out.lines, lines(&["524288", "524288", "x", "0", "x"]));
}
#[test]
fn string_ranges_reject_coercion_arity_bad_receivers_and_earlier_profiles() {
    for source in [
        "export function main():void{'abc'.slice(null);}",
        "export function main():void{'abc'.substring(true);}",
        "export function main():void{'abc'.slice('1');}",
        "export function main():void{'abc'.substring(0,{});}",
        "export function main():void{'abc'.slice(0,1,2);}",
        "export function main():void{'abc'.substring<number>(1);}",
        "export function main():void{'abc'.slice(...[1]);}",
        "export function main():void{const s:string|null='abc';s.substring(1);}",
        "export function main():void{[1].substring(1);}",
        "export function main():void{1..slice(1);}",
        "export function main():void{const n:number='abc'.slice(1);}",
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
        LanguageProfile::CheckedF64Utf16V1,
        LanguageProfile::CheckedF64Utf16V11,
    ] {
        for method in ["slice", "substring"] {
            assert!(
                varta_compiler::compile_typescript_with_profile(
                    &format!("export function main():void{{'abc'.{method}(1);}}"),
                    profile
                )
                .is_err()
            );
        }
    }
}
fn run_with_budget(source: &str, fuel: u64) -> RunOutput {
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(fuel).unwrap();
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
fn string_ranges_gc_keeps_detached_receivers_bounds_pairs_children_and_saved_returns() {
    let out = run_with_budget(
        "function churn():number{let text='x';for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+'!',text+'?'];garbage.slice();}return 8;}function make():string{let source='prefix'+'😀suffix';const result=source.slice(6,churn());source='replacement';churn();return result;}function keep(first:{text:string},pair:VartaResult<string>,last:string):string{churn();console.log(pair.status);return first.text+last;}function saved():string{try{return make().substring(0,2);}finally{churn();}}export function main():void{console.log(keep({text:'earlier'+'!'},varta.result.capture(varta.env.get('VARTA_STRING_RANGE_GC')),make()));console.log(saved());const record={text:make()};const array:string[]=[make().substring()];churn();const selected=array[0];if(selected!==undefined){console.log(record.text+selected);}let count=0;for(let i=0;i<7000;i=i+1){count=count+('owned'+'!').slice(1,4).substring().length;}console.log(count);}",
        300 * abi::FUEL,
    );
    assert_eq!(
        out.lines,
        lines(&[
            "<capability denied: env> (VARTA_STRING_RANGE_GC)",
            "1",
            "earlier!😀",
            "😀",
            "😀😀",
            "21000"
        ])
    );
}
#[test]
fn string_ranges_bound_mutation_keeps_the_original_receiver_and_start_value() {
    let out = run(
        "export function main():void{let text='abcdef';let start:number|undefined=2;const finish=():number=>{text='replacement';start=0;console.log('end');return 4;};console.log(text.slice(start,finish()));console.log(text);console.log(start);console.log(('ab'+'cdef').substring('12'.slice(1).length,'abcde'.substring(0,4).length));}",
    );
    assert_eq!(out.lines, lines(&["end", "cd", "replacement", "0", "bcd"]));
}
#[test]
fn string_ranges_copy_fuel_exhaustion_stays_fatal_and_bypasses_catch_and_finally() {
    let bytes = compile(
        "export function main():void{const source='x'.repeat(200000);console.log('before');try{for(let i=0;i<10;i=i+1){source.slice().substring();}}catch(e){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
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
    assert_eq!(execution.store.data().output.lines, lines(&["before"]));
}
#[test]
fn string_ranges_operand_effects_remain_visible_to_helper_purity_validation() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function source():string{console.log('source');return 'abc';}function bound():number{console.log('bound');return 2;}function fake(value:boolean):boolean{source().slice(0,bound()).substring(bound());return value;}export function main():void{varta.json.parse<number>('1');}",
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
