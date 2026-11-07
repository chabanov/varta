//! V38 declarations and classes (`declaration_forms`, K9 B): enums,
//! namespaces and declaration merging, abstract classes, overloads,
//! setters and auto-accessors, static fields and static inheritance, class
//! expressions, `declare`/`override` and parameter-property defaults
//! (`declaration_form_cases/*.ts`). Every `*.expected` file is the output of
//! Node 24.20.0 running the program as tsc 5.9.3 emits it (strict,
//! ES2022, `useDefineForClassFields`) with `await main();` appended,
//! recorded once and checked in. Every program runs under the production
//! budgets and again in collector stress builds; the refused forms fail
//! with their named diagnostics (TypeScript's error code where tsc rejects
//! the program too), and v37 keeps rejecting every new form.
use super::*;
use crate::codegen_v2_tests::run_test_budgets;
use crate::collector_stress_tests::stressed;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use varta_common::language::LanguageProfile;
use varta_compiler::{CompilerError, ModuleLoader, ModuleSource};

const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;
const V37: LanguageProfile = LanguageProfile::CheckedF64Utf16V37;
/// Test-only fuel budgets of one stress run, in production fuel budgets:
/// stress builds collect continuously (as in `collector_stress_tests`).
const STRESS_BUDGETS: u64 = 400;
/// Work units per allocation of the stress builds.
const STRESS_UNITS: [u32; 2] = [2, 64];

const CASES: &[(&str, &str, &str)] = &[
    (
        "enums",
        include_str!("declaration_form_cases/enums.ts"),
        include_str!("declaration_form_cases/enums.expected"),
    ),
    (
        "namespaces",
        include_str!("declaration_form_cases/namespaces.ts"),
        include_str!("declaration_form_cases/namespaces.expected"),
    ),
    (
        "abstract",
        include_str!("declaration_form_cases/abstract.ts"),
        include_str!("declaration_form_cases/abstract.expected"),
    ),
    (
        "overloads",
        include_str!("declaration_form_cases/overloads.ts"),
        include_str!("declaration_form_cases/overloads.expected"),
    ),
    (
        "accessors",
        include_str!("declaration_form_cases/accessors.ts"),
        include_str!("declaration_form_cases/accessors.expected"),
    ),
    (
        "statics",
        include_str!("declaration_form_cases/statics.ts"),
        include_str!("declaration_form_cases/statics.expected"),
    ),
    (
        "class_expressions",
        include_str!("declaration_form_cases/class_expressions.ts"),
        include_str!("declaration_form_cases/class_expressions.expected"),
    ),
    (
        "declarations",
        include_str!("declaration_form_cases/declarations.ts"),
        include_str!("declaration_form_cases/declarations.expected"),
    ),
    (
        "enum_objects",
        include_str!("declaration_form_cases/enum_objects.ts"),
        include_str!("declaration_form_cases/enum_objects.expected"),
    ),
    (
        "enum_typing",
        include_str!("declaration_form_cases/enum_typing.ts"),
        include_str!("declaration_form_cases/enum_typing.expected"),
    ),
    (
        "index_signatures",
        include_str!("declaration_form_cases/index_signatures.ts"),
        include_str!("declaration_form_cases/index_signatures.expected"),
    ),
    (
        "namespace_objects",
        include_str!("declaration_form_cases/namespace_objects.ts"),
        include_str!("declaration_form_cases/namespace_objects.expected"),
    ),
    (
        "module_evaluation",
        include_str!("declaration_form_cases/module_evaluation.ts"),
        include_str!("declaration_form_cases/module_evaluation.expected"),
    ),
    (
        "class_values",
        include_str!("declaration_form_cases/class_values.ts"),
        include_str!("declaration_form_cases/class_values.expected"),
    ),
    (
        "static_receivers",
        include_str!("declaration_form_cases/static_receivers.ts"),
        include_str!("declaration_form_cases/static_receivers.expected"),
    ),
    (
        "generic_overloads",
        include_str!("declaration_form_cases/generic_overloads.ts"),
        include_str!("declaration_form_cases/generic_overloads.expected"),
    ),
    (
        "index_json",
        include_str!("declaration_form_cases/index_json.ts"),
        include_str!("declaration_form_cases/index_json.expected"),
    ),
    (
        "module_tdz",
        include_str!("declaration_form_cases/module_tdz.ts"),
        include_str!("declaration_form_cases/module_tdz.expected"),
    ),
    (
        "class_tdz",
        include_str!("declaration_form_cases/class_tdz.ts"),
        include_str!("declaration_form_cases/class_tdz.expected"),
    ),
    (
        "early_reads",
        include_str!("declaration_form_cases/early_reads.ts"),
        include_str!("declaration_form_cases/early_reads.expected"),
    ),
    (
        "static_store",
        include_str!("declaration_form_cases/static_store.ts"),
        include_str!("declaration_form_cases/static_store.expected"),
    ),
    (
        "static_super",
        include_str!("declaration_form_cases/static_super.ts"),
        include_str!("declaration_form_cases/static_super.expected"),
    ),
    // The `docs/language-guide.md` example, verbatim.
    (
        "guide",
        include_str!("declaration_form_cases/guide.ts"),
        include_str!("declaration_form_cases/guide.expected"),
    ),
];

