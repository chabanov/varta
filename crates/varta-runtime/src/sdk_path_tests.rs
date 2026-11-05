//! V38 SDK paths (`sdk_paths`): the forms uarp-sdk 0.13.2 reaches on its
//! multipart, query and error paths, as minimal reproductions of the SDK's
//! own code (`sdk_path_cases/*.ts`, nothing imported from `examples/`).
//! Every `*.expected` file is the output of Node 24.20.0 running the same
//! program with `await main();` appended, recorded once and checked in;
//! the multipart and query cases print their wire bytes (the multipart
//! boundary, random in Node and deterministic in Varta, replaced by `<b>`).
//! Every program runs under the production budgets. The refused forms fail
//! with their named diagnostics, and earlier profiles keep rejecting the new
//! forms. The byte cases (`byte_*.ts`, v38 `byte_values`) hold `Uint8Array`,
//! `ArrayBuffer`, `Blob` and `File` values in `unknown`; `byte_in.ts` was
//! generated from Node's own prototype chains.
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

/// The SDK's real `appendPart(form, key, value: unknown)` with byte parts
/// (v38 `byte_values`): a `Uint8Array`, an `ArrayBuffer` and `{ data,
/// filename, contentType }` parts reach `unknown` inside the multipart
/// record, and the wire bytes, part order included, are Node's.
#[test]
fn sdk_path_byte_parts_through_unknown_match_node() {
    matches_node(
        include_str!("sdk_path_cases/byte_parts.ts"),
        include_str!("sdk_path_cases/byte_parts.expected"),
    );
}

/// A `Uint8Array` held by `unknown` keeps its identity and answers
/// `typeof`, `instanceof`, `ArrayBuffer.isView`, `String` and
/// `JSON.stringify` as in Node, also inside records, arrays, dictionaries
/// and unions.
#[test]
fn sdk_path_bytes_held_by_unknown_match_node() {
    matches_node(
        include_str!("sdk_path_cases/byte_values.ts"),
        include_str!("sdk_path_cases/byte_values.expected"),
    );
}

/// `"key" in` a `Uint8Array`, `ArrayBuffer`, `Blob` or `File` held by
/// `unknown` answers every member of their Node 24 prototype chains and the
/// index, numeric and own keys as Node does; records and JSON objects keep
/// their own keys.
#[test]
fn sdk_path_in_of_bytes_and_prelude_instances_matches_node() {
    matches_node(
        include_str!("sdk_path_cases/byte_in.ts"),
        include_str!("sdk_path_cases/byte_in.expected"),
    );
}

