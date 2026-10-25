use super::*;
use std::path::Path;

#[test]
fn oversized_string_root_frames_fail_with_a_source_diagnostic() {
    use varta_common::abi;
    let mut source = "export function main():void{const owned=\"a\"+\"b\";".to_owned();
    for index in 0..abi::STRING_ROOT_BYTES / abi::STRING_ROOT_SLOT_BYTES {
        source.push_str(&format!("const s{index}=\"static\";"));
    }
    source.push('}');
    let error = compile_typescript_with_path(&source, Path::new("roots.ts")).unwrap_err();
    let diagnostic = error.diagnostic().unwrap();
    assert_eq!(diagnostic.file, "roots.ts");
    assert_eq!(
        diagnostic.code,
        varta_common::DiagnosticCode::InvalidProgram
    );
    assert!(
        diagnostic.message.contains("root stack capacity"),
        "{diagnostic}"
    );
}

#[test]
fn mixed_string_operators_report_the_wrong_operand_with_its_real_type_and_range() {
    for (left, op, right) in [
        ("\"x\"", "+", "42"),
        ("42", "+", "\"x\""),
        ("\"x\"", "<", "false"),
        ("\"x\"", "==", "1"),
    ] {
        let source = format!("export function main():void{{console.log({left}{op}{right});}}");
        let error = compile_typescript_with_path(&source, Path::new("operators.ts")).unwrap_err();
        let diagnostic = error.diagnostic().unwrap();
        assert_eq!(diagnostic.file, "operators.ts");
        assert_eq!(diagnostic.code, varta_common::DiagnosticCode::TypeError);
        assert_eq!(
            &source[diagnostic.range.start.byte_offset..diagnostic.range.end.byte_offset],
            right
        );
        assert!(diagnostic.message.contains("expected"));
    }
}
