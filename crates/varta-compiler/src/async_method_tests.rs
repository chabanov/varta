//! V35 `async_methods` at the compiler level: async instance and static
//! methods, async arrows and async function expressions validate in Wasmtime,
//! their scheduled bodies are catalogued as frames whose captured cells are
//! shared slots, v34 keeps every v34 diagnostic, and positions outside the
//! admitted subset fail by construct. Execution against pinned Node logs lives
//! in `varta-runtime` `async_method_tests`.
use varta_common::function::frame::FrameInput;
use varta_common::language::LanguageProfile;
use wasmtime::{Engine, Module};

const V35: LanguageProfile = LanguageProfile::CheckedF64Utf16V35;
const V34: LanguageProfile = LanguageProfile::CheckedF64Utf16V34;

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

const METHODS: &str = "class Base {
  n: number;
  constructor(n: number) { this.n = n; }
  async get(scale?: number): Promise<number> { await null; return this.n * (scale ?? 1); }
  static async make(n: number): Promise<Base> { await null; return new Base(n); }
}
class Child extends Base {
  async get(scale?: number): Promise<number> { const base = await super.get(scale); return base + 1; }
}
async function use(b: Base): Promise<number> { const made = await Base.make(2); return (await b.get(3)) + made.n; }";

const CLOSURES: &str = "async function retry<T>(fn: () => Promise<T>): Promise<T> { return await fn(); }
function outer(label: string): Promise<string> {
  let count = 0;
  const step = async (by: number): Promise<number> => { count = count + by; await null; return count; };
  const named = async function again(n: number): Promise<number> { await null; return n === 0 ? 0 : await again(n - 1); };
  return retry(async () => { const c = await step(1); const d = await named(2); return label + String(c + d); });
}";

#[test]
fn async_methods_and_closures_validate_and_catalogue_scheduled_frames() {
    for body in [METHODS, CLOSURES] {
        let bytes = compile(&program(body), V35).unwrap_or_else(|e| panic!("{e}\n{body}"));
        Module::new(&Engine::default(), &bytes).unwrap();
        let catalog = crate::suspension_tests::catalog(&bytes);
        assert!(catalog.frames.len() >= 3, "{body}");
        for layout in &catalog.frames {
            assert!(!layout.generator);
            assert!(
                layout
                    .states
                    .iter()
                    .all(|s| s.input == FrameInput::Undefined)
            );
            assert_eq!(layout.result, None);
        }
    }
    // An async closure body keeps the closure's captured cells: the captured
    // `count` binding is a shared frame slot of the arrow's body.
    let bytes = compile(&program(CLOSURES), V35).unwrap();
    let catalog = crate::suspension_tests::catalog(&bytes);
    assert!(
        catalog
            .frames
            .iter()
            .any(|layout| layout.slots.iter().any(|slot| slot.shared))
    );
}

#[test]
fn v34_keeps_rejecting_async_methods_closures_and_optional_parameters() {
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
            "async function f(x?: number): Promise<void> { }",
            "unsupported async function with optional or rest parameters",
        ),
        (
            "export function main2(): void { Promise.resolve(1).then(async (v: number): Promise<number> => v); }",
            "unsupported async arrow function",
        ),
    ] {
        let error = message(body, V34);
        assert!(error.contains(expected), "{body}\n=> {error}");
        // V35 admits each of them.
        compile(&program(body), V35).unwrap_or_else(|e| panic!("{e}\n{body}"));
    }
}

#[test]
fn positions_outside_the_async_method_subset_fail_with_diagnostics() {
    for (body, expected) in [
        (
            "function f(): void { const g = async (): number => 1; }",
            "async function must return Promise<T>, not number",
        ),
        (
            "function f(): void { const g = async (): Promise<number> => \"x\"; }",
            "async arrow result mismatch",
        ),
        (
            "class A { async *m(): AsyncGenerator<number> { yield 1; } }",
            "unsupported",
        ),
        (
            "class A<T> { async m(): Promise<number> { return 1; } }",
            "unsupported generic class",
        ),
        (
            "class A { async m<T>(x: T): Promise<T> { return x; } }",
            "unsupported async, generator, generic or decorated function",
        ),
        (
            "function f(): void { const g = async (): Promise<Promise<number>> => 1; }",
            "Promise<Promise<T>>",
        ),
        (
            "function f(): void { const g = async (): Promise<string> => { return `${await Promise.resolve(1)}`; }; }",
            "unsupported await inside",
        ),
    ] {
        let error = message(body, V35);
        assert!(error.contains(expected), "{body}\n=> {error}");
    }
}
