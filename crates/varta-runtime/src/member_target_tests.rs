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
fn member_targets_write_nested_fields_array_elements_and_keep_rhs_identity() {
    let output = run(r#"
export function main():void{const target={field:0,child:{n:0}};const array:number[]=[];const source={a:1,child:{n:2}};const result=({a:target.field,child:{n:target.child.n}}=source);[array[2]]= [3];console.log(JSON.stringify(target));console.log(JSON.stringify(array));console.log(Object.hasOwn(array,0));console.log(Object.hasOwn(array,2));console.log(result===source);const optional:{n?:number}={};({value:optional.n}={value:4});console.log(Object.hasOwn(optional,'n'));console.log(JSON.stringify(optional));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "{\"field\":1,\"child\":{\"n\":2}}",
            "[null,null,3]",
            "0",
            "1",
            "1",
            "1",
            "{\"n\":4}"
        ])
    );
}
#[test]
fn member_references_evaluate_base_and_key_once_before_defaults_and_pin_identity() {
    let output = run(r#"
export function main():void{let current={n:0};const original=current;const base=():{n:number}=>{console.log("base");return current;};const fallback=():number=>{console.log("default");current={n:99};return 7;};({missing:base().n=fallback()}={});console.log(original.n);console.log(current.n);let destination:number[]=[0,0];const previous=destination;let index=0;const select=():number[]=>{console.log("array");return destination;};const key=():number=>{console.log("key");return index;};const replace=():number=>{console.log("array default");destination=[9,9];index=1;return 4;};const empty:number[]=[];[select()[key()]=replace()]=empty;console.log(JSON.stringify(previous));console.log(JSON.stringify(destination));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "base",
            "default",
            "7",
            "99",
            "array",
            "key",
            "array default",
            "[4,0]",
            "[9,9]"
        ])
    );
}
#[test]
fn member_reference_effects_precede_source_property_reads_and_iterator_steps() {
    let output = run(r#"
export function main():void{const source:{n?:number}={};const destination={n:0};const receiver=():{n:number}=>{source.n=4;return destination;};const bad=():number=>{console.log("BAD");return 1;};({n:receiver().n=bad()}=source);console.log(destination.n);const input:number[]=[1,2];const output:number[]=[];let second=0;const key=():number=>{input.length=0;return 0;};const grow=():number=>{input.push(9,10);return 7;};[output[key()]=grow(),second=8]=input;console.log(output[0]);console.log(second);console.log(JSON.stringify(input));}
"#);
    assert_eq!(output.lines, lines(&["4", "7", "8", "[9,10]"]));
}
#[test]
fn rest_member_targets_are_prepared_before_copy_and_keep_presence_and_order() {
    let output = run(r#"
export function main():void{let removed=0;const source:{remove:number;a?:number;b:number}={remove:0,b:1};const holder:{rest:{a?:number;b:number}}={rest:{b:0}};const receiver=():{rest:{a?:number;b:number}}=>{source.a=2;source.b=3;return holder;};({remove:removed,...receiver().rest}=source);console.log(JSON.stringify(holder.rest));console.log(Object.hasOwn(holder.rest,'a'));const input:number[]=[1,2,3];const arrays:{values:(number|undefined)[]}={values:[]};const selected=():{values:(number|undefined)[]}=>{input.push(4);return arrays;};let first=0;const original=([first,...selected().values]=input);console.log(first);console.log(JSON.stringify(arrays.values));console.log(original===input);const nested:(number|undefined)[][]=[];[...nested[0]]=input;console.log(JSON.stringify(nested));}
"#);
    assert_eq!(
        output.lines,
        lines(&["{\"b\":3,\"a\":2}", "1", "1", "[2,3,4]", "1", "[[1,2,3,4]]"])
    );
}
#[test]
fn member_target_failures_preserve_prior_writes_and_skip_later_references() {
    let output = run(r#"
function fail():number{console.log("default");throw SyntaxError("failed");}
export function main():void{const output={a:0,b:9,c:8};const later=():{a:number;b:number;c:number}=>{console.log("BAD");return output;};try{({a:output.a,b:output.b=fail(),c:later().c}={a:1,c:2});}catch(e){if(e instanceof SyntaxError){console.log(e.message);}}console.log(JSON.stringify(output));const values:number[]=[0,9];const input:number[]=[1];try{[values[0],values[1]]=input;}catch(e){if(e instanceof TypeError){console.log(e.name);}}finally{console.log(JSON.stringify(values));}}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "default",
            "failed",
            "{\"a\":1,\"b\":9,\"c\":8}",
            "TypeError",
            "[1,9]"
        ])
    );
}
#[test]
fn member_length_targets_use_existing_range_errors_and_backing_mutation_rules() {
    let output = run(r#"
export function main():void{const values:number[]=[1,2,3];const empty:number[]=[];[values.length]=[1];console.log(JSON.stringify(values));[values.length=3]=empty;console.log(JSON.stringify(values));let after=9;try{[values.length,after]=[-1,2];}catch(e){if(e instanceof RangeError){console.log(e.name);}}console.log(after);console.log(values.length);const input:number[]=[7,8];let second=0;[input[1],second]=input;console.log(second);console.log(JSON.stringify(input));}
"#);
    assert_eq!(
        output.lines,
        lines(&["[1]", "[1,null,null]", "RangeError", "9", "3", "7", "[7,7]"])
    );
}
#[test]
fn member_targets_keep_type_and_immutable_boundary_diagnostics() {
    for source in [
        "export function main():void{const target={n:0};({n:target.n}={n:'bad'});}",
        "export function main():void{const target=Error('x');({message:target.message}={message:'bad'});}",
        "export function main():void{const target={n:0};const key='n';({n:target[key]}={n:1});}",
        "export function main():void{const target:number[]=[];[target['0']]=[1];}",
        "export function main():void{let target:{n:number}|null=null;({n:target.n}={n:1});}",
        "export function main():void{const target={n:0};const {n:target.n}={n:1};}",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V14,
        )
        .unwrap_err();
        assert!(error.diagnostic().is_some(), "{source}: {error}");
    }
    let source = "export function main():void{const target={n:0};({n:target.n}={n:'bad'});}";
    let error = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V14,
    )
    .unwrap_err();
    assert_eq!(
        error.diagnostic().unwrap().range.start.byte_offset,
        source.find("target.n").unwrap()
    );
}
#[test]
fn throwing_receivers_and_keys_skip_defaults_rest_copy_and_later_targets() {
    let output = run(r#"
function fail():number{console.log("key");throw SyntaxError("reference");}
export function main():void{const output:number[]=[0];const empty:number[]=[];const fallback=():number=>{console.log("BAD_DEFAULT");return 1;};let later=9;try{[output[fail()]=fallback(),later]=empty;}catch(e){if(e instanceof SyntaxError){console.log(e.message);}}console.log(JSON.stringify(output));console.log(later);const holder={rest:{n:0}};const receiver=():{rest:{n:number}}=>{console.log("receiver");throw TypeError("rest");};try{({...receiver().rest}={n:1});}catch(e){if(e instanceof TypeError){console.log(e.message);}}console.log(JSON.stringify(holder));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "key",
            "reference",
            "[0]",
            "9",
            "receiver",
            "rest",
            "{\"rest\":{\"n\":0}}"
        ])
    );
}
#[test]
fn prepared_array_reference_uses_current_backing_after_default_resize() {
    let output = run(r#"
export function main():void{const output:number[]=[0];const empty:number[]=[];const grow=():number=>{output.push(2,3,4);return 7;};[output[0]=grow()]=empty;console.log(JSON.stringify(output));const nested={output:{n:0}};const previous=nested.output;const replace=():number=>{nested.output={n:99};return 8;};({missing:nested.output.n=replace()}={});console.log(previous.n);console.log(nested.output.n);const set=function(this:{n:number},value:number):void{({n:this.n}={n:value});};const methods={n:0};set.call(methods,4);console.log(methods.n);}
"#);
    assert_eq!(output.lines, lines(&["[7,2,3,4]", "8", "99", "4"]));
}
#[test]
fn member_targets_keep_profile_specific_writes_and_pattern_gates() {
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        let source = "export function main():void{const output={n:0};const values:number[]=[0];({n:output.n}={n:3});[values[0]]=[4];console.log(output.n);console.log(values[0]);}";
        let bytes = varta_compiler::compile_typescript_with_profile(source, profile).unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            lines(&["3", "4"])
        );
    }
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf16V11,
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{const output={n:0};({n:output.n}={n:1});}",
                profile
            )
            .is_err()
        );
    }
    for profile in [
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                "export function main():void{const output:number[]=[1];[output.length]=[0];}",
                profile
            )
            .is_err()
        );
        let bytes = varta_compiler::compile_typescript_with_profile(
            "export function main():void{const output:number[]=[1];[output[1]]=[2];}",
            profile,
        )
        .unwrap();
        assert!(VartaRuntime::new().run_wasm_bytes(&bytes, &[]).is_err());
    }
    let bytes=varta_compiler::compile_typescript_with_profile("export function main():void{const output:number[]=[,];[output[0]]=[2];console.log(Object.hasOwn(output,0));console.log(output[0]);}",LanguageProfile::CheckedF64Utf16V13).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap()
            .lines,
        lines(&["1", "2"])
    );
}
#[test]
fn prepared_receivers_remain_rooted_across_key_and_default_collection() {
    let source = r#"
function churn():number{let text="x";for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+"!",text+"?"];garbage.slice();}return 0;}
function temporary():{text:string}{return {text:"temporary"+"!"};}
function keep(first:{text:string},pair:VartaResult<string>,last:{}):string{churn();console.log(pair.status);return first.text+JSON.stringify(last);}
export function main():void{let current={text:"original"+"!"};const receiver=():{text:string}=>{const previous=current;current={text:"replacement"};return previous;};const fallback=():string=>{churn();return "assigned"+"!";};console.log(keep(temporary(),varta.result.capture(varta.env.get("VARTA_MEMBER_GC")),({missing:receiver().text=fallback()}={})));console.log(current.text);const empty:number[]=[];const make=():string[]=>["old"+"!"];const key=():number=>churn();const values:string[]=[];[make()[key()]=fallback()]=values;const holder={rest:{text:"old"}};({...holder.rest}={text:fallback()});churn();console.log(holder.rest.text);}
"#;
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
            "<capability denied: env> (VARTA_MEMBER_GC)",
            "1",
            "temporary!{}",
            "replacement",
            "assigned!"
        ])
    );
}
#[test]
fn member_target_effects_remain_visible_to_host_free_helper_validation() {
    use crate::contract_tests::{export_index, rewrite};
    for assignment in [
        "({n:receiver().n}={n:1})",
        "([output[key()]]=[1])",
        "({...receiver().rest}={n:1})",
    ] {
        let bytes = compile(&format!(
            "function receiver():{{n:number;rest:{{n:number}}}}{{console.log('effect');return {{n:0,rest:{{n:0}}}};}}function key():number{{console.log('effect');return 0;}}function fake(value:boolean):boolean{{const output:number[]=[0];{assignment};return value;}}export function main():void{{varta.json.parse<number>('1');}}"
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
fn member_target_bounds_and_invalid_keys_stay_fatal_after_rhs_effects() {
    for target in ["output[1024]", "output[-1]", "output[0.5]", "output.length"] {
        let fallback = if target == "output.length" {
            "1025"
        } else {
            "2"
        };
        let bytes = compile(&format!(
            "export function main():void{{const output:number[]=[0];const empty:number[]=[];const fallback=():number=>{{console.log('default');return {fallback};}};try{{[{target}=fallback()]=empty;}}catch(e){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"
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
                .is_err(),
            "{target}"
        );
        assert_eq!(
            execution.store.data().output.lines,
            lines(&["default"]),
            "{target}"
        );
    }
    let output = run(
        "export function main():void{const output:number[]=[];[output[1023]]=[7];for(let i=0;i<100;i=i+1){[output[i]]=[i];}console.log(output.length);console.log(output[99]);console.log(output[1023]);}",
    );
    assert_eq!(output.lines, lines(&["1024", "99", "7"]));
}
#[test]
fn rhs_precedes_references_and_nested_patterns_acquire_their_source_first() {
    let output = run(r#"
export function main():void{const destination={n:0};const source={inner:{n:1}};const rhs=():{inner:{n:number}}=>{console.log("rhs");return source;};const receiver=():{n:number}=>{console.log("receiver");source.inner={n:2};return destination;};const result=({inner:{n:receiver().n}}=rhs());console.log(destination.n);console.log(result===source);console.log(source.inner.n);const values:number[]=[0];const base=():number[]=>{console.log("base");return values;};const key=():number=>{console.log("key");return 0;};const input=():number[]=>{console.log("input");return [3];};[base()[key()]]=input();console.log(values[0]);const target:{items:number[]}={items:[1]};({missing:target.items=[]}={});console.log(target.items.length);}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "rhs", "receiver", "1", "1", "2", "input", "base", "key", "3", "0"
        ])
    );
}
