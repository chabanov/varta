//! v34 `unknown_type()` and `checked_assertions()`: `unknown` cells, guard
//! narrowing, checked `as`/`!` and caught values. Expected lines were checked
//! against Node 24 with the same sources (Varta logs plain booleans as 0/1).
use super::*;
use varta_common::{exceptions::ThrownValue, language::LanguageProfile as P};

fn compile(source: &str, profile: P) -> Result<Vec<u8>, varta_compiler::CompilerError> {
    varta_compiler::compile_typescript_with_profile(source, profile)
}
fn module(source: &str) -> Vec<u8> {
    let bytes = compile(source, P::CheckedF64Utf16V34).unwrap_or_else(|e| panic!("{e}\n{source}"));
    // Emitted modules must also pass backend validation, not only compilation.
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}
fn lines(expected: &[&str]) -> Vec<String> {
    expected.iter().map(|s| format!("varta log: {s}")).collect()
}
fn run(source: &str, expected: &[&str]) {
    let bytes = module(source);
    assert_eq!(
        VartaRuntime::new()
            .run_wasm_bytes(&bytes, &[])
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"))
            .lines,
        lines(expected),
        "{source}"
    );
}
fn uncaught(source: &str) -> ThrownValue {
    let bytes = module(source);
    match VartaRuntime::new().run_wasm_bytes(&bytes, &[]) {
        Err(RuntimeError::Uncaught(exception)) => exception.value,
        other => panic!("expected an uncaught exception, got {other:?}\n{source}"),
    }
}
fn rejected(source: &str, profile: P) -> String {
    compile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("accepted under {profile:?}:\n{source}"))
        .to_string()
}

// Pinned Node 24 outputs of `unknown_cases/*.ts` (`main()` run by Node with
// type stripping); the files also type-check with `tsc --strict`.
#[test]
fn guards_equality_truth_and_string_conversion() {
    run(
        include_str!("unknown_cases/guards.ts"),
        &[
            "string:ab:2 string truthy",
            "number:42 number truthy",
            "boolean:false boolean truthy",
            "bigint:10 bigint truthy",
            "null object falsy",
            "undefined undefined falsy",
            "other:object object truthy",
            "other:object object truthy",
            "literal fast",
            "no literal",
            "no literal",
            "0 falsy",
            " falsy",
            "NaN falsy",
            "0 falsy",
            "0 falsy",
            "false falsy",
            "x truthy",
            "1 truthy",
            "true",
            "false",
            "false",
            "true",
            "false",
            "false",
            "true",
            "true",
            "default",
            "default",
            "0",
            "empty",
            "",
            "12",
            "undefined",
            "[object Object]",
            "TypeError: bad",
            "now",
            "tpl 1 null [object Object] 2 undefined",
        ],
    );
}

#[test]
fn caught_values_error_causes_and_instanceof() {
    run(
        include_str!("unknown_cases/errors.ts"),
        &[
            "api 503 HTTP 503",
            "connection down cause TypeError: socket",
            "error RangeError: range",
            "value text",
            "value 42",
            "error UarpError: wrapped",
            "ok",
            "number 43",
            "rethrown APIConnectionError: down",
            "rethrown text",
            "true",
            "true",
            "code 7",
            "labeled a",
            "box b",
            "not a box",
            "not a box",
        ],
    );
}

/// The conformance cases, which the inventory records as gaps of its pinned
/// v33 profile, pass under v34 with the stdout their manifest rows pin.
#[test]
fn conformance_cases_pass_under_v34() {
    run(
        include_str!("../../../conformance/cases/unknown-narrowing.ts"),
        &[
            "string a",
            "number 2",
            "null",
            "undefined",
            "object",
            "failure outer string inner",
            "error range",
            "number 42",
            "true",
            "a 1",
        ],
    );
    run(
        include_str!("../../../conformance/cases/checked-assertions.ts"),
        &["4", "2", "7", "3", "a", "high"],
    );
}

