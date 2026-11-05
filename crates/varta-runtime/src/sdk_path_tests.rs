//! V38 SDK paths (`sdk_paths`): the forms uarp-sdk 0.13.2 reaches on its
//! multipart, query and error paths, as minimal reproductions of the SDK's
//! own code (`sdk_path_cases/*.ts`, nothing imported from `examples/`).
//! Every `*.expected` file is the output of Node 24.20.0 running the same
//! program with `await main();` appended, recorded once and checked in;
//! the multipart and query cases print their wire bytes (the multipart
//! boundary, random in Node and deterministic in Varta, replaced by `<b>`).
//! Every program runs under the production budgets. The refused forms fail
//! with their named diagnostics, and earlier profiles keep rejecting the new
//! forms.
use super::*;
use varta_common::language::LanguageProfile;

const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;
const V37: LanguageProfile = LanguageProfile::CheckedF64Utf16V37;

fn compile(source: &str) -> Vec<u8> {
    compile_in(V38, source)
}

fn compile_in(profile: LanguageProfile, source: &str) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}

fn logs(source: &str) -> Vec<String> {
    logs_in(V38, source)
}

fn logs_in(profile: LanguageProfile, source: &str) -> Vec<String> {
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compile_in(profile, source), &[])
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
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

/// The program's log equals the Node output recorded in `expected`.
fn matches_node(source: &str, expected: &str) {
    let expected: Vec<&str> = expected.lines().collect();
    assert_eq!(logs(source), expected, "{source}");
}

fn refused(profile: LanguageProfile, source: &str, diagnostic: &str) {
    let error = varta_compiler::compile_typescript_with_profile(source, profile)
        .err()
        .unwrap_or_else(|| panic!("{profile:?} compiled\n{source}"))
        .to_string();
    assert!(error.contains(diagnostic), "{profile:?}: {error}\n{source}");
}

#[test]
fn sdk_path_multipart_wire_bytes_match_node() {
    matches_node(
        include_str!("sdk_path_cases/multipart.ts"),
        include_str!("sdk_path_cases/multipart.expected"),
    );
}

#[test]
fn sdk_path_query_wire_bytes_match_node() {
    matches_node(
        include_str!("sdk_path_cases/query.ts"),
        include_str!("sdk_path_cases/query.expected"),
    );
}

#[test]
fn sdk_path_error_causes_match_node() {
    matches_node(
        include_str!("sdk_path_cases/error_cause.ts"),
        include_str!("sdk_path_cases/error_cause.expected"),
    );
}

#[test]
fn sdk_path_element_generic_calls_match_node() {
    matches_node(
        include_str!("sdk_path_cases/element_generic.ts"),
        include_str!("sdk_path_cases/element_generic.expected"),
    );
}

#[test]
fn sdk_path_collection_instanceof_matches_node() {
    matches_node(
        include_str!("sdk_path_cases/instanceof_collections.ts"),
        include_str!("sdk_path_cases/instanceof_collections.expected"),
    );
}

#[test]
fn sdk_path_files_and_form_iteration_match_node() {
    matches_node(
        include_str!("sdk_path_cases/file_form.ts"),
        include_str!("sdk_path_cases/file_form.expected"),
    );
}

/// `xs['length']` is no element read: a switch over it is not exhaustive
/// over the element type (it used to trap at `unreachable`), while `xs[i]`
/// still is. V37 keeps its behaviour for the length switch.
#[test]
fn sdk_path_string_keys_of_arrays_are_no_element_reads() {
    let expected = include_str!("sdk_path_cases/element_keys.expected");
    matches_node(include_str!("sdk_path_cases/element_keys.ts"), expected);
    let length_only = "function lengthName(xs: (1 | 2)[]): string {
           switch (xs['length']) { case 1: return 'one'; case 2: return 'two'; }
           return 'other';
         }
         export function main(): void {
           console.log(lengthName([1, 1, 1]));
           console.log(lengthName([1]));
           console.log(lengthName([2, 2]));
         }";
    let lengths: Vec<&str> = expected.lines().take(3).collect();
    assert_eq!(logs_in(V37, length_only), lengths);
    assert_eq!(logs(length_only), lengths);
}

/// WHATWG "create an entry": every Blob entry is a File (named `blob` or the
/// given file name), as get, getAll, iteration and the wire bytes show.
#[test]
fn sdk_path_form_entries_are_files_as_in_node() {
    matches_node(
        include_str!("sdk_path_cases/form_entries.ts"),
        include_str!("sdk_path_cases/form_entries.expected"),
    );
}

