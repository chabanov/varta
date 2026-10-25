use super::*;
use varta_common::ToolCatalog;

fn catalog(source: &str) -> ToolCatalog {
    let bytes = compile_typescript(source).unwrap();
    for payload in wasmparser::Parser::new(0).parse_all(&bytes) {
        if let wasmparser::Payload::CustomSection(section) = payload.unwrap()
            && section.name() == varta_common::TOOL_SECTION
        {
            return ToolCatalog::parse(section.data()).unwrap();
        }
    }
    panic!("missing tool section")
}

#[test]
fn publication_is_explicit_and_deterministic_and_schemas_come_from_checked_types() {
    let functions = "export function echo(text:string):string{return text;} export function hidden(n:number):number{return n;} export function main():void{}";
    let declaration = "varta.tool({name:'text.echo',description:'Echo text'},echo);";
    let source = format!("{functions}{declaration}");
    let tools = catalog(&source);
    assert_eq!(tools.tools.len(), 1);
    assert_eq!(tools.tools[0].parameters[0].name, "text");
    assert_eq!(tools.tools[0].result, varta_common::ToolType::String);
    assert_eq!(
        compile_typescript(&source).unwrap(),
        compile_typescript(&format!("{declaration}{functions}")).unwrap()
    );
    let no_tools = compile_typescript(functions).unwrap();
    assert!(!wasmparser::Parser::new(0).parse_all(&no_tools).any(|payload|
        matches!(payload.unwrap(), wasmparser::Payload::CustomSection(section) if section.name() == varta_common::TOOL_SECTION)));
}

#[test]
fn invalid_or_ambiguous_publication_fails_with_source_diagnostics() {
    let functions = "export function echo(text:string):string{return text;} function privateFn():void{} export function main():void{}";
    for declaration in [
        "varta.tool({name:'x',description:'X'},missing);",
        "varta.tool({name:'x',description:'X'},privateFn);",
        "varta.tool({name:'x',description:'X'},main);",
        "varta.tool({name:'x',description:'X'},(s:string):string=>s);",
        "varta.tool({name:'x'},echo);",
        "varta.tool({name:'x',description:''},echo);",
        "varta.tool({name:'bad name',description:'X'},echo);",
        "varta.tool({name:'x',description:'X',unknown:true},echo);",
        "varta.tool({name:'x',name:'y',description:'X'},echo);",
        "varta.tool({name:'x',description:echo('x')},echo);",
        "varta.tool({['name']:'x',description:'X'},echo);",
        "varta.tool({name:'x',description:'X'},echo,echo);",
        "varta.tool({name:'x',description:'X'},echo);varta.tool({name:'x',description:'X'},echo);",
        "main();",
    ] {
        let error = compile_typescript(&format!("{functions}{declaration}")).unwrap_err();
        assert_eq!(
            error.diagnostic().unwrap().file,
            "input.ts",
            "{declaration}"
        );
    }
    for function in [
        "export function f(r:VartaResult<string>):string{return r.value;}",
        "export function f():VartaResult<number>{return varta.result.capture(varta.time.now());}",
        "export function f(назва:string):string{return назва;}",
    ] {
        assert!(compile_typescript(&format!("{function} export function main():void{{}}varta.tool({{name:'x',description:'X'}},f);")).is_err());
    }
}

#[test]
fn tool_and_parameter_budgets_are_independent_of_function_budgets() {
    let mut source = "export function f():void{} export function main():void{}".to_owned();
    for index in 0..varta_common::MAX_TOOLS {
        source.push_str(&format!(
            "varta.tool({{name:'t{index:03}',description:'Tool'}},f);"
        ));
    }
    assert_eq!(catalog(&source).tools.len(), varta_common::MAX_TOOLS);
    source.push_str("varta.tool({name:'overflow',description:'Tool'},f);");
    assert!(
        compile_typescript(&source)
            .unwrap_err()
            .to_string()
            .contains("too many tools")
    );
    let parameters = (0..=varta_common::MAX_TOOL_PARAMETERS)
        .map(|n| format!("p{n}:number"))
        .collect::<Vec<_>>()
        .join(",");
    assert!(compile_typescript(&format!("export function f({parameters}):void{{}}export function main():void{{}}varta.tool({{name:'x',description:'Tool'}},f);")).is_err());
}

#[test]
fn invalid_tool_metadata_points_to_the_actual_literal() {
    for literal in ["'bad name'", "' '"] {
        let (name, description) = if literal == "'bad name'" {
            (literal, "'Preview'")
        } else {
            ("'preview'", literal)
        };
        let text = format!(
            "export function f():void{{}}export function main():void{{}}\nvarta.tool({{name:{name},description:{description}}},f);"
        );
        let error =
            compile_typescript_with_path(&text, std::path::Path::new("app/tools.ts")).unwrap_err();
        let diagnostic = error.diagnostic().unwrap();
        assert_eq!(diagnostic.file, "app/tools.ts");
        assert_eq!(
            &text[diagnostic.range.start.byte_offset..diagnostic.range.end.byte_offset],
            literal
        );
    }
}