/// The language-guide examples. Node prints the same lines except that the
/// failing `values[0] as Point` logs `undefined` there instead of throwing.
#[test]
fn language_guide_examples() {
    run(
        include_str!("unknown_cases/guide.ts"),
        &[
            "ApiError: request failed",
            "number 405",
            "[object Object]",
            "4",
            "2",
            "3",
            "type assertion to Point failed",
            "set",
        ],
    );
}

#[test]
fn generic_arguments_and_class_fields_hold_unknown() {
    run(
        "function wrap<T>(value: T): T[] { return [value]; }
class Slot { payload: unknown = undefined; }
export function main(): void {
  const items = wrap<unknown>('x');
  console.log(String(items.length) + ' ' + typeof items[0]);
  const slot = new Slot();
  console.log(typeof slot.payload);
  slot.payload = [1, 2];
  console.log(String(Array.isArray(slot.payload)));
  const held = slot.payload as number[];
  console.log(String(held[1]));
}",
        &["1 string", "undefined", "true", "2"],
    );
}

/// Typing caught values as `unknown` keeps the output of the inventory's
/// existing catch cases identical between v33 and v34.
#[test]
fn existing_catch_cases_keep_their_output_under_v34() {
    for source in [
        include_str!("../../../conformance/cases/error-subclasses.ts"),
        include_str!("../../../conformance/cases/generators.ts"),
    ] {
        let output = |profile| {
            let bytes = compile(source, profile).unwrap_or_else(|e| panic!("{e}\n{source}"));
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines
        };
        assert_eq!(output(P::CheckedF64Utf16V33), output(P::CheckedF64Utf16V34));
    }
}

#[test]
fn checked_assertions_that_hold() {
    run(
        include_str!("unknown_cases/assertions.ts"),
        &[
            "text 4", "5", "svc 3", "4", "3 3", "4", "true", "null", "14", "false", "string",
            "true", "high", "number", "svc", "job svc", "true", "a", "set", "2 tail",
        ],
    );
}

#[test]
fn closures_generators_records_arrays_and_json() {
    run(
        include_str!("unknown_cases/flow.ts"),
        &[
            "number 4",
            "other string",
            "yield 1",
            "yield two",
            "yield null",
            "walked 3",
            "a=text;b=array;c=null;d=object;e=number;",
            r#"{"id":7,"tags":["x","y"]}"#,
            r#"[1,"a",null]"#,
            "undefined",
            r#"{"when":3}"#,
            "40 55",
        ],
    );
}

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// Failing assertions throw `TypeError` (Node erases `as`/`!` and would not).
#[test]
fn failing_assertions_throw_type_error() {
    run(
        "interface Point { x: number; y: number }
interface Other { x: number; z: number }
class Shape { sides: number; constructor(sides: number) { this.sides = sides; } }
class Square extends Shape { constructor() { super(4); } }
function attempt(label: string, value: unknown, kind: number): void {
  try {
    if (kind === 0) console.log(value as string);
    if (kind === 1) console.log(String((value as Point).x));
    if (kind === 2) console.log(String((value as number[]).length));
    if (kind === 3) console.log(String((value as Square).sides));
    if (kind === 4) console.log(value as 'a' | 'b');
    if (kind === 5) console.log((value as string | null) === null ? 'null' : 'text');
    console.log(label + ' passed');
  } catch (e) {
    if (e instanceof TypeError) console.log(label + ' ' + e.name + ': ' + e.message);
  }
}
function strict(value: string | undefined): string {
  try {
    return value!;
  } catch (e) {
    return e instanceof TypeError ? e.message : 'other';
  }
}
export function main(): void {
  const other: Other = { x: 1, z: 2 };
  attempt('number as string', 1, 0);
  attempt('other shape', other, 1);
  attempt('point', { x: 3, y: 4 }, 1);
  attempt('string array', ['a'], 2);
  attempt('base shape', new Shape(3), 3);
  attempt('literal', 'c', 4);
  attempt('literal ok', 'b', 4);
  attempt('union', 5, 5);
  console.log(strict(undefined));
  console.log(strict('kept'));
}",
        &[
            "number as string TypeError: type assertion to string failed",
            "other shape TypeError: type assertion to Point failed",
            "3",
            "point passed",
            "string array TypeError: type assertion to number[] failed",
            "base shape TypeError: type assertion to Square failed",
            "literal TypeError: type assertion to 'a' | 'b' failed",
            "b",
            "literal ok passed",
            "union TypeError: type assertion to string | null failed",
            "non-null assertion failed: value is null or undefined",
            "kept",
        ],
    );
    let thrown = uncaught(
        "export function main(): void { const v: unknown = 1; console.log(v as string); }",
    );
    assert!(
        matches!(&thrown, ThrownValue::NativeError { class: varta_common::value::ErrorClass::TypeError, message, .. }
            if *message == units("type assertion to string failed")),
        "{thrown:?}"
    );
}