/// A program's Blob subclass cannot change the multipart file name through
/// a same-named method, and a File subclass entry stays the same object.
#[test]
fn sdk_path_form_subclasses_keep_node_entries() {
    matches_node(
        include_str!("sdk_path_cases/form_subclass.ts"),
        include_str!("sdk_path_cases/form_subclass.expected"),
    );
}

/// An engine-internal member of a prelude class (`Blob.byteString`, which
/// the multipart serializer and `File` read) cannot be overridden.
#[test]
fn sdk_path_engine_internal_members_cannot_be_overridden() {
    let program = |member: &str| {
        format!(
            "class Evil extends Blob {{ {member} }}
             export async function main(): Promise<void> {{
               const form = new FormData();
               form.append('k', new Evil(['x']));
               console.log(await new Response(form).text());
             }}"
        )
    };
    refused(
        V38,
        &program("byteString(): string { return 'evil'; }"),
        "override of the engine-internal member byteString of Blob",
    );
    refused(
        V38,
        &program("encode(text: string): string { return text; }"),
        "override of the engine-internal member encode of Blob",
    );
    refused(
        V38,
        "class Evil extends File { byteString(): string { return 'evil'; } }
         export function main(): void { console.log(String(new Evil(['x'], 'n').size)); }",
        "override of the engine-internal member byteString of Blob",
    );
}

/// `"cause" in` an Error instance is refused: JavaScript defines the own
/// `cause` only when the options object has one (Node prints false for
/// `new Error('y', {})`, `new Error('y', undefined)`, `new TypeError('z',
/// {})` and `new X('z')`), which the instance's cause slot does not record.
/// `error.cause` reads and the inherited `message`/`name` tests stay.
#[test]
fn sdk_path_cause_presence_tests_of_errors_are_refused() {
    let program = |body: &str| {
        format!(
            "class X extends Error {{}}
             class Y extends Error {{ constructor(m: string) {{ super(m, {{ cause: 1 }}); }} }}
             export function main(): void {{ {body} }}"
        )
    };
    let diagnostic = "\"cause\" in an Error instance";
    for body in [
        "console.log(String('cause' in new Error('y', {})));",
        "console.log(String('cause' in new Error('y', undefined)));",
        "console.log(String('cause' in new TypeError('z', {})));",
        "console.log(String('cause' in new X('z')));",
        "const e: X | Y = new X('q'); if ('cause' in e) { console.log('yes'); }",
    ] {
        refused(V38, &program(body), diagnostic);
    }
    assert_eq!(
        logs(&program(
            "const e = new Error('a', { cause: 1 });
             console.log(String('message' in new X('z')) + ' ' + String('name' in e) + ' ' + String(e.cause));"
        )),
        ["true true 1"]
    );
}

