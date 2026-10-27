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
fn catch_patterns_read_error_names_messages_aliases_and_literal_keys() {
    let source = "export function main():void{try{throw TypeError('owned'+'😀');}catch({name,message:text}){console.log(name);console.log(text);text='changed';console.log(text);}try{throw RangeError();}catch({'name':kind,['message']:text}){console.log(kind);console.log(text);}try{throw SyntaxError('bad');}catch({message,name}){console.log(name);console.log(message);}try{throw Error('plain');}catch({name,message}){console.log(name);console.log(message);}}";
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
                "TypeError",
                "owned😀",
                "changed",
                "RangeError",
                "",
                "SyntaxError",
                "bad",
                "Error",
                "plain"
            ])
        );
    }
}
#[test]
fn catch_patterns_defaults_run_only_for_undefined_and_keep_left_to_right_order() {
    let out = run(
        "function fallback(text:string):string{console.log(text);return text;}export function main():void{try{throw 1;}catch({message=fallback('first'),name=fallback(message+' second')}){console.log(message);console.log(name);}try{throw Error();}catch({message=fallback('BAD_MESSAGE'),name=fallback('BAD_NAME')}){console.log(message);console.log(name);}try{throw false;}catch({message,name}){console.log(message);console.log(name);}try{throw 'text';}catch({name='string fallback'}){console.log(name);}try{throw NaN;}catch({message='number fallback'}){console.log(message);}}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "first",
            "first second",
            "first",
            "first second",
            "",
            "Error",
            "undefined",
            "undefined",
            "string fallback",
            "number fallback"
        ])
    );
}
#[test]
fn catch_patterns_nullish_failures_escape_the_same_handler_and_run_finally() {
    let out = run(
        "function attempt(value:null|undefined):void{try{try{throw value;}catch({message='BAD_DEFAULT'}){console.log('BAD_BODY');}finally{console.log('finally');}}catch(e){if(e instanceof TypeError){console.log(e.message);}}}export function main():void{attempt(null);attempt(undefined);try{throw true;}catch({}){console.log('empty');}try{try{throw null;}catch({}){console.log('BAD_EMPTY');}}catch(e){if(e instanceof TypeError){console.log(e.name);}}}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "finally",
            "Cannot destructure null or undefined",
            "finally",
            "Cannot destructure null or undefined",
            "empty",
            "TypeError"
        ])
    );
}
#[test]
fn catch_patterns_default_throws_replace_the_original_and_do_not_enter_body() {
    let out = run(
        "function fail():string{throw SyntaxError('replacement');}export function main():void{try{try{throw 1;}catch({message=fail(),name='BAD_LATER'}){console.log('BAD_BODY');}finally{console.log('finally');}}catch({name,message}){console.log(name);console.log(message);}try{throw TypeError('original');}catch({message}){try{throw RangeError('inner');}catch({message:text}){console.log(text);}console.log(message);}}",
    );
    assert_eq!(
        out.lines,
        lines(&["finally", "SyntaxError", "replacement", "inner", "original"])
    );
}
#[test]
fn catch_patterns_preserve_mutability_scope_shadowing_and_fresh_capture_cells() {
    let out = run(
        "export function main():void{const name='outer';let first:()=>string=()=>'';let second:()=>string=()=>'';for(let i=0;i<2;i=i+1){try{throw TypeError('caught');}catch({message='fallback',name:kind}){const read=():string=>{if(message!==undefined){return message;}return '';};if(i===0){first=read;message='first';}else{second=read;message='second';}console.log(kind);}}console.log(first());console.log(second());console.log(name);try{throw 1;}catch({message=name}){const name='body';console.log(message);console.log(name);}}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "TypeError",
            "TypeError",
            "first",
            "second",
            "outer",
            "outer",
            "body"
        ])
    );
}
#[test]
fn catch_patterns_route_named_branches_returns_pairs_and_finalizer_overrides() {
    let out = run(
        "function text():string{try{throw TypeError('saved');}catch({message=''}){return message;}finally{console.log('text finally');}}function pair():VartaResult<string>{try{throw 1;}catch({message=''}){return varta.result.capture(varta.env.get('VARTA_CATCH_PAIR'));}finally{try{throw Error('local');}catch({name}){console.log(name);}}}function override():string{try{throw Error('original');}catch({message=''}){return message;}finally{return 'override';}}export function main():void{console.log(text());outer:for(let i=0;i<3;i=i+1){try{throw i;}catch({message='branch'}){if(i===0){continue outer;}break outer;}finally{console.log('loop finally');}}console.log(pair().status);console.log(override());}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "text finally",
            "saved",
            "loop finally",
            "loop finally",
            "<capability denied: env> (VARTA_CATCH_PAIR)",
            "Error",
            "1",
            "override"
        ])
    );
}
#[test]
fn catch_patterns_return_inference_and_throwing_callbacks_include_initializers() {
    let out = run(
        "function replacement():string{throw TypeError('replacement');}export function main():void{console.log(JSON.stringify([1,2].map(n=>{try{throw n;}catch({message='fallback'}){return message;}})));try{[1].map(n=>{try{throw n;}catch({message=replacement()}){return message;}});}catch({message}){console.log(message);}}",
    );
    assert_eq!(
        out.lines,
        lines(&["[\"fallback\",\"fallback\"]", "replacement"])
    );
}
#[test]
fn catch_patterns_reject_tdz_duplicates_annotations_and_unadmitted_shapes() {
    for source in [
        "export function main():void{try{throw 1;}catch({message=name,name}){}}",
        "export function main():void{try{throw 1;}catch({message=message}){}}",
        "export function main():void{try{throw 1;}catch({name:same,message:same}){}}",
        "export function main():void{try{throw 1;}catch({name}){let name='duplicate';}}",
        "export function main():void{try{throw 1;}catch({name}:string){}}",
        "export function main():void{try{throw 1;}catch([value]){}}",
        "export function main():void{try{throw 1;}catch({...rest}){}}",
        "export function main():void{try{throw 1;}catch({length}){}}",
        "export function main():void{const key='message';try{throw 1;}catch({[key]:value}){}}",
        "export function main():void{try{throw 1;}catch({message}){const n:number=message;}}",
        "export function main():void{try{throw 1;}catch({message}){const n:string=message;}}",
        "export function main():void{try{throw {message:'aggregate'};}catch({message}){}}",
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
        LanguageProfile::CheckedF64Utf16V7,
        LanguageProfile::CheckedF64Utf16V11,
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{try{throw 1;}catch({message}){}}",
                profile
            )
            .is_err()
        );
    }
}
#[test]
fn catch_patterns_gc_preserves_sources_initialized_leaves_saved_returns_and_captures() {
    let source = "function churn():string{let text='x';for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+'!',text+'?'];garbage.slice();}return 'fallback'+'!';}function saved():string{try{throw TypeError('owned'+'\\ud800');}catch({message='',name=churn()}){churn();return message;}finally{churn();}}function closure():()=>string{try{throw 1;}catch({message=churn(),name=churn()}){return ()=>message+name;}}export function main():void{console.log(JSON.stringify(saved()));const read=closure();churn();console.log(read());let count=0;for(let i=0;i<6000;i=i+1){try{throw Error('owned'+'!');}catch({message='',name}){count=count+message.length;}}console.log(count);}";
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
        lines(&["\"owned\\ud800\"", "fallback!fallback!", "36000"])
    );
}
#[test]
fn catch_patterns_bound_names_share_the_existing_pattern_budget() {
    let holes = ",".repeat(varta_common::function::MAX_BINDING_PATTERN_NODES + 1);
    let source =
        format!("export function main():void{{try{{throw 1;}}catch({{message:[{holes}]}}){{}}}}");
    let error = varta_compiler::compile_typescript_with_profile(
        &source,
        LanguageProfile::CheckedF64Utf16V14,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("binding pattern budget exceeded"),
        "{error}"
    );
}
#[test]
fn catch_patterns_fatal_default_budgets_bypass_the_handler_body_and_finally() {
    let source = "function forever():string{while(true){}return '';}export function main():void{try{throw 1;}catch({message=forever()}){console.log('BAD_BODY');}finally{console.log('BAD_FINALLY');}}";
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
#[test]
fn catch_patterns_default_effects_remain_visible_to_helper_purity_validation() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function fallback():string{console.log('effect');return 'fallback';}function fake(value:boolean):boolean{try{throw 1;}catch({message=fallback()}){}return value;}export function main():void{varta.json.parse<number>('1');}",
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
