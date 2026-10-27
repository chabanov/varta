use super::*;
use varta_common::language::LanguageProfile;
fn compile(source: &str) -> Vec<u8> {
    varta_compiler::compile_typescript_with_profile(source, LanguageProfile::CheckedF64Utf16V14)
        .unwrap()
}
fn run(source: &str) -> RunOutput {
    let bytes = compile(source);
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    VartaRuntime::new().run_wasm_bytes(&bytes, &[]).unwrap()
}
fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| format!("varta log: {s}")).collect()
}
#[test]
fn basic_trim() {
    let out = run(
        "export function main():void{const s='  \\t\\nabc\\r\\n  ';console.log(JSON.stringify(s.trim()));console.log(JSON.stringify(s.trimStart()));console.log(JSON.stringify(s.trimEnd()));console.log(JSON.stringify('   '.trim()));console.log(JSON.stringify(''.trim()));}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "\"abc\"",
            "\"abc\\r\\n  \"",
            "\"  \\t\\nabc\"",
            "\"\"",
            "\"\""
        ])
    );
}
#[test]
fn preserve_surrogates_and_nul() {
    let out = run(
        "export function main():void{const s=' \\u0000\\ud800 X\\udfff ';console.log(JSON.stringify(s.trim()));console.log(s.trim().length);}",
    );
    assert_eq!(out.lines, lines(&["\"\\u0000\\ud800 X\\udfff\"", "5"]));
}

#[test]
fn trim_preserves_non_ecmascript_whitespace_in_all_modes() {
    let out = run(
        r"export function main():void{const text=' \u0085x\u0085 ';console.log(JSON.stringify(text.trim()));console.log(JSON.stringify(text.trimStart()));console.log(JSON.stringify(text.trimEnd()));console.log('\u0085'.trim().length);console.log('\u180e'.trim().length);console.log('\u200b'.trim().length);}",
    );
    assert_eq!(
        out.lines,
        lines(&[
            "\"\u{85}x\u{85}\"",
            "\"\u{85}x\u{85} \"",
            "\" \u{85}x\u{85}\"",
            "1",
            "1",
            "1"
        ])
    );
}
#[test]
fn trim_removes_the_complete_ecmascript_whitespace_and_line_terminator_set() {
    let units = [
        0x0009, 0x000a, 0x000b, 0x000c, 0x000d, 0x0020, 0x00a0, 0x1680, 0x2000, 0x2001, 0x2002,
        0x2003, 0x2004, 0x2005, 0x2006, 0x2007, 0x2008, 0x2009, 0x200a, 0x2028, 0x2029, 0x202f,
        0x205f, 0x3000, 0xfeff,
    ];
    let whitespace = units
        .iter()
        .map(|u| format!("\\u{u:04x}"))
        .collect::<String>();
    let source = format!(
        "export function main():void{{const ws='{whitespace}';console.log(ws.trim().length);console.log(ws.trimStart().length);console.log(ws.trimEnd().length);console.log((ws+'x'+ws).trim());console.log((ws+'x'+ws).trimStart().length);console.log((ws+'x'+ws).trimEnd().length);}}"
    );
    assert_eq!(run(&source).lines, lines(&["0", "0", "0", "x", "26", "26"]));
}
