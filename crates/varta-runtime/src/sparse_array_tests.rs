use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V13)
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
fn sparse_literals_distinguish_holes_from_explicit_undefined_and_fill_existing_indices() {
    let output = run(r#"
export function main():void{const values:(number|undefined)[]=[1,,undefined,3];console.log(values.length);console.log(JSON.stringify(values));console.log(values[1]);console.log(Object.hasOwn(values,1));console.log(Object.hasOwn(values,2));console.log(Object.hasOwn(values,"length"));console.log(Object.hasOwn(values,"01"));values[1]=2;console.log(Object.hasOwn(values,"1"));console.log(JSON.stringify(values));const empty:number[]=[,,];console.log(JSON.stringify(empty));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "4",
            "[1,null,null,3]",
            "undefined",
            "0",
            "1",
            "1",
            "0",
            "1",
            "[1,2,null,3]",
            "[null,null]"
        ])
    );
}
#[test]
fn map_preserves_holes_and_mutation_created_gaps_without_visiting_them() {
    let output = run(r#"
export function main():void{const values=[1,,3];let calls=0;const result=values.map(value=>{calls=calls+1;return value*2;});console.log(calls);console.log(JSON.stringify(result));console.log(Object.hasOwn(result,1));
 const input=[1,2,3];const mapped=input.map((value,index,array)=>{if(index===0){array.pop();array.pop();}return value;});console.log(JSON.stringify(mapped));console.log(mapped.length);console.log(Object.hasOwn(mapped,1));console.log(Object.hasOwn(mapped,2));
 console.log(JSON.stringify(values.filter(value=>true)));values.forEach((value,index)=>console.log(index));console.log(values.find((value,index)=>{console.log(value);return index===1;}));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "2",
            "[2,null,6]",
            "0",
            "[1,null,null]",
            "3",
            "0",
            "0",
            "[1,3]",
            "0",
            "2",
            "1",
            "undefined",
            "undefined"
        ])
    );
}
#[test]
fn sparse_slice_pop_push_at_and_search_preserve_presence() {
    let output = run(r#"
export function main():void{const values:(string|undefined)[]=[,"😀",undefined,,];console.log(values.includes(undefined));console.log(values.indexOf(undefined));console.log(values.at(0));const copy=values.slice(1);console.log(JSON.stringify(copy));console.log(Object.hasOwn(copy,0));console.log(Object.hasOwn(copy,2));console.log(copy.pop());copy.push("new");console.log(Object.hasOwn(copy,2));console.log(JSON.stringify(copy));
 const hole:number[]=[,];console.log(hole.pop());console.log(hole.length);hole.push(2,3);console.log(JSON.stringify(hole));console.log(Object.hasOwn(hole,0));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "1",
            "2",
            "undefined",
            "[\"😀\",null,null]",
            "1",
            "0",
            "undefined",
            "1",
            "[\"😀\",null,\"new\"]",
            "undefined",
            "0",
            "[2,3]",
            "1"
        ])
    );
}
#[test]
fn checked_json_creates_present_sparse_storage_and_errors_remain_recoverable() {
    let output = run(r#"
export function main():void{const parsed=varta.json.parse<{values:(number|null)[],label?:string|undefined}>('{"values":[1,null,3]}');console.log(JSON.stringify(parsed));console.log(Object.hasOwn(parsed.values,1));console.log(Array.isArray(parsed.values));try{varta.json.parse<number[]>('{');}catch(e){if(e instanceof SyntaxError){console.log(e.name);}}console.log(JSON.stringify(parsed.values.map(value=>value)));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "{\"values\":[1,null,3]}",
            "1",
            "1",
            "SyntaxError",
            "[1,null,3]"
        ])
    );
}

