//! V36 `async_generators` at the compiler level: async generator
//! declarations, generic instantiations and methods and `for await` validate
//! in Wasmtime; bodies are catalogued as generator frames and their awaits as
//! scheduled relay frames; v35 keeps every v35 diagnostic; positions outside
//! the admitted subset fail by construct. Execution against pinned Node logs
//! lives in `varta-runtime` `async_generator_tests`.
use varta_common::function::frame::FrameInput;
use varta_common::language::LanguageProfile;
use wasmtime::{Engine, Module};

const V36: LanguageProfile = LanguageProfile::CheckedF64Utf16V36;
const V35: LanguageProfile = LanguageProfile::CheckedF64Utf16V35;

fn program(body: &str) -> String {
    format!("{body}\nexport function main(): void {{}}")
}
fn compile(source: &str, profile: LanguageProfile) -> Result<Vec<u8>, crate::CompilerError> {
    crate::compile_typescript_with_profile(source, profile)
}
fn message(body: &str, profile: LanguageProfile) -> String {
    match compile(&program(body), profile) {
        Ok(_) => format!("unexpectedly compiled: {body}"),
        Err(error) => error.to_string(),
    }
}

const GENERATORS: &str = "async function* pages<T>(all: T[][]): AsyncIterableIterator<T> {
  for (const page of all) {
    if (page === undefined) continue;
    await null;
    for (const item of page) if (item !== undefined) yield item;
  }
}
class Feed {
  name: string;
  constructor(name: string) { this.name = name; }
  async *events(count?: number): AsyncGenerator<string, number, undefined> {
    let sent = 0;
    try {
      for (let i = 0; i < (count ?? 2); i++) { sent++; yield this.name + String(i); }
    } finally {
      await null;
    }
    return sent;
  }
  static async *ticks(n: number = 2): AsyncGenerator<number, void, undefined> { for (let i = 0; i < n; i++) yield i; }
}
async function use(): Promise<number> {
  let total = 0;
  for await (const n of pages<number>([[1, 2], [3]])) total += n;
  for await (const n of Feed.ticks()) total += n;
  for await (const n of [Promise.resolve(1), 2]) total += n;
  const it = new Feed('f').events();
  const first = await it.next();
  await it.return(7);
  await it.throw(new Error('late')).catch(() => {});
  return total + (first.done ? 0 : 1);
}";

#[test]
fn async_generators_validate_and_catalogue_generator_and_relay_frames() {
    let bytes = compile(&program(GENERATORS), V36).unwrap_or_else(|e| panic!("{e}"));
    Module::new(&Engine::default(), &bytes).unwrap();
    let catalog = crate::suspension_tests::catalog(&bytes);
    assert_eq!(
        catalog.version,
        varta_common::function::frame::FRAME_LIVENESS_CATALOG_VERSION
    );
    // Bodies are generator rows resumed by guest code only: `Guest` inputs.
    let bodies: Vec<_> = catalog.frames.iter().filter(|l| l.generator).collect();
    assert!(bodies.len() >= 3, "{:?}", catalog.frames);
    for layout in &bodies {
        assert!(layout.states.iter().all(|s| s.input == FrameInput::Guest));
        assert_eq!(layout.return_input, Some(FrameInput::Guest));
    }
    // Relay, return and async-function frames are scheduled rows.
    let scheduled: Vec<_> = catalog.frames.iter().filter(|l| !l.generator).collect();
    assert!(scheduled.len() >= 3, "{:?}", catalog.frames);
    for layout in &scheduled {
        assert!(
            layout
                .states
                .iter()
                .all(|s| s.input == FrameInput::Undefined)
        );
    }
}

#[test]
fn v35_keeps_rejecting_async_generators_and_for_await() {
    for (body, expected) in [
        (
            "async function* g(): AsyncGenerator<number> { yield 1; }",
            "unsupported async generator function",
        ),
        (
            "class A { async *m(): AsyncGenerator<number> { yield 1; } }",
            "unsupported async generator function",
        ),
        (
            "async function f(xs: number[]): Promise<void> { for await (const x of xs) {} }",
            "unsupported for await...of (async iteration is not implemented yet)",
        ),
        (
            "function f(g: AsyncIterable<number>): void {}",
            "unsupported type reference AsyncIterable",
        ),
        (
            "async function f(): Promise<number> { return (await Promise.resolve(1 as unknown)) as number; }",
            "unsupported await inside intrinsic expression",
        ),
        (
            "async function f(limit = Number.POSITIVE_INFINITY): Promise<number> { return limit; }",
            "parameter needs an annotation",
        ),
    ] {
        let error = message(body, V35);
        assert!(error.contains(expected), "{body}\n=> {error}");
    }
}

#[test]
fn positions_outside_the_async_generator_subset_fail_with_diagnostics() {
    const G: &str = "async function* g(): AsyncGenerator<number, void, undefined> { yield 1; }\n";
    for (body, expected) in [
        (
            "async function* h(): AsyncGenerator<number> { yield* g(); }",
            "unsupported yield* delegation in an async generator",
        ),
        (
            "function f(): void { const h = async function* (): AsyncGenerator<number> { yield 1; }; }",
            "unsupported async generator function expression",
        ),
        (
            "async function* h() { yield 1; }",
            "async generator function requires an AsyncGenerator<T, TReturn, TNext> return annotation",
        ),
        (
            "async function* h(): Promise<number> { yield 1; }",
            "async generator function must return AsyncGenerator<T, TReturn, TNext>, not promise",
        ),
        (
            "function* h(): AsyncGenerator<number> { yield 1; }",
            "generator function must return Generator<T, TReturn, TNext>, not an async generator",
        ),
        (
            "function f(): void { for (const x of g()) {} }",
            "for-of over an async generator requires for await...of",
        ),
        (
            "async function* h(): AsyncGenerator<number, void, string> { const s = yield 1; }\nasync function f(): Promise<void> { for await (const x of h()) {} }",
            "for await...of over an async generator requires TNext to accept undefined",
        ),
        (
            "async function* h(): AsyncGenerator<number, string, undefined> { yield 1; return \"x\"; }\nfunction f(): void { h().return(); }",
            "return() without an argument requires a generator type whose TReturn accepts undefined",
        ),
        (
            "async function* h(): AsyncGenerator<string> { yield String(await Promise.resolve(1)); }",
            "unsupported suspension point inside call to String",
        ),
        (
            "async function* h(): AsyncGenerator<Promise<number>> { yield Promise.resolve(1); }",
            "unsupported promise in an async generator type",
        ),
        (
            "function f(): void { const r = { it: g() }; }",
            "unsupported generator inside an object",
        ),
        (
            "async function* h(): AsyncGenerator<number> { yield \"a\"; }",
            "yield type mismatch: expected number, found string",
        ),
        (
            "async function f(xs: number): Promise<void> { for await (const x of xs) {} }",
            "for await...of requires an async generator, a generator or a checked array",
        ),
        (
            "class C { async *[Symbol.asyncIterator](): AsyncIterator<number> { yield 1; } }",
            "unsupported computed or numeric class member name",
        ),
    ] {
        let error = message(&format!("{G}{body}"), V36);
        assert!(error.contains(expected), "{body}\n=> {error}");
    }
}