/// What the JSON-object view of a value held by `unknown` still refuses,
/// with named `TypeError`s: a `Uint8Array`, an array or an engine instance
/// read as `Record<string, unknown>` (own keys, values, a dynamic `in` key)
/// and an instance of a program's subclass of `Blob`. A logged `Uint8Array`
/// cell uses the diagnostic notation of its JSON object; a typed one is not
/// logged, like any typed record or array.
#[test]
fn sdk_path_record_views_of_bytes_and_instances_throw_named_type_errors() {
    let program = |value: &str, body: &str| {
        format!(
            "class Tagged extends Blob {{ tag = 1; }}
             function read(v: unknown, key: string): string {{ {body} }}
             export function main(): void {{
               const v: unknown = {value};
               try {{ console.log(read(v, 'data')); }} catch (error) {{
                 console.log(error instanceof TypeError ? error.message : 'other');
               }}
             }}"
        )
    };
    let failed = "type assertion to Record<string, unknown> failed";
    let instance = "a class instance held by unknown cannot be read as a JSON object";
    for (value, body, expected) in [
        (
            "new Uint8Array([1])",
            "return Object.keys(v as Record<string, unknown>).join(',');",
            failed,
        ),
        (
            "new Uint8Array([1])",
            "return String(key in (v as Record<string, unknown>));",
            failed,
        ),
        (
            "['x']",
            "return String('data' in (v as Record<string, unknown>));",
            failed,
        ),
        (
            "new ArrayBuffer(1)",
            "return Object.keys(v as Record<string, unknown>).join(',');",
            instance,
        ),
        (
            "new Tagged(['x'])",
            "return String('size' in (v as Record<string, unknown>));",
            instance,
        ),
    ] {
        assert_eq!(logs(&program(value, body)), [expected], "{value}: {body}");
    }
    assert_eq!(
        logs(
            "function either(flag: boolean): Uint8Array | string { return flag ? new Uint8Array(0) : 'x'; }
             export function main(): void {
               const held: unknown = new Uint8Array([1, 255]);
               console.log(held);
               console.log(either(true));
               console.log(either(false));
             }"
        ),
        [r#"{"0":1,"1":255}"#, "{}", "x"]
    );
    refused(
        V38,
        "export function main(): void { console.log(new Uint8Array([1])); }",
        "cannot log Uint8Array",
    );
}

/// The value readers check every byte of a `Uint8Array` they serialize:
/// an element that is no integer from 0 to 255 (a fraction, 256, -0, NaN)
/// or a hole is a fatal layout error in the host and the linked encoder,
/// and value layout 17 rejects the byte-array tag.
#[test]
fn sdk_path_corrupt_byte_arrays_are_rejected_by_value_readers() {
    use varta_common::value::{self, ARRAY_META_BYTES, ARRAY_SLOT_BYTES, ValueKind};
    const MARKER: [f64; 3] = [7.0, 77.0, 177.0];
    let source = "const kept: Uint8Array[] = [];
         export function main(): void { kept.push(new Uint8Array([7, 77, 177])); console.log(String(kept.length)); }";
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&compile(source)).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    instance
        .get_func(&mut *store, "_start")
        .unwrap()
        .call(&mut *store, &[], &mut [])
        .unwrap();
    let bounds = store.data().module;
    let memory = instance.get_memory(&mut *store, "memory").unwrap();
    let data = memory.data(&*store).to_vec();
    let version = value::HASHED_DICTIONARY_VALUE_VERSION;
    // The vector whose storage holds the marker bytes.
    let handle = (varta_common::abi::OWNED_STRING_BASE..data.len() - 8)
        .step_by(value::HEAP_ALIGNMENT)
        .map(|ptr| ptr as u64 | ((value::VECTOR_BYTES as u64) << i32::BITS))
        .find(|handle| {
            value::Vector::read_in(&data, *handle, version, bounds).is_ok_and(|vector| {
                vector.length == MARKER.len()
                    && (0..MARKER.len()).all(|index| {
                        let at = vector.storage + ARRAY_META_BYTES + index * ARRAY_SLOT_BYTES;
                        data[at..at + 8] == MARKER[index].to_le_bytes()
                    })
            })
        })
        .expect("the byte storage in guest memory");
    let kind = ValueKind::Bytes as u32;
    let host = |data: &[u8]| json::render_reference_in(data, kind, handle, version, bounds);
    let linked = |data: &[u8]| {
        let mut out: Vec<u16> = Vec::new();
        varta_intrinsics::json::stringify(data, kind, handle, bounds, &mut out)
            .map(|_| String::from_utf16(&out).unwrap())
    };
    assert_eq!(host(&data).unwrap(), br#"{"0":7,"1":77,"2":177}"#);
    assert_eq!(linked(&data).unwrap(), r#"{"0":7,"1":77,"2":177}"#);
    assert!(
        json::render_reference_in(
            &data,
            kind,
            handle,
            value::FUNCTION_FIELD_VALUE_VERSION,
            bounds
        )
        .is_err()
    );
    let vector = value::Vector::read_in(&data, handle, version, bounds).unwrap();
    let element = |index: usize| vector.storage + ARRAY_META_BYTES + index * ARRAY_SLOT_BYTES;
    let bitmap = element(vector.capacity);
    let mut corruptions: Vec<(&str, Vec<u8>)> = Vec::new();
    for (name, bad) in [
        ("fraction", 1.5f64),
        ("above a byte", 256.0),
        ("negative zero", -0.0),
        ("NaN", f64::NAN),
    ] {
        let mut corrupt = data.clone();
        corrupt[element(1)..element(1) + 8].copy_from_slice(&bad.to_le_bytes());
        corruptions.push((name, corrupt));
    }
    let mut hole = data.clone();
    hole[bitmap] &= !1;
    hole[element(0)..element(0) + 8].copy_from_slice(&0u64.to_le_bytes());
    corruptions.push(("hole", hole));
    for (name, corrupt) in corruptions {
        assert!(
            value::byte_vector(&corrupt, handle, version, bounds).is_err(),
            "{name}"
        );
        assert!(host(&corrupt).is_err(), "{name}");
        assert!(linked(&corrupt).is_err(), "{name}");
    }
}

/// The forms of the pending lists that stay outside the subset fail with
/// their named diagnostics.
#[test]
fn sdk_path_refused_forms_name_their_diagnostics() {
    let main = |body: &str| format!("export function main(): void {{ {body} }}");
    // A Map or Set never enters `unknown` (value layout 18).
    refused(
        V38,
        &main("const u: unknown = new Map<string, number>(); console.log(String(u));"),
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
        "function k(v: object): boolean { return v instanceof Set; }
         export function main(): void { console.log(String(k({}))); }",
        "instanceof Set of an unknown or object value",
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
    // A `Uint8Array` never enters `unknown` and is never serialized in v37
    // (value layout 17 has no byte-array tag).
    let opaque = "of a value holding a Map, Set, Uint8Array";
    refused(
        V37,
        &main("const u: unknown = new Uint8Array([1]); console.log(String(u));"),
        &format!("conversion to unknown {opaque}"),
    );
    refused(
        V37,
        &main("console.log(JSON.stringify(new Uint8Array([1])));"),
        &format!("JSON.stringify {opaque}"),
    );
    refused(
        V37,
        &main("console.log(new Uint8Array([1]));"),
        &format!("logging {opaque}"),
    );
    refused(
        V37,
        "function k(v: unknown): boolean { return v instanceof Uint8Array; }
         export function main(): void { console.log(String(k(1))); }",
        "instanceof requires an unshadowed supported error class",
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