#[test]
fn presence_survives_bitmap_boundaries_growth_slice_and_callback_mutation() {
    let values = (0..128)
        .map(|index| {
            if index % 3 == 0 {
                String::new()
            } else {
                index.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    let output = run(&format!(
        r#"export function main():void{{const values:number[]=[{values}];console.log(Object.hasOwn(values,63));console.log(Object.hasOwn(values,64));console.log(Object.hasOwn(values,127));console.log(Object.hasOwn(values,128));values.push(128);values[63]=63;const copy=values.slice(62,130);console.log(copy.length);console.log(Object.hasOwn(copy,1));console.log(Object.hasOwn(copy,4));console.log(copy[65]);console.log(copy[66]);console.log(values.at(-1));const mapped=values.map(value=>value+1);console.log(Object.hasOwn(mapped,63));console.log(Object.hasOwn(mapped,66));console.log(mapped.length);}}"#
    ));
    assert_eq!(
        output.lines,
        lines(&[
            "0", "1", "1", "0", "67", "1", "0", "127", "128", "128", "1", "0", "129"
        ])
    );
}

#[test]
fn array_presence_queries_preserve_operand_order_and_numeric_key_rules() {
    let output = run(r#"
function base():number[]{console.log("base");return [1,,3];}function key():number{console.log("key");return 1;}
export function main():void{console.log(Object.hasOwn(base(),key()));const values=[1,,3];console.log(Object.hasOwn(values,NaN));console.log(Object.hasOwn(values,1/0));console.log(Object.hasOwn(values,-0));console.log(Object.hasOwn(values,0.5));console.log(Object.hasOwn(values,-1));console.log(Object.hasOwn(values,"4294967295"));console.log(Object.hasOwn(values,"map"));}
"#);
    assert_eq!(
        output.lines,
        lines(&["base", "key", "0", "0", "0", "1", "0", "0", "0", "0"])
    );
}
#[test]
fn sparse_reference_unions_guards_and_checked_json_keep_value_versions_bound() {
    let output = run(r#"
function keep(value:string[]|null):string[]|null{return value;}
export function main():void{const values=["😀",,"\uD800"];const alias=keep(values);console.log(Array.isArray(alias));console.log(alias instanceof Error);console.log(typeof alias);console.log(alias===values);console.log(JSON.stringify(alias));const parsed=varta.json.parse<{items?:string[]|undefined,value:number[]|null}>('{"items":["owned"],"value":[1,2]}');console.log(JSON.stringify(parsed));try{varta.json.parse<number[]>('"wrong"');}catch(e){if(e instanceof TypeError){console.log(e.name);}}}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "1",
            "0",
            "object",
            "1",
            "[\"😀\",null,\"\\ud800\"]",
            "{\"items\":[\"owned\"],\"value\":[1,2]}",
            "TypeError"
        ])
    );
}
#[test]
fn sparse_maps_keep_reference_operands_and_results_alive_through_collection() {
    let source = r#"
function churn():void{let payload="x";for(let i=0;i<14;i=i+1){payload=payload+payload;}for(let i=0;i<260;i=i+1){const values=[payload+"!",payload+"?"];values.slice();}}
export function main():void{const values=["kept"+"!",,"after"+"?"];const result=values.map((value,index,array):string=>{if(index===0){array.pop();churn();}return value+"😀";});churn();console.log(JSON.stringify(result));console.log(Object.hasOwn(result,1));console.log(Object.hasOwn(result,2));
 const copy=result.slice();copy[2]="filled";churn();console.log(JSON.stringify(copy));console.log(JSON.stringify(result));console.log(copy.pop());console.log(JSON.stringify(copy));}
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
        lines(&[
            "[\"kept!😀\",null,null]",
            "0",
            "0",
            "[\"kept!😀\",null,\"filled\"]",
            "[\"kept!😀\",null,null]",
            "filled",
            "[\"kept!😀\",null]"
        ])
    );
}

#[test]
fn malformed_sparse_storage_is_rejected_before_json_observation() {
    use crate::contract_tests::{export_index, rewrite};
    use varta_common::value::*;
    let bytes = compile(
        "function make():string[]|null{return ['owned😀',,];}export function main():void{}",
    );
    let index = export_index(&bytes, "_start") - 1;
    let bytes = rewrite(&bytes, |id, data| {
        let mut data = data.to_vec();
        if id == 7 {
            assert!(data[0] < 127);
            data[0] += 1;
            let name = "fixture.make";
            data.push(name.len() as u8);
            data.extend_from_slice(name.as_bytes());
            data.push(0);
            data.push(index);
        }
        vec![data]
    });
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    for mutation in 0..6 {
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let handle = instance
            .get_typed_func::<(), i64>(&mut execution.store, "fixture.make")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap();
        let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
        let data = memory.data(&execution.store);
        let boxed =
            ReferenceBox::read_with_version(data, handle as u64, SPARSE_VALUE_VERSION).unwrap();
        let vector = Vector::read_with_version(data, boxed.payload, SPARSE_VALUE_VERSION).unwrap();
        assert!(
            ReferenceBox::read_with_version(data, handle as u64, ERROR_FAMILY_VALUE_VERSION)
                .is_err()
        );
        let bitmap = vector.presence.unwrap();
        let (at, bytes) = match mutation {
            0 => (bitmap, vec![0]),
            1 => (bitmap, vec![5]),
            2 => (bitmap + SPARSE_BITMAP_BYTES - 1, vec![128]),
            3 => (
                vector.storage + ARRAY_META_BYTES + ARRAY_SLOT_BYTES,
                1u64.to_le_bytes().to_vec(),
            ),
            4 => (
                vector.storage - abi::STRING_HEADER_BYTES + HEAP_FLAGS_OFFSET,
                (HEAP_ALLOCATED_FLAG | HEAP_ARRAY_FLAG as u32)
                    .to_le_bytes()
                    .to_vec(),
            ),
            _ => (
                vector.ptr + VECTOR_STORAGE_OFFSET as usize + 4,
                1u32.to_le_bytes().to_vec(),
            ),
        };
        memory.write(&mut execution.store, at, &bytes).unwrap();
        assert!(
            crate::json::render_reference(
                memory.data(&execution.store),
                ValueKind::ReferenceUnion as u32,
                handle as u64,
                SPARSE_VALUE_VERSION
            )
            .is_err(),
            "mutation {mutation}"
        );
    }
}
#[test]
fn sparse_profiles_imports_and_value_schema_versions_cannot_be_downgraded() {
    use crate::contract_tests::rewrite;
    let bytes = compile(
        "function keep(value:number[]|null):number[]|null{return value;}export function main():void{const values=keep(varta.json.parse<number[]>('[1]'));console.log(JSON.stringify(values));console.log(Array.isArray(values));}",
    );
    for (old, new) in [
        ("json_parse_v14", "json_parse_v13"),
        ("json_stringify_v12", "json_stringify_v11"),
        ("value_is_array_v6", "value_is_array_v5"),
    ] {
        let bad = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 2 {
                let at = data
                    .windows(old.len())
                    .position(|s| s == old.as_bytes())
                    .unwrap();
                data[at..at + old.len()].copy_from_slice(new.as_bytes());
            }
            vec![data]
        });
        assert!(VartaRuntime::new().validate_module(&bad).is_err(), "{old}");
    }
    for name in [
        varta_common::abi::SECTION,
        varta_common::value::VALUE_SECTION,
    ] {
        let bad = rewrite(&bytes, |id, data| {
            let mut data = data.to_vec();
            if id == 0 && data.get(1..1 + name.len()) == Some(name.as_bytes()) {
                let at = 1 + name.len();
                let version = if name == varta_common::abi::SECTION {
                    varta_common::abi::CLOSURE_VERSION
                } else {
                    varta_common::value::ERROR_FAMILY_VALUE_VERSION
                };
                data[at..at + 4].copy_from_slice(&version.to_le_bytes());
            }
            vec![data]
        });
        assert!(VartaRuntime::new().validate_module(&bad).is_err(), "{name}");
    }
    assert!(
        varta_compiler::compile_typescript_with_profile(
            "export function main():void{const values:number[]=[1,,3];}",
            LanguageProfile::CheckedF64Utf16V12
        )
        .is_err()
    );
    for source in [
        "export function main():void{const values=[,,];}",
        "export function main():void{const values=[1];values[2]=3;}",
        "export function main():void{const values=[1];values.length=3;}",
    ] {
        let result = varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V13,
        );
        // Existing-slot bounds are checked at execution; broader syntax stays rejected.
        if source.contains("[2]=") {
            assert!(
                VartaRuntime::new()
                    .run_wasm_bytes(&result.unwrap(), &[])
                    .is_err()
            );
        } else {
            assert!(result.is_err());
        }
    }
}

