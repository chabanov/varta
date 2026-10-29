//! Interactions between the v32 pilot streams in one module: literal types,
//! classes, module constants and template literals. Module-graph interactions
//! (re-exported constants and classes) live in `tests/public_api.rs`.
use super::*;
use varta_common::language::LanguageProfile as P;
fn compile(source: &str, profile: P) -> Result<Vec<u8>, varta_compiler::CompilerError> {
    varta_compiler::compile_typescript_with_profile(source, profile)
}
fn run(source: &str, expected: &[&str]) {
    let bytes = compile(source, P::CheckedF64Utf16V32).unwrap_or_else(|e| panic!("{e}\n{source}"));
    // Emitted modules must also pass backend validation, not only compilation.
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    let expected: Vec<String> = expected.iter().map(|s| format!("varta log: {s}")).collect();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        expected
    );
}
fn rejected(source: &str) -> String {
    compile(source, P::CheckedF64Utf16V32)
        .err()
        .unwrap_or_else(|| panic!("accepted:\n{source}"))
        .to_string()
}

const AUTH: &str = "type Kind='none'|'apiKey';type Code=200|404;\
    class Auth{kind:Kind;readonly code:Code=200;header:'Authorization'|null=null;ok:true=true;\
    constructor(kind:Kind){this.kind=kind;}\
    describe():string{return `${this.kind}:${this.code}:${this.header}:${this.ok}`;}\
    setKind(text:string):boolean{if(text==='none'||text==='apiKey'){this.kind=text;return true;}return false;}}\
    class Bearer extends Auth{constructor(){super('apiKey');}describe():string{return `bearer ${super.describe()}`;}}";

#[test]
fn literal_typed_values_are_primitive_template_substitutions() {
    run(
        "type Kind='none'|'apiKey';type Code=200|404;\
         function pick(flag:boolean):Kind|null{return flag?'apiKey':null;}\
         export function main():void{const k:Kind='none';const c:Code=404;const t:true=true;const n:-1=-1;\
         console.log(`${k}/${c}/${t}/${n}/${pick(true)}/${pick(false)}`);\
         const wide:Kind|Code|undefined=c;console.log(`[${wide}]`);}",
        &["none/404/true/-1/apiKey/null", "[404]"],
    );
}

#[test]
fn class_fields_hold_literal_sets_with_narrowing_and_erased_layouts() {
    // A number literal field takes its erased Float64 placeholder before the
    // initializer runs; literal fields serialize and dispatch as their base.
    run(
        &format!(
            "{AUTH}export function main():void{{const a=new Auth('none');console.log(a.describe());\
             console.log(a.setKind('apiKey'));console.log(a.setKind('other'));const kind=a.kind;\
             if(kind==='apiKey'){{const only:'apiKey'=kind;console.log(only);}}\
             const b:Auth=new Bearer();console.log(b.describe());console.log(b.code===200);\
             console.log(JSON.stringify(a));}}"
        ),
        &[
            "none:200:null:true",
            "1",
            "0",
            "apiKey",
            "bearer apiKey:200:null:true",
            "1",
            "{\"kind\":\"apiKey\",\"code\":200,\"header\":null,\"ok\":true}",
        ],
    );
    for (main, expected) in [
        (
            "const a=new Auth('none');const s:string='x';a.kind=s;",
            "value of type string is not assignable to literal type 'apiKey' | 'none'",
        ),
        (
            "const a=new Auth('bogus');",
            "value 'bogus' is not assignable to literal type 'apiKey' | 'none'",
        ),
        (
            "const a=new Auth('none');const k:'none'=a.kind;",
            "is not assignable to literal type 'none'",
        ),
        (
            "const a=new Auth('none');a.code=404;",
            "cannot assign to readonly class field code",
        ),
        (
            "const a=new Auth('none');console.log(`${a}`);",
            "template literal substitution requires a primitive value, found class instance",
        ),
    ] {
        let error = rejected(&format!("{AUTH}export function main():void{{{main}}}"));
        assert!(error.contains(expected), "{main}\n{error}");
    }
}

#[test]
fn class_values_stay_outside_the_module_constant_initializer_subset() {
    let client = "class Client{base:string=BASE;label():string{return `${PREFIX}${this.base}`;}}";
    // Class bodies, field initializers and template substitutions read
    // module constants; a class-typed constant may only hold `null`.
    run(
        &format!(
            "const BASE='https://api';const PREFIX='v1 ';{client}\
             const NONE:Client|null=null;\
             export function main():void{{console.log(new Client().label());console.log(NONE===null);}}"
        ),
        &["v1 https://api", "1"],
    );
    for (declaration, expected) in [
        (
            "export const CLIENT=new Client();",
            "unsupported call in a module constant initializer",
        ),
        (
            "export const CLIENT:Client={base:'x'};",
            "type mismatch for CLIENT: expected class instance, found object",
        ),
        (
            "export const LABEL=`${PREFIX}`;",
            "unsupported template literal in a module constant initializer",
        ),
    ] {
        let error = rejected(&format!(
            "const BASE='b';const PREFIX='p';{client}{declaration}export function main():void{{}}"
        ));
        assert!(error.contains(expected), "{declaration}\n{error}");
    }
}
