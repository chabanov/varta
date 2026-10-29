//! Interactions between the second-wave v32 pilot streams: generics, default
//! exports, interface inheritance and Error subclasses, in one module and
//! across a virtual module graph.
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
fn execute(entry: &str, modules: &[(&str, &str)]) -> Result<RunOutput, RuntimeError> {
    let bytes =
        compile(entry, modules, P::CheckedF64Utf16V32).unwrap_or_else(|e| panic!("{e}\n{entry}"));
    // Emitted modules must also pass backend validation, not only compilation.
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[])
}
fn run(entry: &str, modules: &[(&str, &str)], expected: &[&str]) {
    assert_eq!(
        execute(entry, modules)
            .unwrap_or_else(|e| panic!("{e:?}\n{entry}"))
            .lines,
        expected
            .iter()
            .map(|s| format!("varta log: {s}"))
            .collect::<Vec<_>>()
    );
}
fn rejected(entry: &str, modules: &[(&str, &str)], profile: P) -> String {
    compile(entry, modules, profile)
        .err()
        .unwrap_or_else(|| panic!("accepted under {profile:?}:\n{entry}"))
        .to_string()
}

#[test]
fn generic_interfaces_extend_generic_bases_across_modules() {
    run(
        "import type {Page} from './page.ts';\
         interface Agent{id:string}\
         interface CursorPage<T> extends Page<T>{cursor:string|null}\
         interface Tagged<T,K extends string='tag'> extends CursorPage<T>{kind:K}\
         function count<T>(page:CursorPage<T>):number{return page.items.length;}\
         export function main():void{\
         const page:CursorPage<Agent>={items:[{id:'a1'},{id:'a2'}],cursor:null};\
         console.log(count(page));console.log(count<number>({items:[1],cursor:'c'}));\
         const plain:{items:Agent[];cursor:string|null}=page;const second=plain.items[1];console.log(second===undefined?'':second.id);\
         const t:Tagged<boolean>={items:[true],cursor:null,kind:'tag'};console.log(t.kind+t.items.length);\
         console.log(JSON.stringify(t));}",
        &[("page.ts", "export interface Page<T>{items:T[]}")],
        &[
            "2",
            "1",
            "a2",
            "tag1",
            "{\"items\":[true],\"cursor\":null,\"kind\":\"tag\"}",
        ],
    );
}

#[test]
fn generic_interface_inheritance_rejects_invalid_forms_precisely() {
    for (entry, expected) in [
        (
            "interface A<T>{x:T}interface B<T> extends A{}function f(b:B<number>):void{}",
            "type A requires a type argument for T",
        ),
        (
            "interface A<T>{x:T}interface B<T> extends A<T>{x:string}function f(b:B<number>):void{}",
            "field x of interface B is not assignable to the field inherited from A",
        ),
        (
            "interface B<T> extends B<T>{x:T}function f(b:B<number>):void{}",
            "recursive type declaration",
        ),
        (
            "interface A<T>{x:T}interface A<T>{y:T}function f(a:A<number>):void{}",
            "declaration merging of generic interface A",
        ),
        (
            "interface A{x:number}interface A<T>{y:T}function f(a:A<number>):void{}",
            "declaration merging of generic interface A",
        ),
        (
            "interface A<T extends string>{x:T}interface B<T> extends A<T>{}function f(b:B<number>):void{}",
            "does not satisfy its constraint",
        ),
    ] {
        let source = format!("{entry}export function main():void{{}}");
        let error = rejected(&source, &[], P::CheckedF64Utf16V32);
        assert!(error.contains(expected), "{source}\n{error}");
    }
}

#[test]
fn generic_functions_and_types_are_default_exportable() {
    run(
        "import first, {type Box} from './first.ts';import Pair from './pair.ts';\
         import wrap from './wrap.ts';\
         export function main():void{console.log(first([4,5],0));console.log(first<string>([],'none'));\
         const p:Pair<number>={a:1,b:2};const b:Box<string>={value:'v'};\
         console.log(p.a+p.b);console.log(b.value);console.log(wrap(true).value);}",
        &[
            (
                "first.ts",
                "export interface Box<T>{value:T}\
                 export default function first<T>(xs:T[],fallback:T):T{const x=xs[0];return x===undefined?fallback:x;}",
            ),
            ("pair.ts", "export default interface Pair<T>{a:T;b:T}"),
            (
                "wrap.ts",
                "import type {Box} from './first.ts';\
                 function wrap<T>(value:T):Box<T>{return {value};}export default wrap;",
            ),
        ],
        &["4", "none", "3", "v", "1"],
    );
}

