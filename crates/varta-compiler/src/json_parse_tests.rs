use super::*;

#[test]
fn checked_json_calls_require_a_supported_explicit_type_and_one_string() {
    for body in [
        "varta.json.parse(\"1\");",
        "varta.json.parse<number,string>(\"1\");",
        "varta.json.parse<number>();",
        "varta.json.parse<number>(\"1\",\"2\");",
        "varta.json.parse<number>(1);",
        "varta.json.parse<void>(\"null\");",
        "varta.json.parse<any>(\"null\");",
        "varta.json.parse<VartaResult<string>>(\"{}\");",
        "varta.json.parse<{optional?:number}>(\"{}\");",
        "varta.json.parse<[number]>(\"[1]\");",
        "const a=varta.result.capture(varta.json.parse<number>(\"1\"));",
        "const a:number=varta.json.parse<string>(\"\\\"s\\\"\");",
    ] {
        let source = format!("export function main():void{{{body}}}");
        let error = compile_typescript(&source).unwrap_err();
        let diagnostic = error.diagnostic().unwrap();
        assert_eq!(diagnostic.file, "input.ts");
        assert_ne!(
            diagnostic.range.start.byte_offset,
            diagnostic.range.end.byte_offset
        );
    }
    let source = "export function main():void{varta.json.parse<number>(false);}";
    let error =
        compile_typescript_with_path(source, std::path::Path::new("parsing.ts")).unwrap_err();
    let diagnostic = error.diagnostic().unwrap();
    assert_eq!(diagnostic.file, "parsing.ts");
    assert_eq!(
        &source[diagnostic.range.start.byte_offset..diagnostic.range.end.byte_offset],
        "false"
    );
}

#[test]
fn embedded_json_schemas_share_the_static_memory_budget() {
    let mut fields = Vec::new();
    for i in 0..32 {
        fields.push(format!("\"{i}_{}\":number", "x".repeat(250)));
    }
    let record = format!("{{{}}}", fields.join(";"));
    let source = format!(
        "export function main():void{{varta.json.parse<{{a:{record};b:{record};c:{record};d:{record}}}>(\"{{}}\");}}"
    );
    // Shared record types must not expand exponentially in the generated schema.
    compile_typescript(&source).unwrap();
    let many = (0..10)
        .map(|i| {
            let fields = (0..32)
                .map(|j| format!("\"{i}_{j}_{}\":number", "x".repeat(200)))
                .collect::<Vec<_>>()
                .join(";");
            format!("varta.json.parse<{{{fields}}}>(\"{{}}\");")
        })
        .collect::<String>();
    let error = compile_typescript(&format!("export function main():void{{{many}}}")).unwrap_err();
    assert!(error.to_string().contains("static"), "{error}");
    assert!(error.diagnostic().is_some());
}
