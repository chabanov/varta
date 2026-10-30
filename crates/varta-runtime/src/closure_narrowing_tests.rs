//! V38 `closure_narrowing`: TypeScript 5.4's narrowing in closures created
//! after the last assignment. Expected logs of the `closure_narrowing_cases`
//! programs were produced by Node 24 (type stripping, `main()` appended) and
//! are pinned here; the rejected programs are the ones TypeScript 5.4 also
//! rejects ("narrow it first" there is TypeScript's "property does not exist
//! on type").
use super::*;
use varta_common::language::LanguageProfile as P;

const V37: P = P::CheckedF64Utf16V37;
const V38: P = P::CheckedF64Utf16V38;

fn compile(source: &str, profile: P) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
fn rejected(source: &str, profile: P) -> String {
    varta_compiler::compile_typescript_with_profile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("{profile:?} accepted\n{source}"))
        .to_string()
}

#[test]
fn stable_parameters_and_lets_keep_their_narrowing_in_closures() {
    let source = include_str!("closure_narrowing_cases/stable.ts");
    let lines = VartaRuntime::new()
        .run_wasm_bytes(&compile(source, V38), &[])
        .unwrap_or_else(|e| panic!("{e}"))
        .lines;
    let expected: Vec<String> = [
        "<code ts>x</code>",
        "<a /a>A</a>",
        "plain",
        "ABC",
        "n4.0",
        "outer",
        "5:Docs",
        "rs1,rs2;go1,go2;",
    ]
    .iter()
    .map(|line| format!("varta log: {line}"))
    .collect();
    assert_eq!(lines, expected);
    // V37 keeps narrowing only `const` bindings into closures.
    assert!(rejected(source, V37).contains("narrow it first"));
}

const UNION: &str = r#"
type Inline = { kind: "text"; text: string } | { kind: "link"; href: string };
"#;

#[test]
fn any_later_assignment_withholds_the_narrowing() {
    for body in [
        // A parameter reassigned after the closure is created.
        r#"function f(n: Inline): () => string {
  if (n.kind === "link") {
    const get = () => n.href;
    n = { kind: "text", text: "t" };
    return get;
  }
  return () => "";
}"#,
        // An assignment inside another closure, even one never called.
        r#"function f(n: Inline): () => string {
  const reset = () => { n = { kind: "text", text: "t" }; };
  if (n.kind === "link") {
    return () => n.href;
  }
  return () => "";
}"#,
        // A `let` reassigned in a loop after the closure in source order.
        r#"function f(items: Inline[]): (() => string)[] {
  const out: (() => string)[] = [];
  let n: Inline = items[0];
  for (const next of items) {
    if (n.kind === "link") {
      out.push(() => n.href);
    }
    n = next;
  }
  return out;
}"#,
        // An assignment before the closure, but in the same loop.
        r#"function f(items: Inline[]): (() => string)[] {
  const out: (() => string)[] = [];
  let n: Inline = items[0];
  for (let i = 0; i < items.length; i++) {
    n = items[i];
    if (n.kind === "link") {
      out.push(() => n.href);
    }
  }
  return out;
}"#,
        // A hoisted function declaration never keeps it.
        r#"function f(n: Inline): string {
  if (n.kind === "link") {
    function get(): string { return n.href; }
    return get();
  }
  return "";
}"#,
    ] {
        let source = format!("{UNION}{body}\nexport function main(): void {{}}");
        let error = rejected(&source, V38);
        assert!(error.contains("narrow it first"), "{error}\n{source}");
    }
}

#[test]
fn stable_narrowing_survives_calls_inside_the_closure() {
    // Inside the closure no call can write a stable binding again, so an
    // unknown call before the read keeps the narrowing (v37 would drop a
    // mutable cell's fact at the call).
    let source = format!(
        r#"{UNION}
function note(text: string): void {{ console.log(text); }}
function f(n: Inline): () => string {{
  if (n.kind !== "link") {{
    return () => "";
  }}
  return () => {{
    note("before");
    return n.href;
  }};
}}
export function main(): void {{ console.log(f({{ kind: "link", href: "/x" }})()); }}"#
    );
    let lines = VartaRuntime::new()
        .run_wasm_bytes(&compile(&source, V38), &[])
        .unwrap_or_else(|e| panic!("{e}"))
        .lines;
    assert_eq!(lines, ["varta log: before", "varta log: /x"]);
}
