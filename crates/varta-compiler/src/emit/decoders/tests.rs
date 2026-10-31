use crate::{compile_typescript_with_profile, emit::decoder_hook};
use varta_common::language::LanguageProfile as P;

const V38: P = P::CheckedF64Utf16V38;
const V37: P = P::CheckedF64Utf16V37;

fn compile(source: &str, mode: decoder_hook::Mode) -> Vec<u8> {
    let bytes = decoder_hook::with(mode, || compile_typescript_with_profile(source, V38))
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}
/// Defined functions and code-section bytes of a module.
fn code(bytes: &[u8]) -> (u32, usize) {
    let mut functions = 0;
    let mut size = 0;
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        match payload.unwrap() {
            wasmparser::Payload::FunctionSection(reader) => functions = reader.count(),
            wasmparser::Payload::CodeSectionStart { size: bytes, .. } => size = bytes as usize,
            _ => {}
        }
    }
    (functions, size)
}
fn program(declarations: &str, parses: &[&str]) -> String {
    let body: String = parses
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            format!("const v{index} = JSON.parse(t) as {ty}; console.log(typeof v{index});")
        })
        .collect();
    format!("{declarations}\nexport function main(): void {{ const t = \"[]\"; {body} }}")
}

const ITEM: &str = "interface Item { id: number; name: string; tags: string[]; note?: string }";

/// Decoders validate in Wasmtime, exist only under v38 and are shared per
/// type: a second call site of the same target adds no decoder function,
/// and a target sharing nodes adds only its own.
#[test]
fn decoders_validate_and_are_shared_per_type() {
    let one = program(ITEM, &["Item[]"]);
    let two = program(ITEM, &["Item[]", "Item[]"]);
    let generic = code(&compile(&one, decoder_hook::Mode::Generic)).0;
    let decoded = code(&compile(&one, decoder_hook::Mode::Production)).0;
    assert!(decoded > generic, "{decoded} {generic}");
    assert_eq!(
        code(&compile(&two, decoder_hook::Mode::Production)).0,
        decoded
    );
    // `Item` alone shares the record node: one more entry only.
    let both = program(ITEM, &["Item[]", "Item"]);
    assert_eq!(
        code(&compile(&both, decoder_hook::Mode::Production)).0,
        decoded + 1
    );
    // Collecting decoders are the same functions.
    assert_eq!(
        code(&compile(&one, decoder_hook::Mode::Collecting)).0,
        decoded
    );
    // V37 has no decoders: the switch changes nothing there.
    let v37 =
        |mode| decoder_hook::with(mode, || compile_typescript_with_profile(&one, V37)).unwrap();
    assert_eq!(
        v37(decoder_hook::Mode::Production),
        v37(decoder_hook::Mode::Generic)
    );
}

/// Code bytes the decoders add per target (`--nocapture`): the
/// development-plan table.
#[test]
#[ignore]
fn decoder_code_bytes_per_target() {
    let event = "interface A { type: string; seq: number; detail: { code: number; reason: string | null } }";
    let wide: Vec<String> = (0..40).map(|i| format!("f{i}: number")).collect();
    let wide = format!("interface W {{ {} }}", wide.join("; "));
    let targets = [
        ("Item[] (5 nodes)", ITEM, "Item[]"),
        ("unknown (7 members, array, dictionary)", "", "unknown"),
        ("event record with a nested record", event, "A"),
        ("40-field record", wide.as_str(), "W"),
        ("Record<string, number>", "", "Record<string, number>"),
        ("number[]", "", "number[]"),
    ];
    println!("| target | module bytes | generic bytes | code bytes added | decoder functions |");
    for (label, declarations, ty) in targets {
        let source = program(declarations, &[ty]);
        let decoded = compile(&source, decoder_hook::Mode::Production);
        let generic = compile(&source, decoder_hook::Mode::Generic);
        let (functions, size) = code(&decoded);
        let (generic_functions, generic_size) = code(&generic);
        println!(
            "| {label} | {} | {} | {} | {} |",
            decoded.len(),
            generic.len(),
            size - generic_size,
            functions - generic_functions
        );
    }
}
