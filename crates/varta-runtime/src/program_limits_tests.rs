//! V38 (`program_limits`) structural limits come from the profile registry
//! (`varta_common::limits`): synthetic programs over every frozen ceiling
//! compile, validate in Wasmtime and run under production budgets in v38,
//! while v37 keeps rejecting each with its frozen diagnostic.
use super::*;
use varta_common::language::LanguageProfile;
use varta_common::limits;

const V37: LanguageProfile = LanguageProfile::CheckedF64Utf16V37;
const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;

/// Compile under v38, instantiate in Wasmtime and run with production
/// budgets; v37 rejects the same source with `frozen`.
fn over_the_frozen_limit(source: &str, frozen: &str, expected: &[String]) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, V38)
        .unwrap_or_else(|e| panic!("v38 rejected: {e}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    let run = VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap();
    assert_eq!(run.lines, expected);
    let error = varta_compiler::compile_typescript_with_profile(source, V37)
        .err()
        .unwrap_or_else(|| panic!("v37 accepted a program over its limit: {frozen}"));
    assert!(error.to_string().contains(frozen), "{error}");
    bytes
}

fn log(line: impl std::fmt::Display) -> String {
    format!("varta log: {line}")
}

/// Distinct literal unions `"a{i}" | "b{i}"`, one per index.
fn literal_unions(count: usize) -> String {
    (0..count)
        .map(|i| format!("type L{i} = \"a{i}\" | \"b{i}\";\n"))
        .collect()
}

#[test]
fn user_functions_beyond_256_are_admitted() {
    const COUNT: usize = 600;
    let mut source: String = (0..COUNT)
        .map(|i| format!("function f{i}(n: number): number {{ return n + {i}; }}\n"))
        .collect();
    source.push_str("export function main(): void {\n  let total = 0;\n");
    for i in 0..COUNT {
        source.push_str(&format!("  total = f{i}(total);\n"));
    }
    source.push_str("  console.log(String(total));\n}\n");
    let total: usize = (0..COUNT).sum();
    let bytes = over_the_frozen_limit(&source, "too many functions across modules", &[log(total)]);
    const { assert!(abi::MAX_USER_FUNCTIONS < COUNT && COUNT < limits::WASM.user_functions) };
    // The function section holds more than the frozen 270 entries.
    let defined = wasmparser::Parser::new(0)
        .parse_all(&bytes)
        .find_map(|payload| match payload.unwrap() {
            wasmparser::Payload::FunctionSection(reader) => Some(reader.count() as usize),
            _ => None,
        })
        .unwrap();
    assert!(defined > limits::FROZEN.wasm_functions, "{defined}");
}

#[test]
fn record_shapes_unions_arrays_and_function_types_beyond_128_are_admitted() {
    const COUNT: usize = 200;
    let mut source = literal_unions(COUNT);
    for i in 0..COUNT {
        source.push_str(&format!(
            "interface S{i} {{ k{i}: L{i}; n: number }}\n\
             function make{i}(n: number): S{i} {{ return {{ k{i}: \"b{i}\", n: n }}; }}\n\
             function pick{i}(f: (n: number) => S{i}, items: S{i}[]): number {{ \
             return f(items.length).n + items[0].n; }}\n"
        ));
    }
    source.push_str("export function main(): void {\n  let total = 0;\n  let keys = \"\";\n");
    for i in 0..COUNT {
        source.push_str(&format!(
            "  {{ const s = make{i}({i}); total = total + pick{i}(make{i}, [s]); \
             if ({i} % 50 === 0) {{ keys = keys + s.k{i}; }} }}\n"
        ));
    }
    source.push_str("  console.log(String(total) + \":\" + keys);\n}\n");
    // pick(i) = 1 (the array length) + i.
    let total: usize = (0..COUNT).map(|i| 1 + i).sum();
    over_the_frozen_limit(
        &source,
        "object shape count exceeds limit",
        &[log(format!("{total}:b0b50b100b150"))],
    );
    for (frozen, wasm) in [
        (limits::FROZEN.record_shapes, limits::WASM.record_shapes),
        (limits::FROZEN.union_types, limits::WASM.union_types),
        (limits::FROZEN.array_types, limits::WASM.array_types),
        (limits::FROZEN.function_types, limits::WASM.function_types),
    ] {
        assert!(frozen < COUNT && COUNT < wasm);
    }
}

#[test]
fn record_shapes_beyond_128_are_admitted_without_unions() {
    const COUNT: usize = 300;
    let mut source = String::new();
    for i in 0..COUNT {
        source.push_str(&format!(
            "interface R{i} {{ v{i}: number }}\nfunction r{i}(): R{i} {{ return {{ v{i}: {i} }}; }}\n"
        ));
    }
    source.push_str("export function main(): void {\n  let total = 0;\n");
    for i in 0..COUNT {
        source.push_str(&format!("  total = total + r{i}().v{i};\n"));
    }
    source.push_str("  console.log(String(total));\n}\n");
    let total: usize = (0..COUNT).sum();
    over_the_frozen_limit(&source, "object shape count exceeds limit", &[log(total)]);
}

#[test]
fn promise_types_beyond_64_are_admitted() {
    const COUNT: usize = 70;
    // Distinct record results: promise types are interned by value type.
    let mut source = String::new();
    for i in 0..COUNT {
        source.push_str(&format!(
            "interface P{i} {{ v: string; w{i}: number }}\n\
             async function p{i}(): Promise<P{i}> {{ return {{ v: \"a{i}\", w{i}: {i} }}; }}\n"
        ));
    }
    // Ten awaits per async function keep every resume body small.
    for chunk in 0..COUNT / 10 {
        source.push_str(&format!(
            "async function chunk{chunk}(): Promise<string> {{\n  let text = \"\";\n"
        ));
        for i in chunk * 10..chunk * 10 + 10 {
            source.push_str(&format!(
                "  {{ const r = await p{i}(); if ({i} % 30 === 0) {{ text = text + r.v; }} }}\n"
            ));
        }
        source.push_str("  return text;\n}\n");
    }
    source.push_str("export async function main(): Promise<void> {\n  let text = \"\";\n");
    for chunk in 0..COUNT / 10 {
        source.push_str(&format!(
            "  {{ const part = await chunk{chunk}(); text = text + part; }}\n"
        ));
    }
    source.push_str("  console.log(text);\n}\n");
    // Each promise type also owns private record shapes, so v37 reaches its
    // shape limit at the 64th promise type, just before its promise limit.
    over_the_frozen_limit(
        &source,
        "object shape count exceeds limit",
        &[log("a0a30a60")],
    );
    const { assert!(limits::FROZEN.promise_types < COUNT && COUNT < limits::WASM.promise_types) };
}

#[test]
fn generator_types_beyond_64_are_admitted() {
    const COUNT: usize = 70;
    let mut source = literal_unions(COUNT);
    for i in 0..COUNT {
        source.push_str(&format!(
            "function* g{i}(): Generator<L{i}, void, undefined> {{ yield \"a{i}\"; }}\n"
        ));
    }
    source.push_str("export function main(): void {\n  let count = 0;\n");
    for i in 0..COUNT {
        source.push_str(&format!(
            "  for (const v of g{i}()) {{ if (v === \"a{i}\") {{ count++; }} }}\n"
        ));
    }
    source.push_str("  console.log(String(count));\n}\n");
    over_the_frozen_limit(&source, "generator type count exceeds limit", &[log(COUNT)]);
}

#[test]
fn function_bodies_beyond_64_kib_are_admitted() {
    const STATEMENTS: usize = 6_000;
    let mut source = String::from("export function main(): void {\n  let t = 0;\n");
    for i in 0..STATEMENTS {
        source.push_str(&format!("  t = t + {};\n", i % 7));
    }
    source.push_str("  console.log(String(t));\n}\n");
    let total: usize = (0..STATEMENTS).map(|i| i % 7).sum();
    let bytes = over_the_frozen_limit(
        &source,
        "function body exceeds compilation budget",
        &[log(total)],
    );
    let largest = wasmparser::Parser::new(0)
        .parse_all(&bytes)
        .filter_map(|payload| match payload.unwrap() {
            wasmparser::Payload::CodeSectionEntry(body) => Some(body.range().len()),
            _ => None,
        })
        .max()
        .unwrap();
    assert!(largest > limits::FROZEN.function_body_bytes, "{largest}");
    assert!(largest < limits::WASM.function_body_bytes);
}

#[test]
fn wasm_signatures_beyond_256_are_admitted() {
    // Nine parameters, each number or string, give 512 distinct signatures.
    const COUNT: usize = 300;
    let mut source = String::new();
    let mut calls = String::new();
    for i in 0..COUNT {
        let params: Vec<String> = (0..9)
            .map(|bit| {
                format!(
                    "p{bit}: {}",
                    if i & (1 << bit) == 0 {
                        "number"
                    } else {
                        "string"
                    }
                )
            })
            .collect();
        let args: Vec<&str> = (0..9)
            .map(|bit| if i & (1 << bit) == 0 { "1" } else { "\"s\"" })
            .collect();
        source.push_str(&format!(
            "function s{i}({}): number {{ return {i}; }}\n",
            params.join(", ")
        ));
        calls.push_str(&format!("  total = total + s{i}({});\n", args.join(", ")));
    }
    source.push_str(&format!(
        "export function main(): void {{\n  let total = 0;\n{calls}  console.log(String(total));\n}}\n"
    ));
    let total: usize = (0..COUNT).sum();
    let bytes = over_the_frozen_limit(&source, "too many functions across modules", &[log(total)]);
    let types = wasmparser::Parser::new(0)
        .parse_all(&bytes)
        .find_map(|payload| match payload.unwrap() {
            wasmparser::Payload::TypeSection(reader) => Some(reader.count() as usize),
            _ => None,
        })
        .unwrap();
    assert!(types > limits::FROZEN.wasm_signatures, "{types}");
}

/// Function values and closures whose function index lies beyond the frozen
/// 256: the value17 readers (logging, JSON, `unknown`) and the function
/// catalog accept them within the v38 limits.
#[test]
fn function_values_beyond_the_frozen_function_index_are_read() {
    const COUNT: usize = 300;
    let mut source: String = (0..COUNT)
        .map(|i| format!("function f{i}(n: number): number {{ return n + {i}; }}\n"))
        .collect();
    source.push_str(
        "interface Holder { cb: (n: number) => number; n: number }\n\
         export function main(): void {\n  let total = 0;\n",
    );
    for i in 0..COUNT {
        source.push_str(&format!("  total = f{i}(total);\n"));
    }
    source.push_str(&format!(
        "  const offset = total;\n\
         const late = (n: number): number => n + offset;\n\
         const holder: Holder = {{ cb: f{last}, n: 1 }};\n\
         const value: unknown = holder.cb;\n\
         console.log(JSON.stringify(holder) + \":\" + String(holder.cb(1)) + \":\" + typeof value);\n\
         console.log(holder.cb);\n\
         console.log(String(late(1)));\n}}\n",
        last = COUNT - 1
    ));
    let total: usize = (0..COUNT).sum();
    over_the_frozen_limit(
        &source,
        "too many functions across modules",
        &[
            log(format!("{{\"n\":1}}:{}:function", COUNT)),
            log("[Function (anonymous)]"),
            log(total + 1),
        ],
    );
}

/// A v38 module over a frozen limit fails before any guest effect when its
/// profile is relabelled as v37: at its host ABI section, and with that
/// downgraded as well, at the frozen structural limits of the runtime.
#[test]
fn a_v38_module_over_the_frozen_limits_fails_as_v37_before_effects() {
    use crate::language_tests::{host_abi, replace};
    use varta_common::language::{LanguageContract, SCHEMA_VERSION};
    let mut source = String::new();
    for i in 0..300 {
        source.push_str(&format!(
            "function f{i}(n: number): number {{ return n + {i}; }}\n"
        ));
    }
    source.push_str("export function main(): void { let t = 0;\n");
    for i in 0..300 {
        source.push_str(&format!("t = f{i}(t);\n"));
    }
    source.push_str("console.log(String(t)); }\n");
    let bytes = varta_compiler::compile_typescript_with_profile(&source, V38).unwrap();
    let v37 = LanguageContract {
        schema_version: SCHEMA_VERSION,
        profile: V37,
    }
    .to_bytes()
    .unwrap();
    let relabelled = replace(&bytes, &v37);
    let runtime = VartaRuntime::new();
    assert!(matches!(
        runtime.validate_module(&relabelled),
        Err(RuntimeError::Language(_))
    ));
    let downgraded = host_abi(&relabelled, Some(abi::NET_REQUEST_VERSION));
    let error = runtime.validate_module(&downgraded).unwrap_err();
    assert!(error.to_string().contains("too many functions"), "{error}");
    assert!(runtime.run_wasm_bytes(&downgraded, &[]).is_err());
    assert_eq!(
        runtime.run_wasm_bytes(&bytes, &[]).unwrap().lines,
        [log((0..300).sum::<usize>())]
    );
}
