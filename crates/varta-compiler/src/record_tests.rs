use super::*;
use varta_common::value::{MAX_RECORD_FIELDS, MAX_RECORD_KEY_BYTES, MAX_RECORD_SHAPES};

#[test]
fn closed_record_profile_rejects_unsupported_or_inconsistent_source() {
    for body in [
        "const o={a:1};o.a=true;",
        "const o={a:1};o.missing=2;",
        "const o={a:1};o={a:2};",
        "let o:{a:number}={b:1};",
        "const o={a:1,a:2};",
        "const o={__proto__:null};",
        "const o={a:1};const p={...o};",
        "const o={method():number{return 1;}};",
        "const o={get a(){return 1;}};",
        "const o={a:1};const key=\"a\";console.log(o[key]);",
        "const o={[\"a\"]:1};",
        "const o:{readonly a:number}={a:1};",
        "const o:{a?:number}={a:1};",
        "const o={a:undefined};",
        "const o={a:console.log(1)};",
        "const o={a:varta.result.capture(varta.env.get(\"NAME\"))};",
        "console.log(JSON.stringify(1, null));",
        "const o=varta.result.capture(JSON.stringify({a:1}));",
        "const JSON=1;",
    ] {
        let source = format!("export function main():void{{{body}}}");
        let error = compile_typescript(&source).unwrap_err();
        let diagnostic = error.diagnostic().unwrap();
        assert_eq!(diagnostic.file, "input.ts", "{body}");
        assert_ne!(
            diagnostic.range.start.byte_offset,
            diagnostic.range.end.byte_offset
        );
    }
    for source in [
        "export function f(o:{a:number}):number{return o.a;}export function main():void{}varta.tool({name:\"f\",description:\"F\"},f);",
        "export function f():null{return null;}export function main():void{}varta.tool({name:\"f\",description:\"F\"},f);",
    ] {
        let error = compile_typescript(source).unwrap_err();
        assert!(error.to_string().contains("tool contract v1"), "{error}");
    }
}

#[test]
fn field_errors_keep_the_property_or_rhs_source_range() {
    for (body, expected) in [
        ("const o={a:1};o.a=\"wrong\";", "\"wrong\""),
        ("const o={a:1};console.log(o.missing);", "missing"),
    ] {
        let source = format!("export function main():void{{{body}}}");
        let error =
            compile_typescript_with_path(&source, std::path::Path::new("values.ts")).unwrap_err();
        let diagnostic = error.diagnostic().unwrap();
        assert_eq!(diagnostic.file, "values.ts");
        let range = &diagnostic.range;
        assert_eq!(
            &source[range.start.byte_offset..range.end.byte_offset],
            expected
        );
    }
}

#[test]
fn object_fields_shapes_keys_and_static_descriptors_are_bounded() {
    let fields = (0..=MAX_RECORD_FIELDS)
        .map(|i| format!("k{i}:1"))
        .collect::<Vec<_>>()
        .join(",");
    let too_many_fields = format!("export function main():void{{const o={{{fields}}};}}");
    let too_long_key = format!(
        "export function main():void{{const o={{\"{}\":1}};}}",
        "x".repeat(MAX_RECORD_KEY_BYTES + 1)
    );
    let shapes = (0..=MAX_RECORD_SHAPES)
        .map(|i| format!("{{const o={{k{i}:1}};}}"))
        .collect::<String>();
    let too_many_shapes = format!("export function main():void{{{shapes}}}");
    // Keys fit individually, but their shared static pool and descriptors do not.
    let static_shapes = (0..12)
        .map(|i| {
            let fields = (0..MAX_RECORD_FIELDS)
                .map(|j| format!("\"{i}_{j}_{}\":1", "x".repeat(200)))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{const o={{{fields}}};}}")
        })
        .collect::<String>();
    let too_much_static_data = format!("export function main():void{{{static_shapes}}}");
    for (source, expected) in [
        (too_many_fields, "field count"),
        (too_long_key, "key exceeds"),
        (too_many_shapes, "shape count"),
        (too_much_static_data, "static"),
    ] {
        let error = compile_typescript(&source).unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
        assert!(error.diagnostic().is_some());
    }
}
