#![forbid(unsafe_code)]
//! Checked TypeScript subset -> typed IR -> validated portable WebAssembly.
mod emit;
mod error;
mod ir;
mod lower;
mod modules;
pub use modules::{Compilation, ModuleLoader, ModuleSource};
#[cfg(test)]
mod array_tests;
#[cfg(test)]
mod async_generator_tests;
#[cfg(test)]
mod async_method_tests;
#[cfg(test)]
mod frame_liveness_tests;
#[cfg(test)]
mod generator_tests;
#[cfg(test)]
mod http_tests;
#[cfg(test)]
mod json_parse_tests;
mod parse;
#[cfg(test)]
mod promise_tests;
#[cfg(test)]
mod promise_value_tests;
#[cfg(test)]
mod record_tests;
mod regexp;
#[cfg(test)]
mod result_tests;
mod source;
#[cfg(test)]
mod string_operation_tests;
#[cfg(test)]
mod suspension_tests;
#[cfg(test)]
mod tool_tests;

pub use error::CompilerError;

/// Test-only access to the K4.4 suspension lowering hook for other crates'
/// tests (runtime admission of resumable frames). Never enabled by production
/// dependents; no source profile admits suspension syntax through it.
#[cfg(feature = "suspension-test-hook")]
#[doc(hidden)]
pub mod suspension_test_hook {
    /// The hook call that lowers to a suspension point.
    pub const SUSPEND: &str = crate::lower::suspension::hook::NAME;
    /// Export prefix of frame creators, followed by the layout id.
    pub const CREATE_PREFIX: &str = crate::emit::TEST_CREATE_PREFIX;
    /// Export of the collector, for forced collections between resumptions.
    pub const COLLECT: &str = crate::emit::TEST_COLLECT;

    /// Compile one source with the hook enabled on this thread.
    pub fn compile_with_profile(
        source: &str,
        profile: varta_common::language::LanguageProfile,
    ) -> Result<Vec<u8>, crate::CompilerError> {
        crate::lower::suspension::hook::with(|| {
            crate::compile_typescript_with_profile(source, profile)
        })
    }
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

pub fn compile_typescript_with_profile(
    source: &str,
    profile: varta_common::language::LanguageProfile,
) -> Result<Vec<u8>, CompilerError> {
    compile_modules_with_profile(
        ModuleSource {
            path: "input.ts".into(),
            text: source.into(),
        },
        &modules::NoImports,
        profile,
    )
    .map(|c| c.wasm)
}

/// Compile without reading files; `path` identifies diagnostics for the supplied text.
pub fn compile_typescript_with_path(
    text: &str,
    path: &std::path::Path,
) -> Result<Vec<u8>, CompilerError> {
    bounded_source(text).map_err(|e| source::limit_error(path, e))?;
    Ok(compile_modules(
        ModuleSource {
            path: path.to_owned(),
            text: text.to_owned(),
        },
        &modules::NoImports,
    )?
    .wasm)
}

/// Compile one bounded module graph. The loader is explicit host authority; the
/// compiler itself never opens files or makes network requests.
pub fn compile_modules(
    entry: ModuleSource,
    loader: &dyn ModuleLoader,
) -> Result<Compilation, CompilerError> {
    compile_modules_with_profile(
        entry,
        loader,
        varta_common::language::LanguageProfile::default(),
    )
}

pub fn compile_modules_with_profile(
    entry: ModuleSource,
    loader: &dyn ModuleLoader,
    profile: varta_common::language::LanguageProfile,
) -> Result<Compilation, CompilerError> {
    use swc_common::Spanned;
    let graph = modules::Graph::load_with_profile(entry, loader, profile)?;
    let root = &graph.units[&graph.entry];
    let program = lower::lower(&graph, profile)?;
    let bytes = emit::emit(&program).map_err(|e| root.source.locate(e, root.ast.span()))?;
    wasmparser::Validator::new()
        .validate_all(&bytes)
        .map_err(|e| {
            root.source.locate(
                CompilerError::Internal(format!("generated invalid Wasm: {e}")),
                root.ast.span(),
            )
        })?;
    Ok(Compilation {
        wasm: bytes,
        modules: graph.metadata(),
    })
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
    fn checked_profile_does_not_silently_accept_unimplemented_js_values_or_coercions() {
        for expression in [
            "1.5",
            "2147483648",
            "undefined",
            "NaN",
            "Infinity",
            "void 0",
            "1==\"1\"",
            "null===0",
            r#""\uD800""#,
        ] {
            let source = format!("export function main():void{{const value={expression};}}");
            let error = compile_typescript(&source).unwrap_err();
            assert!(error.diagnostic().is_some(), "{expression}: {error}");
        }
    }
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
    fn flat_type_unions_and_lists_do_not_count_as_expression_chains() {
        let complexity = |source: &str| {
            compile_typescript(source)
                .err()
                .is_some_and(|e| e.to_string().contains("complexity"))
        };
        let members = (0..400).map(|n| format!("'m{n}'")).collect::<Vec<_>>();
        // Wide flat constructs: alias unions (single and multi-line), list commas.
        let alias = format!("type Wide = {};", members.join(" | "));
        let lines = format!("type Wide =\n  | {}\n", members.join("\n  | "));
        let array = format!("const a = [{}];", members.join(", "));
        let block = format!(
            "const o = {{ {} }};",
            (0..400)
                .map(|n| format!("k{n}: {n}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        for source in [
            alias.clone(),
            lines,
            format!("function f(): void {{ {array} {block} }}"),
        ] {
            let source = format!("{source}\nexport function main(): void {{}}");
            assert!(!complexity(&source), "{source}");
        }
        // More than the former 65,536-token file budget is bounded by bytes instead.
        let many = (0..20_000)
            .map(|n| format!("type T{n} = number;"))
            .collect::<String>();
        compile_typescript(&format!("{many}export function main(): void {{}}")).unwrap();
        // Chains that build nested AST nodes keep the existing ceiling.
        let chain = vec!["1"; 300].join(" | ");
        for source in [
            format!("export function main(): void {{ const x = {chain}; }}"),
            format!("type A = 'a'\nexport function main(): void {{ const x = {chain}; }}"),
            format!(
                "type A = {{ x: number }}; export function main(): void {{ const x = {chain}; }}"
            ),
            format!("type A = Array<{}>;", vec!["'a'"; 300].join(" | ")),
            format!(
                "export function main(): void {{ f({}); }}",
                vec!["1"; 300].join(", ")
            ),
        ] {
            assert!(complexity(&source), "{source}");
        }
    }
    #[test]
    fn static_strings_remain_bounded_and_void_values_are_rejected() {
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
            compile_typescript(
                "function nothing():void{} export function main():void { let s=nothing(); }"
            )
            .is_err()
        );
    }

    #[test]
    fn complete_string_signatures_obey_the_shared_complexity_budget() {
        let mut source = String::new();
        for index in 1..varta_common::abi::MAX_USER_FUNCTIONS {
            let params = (0..u8::BITS)
                .map(|bit| {
                    format!(
                        "p{bit}:{}",
                        if index & (1 << bit) == 0 {
                            "number"
                        } else {
                            "string"
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            source.push_str(&format!("function f{index}({params}):void{{}}\n"));
        }
        source.push_str("export function main():void{varta.env.get(\"NAME\");}");
        let error = compile_typescript(&source).unwrap_err();
        assert!(
            error.to_string().contains("signature complexity"),
            "{error}"
        );
        assert_eq!(error.diagnostic().unwrap().file, "input.ts");
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
