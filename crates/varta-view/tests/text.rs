use varta_view::{TextFont, TextMeasureError, TextMeasurer};

#[test]
fn explicit_font_shapes_predictable_text_and_rejects_invalid_inputs() {
    let font = include_bytes!("fixtures/advance-400.ttf").to_vec();
    assert!(matches!(
        TextMeasurer::new(vec![0, 1, 2], "missing"),
        Err(TextMeasureError::Font)
    ));
    assert!(matches!(
        TextMeasurer::new(font.clone(), "missing"),
        Err(TextMeasureError::Font)
    ));
    assert!(matches!(
        TextMeasurer::new_with_aliases(font.clone(), "Varta Fixture 400", &["serif, sans-serif"]),
        Err(TextMeasureError::Font)
    ));
    let mut measurer = TextMeasurer::new(font, "Varta Fixture 400").unwrap();
    let metrics = measurer.measure("AAAA", 20.0, None).unwrap();
    assert!((metrics.width - 32.0).abs() < 0.01, "{metrics:?}");
    assert!(metrics.height > 0.0);
    assert_eq!(metrics.lines, 1);
    assert!(matches!(
        measurer.measure("A", f32::NAN, None),
        Err(TextMeasureError::Limit)
    ));
    assert!(matches!(
        measurer.measure("A", 20.0, Some(0.0)),
        Err(TextMeasureError::Limit)
    ));
    let boundary = measurer.measure(&"A".repeat(65536), 20.0, None).unwrap();
    assert_eq!(boundary.width, 524_288.0);
    assert!(matches!(
        measurer.measure(&"A".repeat(65537), 20.0, None),
        Err(TextMeasureError::Limit)
    ));
}

#[test]
fn explicit_font_list_falls_back_for_missing_glyphs() {
    let wide = include_bytes!("fixtures/advance-900.ttf").to_vec();
    let cyrillic = include_bytes!("fixtures/cyrillic-400.ttf").to_vec();
    let fonts = vec![
        TextFont {
            data: wide,
            family: "Varta Fixture 900".into(),
            aliases: vec![],
        },
        TextFont {
            data: cyrillic,
            family: "Varta Fixture 400".into(),
            aliases: vec![],
        },
    ];
    let mut measurer = TextMeasurer::new_with_fonts(fonts).unwrap();
    let metrics = measurer
        .measure_with_family_list(
            "АA",
            20.0,
            None,
            "\"Varta Fixture 900\", \"Varta Fixture 400\"",
        )
        .unwrap();
    assert!((metrics.width - 26.0).abs() < 0.01, "{metrics:?}");
}

#[test]
fn font_collection_rejects_ambiguous_aliases_and_empty_input() {
    assert!(matches!(
        TextMeasurer::new_with_fonts(vec![]),
        Err(TextMeasureError::Font)
    ));
    let fonts = vec![
        TextFont {
            data: include_bytes!("fixtures/advance-900.ttf").to_vec(),
            family: "Varta Fixture 900".into(),
            aliases: vec!["system-ui".into()],
        },
        TextFont {
            data: include_bytes!("fixtures/cyrillic-400.ttf").to_vec(),
            family: "Varta Fixture 400".into(),
            aliases: vec!["system-ui".into()],
        },
    ];
    assert!(matches!(
        TextMeasurer::new_with_fonts(fonts),
        Err(TextMeasureError::Font)
    ));
    let mismatched_face = vec![
        TextFont {
            data: include_bytes!("fixtures/advance-900.ttf").to_vec(),
            family: "Varta Fixture 900".into(),
            aliases: vec![],
        },
        TextFont {
            data: include_bytes!("fixtures/advance-400.ttf").to_vec(),
            family: "Varta Fixture 900".into(),
            aliases: vec![],
        },
    ];
    assert!(matches!(
        TextMeasurer::new_with_fonts(mismatched_face),
        Err(TextMeasureError::Font)
    ));
}
