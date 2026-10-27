use super::*;

fn run(source: &str) -> RunOutput {
    let bytes = varta_compiler::compile_typescript(source).unwrap();
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}

#[test]
fn arrays_preserve_types_identity_nested_values_mutation_and_json() {
    let output = run(r#"
function update(a:number[]):Array<number>{a[1]=a[1]+7;return a;}
function empty():number[]{return [];}
function nested(o:{values:string[]}):{values:Array<string>}{return o;}
export function main():void{
 const a=[1,2,-3];const alias=update(a);console.log(alias===a);console.log(a!==[1,9,-3]);
 console.log(a.length);console.log(JSON.stringify(a));
 let sum=0;for(let i=0;i<a.length;i=i+1){sum=sum+a[i];}console.log(sum);
 const strings:string[]=["Привіт"+"🦀","\"\\\n\u0000"];strings[1]="ok";
 const o={flags:[true,false],values:strings,empty:empty(),nulls:[null,null]};console.log(JSON.stringify(o));
 const rows:{text:string;count:number}[]=[{text:"first",count:1},{count:2,text:"second"}];
 const same=rows[0];same.count=3;rows[1]={text:"third",count:4};console.log(JSON.stringify(rows));
 const matrix:number[][]=[[],[1,2]];matrix[0]=[7];console.log(JSON.stringify(matrix));
 const inline:{data:number[][]}={data:[[]]};console.log(JSON.stringify(inline));
 console.log(JSON.stringify(nested({values:[]})));console.log(JSON.stringify(empty()));
}
"#);
    assert_eq!(
        output.lines,
        [
            "varta log: 1",
            "varta log: 1",
            "varta log: 3",
            "varta log: [1,9,-3]",
            "varta log: 7",
            "varta log: {\"flags\":[true,false],\"values\":[\"Привіт🦀\",\"ok\"],\"empty\":[],\"nulls\":[null,null]}",
            "varta log: [{\"text\":\"first\",\"count\":3},{\"text\":\"third\",\"count\":4}]",
            "varta log: [[7],[1,2]]",
            "varta log: {\"data\":[[]]}",
            "varta log: {\"values\":[]}",
            "varta log: []",
        ]
    );
}

#[test]
fn array_operations_evaluate_operands_once_in_source_order() {
    let output = run(r#"
function base(a:number[]):number[]{console.log("base");return a;}
function index():number{console.log("index");return 1;}
function rhs():number{console.log("rhs");return 9;}
function first():number{console.log(1);return 1;}
function second():number{console.log(2);return 2;}
export function main():void{
 const a=[first(),second()];base(a)[index()]=rhs();console.log(base(a)[index()]);
 console.log(base(a).length);console.log(JSON.stringify(a));
}
"#);
    assert_eq!(
        output.lines,
        [
            "varta log: 1",
            "varta log: 2",
            "varta log: base",
            "varta log: index",
            "varta log: rhs",
            "varta log: base",
            "varta log: index",
            "varta log: 9",
            "varta log: base",
            "varta log: 2",
            "varta log: [1,9]",
        ]
    );
}

#[test]
fn negative_large_empty_and_out_of_range_indices_trap_with_bounded_memory() {
    for body in [
        "const a=[1,2];console.log(a[-1]);",
        "const a=[1,2];console.log(a[2]);",
        "const a=[1,2];console.log(a[2147483647]);",
        "const a:number[]=[];console.log(a[0]);",
        "const a=[1,2];a[2]=rhs();",
    ] {
        let source = format!(
            "function rhs():number{{console.log(9);return 7;}}export function main():void{{{body}}}"
        );
        let bytes = varta_compiler::compile_typescript(&source).unwrap();
        let runtime = VartaRuntime::new();
        let module = runtime.validate_module(&bytes).unwrap();
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let instance = execution.instantiate(&module).unwrap();
        let error = instance
            .get_typed_func::<(), ()>(&mut execution.store, "_start")
            .unwrap()
            .call(&mut execution.store, ())
            .unwrap_err();
        assert!(format!("{error:?}").contains("unreachable"), "{error:?}");
        if body.contains("rhs()") {
            assert_eq!(execution.store.data().output.lines, ["varta log: 9"]);
            let memory = instance.get_memory(&mut execution.store, "memory").unwrap();
            let start = abi::OWNED_STRING_BASE
                + abi::STRING_HEADER_BYTES
                + varta_common::value::ARRAY_META_BYTES;
            assert_eq!(
                &memory.data(&execution.store)[start..start + 16],
                &[1, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0]
            );
        }
    }
}

#[test]
fn collector_traces_array_elements_beyond_bit_32_and_pending_arguments() {
    let values = (0..100)
        .map(|i| format!("{{text:\"saved\"+\"{i}\"}}"))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        r#"
function make():{{text:string}}[]{{return [{values}];}}
function churn():number{{for(let i=0;i<800;i=i+1){{const dead=[{{text:"dead"+"!"}},{{text:"unused"}}];}}return 0;}}
function pick(a:{{text:string}}[],unused:number):{{text:string}}[]{{return a;}}
export function main():void{{
 const holder={{rows:make()}};const other=pick(make(),churn());const old=holder.rows[99];
 holder.rows[99]={{text:"changed"+"!"}};churn();
 console.log(holder.rows[33].text);console.log(holder.rows[99].text);
 console.log(old.text);console.log(other[99].text);
 console.log(JSON.stringify([[holder.rows[0]],[other[99]]]));
}}
"#
    );
    let output = crate::record_tests::run_collection_stress(&source);
    assert_eq!(
        output.lines,
        [
            "varta log: saved33",
            "varta log: changed!",
            "varta log: saved99",
            "varta log: saved99",
            "varta log: [[{\"text\":\"saved0\"}],[{\"text\":\"saved99\"}]]"
        ]
    );
}

#[test]
fn array_operands_and_replacements_keep_tool_effect_catalogs() {
    let source = r#"
function values():string[]{return [varta.env.get("NAME")];}
function index():number{return varta.time.now();}
export function encode():string{
 const a=values();a[index()]=varta.fs.read("/not-granted");
 return JSON.stringify({values:a,last:values()[index()],length:values().length});
}
export function main():void{}
varta.tool({name:"encode",description:"Encode"},encode);
"#;
    let bytes = varta_compiler::compile_typescript(source).unwrap();
    let (_, catalog) = VartaRuntime::new()
        .validate_module_and_tools(&bytes)
        .unwrap();
    assert_eq!(
        catalog.unwrap().tools[0].required_effects,
        ["env.get", "fs.read", "time.now"]
    );
}

#[test]
fn maximum_length_arrays_and_legacy_modules_validate_and_execute() {
    use varta_common::value::{LEGACY_VALUE_VERSION, MAX_ARRAY_ELEMENTS, VALUE_SECTION};
    let values = vec!["7"; MAX_ARRAY_ELEMENTS].join(",");
    for annotation in ["", ":number[]", ":Array<number>"] {
        let source = format!(
            "export function main():void{{const a{annotation}=[{values}];console.log(a.length);console.log(a[1023]);}}"
        );
        let output = run(&source);
        assert_eq!(output.lines, ["varta log: 1024", "varta log: 7"]);
    }
    let bytes = varta_compiler::compile_typescript(
        "export function main():void{console.log(JSON.stringify(7));}",
    )
    .unwrap();
    let legacy = crate::contract_tests::rewrite(&bytes, |id, section| {
        let mut data = section.to_vec();
        if id == 0 && section.get(1..1 + VALUE_SECTION.len()) == Some(VALUE_SECTION.as_bytes()) {
            data.truncate(1 + VALUE_SECTION.len());
            data.extend(LEGACY_VALUE_VERSION.to_le_bytes());
        }
        vec![data]
    });
    let output = VartaRuntime::new().run_wasm_bytes(&legacy, &[]).unwrap();
    assert_eq!(output.lines, ["varta log: 7"]);
}
