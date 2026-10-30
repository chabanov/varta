//! V38 web bytes (`web_bytes`) without the network: `Blob`, `ArrayBuffer`
//! and `FormData`, the typed `globalThis`, `Promise.race` over promises of
//! different types and strict checked conversions to unions of record
//! shapes. Expected logs were produced by Node 24 running the same programs
//! (`web_bytes_cases/*.ts` with `await main();` appended), except the lines
//! where Varta's checked conversions throw (TypeScript's `as` is unchecked).
//! Every program runs under the production budgets. The byte bodies of
//! `fetch` are in `net_tests.rs`; `process.env` grants in the CLI smoke tests.
use super::*;
use varta_common::language::LanguageProfile;
use varta_common::scheduler::TIMER_CAPABILITY;
use varta_common::{CapabilityGrant, CapabilityValue};

const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;

fn compile(source: &str) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, V38)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}

fn run(source: &str, grants: &[CapabilityGrant], expected: &[&str]) {
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compile(source), grants)
        .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    let expected: Vec<String> = expected.iter().map(|s| format!("varta log: {s}")).collect();
    assert_eq!(output.lines, expected, "{source}");
}

#[test]
fn blobs_buffers_and_forms_match_node() {
    run(
        include_str!("web_bytes_cases/blob_form.ts"),
        &[],
        &[
            "12 text/plain",
            "héllo world",
            "3 195,169,108 3 0",
            "12 72 2",
            "27 []",
            "true 2 name,file,raw,name",
            "blob 3 text/plain",
            "1 only false true",
            r#"multipart/form-data "--<b>\r\nContent-Disposition: form-data; name=\"name\"\r\n\r\nonly\r\n--<b>\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a%22b.txt\"\r\nContent-Type: text/plain\r\n\r\nabc\r\n--<b>--\r\n""#,
            "3 text/plain;charset=utf-8",
            "€ null",
            "application/json {\"a\":1}",
            "0,0 true",
        ],
    );
}

/// The multipart boundary is deterministic: the first counter value that
/// occurs in no part, so equal forms serialize to equal bytes.
#[test]
fn multipart_boundaries_are_deterministic() {
    run(
        "async function body(text: string): Promise<string> {
           const form = new FormData();
           form.append('a', text);
           const response = new Response(form);
           return response.headers.get('content-type') ?? '';
         }
         export async function main(): Promise<void> {
           const plain = await body('x');
           console.log(plain);
           const colliding = await body('----formdata-varta-00000000000');
           console.log(colliding);
         }",
        &[],
        &[
            "multipart/form-data; boundary=----formdata-varta-00000000000",
            "multipart/form-data; boundary=----formdata-varta-00000000001",
        ],
    );
}

#[test]
fn the_typed_global_object_matches_node() {
    run(
        include_str!("web_bytes_cases/global.ts"),
        &[],
        &["36 4 5", "4", "fetch", "function a%20b", "undefined", "36"],
    );
}

/// `globalThis` members are typed: an unknown member is a compile error,
/// never `any`; earlier profiles do not know `globalThis` at all.
#[test]
fn unknown_global_members_are_compile_errors() {
    for source in [
        "export function main(): void { console.log(String(globalThis.window)); }",
        "export function main(): void { const p = globalThis.process; console.log(String(p.argv)); }",
        "export function main(): void { globalThis.fetch = (): Promise<Response> => Promise.reject(new Error('x')); }",
    ] {
        assert!(
            varta_compiler::compile_typescript_with_profile(source, V38).is_err(),
            "{source}"
        );
    }
}

#[test]
fn mixed_races_match_node() {
    let timer = CapabilityGrant {
        id: TIMER_CAPABILITY.into(),
        value: CapabilityValue::Boolean(true),
    };
    run(
        include_str!("web_bytes_cases/race.ts"),
        &[timer],
        &["timed out", "false data", "boom", "1 a,b"],
    );
}

#[test]
fn record_unions_convert_by_strict_matching() {
    run(
        include_str!("web_bytes_cases/unions.ts"),
        &[],
        &[
            "list 1",
            "one null -",
            "one down slow",
            // Node: "one down -" (an unknown key is accepted by `as`).
            "TypeError type assertion to Answer failed",
            // Node: "list 0" (both members' keys, unchecked).
            "TypeError type assertion to Answer failed",
            // Node: "one null -" (an array asserted to an object type).
            "TypeError type assertion to Answer failed",
            "no reason",
            "reason r",
            "no reason",
            "left 1",
            "right",
            // Node: "right"; `{}` matches both members strictly.
            "TypeError checked conversion to { x } | { y } is ambiguous: the value matches more than one member",
            "one up -",
            "one kept -",
        ],
    );
}

/// A record holding a `Uint8Array` may enter `unknown`; its JSON-object view
/// throws `TypeError` for the bytes, while `Blob` and `ArrayBuffer` fields
/// pass. A `Uint8Array` itself still cannot be held by `unknown`.
#[test]
fn bytes_inside_unknown_records_are_checked() {
    run(
        "type BinaryInput = Blob | ArrayBuffer | ArrayBufferView | string;
         interface Upload { photo: BinaryInput | Uint8Array; slots?: string }
         function names(body: object): string {
           try {
             return Object.keys(body as Record<string, unknown>).join(',');
           } catch (error) {
             return error instanceof TypeError ? `TypeError: ${error.message}` : 'other';
           }
         }
         export function main(): void {
           const blob: Upload = { photo: new Blob(['x']), slots: '{}' };
           console.log(names(blob));
           const bytes: Upload = { photo: new Uint8Array(2) };
           console.log(names(bytes));
         }",
        &[],
        &[
            "photo,slots",
            "TypeError: a Uint8Array held by unknown cannot be read as a JSON value",
        ],
    );
    let error = varta_compiler::compile_typescript_with_profile(
        "export function main(): void { const u: unknown = new Uint8Array(1); console.log(typeof u); }",
        V38,
    )
    .unwrap_err();
    assert!(error.to_string().contains("Uint8Array"), "{error}");
}