fn compile_in(profile: LanguageProfile, source: &str) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}

fn lines(bytes: &[u8]) -> Vec<String> {
    let output = VartaRuntime::new()
        .run_wasm_bytes(bytes, &[])
        .unwrap_or_else(|e| panic!("{e:?}"));
    output
        .lines
        .iter()
        .map(|line| {
            line.strip_prefix("varta log: ")
                .unwrap_or_else(|| panic!("not a log line: {line}"))
                .to_owned()
        })
        .collect()
}

fn case(name: &str) -> (&'static str, &'static str) {
    CASES
        .iter()
        .find(|(case, ..)| *case == name)
        .map(|(_, source, expected)| (*source, *expected))
        .unwrap_or_else(|| panic!("missing case {name}"))
}

/// The program's log equals the Node output recorded for `name`.
fn matches_node(name: &str) {
    let (source, expected) = case(name);
    let expected: Vec<&str> = expected.lines().collect();
    assert_eq!(lines(&compile_in(V38, source)), expected, "{name}");
}

fn refused(profile: LanguageProfile, source: &str, diagnostic: &str) {
    let error = varta_compiler::compile_typescript_with_profile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("{profile:?} compiled\n{source}"))
        .to_string();
    assert!(error.contains(diagnostic), "{profile:?}: {error}\n{source}");
}

#[test]
fn enums_match_node() {
    matches_node("enums");
}

#[test]
fn namespaces_and_merging_match_node() {
    matches_node("namespaces");
}

#[test]
fn abstract_classes_match_node() {
    matches_node("abstract");
}

#[test]
fn overloads_match_node() {
    matches_node("overloads");
}

#[test]
fn accessors_match_node() {
    matches_node("accessors");
}

#[test]
fn static_members_match_node() {
    matches_node("statics");
}

#[test]
fn class_expressions_match_node() {
    matches_node("class_expressions");
}

#[test]
fn declare_override_and_parameter_properties_match_node() {
    matches_node("declarations");
}

/// A `number` is assignable to a numeric enum type, a numeric literal only
/// if it is a member value, and a string enum type is nominal (tsc 5).
#[test]
fn enum_assignability_matches_tsc() {
    matches_node("enum_typing");
    refused(
        V38,
        "enum D { Up = 'UP', Down = 'DOWN' } function f(d: D): D { return d; } export function main(): void { console.log(f('UP')); }",
        "value 'UP' is not assignable",
    );
    refused(
        V38,
        "enum L { Low = 1, High } export function main(): void { let l: L = L.Low; l = 7; console.log(String(l)); }",
        "is not assignable",
    );
}

/// Class index signatures: undeclared members are a hidden dictionary,
/// serialized as own properties in own-key order; members that do not fit
/// the index type are TS2411.
#[test]
fn class_index_signatures_match_node() {
    matches_node("index_signatures");
    matches_node("index_json");
    refused(
        V38,
        "class C { [key: string]: number; m(): number { return 1; } } export function main(): void { console.log('x'); }",
        "property m of type method or accessor is not assignable to the string index type number (TS2411)",
    );
    refused(
        V38,
        "class C { [key: string]: number; name = 'x'; } export function main(): void { console.log('x'); }",
        "(TS2411)",
    );
    refused(
        V38,
        "class C { [key: number]: string; } export function main(): void { console.log('x'); }",
        "class index signature with a key other than string",
    );
}

