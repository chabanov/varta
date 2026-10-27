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
fn object_patterns_bind_aliases_nested_values_and_defaults_in_source_order() {
    let output = run(r#"
function take({name:label,child:{n},missing=7,optional=8}:{name:string;child:{n:number};optional?:number}):string{return label+JSON.stringify([n,missing,optional]);}
function top({n=3}:{n?:number}={}):number{return n;}
export function main():void{console.log(take({name:"value",child:{n:2}}));console.log(top());console.log(top({n:0}));console.log(top(undefined));const fn:({n}:{n:number})=>number=({n})=>n;console.log(fn({n:5}));const other=function({child:{n}={n:9}}:{child?:{n:number}}):number{return n;};console.log(other({}));}
"#);
    assert_eq!(
        output.lines,
        lines(&["value[2,7,8]", "3", "0", "3", "5", "9"])
    );
}
#[test]
fn array_patterns_handle_elisions_missing_holes_defaults_and_nested_rest() {
    let output = run(r#"
function take([first=1,,third=3,...tail]:number[]):void{console.log(first);console.log(third);console.log(JSON.stringify(tail));}
function nested([[a=4,b=5]=[]]:number[][]):number{return a+b;}
function rest(...[a=2,b=3]:number[]):number{return a+b;}
export function main():void{take([]);take([0,8,2,4,5]);take([,,]);console.log(nested([]));console.log(nested([[7]]));console.log(rest());console.log(rest(...[4,5]));const fn=function([a=2,...[b=3,c=4]]:number[]):number{return a+b+c;};console.log(fn([1,2,3]));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "1", "3", "[]", "0", "2", "[4,5]", "1", "3", "[]", "9", "12", "5", "9", "6"
        ])
    );
}
#[test]
fn destructuring_observes_live_source_mutation_and_sticky_iterator_exhaustion() {
    let output = run(r#"
export function main():void{const values:number[]=[];const grow=():number=>{values.push(7,8);return 1;};const exhausted=function([a=grow(),b=2,...tail]:number[]):void{console.log(JSON.stringify([a,b]));console.log(JSON.stringify(tail));};exhausted(values);console.log(JSON.stringify(values));
 const live:number[]=[,2,3];const change=():number=>{live[1]=9;live.push(4);return 1;};const read=function([a=change(),b,...tail]:number[]):void{const pair:(number|undefined)[]=[a,b];console.log(JSON.stringify(pair));console.log(JSON.stringify(tail));};read(live);
 const record:{first?:number;second:number;extra?:number}={second:2};const update=():number=>{record.second=9;record.extra=4;return 1;};const object=function({first=update(),second,...tail}:{first?:number;second:number;extra?:number}):void{console.log(first);console.log(second);console.log(JSON.stringify(tail));};object(record);}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "[1,2]",
            "[]",
            "[7,8]",
            "[1,9]",
            "[3,4]",
            "1",
            "9",
            "{\"extra\":4}"
        ])
    );
}
#[test]
fn object_rest_is_fresh_shallow_and_preserves_presence_and_enumeration_order() {
    let output = run(r#"
function copy({omit,...rest}:{omit:number;a?:number;z:number;child:{n:number};"2":number;"1":number}):{a?:number;z:number;child:{n:number};"2":number;"1":number}{return rest;}
export function main():void{const child={n:3};const source:{omit:number;a?:number;z:number;child:{n:number};"2":number;"1":number}={z:2,child:child,omit:0,"2":20,"1":10};const first=copy(source);console.log(JSON.stringify(first));console.log(Object.hasOwn(first,'a'));first.z=9;first.child.n=4;console.log(source.z);console.log(source.child.n);source.a=undefined;const second=copy(source);console.log(Object.hasOwn(second,'a'));console.log(JSON.stringify(second));console.log(first===second);const empty=function({...rest}:{}):{}{return rest;};console.log(JSON.stringify(empty({})));}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "{\"1\":10,\"2\":20,\"z\":2,\"child\":{\"n\":3}}",
            "0",
            "2",
            "4",
            "1",
            "{\"1\":10,\"2\":20,\"z\":2,\"child\":{\"n\":4}}",
            "0",
            "{}"
        ])
    );
}
#[test]
fn destructuring_callbacks_infer_types_and_share_checked_call_this_and_spread_paths() {
    let output = run(r#"
function method(this:{base:number},{n=2}:{n?:number},[extra=3]:number[]=[]):number{return this.base+n+extra;}
export function main():void{console.log(JSON.stringify([{n:1},{n:2}].map(({n})=>n*2)));console.log([{n:1},{n:2}].reduce(({n},value)=>{return {n:n+value.n};},{n:0}).n);console.log(JSON.stringify([[1,2],[3]].map(([a=0,b=0])=>a+b)));console.log(method.call({base:10},{},...[]));const named=function self({n}:{n:number}):number{if(n===0){return 7;}return self(...[{n:n-1}]);};console.log(named({n:3}));}
"#);
    assert_eq!(output.lines, lines(&["[2,4]", "3", "[3,3]", "15", "7"]));
}
#[test]
fn nullish_nested_patterns_throw_before_later_defaults_and_body() {
    let output = run(r#"
function fail({child:{n}}:{child:{n:number}|null|undefined},later:number=effect()):void{console.log("BAD_BODY");}
function effect():number{console.log("BAD_DEFAULT");return 1;}
function array([{}]:{n:number}[]):void{console.log("BAD_BODY");}
function root({}:{n:number}|null|undefined):void{console.log("BAD_BODY");}
export function main():void{try{fail({child:null});}catch(e){if(e instanceof TypeError){console.log(e.message);}}try{fail({child:undefined});}catch(e){if(e instanceof TypeError){console.log(e.message);}}try{array([]);}catch(e){if(e instanceof TypeError){console.log(e.name);}}try{root(null);}catch(e){if(e instanceof TypeError){console.log(e.name);}}try{root(undefined);}catch(e){if(e instanceof TypeError){console.log(e.name);}}finally{console.log("finally");}}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "Cannot destructure null or undefined",
            "Cannot destructure null or undefined",
            "TypeError",
            "TypeError",
            "TypeError",
            "finally"
        ])
    );
}
#[test]
fn nested_binding_defaults_infer_present_types_and_widen_scalar_fallbacks() {
    let output = run(r#"
function take([value="missing"]:number[]):void{console.log(JSON.stringify(value));}
function unknown({missing=1+2}:{}):number{return missing;}
function sequence({a=1,b=a+1}:{a?:number;b?:number},c:number=b+1):number{return a+b+c;}
export function main():void{take([]);take([2]);console.log(unknown({}));console.log(sequence({}));}
"#);
    assert_eq!(output.lines, lines(&["\"missing\"", "2", "3", "6"]));
}
#[test]
fn destructuring_rejects_tdz_duplicates_untyped_and_unsupported_sources_with_real_spans() {
    for source in [
        "function f({a=a}:{a?:number}):void{}export function main():void{}",
        "function f({a=b,b=1}:{a?:number;b?:number}):void{}export function main():void{}",
        "function f({a=(()=>b)(),b=1}:{a?:number;b?:number}):void{}export function main():void{}",
        "function f({a}:{a:number},a:number):void{}export function main():void{}",
        "function f({a}):void{}export function main():void{}",
        "function f({a}:number):void{}export function main():void{}",
        "function f([a]:string):void{}export function main():void{}",
        "function f({[varta.env.get('KEY')]:a}:{a:number}):void{}export function main():void{}",
        "function f({toString}:{}):void{}export function main():void{}",
        "function f(key:string,{[key]:a}:{a:number}):void{}export function main():void{}",
        "export function main():void{const target={a:1};({a:target.a}={a:'bad'});}",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(
            source,
            LanguageProfile::CheckedF64Utf16V14,
        )
        .unwrap_err();
        assert!(error.diagnostic().is_some(), "{source}: {error}");
    }
    let source = "function f({a=b,b=1}:{a?:number;b?:number}):void{}export function main():void{}";
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
fn destructured_bindings_share_cells_with_siblings_later_defaults_and_escaped_closures() {
    let output = run(r#"
export function main():void{const factory=function({text,fn=()=>text}:{text:string}):()=>string{text=text+"!";return fn;};const first=factory({text:"one"});const second=factory({text:"two"});console.log(first());console.log(second());
 let reader:()=>number=()=>0;const use=function({n=1,ignored=(()=>{reader=()=>n;n=n+1;return n;})()}:{n?:number}):number{n=n+1;return ignored;};console.log(use({}));console.log(reader());
 const outer=9;const scope=function({n=outer}:{}):number{const outer=1;return n;};console.log(scope({}));}
"#);
    assert_eq!(output.lines, lines(&["one!", "two!", "2", "3", "9"]));
}
#[test]
fn rest_holes_are_dense_present_entries_and_defaults_preserve_null_false_zero_nan() {
    let output = run(r#"
function tail([,...rest]:number[]):void{console.log(JSON.stringify(rest));console.log(Object.hasOwn(rest,0));console.log(Object.hasOwn(rest,1));}
function values({n=7,b=true,text="default",child={n:1}}:{n?:number|null;b?:boolean;text?:string;child?:{n:number}|null}):void{console.log(JSON.stringify(n));console.log(b);console.log(text);console.log(JSON.stringify(child));}
export function main():void{tail([1,,3]);values({n:null,b:false,text:"",child:null});values({n:0});values({n:NaN});}
"#);
    assert_eq!(
        output.lines,
        lines(&[
            "[null,3]",
            "1",
            "1",
            "null",
            "0",
            "",
            "null",
            "0",
            "1",
            "default",
            "{\"n\":1}",
            "null",
            "1",
            "default",
            "{\"n\":1}"
        ])
    );
}
#[test]
fn rest_and_object_payloads_survive_collection_with_receiver_and_physical_result_pairs() {
    let source = r#"
function churn():void{let text="x";for(let i=0;i<14;i=i+1){text=text+text;}for(let i=0;i<260;i=i+1){const garbage=[text+"!",text+"?"];garbage.slice();}}
function save(this:{text:string},pair:VartaResult<string>,{text,child,...copy}:{text:string;child:{n:number};other?:string},[first,...tail]:{n:number}[]):()=>string{churn();return()=>{churn();console.log(pair.status);console.log(first===child);console.log(JSON.stringify(tail));console.log(JSON.stringify(copy));return this.text+text;};}
export function main():void{const pair=varta.result.capture(varta.env.get("VARTA_PATTERN_PAIR"));const child={n:3};const saved=save.call({text:"receiver"+"!"},pair,{other:"rest"+"!",child:child,text:"bound"+"!"},[child,child]);churn();console.log(saved());}
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
            "1",
            "[{\"n\":3}]",
            "{\"other\":\"rest!\"}",
            "receiver!bound!"
        ])
    );
}
#[test]
fn nested_default_throws_skip_remaining_bindings_and_preserve_outer_finalizers() {
    let output = run(r#"
function fail():number{console.log("default");throw SyntaxError("nested");}
function argument():number{console.log("argument");return 1;}
function take({n=fail(),later=bad()}:{n?:number},extra:number):void{console.log("BAD_BODY");}
function bad():number{console.log("BAD_DEFAULT");return 1;}
export function main():void{try{take({},argument());}catch(e){if(e instanceof SyntaxError){console.log(e.message);}}finally{console.log("finally");}}
"#);
    assert_eq!(
        output.lines,
        lines(&["argument", "default", "nested", "finally"])
    );
}
#[test]
fn maximum_rest_copy_and_destructured_callbacks_run_under_default_budgets() {
    let values = vec!["1"; varta_common::value::MAX_ARRAY_ELEMENTS].join(",");
    let source = format!(
        "function size([...items]:number[]):number{{return items.length;}}export function main():void{{console.log(size([{values}]));}}"
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
    let values = vec!["item"; 1024].join(",");
    let output = run(&format!(
        "export function main():void{{const item={{n:1}};console.log([{values}].map(({{n=0}})=>n).length);}}"
    ));
    assert_eq!(output.lines, lines(&["1024"]));
}
#[test]
fn pattern_node_depth_and_profile_gates_fail_before_emission() {
    let holes = ",".repeat(varta_common::function::MAX_PARAMETER_PATTERN_NODES + 1);
    let deep = varta_common::function::MAX_PARAMETER_PATTERN_DEPTH + 1;
    for (pattern, message) in [
        (format!("[{holes}]"), "pattern budget exceeded"),
        (
            format!("{}value{}", "[".repeat(deep), "]".repeat(deep)),
            "source complexity exceeds compiler limits",
        ),
    ] {
        let source =
            format!("function f({pattern}:number[]):void{{}}export function main():void{{}}");
        let error = varta_compiler::compile_typescript_with_profile(
            &source,
            LanguageProfile::CheckedF64Utf16V14,
        )
        .unwrap_err();
        assert!(error.to_string().contains(message), "{error}");
        assert!(error.diagnostic().is_some());
    }
    let source = "function f({n}:{n:number}):number{return n;}export function main():void{console.log(f({n:1}));}";
    for profile in [
        LanguageProfile::CheckedI32Utf8V1,
        LanguageProfile::CheckedF64Utf16V11,
    ] {
        assert!(varta_compiler::compile_typescript_with_profile(source, profile).is_err());
    }
}
#[test]
fn nested_default_effects_cannot_be_hidden_behind_guest_helpers() {
    use crate::contract_tests::{export_index, rewrite};
    let bytes = compile(
        "function effect():number{console.log('effect');return 1;}function fake(value:boolean):boolean{const fn=function({n=effect()}:{n?:number}):number{return n;};fn({});return value;}export function main():void{varta.json.parse<number>('1');}",
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
fn fatal_pattern_default_limits_bypass_guest_catch_and_finally() {
    let bytes = compile(
        "function consume({n=(()=>{while(true){}return 1;})()}:{n?:number}):void{console.log('BAD_BODY');}export function main():void{try{consume({});}catch(e){console.log('BAD_CATCH');}finally{console.log('BAD_FINALLY');}}",
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
#[test]
fn literal_binding_keys_and_repeated_properties_keep_source_order() {
    let output = run(r#"
function take({["name"]:label,1:one}:{name:string;"1":number}):string{return label+JSON.stringify(one);}
export function main():void{console.log(take({name:"value","1":2}));const source:{n?:number}={};const change=():number=>{source.n=9;return 1;};const twice=function({n:a=change(),n:b,...rest}:{n?:number}):void{console.log(a);console.log(b);console.log(JSON.stringify(rest));};twice(source);}
"#);
    assert_eq!(output.lines, lines(&["value2", "1", "9", "{}"]));
}
#[test]
fn object_rest_preserves_all_thirty_two_presence_bits_and_zero_unused_order_slots() {
    let fields = (0..varta_common::value::MAX_RECORD_FIELDS)
        .map(|i| format!("a{i}?:number"))
        .collect::<Vec<_>>()
        .join(";");
    let source = format!(
        "function copy({{...rest}}:{{{fields}}}):{{{fields}}}{{return rest;}}export function main():void{{const source:{{{fields}}}={{a31:31,a0:0,a15:undefined}};const result=copy(source);console.log(JSON.stringify(result));console.log(Object.hasOwn(result,'a15'));console.log(Object.hasOwn(result,'a30'));result.a30=30;console.log(JSON.stringify(result));}}"
    );
    assert_eq!(
        run(&source).lines,
        lines(&[
            "{\"a31\":31,\"a0\":0}",
            "1",
            "0",
            "{\"a31\":31,\"a0\":0,\"a30\":30}"
        ])
    );
}
