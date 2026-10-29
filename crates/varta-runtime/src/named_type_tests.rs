use super::*;
use varta_common::language::LanguageProfile as P;
fn compile(source: &str, profile: P) -> Result<Vec<u8>, varta_compiler::CompilerError> {
    varta_compiler::compile_typescript_with_profile(source, profile)
}
fn run(source: &str, expected: &[&str]) {
    let bytes = compile(source, P::CheckedF64Utf16V31).unwrap_or_else(|e| panic!("{e}\n{source}"));
    // Emitted modules must also pass backend validation, not only compilation.
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
}
fn rejected(source: &str, profile: P) -> String {
    compile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("accepted under {profile:?}:\n{source}"))
        .to_string()
}

#[test]
fn named_types_resolve_interfaces_aliases_and_forward_references() {
    run(
        "type Score=number;type Tags=Tag[];type MaybeTag=Tag|null;interface Tag{name:string;score:Score;note?:string}\
         function make(name:string,score:Score):Tag{return {name,score};}\
         function best(tags:Tags):MaybeTag{let out:MaybeTag=null;let top=-1;for(const t of tags){if(t!==undefined&&t.score>top){out=t;top=t.score;}}return out;}\
         export function main():void{const tags:Tags=[make('a',1),make('b',2.5)];const top=best(tags);\
         if(top!==null){console.log(top.name);console.log(top.score);console.log(Object.hasOwn(top,'note'));}\
         console.log(JSON.stringify(tags));console.log(best([])===null);}",
        &[
            "b",
            "2.5",
            "0",
            "[{\"name\":\"a\",\"score\":1},{\"name\":\"b\",\"score\":2.5}]",
            "1",
        ],
    );
}
#[test]
fn named_types_are_structural_aliases_of_their_literal_shapes() {
    // An interface and an equal type literal intern to the same record shape.
    run(
        "interface Point{x:number;y:number}type Same={y:number;x:number};\
         function sum(p:Point):number{return p.x+p.y;}\
         export function main():void{const q:Same={x:1,y:2};console.log(sum(q));const r:{x:number;y:number}={x:3,y:4};console.log(sum(r));}",
        &["3", "7"],
    );
}
#[test]
fn named_types_cover_functions_unions_json_and_errors() {
    run(
        "type Pair={left:number;right:number};type Op=(a:number,b:number)=>number;type Out=string|number;\
         function apply(op:Op,p:Pair):Out{const v=op(p.left,p.right);return v>2?v:'small';}\
         export function main():void{const p=varta.json.parse<Pair>('{\"left\":2,\"right\":3}');\
         console.log(apply((a,b)=>a*b,p));console.log(apply((a,b)=>a-b,p));\
         try{throw TypeError('t');}catch(e){if(e instanceof TypeError){const l:Local=e;console.log(l.message);}}}type Local=TypeError;",
        &["6", "small", "t"],
    );
}
#[test]
fn named_types_reject_unsupported_declarations_with_clear_diagnostics() {
    for (source, expected) in [
        (
            "type A=B;type B=A;function f(x:A):void{}",
            "recursive type declaration",
        ),
        (
            "type J={v:J[]};function f(x:J):void{}",
            "recursive type declaration",
        ),
        ("interface G<T>{v:T}", "generic type declaration"),
        ("type G<T>=T[];", "generic type declaration"),
        (
            "interface A{x:number}interface B extends A{y:number}",
            "interface extends",
        ),
        (
            "function f(x:Record<string,number>):void{}",
            "generic type reference",
        ),
        ("function f(x:Missing):void{}", "type reference Missing"),
        (
            "interface A{x:number}interface A{y:number}",
            "duplicate type declaration: A",
        ),
        (
            "type A=number;interface A{x:number}",
            "duplicate type declaration: A",
        ),
        (
            "interface V{f():void}function f(x:V):void{}",
            "object type member",
        ),
        (
            "type A={readonly x:number};function f(x:A):void{}",
            "readonly",
        ),
        (
            "export function main():void{type L=number;}",
            "unsupported statement",
        ),
    ] {
        let source = if source.contains("main") {
            source.to_owned()
        } else {
            format!("{source}export function main():void{{}}")
        };
        let error = rejected(&source, P::CheckedF64Utf16V31);
        assert!(error.contains(expected), "{source}\n{error}");
    }
}
#[test]
fn named_types_stay_rejected_in_v30() {
    for source in [
        "interface P{x:number}function f(p:P):number{return p.x;}export function main():void{}",
        "type N=number;function f(n:N):N{return n;}export function main():void{}",
    ] {
        let error = rejected(source, P::CheckedF64Utf16V30);
        assert!(error.contains("unsupported type annotation"), "{error}");
        compile(source, P::CheckedF64Utf16V31).unwrap();
    }
    // Unused declarations remain accepted and ignored by earlier profiles.
    compile(
        "interface P{x:number}type Q=P;export function main():void{}",
        P::CheckedF64Utf16V30,
    )
    .unwrap();
}
#[test]
fn named_types_shadow_global_error_type_names_but_not_values() {
    // A module-local `Error` type (an API model) shadows the built-in type name in
    // annotations; the `Error` value and `instanceof` keep the built-in meaning.
    run(
        "interface Error{code:string;status:number}function describe(e:Error):string{return e.code+'/'+String(e.status);}\
         export function main():void{console.log(describe({code:'E',status:404}));\
         try{throw TypeError('boom');}catch(e){if(e instanceof TypeError){console.log(e.message);}}}",
        &["E/404", "boom"],
    );
}
