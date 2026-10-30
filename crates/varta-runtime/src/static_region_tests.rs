//! The v38 multi-page static region (host ABI 34, `program_limits`): static
//! data past the first page lives in an extension after the root stack and
//! the managed heap follows it. Earlier profiles keep the single page.
use super::*;
use crate::contract_tests::rewrite;
use crate::language_tests::{host_abi, replace};
use varta_common::language::{LanguageContract, LanguageProfile, SCHEMA_VERSION};

/// The first literal leaves only a few bytes of the first page, so the other
/// literals, every record descriptor and the JSON schema live in the
/// extension.
const FIRST_UNITS: usize = 32_750;
const UNITS: usize = 6_000;

/// Literals filling the first page and spilling into the extension, record
/// descriptors and a checked JSON schema in the extension, slices sharing
/// static bytes, records pointing into the extension and enough allocation to
/// collect while they are live.
fn source() -> String {
    let a = "a".repeat(FIRST_UNITS);
    let b = "b".repeat(UNITS);
    let c = format!("{}tail", "c".repeat(UNITS - 4));
    format!(
        r#"
interface Wide {{ first: string; second: string; }}
export function main(): void {{
  const a = "{a}";
  const b = "{b}";
  const c = "{c}";
  const wide: Wide = {{ first: a.slice(0, 3), second: c.slice(c.length - 4) }};
  const box = {{ text: c, other: b }};
  console.log(JSON.stringify(wide));
  let last = "";
  for (let i = 37; i < 40; i++) {{
    last = b.slice(0, 5000) + String(i);
  }}
  console.log(String(a.length + box.text.length + box.other.length));
  console.log(last.slice(last.length - 4) + ":" + box.text.slice(0, 2));
  const parsed = JSON.parse('{{"first":"x","second":"y"}}') as Wide;
  console.log(parsed.first + parsed.second + ":" + String(Object.keys(box).length));
}}
"#
    )
}

fn custom(data: &[u8], name: &str) -> bool {
    data.first() == Some(&(name.len() as u8))
        && data.get(1..1 + name.len()) == Some(name.as_bytes())
}

/// Replace (or with `None`, remove) the static section; `extra` appends a
/// second copy after the language section.
fn static_section(bytes: &[u8], pages: Option<u32>, extra: Option<u32>) -> Vec<u8> {
    let section = |pages: u32| {
        let mut data = vec![abi::STATIC_SECTION.len() as u8];
        data.extend(abi::STATIC_SECTION.as_bytes());
        data.extend(pages.to_le_bytes());
        data
    };
    rewrite(bytes, |id, data| {
        if id == 0 && custom(data, abi::STATIC_SECTION) {
            pages.map(section).into_iter().collect()
        } else if id == 0 && custom(data, varta_common::language::SECTION) {
            std::iter::once(data.to_vec())
                .chain(extra.map(section))
                .collect()
        } else {
            vec![data.to_vec()]
        }
    })
}

fn intrinsic_pages(bytes: &[u8]) -> u32 {
    wasmparser::Parser::new(0)
        .parse_all(bytes)
        .find_map(|payload| match payload.unwrap() {
            wasmparser::Payload::CustomSection(section)
                if section.name() == abi::INTRINSIC_SECTION =>
            {
                Some(u32::from_le_bytes(section.data().try_into().unwrap()))
            }
            _ => None,
        })
        .unwrap_or(0)
}

fn declared_pages(bytes: &[u8]) -> Option<u32> {
    wasmparser::Parser::new(0)
        .parse_all(bytes)
        .find_map(|payload| match payload.unwrap() {
            wasmparser::Payload::CustomSection(section)
                if section.name() == abi::STATIC_SECTION =>
            {
                Some(
                    abi::StaticRegion::read_section(section.data())
                        .unwrap()
                        .extension_pages(),
                )
            }
            _ => None,
        })
}

#[test]
fn v38_static_data_beyond_one_page_runs_with_the_heap_after_the_extension() {
    let source = source();
    let v38 = LanguageProfile::CheckedF64Utf16V38;
    let bytes = varta_compiler::compile_typescript_with_profile(&source, v38).unwrap();
    let module = wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    let pages = declared_pages(&bytes).unwrap();
    // 65500 bytes of `a` stay in the first page; b, c (24000 bytes) and the
    // metadata spill into one extension page.
    assert_eq!(pages, 1);
    let extension = wasmparser::Parser::new(0)
        .parse_all(&bytes)
        .find_map(|payload| match payload.unwrap() {
            wasmparser::Payload::DataSection(reader) => reader
                .into_iter()
                .map(|segment| segment.unwrap().data.len())
                .nth(1),
            _ => None,
        })
        .unwrap();
    assert!(extension > 4 * UNITS, "{extension}");
    // The conversions and the parse this program makes link intrinsics,
    // whose region follows the extension (`linked_intrinsics`).
    let expected = abi::StaticRegion::with_extension_pages(pages)
        .and_then(|region| region.with_intrinsic_pages(intrinsic_pages(&bytes)))
        .unwrap();
    let memory = module
        .exports()
        .find_map(|export| export.ty().memory().cloned())
        .unwrap();
    let runtime = VartaRuntime::new();
    let prepared = runtime.prepare_module(&bytes).unwrap();
    // The extension, then the intrinsic region of its linked code.
    let region = prepared.static_region();
    assert_eq!(region, expected);
    assert_eq!(region.extension_pages(), pages);
    assert!(region.intrinsic_pages() > 0);
    assert_eq!(memory.minimum(), region.minimum_pages());
    let run = runtime.run_wasm_bytes(&bytes, &[]).unwrap();
    assert_eq!(
        run.lines,
        [
            r#"varta log: {"first":"aaa","second":"tail"}"#.to_string(),
            format!("varta log: {}", FIRST_UNITS + 2 * UNITS),
            "varta log: bb39:cc".to_string(),
            "varta log: xy:2".to_string(),
        ]
    );
    // The same program exceeds the single page of every earlier profile.
    let error = varta_compiler::compile_typescript_with_profile(
        &source,
        LanguageProfile::CheckedF64Utf16V37,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("static string data exceeds one Wasm page"),
        "{error}"
    );
}

