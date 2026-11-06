//! Platform fallback faces (`TextMeasurer::set_fallback`): a host names the
//! face its platform draws a `.notdef` cluster with, and only those clusters
//! take it, after their own CSS families.
use std::sync::{Arc, Mutex};
use varta_common::view::{CssProperty, Tag};
use varta_view::{
    Access, BatchWriter, Broker, FallbackFace, FallbackRefusal, FallbackRequest, FontFallback,
    MAX_FALLBACK_FACES, TextFont, TextMeasureError, TextMeasurer, View,
};

const WIDE: &[u8] = include_bytes!("fixtures/advance-900.ttf");
const WIDE_FAMILY: &str = "Varta Fixture 900";
/// Maps Cyrillic А (U+0410) to the 400-unit glyph; the wide face lacks it.
const CYRILLIC: &[u8] = include_bytes!("fixtures/cyrillic-400.ttf");
const SIZE: f32 = 20.0;
/// The 400-unit and 900-unit advances at `SIZE` (1000 units per em).
const NARROW_ADVANCE: f32 = 8.0;
const WIDE_ADVANCE: f32 = 18.0;

#[derive(Default)]
struct Log {
    requests: Vec<(String, String, f32, bool)>,
}

/// Answers every request with one face (or a refusal) and logs it.
struct Fixed {
    answer: Result<Option<FallbackFace>, FallbackRefusal>,
    log: Arc<Mutex<Log>>,
}
impl FontFallback for Fixed {
    fn face(
        &mut self,
        request: &FallbackRequest<'_>,
    ) -> Result<Option<FallbackFace>, FallbackRefusal> {
        self.log.lock().unwrap().requests.push((
            request.character.to_string(),
            request.family.into(),
            request.weight,
            request.italic,
        ));
        self.answer.clone()
    }
}

fn wide() -> TextMeasurer {
    TextMeasurer::new(WIDE.to_vec(), WIDE_FAMILY).unwrap()
}

fn cyrillic(key: &str) -> FallbackFace {
    FallbackFace {
        key: key.into(),
        data: CYRILLIC.into(),
    }
}

fn with(
    measurer: &mut TextMeasurer,
    answer: Result<Option<FallbackFace>, FallbackRefusal>,
) -> Arc<Mutex<Log>> {
    let log = Arc::new(Mutex::new(Log::default()));
    measurer.set_fallback(Some(Box::new(Fixed {
        answer,
        log: log.clone(),
    })));
    log
}

#[test]
fn a_notdef_cluster_takes_the_platform_face_and_covered_text_keeps_its_font() {
    let mut measurer = wide();
    // Without a provider the cluster keeps .notdef in the wide face.
    let notdef = measurer.measure("АA", SIZE, None).unwrap().width;
    assert_ne!(notdef, NARROW_ADVANCE + WIDE_ADVANCE);
    let log = with(&mut measurer, Ok(Some(cyrillic("cyrillic"))));
    let width = measurer.measure("АA", SIZE, None).unwrap().width;
    assert_eq!(width, NARROW_ADVANCE + WIDE_ADVANCE);
    // Latin A is covered by the wide face: it keeps that face although the
    // fallback face maps A too, and it never reaches the host.
    assert_eq!(
        log.lock().unwrap().requests,
        [("А".into(), WIDE_FAMILY.into(), 400.0, false)]
    );
    // Text the CSS families cover asks nothing.
    measurer.measure("AAA", SIZE, None).unwrap();
    assert_eq!(log.lock().unwrap().requests.len(), 1);
}

#[test]
fn missing_ranges_of_one_style_ask_for_the_first_character_once() {
    let mut measurer = wide();
    let log = with(&mut measurer, Ok(Some(cyrillic("cyrillic"))));
    // Every `.notdef` range of one style tries the face of the first
    // range's first character, as Blink's shaper falls back over a segment.
    let width = measurer
        .measure_with_family_list("\u{410}\u{456}A\u{430}", SIZE, None, WIDE_FAMILY)
        .unwrap()
        .width;
    assert_eq!(width, 3.0 * NARROW_ADVANCE + WIDE_ADVANCE);
    let requests: Vec<String> = log
        .lock()
        .unwrap()
        .requests
        .iter()
        .map(|request| {
            assert_eq!(request.1, WIDE_FAMILY);
            request.0.clone()
        })
        .collect();
    assert_eq!(requests, ["\u{410}"]);
    // A second text reuses the registered face.
    let width = measurer.measure("\u{430}", SIZE, None).unwrap().width;
    assert_eq!(width, NARROW_ADVANCE);
}

#[test]
fn no_platform_face_keeps_notdef() {
    let mut measurer = wide();
    let notdef = measurer.measure("А", SIZE, None).unwrap().width;
    let log = with(&mut measurer, Ok(None));
    assert_eq!(measurer.measure("А", SIZE, None).unwrap().width, notdef);
    assert_eq!(log.lock().unwrap().requests.len(), 1);
}

