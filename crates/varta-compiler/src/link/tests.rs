use super::library::{LIBRARY, library};
use varta_common::{HostOperation, HostScalar, abi, language::LanguageProfile};
use wasmparser::{FuncType, Parser, Payload, TypeRef, ValType};

const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;
const V37: LanguageProfile = LanguageProfile::CheckedF64Utf16V37;

/// One program that reaches every linked family.
const EVERY_FAMILY: &str = r#"
function pick(flag: boolean): string | number { return flag ? "0x1f" : 2.5; }
export function main(): void {
  const x = 0.5;
  console.log(Math.sin(x) + Math.cos(x) + Math.tan(x) + Math.asin(x) + Math.acos(x));
  console.log(Math.atan(x) + Math.atan2(x, 2) + Math.sinh(x) + Math.cosh(x) + Math.tanh(x));
  console.log(Math.asinh(x) + Math.acosh(2) + Math.atanh(x) + Math.cbrt(x) + Math.exp(x));
  console.log(Math.expm1(x) + Math.log(x) + Math.log10(x) + Math.log2(x) + Math.log1p(x));
  console.log(x ** 3 + (7 % 3));
  console.log((255).toString(16) + (1.5).toFixed(2) + (12345).toExponential(1) + (0.5).toPrecision(3));
  console.log(parseInt("ff", 16) + parseFloat("1.5e3"));
  console.log(String(pick(true)) + Number(pick(true)));
  const u: unknown = pick(false);
  const n: bigint | number = 5n;
  console.log(typeof u + String(u === 2.5) + String(Array.isArray(u)) + String(u instanceof RangeError));
  console.log(String(pick(true) < pick(false)) + String(pick(true) == pick(false)) + String(n < 7));
  const s = "Straße";
  console.log(s.charAt(1) + s.charCodeAt(2) + s.toUpperCase() + s.toLowerCase());
}
"#;

fn compile(source: &str, profile: LanguageProfile) -> Vec<u8> {
    crate::compile_typescript_with_profile(source, profile).unwrap()
}
fn imports(bytes: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::ImportSection(reader) = payload.unwrap() {
            for import in reader {
                let import = import.unwrap();
                assert!(matches!(import.ty, TypeRef::Func(_)));
                names.push(format!("{}.{}", import.module, import.name));
            }
        }
    }
    names
}
fn linked_imports(bytes: &[u8]) -> Vec<String> {
    imports(bytes)
        .into_iter()
        .filter(|name| {
            name.strip_prefix("varta.")
                .and_then(HostOperation::from_name)
                .is_some_and(HostOperation::linked)
        })
        .collect()
}
fn memories(bytes: &[u8]) -> Vec<wasmparser::MemoryType> {
    let mut memories = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::MemorySection(reader) = payload.unwrap() {
            memories.extend(reader.into_iter().map(Result::unwrap));
        }
    }
    memories
}
fn instantiate(bytes: &[u8]) {
    let engine = wasmtime::Engine::default();
    let module = wasmtime::Module::new(&engine, bytes).unwrap();
    let mut store = wasmtime::Store::new(&engine, ());
    let mut linker = wasmtime::Linker::new(&engine);
    linker.define_unknown_imports_as_traps(&module).unwrap();
    linker.instantiate(&mut store, &module).unwrap();
}
fn scalar(value: &HostScalar) -> ValType {
    match value {
        HostScalar::I32 => ValType::I32,
        HostScalar::I64 => ValType::I64,
        HostScalar::F64 => ValType::F64,
    }
}

/// The files of the `no_std` value-layout core of varta-common the library
/// compiles (`COMMON_CORE` of `scripts/build-intrinsics.py`).
const COMMON_CORE: &[&str] = &[
    "crates/varta-common/Cargo.toml",
    "crates/varta-common/src/lib.rs",
    "crates/varta-common/src/abi.rs",
    "crates/varta-common/src/abi/static_region.rs",
    "crates/varta-common/src/bigint.rs",
    "crates/varta-common/src/limits.rs",
    "crates/varta-common/src/memory.rs",
    "crates/varta-common/src/text.rs",
    "crates/varta-common/src/value.rs",
    "crates/varta-common/src/value/error.rs",
    "crates/varta-common/src/value/primitive.rs",
    "crates/varta-common/src/value/reference.rs",
    "crates/varta-common/src/value/vector.rs",
    "crates/varta-common/src/value/dictionary.rs",
    "crates/varta-common/src/function.rs",
    "crates/varta-common/src/function/view.rs",
];

