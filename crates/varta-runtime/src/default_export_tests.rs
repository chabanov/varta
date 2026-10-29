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
fn rejected(entry: &str, modules: &[(&str, &str)], profile: P) -> String {
    compile(entry, modules, profile)
        .err()
        .unwrap_or_else(|| panic!("accepted under {profile:?}:\n{entry}"))
        .to_string()
}

#[test]
fn default_exports_bind_functions_classes_and_constants() {
    run(
        "import add from './add.ts';import Counter,{start} from './counter.ts';\
         import LIMIT from './limit.ts';import pair from './pair.ts';import anon from './anon.ts';\
         import type Shape from './shape.ts';\
         export function main():void{const c:Counter=new Counter(start());c.bump();\
         console.log(add(c.value,LIMIT));const s:Shape={w:2};console.log(s.w+pair.size);\
         console.log(anon(4));}",
        &[
            (
                "add.ts",
                "export default function add(a:number,b:number):number{return a+b;}",
            ),
            (
                "counter.ts",
                "export function start():number{return 40;}\
                 export default class Counter{value:number;constructor(v:number){this.value=v;}bump():void{this.value=this.value+1;}}",
            ),
            ("limit.ts", "export default 2;"),
            ("pair.ts", "export default {name:'p',size:5};"),
            (
                "anon.ts",
                "export default function(n:number):number{return n*n;}",
            ),
            ("shape.ts", "export default interface Shape{w:number}"),
        ],
        &["43", "7", "16"],
    );
}

#[test]
fn default_exports_name_existing_bindings_and_forward_through_reexports() {
    run(
        "import first,{second,default as third,default3} from './barrel.ts';import Box from './box.ts';\
         export function main():void{console.log(first()+second+third()+default3());\
         const b=new Box();console.log(b instanceof Box);console.log(b.n);}",
        &[
            (
                "one.ts",
                "function one():number{return 1;}export {one as default};",
            ),
            ("two.ts", "const TWO=20;export default TWO;"),
            (
                "barrel.ts",
                "export {default} from './one.ts';export {default as second} from './two.ts';\
                 import hundred from './three.ts';export {hundred as default3};export * from './three.ts';",
            ),
            (
                "three.ts",
                "export default function h():number{return 100;}",
            ),
            ("box.ts", "class Box{n=7}export default Box;"),
        ],
        &["122", "1", "7"],
    );
    // `third` names `default` of the barrel, which is `one`; `export *` never
    // forwards the default of `three.ts`.
    let error = rejected(
        "import x from './star.ts';export function main():void{}",
        &[
            ("star.ts", "export * from './three.ts';"),
            (
                "three.ts",
                "export default function h():number{return 100;}",
            ),
        ],
        P::CheckedF64Utf16V32,
    );
    assert!(
        error.contains("module star.ts has no default export"),
        "{error}"
    );
}

#[test]
fn default_exports_reject_ambiguous_and_unsupported_forms() {
    let lib = [("lib.ts", "export default function f():number{return 1;}")];
    for (entry, modules, expected) in [
        (
            "import * as ns from './lib.ts';export function main():void{}",
            &lib[..],
            "namespace import (import * as name)",
        ),
        (
            "export * as ns from './lib.ts';export function main():void{}",
            &lib[..],
            "namespace re-export",
        ),
        (
            "import f from './none.ts';export function main():void{}",
            &[("none.ts", "export function f():number{return 1;}")][..],
            "module none.ts has no default export",
        ),
        (
            "export default 1;export default 2;export function main():void{}",
            &[][..],
            "duplicate export: default",
        ),
        (
            "function f():void{}export default f;export {f as default};export function main():void{}",
            &[][..],
            "duplicate export: default",
        ),
        (
            "export default LIMIT;const LIMIT=3;export function main():void{}",
            &[][..],
            "default export of LIMIT precedes its declaration",
        ),
        (
            "export default C;class C{}export function main():void{}",
            &[][..],
            "default export of C precedes its declaration",
        ),
        (
            "export default Missing;export function main():void{}",
            &[][..],
            "reference to Missing in a module constant initializer",
        ),
        (
            "function f():number{return 1;}export default f();export function main():void{}",
            &[][..],
            "call in a module constant initializer",
        ),
        (
            "export {default};export function main():void{}",
            &[][..],
            "without `from` clause",
        ),
    ] {
        let error = rejected(entry, modules, P::CheckedF64Utf16V32);
        assert!(error.contains(expected), "{entry}\n{error}");
    }
}

#[test]
fn default_exports_stay_rejected_before_v32() {
    let lib = [("lib.ts", "export function f():number{return 1;}")];
    for profile in [P::CheckedF64Utf16V31, P::default()] {
        for (entry, expected) in [
            (
                "import f from './lib.ts';export function main():void{}",
                "default or namespace import",
            ),
            (
                "export default function main():void{}",
                "module export form",
            ),
            (
                "export default 1;export function main():void{}",
                "module export form",
            ),
            (
                "import {f} from './lib.ts';export {f as default};export function main():void{}",
                "string or default export name",
            ),
        ] {
            let error = rejected(entry, &lib, profile);
            assert!(error.contains(expected), "{profile:?} {entry}\n{error}");
        }
    }
}