#[test]
fn enum_and_namespace_objects_match_node() {
    matches_node("enum_objects");
    matches_node("namespace_objects");
}

#[test]
fn module_evaluation_and_static_blocks_match_node() {
    matches_node("module_evaluation");
}

/// A module binding read or written before its initialization by code
/// that runs during module evaluation throws `ReferenceError` at run time;
/// only a read while an initializer is evaluated, which tsc reports too, is
/// refused (TS2448). A `var` or static field read early reads `undefined`
/// as in JavaScript, a checked read: a declared type that does not admit it
/// throws `TypeError` there (owner decision of 2026-11-07).
#[test]
fn module_temporal_dead_zone_matches_node() {
    matches_node("module_tdz");
    matches_node("early_reads");
    matches_node("class_tdz");
    for (source, diagnostic) in [
        (
            "const a = b; const b = 1; export function main(): void { console.log(String(a)); }",
            "binding used before initialization: b (TS2448)",
        ),
        (
            "const a = (() => b)(); const b = 1; export function main(): void { console.log(String(a)); }",
            "binding used before initialization: b (TS2448)",
        ),
        (
            "export function main(): void { console.log(String(x)); const x = 1; }",
            "binding used before initialization: x (TS2448)",
        ),
        (
            "const w = new Widget(); class Widget {} export function main(): void { console.log(String(w instanceof Widget)); }",
            "class Widget used before its declaration (TS2449)",
        ),
    ] {
        refused(V38, source, diagnostic);
    }
    // Uncaught, the error ends the program as in Node.
    let bytes = compile_in(
        V38,
        "function f(): number { return M; } const r = f(); const M = 1; export function main(): void { console.log(String(r)); }",
    );
    let error = VartaRuntime::new()
        .run_wasm_bytes(&bytes, &[])
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(
        error.contains("ReferenceError")
            && error.contains("Cannot access 'M' before initialization"),
        "{error}"
    );
}

/// Classes as values (`typeof C`): `new`, static calls, `name` and `===`
/// dispatch over the closed set of subclasses; `this` in a static method
/// is the class it is called through, also through a class value.
#[test]
fn class_values_match_node() {
    matches_node("class_values");
    matches_node("static_receivers");
    for (source, diagnostic) in [
        (
            "class A { private static n = 1; } export function main(): void { const k = A; console.log(String(k.n)); }",
            "is private",
        ),
        (
            "class A {} export function main(): void { const ks: (typeof A)[] = [A]; console.log(String(ks.length)); }",
            "array element type",
        ),
        (
            "class A {} export function main(): void { const k: typeof A | null = A; console.log(String(k === null)); }",
            "class value in a union type",
        ),
        (
            "class A { constructor(public n: number) {} } class B extends A { constructor() { super(1); } } function make(k: typeof A): A { return new k(2); } export function main(): void { console.log(String(make(A).n)); }",
            "the constructor of class B does not accept the arguments of the constructor of class A",
        ),
        (
            "abstract class A { abstract f(): number; } function make(k: typeof A): A { return new k(); } export function main(): void { console.log('x'); }",
            "cannot create an instance of the abstract class A (TS2511)",
        ),
        (
            "class A {} export function main(): void { const k = A; console.log(String(k === 1)); }",
            "comparison of a class value with number (TS2367)",
        ),
    ] {
        refused(V38, source, diagnostic);
    }
    refused(
        V37,
        "class A {} export function main(): void { const k = A; console.log(typeof k); }",
        "unknown identifier: A",
    );
}

