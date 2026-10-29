//! K4.4 step 2: synchronous generators in profile v33 at the compiler level.
//! Emitted modules validate in Wasmtime, generator frames are catalogued as
//! guest-resumed rows the host dispatcher can never reach, and every position
//! outside the admitted subset fails with a construct-specific diagnostic.
//! Execution against pinned Node logs lives in `varta-runtime` `generator_tests`.
use varta_common::function::frame::{self, Completion, FrameInput, ResumeToken};
use varta_common::language::LanguageProfile;
use wasmtime::{Engine, Linker, Module, Store};

const V33: LanguageProfile = LanguageProfile::CheckedF64Utf16V33;
const V32: LanguageProfile = LanguageProfile::CheckedF64Utf16V32;
const COUNT: &str = "function* count(n: number): Generator<number> {
  try { for (let i = 0; i < n; i++) { yield i; } } finally { n = 0; }
}";

fn program(body: &str) -> String {
    format!("{body}\nexport function main(): void {{}}")
}
fn compile(source: &str, profile: LanguageProfile) -> Result<Vec<u8>, crate::CompilerError> {
    crate::compile_typescript_with_profile(source, profile)
}
fn message(body: &str) -> String {
    match compile(&program(body), V33) {
        Ok(_) => format!("unexpectedly compiled: {body}"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn generator_modules_validate_and_catalogue_guest_frames() {
    let bytes = compile(&program(COUNT), V33).unwrap();
    Module::new(&Engine::default(), &bytes).unwrap();
    let catalog = crate::suspension_tests::catalog(&bytes);
    let [layout] = catalog.frames.as_slice() else {
        panic!("one generator layout expected");
    };
    assert!(layout.generator);
    assert_eq!(layout.return_input, Some(FrameInput::Guest));
    assert!(layout.states.iter().all(|s| s.input == FrameInput::Guest));
    // The same module passes the shared runtime/artifact admission rules.
    let mut sections = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        if let wasmparser::Payload::ExportSection(exports) = payload.unwrap() {
            for export in exports {
                sections.push(export.unwrap().name.to_owned());
            }
        }
    }
    assert!(sections.iter().any(|name| name == frame::RESUME_EXPORT));
}

#[test]
fn generator_frames_never_reach_the_host_dispatcher() {
    let bytes = compile(&program(COUNT), V33).unwrap();
    let engine = Engine::default();
    let module = Module::new(&engine, &bytes).unwrap();
    let mut linker = Linker::new(&engine);
    linker.define_unknown_imports_as_traps(&module).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = linker.instantiate(&mut store, &module).unwrap();
    let create = instance
        .get_typed_func::<f64, i64>(&mut store, &format!("{}0", crate::emit::TEST_CREATE_PREFIX))
        .unwrap();
    let handle = create.call(&mut store, 3.0).unwrap();
    let view = varta_common::function::frame::FrameView::read_envelope(
        instance
            .get_memory(&mut store, "memory")
            .unwrap()
            .data(&store),
        handle as u32,
    )
    .unwrap();
    assert_eq!(view.status, frame::STATUS_SUSPENDED);
    let resume = instance
        .get_typed_func::<(i64, i32, i64), i32>(&mut store, frame::RESUME_EXPORT)
        .unwrap();
    // The creator released the live-table slot: ownership fails for every
    // completion kind before any guest effect.
    let token = view.token(handle as u32).to_bits();
    assert_eq!(ResumeToken::from_bits(token).frame, handle as u32);
    for kind in [Completion::Normal, Completion::Throw, Completion::Return] {
        assert!(resume.call(&mut store, (token, kind as i32, 0)).is_err());
    }
}

#[test]
fn earlier_profiles_keep_rejecting_generator_syntax() {
    let source = program("function* g(): Generator<number> { yield 1; }");
    let error = compile(&source, V32).unwrap_err().to_string();
    assert!(
        error.contains("unsupported async, generator, generic or decorated function"),
        "{error}"
    );
}

#[test]
fn positions_outside_the_generator_subset_fail_with_diagnostics() {
    let mut failures = Vec::new();
    for (body, expected) in [
        (
            "function* a(): Generator<number> { yield 1; }\nfunction* g(): Generator<number> { yield* a(); }",
            "unsupported yield* delegation",
        ),
        (
            "async function* g(): AsyncGenerator<number> { yield 1; }",
            "unsupported async generator function",
        ),
        (
            "function* g() { yield 1; }",
            "requires a Generator<T, TReturn, TNext> return annotation",
        ),
        (
            "function* g(): number { return 1; }",
            "generator function must return Generator<T, TReturn, TNext>",
        ),
        (
            "function* g(): Generator<any> { yield 1; }",
            "unsupported type annotation",
        ),
        (
            "function* g(): Generator<number> { yield \"text\"; }",
            "expected number, found string",
        ),
        (
            "function* g(): Generator<number> { yield; }",
            "expected number, found undefined",
        ),
        (
            "function* g(x?: number): Generator<number> { yield 1; }",
            "unsupported generator with optional or rest parameters",
        ),
        (
            "class A { *g(): Generator<number> { yield 1; } }",
            "unsupported generator method",
        ),
        (
            "function f(): void { const h = function* (): Generator<number> { yield 1; }; }",
            "unsupported async/generator/generic/decorated function expression",
        ),
        (
            "function* g(): Generator<number> { yield 1; }\nfunction f(): void { const h = g; }",
            "unsupported generator function as a value",
        ),
        (
            "function* g(): Generator<string> { yield String(yield \"a\"); }",
            "unsupported suspension point inside call to String",
        ),
        (
            "function* g(): Generator<number, void, boolean> { do { } while (yield 1); }",
            "unsupported suspension point inside do-while condition",
        ),
        (
            "function* g(): Generator<number, void, number> { yield 1; }\nfunction f(): void { g().next(); }",
            "next() without an argument requires a generator type whose TNext accepts undefined",
        ),
        (
            "function* g(): Generator<number, number> { yield 1; return 2; }\nfunction f(): void { g().return(); }",
            "return() without an argument requires a generator type whose TReturn accepts undefined",
        ),
        (
            "function* g(): Generator<number, number> { yield 1; return 2; }\nfunction f(): void { for (const x of g()) { } }",
            "for-of over a generator requires TNext and TReturn to accept undefined",
        ),
        (
            "function f(): void { const n = 1; n.next(); }",
            "next requires a generator receiver",
        ),
        (
            "function* g(): Generator<number> { yield 1; }\nfunction f(): void { const a: Generator<number>[] = []; }",
            "unsupported generator inside an array",
        ),
        (
            "function* g(): Generator<number> { yield 1; }\nfunction f(): void { let u: Generator<number> | undefined; }",
            "unsupported generator inside a union",
        ),
        (
            "function* g(): Generator<number> { yield 1; }\nfunction f(): void { const o = { g: g() }; }",
            "unsupported generator inside an object",
        ),
        (
            "function* g(): Generator<number> { yield 1; }\nfunction f(): void { console.log(g()); }",
            "cannot log generator",
        ),
        (
            "function* g(): Generator<number> { yield 1; }\nfunction f(): void { console.log(JSON.stringify(g())); }",
            "unsupported JSON.stringify value type",
        ),
        (
            "function* g(): Generator<number> { yield 1; }\nfunction f(): boolean { const a = g(); return a === a; }",
            "unsupported binary operator",
        ),
        (
            "function* g(): Generator<number> { yield 1; }\nfunction f(): string { return typeof g(); }",
            "unsupported typeof an opaque result value",
        ),
        (
            "function* g(): Generator<Generator<number>> { }",
            "unsupported generator yielding, returning or receiving a generator",
        ),
        (
            "function* g(): Generator<{ a: number }, { b: string }> { yield { a: 1 }; return { b: \"\" }; }",
            "IteratorResult value union",
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
fn generators_are_rejected_at_tool_and_http_boundaries() {
    let tool = "export function* g(): Generator<number> { yield 1; }\nexport function main(): void {}\nvarta.tool({ name: \"g\", description: \"G\" }, g);";
    let error = compile(tool, LanguageProfile::default())
        .unwrap_err()
        .to_string();
    assert!(error.contains("generator"), "{error}");
    let error = compile(tool, V33).unwrap_err().to_string();
    assert!(
        error.contains("tool/HTTP contracts in binary64 profile"),
        "{error}"
    );
}