#[test]
fn v38_modules_within_one_page_declare_no_static_section() {
    let bytes = varta_compiler::compile_typescript_with_profile(
        r#"export function main(): void { console.log("small"); }"#,
        LanguageProfile::CheckedF64Utf16V38,
    )
    .unwrap();
    assert_eq!(declared_pages(&bytes), None);
    let runtime = VartaRuntime::new();
    assert_eq!(
        runtime.prepare_module(&bytes).unwrap().static_region(),
        abi::StaticRegion::SINGLE_PAGE
    );
    assert_eq!(
        runtime.run_wasm_bytes(&bytes, &[]).unwrap().lines,
        ["varta log: small"]
    );
}

#[test]
fn malformed_or_foreign_static_regions_fail_before_instantiation() {
    let v38 = LanguageProfile::CheckedF64Utf16V38;
    let bytes = varta_compiler::compile_typescript_with_profile(&source(), v38).unwrap();
    let pages = declared_pages(&bytes).unwrap();
    let older = varta_compiler::compile_typescript_with_profile(
        r#"export function main(): void { console.log("v37"); }"#,
        LanguageProfile::CheckedF64Utf16V37,
    )
    .unwrap();
    let runtime = VartaRuntime::new();
    for changed in [
        // A duplicate declaration.
        static_section(&bytes, Some(pages), Some(pages)),
        // An extension larger than the module's initial memory.
        static_section(&bytes, Some(pages + 1), None),
        // A zero-page or malformed declaration.
        static_section(&bytes, Some(0), None),
        // Without its declaration the spilled data lies outside the region.
        static_section(&bytes, None, None),
        // A smaller extension than the data it holds.
        static_section(&bytes, Some(pages - 1).filter(|p| *p > 0), None),
        // Earlier profiles never declare a static extension.
        static_section(&older, None, Some(1)),
    ] {
        assert!(runtime.validate_module(&changed).is_err());
        assert!(runtime.run_wasm_bytes(&changed, &[]).is_err());
    }
    // A v38 module relabelled as v37 still fails at its host ABI section, and
    // with the v37 host ABI as well at its static section.
    let v37 = LanguageContract {
        schema_version: SCHEMA_VERSION,
        profile: LanguageProfile::CheckedF64Utf16V37,
    }
    .to_bytes()
    .unwrap();
    let relabelled = replace(&bytes, &v37);
    assert!(matches!(
        runtime.validate_module(&relabelled),
        Err(RuntimeError::Language(_))
    ));
    let downgraded = host_abi(&relabelled, Some(abi::NET_REQUEST_VERSION));
    assert!(runtime.validate_module(&downgraded).is_err());
    assert!(runtime.run_wasm_bytes(&downgraded, &[]).is_err());
}

/// Guest fuel of one run: instantiation plus `_start` under a test-only
/// ceiling large enough for the v37 data-segment copy.
pub(super) fn fuel_of(bytes: &[u8]) -> u64 {
    const CEILING: u64 = 64 * abi::FUEL;
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    execution.store.set_fuel(CEILING).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let store = &mut execution.store;
    let start = instance.get_func(&mut *store, "_start").unwrap();
    start.call(&mut *store, &[], &mut []).unwrap();
    CEILING - store.get_fuel().unwrap()
}

/// Wasmtime meters instantiation's data-segment copy per byte of the
/// initialized image (rounded to host pages, including the gap before a
/// static extension). V38 does not charge it to the guest, so startup fuel
/// does not grow with unused static data.
#[test]
fn v38_startup_fuel_is_independent_of_static_size() {
    let program = |units: usize| {
        let literal = "q".repeat(units);
        format!(r#"export function main(): void {{ const a = "{literal}"; console.log("x"); }}"#)
    };
    let v38 = LanguageProfile::CheckedF64Utf16V38;
    let fuel = |units: usize| {
        fuel_of(&varta_compiler::compile_typescript_with_profile(&program(units), v38).unwrap())
    };
    let small = fuel(1);
    // 30000 units fit the first page, 90000 spill into the extension.
    for units in [30_000, 90_000] {
        assert_eq!(fuel(units), small, "{units} units");
    }
    assert!(small < 1_000, "{small}");
    // Earlier profiles keep charging the copy (the amount is host-specific).
    let older = varta_compiler::compile_typescript_with_profile(
        &program(30_000),
        LanguageProfile::CheckedF64Utf16V37,
    )
    .unwrap();
    assert!(fuel_of(&older) >= small);
}