/// Static fields per class (`static_store`): a write through a subclass
/// creates its own property, in the declaring module and across modules,
/// through class values, `this` in static code, compound, logical and
/// update assignments; `Object.keys` and `for…in` list them. Node 24 (on
/// tsc 5's ES2022 emit) prints the same lines.
#[test]
fn static_store_matches_node() {
    matches_node("static_store");
    let lib = "export class A {
           static x: number = 1;
           static tag: string = 'a';
           static bump(): number { this.x = this.x + 10; return this.x; }
         }
         export class B extends A {}
         export function readB(): number { return B.x; }";
    let entry = "import { A, B, readB } from './lib.ts';
         class D extends B {
           static own(): string { return this.tag; }
         }
         export function main(): void {
           console.log(String(D.x) + ' ' + D.own());
           D.x = 5;
           D.tag += 'd';
           console.log(String(A.x) + ' ' + String(B.x) + ' ' + String(D.x) + ' ' + D.tag + ' ' + A.tag);
           console.log(String(D.bump()) + ' ' + String(B.bump()) + ' ' + String(readB()));
           A.x = 100;
           console.log(String(A.x) + ' ' + String(B.x) + ' ' + String(D.x) + ' ' + Object.keys(D).join(','));
         }";
    let bytes = compile_modules(entry, &[("lib.ts", lib)]).unwrap();
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    assert_eq!(
        lines(&bytes),
        ["1 a", "1 1 5 ad a", "15 11 11", "100 11 15 x,tag"]
    );
    // A field typed by its initializer has the store too.
    let inferred = compile_in(
        V38,
        "class A { static n = make(); } function make(): number { return 1; } class B extends A {}
         export function main(): void { const k: typeof A = B; k.n = 5; console.log(String(A.n) + ' ' + String(k.n)); }",
    );
    assert_eq!(lines(&inferred), ["1 5"]);
    for (source, diagnostic) in [
        (
            "class A { static n: number = 1; } class B extends A {} export function main(): void { console.log(Object.values(B).length); }",
            "Object.values of a class object is typed any[]",
        ),
        (
            "class A { static readonly n: number = 1; } class B extends A {} export function main(): void { const k: typeof A = B; k.n++; }",
            "cannot assign to n because it is a read-only static field (TS2540)",
        ),
    ] {
        refused(V38, source, diagnostic);
    }
}

/// A `this`-using static method inherited by a subclass declared in
/// another module runs with that subclass as `this` (its receiver
/// version); before, it silently ran with the declaring class. Node 24
/// prints the same line.
#[test]
fn static_methods_inherited_across_modules_match_node() {
    let model = "function prefix(): string { return 'table:'; }
         export class Model {
           static table(): string { return 'models'; }
           static describe(): string { return prefix() + this.table() + ' ' + this.name; }
           static count = 0;
           static bump(): number { this.count++; return this.count; }
           static create(): Model { return new this(); }
           kind(): string { return 'model'; }
         }";
    let entry = "import { Model } from './model.ts';
         class User extends Model {
           static table(): string { return 'users'; }
           kind(): string { return 'user'; }
         }
         export function main(): void {
           console.log(User.describe() + ' ' + User.create().kind() + ' ' + Model.describe() + ' ' + String(Model.bump()));
         }";
    let bytes = compile_modules(entry, &[("model.ts", model)]).unwrap();
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    assert_eq!(
        lines(&bytes),
        ["table:users User user table:models Model 1"]
    );
    // `this.count++` through User creates User's own static property
    // (`static_store`); Model's stays apart.
    let writes = "import { Model } from './model.ts';
         class User extends Model {}
         export function main(): void {
           console.log(String(User.bump()) + ' ' + String(User.bump()) + ' ' + String(Model.bump())
             + ' ' + String(Model.count) + ' ' + String(User.count));
         }";
    let bytes = compile_modules(writes, &[("model.ts", model)]).unwrap();
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    assert_eq!(lines(&bytes), ["1 2 1 1 2"]);
}

/// Generic overload signatures of functions, methods and static methods
/// (also in generic class instances) infer their type arguments per
/// candidate (constraints included) and check the result against the
/// instantiated signature.
#[test]
fn generic_overloads_match_node() {
    matches_node("generic_overloads");
}

#[test]
fn super_in_static_methods_matches_node() {
    matches_node("static_super");
}

#[test]
fn declaration_language_guide_example_matches_node() {
    matches_node("guide");
}

