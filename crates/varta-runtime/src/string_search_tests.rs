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
fn string_searches_match_exact_substrings_prefixes_suffixes_and_empty_needles() {
    let source = "export function main():void{console.log('abcabc'.includes('bca'));console.log('abcabc'.includes('bca',2));console.log('abcabc'.includes('bc',4));console.log('abc'.includes('abcd'));console.log('abc'.startsWith('ab'));console.log('abc'.startsWith('bc',1));console.log('abc'.startsWith('bc'));console.log('abc'.endsWith('bc'));console.log('abc'.endsWith('ab',2));console.log('abc'.endsWith('ab'));console.log('abc'.includes('',Infinity));console.log('abc'.startsWith('',Infinity));console.log('abc'.endsWith('',-Infinity));console.log(''.includes(''));console.log(''.startsWith(''));console.log(''.endsWith(''));console.log(''.includes('x'));console.log(''.startsWith('x'));console.log(''.endsWith('x'));console.log('abc'.includes('ABC'));console.log('aaaaab'.includes('aaab'));}";
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
                "1", "0", "1", "0", "1", "1", "0", "1", "1", "0", "1", "1", "1", "1", "1", "1",
                "0", "0", "0", "0", "1"
            ])
        );
    }
}
#[test]
fn string_searches_normalize_positions_without_relative_negative_indexing() {
    let cases = [
        ("undefined", "1", "1", "1"),
        ("NaN", "1", "1", "0"),
        ("-Infinity", "1", "1", "0"),
        ("Infinity", "0", "0", "1"),
        ("-1.9", "1", "1", "0"),
        ("0.9", "1", "1", "0"),
        ("1.9", "0", "0", "0"),
        ("2.9", "0", "0", "0"),
        ("3.9", "0", "0", "0"),
        ("4.9", "0", "0", "1"),
        ("1e300", "0", "0", "1"),
        ("-1e300", "1", "1", "0"),
        ("-0", "1", "1", "0"),
    ];
    let mut source = String::from("export function main():void{");
    let mut expected = Vec::new();
    for (position, includes, starts, ends) in cases {
        source.push_str(&format!("console.log('abcd'.includes('a',{position}));console.log('abcd'.startsWith('a',{position}));console.log('abcd'.endsWith('d',{position}));"));
        expected.extend([includes, starts, ends].map(|s| format!("varta log: {s}")));
    }
    source.push('}');
    assert_eq!(run(&source).lines, expected);
}
#[test]
fn string_searches_preserve_utf16_units_surrogates_nuls_and_unit_alignment() {
    let out = run(
        "export function main():void{const text='A😀B\\u0000\\ud800Z';console.log(text.includes('😀'));console.log(text.includes('\\ud83d',1));console.log(text.includes('\\ude00',2));console.log(text.startsWith('\\ude00',2));console.log(text.endsWith('\\ud83d',2));console.log(text.includes('\\u0000\\ud800'));console.log(text.endsWith('\\ud800Z'));console.log(text.startsWith('😀',2));console.log('\\u0100\\u0002'.includes('\\u0201'));console.log('é'.includes('é'));console.log('Привіт'.startsWith('При'));console.log('Привіт'.endsWith('віт'));}",
    );
    assert_eq!(
        out.lines,
        lines(&["1", "1", "1", "1", "1", "1", "1", "0", "0", "0", "1", "1"])
    );
}
#[test]
fn string_searches_evaluate_receiver_needle_position_once_even_for_obvious_results() {
    let out = run(
        "function source():string{console.log('source');return 'abc';}function needle(text:string):string{console.log('needle');return text;}function position():number{console.log('position');return 1;}export function main():void{console.log(source().includes(needle('bc'),position()));console.log(source().startsWith(needle(''),position()));console.log(source().endsWith(needle('too long'),position()));console.log(source().includes(needle('x'),source().slice(1).length));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "source", "needle", "position", "1", "source", "needle", "position", "1", "source",
            "needle", "position", "0", "source", "needle", "source", "0"
        ])
    );
}
#[test]
fn string_searches_keep_prior_operands_across_mutating_positions_and_union_defaults() {
    let out = run(
        "function absent():number|undefined{return undefined;}export function main():void{let text='abcdef';let needle='bc';const position=():number=>{text='changed';needle='changed';return 1;};console.log(text.includes(needle,position()));console.log(text);console.log(needle);console.log('abcdef'.endsWith('ef',absent()));console.log('abcdef'.startsWith('ab',absent()));console.log('abcdef'.includes('cd',absent()));const maybe:string|null='abc';if(maybe!==null){console.log(maybe.startsWith('a'));}console.log(JSON.stringify(['abc','xyz'].map(s=>s.endsWith('c'))));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "1",
            "changed",
            "changed",
            "1",
            "1",
            "1",
            "1",
            "[true,false]"
        ])
    );
}
#[test]
fn string_searches_operand_throws_skip_later_and_enclosing_effects() {
    let out = run(
        "function source():string{console.log('source');return 'abc';}function badNeedle():string{throw TypeError('needle');}function badPosition():number{throw RangeError('position');}function later():number{console.log('BAD_POSITION');return 0;}function consume(a:boolean,b:number):void{console.log('BAD_BODY');}export function main():void{try{consume(source().includes(badNeedle(),later()),later());}catch({message}){console.log(message);}finally{console.log('finally');}try{consume(source().startsWith('a',badPosition()),later());}catch({message}){console.log(message);}try{source().endsWith('a',varta.json.parse<number>('{'));}catch({name}){console.log(name);}}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "source",
            "needle",
            "finally",
            "source",
            "position",
            "source",
            "SyntaxError"
        ])
    );
}
#[test]
fn string_searches_shared_includes_dispatch_keeps_array_rules_and_old_profiles() {
    let out = run(
        "export function main():void{const source=():number[]=>{console.log('array source');return [1,2,3];};console.log(source().includes(2));console.log([NaN].includes(NaN));console.log(['abc'].includes('b'));console.log('abc'.includes('b'));const sparse:number[]=[,1];console.log(sparse.includes(undefined));console.log(sparse.includes(1,-1));console.log('abc'.includes('a',-1));}",
    );
    assert_eq!(
        out.lines,
        lines(&["array source", "1", "1", "0", "1", "1", "1", "1"])
    );
    let bytes = varta_compiler::compile_typescript_with_profile(
        "export function main():void{console.log([1,2,3].includes(2));}",
        LanguageProfile::CheckedF64Utf16V6,
    )
    .unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap()
            .lines,
        lines(&["1"])
    );
}
#[test]
fn string_searches_reject_coercion_bad_arity_receivers_and_earlier_profiles() {
    for source in [
        "export function main():void{'abc'.includes();}",
        "export function main():void{'abc'.startsWith(undefined);}",
        "export function main():void{'abc'.endsWith(1);}",
        "export function main():void{'abc'.includes('a',null);}",
        "export function main():void{'abc'.startsWith('a','1');}",
        "export function main():void{'abc'.endsWith('a',0,1);}",
        "export function main():void{'abc'.includes<number>('a');}",
        "export function main():void{'abc'.startsWith(...['a']);}",
        "export function main():void{const s:string|null='abc';s.endsWith('c');}",
        "export function main():void{1..includes('1');}",
        "export function main():void{const s:string='abc'.includes('a');}",
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
        for method in ["includes", "startsWith", "endsWith"] {
            assert!(
                varta_compiler::compile_typescript_with_profile(
                    &format!("export function main():void{{'abc'.{method}('a');}}"),
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
fn string_searches_gc_preserves_detached_receivers_needles_pairs_and_saved_booleans() {
    let out = run_with_budget(
        "function churn():number{let text='x';for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+'!',text+'?'];garbage.slice();}return 1;}function source():string{return 'prefix'+'😀suffix';}function needle():string{churn();return '😀'+'suffix';}function keep(first:{text:string},pair:VartaResult<string>,last:boolean):boolean{churn();console.log(first.text);console.log(pair.status);return last;}function saved():boolean{try{return source().endsWith(needle(),undefined);}finally{churn();}}export function main():void{console.log(keep({text:'earlier'+'!'},varta.result.capture(varta.env.get('VARTA_STRING_SEARCH_GC')),source().includes(needle(),churn())));console.log(saved());let count=0;for(let i=0;i<7000;i=i+1){if(('owned'+'!').includes('ned',1)&&('owned'+'!').startsWith('own')&&('owned'+'!').endsWith('!')){count=count+1;}}console.log(count);}",
        300 * abi::FUEL,
    );
    assert_eq!(
        out.lines,
        lines(&[
            "<capability denied: env> (VARTA_STRING_SEARCH_GC)",
            "earlier!",
            "1",
            "1",
            "1",
            "7000"
        ])
    );
}
#[test]
fn string_searches_maximum_inputs_keep_existing_byte_bounds() {
    let units = abi::MAX_STRING_BYTES / size_of::<u16>();
    let source = format!(
        "export function main():void{{const text='x'.repeat({units});console.log(text.startsWith('x'));console.log(text.endsWith('x'));console.log(text.includes('y',text.length-1));console.log(text.includes('x',Infinity));console.log(text.startsWith(text));console.log(text.endsWith(text));console.log(text.includes(text));}}"
    );
    assert_eq!(
        run_with_budget(&source, 100 * abi::FUEL).lines,
        lines(&["1", "1", "0", "0", "1", "1", "1"])
    );
}
#[test]
fn string_searches_adversarial_scan_fuel_exhaustion_stays_fatal() {
    let bytes = compile(
        "export function main():void{const text='a'.repeat(2000);const needle='a'.repeat(999)+'b';console.log('before');try{text.includes(needle);}catch(e){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
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
fn string_searches_argument_effects_remain_visible_to_helper_purity_validation() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function source():string{console.log('source');return 'abc';}function needle():string{console.log('needle');return 'a';}function position():number{console.log('position');return 0;}function fake(value:boolean):boolean{source().includes(needle(),position());source().startsWith(needle(),position());source().endsWith(needle(),position());return value;}export function main():void{varta.json.parse<number>('1');}",
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
