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
fn index_extension_preserves_identity_and_creates_holes() {
    let output = run(r#"
export function main():void{const values:number[]=[];const alias=values;values[3]=4;console.log(values===alias);console.log(values.length);console.log(JSON.stringify(alias));console.log(Object.hasOwn(values,0));console.log(Object.hasOwn(values,3));values[1]=2;console.log(JSON.stringify(values));values[8]=9;console.log(values.length);console.log(values[7]);console.log(values.at(-1));console.log(Object.hasOwn(values,3));console.log(JSON.stringify(values));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "1",
            "4",
            "[null,null,null,4]",
            "0",
            "1",
            "[null,2,null,4]",
            "9",
            "undefined",
            "9",
            "1",
            "[null,2,null,4,null,null,null,null,9]"
        ])
    );
}
#[test]
fn length_shrink_grow_and_reset_clear_values_and_presence() {
    let output = run(r#"
export function main():void{const values=[1,2,3,4];const alias=values;values.length=2;console.log(JSON.stringify(alias));values["length"]=4;console.log(JSON.stringify(values));console.log(Object.hasOwn(values,2));values.length=0;values.length=9;values[8]=9;console.log(values["length"]);console.log(JSON.stringify(values));console.log(Object.hasOwn(values,0));console.log(values===alias);values.length=-0;console.log(values.length);console.log(JSON.stringify(values));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "[1,2]",
            "[1,2,null,null]",
            "0",
            "9",
            "[null,null,null,null,null,null,null,null,9]",
            "0",
            "1",
            "0",
            "[]"
        ])
    );
}
#[test]
fn delete_preserves_length_handles_absence_and_enforces_nonconfigurable_length() {
    let output = run(r#"
export function main():void{const values:(number|undefined)[]=[1,undefined,3];console.log(delete values[1]);console.log(values.length);console.log(Object.hasOwn(values,1));console.log(values[1]);console.log(delete values[1]);console.log(delete values[99]);console.log(delete values[NaN]);console.log(delete values[-1]);console.log(delete values["01"]);console.log(delete values["0"]);console.log(JSON.stringify(values));try{delete values.length;}catch(e){if(e instanceof TypeError){console.log(e.name);console.log(e.message);}}console.log(values.length);}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "1",
            "3",
            "0",
            "undefined",
            "1",
            "1",
            "1",
            "1",
            "1",
            "1",
            "[null,null,3]",
            "TypeError",
            "Cannot delete array length",
            "3"
        ])
    );
}
#[test]
fn invalid_lengths_raise_owned_range_errors_without_changing_the_array() {
    let output = run(r#"
function invalid(values:number[],length:number):void{try{values.length=length;console.log("BAD");}catch(e){if(e instanceof RangeError){console.log(e.name);console.log(values.length);}}}
export function main():void{const values=[1,2];invalid(values,-1);invalid(values,1.5);invalid(values,NaN);invalid(values,1/0);invalid(values,-1/0);invalid(values,4294967296);console.log(JSON.stringify(values));}
"#);
    let mut expected = Vec::new();
    for _ in 0..6 {
        expected.extend(["RangeError", "2"]);
    }
    expected.push("[1,2]");
    assert_eq!(output.lines, lines(&expected));
}

#[test]
fn writes_evaluate_receiver_key_and_rhs_once_and_keep_the_original_references() {
    let output = run(r#"
function receiver(values:number[]):number[]{console.log("base");return values;}function key(values:number[]):number{console.log("key");values.length=0;return 5;}function rhs(values:number[]):number{console.log("rhs");values.length=2;return 9;}
export function main():void{const values=[1,2,3];receiver(values)[key(values)]=rhs(values);console.log(JSON.stringify(values));
 let index=1;const update=()=>{index=5;return 7;};values[index]=update();console.log(values[1]);console.log(values[5]);
 let current=[1];const old=current;const replace=()=>{current=[99];return 5;};current[2]=replace();console.log(JSON.stringify(old));console.log(JSON.stringify(current));
 const length=()=>{console.log("length");return 2;};receiver(values).length=length();console.log(JSON.stringify(values));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "base",
            "key",
            "rhs",
            "[null,null,null,null,null,9]",
            "7",
            "9",
            "[1,null,5]",
            "[99]",
            "base",
            "length",
            "[null,7]"
        ])
    );
}
#[test]
fn deletion_and_length_changes_inside_callbacks_follow_live_presence_and_initial_range() {
    let output = run(r#"
export function main():void{const input=[1,2,3];const mapped=input.map((value,index,array)=>{if(index===0){delete array[1];array[5]=6;}return value;});console.log(JSON.stringify(mapped));console.log(input.length);
 const filtered=[1,2,3].filter((value,index,array)=>{if(index===0){array.length=1;}return true;});console.log(JSON.stringify(filtered));
 const source=[1,2];console.log(source.find((value,index,array)=>{console.log(value);if(index===0){delete array[1];return false;}array.length=0;return true;}));console.log(source.length);
 const grown=[1,2,3];grown.forEach((value,index,array)=>{console.log(value);if(index===0){array.length=1;array[2]=9;}});}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "[1,null,3]",
            "6",
            "[1]",
            "1",
            "undefined",
            "undefined",
            "0",
            "1",
            "9"
        ])
    );
}
#[test]
fn rhs_errors_and_invalid_length_errors_do_not_publish_the_requested_write() {
    let output = run(r#"
function fail():number{throw TypeError("rhs");}function invalid(values:number[]):number{values.length=1;return 1.5;}
export function main():void{const values=[1,2,3];try{values[7]=fail();}catch(e){if(e instanceof TypeError){console.log(e.message);}}console.log(JSON.stringify(values));try{values.length=invalid(values);}catch(e){if(e instanceof RangeError){console.log(e.message);}}console.log(JSON.stringify(values));
 try{try{delete values["length"];}finally{console.log("finally");}}catch(e){if(e instanceof TypeError){console.log(e.name);}}}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "rhs",
            "[1,2,3]",
            "Invalid array length",
            "[1]",
            "finally",
            "TypeError"
        ])
    );
}
#[test]
fn new_properties_length_coercion_and_old_profiles_are_not_silently_admitted() {
    for source in [
        "export function main():void{const a=[1];a.length='2';}",
        "export function main():void{const a=[1];a.length=undefined;}",
        "export function main():void{const a=[1];const key='0';delete a[key];}",
        "export function main():void{const a={n:1};delete a.n;}",
        "export function main():void{const a=[1];a.length+=1;}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                source,
                LanguageProfile::CheckedF64Utf16V14
            )
            .is_err(),
            "{source}"
        );
    }
    for source in [
        "export function main():void{const a=[1];a.length=2;}",
        "export function main():void{const a=[1];delete a[0];}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(
                source,
                LanguageProfile::CheckedF64Utf16V13
            )
            .is_err()
        );
    }
    let old = varta_compiler::compile_typescript_with_profile(
        "export function main():void{const a=[1];a[2]=3;}",
        LanguageProfile::CheckedF64Utf16V13,
    )
    .unwrap();
    assert!(VartaRuntime::new().run_wasm_bytes(&old, &[]).is_err());
}
#[test]
fn valid_lengths_above_the_resource_bound_and_unsupported_numeric_writes_are_fatal() {
    for body in [
        "a.length=1025;",
        "a.length=4294967295;",
        "a[1024]=1;",
        "a[-1]=1;",
        "a[NaN]=1;",
        "a[0.5]=1;",
    ] {
        let runtime = VartaRuntime::new();
        let module=runtime.validate_module(&compile(&format!("export function main():void{{const a=[1];try{{{body}}}catch(e){{console.log('BAD_CATCH');}}finally{{console.log('BAD_FINALLY');}}}}"))).unwrap();
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        assert!(
            instance
                .get_typed_func::<(), ()>(&mut execution.store, "_start")
                .unwrap()
                .call(&mut execution.store, ())
                .is_err(),
            "{body}"
        );
        assert!(execution.store.data().output.lines.is_empty(), "{body}");
    }
}
#[test]
fn mutation_sequences_match_an_independent_presence_model() {
    let mut source = String::from(
        "function snapshot(values:number[]):void{const bits:boolean[]=[];for(let i=0;i<values.length;i=i+1){bits.push(Object.hasOwn(values,i));}console.log(JSON.stringify(values)+'|'+JSON.stringify(bits));}export function main():void{const values:number[]=[];",
    );
    let mut state = 7u32;
    let mut model: Vec<Option<u32>> = Vec::new();
    let mut expected = Vec::new();
    for step in 0..48u32 {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let index = (state >> 8) as usize % 96;
        match state % 5 {
            0 => {
                source.push_str(&format!("values.length={index};"));
                model.resize(index, None);
            }
            1 => {
                source.push_str(&format!("values[{index}]={step};"));
                if index >= model.len() {
                    model.resize(index + 1, None);
                }
                model[index] = Some(step);
            }
            2 => {
                source.push_str(&format!("delete values[{index}];"));
                if index < model.len() {
                    model[index] = None;
                }
            }
            3 => {
                source.push_str(&format!("values.push({step});"));
                model.push(Some(step));
            }
            _ => {
                source.push_str("values.pop();");
                model.pop();
            }
        }
        source.push_str("snapshot(values);");
        let bits: Vec<_> = model.iter().map(Option::is_some).collect();
        expected.push(format!(
            "varta log: {}|{}",
            serde_json::to_string(&model).unwrap(),
            serde_json::to_string(&bits).unwrap()
        ));
    }
    source.push('}');
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(&source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(100 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    assert_eq!(execution.store.data().output.lines, expected);
}

#[test]
fn detached_aliases_and_new_rhs_references_survive_growth_deletion_and_collection() {
    let source = r#"
function churn():void{let payload="x";for(let i=0;i<14;i=i+1){payload=payload+payload;}for(let i=0;i<260;i=i+1){const values=[payload+"!",payload+"?"];values.slice();}}
function make():{text:string}{churn();return {text:"new"+"😀"};}
export function main():void{const values=[{text:"live"+"!"},{text:"removed"+"?"}];const alias=values[0];delete values[0];values.length=0;churn();if(alias!==undefined){console.log(alias.text);}values[200]=make();churn();console.log(values.length);console.log(Object.hasOwn(values,199));const result=values[200];if(result!==undefined){console.log(result.text);}values.length=0;values.length=201;churn();console.log(Object.hasOwn(values,200));console.log(values[200]);if(alias!==undefined){console.log(alias.text);}}
"#;
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(500 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    crate::exceptions::finish(&instance, &mut execution.store).unwrap();
    assert_eq!(
        execution.store.data().output.lines,
        lines(&["live!", "201", "0", "new😀", "0", "undefined", "live!"])
    );
}
#[test]
fn maximum_length_and_last_index_remain_bounded_and_canonical() {
    let output = run(
        "export function main():void{const values:number[]=[];values.length=1024;values[1023]=7;console.log(values.length);console.log(Object.hasOwn(values,1023));console.log(values.at(-1));values.length=1;values.length=1024;console.log(Object.hasOwn(values,1023));console.log(values.at(-1));console.log(values.includes(undefined));values.length=0;console.log(JSON.stringify(values));}",
    );
    assert_eq!(
        output.lines,
        lines(&["1024", "1", "7", "0", "undefined", "1", "[]"])
    );
}
#[test]
fn old_metadata_cannot_select_the_new_source_semantics_and_helpers_stay_host_free() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "export function main():void{const values:number[]=[];values[2]=3;values.length=1;delete values[0];}",
    );
    let bad = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        let name = varta_common::abi::SECTION;
        if id == 0 && data.get(1..1 + name.len()) == Some(name.as_bytes()) {
            let at = 1 + name.len();
            data[at..at + 4].copy_from_slice(&varta_common::abi::SPARSE_VERSION.to_le_bytes());
        }
        vec![data]
    });
    assert!(VartaRuntime::new().validate_module(&bad).is_err());
    let bytes = compile(
        "function fake(value:boolean):boolean{const values=[value];values.length=3;delete values[0];values.forEach(item=>console.log(item));return value;}export function main():void{varta.json.parse<number>('1');}",
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
fn numeric_write_domain_checks_follow_rhs_effects_but_key_errors_stop_rhs() {
    let runtime = VartaRuntime::new();
    let bytes = compile(
        "function rhs():number{console.log('rhs');return 1;}export function main():void{const values=[1];try{values[-1]=rhs();}catch(e){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
    );
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
    assert_eq!(execution.store.data().output.lines, lines(&["rhs"]));
    let output = run(
        "function key():number{throw TypeError('key');}function rhs():number{console.log('BAD_RHS');return 2;}export function main():void{const values=[1];try{values[key()]=rhs();}catch(e){if(e instanceof TypeError){console.log(e.message);}}console.log(JSON.stringify(values));}",
    );
    assert_eq!(output.lines, lines(&["key", "[1]"]));
}
#[test]
fn mutation_profile_keeps_the_complete_thirteen_helper_dependency_bound() {
    let bytes = compile(
        r#"export function main():void{const values=varta.json.parse<number[]>('[1]');console.log(JSON.stringify(values.map(value=>value)));console.log("a"+"b");console.log("a"<"c");values.push(2);values.slice();values.includes(1);values.pop();console.log(values[0]);values.length=4;delete values[0];values[3]=4;console.log(varta.result.status());}"#,
    );
    let count = wasmparser::Parser::new(0)
        .parse_all(&bytes)
        .find_map(|payload| match payload.unwrap() {
            wasmparser::Payload::FunctionSection(reader) => Some(reader.count() as usize),
            _ => None,
        })
        .unwrap();
    assert_eq!(count, 3 + abi::MAX_INTERNAL_FUNCTIONS);
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap()
            .lines
            .last()
            .unwrap(),
        "varta log: 0"
    );
}

