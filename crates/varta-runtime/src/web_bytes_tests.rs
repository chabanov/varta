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

#[test]
fn typeof_of_engine_globals_matches_node() {
    run(
        include_str!("web_bytes_cases/typeof_globals.ts"),
        &[],
        &[
            "function function function function function function function",
            "function function function function function",
            "object object object object object object",
            "image/png",
            "image/*",
            "application/octet-stream",
            "true /x",
            "string number function",
        ],
    );
}

/// A module binding of a global's name is the module's own value, as in
/// JavaScript (Node 24 prints the same lines).
#[test]
fn module_bindings_shadow_engine_globals_in_typeof() {
    run(
        "const Headers = 3;
         function fetch(): string { return 'mine'; }
         export function main(): void {
           console.log(`${typeof Headers} ${typeof fetch} ${fetch()}`);
         }",
        &[],
        &["number function mine"],
    );
}

/// The folded `typeof` evaluates nothing and loads no prelude for its
/// operand: a prelude class, a prelude function and a compiler intrinsic of
/// the same kind compile to the same bytes.
#[test]
fn folded_typeof_loads_no_prelude() {
    let program = |name: &str| {
        compile(&format!(
            "export function main(): void {{ console.log(typeof {name}); }}"
        ))
    };
    let intrinsic = program("Promise");
    for name in ["Blob", "Headers", "fetch", "Map", "Uint8Array", "URL"] {
        assert_eq!(program(name), intrinsic, "{name}");
    }
    let object = program("Math");
    for name in ["globalThis", "crypto", "process"] {
        assert_eq!(program(name), object, "{name}");
    }
    // `instanceof` still needs the class, which loads its prelude as usual.
    assert_ne!(
        compile(
            "export function main(): void {
               const body: string | Blob = 'x';
               console.log(String(typeof Blob !== 'undefined' && body instanceof Blob));
             }"
        )
        .len(),
        intrinsic.len()
    );
}

/// Only engine globals fold: an unknown identifier, a type-only name, an
/// engine-created class with no global binding (Node 24: `undefined`) and a
/// global v38 does not provide stay compile errors, and v37 keeps rejecting
/// `typeof` of a prelude class.
#[test]
fn typeof_of_other_identifiers_stays_rejected() {
    let source =
        |name: &str| format!("export function main(): void {{ console.log(typeof {name}); }}");
    for name in [
        "NotAGlobal",
        "HeadersInit",
        "ReadonlyMap",
        "RegExpExecArray",
        "ViewEvent",
        "Symbol",
        "window",
    ] {
        let error = varta_compiler::compile_typescript_with_profile(&source(name), V38)
            .expect_err(name)
            .to_string();
        assert!(
            error.contains(&format!("unknown identifier: {name}")),
            "{error}"
        );
    }
    for name in ["Blob", "Headers", "Map", "Math", "crypto"] {
        let error = varta_compiler::compile_typescript_with_profile(
            &source(name),
            LanguageProfile::CheckedF64Utf16V37,
        )
        .expect_err(name)
        .to_string();
        assert!(
            error.contains(&format!("unknown identifier: {name}")),
            "{error}"
        );
    }
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

/// `process.env` lists only exactly granted names and never enumerates the
/// environment for a wildcard grant; a static key reads
/// through `env.get`. The names are unset in every test environment; the CLI
/// smoke test covers set variables.
#[test]
fn process_env_lists_exact_grants_only() {
    let source = "export function main(): void {
       const env = (globalThis as { process?: { env?: Record<string, string | undefined> } }).process?.env ?? {};
       console.log(`${Object.keys(env).length} ${globalThis.process.env.VARTA_TEST_UNSET_A}`);
     }";
    let env = |patterns: &[&str]| CapabilityGrant {
        id: "env.get".into(),
        value: CapabilityValue::PathGlobs(patterns.iter().map(|p| (*p).into()).collect()),
    };
    for grants in [
        vec![env(&["VARTA_TEST_*"])],
        vec![env(&["VARTA_TEST_UNSET_A"])],
        vec![env(&["VARTA_TEST_UNSET_?"])],
    ] {
        run(source, &grants, &["0 undefined"]);
    }
    // Without a grant the static read is the ordinary recoverable `env.get`
    // denial (logged), checked before any effect.
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compile(source), &[env(&["OTHER_NAME"])])
        .unwrap();
    assert_eq!(
        output.lines,
        [
            "varta log: <capability denied: env> (VARTA_TEST_UNSET_A)",
            "varta log: 0 undefined",
        ]
    );
}

