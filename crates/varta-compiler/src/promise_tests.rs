//! K4.4 steps 3 and 4: promises and async functions in profile v34 at the
//! compiler level. Emitted modules validate in Wasmtime, async bodies and
//! promise jobs are catalogued as scheduled frames with `undefined` inputs,
//! scheduler imports appear only when used, and every position outside the
//! admitted subset fails with a construct-specific diagnostic. Execution
//! against pinned Node logs lives in `varta-runtime` `promise_tests` and
//! `async_tests`.
use varta_common::HostOperation;
use varta_common::function::frame::{self, FrameInput};
use varta_common::language::LanguageProfile;
use wasmtime::{Engine, Module};

const V34: LanguageProfile = LanguageProfile::CheckedF64Utf16V34;
const V33: LanguageProfile = LanguageProfile::CheckedF64Utf16V33;
const ASYNC: &str = "async function twice(n: number): Promise<number> {
  const first = await Promise.resolve(n);
  try { return first + (await n); } finally { n = 0; }
}";

fn program(body: &str) -> String {
    format!("{body}\nexport function main(): void {{}}")
}
fn compile(source: &str, profile: LanguageProfile) -> Result<Vec<u8>, crate::CompilerError> {
    crate::compile_typescript_with_profile(source, profile)
}
fn message(body: &str) -> String {
    match compile(&program(body), V34) {
        Ok(_) => format!("unexpectedly compiled: {body}"),
        Err(error) => error.to_string(),
    }
}
fn imports(bytes: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let wasmparser::Payload::ImportSection(imports) = payload.unwrap() {
            for import in imports {
                names.push(import.unwrap().name.to_owned());
            }
        }
    }
    names
}
fn scheduler_imports() -> [&'static str; 4] {
    [
        HostOperation::PromiseReact.name(),
        HostOperation::PromiseSettle.name(),
        HostOperation::SchedulerEnqueue.name(),
        HostOperation::PromiseHandled.name(),
    ]
}

#[test]
fn async_modules_validate_and_catalogue_scheduled_frames() {
    let bytes = compile(&program(ASYNC), V34).unwrap();
    Module::new(&Engine::default(), &bytes).unwrap();
    let catalog = crate::suspension_tests::catalog(&bytes);
    assert!(!catalog.frames.is_empty());
    for layout in &catalog.frames {
        assert!(!layout.generator);
        // Every scheduled suspension resumes with `undefined`; the body reads
        // the settled promise itself, so no host value crosses the boundary.
        assert!(
            layout
                .states
                .iter()
                .all(|s| s.input == FrameInput::Undefined)
        );
        assert_eq!(layout.result, None);
    }
    let names = imports(&bytes);
    for name in scheduler_imports() {
        assert!(names.iter().any(|n| n == name), "{name} missing: {names:?}");
    }
    assert!(
        names
            .iter()
            .any(|n| n == HostOperation::PromiseSettle.name())
    );
}

#[test]
fn promise_chains_validate_without_async_functions() {
    let source = program(
        "function chain(): Promise<string> {
  return new Promise<number>((resolve) => resolve(1))
    .then((v) => v + 1)
    .catch(() => 0)
    .finally(() => {})
    .then((v) => String(v));
}",
    );
    let bytes = compile(&source, V34).unwrap();
    Module::new(&Engine::default(), &bytes).unwrap();
    assert!(
        imports(&bytes)
            .iter()
            .any(|n| n == HostOperation::PromiseUnhandled.name())
    );
    let catalog = crate::suspension_tests::catalog(&bytes);
    assert!(
        catalog.frames.len() >= 4,
        "then, catch, finally and adoption jobs"
    );
}

#[test]
fn modules_without_promises_keep_their_imports_and_exports() {
    let bytes = compile(&program("function f(): number { return 1; }"), V34).unwrap();
    let names = imports(&bytes);
    for name in scheduler_imports() {
        assert!(
            !names.iter().any(|n| n == name),
            "{name} unexpectedly imported"
        );
    }
    let mut exports = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        if let wasmparser::Payload::ExportSection(section) = payload.unwrap() {
            for export in section {
                exports.push(export.unwrap().name.to_owned());
            }
        }
    }
    assert!(!exports.iter().any(|name| name == frame::RESUME_EXPORT));
}