#[test]
fn maximum_sparse_length_preserves_presence_and_canonical_json() {
    let values = (0..varta_common::value::MAX_ARRAY_ELEMENTS)
        .map(|index| {
            if index % 2 == 0 {
                String::new()
            } else {
                index.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    let output = run(&format!(
        "export function main():void{{const values:number[]=[{values}];const result=values.map(value=>value+1);console.log(result.length);console.log(Object.hasOwn(result,0));console.log(Object.hasOwn(result,1023));console.log(result.at(-1));console.log(result.indexOf(undefined));console.log(result.includes(undefined));}}"
    ));
    assert_eq!(output.lines, lines(&["1024", "0", "1", "1024", "-1", "1"]));
}

#[test]
fn sparse_json_helpers_keep_complete_dependency_bounds_and_transitive_host_purity() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        r#"export function main():void{const values=varta.json.parse<number[]>('[1]');console.log(JSON.stringify(values.map(value=>value)));console.log("a"+"b");console.log("a"<"c");values.push(2);values.slice();values.includes(1);values.pop();console.log(values[0]);console.log(Object.hasOwn(values,0));}"#,
    );
    let count = wasmparser::Parser::new(0)
        .parse_all(&bytes)
        .find_map(|payload| match payload.unwrap() {
            wasmparser::Payload::FunctionSection(reader) => Some(reader.count() as usize),
            _ => None,
        })
        .unwrap();
    assert_eq!(count, 3 + abi::SPARSE_INTERNAL_FUNCTIONS);
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap();
    let bytes = compile(
        "function fake(value:boolean):boolean{[value,,].forEach(item=>console.log(item));return value;}export function main():void{varta.json.parse<number>('1');}",
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
fn void_mapping_and_json_roundtrips_keep_holes_distinct_from_present_undefined() {
    let output = run(r#"
export function main():void{const values=[1,,3].map(()=>{});console.log(Object.hasOwn(values,0));console.log(Object.hasOwn(values,1));console.log(Object.hasOwn(values,2));console.log(values.indexOf(undefined));console.log(JSON.stringify(values));const parsed=varta.json.parse<null[]>(JSON.stringify(values));console.log(Object.hasOwn(parsed,1));console.log(parsed[1]);}
"#);
    assert_eq!(
        output.lines,
        lines(&["1", "0", "1", "0", "[null,null,null]", "1", "null"])
    );
}