#[test]
fn a_refused_platform_face_refuses_the_text() {
    for refusal in [
        FallbackRefusal::Color,
        FallbackRefusal::Limit,
        FallbackRefusal::Unreadable,
    ] {
        let mut measurer = wide();
        with(&mut measurer, Err(refusal));
        assert!(matches!(
            measurer.measure("AА", SIZE, None),
            Err(TextMeasureError::Fallback { character: 'А', refusal: r }) if r == refusal
        ));
    }
    // Unparseable bytes are refused, not registered.
    let mut measurer = wide();
    with(
        &mut measurer,
        Ok(Some(FallbackFace {
            key: "garbage".into(),
            data: vec![0u8; 64].into(),
        })),
    );
    assert!(matches!(
        measurer.measure("А", SIZE, None),
        Err(TextMeasureError::Fallback {
            refusal: FallbackRefusal::Unreadable,
            ..
        })
    ));
}

#[test]
fn a_face_of_an_already_loaded_family_uses_that_family() {
    // The measurer already loads the Cyrillic face's family under its own
    // name: the platform face adds nothing and that family's face is used.
    let mut measurer = TextMeasurer::new_with_fonts(vec![
        TextFont {
            data: WIDE.to_vec(),
            family: WIDE_FAMILY.into(),
            aliases: vec![],
        },
        TextFont {
            data: include_bytes!("fixtures/advance-400.ttf").to_vec(),
            family: "Varta Fixture 400".into(),
            aliases: vec![],
        },
    ])
    .unwrap();
    // `advance-400` lacks Cyrillic, so the loaded family still draws .notdef:
    // the platform's face of that family is not added beside it.
    let before = measurer.measure("А", SIZE, None).unwrap().width;
    with(&mut measurer, Ok(Some(cyrillic("loaded family"))));
    let after = measurer.measure("А", SIZE, None).unwrap().width;
    assert_ne!(after, NARROW_ADVANCE);
    assert_eq!(before, after);
}

#[test]
fn distinct_fallback_faces_are_bounded() {
    /// A new face key for every request.
    struct Fresh(usize);
    impl FontFallback for Fresh {
        fn face(
            &mut self,
            _: &FallbackRequest<'_>,
        ) -> Result<Option<FallbackFace>, FallbackRefusal> {
            self.0 += 1;
            // Each copy registers as its own face of one family; the
            // family is new to the measurer, so every key registers.
            Ok(Some(cyrillic(&self.0.to_string())))
        }
    }
    let mut measurer = wide();
    measurer.set_fallback(Some(Box::new(Fresh(0))));
    for _ in 0..MAX_FALLBACK_FACES {
        measurer.measure("А", SIZE, None).unwrap();
    }
    assert!(matches!(
        measurer.measure("А", SIZE, None),
        Err(TextMeasureError::Fallback {
            refusal: FallbackRefusal::Limit,
            ..
        })
    ));
}

/// Maps U+4E2D with an ascent of 900 and a descent of 100 units; the wide
/// face has 800 and 200.
const TALL: &[u8] = include_bytes!("fixtures/unicode.ttf");

struct Denied;
impl Broker for Denied {
    fn link_requested(&self, _: &str) -> bool {
        false
    }
    fn open_url(&mut self, _: &str) -> Access {
        Access::Denied
    }
    fn clipboard_write(&mut self) -> Access {
        Access::Denied
    }
}

/// The height of a 20px line of `A\u{4E2D}` in the wide family whose Han
/// character takes the tall face, with this `line-height`, as plain text or
/// in a `span`.
fn fallback_line_height(line_height: &str, span: bool) -> f32 {
    let mut batch = BatchWriter::new();
    batch
        .create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, CssProperty::FontFamily, "\"Varta Fixture 900\"")
        .set_style(2, CssProperty::FontSize, "20px")
        .set_style(2, CssProperty::LineHeight, line_height);
    let holder = if span {
        batch.create_element(4, Tag::Span).insert(2, 4, 0);
        4
    } else {
        2
    };
    batch.create_text(3, "A\u{4E2D}").insert(holder, 3, 0);
    let mut view = View::default();
    view.commit(&batch.finish(), false, &mut Denied).unwrap();
    let mut measurer = wide();
    with(
        &mut measurer,
        Ok(Some(FallbackFace {
            key: "tall".into(),
            data: TALL.into(),
        })),
    );
    view.layout_boxes_with_text(200.0, 100.0, &mut measurer, 16.0)
        .unwrap()[&2]
        .height
}

#[test]
fn a_fixed_line_height_keeps_the_primary_font_metrics_for_fallback_text() {
    // Blink adds a fallback face's ascent and descent to the line only for
    // `line-height: normal` (18 + 4 at 20px); otherwise the primary font's
    // metrics place every fragment.
    for span in [false, true] {
        assert_eq!(fallback_line_height("20px", span), 20.0);
        assert_eq!(fallback_line_height("1", span), 20.0);
    }
    assert_eq!(fallback_line_height("normal", true), 22.0);
}

#[test]
fn an_enlarged_ascent_family_grows_by_fifteen_percent_of_its_height() {
    let mut measurer = wide();
    // 16 + 4 at 20px and no line gap.
    assert_eq!(measurer.measure("A", 20.0, None).unwrap().height, 20.0);
    measurer.set_enlarged_ascent_families(&["Varta Other Family"]);
    assert_eq!(measurer.measure("A", 20.0, None).unwrap().height, 20.0);
    measurer.set_enlarged_ascent_families(&[WIDE_FAMILY]);
    // The ascent gains round(20 * 0.15) = 3.
    assert_eq!(measurer.measure("A", 20.0, None).unwrap().height, 23.0);
}