/// Own-key enumeration of a view of a wider object (Object.keys, values,
/// entries, an Object.assign source, for...in) is refused like a spread:
/// Node enumerates the object's own keys (`b,a,c`), not the view's (`a,b`).
/// A view whose sources have exactly its fields in its order stays
/// admitted and prints Node's output.
#[test]
fn sdk_path_enumerations_of_wider_views_are_refused() {
    let program = |body: &str| {
        format!(
            "interface Wide {{ b: number; a: number; c: string }}
             interface Narrow {{ a: number; b: number }}
             function show(n: Narrow): string {{ {body} }}
             export function main(): void {{
               const w: Wide = {{ b: 1, a: 2, c: 'x' }};
               console.log(show(w));
             }}"
        )
    };
    let diagnostic = "own-key enumeration (Object.keys, values, entries, an Object.assign \
                      source or for...in) of a Narrow that may hold a wider Wide (a view)";
    for body in [
        "return Object.keys(n).join(',');",
        "return JSON.stringify(Object.values(n));",
        "return JSON.stringify(Object.entries(n));",
        "return JSON.stringify(Object.assign({}, n));",
        "let s = ''; for (const k in n) s += k; return s;",
    ] {
        refused(V38, &program(body), diagnostic);
    }
    refused(
        V38,
        "interface Wide { b: number; a: number; c: string }
         interface Narrow { a: number; b: number }
         export function main(): void {
           const w: Wide = { b: 1, a: 2, c: 'x' };
           console.log(JSON.stringify(Object.assign({}, w as Narrow)));
         }",
        diagnostic,
    );
    // A view of a narrower field type with the same fields in the same
    // order: Node prints the same keys.
    assert_eq!(
        logs(
            "interface Point { a: 1; b: number }
             interface Pair { a: number; b: number }
             function show(n: Pair): string {
               let s = '';
               for (const k in n) s += k;
               return Object.keys(n).join(',') + ' ' + JSON.stringify(Object.entries(n)) + ' '
                 + JSON.stringify(Object.values(n)) + ' ' + JSON.stringify(Object.assign({}, n)) + ' ' + s;
             }
             export function main(): void {
               const p: Point = { a: 1, b: 2 };
               console.log(show(p));
             }"
        ),
        [r#"a,b [["a",1],["b",2]] [1,2] {"a":1,"b":2} ab"#]
    );
}

/// The SDK's `appendPart(form, key, value: unknown)` with a byte part: the
/// part reaches `unknown` inside the multipart record, and reading it there
/// throws a named `TypeError` instead of guessing (Node appends the bytes).
/// A `Uint8Array` has no `unknown` cell in value layout 18, and the record
/// view of an `ArrayBuffer` instance held by `unknown` is not modelled.
#[test]
fn sdk_path_byte_parts_through_unknown_throw_named_type_errors() {
    let program = |field: &str, value: &str| {
        format!(
            "interface Upload {{ file: {field} }}
             function appendPart(form: FormData, key: string, value: unknown): void {{
               if (value instanceof Blob) {{ form.append(key, value); return; }}
               if (typeof value === 'object' && value !== null && 'data' in (value as Record<string, unknown>)) {{
                 form.append(key, 'file part');
                 return;
               }}
               form.append(key, String(value));
             }}
             export function main(): void {{
               const upload: Upload = {{ file: {value} }};
               const spec: {{ multipart?: object }} = {{ multipart: upload }};
               const form = new FormData();
               try {{
                 for (const [key, value] of Object.entries(spec.multipart as Record<string, unknown>)) {{
                   appendPart(form, key, value);
                 }}
                 console.log('appended');
               }} catch (error) {{
                 console.log(error instanceof TypeError ? error.message : 'other');
               }}
             }}"
        )
    };
    assert_eq!(
        logs(&program("Uint8Array", "new Uint8Array([1, 2])")),
        ["a Uint8Array held by unknown cannot be read as a JSON value"]
    );
    assert_eq!(
        logs(&program(
            "{ data: Uint8Array; filename?: string }",
            "{ data: new Uint8Array([1]), filename: 'a.bin' }"
        )),
        ["a Uint8Array held by unknown cannot be read as a JSON value"]
    );
    assert_eq!(
        logs(&program("ArrayBuffer", "new ArrayBuffer(2)")),
        ["a class instance held by unknown cannot be read as a JSON object"]
    );
    assert_eq!(logs(&program("Blob", "new Blob(['x'])")), ["appended"]);
}

/// The forms of the pending lists that stay outside the subset fail with
/// their named diagnostics.
#[test]
fn sdk_path_refused_forms_name_their_diagnostics() {
    let main = |body: &str| format!("export function main(): void {{ {body} }}");
    // A Map, Set or Uint8Array never enters `unknown` (value layout 18).
    refused(
        V38,
        &main("const u: unknown = new Uint8Array([1]); console.log(String(u));"),
        "conversion to unknown of a value holding a Map, Set, Uint8Array",
    );
    refused(
        V38,
        "function k(v: unknown): boolean { return v instanceof Map; }
         export function main(): void { console.log(String(k(1))); }",
        "instanceof Map of an unknown or object value",
    );
    refused(
        V38,
        "function k(v: object): boolean { return v instanceof Uint8Array; }
         export function main(): void { console.log(String(k({}))); }",
        "instanceof Uint8Array of an unknown or object value",
    );
    // A Uint8Array and another array share the array tag: no such union.
    refused(
        V38,
        "function k(v: Uint8Array | string[]): boolean { return v instanceof Uint8Array; }
         export function main(): void { console.log(String(k(['a']))); }",
        "multiple record shapes, array element types",
    );
    // `A[] | B[]` of records: one element kind, no element tag.
    refused(
        V38,
        "interface A { a: number } interface B { b: string }
         function n(x: A[] | B[]): number { return x.length; }
         export function main(): void { console.log(String(n([{ a: 1 }]))); }",
        "array members of one union with the same element kind",
    );
    // Spreads through a view of a wider object, into a record or a dictionary.
    let view = |spread: &str| {
        format!(
            "interface Wide {{ b: number; a: number; c: string }}
             interface Narrow {{ a: number; b: number }}
             function copy(n: Narrow): string {{ {spread} }}
             export function main(): void {{
               const w: Wide = {{ b: 1, a: 2, c: 'x' }};
               console.log(copy(w));
             }}"
        )
    };
    let wider = "object spread of a Narrow that may hold a wider Wide (a view)";
    refused(V38, &view("return JSON.stringify({ ...n });"), wider);
    refused(
        V38,
        &view("const q: Record<string, unknown> = { ...n }; return Object.keys(q).join(',');"),
        wider,
    );
    // Error constructors take (message?, options?) and nothing else.
    let constructor = "error constructor arguments other than (message?, options?)";
    refused(
        V38,
        &main("const args: [string] = ['m']; console.log(new Error(...args).message);"),
        constructor,
    );
    refused(
        V38,
        &main("console.log(new TypeError('a', { cause: 1 }, 'x').message);"),
        constructor,
    );
    refused(
        V38,
        &main("console.log(new Error<string>('a', { cause: 1 }).message);"),
        constructor,
    );
    // `File.lastModified` defaults to the wall clock; the Web byte members
    // not in the subset are undeclared.
    refused(
        V38,
        &main("const f = new File(['a'], 'a.txt'); console.log(String(f.lastModified));"),
        "property lastModified is not declared by File",
    );
    refused(
        V38,
        &main("const s = new Blob(['a']).stream(); console.log(String(s.locked));"),
        "property stream is not declared by Blob",
    );
    refused(
        V38,
        "export async function main(): Promise<void> {
           const form = await new Response('a=b').formData();
           console.log(String(form.has('a')));
         }",
        "property formData is not declared by Response",
    );
}