/// Hoisted statics, accessor receivers and overload arguments stay rooted:
/// the stress builds print what the normal build prints.
#[test]
fn declaration_forms_survive_collector_stress() {
    for (name, source, expected) in CASES {
        let expected: Vec<String> = expected
            .lines()
            .map(|line| format!("varta log: {line}"))
            .collect();
        for units in STRESS_UNITS {
            let run = run_test_budgets(&stressed(source, units), STRESS_BUDGETS);
            assert_eq!(
                (&run.lines, &run.error),
                (&expected, &None),
                "{name}: stress build ({units} units per allocation)"
            );
        }
    }
}

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

fn compile_modules(entry: &str, modules: &[(&str, &str)]) -> Result<Vec<u8>, CompilerError> {
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
        V38,
    )
    .map(|c| c.wasm)
}

/// A class declared in a function is a module class; Node 24 prints the
/// same line.
#[test]
fn local_classes_match_node() {
    let source = "function make(n: number): string {
           class Box { constructor(public n: number) {} twice(): Box { return new Box(this.n * 2); } }
           return String(new Box(n).twice().n);
         }
         export function main(): void {
           class Box { label = 'outer'; }
           console.log(make(3) + ' ' + new Box().label);
         }";
    assert_eq!(lines(&compile_in(V38, source)), ["6 outer"]);
    refused(V37, source, "class");
}

/// Exported enums, namespaces and classes with static fields are used from
/// another module, also under an import alias, through their generated
/// declarations. Node 24 prints the same lines.
#[test]
fn imported_declarations_match_node() {
    let library = "export enum Level { Low = 1, Mid, High }
         export const enum Unit { Px = 'px' }
         export namespace Theme {
           export const base = 4;
           export function space(level: Level): string { return String(base * level) + Unit.Px; }
           export namespace Names { export const title = 'theme'; }
           export interface Box { width: number }
         }
         export class Counter {
           static count = 0;
           static readonly label: string = 'counter';
           static bump(): number { Counter.count++; return Counter.count; }
         }";
    let entry = "import { Level, Theme, Counter } from './lib.ts';
         import { Level as L, Theme as T } from './lib.ts';
         export function main(): void {
           let level = Level.Mid;
           level = L.High;
           const box: Theme.Box = { width: Theme.base };
           console.log(Level[level] + ' ' + Theme.space(level) + ' ' + T.Names.title + ' ' + String(box.width));
           Counter.bump();
           console.log(String(Counter.bump()) + ' ' + String(Counter.count) + ' ' + Counter.label + ' ' + L[1]);
         }";
    let bytes = compile_modules(entry, &[("lib.ts", library)]).unwrap();
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    assert_eq!(lines(&bytes), ["High 12px theme 4", "2 2 counter Low"]);
    // An imported class's static field is written through the generated
    // setter its module exports; a readonly one stays read-only.
    let write = "import { Counter } from './lib.ts';
         export function main(): void {
           Counter.count = 10; Counter.count += 5; const before = Counter.count++; ++Counter.count;
           console.log(String(before) + ' ' + String(Counter.count) + ' ' + String(Counter.bump()));
         }";
    let bytes = compile_modules(write, &[("lib.ts", library)]).unwrap();
    assert_eq!(lines(&bytes), ["15 17 18"]);
    let readonly = "import { Counter } from './lib.ts';
         export function main(): void { Counter.label = 'x'; }";
    let error = compile_modules(readonly, &[("lib.ts", library)])
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    assert!(error.contains("read-only static field (TS2540)"), "{error}");
}

