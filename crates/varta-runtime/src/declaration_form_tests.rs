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
    // Imported module variables are read-only, as for every module `let`.
    let write = "import { Counter } from './lib.ts';
         export function main(): void { Counter.count = 5; }";
    assert!(compile_modules(write, &[("lib.ts", library)]).is_err());
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
            "enum E { A, B } function take(x: unknown): void {} export function main(): void { take(E); }",
            "enum object E used as a value",
        ),
        (
            "namespace N { export const a = 1; } function take(x: unknown): void {} export function main(): void { take(N); }",
            "namespace object N used as a value",
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
            "class C { static n = 1; static { C.n = 2; } } export function main(): void { console.log(String(C.n)); }",
            "static class block",
        ),
        (
            "function dec(value: unknown, context: unknown): void {} @dec class C {} export function main(): void { console.log('x'); }",
            "decorator (a decorator runs code when its class is defined",
        ),
        (
            "function dec(value: unknown, context: unknown): void {} class C { @dec m(): void {} } export function main(): void { console.log('x'); }",
            "decorator (a decorator runs code when its class is defined",
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
            "class A { static n = 1; } class B extends A {} export function main(): void { B.n = 2; console.log(String(A.n)); }",
            "assignment to the inherited static field n through a subclass",
        ),
        (
            "class C { static make(): C { return new C(); } static x = C.make(); } export function main(): void { console.log('x'); }",
            "call in a module constant initializer",
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
        (
            "class C { [key: string]: number; } export function main(): void { console.log('x'); }",
            "class index signature",
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
