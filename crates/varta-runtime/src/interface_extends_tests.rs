use super::*;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use varta_common::language::LanguageProfile as P;
use varta_compiler::{CompilerError, ModuleLoader, ModuleSource};

struct Modules(BTreeMap<PathBuf, String>);
impl ModuleLoader for Modules {
    fn resolve(&self, _: &Path, specifier: &str) -> Result<PathBuf, CompilerError> {
        Ok(specifier.trim_start_matches("./").into())
    }
    fn load(&self, path: &Path) -> Result<String, CompilerError> {
        self.0
            .get(path)
            .cloned()
            .ok_or_else(|| CompilerError::Module("missing virtual module".into()))
    }
}
fn compile(entry: &str, modules: &[(&str, &str)], profile: P) -> Result<Vec<u8>, CompilerError> {
    let loader = Modules(
        modules
            .iter()
            .map(|(path, text)| (PathBuf::from(path), (*text).to_owned()))
            .collect(),
    );
    varta_compiler::compile_modules_with_profile(
        ModuleSource {
            path: "main.ts".into(),
            text: entry.into(),
        },
        &loader,
        profile,
    )
    .map(|compilation| compilation.wasm)
}
fn run(entry: &str, modules: &[(&str, &str)], expected: &[&str]) {
    let bytes =
        compile(entry, modules, P::CheckedF64Utf16V32).unwrap_or_else(|e| panic!("{e}\n{entry}"));
    // Emitted modules must also pass backend validation, not only compilation.
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{entry}"))
            .lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
}
fn rejected(source: &str, profile: P) -> String {
    let source = format!("{source}export function main():void{{}}");
    compile(&source, &[], profile)
        .err()
        .unwrap_or_else(|| panic!("accepted under {profile:?}:\n{source}"))
        .to_string()
}

#[test]
fn interfaces_extend_bases_and_combine_closed_shapes() {
    run(
        "interface Named{name:string}interface Sized{size:number;note?:string}\
         type Tagged={tag:'a'|'b'};\
         interface Item extends Named,Sized,Tagged{id:number;tag:'a'}\
         interface Wide extends Item{}\
         interface Diamond extends Named,Item{}\
         function show(i:Item):string{return i.name+'/'+String(i.size)+'/'+i.tag+'/'+String(i.id);}\
         export function main():void{const i:Item={id:1,name:'n',size:2,tag:'a'};console.log(show(i));\
         const w:Wide=i;const d:Diamond=w;console.log(JSON.stringify(d));\
         const p=varta.json.parse<Named>('{\"name\":\"parsed\"}');console.log(p.name);\
         const s:Sized={size:3};console.log(s.note===undefined);}",
        &[],
        &[
            "n/2/a/1",
            "{\"id\":1,\"name\":\"n\",\"size\":2,\"tag\":\"a\"}",
            "parsed",
            "1",
        ],
    );
}

#[test]
fn interfaces_redeclare_inherited_fields_compatibly_and_merge_declarations() {
    run(
        "interface Base{kind:string;limit?:number;value:number|string}\
         interface Narrow extends Base{kind:'n';limit:number;value:number}\
         interface Merged{a:number}interface Merged{b:string;a:number}\
         interface Merged extends Base{}\
         export function main():void{const n:Narrow={kind:'n',limit:4,value:5};\
         console.log(n.limit+n.value);const m:Merged={a:1,b:'x',kind:'k',value:'v'};\
         console.log(m.b+m.kind+String(m.a));}",
        &[],
        &["9", "xk1"],
    );
}

#[test]
fn interfaces_extend_bases_from_other_modules() {
    run(
        "import type {Page} from './page.ts';import {type Entry,type Listing,first} from './entry.ts';\
         interface AgentPage extends Page,Listing{total:number}\
         export function main():void{const e:Entry={id:'a'};const p:AgentPage={cursor:'c1',entry:e,total:2};\
         const l:Listing={entry:p.entry};console.log(first(l)+p.cursor+String(p.total));}",
        &[
            ("page.ts", "export interface Page{cursor:string}"),
            (
                "entry.ts",
                "export interface Entry{id:string}export interface Listing{entry:Entry}\
                 interface Local extends Listing{}\
                 export function first(l:Local):string{return l.entry.id;}",
            ),
        ],
        &["ac12"],
    );
}

#[test]
fn interface_extends_rejects_incompatible_and_unsupported_bases() {
    for (source, expected) in [
        (
            "interface A{x:number}interface B extends A{x:string}function f(b:B):void{}",
            "field x of interface B is not assignable to the field inherited from A",
        ),
        (
            "interface A{x:number}interface B extends A{x?:number}function f(b:B):void{}",
            "field x of interface B is optional but required in A",
        ),
        (
            "interface A{x:number}interface C{x:string}interface B extends A,C{}function f(b:B):void{}",
            "interface B inherits field x from A and C with different types",
        ),
        (
            "interface A{x:number}interface A{x:string}function f(a:A):void{}",
            "merged declarations of interface A declare field x with different types",
        ),
        (
            "interface A{x:number;x:number}interface B extends A{}function f(b:B):void{}",
            "duplicate object key",
        ),
        (
            "interface A extends B{}interface B extends A{}function f(a:A):void{}",
            "recursive type declaration",
        ),
        (
            "interface A extends A{}function f(a:A):void{}",
            "recursive type declaration",
        ),
        (
            "type U=number|string;interface A extends U{}function f(a:A):void{}",
            "interface A extends U, which is not a closed record type",
        ),
        (
            "class C{x=1}interface A extends C{}function f(a:A):void{}",
            "interface A extends class C; class instance types are nominal",
        ),
        (
            "interface A extends Missing{}function f(a:A):void{}",
            "type reference Missing",
        ),
        (
            // Generic bases follow the v32 generics rules after integration.
            "interface G{v:number}interface A extends G<number>{}function f(a:A):void{}",
            "type G is not generic and takes no type arguments",
        ),
        (
            "interface Resources{a:number}interface Client extends Resources{}class Client{}",
            "declaration merging of interface Client with class Client",
        ),
        (
            "class Client{}interface Client{}",
            "declaration merging of interface Client with class Client",
        ),
        (
            "type A=number;interface A{x:number}function f(a:A):void{}",
            "duplicate type declaration: A",
        ),
        (
            "export interface A{x:number}interface A{y:number}",
            "declarations of merged interface A must be all exported or all local",
        ),
        (
            "interface A{x:number}interface B extends A{y:string}function f(b:B):void{const a:A=b;}",
            "type mismatch",
        ),
    ] {
        let error = rejected(source, P::CheckedF64Utf16V32);
        assert!(error.contains(expected), "{source}\n{error}");
    }
}

#[test]
fn merged_exported_interfaces_export_one_type() {
    run(
        "import type {Options} from './options.ts';\
         export function main():void{const o:Options={retries:2,timeout:5};console.log(o.retries*o.timeout);}",
        &[(
            "options.ts",
            "export interface Options{retries:number}export interface Options{timeout:number}",
        )],
        &["10"],
    );
}

#[test]
fn interface_extends_and_merging_stay_rejected_before_v32() {
    for (source, expected) in [
        (
            "interface A{x:number}interface B extends A{y:number}",
            "interface extends",
        ),
        (
            "interface A{x:number}interface A{y:number}",
            "duplicate type declaration: A",
        ),
    ] {
        let error = rejected(source, P::CheckedF64Utf16V31);
        assert!(error.contains(expected), "{source}\n{error}");
    }
}
