use super::*;
use varta_common::value::{MAX_ARRAY_ELEMENTS, MAX_ARRAY_TYPES};

#[test]
fn dense_array_profile_rejects_unsupported_or_inconsistent_values() {
    for body in [
        "const a=[];",
        "const a=[1,true];",
        "const a:string[]=[1];",
        "const a=[1];a[0]=true;",
        "const a=[1];a[true];",
        "const a=[1];a[\"0\"];",
        "const a=[1];a.length=0;",
        "const a=[1];a={length:1};",
        "const a=[1];a=[2];",
        "const a=[1];a.push(2);",
        "const a=[1,,2];",
        "const a=[1,...[2]];",
        "const a=[undefined];",
        "const a=[console.log(1)];",
        "const a:void[]=[];",
        "const a:readonly number[]=[1];",
        "const a:[number]=[1];",
        "const a=Array(3);",
        "const a=new Array(3);",
        "const a=[[1],[true]];",
    ] {
        let source = format!("export function main():void{{{body}}}");
        let error = compile_typescript(&source).unwrap_err();
        let diagnostic = error.diagnostic().unwrap();
        assert_eq!(diagnostic.file, "input.ts", "{body}");
        assert_ne!(
            diagnostic.range.start.byte_offset, diagnostic.range.end.byte_offset,
            "{body}"
        );
    }
    for signature in [
        "(a:number[]):number{return a.length;}",
        "():number[]{return [1];}",
    ] {
        let source = format!(
            "export function f{signature}export function main():void{{}}varta.tool({{name:\"f\",description:\"F\"}},f);"
        );
        let error = compile_typescript(&source).unwrap_err();
        assert!(error.to_string().contains("tool contract v1"), "{error}");
    }
}

#[test]
fn array_type_and_index_errors_keep_the_innermost_source_range() {
    for (body, expected) in [
        ("const a=[1,\"wrong\"];", "\"wrong\""),
        ("const a=[1];a[0]=\"wrong\";", "\"wrong\""),
        ("const a=[1];a[false];", "false"),
    ] {
        let source = format!("export function main():void{{{body}}}");
        let error =
            compile_typescript_with_path(&source, std::path::Path::new("arrays.ts")).unwrap_err();
        let diagnostic = error.diagnostic().unwrap();
        assert_eq!(diagnostic.file, "arrays.ts");
        let range = &diagnostic.range;
        assert_eq!(
            &source[range.start.byte_offset..range.end.byte_offset],
            expected
        );
    }
}

#[test]
fn array_element_and_type_counts_have_shared_limits() {
    let values = vec!["1"; MAX_ARRAY_ELEMENTS + 1].join(",");
    let source = format!("export function main():void{{const a=[{values}];}}");
    assert!(
        compile_typescript(&source)
            .unwrap_err()
            .to_string()
            .contains("element count")
    );
    let declarations = ["number", "boolean", "string", "null"]
        .into_iter()
        .enumerate()
        .map(|(i, ty)| {
            format!(
                "const a{i}:{ty}{}=[];",
                "[]".repeat(MAX_ARRAY_TYPES / 4 + 1)
            )
        })
        .collect::<String>();
    let source = format!("export function main():void{{{declarations}}}");
    let error = compile_typescript(&source).unwrap_err();
    assert!(error.to_string().contains("array type count"), "{error}");
}

#[test]
fn wide_array_support_preserves_expression_and_generic_recursion_limits() {
    for expression in [
        format!("[{}1]", "1+".repeat(1000)),
        format!("[f<{}T{}>()]", "T,T,f<".repeat(1000), ">".repeat(1000)),
    ] {
        let source = format!("export function main():void{{const a={expression};}}");
        assert!(
            compile_typescript(&source)
                .unwrap_err()
                .to_string()
                .contains("complexity")
        );
    }
    for declaration in ["interface Array<T>{length:number}", "type Array<T>={x:T};"] {
        let source = format!("{declaration}export function main():void{{}}");
        assert!(
            compile_typescript(&source)
                .unwrap_err()
                .to_string()
                .contains("redeclaring the built-in Array")
        );
    }
}