/// Varta-specific results: logging boxes, `as Error` over Error classes,
/// rethrowing non-throwable values and unsupported String conversions.
#[test]
fn logging_error_assertions_and_unthrowable_values() {
    run(
        "class AppError extends Error {}
function message(e: unknown): string {
  try {
    return (e as Error).message;
  } catch (failure) {
    return failure instanceof TypeError ? failure.message : 'other';
  }
}
function raise(value: unknown): string {
  try {
    throw value;
  } catch (e) {
    return e instanceof TypeError ? 'TypeError ' + e.message : 'kept ' + String(e);
  }
}
function text(value: unknown): string {
  try {
    return String(value);
  } catch (e) {
    return e instanceof TypeError ? e.message : 'other';
  }
}
export function main(): void {
  const values: unknown[] = ['s', 1.5, true, null, undefined, 3n, { a: [1] }];
  for (const value of values) console.log(value);
  console.log(message(new AppError('app')));
  console.log(message(new RangeError('range')));
  console.log(message('plain'));
  console.log(raise('text'));
  console.log(raise(new AppError('rethrown')));
  console.log(raise({ a: 1 }));
  console.log(text([1, 2]));
  console.log(text(new AppError('shown')));
}",
        &[
            "s",
            "1.5",
            "1",
            "null",
            "undefined",
            "3",
            r#"{"a":[1]}"#,
            "app",
            "range",
            "type assertion to Error failed",
            "kept text",
            "kept Error: rethrown",
            "TypeError thrown value is not a primitive, BigInt or Error value",
            "String conversion of an array held by unknown is not supported",
            "Error: shown",
        ],
    );
}

/// Run with an explicit test-only fuel budget so the churn forces collections.
fn run_collected(source: &str) -> Vec<String> {
    let bytes = module(source);
    let runtime = VartaRuntime::new();
    let module = runtime.validate_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(400 * abi::FUEL).unwrap();
    let instance = execution.instantiate(&module).unwrap();
    instance
        .get_typed_func::<(), ()>(&mut execution.store, "_start")
        .unwrap()
        .call(&mut execution.store, ())
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    crate::exceptions::finish(&instance, &mut execution.store).unwrap();
    execution.store.data().output.lines.clone()
}

#[test]
fn reference_payloads_held_as_unknown_survive_collection() {
    // Records, arrays, strings, BigInts, errors and class instances are
    // reachable only through `unknown` locals, fields, array elements, a
    // closure capture and a caught value while garbage forces collections.
    let lines = run_collected(
        r#"
function churn():void{let payload="x";for(let i=0;i<14;i=i+1){payload=payload+payload;}for(let i=0;i<350;i=i+1){const values=[payload+"!",payload+"?"];values.slice();}}
class Node{label:string;constructor(label:string){this.label=label;}}
interface Holder{first:unknown;rest:unknown[]}
function garbage(i:number):unknown[]{const g:unknown={k:"g"+"h",v:[i,i+1]};const h:unknown="s"+"t";return [g,h];}
function keep(value:unknown):()=>unknown{return ()=>value;}
export function main():void{
 const record:unknown={name:"a"+"b",tags:["c"+"d"]};
 const holder:Holder={first:new Node("n"+"1"),rest:["e"+"f",12n**20n,new RangeError("r"+"s"),[1,2,3]]};
 const kept=keep({deep:{text:"g"+"h"}});
 let caught:unknown=undefined;
 try{throw new TypeError("t"+"u");}catch(e){caught=e;}
 churn();for(let i=0;i<6000;i=i+1){garbage(i);}churn();
 console.log(JSON.stringify(record));
 console.log((holder.first as Node).label);
 for(const item of holder.rest){console.log(Array.isArray(item)?"array "+String((item as number[]).length):String(item));}
 console.log(JSON.stringify(kept()));
 if(caught instanceof TypeError){console.log(caught.message);}
}
"#,
    );
    assert_eq!(
        lines,
        [
            r#"{"name":"ab","tags":["cd"]}"#,
            "n1",
            "ef",
            "3833759992447475122176",
            "RangeError: rs",
            "array 3",
            r#"{"deep":{"text":"gh"}}"#,
            "tu",
        ]
        .map(|s| format!("varta log: {s}"))
    );
}

