use super::*;
use varta_common::language::LanguageProfile as P;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V30)
        .unwrap_or_else(|e| panic!("{e}\n{source}"))
}
fn run(source: &str, expected: &[&str]) {
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&compile(source), &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
}
#[test]
fn object_assign_records_follow_own_order_presence_overwrites_and_identity() {
    run(
        "export function main():void{const target:{a?:number;b?:number;'1'?:number;empty?:undefined}={};const first:{b?:number;a?:number;'1'?:number;empty?:undefined}={b:2,a:1,'1':9,empty:undefined};delete first.a;first.a=3;const result=Object.assign(target,first,{a:7});console.log(Object.is(result,target));console.log(JSON.stringify(Object.keys(target)));console.log(JSON.stringify(target));console.log(Object.hasOwn(target,'empty'));}",
        &[
            "1",
            "[\"1\",\"b\",\"empty\",\"a\"]",
            "{\"1\":9,\"b\":2,\"a\":7}",
            "1",
        ],
    );
}
#[test]
fn object_assign_evaluates_all_arguments_then_copies_sources_in_sequence() {
    run(
        "function source(s:string,r:{a?:number;b?:number}):{a?:number;b?:number}{console.log(s);return r;}export function main():void{const a:{a?:number;b?:number}={a:1};const b:{a?:number;b?:number}={b:2};const extra=():{a?:number;b?:number}=>{a.a=9;console.log('extra');return a;};Object.assign(source('target',a),source('source',b),extra());console.log(JSON.stringify(a));const fail=():{a:number}=>{throw SyntaxError('arg');};try{Object.assign(null,fail());}catch(e){console.log(e instanceof SyntaxError);}try{Object.assign();}catch(e){console.log(e instanceof TypeError);}}",
        &["target", "source", "extra", "{\"a\":9,\"b\":2}", "1", "1"],
    );
}
#[test]
fn object_assign_arrays_skip_holes_extend_and_keep_aliases() {
    run(
        "export function main():void{const a:(number|undefined)[]=[1,2];const b:(number|undefined)[]=[,undefined,4];const r=Object.assign(a,b);console.log(Object.is(r,a));console.log(JSON.stringify(a));console.log(Object.hasOwn(a,0));console.log(Object.hasOwn(a,1));Object.assign(a,{'5':9});console.log(a.length);console.log(JSON.stringify(a));Object.assign(a,a);console.log(JSON.stringify(a));}",
        &[
            "1",
            "[1,null,4]",
            "1",
            "1",
            "6",
            "[1,null,4,null,null,9]",
            "[1,null,4,null,null,9]",
        ],
    );
}
#[test]
fn object_assign_strings_and_primitive_nullish_sources() {
    run(
        "export function main():void{const a:string[]=[];Object.assign(a,'😀');console.log(JSON.stringify(a));const r:{'0'?:string;'1'?:string}={};Object.assign(r,'ab',null,undefined,1,true,3n,TypeError('x'));console.log(JSON.stringify(r));const n:{'0'?:number;'2'?:number}={};const values:number[]=[1,,3];Object.assign(n,values);console.log(JSON.stringify(n));}",
        &[
            "[\"\\ud83d\",\"\\ude00\"]",
            "{\"0\":\"a\",\"1\":\"b\"}",
            "{\"0\":1,\"2\":3}",
        ],
    );
}
#[test]
fn object_assign_array_length_failure_retains_earlier_writes() {
    run(
        "export function main():void{const a:number[]=[1,2,3];try{Object.assign(a,{'0':7,length:-1});}catch(e){console.log(e instanceof RangeError);}console.log(JSON.stringify(a));Object.assign(a,{'5':9,length:2});console.log(JSON.stringify(a));console.log(a.length);}",
        &["1", "[7,2,3]", "[7,2]", "2"],
    );
}
#[test]
fn object_assign_nullable_targets_sources_and_reference_values() {
    run(
        "function copy(t:{n?:number}|number[]|undefined,s:{n?:number}|undefined):void{if(Array.isArray(t)){Object.assign(t,{'0':4});console.log(JSON.stringify(t));}else{const r=Object.assign(t,s);console.log(JSON.stringify(r));}}export function main():void{const r:{n?:number}={};copy(r,{n:3});copy(r,undefined);const a:number[]=[];copy(a,undefined);try{copy(undefined,undefined);}catch(e){console.log(e instanceof TypeError);}const child={n:1};const target:{child?:{n:number}|undefined}={};Object.assign(target,{child:child});const saved=target.child;if(saved!==undefined){saved.n=8;}console.log(child.n);}",
        &["{\"n\":3}", "{\"n\":3}", "[4]", "1", "8"],
    );
}
#[test]
fn object_assign_older_admission_and_checked_storage_gaps_are_explicit() {
    assert!(
        varta_compiler::compile_typescript_with_profile(
            "export function main():void{Object.assign({n:1},{n:2});}",
            P::CheckedF64Utf16V29
        )
        .is_err()
    );
    for source in [
        "export function main():void{const target={};Object.assign(target,{newField:1});}",
        "export function main():void{Object.assign(1,{});}",
        "export function main():void{const a:number[]=[];Object.assign(a,{name:1});}",
        "export function main():void{const Object={assign:1};Object.assign({});}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V30).is_err()
        );
    }
}
#[test]
fn object_assign_fresh_literal_reserves_new_fields_and_widens_overwritten_storage() {
    run(
        "export function main():void{const child={n:1};const r=Object.assign({a:1},{b:'x',child:child},{a:'last'});console.log(JSON.stringify(r));console.log(JSON.stringify(Object.keys(r)));const saved=r.child;if(saved!==undefined){saved.n=8;}console.log(child.n);const source:{optional?:number}={};const empty=Object.assign({},source);console.log(JSON.stringify(empty));source.optional=7;console.log(JSON.stringify(Object.assign({},source)));}",
        &[
            "{\"a\":\"last\",\"b\":\"x\",\"child\":{\"n\":1}}",
            "[\"a\",\"b\",\"child\"]",
            "8",
            "{}",
            "{\"optional\":7}",
        ],
    );
}
#[test]
fn object_assign_different_nested_shapes_do_not_need_one_erased_value_union() {
    run(
        "export function main():void{const a={x:1};const b={y:'b'};const r=Object.assign({},{a:a,b:b});console.log(JSON.stringify(r));console.log(Object.is(r.a,a));console.log(Object.is(r.b,b));}",
        &["{\"a\":{\"x\":1},\"b\":{\"y\":\"b\"}}", "1", "1"],
    );
}
#[test]
fn object_assign_union_target_returns_the_original_reference() {
    run(
        "function same(t:{n?:number}|number[]):boolean{return Object.is(Object.assign(t,null,undefined),t);}export function main():void{console.log(same({n:1}));const a:number[]=[];console.log(same(a));}",
        &["1", "1"],
    );
}
#[test]
fn object_assign_later_source_alias_observes_earlier_writes() {
    run(
        "function copy(t:{'0'?:number}|number[]):void{console.log(Object.is(Object.assign(t,{'0':4}),t));console.log(JSON.stringify(t));}export function main():void{const r={a:1,b:2};Object.assign(r,{b:9},r);console.log(JSON.stringify(r));const a:number[]=[1,2];Object.assign(a,{'0':8},a);console.log(JSON.stringify(a));copy({'0':1});const values:number[]=[];copy(values);}",
        &["{\"a\":1,\"b\":9}", "[8,2]", "1", "{\"0\":4}", "1", "[4]"],
    );
}
#[test]
fn object_assign_copied_graphs_and_replaced_fields_survive_collection() {
    let source = "export function main():void{const r=Object.assign({},{text:'x'.repeat(20000),big:9007199254740993n});const saved=r.text;let i=0;while(i<24){Object.assign(r,{text:'z'.repeat(300000)});i++;}if(saved!==undefined){console.log(saved.length);}const text=r.text;if(text!==undefined){console.log(text.length);}console.log(r.big);console.log(JSON.stringify(Object.keys(r)));}";
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&compile(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], Some(&[])).unwrap();
    // Deliberate collection fixture; production memory/fuel budgets stay unchanged.
    execution
        .store
        .set_fuel(1000 * varta_common::abi::FUEL)
        .unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    assert_eq!(
        execution.store.data().output.lines,
        [
            "varta log: 20000",
            "varta log: 300000",
            "varta log: 9007199254740993",
            "varta log: [\"text\",\"big\"]"
        ]
    );
}
#[test]
fn object_assign_maximum_array_copy_fits_default_budgets() {
    let values = (0..varta_common::value::MAX_ARRAY_ELEMENTS)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        "export function main():void{{const source:number[]=[{values}];const target:number[]=[];Object.assign(target,source);console.log(target.length);console.log(target[0]);console.log(target[1023]);}}"
    );
    run(&source, &["1024", "0", "1023"]);
}
#[test]
fn object_assign_checked_field_failure_retains_prior_writes_without_corrupting_types() {
    run(
        "export function main():void{const target={a:0,b:0};const source:{a:number;b:number|string}={a:7,b:'bad'};try{Object.assign(target,source);}catch(e){console.log(e instanceof TypeError);}console.log(JSON.stringify(target));}",
        &["1", "{\"a\":7,\"b\":0}"],
    );
}
#[test]
fn object_assign_dispatches_record_array_string_and_nullish_source_union() {
    run(
        "function copy(v:{n?:number}|number[]|string|null):void{const target:{n?:number;'0'?:number|string;'1'?:number|string}={};console.log(JSON.stringify(Object.assign(target,v)));}export function main():void{copy({n:9});const a:number[]=[1,2];copy(a);copy('ab');copy(null);}",
        &[
            "{\"n\":9}",
            "{\"0\":1,\"1\":2}",
            "{\"0\":\"a\",\"1\":\"b\"}",
            "{}",
        ],
    );
}
