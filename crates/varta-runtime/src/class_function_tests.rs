//! Class/function prerequisite families share compiler, GC and artifact checks.
use super::*;
use varta_common::language::LanguageProfile as P;
const V38: P = P::CheckedF64Utf16V38;
const CASES: &[(&str, &str, &[&str])] = &[
    (
        "constructor_receivers",
        include_str!("class_function_cases/constructor_receivers.ts"),
        &[
            "true 6 3 3 4",
            "5",
            "5",
            "true 16 8 8 9",
            "10",
            "10",
            "true true 5 10",
            "leaf4",
            "true leaf",
            "true",
            "retained",
            "true 11",
            "true generic",
            "3",
            "live live true",
        ],
    ),
    (
        "initializers",
        include_str!("class_function_cases/initializers.ts"),
        &["true true false 12123", "true true 3 3 6"],
    ),
    (
        "methods",
        include_str!("class_function_cases/methods.ts"),
        &["true true 1", "5 6 10 8", "14", "9"],
    ),
    (
        "tuple_rest",
        include_str!("class_function_cases/tuple_rest.ts"),
        &[
            "2:2x 1:3:undefined 2:4:undefined 2:5:y",
            "3:1:a 4:2",
            "7 z",
            "8w",
            "2:9v",
            "4u",
            "16",
            "3",
            "1a 2b",
            "3",
        ],
    ),
    (
        "suspension",
        include_str!("class_function_cases/suspension.ts"),
        &["7 10 true true"],
    ),
];
#[test]
fn class_function_prerequisites_match_node() {
    for (name, source, expected) in CASES {
        let bytes = varta_compiler::compile_typescript_with_profile(source, V38)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let actual = VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            actual.lines,
            expected
                .iter()
                .map(|s| format!("varta log: {s}"))
                .collect::<Vec<_>>(),
            "{name}"
        );
    }
}
#[test]
fn class_function_prerequisites_keep_checked_rejections() {
    let arity = "function needs(...args:[number,...string[]]):number{return args[0];}export function main():void{const empty:(...args:[])=>number=needs;console.log(empty());}";
    let error = varta_compiler::compile_typescript_with_profile(arity, V38)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("tuple-rest adapters require identical parameter layouts"),
        "{error}"
    );
    for source in [
        "class Box<T>{value:T;constructor(v:T){this.value=v;}get():T{return this.value;}}export function main():void{const a=new Box<number>(1);const b=new Box<string>('x');const f=a.get;}",
        "class C{fn:(this:C)=>number=function(this:C):number{return this.later;};early:number=this.fn();later:number=7;}export function main():void{}",
        "function f(...args:[number,...string[]]):number{return args.length;}function g(...args:[number,...string[]]):number{return f(...args);}export function main():void{}",
        "class C{self:C=this; n:number=this.self.n;} export function main():void{}",
        "class C{n:number=1;m():number{return this.n;}} export function main():void{const c=new C();const f=c.m;f();}",
        "function f(...args:[number,string]):void{} export function main():void{f(1);}",
        "function f(...args:[number,string]):void{} export function main():void{f(1,2);}",
        "function f(...args:[number,string]):void{} export function main():void{f(1,'s',3);}",
        "class B{m():number{return 1;}} class C extends B{m():number{return 2;}} function f(v:B):void{const m=v.m;} export function main():void{}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, V38).is_err(),
            "{source}"
        );
    }
    for source in [
        "class C{self:C=this;} export function main():void{}",
        "class C{m():number{return 1;}} export function main():void{const c=new C();const f=c.m;}",
        "function f(...args:[number,string]):void{} export function main():void{}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, P::CheckedF64Utf16V37).is_err(),
            "{source}"
        );
    }
}

#[test]
fn class_function_prerequisites_survive_collector_stress() {
    use crate::codegen_v2_tests::run_test_budgets;
    use crate::collector_stress_tests::stressed;
    // Test-only budget for instrumentation that collects continuously.
    const STRESS_BUDGETS: u64 = 400;
    for (name, source, expected) in CASES {
        let expected = expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>();
        for units in [2, 64] {
            let actual = run_test_budgets(&stressed(source, units), STRESS_BUDGETS);
            assert_eq!((&actual.lines, &actual.error), (&expected, &None), "{name}");
        }
    }
}

#[test]
fn class_constructor_receivers_keep_partial_instances_private() {
    for source in [
        "class C{n:number;constructor(){console.log(this.read());this.n=1;}read():number{return this.n;}}export function main():void{}",
        "function take(c:C):void{}class C{n:number;constructor(){take(this);this.n=1;}}export function main():void{}",
        "class C{n:number;read:()=>C;constructor(){this.read=()=>this;this.n=1;}}export function main():void{}",
        "class C{n:number;constructor(){const alias=this;this.n=1;}}export function main():void{}",
        "class C{n:number;constructor(){if(true){this.n=1;}const alias=this;}}export function main():void{}",
        "class C{n:number;constructor(){this.assign();const alias=this;}assign():void{this.n=1;}}export function main():void{}",
        "class C{n:number;constructor(){this.n=1;return this;}}export function main():void{}",
        "class B{n:number=1;}class C extends B{constructor(){console.log(this);super();}}export function main():void{}",
        "class C{n:number=1;constructor(v:C=this){}}export function main():void{}",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, V38).is_err(),
            "{source}"
        );
    }
    // These would look complete in a base constructor, but the actual object
    // has a descendant shape, including subclasses materialized by main later.
    for source in [
        "class B{n:number=1;constructor(){this.read();}read():number{return this.n;}}class C extends B{extra:string='late';read():number{return this.extra.length;}}export function main():void{}",
        "class B{n:number=1;constructor(){const alias=this;}}class C<T> extends B{extra:T;constructor(v:T){super();this.extra=v;}}export function main():void{new C<string>('late');}",
        "class B{self:B=this;constructor(){console.log(this.self.n);}n:number=1;}class C extends B{extra:string='late';}export function main():void{}",
        "class B{fn:(this:B)=>number=function(this:B):number{return this.n;};n:number=1;constructor(){this.fn();}}class C extends B{extra:string='late';}export function main():void{}",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(source, V38)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains(
                "fully initialized constructor receiver requires a class without descendants"
            ),
            "{error}"
        );
    }
    let old = "class C{n:number=1;constructor(){this.read();}read():number{return this.n;}}export function main():void{}";
    assert!(varta_compiler::compile_typescript_with_profile(old, P::CheckedF64Utf16V37).is_err());
}