#[test]
fn unsupported_unknown_uses_and_assertions_are_diagnosed() {
    let main = |body: &str| format!("export function main(): void {{ {body} }}");
    for (body, expected) in [
        (
            "const v: unknown = { a: 1 }; console.log(v.a);",
            "property access on an unknown value",
        ),
        (
            "const v: unknown = 1; console.log(v + 1);",
            "operator on an unknown value",
        ),
        (
            "const v: unknown = 1; console.log(v < 2);",
            "operator on an unknown value",
        ),
        (
            "const g = (): number => 1; const f: unknown = g; console.log(typeof f);",
            "function value is not assignable to unknown",
        ),
        (
            "const v: unknown = [1]; console.log(v as unknown[]);",
            "arrays are checked by element kind",
        ),
        (
            "const v: unknown = 1; const f = v as () => number; console.log(f());",
            "unsupported type assertion to () => number",
        ),
        (
            "const s: string = 'a'; console.log(s as number);",
            "neither type sufficiently overlaps",
        ),
        (
            "const p: { a: number } = { a: 1 }; console.log(p as { b: string });",
            "distinct record or array type",
        ),
        (
            "const v: unknown = 'a'; console.log(<string>v);",
            "angle-bracket type assertion",
        ),
        ("const n: null = null; console.log(n!);", "always null"),
        (
            "const v: unknown = '1'; console.log(Number(v));",
            "Number conversion",
        ),
        (
            "const v = varta.json.parse<unknown>('1'); console.log(v);",
            "JSON parsing target type",
        ),
    ] {
        let error = rejected(&main(body), P::CheckedF64Utf16V34);
        assert!(error.contains(expected), "{body}: {error}");
    }
    let error = rejected(
        "class E extends Error {} export function main(): void { const v: unknown = new E(); console.log(JSON.stringify(v)); }",
        P::CheckedF64Utf16V34,
    );
    assert!(
        error.contains("JSON.stringify of an Error subclass"),
        "{error}"
    );
}

#[test]
fn earlier_profiles_keep_rejecting_unknown_assertions_and_non_null() {
    for profile in [P::CheckedF64Utf16V32, P::CheckedF64Utf16V33] {
        for (source, expected) in [
            (
                "export function main(): void { const v: unknown = 1; console.log(typeof v); }",
                "unsupported type annotation",
            ),
            (
                "export function main(): void { const v: string = 'a'; console.log(v as string); }",
                "unsupported expression",
            ),
            (
                "export function main(): void { const v: string | undefined = 'a'; console.log(v!); }",
                "unsupported expression",
            ),
            (
                "export function main(): void { try { console.log('a'); } catch (e: unknown) { console.log('b'); } }",
                "catch binding annotation",
            ),
        ] {
            let error = rejected(source, profile);
            assert!(error.contains(expected), "{profile:?} {source}: {error}");
        }
        // The catch binding keeps the closed catch union there: narrowing to
        // a union member and `typeof` work as before, unchanged by v34.
        let bytes = compile(
            "export function main(): void { try { throw 'x'; } catch (e) { if (typeof e === 'string') console.log(e + '!'); } }",
            profile,
        )
        .unwrap();
        assert_eq!(
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines,
            lines(&["x!"])
        );
    }
}
