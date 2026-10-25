#![forbid(unsafe_code)]
//! Checked TypeScript subset -> typed IR -> validated portable WebAssembly.
mod emit;
mod error;
mod ir;
mod lower;
mod parse;
mod source;

pub use error::CompilerError;

#[must_use]
pub fn minimal_module() -> Vec<u8> {
    b"\0asm\x01\0\0\0".to_vec()
}

/// Parse an in-memory source using the compatibility filename `input.ts`.
pub fn parse_typescript(source: &str) -> Result<usize, CompilerError> {
    parse_typescript_with_path(source, std::path::Path::new("input.ts"))
}

pub fn parse_typescript_with_path(
    text: &str,
    path: &std::path::Path,
) -> Result<usize, CompilerError> {
    bounded_source(text).map_err(|e| source::limit_error(path, e))?;
    let source = source::Source::new(path, text);
    Ok(parse::parse_module(&source)?.body.len())
}

pub fn compile_typescript(source: &str) -> Result<Vec<u8>, CompilerError> {
    compile_typescript_with_path(source, std::path::Path::new("input.ts"))
}

/// Compile without reading files; `path` identifies diagnostics for the supplied text.
pub fn compile_typescript_with_path(
    text: &str,
    path: &std::path::Path,
) -> Result<Vec<u8>, CompilerError> {
    use swc_common::Spanned;
    bounded_source(text).map_err(|e| source::limit_error(path, e))?;
    let source = source::Source::new(path, text);
    let module = parse::parse_module(&source)?;
    let program = lower::lower(&module, &source).map_err(|e| source.locate(e, module.span()))?;
    let bytes = emit::emit(&program, &source)
        .map_err(|e| source.locate(e, program.functions[program.main].span))?;
    wasmparser::Validator::new()
        .validate_all(&bytes)
        .map_err(|e| {
            source.locate(
                CompilerError::Internal(format!("generated invalid Wasm: {e}")),
                module.span(),
            )
        })?;
    Ok(bytes)
}