/// A spread whose view sources have exactly the view's fields in its order
/// copies the same keys and stays admitted (the SDK's `{ ...spec, headers,
/// query }` in `Transport.stream`, passed back as a `RequestSpec`).
#[test]
fn sdk_path_spreads_of_same_shape_views_stay_admitted() {
    assert_eq!(
        logs(
            "interface Spec { method: string; path: string; query?: Record<string, unknown> }
             function send(spec: Spec): string { return JSON.stringify(spec); }
             function stream(spec: Spec, token: string): string {
               const query = token ? { ...spec.query, token: token } : spec.query;
               return send({ ...spec, query });
             }
             export function main(): void {
               console.log(stream({ method: 'GET', path: '/p', query: { a: 1 } }, 't'));
               console.log(stream({ method: 'GET', path: '/q' }, ''));
             }"
        ),
        [
            r#"{"method":"GET","path":"/p","query":{"a":1,"token":"t"}}"#,
            r#"{"method":"GET","path":"/q"}"#,
        ]
    );
}

/// V37 keeps rejecting the forms v38 admits here.
#[test]
fn sdk_path_forms_stay_refused_by_v37() {
    let main = |body: &str| format!("export function main(): void {{ {body} }}");
    refused(
        V37,
        &main("const e = new Error('outer', { cause: 'inner' }); console.log(e.message);"),
        "error constructor accepts zero or one string/undefined argument",
    );
    refused(
        V37,
        "function causeOf(error: unknown): unknown { return error instanceof Error ? error.cause : undefined; }
         export function main(): void { console.log(String(causeOf(new Error('x')))); }",
        "Error property other than name/message",
    );
    refused(
        V37,
        "function k(v: Uint8Array | string): boolean { return v instanceof Uint8Array; }
         export function main(): void { console.log(String(k('s'))); }",
        "instanceof requires an unshadowed supported error class",
    );
    refused(
        V37,
        &main("const f = new File(['a'], 'a.txt'); console.log(f.name);"),
        "unsupported constructor",
    );
}

/// An uncaught `new TypeError(message, options)` is reported like an Error
/// subclass instance: the family's name and the message.
#[test]
fn sdk_path_uncaught_options_errors_report_name_and_message() {
    let source = "export function main(): void { throw new TypeError('boom', { cause: 'x' }); }";
    match VartaRuntime::new().run_wasm_bytes(&compile(source), &[]) {
        Err(RuntimeError::Uncaught(exception)) => assert_eq!(
            exception.value,
            varta_common::exceptions::ThrownValue::ErrorInstance {
                name: "TypeError".encode_utf16().collect(),
                message: "boom".encode_utf16().collect(),
            }
        ),
        other => panic!("expected an uncaught exception, got {other:?}"),
    }
}
