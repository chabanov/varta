use super::*;
use varta_common::language::LanguageProfile as P;
fn compile(source: &str, profile: P) -> Result<Vec<u8>, varta_compiler::CompilerError> {
    varta_compiler::compile_typescript_with_profile(source, profile)
}
fn lines(expected: &[&str]) -> Vec<String> {
    expected.iter().map(|s| format!("varta log: {s}")).collect()
}
fn run(source: &str, expected: &[&str]) {
    let bytes = compile(source, P::CheckedF64Utf16V32).unwrap_or_else(|e| panic!("{e}\n{source}"));
    // Emitted modules must also pass backend validation, not only compilation.
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        lines(expected)
    );
}
fn rejected(source: &str, profile: P) -> String {
    compile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("accepted under {profile:?}:\n{source}"))
        .to_string()
}
/// Test-only fuel so allocation churn reaches the collector; production budgets
/// are unchanged.
fn run_with_collection_fuel(source: &str) -> Vec<String> {
    let runtime = VartaRuntime::new();
    let bytes = compile(source, P::CheckedF64Utf16V32).unwrap_or_else(|e| panic!("{e}\n{source}"));
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(1000 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap();
    crate::exceptions::finish(&instance, &mut execution.store).unwrap();
    execution.store.data().output.lines.clone()
}

#[test]
fn module_constants_fold_scalars_and_store_reference_values() {
    run(
        "export const SPEC_VERSION='0.4.0';const LIMIT=3;const NEG=-2.5;const ON=true;const NONE=null;\
         const MISSING=undefined;const WIDE:number|undefined=7;const BIG=12n;const ALIAS=LIMIT;\
         const URL='https://'+'api.example';const PAIR={name:SPEC_VERSION,size:LIMIT};\
         const LIST:number[]=[LIMIT,NEG,1];\
         function describe():string{return SPEC_VERSION+'/'+String(LIMIT)+'/'+String(NEG);}\
         export function main():void{console.log(describe());console.log(ON);console.log(NONE===null);\
         console.log(MISSING===undefined);console.log(WIDE===7);console.log(String(BIG*2n));\
         console.log(ALIAS);console.log(URL);console.log(JSON.stringify(PAIR));console.log(JSON.stringify(LIST));\
         console.log(NaN===NaN);}",
        &[
            "0.4.0/3/-2.5",
            "1",
            "1",
            "1",
            "1",
            "24",
            "3",
            "https://api.example",
            "{\"name\":\"0.4.0\",\"size\":3}",
            "[3,-2.5,1]",
            "0",
        ],
    );
}
#[test]
fn module_constants_share_one_identity_across_functions_and_closures() {
    // A stored constant is evaluated once: mutations through its handle are
    // visible everywhere, and nested constants alias the same values.
    run(
        "const ITEMS:string[]=['a'];const BOX={items:ITEMS,label:'box'};\
         function add(value:string):number{return ITEMS.push(value);}\
         function size():number{return BOX.items.length;}\
         export function main():void{add('b');console.log(size());BOX.label='renamed';\
         const read=():string=>BOX.label+':'+JSON.stringify(ITEMS);console.log(read());\
         const nested=(n:number):number=>{const inner=():number=>ITEMS.length+n;return inner();};\
         console.log(nested(10));}",
        &["2", "renamed:[\"a\",\"b\"]", "12"],
    );
}
#[test]
fn module_constants_as_const_keeps_element_and_field_types_and_is_readonly() {
    run(
        "export const SCOPES=['admin','agents:read','agents:write'] as const;\
         const CONFIG={retries:3,modes:['a','b']} as const;\
         function has(scope:string):boolean{return SCOPES.includes(scope);}\
         export function main():void{console.log(SCOPES.length);console.log(SCOPES[1]);\
         console.log(has('agents:write'));console.log(has('root'));console.log(CONFIG.retries+CONFIG.modes.length);\
         const local=[1,2] as const;console.log(local.indexOf(2));\
         for(const scope of SCOPES){console.log(scope);}}",
        &[
            "3",
            "agents:read",
            "1",
            "0",
            "5",
            "1",
            "admin",
            "agents:read",
            "agents:write",
        ],
    );
    for (write, expected) in [
        (
            "SCOPES.push('x');",
            "cannot modify readonly `as const` value SCOPES",
        ),
        ("SCOPES.pop();", "readonly `as const` value SCOPES"),
        ("SCOPES.reverse();", "readonly `as const` value SCOPES"),
        ("SCOPES.fill('x');", "readonly `as const` value SCOPES"),
        ("SCOPES[0]='x';", "readonly `as const` value SCOPES"),
        ("SCOPES.length=0;", "readonly `as const` value SCOPES"),
        ("delete SCOPES[0];", "readonly `as const` value SCOPES"),
        ("CONFIG.retries=4;", "readonly `as const` value CONFIG"),
        ("CONFIG.retries+=1;", "readonly `as const` value CONFIG"),
        ("CONFIG.retries++;", "readonly `as const` value CONFIG"),
        (
            "CONFIG.modes.push('c');",
            "readonly `as const` value CONFIG",
        ),
        ("(CONFIG.modes)[0]='c';", "readonly `as const` value CONFIG"),
        (
            "Object.assign(CONFIG,{retries:1});",
            "readonly `as const` value CONFIG",
        ),
        ("[SCOPES[0]]=['x'];", "readonly `as const` value SCOPES"),
        (
            "({retries:CONFIG.retries}={retries:1});",
            "readonly `as const` value CONFIG",
        ),
        (
            "const f=():void=>{SCOPES.push('x');};",
            "readonly `as const` value SCOPES",
        ),
        (
            "const local=[1] as const;local.push(2);",
            "readonly `as const` value local",
        ),
    ] {
        let source = format!(
            "const SCOPES=['admin'] as const;const CONFIG={{retries:3,modes:['a','b']}} as const;\
             export function main():void{{{write}}}"
        );
        let error = rejected(&source, P::CheckedF64Utf16V32);
        assert!(error.contains(expected), "{write}\n{error}");
    }
}
#[test]
fn module_constants_initialize_before_main_and_survive_collection() {
    let out = run_with_collection_fuel(
        "const HEADER='x-'+'trace';const NAMES=['alpha'+'!','beta'+'?'];\
         const NESTED={names:NAMES,tag:HEADER+'-id',count:2};\
         function churn():void{let text='x';for(let i=0;i<14;i=i+1){text=text+text;}\
         for(let i=0;i<260;i=i+1){const garbage=[text+'!',text+'?'];garbage.slice();}}\
         function show():string{return HEADER+'|'+JSON.stringify(NAMES)+'|'+NESTED.tag+'|'+String(NESTED.names[1]);}\
         export function main():void{churn();console.log(show());churn();console.log(show());}",
    );
    assert_eq!(
        out,
        lines(&[
            "x-trace|[\"alpha!\",\"beta?\"]|x-trace-id|beta?",
            "x-trace|[\"alpha!\",\"beta?\"]|x-trace-id|beta?"
        ])
    );
}
#[test]
fn module_constants_split_large_modules_into_bounded_initializers() {
    // Hundreds of generated `*_VALUES` arrays (the pilot `models.ts` shape) would
    // exceed one function body; initializers are chunked in declaration order
    // and later chunks read constants published by earlier ones.
    let declarations: String = (0..400)
        .map(|i| {
            format!("export const V{i}_VALUES=['a{i}','b{i}','c{i}'] as const;const N{i}={i};")
        })
        .collect();
    run(
        &format!(
            "{declarations}const FIRST_LAST=[V0_VALUES,V399_VALUES];const TOTAL=[N0,N399];\
             export function main():void{{console.log(V0_VALUES[0]);console.log(V200_VALUES[1]);\
             console.log(V399_VALUES[2]);console.log(JSON.stringify(FIRST_LAST));console.log(JSON.stringify(TOTAL));}}"
        ),
        &[
            "a0",
            "b200",
            "c399",
            "[[\"a0\",\"b0\",\"c0\"],[\"a399\",\"b399\",\"c399\"]]",
            "[0,399]",
        ],
    );
}
#[test]
fn module_constants_serve_parameter_defaults_and_narrowing() {
    run(
        "const LIMIT=4;const LABEL:string|undefined='tag';const EMPTY:string|undefined=undefined;\
         function page(size:number=LIMIT):number{return size*2;}\
         function len(value:string|undefined):number{return value===undefined?-1:value.length;}\
         export function main():void{console.log(page());console.log(page(1));\
         if(LABEL!==undefined){console.log(LABEL.length);}console.log(len(EMPTY));}",
        &["8", "2", "3", "-1"],
    );
}
#[test]
fn module_constants_are_shadowed_by_locals_and_parameters() {
    run(
        "const NAME='module';function param(NAME:string):string{return NAME;}\
         function local():string{const NAME='local';return NAME;}\
         function inner():string{{const NAME='block';}return NAME;}\
         export function main():void{console.log(param('param'));console.log(local());console.log(inner());}",
        &["param", "local", "module"],
    );
}
#[test]
fn module_constants_reject_unsupported_initializers_and_bindings() {
    for (source, expected) in [
        (
            "let COUNT=1;",
            "unsupported top-level let or var declaration",
        ),
        (
            "var COUNT=1;",
            "unsupported top-level let or var declaration",
        ),
        (
            "const {a}={a:1};",
            "unsupported top-level destructuring declaration",
        ),
        (
            "const [a]=[1];",
            "unsupported top-level destructuring declaration",
        ),
        (
            "function f():number{return 1;}const X=f();",
            "call in a module constant initializer",
        ),
        (
            "const X=Math.max(1,2);",
            "call in a module constant initializer",
        ),
        (
            "const A={n:1};const X=A.n;",
            "member access in a module constant initializer",
        ),
        (
            "const X=`v`;",
            "template literal in a module constant initializer",
        ),
        (
            "const X=():number=>1;",
            "function or class in a module constant initializer",
        ),
        (
            "function f():void{}const X=f;",
            "reference to f in a module constant initializer",
        ),
        (
            "const X=[1,...[2]];",
            "spread in a module constant initializer",
        ),
        (
            "const X=[1,,2];",
            "array hole in a module constant initializer",
        ),
        (
            "const X={a:1,...{b:2}};",
            "spread in a module constant initializer",
        ),
        (
            "const X=[[1] as const];",
            "nested as const in a module constant initializer",
        ),
        (
            "const X=1+2;",
            "non-literal number module constant initializer",
        ),
        (
            "const X=!true;",
            "unary operator in a module constant initializer",
        ),
        (
            "const X=1<2;",
            "binary operator other than + in a module constant initializer",
        ),
        (
            "const X=Y;const Y='y';",
            "binding used before initialization: Y",
        ),
        ("const X=X;", "binding used before initialization: X"),
        ("const X=1;const X=2;", "duplicate binding: X"),
        ("const main=1;", "duplicate module binding: main"),
        ("const X:string=1;", "mismatch"),
    ] {
        let source = if source.contains("function main") {
            source.to_owned()
        } else {
            format!("{source}export function main():void{{}}")
        };
        let error = rejected(&source, P::CheckedF64Utf16V32);
        assert!(error.contains(expected), "{source}\n{error}");
    }
    let error = rejected(
        "const X=1;export function main():void{X=2;}",
        P::CheckedF64Utf16V32,
    );
    assert!(error.contains("assignment to constant"), "{error}");
    // `as const` stays restricted to whole declaration initializers.
    let error = rejected(
        "function f(v:string[]):number{return v.length;}export function main():void{console.log(f(['a'] as const));}",
        P::CheckedF64Utf16V32,
    );
    assert!(error.contains("unsupported"), "{error}");
}
#[test]
fn module_constants_stay_rejected_in_v31() {
    for source in [
        "const LIMIT=3;export function main():void{console.log(LIMIT);}",
        "export const V='1';export function main():void{console.log(V);}",
        "const S=['a'] as const;export function main():void{}",
    ] {
        let error = rejected(source, P::CheckedF64Utf16V31);
        assert!(
            error.contains("unsupported top-level declaration"),
            "{error}"
        );
        compile(source, P::CheckedF64Utf16V32).unwrap();
    }
    let local = "export function main():void{const s=['a'] as const;console.log(s.length);}";
    let error = rejected(local, P::CheckedF64Utf16V31);
    assert!(error.contains("unsupported"), "{error}");
    compile(local, P::CheckedF64Utf16V32).unwrap();
}