#[test]
fn array_reverse_validates_and_preserves_identity_in_each_admitted_profile() {
    let source = "export function main():void{const a=[1,2,3];console.log(a.reverse()===a);console.log(JSON.stringify(a));console.log(JSON.stringify(a.reverse()));const empty:number[]=[];console.log(empty.reverse()===empty);console.log(JSON.stringify(empty));console.log(JSON.stringify([1,2,3,4].reverse()));}";
    for profile in [
        LanguageProfile::CheckedF64Utf16V6,
        LanguageProfile::CheckedF64Utf16V12,
        LanguageProfile::CheckedF64Utf16V13,
        LanguageProfile::CheckedF64Utf16V14,
    ] {
        let bytes = varta_compiler::compile_typescript_with_profile(source, profile).unwrap();
        wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            lines(&["1", "[3,2,1]", "[1,2,3]", "1", "[]", "[4,3,2,1]"]),
            "{profile:?}"
        );
    }
}
#[test]
fn array_reverse_moves_sparse_presence_and_reference_payloads_together() {
    let source = "export function main():void{const child={n:1};const a:({n:number}|undefined)[]=[child,,undefined,,];const reversed=a.reverse();console.log(reversed===a);console.log(JSON.stringify(a));console.log(Object.hasOwn(a,0));console.log(Object.hasOwn(a,1));console.log(Object.hasOwn(a,2));console.log(Object.hasOwn(a,3));console.log(a[3]===child);console.log(JSON.stringify(a.reverse()));console.log(Object.hasOwn(a,0));console.log(Object.hasOwn(a,1));console.log(Object.hasOwn(a,2));console.log(Object.hasOwn(a,3));}";
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
                "1",
                "[null,null,null,{\"n\":1}]",
                "0",
                "1",
                "0",
                "1",
                "1",
                "[{\"n\":1},null,null,null]",
                "1",
                "0",
                "1",
                "0"
            ]),
            "{profile:?}"
        );
    }
}