#[test]
fn merged_classes_take_their_members_from_object_assign() {
    run(
        include_str!("web_bytes_cases/merged_class.ts"),
        &[],
        &[
            "GET https://api.test/agents",
            "GET https://api.test/runs/r1",
            "client of https://api.test 2 https://api.test",
            "true",
        ],
    );
}

/// A merged member that the constructor cannot prove provided, with a
/// compatible type, before any other use of `this` is a compile error that
/// names it; v37 keeps rejecting class and interface merging.
#[test]
fn merged_class_diagnostics_name_the_members() {
    let program = |class: &str| {
        format!(
            "interface R {{ a: number; b: string }} interface C extends R {{}} {class}
             export function main(): void {{ const c = new C(); console.log(String(c.a)); }}"
        )
    };
    for (class, expected) in [
        (
            "class C { constructor() { Object.assign(this, { a: 1 }); } }",
            "(missing field b)",
        ),
        (
            "class C { constructor() { Object.assign(this, { a: 'x', b: 'y' }); } }",
            "(field a: expected number, found string)",
        ),
        (
            "class C { constructor() { } }",
            "must provide its members a, b with one top-level Object.assign(this, …)",
        ),
        (
            "class C { constructor() { Object.assign(this, { a: 1, b: 'y' }); Object.assign(this, { a: 2, b: 'z' }); } }",
            "more than one Object.assign(this, …)",
        ),
        (
            "class C { m(): number { return 1; } constructor() { this.m(); Object.assign(this, { a: 1, b: 'y' }); } }",
            "this may only access declared fields",
        ),
        (
            "class C { constructor() { const x = this.a; Object.assign(this, { a: 1, b: 'y' }); } }",
            "",
        ),
        (
            "class C { a = 1; constructor() { Object.assign(this, { a: 1, b: 'y' }); } }",
            "merged interface member a also declared by class C",
        ),
    ] {
        let source = program(class);
        let error = varta_compiler::compile_typescript_with_profile(&source, V38)
            .expect_err(class)
            .to_string();
        assert!(error.contains(expected), "{class}: {error}");
        let error = varta_compiler::compile_typescript_with_profile(
            &source,
            LanguageProfile::CheckedF64Utf16V37,
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("declaration merging"), "{class}: {error}");
    }
}

/// Member order never decides a strict record-union conversion: both orders
/// select the same single match and throw the same catchable ambiguity.
#[test]
fn strict_union_conversions_ignore_member_order() {
    for union in ["Left | Right", "Right | Left"] {
        let source = format!(
            "interface Left {{ x?: number; shared?: string }}
             interface Right {{ y?: number; shared?: string }}
             function convert(text: string): string {{
               try {{
                 const value = JSON.parse(text) as {union};
                 return 'x' in value ? 'left' : 'y' in value ? 'right' : 'neither';
               }} catch (error) {{
                 return error instanceof TypeError ? 'TypeError' : 'other';
               }}
             }}
             export function main(): void {{
               console.log(convert('{{\"x\":1}}'));
               console.log(convert('{{\"y\":2,\"shared\":\"s\"}}'));
               console.log(convert('{{\"shared\":\"s\"}}'));
               console.log(convert('{{\"x\":1,\"y\":2}}'));
             }}"
        );
        run(&source, &[], &["left", "right", "TypeError", "TypeError"]);
    }
}
