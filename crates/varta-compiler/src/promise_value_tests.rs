//! V35 `promise_values` at the compiler level: promises inside aggregates,
//! unions and `unknown`, and the combinators validate in Wasmtime under value
//! layout 16; v34 keeps rejecting every one of them; positions outside the
//! subset fail by construct. Execution against pinned Node logs lives in
//! `varta-runtime` `promise_value_tests`.
use varta_common::language::LanguageProfile;
use wasmtime::{Engine, Module};

const V35: LanguageProfile = LanguageProfile::CheckedF64Utf16V35;
const V34: LanguageProfile = LanguageProfile::CheckedF64Utf16V34;

fn program(body: &str) -> String {
    format!("{body}\nexport function main(): void {{}}")
}
fn message(body: &str, profile: LanguageProfile) -> String {
    match crate::compile_typescript_with_profile(&program(body), profile) {
        Ok(_) => format!("unexpectedly compiled: {body}"),
        Err(error) => error.to_string(),
    }
}

const VALUES: &str = "interface Job { name: string; done: Promise<number>; }
async function value(n: number): Promise<number> { await null; return n; }
async function use(): Promise<number> {
  const list: Promise<number>[] = [value(1), value(2)];
  const job: Job = { name: 'j', done: value(3) };
  const either: Promise<number> | number = list.length > 1 ? job.done : 4;
  const cell: unknown = job.done;
  const named: Record<string, Promise<number>> = { a: value(5) };
  const all = await Promise.all(list);
  const any = await Promise.any(list);
  const raced = await Promise.race([job.done, named['a']!]);
  const settled = await Promise.allSettled(list);
  console.log(cell);
  console.log(JSON.stringify(job));
  return all.length + any + raced + settled.length + (await either);
}";

#[test]
fn promise_values_and_combinators_validate_under_value_layout_16() {
    let bytes = crate::compile_typescript_with_profile(&program(VALUES), V35)
        .unwrap_or_else(|e| panic!("{e}"));
    Module::new(&Engine::default(), &bytes).unwrap();
    let catalog = crate::suspension_tests::catalog(&bytes);
    assert_eq!(
        catalog.value_version,
        varta_common::value::PROMISE_VALUE_VERSION
    );
    // A record descriptor in the data section stamps value16 and tags the
    // promise field with the value16 promise tag.
    let mut header = varta_common::value::PROMISE_VALUE_VERSION
        .to_le_bytes()
        .to_vec();
    header.extend(2u32.to_le_bytes());
    assert!(bytes.windows(header.len()).any(|w| w == header.as_slice()));
}

#[test]
fn v34_keeps_rejecting_promises_inside_values_and_the_combinators() {
    for (body, expected) in [
        (
            "function f(): void { const a: Promise<number>[] = []; }",
            "unsupported promise inside an array",
        ),
        (
            "function f(): void { let u: Promise<number> | undefined; }",
            "unsupported promise inside a union",
        ),
        (
            "interface J { p: Promise<number>; }\nfunction f(j: J): void {}",
            "unsupported promise inside an object",
        ),
        (
            "function f(p: Promise<number>): string { return JSON.stringify(p); }",
            "JSON",
        ),
        (
            "function f(p: Promise<number>): Promise<number[]> { return Promise.all([p]); }",
            "unsupported Promise.all",
        ),
        (
            "function f(p: Promise<number>): void { const u: unknown = p; }",
            "promise value is not assignable to unknown",
        ),
    ] {
        let error = message(body, V34);
        assert!(error.contains(expected), "{body}\n=> {error}");
    }
}

#[test]
fn positions_outside_the_promise_value_subset_fail_with_diagnostics() {
    for (body, expected) in [
        (
            "function f(a: Promise<number>, b: Promise<string>): void { const u: Promise<number> | Promise<string> = a; }",
            "promise types in one union",
        ),
        (
            "function f(): Promise<number[]> { return Promise.all([1, 2]); }",
            "Promise.all takes an array of promises of one type",
        ),
        (
            "async function f(p: Promise<number> | string | boolean): Promise<void> { await p; }",
            "several other members",
        ),
        (
            "function f(p: Promise<number>): Promise<number | Promise<number>> { return Promise.resolve(1); }",
            "a promise never fulfills with a promise",
        ),
        (
            "function f(p: Promise<number> | string): Promise<number> { return Promise.resolve(p); }",
            "requires its other member to be a number",
        ),
        (
            "function f(): void { const r = varta.json.parse<{ p: Promise<number> }>('{}'); }",
            "unsupported",
        ),
    ] {
        let error = message(body, V35);
        assert!(error.contains(expected), "{body}\n=> {error}");
    }
}
