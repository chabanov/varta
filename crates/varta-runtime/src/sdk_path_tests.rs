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
    let bytes = varta_compiler::compile_typescript_with_profile(source, V38)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}

fn logs(source: &str) -> Vec<String> {
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compile(source), &[])
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
