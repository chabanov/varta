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
fn local_patterns_infer_and_annotate_nested_bindings_defaults_and_rest() {
    let output = run(r#"
export function main():void{const {name:label,child:{n},missing=7,...other}:{name:string;child:{n:number};enabled?:boolean}={name:"item",child:{n:3},enabled:false};console.log(label);console.log(n);console.log(missing);console.log(JSON.stringify(other));let [first=1,,third=3,...tail]:number[]=[2,,4,,6];first=first+1;console.log(first);console.log(third);console.log(JSON.stringify(tail));const {value:inferred}={value:"inferred"};console.log(inferred);const [a=1,...[b=2,c=3]]:number[]=[];console.log(a+b+c);let {value}:{value:number|string}={value:1};value="changed";console.log(JSON.stringify(value));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "item",
            "3",
            "7",
            "{\"enabled\":false}",
            "3",
            "4",
            "[null,6]",
            "inferred",
            "6",
            "\"changed\""
        ])
    );
}
#[test]
fn local_initializers_run_once_and_declarators_and_leaves_initialize_in_order() {
    let output = run(r#"
function source():{a?:number;b?:number}{console.log("source");return {};}
function effect(label:string,value:number):number{console.log(label);return value;}
export function main():void{const {a=effect("a",1),b=effect("b",a+1)}=source(),[c=effect("c",b+1)]:number[]=[],d=effect("d",c+1);console.log(a+b+c+d);const {}=source();const []:number[]=[effect("empty array",9)];const {a:present=effect("BAD",0)}={a:8};console.log(present);}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "source",
            "a",
            "b",
            "c",
            "d",
            "10",
            "source",
            "empty array",
            "8"
        ])
    );
}
#[test]
fn local_object_rest_preserves_presence_order_identity_and_const_child_mutation() {
    let output = run(r#"
export function main():void{const child={n:1};const source:{remove:number;a?:number;z:number;child:{n:number};"2":number;"1":number}={z:2,child:child,remove:0,"2":20,"1":10};const {remove,...rest}=source;console.log(JSON.stringify(rest));console.log(Object.hasOwn(rest,'a'));rest.z=9;rest.child.n=3;console.log(source.z);console.log(source.child.n);source.a=undefined;const {...copy}=source;console.log(Object.hasOwn(copy,'a'));console.log(JSON.stringify(copy));console.log(copy===source);const {["z"]:alias}=source;console.log(alias);}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "{\"1\":10,\"2\":20,\"z\":2,\"child\":{\"n\":1}}",
            "0",
            "2",
            "3",
            "1",
            "{\"1\":10,\"2\":20,\"z\":2,\"child\":{\"n\":3},\"remove\":0}",
            "0",
            "2"
        ])
    );
}
#[test]
fn local_array_binding_observes_live_mutation_and_retains_exhaustion() {
    let output = run(r#"
export function main():void{const source:number[]=[];const grow=():number=>{source.push(7,8);return 1;};const [a=grow(),b=2,...tail]=source;console.log(a);console.log(b);console.log(JSON.stringify(tail));console.log(JSON.stringify(source));const live:number[]=[,2,3];const change=():number=>{live[1]=9;live.push(4);return 1;};const [first=change(),second=0,...rest]=live;console.log(first);console.log(second);console.log(JSON.stringify(rest));const [...holes]:(number|undefined)[]=[,undefined];console.log(Object.hasOwn(holes,0));console.log(Object.hasOwn(holes,1));console.log(JSON.stringify(holes));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "1",
            "2",
            "[]",
            "[7,8]",
            "1",
            "9",
            "[3,4]",
            "1",
            "1",
            "[null,null]"
        ])
    );
}
#[test]
fn local_let_bindings_share_cells_while_shadowed_scopes_stay_independent() {
    let output = run(r#"
export function main():void{let read:()=>number=()=>0;let write:()=>number=()=>0;const {n:outer}={n:9};{let {n}= {n:1};read=()=>n;write=()=>{n=n+1;return n;};{const {n}={n:7};console.log(n);}n=3;}console.log(read());console.log(write());console.log(read());console.log(outer);const factory=():()=>string=>{let {text}={text:"one"};const {fn=()=>text}:{}={};text=text+"!";return fn;};console.log(factory()());const named=function self():number{const {self}={self:5};return self;};console.log(named());}
"#);
    assert_eq!(output.lines, lines(&["7", "3", "4", "4", "9", "one!", "5"]));
}
#[test]
fn for_let_patterns_keep_initializer_and_each_iteration_cells_distinct() {
    let output = run(r#"
export function main():void{let initial:()=>number=()=>99;let first:()=>number=()=>99;let second:()=>number=()=>99;for(let {i=0,read=()=>i}:{}={};i<3;i=i+1){initial=read;if(i===0){i=1;first=()=>i;}else{second=()=>i;}}console.log(initial());console.log(first());console.log(second());let original:()=>number=()=>99;for(let i=0,read=()=>i;i<2;i=i+1){original=read;if(i===0){i=1;}}console.log(original());}
"#);
    assert_eq!(output.lines, lines(&["0", "1", "2", "0"]));
}
#[test]
fn repeated_block_patterns_and_loop_finalizers_keep_escaped_cells_and_scopes() {
    let output = run(r#"
export function main():void{let first:()=>number=()=>99;let second:()=>number=()=>99;for(let {i=0}:{}={};i<3;i=i+1){const {local}={local:i};if(i===0){first=()=>local;}if(i===1){second=()=>local;}try{if(i===0){continue;}if(i===2){break;}}finally{console.log(local);}}console.log(first());console.log(second());const {i}={i:9};console.log(i);let count=0;for(const {limit}={limit:2};count<limit;count=count+1){const {n}={n:count};console.log(n);}}
"#);
    assert_eq!(
        output.lines,
        lines(&["0", "1", "2", "0", "1", "9", "0", "1"])
    );
}
#[test]
fn local_nullish_and_default_throws_stop_later_leaves_and_declarators() {
    let output = run(r#"
function nullable():{n:number}|null{return null;}
function bad():number{console.log("BAD");return 0;}
function fail():number{console.log("default");throw SyntaxError("local");}
export function main():void{try{const {n}=nullable(),other=bad();console.log("BAD");}catch(e){if(e instanceof TypeError){console.log(e.message);}}try{const [{n}]:{n:number}[]=[];console.log("BAD");}catch(e){if(e instanceof TypeError){console.log(e.name);}}try{const {n=fail(),later=bad()}:{}={};}catch(e){if(e instanceof SyntaxError){console.log(e.message);}}finally{console.log("finally");}try{const {}:{n:number}|undefined=undefined;}catch(e){if(e instanceof TypeError){console.log(e.name);}}}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "Cannot destructure null or undefined",
            "TypeError",
            "default",
            "local",
            "finally",
            "TypeError"
        ])
    );
}
#[test]
fn local_patterns_reject_tdz_duplicate_const_writes_and_unsupported_forms() {
    for source in [
        "export function main():void{const {a=a}:{}={};}",
        "export function main():void{const {a=b,b=1}:{}={};}",
        "export function main():void{const {a=b}:{}={},b=1;}",
        "export function main():void{const {a=b}:{}={};let b=1;}",
        "export function main():void{const {a=(()=>b)(),b=1}:{}={};}",
        "export function main():void{const a=9;{const {a}={a:a};}}",
        "export function main():void{const {a:x,b:x}={a:1,b:2};}",
        "export function main():void{const {a}={a:1};a=2;}",
        "export function main():void{const {a}={a:1};const change=()=>{a=2;};}",
        "export function main():void{let {a}={a:1};a='bad';}",
        "export function main():void{let {a}:{a:number};}",
        "export function main():void{var {a}={a:1};}",
        "export function main():void{const key='a';const {[key]:a}={a:1};}",
        "export function main():void{const [a]='text';}",
        "export function main():void{const {a}:number=1;}",
        "export function main():void{const target={a:0};({a:target.a}={a:'bad'});}",
        "export function main():void{try{throw 1;}catch({a}){}}",
        "export function main():void{for(const {a} of [1]){}}",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V14,
        )
        .unwrap_err();
        assert!(error.diagnostic().is_some(), "{source}: {error}");
    }
    let source = "export function main():void{const {a=b,b=1}:{}={};}";
    let error = varta_compiler::compile_typescript_with_profile(
        source,
        LanguageProfile::CheckedF64Utf16V14,
    )
    .unwrap_err();
    assert_eq!(
        error.diagnostic().unwrap().range.start.byte_offset,
        source.find("=b").unwrap() + 1
    );
}
#[test]
fn local_pattern_temporaries_and_escaped_cells_survive_collection_with_result_pairs() {
    let source = r#"
function churn():number{let text="x";for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+"!",text+"?"];garbage.slice();}return 1;}
function make():{head:string;tail:string;child:{n:number};optional?:number}{return {tail:"tail"+"!",head:"head"+"!",child:{n:7}};}
function child():{n:number}{churn();return {n:9};}
function saved():()=>string{const pair=varta.result.capture(varta.env.get("VARTA_LOCAL_PAIR"));let {head,optional=churn(),tail,...rest}=make();const [first=child(),...others]:{n:number}[]=[,{n:2}];head=head+"?";return()=>{churn();console.log(pair.status);console.log(first.n);console.log(JSON.stringify(others));console.log(JSON.stringify(rest));return head+tail;};}
export function main():void{const read=saved();churn();console.log(read());}
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
    let output = &execution.store.data().output.lines;
    assert_eq!(
        &output[output.len() - 5..],
        lines(&[
            "1",
            "9",
            "[{\"n\":2}]",
            "{\"child\":{\"n\":7}}",
            "head!?tail!"
        ])
    );
}
#[test]
fn local_const_union_guards_remain_valid_in_captured_bindings() {
    let output = run(r#"
function input():{value:number|string}{return {value:3};}
export function main():void{const {value}=input();const read=()=>value;if(typeof value==="number"){console.log(value+1);}console.log(JSON.stringify(read()));let first:()=>number=()=>99;let second:()=>number=()=>99;let i=0;while(i<2){const [n=0]:number[]=[i];if(i===0){first=()=>n;}else{second=()=>n;}i=i+1;}console.log(first());console.log(second());}
"#);
    assert_eq!(output.lines, lines(&["4", "3", "0", "1"]));
    for source in [
        "export function main():void{let {n}:{n:number|string}={n:1};const change=()=>{n='bad';};if(typeof n==='number'){console.log(n+1);}}",
        "export function main():void{let {n}:{n:number|string}={n:1};if(typeof n==='number'){console.log(n+1);}const change=()=>{n='bad';};}",
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
}
#[test]
fn local_patterns_keep_nested_this_and_private_self_while_closures_escape() {
    let output = run(r#"
export function main():void{const fn=function self(this:{base:number},depth:number):()=>number{const {base=this.base}:{}={};if(depth===0){return ()=>base;}const {next=self.call(this,depth-1)}:{}={};return ()=>next()+base;};console.log(fn.call({base:2},2)());}
"#);
    assert_eq!(output.lines, lines(&["6"]));
}
#[test]
fn local_pattern_host_effects_remain_visible_to_guest_helper_validation() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function effect():number{console.log('effect');return 1;}function fake(value:boolean):boolean{const {n=effect()}:{n?:number}={};return value;}export function main():void{varta.json.parse<number>('1');}",
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
fn local_rest_copies_maximum_arrays_under_default_limits_in_each_profile() {
    let values = vec!["1"; varta_common::value::MAX_ARRAY_ELEMENTS].join(",");
    let source = format!(
        "export function main():void{{const [...rest]:number[]=[{values}];console.log(rest.length);}}"
    );
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
            lines(&["1024"]),
            "{profile:?}"
        );
    }
}
#[test]
fn local_pattern_limits_and_older_profiles_fail_with_source_diagnostics() {
    let holes = ",".repeat(varta_common::function::MAX_BINDING_PATTERN_NODES + 1);
    let source = format!("export function main():void{{const [{holes}]:number[]=[];}}");
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
    assert_eq!(
        error.diagnostic().unwrap().code,
        varta_common::DiagnosticCode::InvalidProgram
    );
    let source = "export function main():void{const {n}={n:1};console.log(n);}";
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf16V11,
    ] {
        let error = varta_compiler::compile_typescript_with_profile(source, profile).unwrap_err();
        assert!(
            error.to_string().contains("local destructuring requires"),
            "{error}"
        );
    }
}
#[test]
fn local_default_resource_traps_bypass_guest_catch_and_finally() {
    let bytes = compile(
        "export function main():void{try{const {n=(()=>{while(true){}return 1;})()}:{}={};console.log('BAD_BODY');}catch(e){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
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