#[test]
fn earlier_profiles_keep_rejecting_async_syntax_and_promise_types() {
    for (body, expected) in [
        (
            "async function f(): Promise<number> { return 1; }",
            "unsupported async, generator, generic or decorated function",
        ),
        (
            "function f(p: Promise<number>): void {}",
            "unsupported Promise<T> type",
        ),
    ] {
        let error = compile(&program(body), V33).unwrap_err().to_string();
        assert!(error.contains(expected), "{body}\n=> {error}");
    }
}

#[test]
fn positions_outside_the_async_subset_fail_with_diagnostics() {
    let mut failures = Vec::new();
    for (body, expected) in [
        (
            "function f(): void { const g = async (): Promise<number> => 1; }",
            "unsupported async arrow function",
        ),
        (
            "function f(): void { const g = async function (): Promise<number> { return 1; }; }",
            "unsupported async function expression",
        ),
        (
            "class A { async m(): Promise<number> { return 1; } }",
            "unsupported async class method",
        ),
        (
            "async function* g(): AsyncGenerator<number> { yield 1; }",
            "unsupported async generator function",
        ),
        (
            "async function f(xs: number[]): Promise<void> { for await (const x of xs) { } }",
            "unsupported for await...of",
        ),
        (
            "const top = await Promise.resolve(1);",
            "unsupported await outside an async function (top-level await is not supported)",
        ),
        (
            "async function f() { return 1; }",
            "async function requires a Promise<T> return annotation",
        ),
        (
            "async function f(): number { return 1; }",
            "async function must return Promise<T>, not number",
        ),
        (
            "async function f(x?: number): Promise<void> { }",
            "unsupported async function with optional or rest parameters",
        ),
        (
            "async function f(): Promise<number> { return \"x\"; }",
            "return type mismatch",
        ),
        (
            "async function f(p: Promise<string>): Promise<number> { const s = \"n\" + (await p); return 1; }",
            "unsupported await inside `+` expression",
        ),
        (
            "function f(): void { const a: Promise<number>[] = []; }",
            "unsupported promise inside an array",
        ),
        (
            "function f(): void { let u: Promise<number> | undefined; }",
            "unsupported promise inside a union",
        ),
        (
            "function f(): void { const o = { p: Promise.resolve(1) }; }",
            "unsupported promise inside an object",
        ),
        (
            "function f(): Promise<Promise<number>> { return Promise.resolve(1); }",
            "Promise<Promise<T>>",
        ),
        (
            "function f(): Promise<any> { return Promise.resolve(1); }",
            "unsupported type annotation",
        ),
        (
            "function f(): void { Promise.all([]); }",
            "unsupported Promise.all",
        ),
        (
            "function f(): void { Promise.race([]); }",
            "unsupported Promise.race",
        ),
        (
            "function f(): void { const p = new Promise((resolve) => resolve(1)); }",
            "new Promise requires a type argument or a Promise<T> context",
        ),
        (
            "function f(): void { new Promise<number>((resolve) => resolve(Promise.resolve(1))); }",
            "mismatch",
        ),
        (
            "function f(): void { const n = 1; n.then(() => 0); }",
            "then requires a Promise receiver",
        ),
        (
            "function f(p: Promise<number>): void { p.then(() => Promise.resolve(1), () => Promise.resolve(\"x\")); }",
            "promise handlers that return promises must agree on the result type",
        ),
        (
            "function f(p: Promise<number>): void { console.log(p); }",
            "promise",
        ),
        (
            "function f(p: Promise<number>): string { return JSON.stringify(p); }",
            "unsupported JSON.stringify value type",
        ),
        (
            "function f(p: Promise<number>): string { return typeof p; }",
            "unsupported typeof an opaque result value",
        ),
    ] {
        let error = message(body);
        if !error.contains(expected) {
            failures.push(format!("{body}\n=> {error}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn async_main_keeps_the_entry_signature() {
    let source = "export async function main(): Promise<void> { await null; }";
    let bytes = compile(source, V34).unwrap();
    let module = Module::new(&Engine::default(), &bytes).unwrap();
    let start = module.get_export("_start").unwrap();
    let wasmtime::ExternType::Func(ty) = start else {
        panic!("_start is a function");
    };
    assert_eq!((ty.params().len(), ty.results().len()), (0, 0));
    let error = compile(
        "export async function main(): Promise<number> { return 1; }",
        V34,
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("main must have signature () -> void"),
        "{error}"
    );
}
