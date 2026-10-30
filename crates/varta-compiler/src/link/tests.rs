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
    for crate_dir in ["crates/varta-intrinsics", "crates/varta-intrinsics-wasm"] {
        walk(&root.join(crate_dir), &root, &mut files);
    }
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
        let expected = FuncType::new(params.iter().map(scalar), results.iter().map(scalar));
        assert_eq!(library.signature(index), Some(&expected), "{name}");
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

#[test]
fn intrinsic_library_declares_its_memory_bound() {
    let memory = library().unwrap().memory;
    assert!(!memory.memory64 && !memory.shared);
    assert_eq!(
        memory.maximum,
        Some((abi::MAX_INTRINSIC_MEMORY_BYTES / abi::WASM_PAGE_BYTES) as u64)
    );
    assert!(library().unwrap().functions.len() <= abi::MAX_LINKED_INTRINSIC_FUNCTIONS);
}

#[test]
fn v38_modules_import_no_linked_intrinsic_and_instantiate() {
    let v38 = compile(EVERY_FAMILY, V38);
    assert_eq!(linked_imports(&v38), Vec::<String>::new());
    let remaining = imports(&v38);
    assert!(remaining.iter().all(|name| name.starts_with("varta.")));
    assert!(remaining.iter().any(|name| name == "varta.log_string16"));
    let memories = memories(&v38);
    assert_eq!(
        memories.len(),
        2,
        "guest memory and private intrinsic memory"
    );
    assert_eq!(
        memories[1].maximum,
        Some((abi::MAX_INTRINSIC_MEMORY_BYTES / abi::WASM_PAGE_BYTES) as u64)
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
    assert_eq!(super::link(v37.clone(), V37).unwrap(), v37);
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
    assert_eq!(memories(&plain).len(), 1, "no intrinsic, no private memory");
    assert!(sine.len() > plain.len());
    assert!(sine.len() < first.len());
    assert!(
        sine.len() - plain.len() < 16 * 1024,
        "Math.sin links {} bytes",
        sine.len() - plain.len()
    );
}