/// `export { E } from`, `export *` and `export { E }` of an import forward
/// enums, namespaces and classes with static fields with their generated
/// declarations; static fields of a class in a namespace are reached
/// through it, also from another module. Node 24 prints the same lines.
#[test]
fn re_exported_declarations_match_node() {
    let a = "export enum Level { Low = 1, Mid, High }
         export namespace Units { export const scale = 2; export function cm(n: number): string { return String(n * scale) + 'cm'; } }
         export class Counter { static count = 0; static bump(): number { Counter.count++; return Counter.count; } }
         export namespace Store {
           export class Item {
             static created = 0;
             static readonly kind: string = 'item';
             constructor(public name: string) { Item.created++; }
             static make(name: string): Item { return new this(name); }
             static total(): string { return this.kind + ':' + String(this.created); }
           }
         }";
    let b = "export { Level, Units } from './a.ts';
         export * from './c.ts';
         import { Counter, Store } from './a.ts';
         export { Counter, Store };";
    let c = "export enum Tone { Warm = 'warm' }
         export enum Level { Other }";
    let entry = "import { Level, Units, Tone, Counter, Store } from './b.ts';
         export function main(): void {
           let level = Level.Mid;
           level = Level.High;
           console.log(Level[level] + ' ' + Units.cm(level) + ' ' + Tone.Warm);
           Counter.count = 4;
           Store.Item.make('a');
           Store.Item.created += 10;
           console.log(String(Counter.bump()) + ' ' + Store.Item.total());
         }";
    let bytes = compile_modules(entry, &[("a.ts", a), ("b.ts", b), ("c.ts", c)]).unwrap();
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    assert_eq!(lines(&bytes), ["High 6cm warm", "5 item:11"]);
}

#[test]
fn unsupported_declaration_forms_are_refused_by_name() {
    for (source, diagnostic) in [
        (
            "abstract class A { abstract f(): number; } export function main(): void { const a = new A(); console.log(String(a.f())); }",
            "cannot create an instance of the abstract class A (TS2511)",
        ),
        (
            "abstract class A { abstract f(): number; } class B extends A {} export function main(): void { console.log('x'); }",
            "non-abstract class B does not implement inherited abstract member f from class A (TS2515)",
        ),
        (
            "abstract class A { abstract f(): number; } class B extends A { f(): number { return super.f(); } } export function main(): void { console.log('x'); }",
            "abstract method f cannot be accessed via super (TS2513)",
        ),
        (
            "abstract class A { abstract kind: string; } class B extends A {} export function main(): void { console.log('x'); }",
            "does not implement inherited abstract member kind from class A (TS2515)",
        ),
        (
            "function f(x: string): string; function f(x: number): number; function f(x: string | number): string | number { return x; } export function main(): void { console.log(String(f(true))); }",
            "no overload matches this call with arguments (boolean) (TS2769)",
        ),
        (
            "function f(x: string): string; function f(x: number): number; function f(x: string | number): string | number { return x; } export function main(): void { const g = f; console.log('x'); }",
            "overloaded function used as a value",
        ),
        (
            "function f(x: string): string; export function main(): void { console.log('x'); }",
            "function implementation of f is missing",
        ),
        (
            "function f(x: string): string; function f(x: boolean): number { return 1; } export function main(): void { console.log('x'); }",
            "not compatible with its implementation signature (TS2394)",
        ),
        (
            "const enum E { A, B } export function main(): void { const i = 1; console.log(E[i]); }",
            "a const enum member can only be accessed using a string literal (TS2476)",
        ),
        (
            "const enum E { A, B } function take(x: unknown): void {} export function main(): void { take(E); }",
            "const enum object (TS2475) E used as a value",
        ),
        (
            "namespace N { export let a = 1; } function take(x: unknown): void {} export function main(): void { take(N); }",
            "namespace object (a class or let member has no value in it) N used as a value",
        ),
        (
            "namespace N { const hidden = 1; export const a = hidden; } export function main(): void { console.log(String(N.hidden)); }",
            "namespace N has no exported member hidden",
        ),
        (
            "namespace N { console.log('effect'); } export function main(): void { console.log('x'); }",
            "statement in a namespace body",
        ),
        (
            "enum E { A } declare namespace N { const x: number; } export function main(): void { console.log('x'); }",
            "ambient namespace or module declaration",
        ),
        (
            "function f(): number { return 1; } enum E { A = f() } export function main(): void { console.log('x'); }",
            "computed enum member (an initializer that is not a constant expression)",
        ),
        (
            "enum A { X = 'x', Y } export function main(): void { console.log('x'); }",
            "enum member Y must have an initializer (TS1061)",
        ),
        (
            "class B { static make(): number { return this.n; } static n = 1; } class C extends B { static { super.make(); } } export function main(): void { console.log('x'); }",
            "super call of a static method that uses this, from a static block",
        ),
        (
            "function dec(value: unknown, context: unknown): void {} @dec class C {} export function main(): void { console.log('x'); }",
            "decorator (standard decorators are not implemented yet",
        ),
        (
            "function dec(value: unknown, context: unknown): void {} class C { @dec m(): void {} } export function main(): void { console.log('x'); }",
            "decorator (standard decorators are not implemented yet",
        ),
        (
            "class C { private static secret = 1; } export function main(): void { console.log(String(C.secret)); }",
            "static field secret is private and only accessible within class C (TS2341)",
        ),
        (
            "class C { static readonly limit: number = 1; } export function main(): void { C.limit = 2; console.log(String(C.limit)); }",
            "cannot assign to limit because it is a read-only static field (TS2540)",
        ),
        (
            "class A { static readonly n: number = 1; } class B extends A {} export function main(): void { B.n = 2; console.log(String(A.n)); }",
            "cannot assign to n because it is a read-only static field (TS2540)",
        ),
        (
            "class A { private static n: number = 1; } class B extends A {} export function main(): void { console.log(String(B.n)); }",
            "static field n is private and only accessible within class A (TS2341)",
        ),
        (
            "class A { static n: number = 1; } class B extends A {} export function main(): void { B.n = 'x'; console.log(String(A.n)); }",
            "TS2322",
        ),
        (
            "export function main(): void { const k = 2; const C = class { v = k; }; console.log(String(new C().v)); }",
            "class expression that captures the local binding k",
        ),
        (
            "function make(c: unknown): void {} export function main(): void { make(class { v = 1; }); }",
            "class expression outside a const declaration of its own",
        ),
        (
            "export function main(): void { const k = 2; class Local { v = k; } console.log(String(new Local().v)); }",
            "local class that captures the local binding k",
        ),
        (
            "class C { private v = 0; set x(n: number) { this.v = n; } } export function main(): void { const c = new C(); c.x += 1; }",
            "compound assignment or update of the setter-only accessor x",
        ),
        (
            "class C { get x(): number { return 1; } } export function main(): void { const c = new C(); c.x = 2; }",
            "class getter x has no setter",
        ),
        (
            "class A { get x(): number { return 1; } set x(v: number) {} } class B extends A { get x(): number { return 2; } } export function main(): void { console.log('x'); }",
            "accessor x that overrides only one half of an inherited get/set pair",
        ),
        (
            "declare function f(): void; export function main(): void { f(); }",
            "ambient function declaration",
        ),
        (
            "export function main(): void { const __vartaDecl$x = 1; console.log(String(__vartaDecl$x)); }",
            "identifiers starting with __vartaDecl are reserved",
        ),
    ] {
        refused(V38, source, diagnostic);
    }
}

