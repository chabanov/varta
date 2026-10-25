use super::*;

fn catalog() -> ToolCatalog {
    ToolCatalog::new(vec![ToolContract {
        name: "document.rename".into(),
        description: "Rename a document".into(),
        export: format!("{TOOL_EXPORT_PREFIX}document.rename"),
        parameters: vec![ToolParameter {
            name: "title".into(),
            value_type: ToolType::String,
        }],
        result: ToolType::Integer,
        required_effects: vec![],
        limits: ToolLimits::default(),
    }])
}

#[test]
fn schemas_express_closed_named_inputs_i32_and_wrapped_outputs() {
    let source = catalog();
    assert_eq!(
        ToolCatalog::parse(&source.to_bytes().unwrap()).unwrap(),
        source
    );
    let json = source.discovery();
    assert_eq!(
        json["tools"][0]["inputSchema"]["additionalProperties"],
        false
    );
    assert_eq!(
        json["tools"][0]["inputSchema"]["required"],
        serde_json::json!(["title"])
    );
    assert_eq!(
        json["tools"][0]["outputSchema"]["properties"]["value"]["maximum"],
        i32::MAX
    );
    assert_eq!(ToolType::Void.schema()["type"], "null");
}

#[test]
fn wire_versions_duplicates_unknown_fields_and_resource_limits_fail_closed() {
    let source = catalog();
    let json = serde_json::to_value(&source).unwrap();
    for (key, value) in [("contractVersion", 2), ("invocationAbiVersion", 0)] {
        let mut invalid = json.clone();
        invalid[key] = value.into();
        assert!(ToolCatalog::parse(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }
    let mut invalid = json.clone();
    invalid["tools"][0]["inputSchema"] = serde_json::json!({});
    assert!(ToolCatalog::parse(&serde_json::to_vec(&invalid).unwrap()).is_err());
    let text = String::from_utf8(source.to_bytes().unwrap()).unwrap();
    assert!(
        ToolCatalog::parse(
            text.replacen(
                "\"contractVersion\":1",
                "\"contractVersion\":1,\"contractVersion\":1",
                1
            )
            .as_bytes()
        )
        .is_err()
    );
    assert!(ToolCatalog::parse(&vec![b' '; MAX_TOOL_CATALOG_BYTES + 1]).is_err());
    for change in [
        |c: &mut ToolCatalog| c.tools.push(c.tools[0].clone()),
        |c: &mut ToolCatalog| c.tools[0].parameters[0].value_type = ToolType::Void,
        |c: &mut ToolCatalog| {
            let duplicate = c.tools[0].parameters[0].clone();
            c.tools[0].parameters.push(duplicate);
        },
        |c: &mut ToolCatalog| c.tools[0].description = "x".repeat(MAX_TOOL_DESCRIPTION_BYTES + 1),
        |c: &mut ToolCatalog| c.tools[0].export = "_start".into(),
        |c: &mut ToolCatalog| c.tools[0].limits.fuel += 1,
        |c: &mut ToolCatalog| c.tools[0].required_effects = vec!["env.unknown".into()],
        |c: &mut ToolCatalog| c.tools[0].parameters[0].name = "bad.field".into(),
    ] {
        let mut invalid = source.clone();
        change(&mut invalid);
        assert!(invalid.to_bytes().is_err());
    }
}