#[test]
fn the_fingerprint_covers_every_compiled_common_core_file() {
    // Every common module the core build compiles is fingerprinted: a module
    // declared without `#[cfg(feature = "std")]` (or `#[cfg(test)]`) in
    // lib.rs, value.rs or function.rs must be listed.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../varta-common/src");
    for (file, prefix) in [
        ("lib.rs", ""),
        ("value.rs", "value/"),
        ("function.rs", "function/"),
    ] {
        let source = std::fs::read_to_string(root.join(file)).unwrap();
        let mut gated = false;
        for line in source.lines() {
            if let Some(name) = line
                .strip_prefix("pub mod ")
                .or_else(|| line.strip_prefix("mod "))
                .and_then(|rest| rest.strip_suffix(';'))
                && !gated
            {
                let module = format!("crates/varta-common/src/{prefix}{name}.rs");
                assert!(COMMON_CORE.contains(&module.as_str()), "{module}");
            }
            if !line.starts_with("///") {
                gated = matches!(line, "#[cfg(feature = \"std\")]" | "#[cfg(test)]");
            }
        }
    }
}

/// FNV-1a 64 over `path NUL content NUL` of each intrinsics source file,
/// sorted by path: the algorithm of `scripts/build-intrinsics.py`.
fn source_fingerprint() -> String {
    fn walk(dir: &std::path::Path, root: &std::path::Path, files: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name != "target") {
                    walk(&path, root, files);
                }
            } else {
                files.push(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = root.canonicalize().unwrap();
    let mut files = Vec::new();
    for crate_dir in [
        "crates/varta-bigint",
        "crates/varta-intrinsics",
        "crates/varta-intrinsics-wasm",
    ] {
        walk(&root.join(crate_dir), &root, &mut files);
    }
    // The `no_std` value-layout core of varta-common the library compiles
    // (`COMMON_CORE` of the build script).
    files.extend(COMMON_CORE.iter().map(|file| (*file).to_owned()));
    files.sort();
    let mut value: u64 = 0xcbf2_9ce4_8422_2325;
    for name in files {
        let content = std::fs::read(root.join(&name)).unwrap();
        for chunk in [name.as_bytes(), b"\0", &content, b"\0"] {
            for &byte in chunk {
                value = (value ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
            }
        }
    }
    format!("{value:016x}")
}

#[test]
fn intrinsic_library_matches_its_sources() {
    let metadata = include_str!("intrinsics.json");
    let recorded = metadata
        .split("\"sourceFingerprint\": \"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap();
    assert_eq!(
        recorded,
        source_fingerprint(),
        "intrinsics.wasm is stale: run scripts/build-intrinsics.py"
    );
    assert!(metadata.contains(&format!("\"bytes\": {}", LIBRARY.len())));
}

#[test]
fn intrinsic_library_exports_every_linked_operation_with_its_signature() {
    let library = library().unwrap();
    let mut count = 0;
    for (name, &index) in &library.exports {
        let Some(operation) = HostOperation::from_name(name) else {
            continue;
        };
        assert!(operation.linked(), "{name} is exported but not linked");
        let (params, results) = operation.scalar_signature();
        // A pair of i32 results is exported packed into one i64.
        let expected = if results == [HostScalar::I32, HostScalar::I32] {
            FuncType::new(params.iter().map(scalar), [ValType::I64])
        } else {
            FuncType::new(params.iter().map(scalar), results.iter().map(scalar))
        };
        assert_eq!(library.signature(index), Some(&expected), "{name}");
        assert_eq!(
            super::merge::export_signature(operation),
            expected,
            "{name}"
        );
        assert!(operation.capability().is_none(), "{name} is privileged");
        count += 1;
    }
    let linked = every_linked_operation();
    assert_eq!(count, linked.len());
    for operation in linked {
        assert!(V38.links(operation) && !V37.links(operation));
        assert!(
            library.export(operation.name()).is_some(),
            "{}",
            operation.name()
        );
    }
}

/// Every registered operation the registry marks as linked.
fn every_linked_operation() -> Vec<HostOperation> {
    let source = include_str!("../../../varta-common/src/host.rs");
    let start = source.find("pub fn from_name(").unwrap();
    source[start..]
        .lines()
        .take_while(|line| !line.contains("_ => return None"))
        .filter_map(|line| {
            let rest = line.trim().strip_prefix('"')?;
            HostOperation::from_name(&rest[..rest.find('"')?])
        })
        .filter(|operation| operation.linked())
        .collect()
}

/// The declared layout of a module: its extension and intrinsic region.
fn region(bytes: &[u8]) -> abi::StaticRegion {
    let mut region = abi::StaticRegion::SINGLE_PAGE;
    let mut intrinsics = None;
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CustomSection(section) = payload.unwrap() {
            if section.name() == abi::STATIC_SECTION {
                region = abi::StaticRegion::read_section(section.data()).unwrap();
            } else if section.name() == abi::INTRINSIC_SECTION {
                intrinsics = Some(section.data().to_vec());
            }
        }
    }
    match intrinsics {
        Some(data) => region.read_intrinsic_section(&data).unwrap(),
        None => region,
    }
}
/// (offset, length) of every active data segment.
fn segments(bytes: &[u8]) -> Vec<(usize, usize)> {
    let mut segments = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::DataSection(reader) = payload.unwrap() {
            for data in reader {
                let data = data.unwrap();
                let wasmparser::DataKind::Active {
                    memory_index: 0,
                    offset_expr,
                } = data.kind
                else {
                    panic!("only active memory 0 segments");
                };
                let Ok(wasmparser::Operator::I32Const { value }) =
                    offset_expr.get_operators_reader().read()
                else {
                    panic!("constant offsets");
                };
                segments.push((value as u32 as usize, data.data.len()));
            }
        }
    }
    segments
}

#[test]
fn intrinsic_library_fits_the_wasm_limits() {
    // Linked code counts against the profile's own structural limits (v38:
    // the Wasm limits of `varta_common::limits::WASM`).
    assert!(library().unwrap().functions.len() < varta_common::limits::WASM.wasm_functions / 2);
}

#[test]
fn v38_modules_import_no_linked_intrinsic_and_instantiate() {
    let v38 = compile(EVERY_FAMILY, V38);
    assert_eq!(linked_imports(&v38), Vec::<String>::new());
    let remaining = imports(&v38);
    assert!(remaining.iter().all(|name| name.starts_with("varta.")));
    assert!(remaining.iter().any(|name| name == "varta.log_string16"));
    // One memory: the library lives in the intrinsic region of memory 0.
    let memories = memories(&v38);
    assert_eq!(memories.len(), 1, "linked code needs no second memory");
    let region = region(&v38);
    assert!(region.intrinsic_pages() > 0);
    assert!(memories[0].initial >= region.minimum_pages());
    let segments = segments(&v38);
    for &(offset, len) in &segments {
        assert!(
            region.contains(offset, len) || region.intrinsic_contains(offset, len),
            "segment [{offset}, +{len}) outside the static and intrinsic regions"
        );
    }
    let library_data: Vec<_> = segments
        .iter()
        .filter(|&&(offset, len)| region.intrinsic_contains(offset, len))
        .collect();
    assert!(!library_data.is_empty(), "library data in the region");
    // The library's data follows the region's shadow stack.
    let data_start = region.intrinsic_base() + abi::INTRINSIC_STACK_BYTES;
    assert!(
        library_data
            .iter()
            .all(|&&(offset, _)| offset >= data_start)
    );
    wasmparser::Validator::new().validate_all(&v38).unwrap();
    instantiate(&v38);
    // The same program under v37 imports the host implementations.
    let v37 = compile(EVERY_FAMILY, V37);
    assert!(
        linked_imports(&v37).len() >= 30,
        "{:?}",
        linked_imports(&v37)
    );
    assert_eq!(super::link(v37.clone(), V37, None).unwrap(), v37);
    assert_eq!(super::tests::region(&v37), abi::StaticRegion::SINGLE_PAGE);
}

#[test]
fn linking_is_deterministic_and_links_only_what_a_program_uses() {
    let first = compile(EVERY_FAMILY, V38);
    assert_eq!(first, compile(EVERY_FAMILY, V38));
    let sine = compile(
        "export function main(): void { console.log(Math.sin(0.5)); }",
        V38,
    );
    let plain = compile("export function main(): void { console.log(0.5); }", V38);
    assert_eq!(
        region(&plain).intrinsic_pages(),
        0,
        "no intrinsic, no region"
    );
    assert!(sine.len() > plain.len());
    assert!(sine.len() < first.len());
    assert!(
        sine.len() - plain.len() < 16 * 1024,
        "Math.sin links {} bytes",
        sine.len() - plain.len()
    );
    // The region holds the stack and the reached data only.
    let stack_pages = (abi::INTRINSIC_STACK_BYTES / abi::WASM_PAGE_BYTES) as u32;
    assert!(region(&sine).intrinsic_pages() <= stack_pages + 1);
    assert!(region(&sine).intrinsic_pages() <= region(&first).intrinsic_pages());
}

#[test]
fn the_intrinsic_region_follows_the_static_extension() {
    // Enough literal text to spill into the static extension.
    let words: Vec<String> = (0..4_000)
        .map(|i| format!("\"word number {i:05}\""))
        .collect();
    let source = format!(
        "export function main(): void {{ const words: string[] = [{}]; \
         console.log(words[1].toUpperCase() + (2.5).toFixed(1)); }}",
        words.join(", ")
    );
    let bytes = compile(&source, V38);
    let region = region(&bytes);
    assert!(region.extension_pages() > 0 && region.intrinsic_pages() > 0);
    assert_eq!(
        region.intrinsic_base(),
        abi::OWNED_STRING_BASE + region.extension_bytes()
    );
    wasmparser::Validator::new().validate_all(&bytes).unwrap();
    instantiate(&bytes);
}

#[test]
fn every_linked_stack_pointer_write_is_checked_against_the_region() {
    let bytes = compile(
        "export function main(): void { console.log((1.5).toFixed(2)); }",
        V38,
    );
    let base = region(&bytes).intrinsic_base();
    let mut checks = 0;
    for payload in Parser::new(0).parse_all(&bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            let operators: Vec<_> = body
                .get_operators_reader()
                .unwrap()
                .into_iter()
                .map(Result::unwrap)
                .collect();
            if let [
                wasmparser::Operator::LocalGet { local_index: 0 },
                wasmparser::Operator::I32Const { value },
                wasmparser::Operator::I32LtU,
                ..,
            ] = operators.as_slice()
                && *value as u32 as usize == base
            {
                checks += 1;
            }
        }
    }
    assert_eq!(checks, 1, "one checked stack-pointer setter");
}

/// Code each linked conversion, comparison and value18 reader brings into a
/// module on its own (the measurement of the development plan):
/// `cargo test -p varta-compiler --lib linked_closure_sizes -- --ignored --nocapture`.
#[test]
#[ignore]
fn linked_closure_sizes() {
    let library = library().unwrap();
    for name in [
        "primitive_to_string",
        "primitive_to_number",
        "primitive_to_string_v6",
        "primitive_to_number_v6",
        "primitive_compare",
        "primitive_equal",
        "value_typeof_v11",
        "value_truthy_v11",
        "value_nullish_v11",
        "value_is_array_v11",
        "value_strict_equal_v11",
        "value_nullish_equal_v11",
        "value_is_error_class_v7",
    ] {
        let root = library.export(name).unwrap();
        let closure = library.closure(&[root]).unwrap();
        let code: usize = closure
            .functions
            .iter()
            .filter_map(|&f| library.defined(f))
            .map(|f| f.body.range().len())
            .sum();
        println!(
            "{name}: {} functions, {code} code bytes, {} segments",
            closure.functions.len(),
            closure.segments.len()
        );
    }
}
