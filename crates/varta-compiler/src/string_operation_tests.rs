use super::*;
use std::path::Path;

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