#[test]
fn default_exported_generic_functions_keep_generic_value_rules() {
    for (entry, modules, expected) in [
        (
            "import f from './f.ts';export function main():void{const g=f;}",
            &[("f.ts", "export default function f<T>(x:T):T{return x;}")][..],
            "generic function as a value",
        ),
        (
            "import f from './f.ts';export function main():void{}",
            &[("f.ts", "export default function <T>(x:T):T{return x;}")][..],
            "",
        ),
    ] {
        if expected.is_empty() {
            compile(entry, modules, P::CheckedF64Utf16V32).unwrap_or_else(|e| panic!("{e}"));
        } else {
            let error = rejected(entry, modules, P::CheckedF64Utf16V32);
            assert!(error.contains(expected), "{entry}\n{error}");
        }
    }
}

const ERRORS: &str = "export default class UarpError extends Error{\
    constructor(message:string,options?:{cause?:string}){super(message,options);this.name=new.target.name;}}\
    export class APIError extends UarpError{readonly status:number;\
    constructor(status:number){super(`HTTP ${status}`);this.status=status;}}\
    export class NotFoundError extends APIError{}";

#[test]
fn default_exported_error_subclasses_link_through_star_reexports() {
    run(
        "import UarpError, {APIError, NotFoundError, fail} from './index.ts';\
         import Base from './errors.ts';\
         export function main():void{\
         try{fail(404);}catch(e){console.log(e instanceof NotFoundError);\
         if(e instanceof APIError){console.log(e.toString()+' '+e.status);}\
         console.log(e instanceof Base);console.log(e instanceof UarpError);}\
         try{fail(500);}catch(e){if(e instanceof UarpError){console.log(e.name);\
         console.log(e instanceof NotFoundError);}}\
         const direct=new UarpError('m',{cause:'c'});console.log(direct.cause==='c');}",
        &[
            ("errors.ts", ERRORS),
            (
                "index.ts",
                "export * from './errors.ts';export {default} from './errors.ts';\
                 export {fail} from './fail.ts';",
            ),
            (
                "fail.ts",
                "import {APIError,NotFoundError} from './errors.ts';\
                 export function fail(status:number):void{\
                 if(status===404){throw new NotFoundError(status);}throw new APIError(status);}",
            ),
        ],
        &[
            "1",
            "NotFoundError: HTTP 404 404",
            "1",
            "1",
            "APIError",
            "0",
            "1",
        ],
    );
}

#[test]
fn star_reexports_never_forward_a_default_error_class() {
    let error = rejected(
        "import UarpError from './index.ts';export function main():void{}",
        &[
            ("errors.ts", ERRORS),
            ("index.ts", "export * from './errors.ts';"),
        ],
        P::CheckedF64Utf16V32,
    );
    assert!(error.contains("has no default export"), "{error}");
}

#[test]
fn error_subclass_instances_flow_through_generic_functions() {
    run(
        &format!(
            "{}\
             function identity<T>(x:T):T{{return x;}}\
             function first<T>(xs:T[],fallback:T):T{{const x=xs[0];return x===undefined?fallback:x;}}\
             function raise<E extends UarpError>(error:E):void{{throw error;}}\
             function status<E extends APIError>(error:E):number{{return error.status;}}\
             export function main():void{{\
             const e=identity(new NotFoundError(404));console.log(e.toString());console.log(status(e));\
             const errors:APIError[]=[new APIError(500),e];\
             const picked=first(errors,new APIError(0));console.log(picked.name+' '+picked.status);\
             try{{raise(e);}}catch(caught){{if(caught instanceof NotFoundError){{console.log(caught.message);}}}}\
             try{{raise<UarpError>(new UarpError('plain'));}}catch(caught){{\
             console.log(caught instanceof APIError);console.log(caught instanceof Error);}}}}",
            ERRORS.replace("export default class", "export class")
        ),
        &[],
        &[
            "NotFoundError: HTTP 404",
            "404",
            "APIError 500",
            "HTTP 404",
            "0",
            "1",
        ],
    );
}

#[test]
fn generic_classes_and_generic_error_subclasses_stay_rejected() {
    for (entry, expected) in [
        (
            "class Box<T>{value:T;constructor(value:T){this.value=value;}}",
            "generic class",
        ),
        (
            "class Failure<T> extends Error{detail:T;constructor(detail:T){super('x');this.detail=detail;}}",
            "generic class",
        ),
        (
            "class Failure extends Error{}function g(x:Failure<number>):void{}",
            "generic class type reference",
        ),
    ] {
        let source = format!("{entry}export function main():void{{}}");
        let error = rejected(&source, &[], P::CheckedF64Utf16V32);
        assert!(error.contains(expected), "{source}\n{error}");
    }
}