/// V37 keeps rejecting every new form with its old diagnostic.
#[test]
fn v37_keeps_rejecting_declaration_forms() {
    for (source, diagnostic) in [
        (
            "enum E { A } export function main(): void { console.log(String(E.A)); }",
            "top-level declaration",
        ),
        (
            "namespace N { export const a = 1; } export function main(): void { console.log(String(N.a)); }",
            "top-level declaration",
        ),
        (
            "abstract class A { f(): number { return 1; } } export function main(): void { console.log('x'); }",
            "abstract class",
        ),
        (
            "function f(x: string): string; function f(x: string): string { return x; } export function main(): void { console.log(f('a')); }",
            "function body is required",
        ),
        (
            "class C { private v = 0; set x(n: number) { this.v = n; } } export function main(): void { console.log('x'); }",
            "class setter",
        ),
        (
            "class C { static n = 1; } export function main(): void { console.log(String(C.n)); }",
            "static class field",
        ),
        (
            "export function main(): void { const C = class { v = 1; }; console.log(String(new C().v)); }",
            "class expression",
        ),
        (
            "class C { accessor n = 1; } export function main(): void { console.log('x'); }",
            "class auto-accessor",
        ),
        (
            "function dec(value: unknown, context: unknown): void {} @dec class C {} export function main(): void { console.log('x'); }",
            "Expression expected",
        ),
    ] {
        refused(V37, source, diagnostic);
    }
}
