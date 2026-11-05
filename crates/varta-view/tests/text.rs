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

/// `optical.ttf`'s A advance (units per em) at an optical size: 300 at the
/// axis minimum 8, 600 at the default 12, 1000 at the maximum 72.
fn optical_advance(opsz: f32) -> f32 {
    let opsz = opsz.clamp(8.0, 72.0);
    if opsz < 12.0 {
        600.0 - 300.0 * (12.0 - opsz) / 4.0
    } else {
        600.0 + 400.0 * (opsz - 12.0) / 60.0
    }
}

#[test]
fn text_takes_the_optical_size_of_its_font_size() {
    let font = include_bytes!("fixtures/optical.ttf").to_vec();
    let mut measurer = TextMeasurer::new(font, "Varta Optical Fixture").unwrap();
    // Below, inside and above the axis range: CSS `font-optical-sizing:
    // auto` sets opsz to the size in CSS pixels, clamped to the axis.
    for size in [4.0, 10.0, 12.0, 42.0, 100.0] {
        let width = measurer.measure("A", size, None).unwrap().width;
        let expected = size * optical_advance(size) / 1000.0;
        assert!(
            (width - expected).abs() < 0.01,
            "{size}px: {width} != {expected}"
        );
    }
}

#[test]
fn system_font_files_may_exceed_the_packaged_face_limit_only() {
    use varta_common::view::fonts::{
        MAX_FONT_BYTES, MAX_FONT_FACE_BYTES, MAX_SYSTEM_FONT_FILE_BYTES,
    };
    let padded = |length: usize| {
        let mut data = include_bytes!("fixtures/optical.ttf").to_vec();
        data.resize(length, 0);
        vec![TextFont {
            data,
            family: "Varta Optical Fixture".into(),
            aliases: vec!["system-ui".into()],
        }]
    };
    assert!(matches!(
        TextMeasurer::new_with_fonts(padded(MAX_FONT_FACE_BYTES + 1)),
        Err(TextMeasureError::Font)
    ));
    let mut measurer =
        TextMeasurer::new_with_system_fonts(padded(MAX_FONT_FACE_BYTES + 1)).unwrap();
    let width = measurer
        .measure_with_family_list("A", 12.0, None, "system-ui")
        .unwrap()
        .width;
    assert!((width - 7.2).abs() < 0.01, "{width}");
    assert_eq!(
        TextMeasurer::system_font_families(padded(MAX_FONT_FACE_BYTES + 1).remove(0).data).unwrap(),
        ["Varta Optical Fixture"]
    );
    assert!(matches!(
        TextMeasurer::system_font_families(vec![0; 16]),
        Err(TextMeasureError::Font)
    ));
    assert!(matches!(
        TextMeasurer::new_with_system_fonts(padded(MAX_SYSTEM_FONT_FILE_BYTES + 1)),
        Err(TextMeasureError::Font)
    ));
    // The collection total still bounds system files.
    let mut two = padded(MAX_FONT_BYTES / 2 + 1);
    two.extend(padded(MAX_FONT_BYTES / 2 + 1).into_iter().map(|mut font| {
        font.aliases = vec!["sans-serif".into()];
        font
    }));
    assert!(matches!(
        TextMeasurer::new_with_system_fonts(two),
        Err(TextMeasureError::Font)
    ));
}