fn bounded_source(source: &str) -> Result<(), CompilerError> {
    const MAX_SOURCE_BYTES: usize = 1024 * 1024;
    if source.len() > MAX_SOURCE_BYTES {
        return Err(CompilerError::Invalid("source exceeds 1 MiB".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_diagnostics_include_source_position_and_typed_variant() {
        let error = compile_typescript("export function main( {").unwrap_err();
        assert!(matches!(error, CompilerError::Parse(_)));
        assert!(error.to_string().contains("input.ts:1:"));
    }
    #[test]
    fn source_complexity_is_rejected_before_recursive_parsing() {
        let source = format!(
            "export function main():void {{ console.log({}1{}); }}",
            "(".repeat(1000),
            ")".repeat(1000)
        );
        assert!(
            compile_typescript(&source)
                .unwrap_err()
                .to_string()
                .contains("complexity")
        );
    }
    #[test]
    fn static_strings_remain_bounded_and_unsupported_results_are_explicit() {
        let source = format!(
            "export function main():void {{ console.log({:?}); }}",
            "A".repeat(65_537)
        );
        assert!(
            compile_typescript(&source)
                .unwrap_err()
                .to_string()
                .contains("static string data")
        );
        assert!(
            compile_typescript("function value():number {} export function main():void {}")
                .is_err()
        );
        assert!(
            compile_typescript("export function main():void { let s=varta.env.get(\"NAME\"); }")
                .is_err()
        );
    }
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    use std::path::Path;
    use varta_common::DiagnosticCode;

    fn diagnostic(source: &str) -> varta_common::Diagnostic {
        compile_typescript_with_path(source, Path::new("nested/actual source.ts"))
            .unwrap_err()
            .diagnostic()
            .unwrap()
            .clone()
    }

    #[test]
    fn type_mismatch_points_to_the_initializer_and_reports_expected_and_actual_types() {
        let source = "export function main(): void {\n  let x: string = 7;\n}\n";
        let error = diagnostic(source);
        assert_eq!(error.file, "nested/actual source.ts");
        assert_eq!(error.code, DiagnosticCode::TypeError);
        assert_eq!((error.range.start.line, error.range.start.column), (2, 19));
        assert_eq!((error.range.end.line, error.range.end.column), (2, 20));
        assert_eq!(
            &source[error.range.start.byte_offset..error.range.end.byte_offset],
            "7"
        );
        assert!(error.message.contains("expected string, found number"));
        assert!(error.to_string().contains("  |   let x: string = 7;"));
    }

    #[test]
    fn nested_diagnostics_keep_the_innermost_expression_range() {
        for (source, target, code) in [
            (
                "export function main():void { console.log(missing); }",
                "missing",
                DiagnosticCode::InvalidProgram,
            ),
            (
                "function f(x:number):number{return x;} export function main():void { console.log(f(true)); }",
                "true",
                DiagnosticCode::TypeError,
            ),
            (
                "export function main():void { console.log(varta.env.get(42)); }",
                "42",
                DiagnosticCode::TypeError,
            ),
            (
                "export function main():void { console.log(unknown(7)); }",
                "unknown(7)",
                DiagnosticCode::UnsupportedSyntax,
            ),
            (
                "function nothing():void {} export function main():void {console.log(nothing());}",
                "nothing()",
                DiagnosticCode::TypeError,
            ),
            (
                "export function main():void {if(\"text\") {}}",
                "\"text\"",
                DiagnosticCode::UnsupportedSyntax,
            ),
            (
                "function f(x:any):number{return 1;} export function main():void {}",
                "any",
                DiagnosticCode::UnsupportedSyntax,
            ),
        ] {
            let error = diagnostic(source);
            assert_eq!(error.code, code, "{error}");
            assert_eq!(
                &source[error.range.start.byte_offset..error.range.end.byte_offset],
                target,
                "{error}"
            );
        }
    }

    #[test]
    fn unicode_columns_utf8_offsets_crlf_and_bom_are_distinct_and_correct() {
        for prefix in ["", "\u{feff}"] {
            let source = format!(
                "{prefix}export function main():void {{\r\n  const назва=1; console.log(невідома);\r\n}}\r\n"
            );
            let error = diagnostic(&source);
            assert_eq!(error.range.start.line, 2);
            let line = "  const назва=1; console.log(невідома);";
            let column = line[..line.find("невідома").unwrap()].chars().count() + 1;
            assert_eq!(error.range.start.column, column);
            assert_eq!(
                error.range.start.byte_offset,
                source.find("невідома").unwrap()
            );
            assert_eq!(
                &source[error.range.start.byte_offset..error.range.end.byte_offset],
                "невідома"
            );
            assert!(error.snippet.unwrap().text.contains("назва"));
        }
    }

    #[test]
    fn parser_and_preflight_errors_use_the_supplied_filename() {
        let parse = diagnostic("export function main( {");
        assert_eq!(parse.code, DiagnosticCode::SyntaxError);
        assert_eq!(parse.file, "nested/actual source.ts");
        let source = format!(
            "export function main():void {{console.log({}1{});}}",
            "(".repeat(1000),
            ")".repeat(1000)
        );
        let limit = diagnostic(&source);
        assert_eq!(limit.file, "nested/actual source.ts");
        assert!(limit.message.contains("complexity"));
        let too_large = diagnostic(&" ".repeat(1024 * 1024 + 1));
        assert_eq!(too_large.range.start.byte_offset, 0);
        assert!(too_large.snippet.is_none());
    }

    #[test]
    fn snippets_are_bounded_and_static_data_errors_retain_literal_spans() {
        let source = format!(
            "export function main():void {{console.log({:?});}}",
            "A".repeat(65_537)
        );
        let error = diagnostic(&source);
        assert!(error.message.contains("static string data"));
        assert_eq!(source.as_bytes()[error.range.start.byte_offset], b'"');
        assert!(error.to_string().len() < 1024);
        let source = format!(
            "export function main():void {{\n{}console.log(missing);\n}}",
            " ".repeat(500)
        );
        let error = diagnostic(&source);
        let snippet = error.snippet.unwrap();
        assert!(snippet.text.starts_with('…'));
        assert!(snippet.text.chars().count() <= 162);
        assert!(snippet.highlight_start <= 61);
        assert_eq!(
            error.range.start.byte_offset,
            source.find("missing").unwrap()
        );
    }

    #[test]
    fn source_names_do_not_change_wasm_bytes_or_leak_between_compilations() {
        let source = "export function main():void {console.log(42);}";
        assert_eq!(
            compile_typescript(source).unwrap(),
            compile_typescript_with_path(source, Path::new("other/path.ts")).unwrap()
        );
        for name in ["one.ts", "second source.ts"] {
            let error = compile_typescript_with_path(
                "export function main():void {console.log(missing);}",
                Path::new(name),
            )
            .unwrap_err();
            assert_eq!(error.diagnostic().unwrap().file, name);
        }
    }
}
