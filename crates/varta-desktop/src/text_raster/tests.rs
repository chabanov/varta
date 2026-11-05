use super::*;
const VARIABLE: &[u8] = include_bytes!("../../tests/fixtures/fonts/variable.ttf");
const COLLECTION: &[u8] = include_bytes!("../../tests/fixtures/fonts/collection.ttc");
/// Glyph order of both fixtures: `.notdef`, `space`, `A`, `B`.
const GLYPH_A: u32 = 2;
const GLYPH_B: u32 = 3;
/// 100 px at 1000 units per em: one pixel per ten font units.
const SIZE: f32 = 100.0;
const CACHE: usize = 8 * 1024 * 1024;

fn request<'a>(
    id: u64,
    data: &'a [u8],
    index: u32,
    glyph: u32,
    coords: &'a [i16],
) -> GlyphRasterRequest<'a> {
    GlyphRasterRequest {
        font_id: id,
        font_data: data,
        font_index: index,
        glyph,
        size: SIZE,
        normalized_coords: coords,
        phase_x: 0,
        max_pixels: 1 << 20,
        cache_bytes: CACHE,
    }
}
struct Drawn {
    bounds: GlyphRasterBounds,
    coverage: Vec<u8>,
}
/// Inked columns and rows, independent of the reserved smoothing fringe.
fn ink(drawn: &Drawn) -> (u32, u32) {
    let width = drawn.bounds.width as usize;
    let rows = drawn.coverage.chunks(width.max(1));
    let columns = (0..width)
        .filter(|x| drawn.coverage.chunks(width).any(|row| row[*x] > 0))
        .count();
    (
        columns as u32,
        rows.filter(|row| row.iter().any(|v| *v > 0)).count() as u32,
    )
}
fn draw(raster: &mut CoreTextRasterizer, r: &GlyphRasterRequest<'_>) -> Drawn {
    let bounds = raster.bounds(r).expect("native bounds");
    let coverage = raster.coverage(r, bounds).expect("native coverage");
    assert!(raster.rasterize(r, bounds).is_some());
    Drawn { bounds, coverage }
}

#[test]
fn collection_members_rasterize_their_own_outlines() {
    let mut raster = CoreTextRasterizer::default();
    let statistics = raster.statistics();
    // Member 0's A is a 340x700-unit slanted stem; member 1's A is a 520x630-unit
    // diamond with a curved top.
    let zero = draw(&mut raster, &request(1, COLLECTION, 0, GLYPH_A, &[]));
    let one = draw(&mut raster, &request(1, COLLECTION, 1, GLYPH_A, &[]));
    let (zero_width, zero_height) = ink(&zero);
    let (one_width, one_height) = ink(&one);
    assert!((34..=36).contains(&zero_width), "{zero_width}");
    assert!((70..=72).contains(&zero_height), "{zero_height}");
    assert!((52..=54).contains(&one_width), "{one_width}");
    assert!((63..=65).contains(&one_height), "{one_height}");
    // Member 1 has a B; member 0 does not.
    assert!(
        raster
            .bounds(&request(1, COLLECTION, 1, GLYPH_B, &[]))
            .is_some()
    );
    assert!(
        raster
            .bounds(&request(1, COLLECTION, 0, GLYPH_B, &[]))
            .is_none()
    );
    // The standalone member is the same face.
    let member = sfnt::Face::parse(COLLECTION, 1)
        .unwrap()
        .standalone()
        .unwrap();
    let rebuilt = draw(&mut raster, &request(2, &member, 0, GLYPH_A, &[]));
    assert_eq!(rebuilt.coverage, one.coverage);
    let counts = statistics.counts();
    assert_eq!(counts.collection_members, 2);
    assert_eq!((counts.declined_unsupported, counts.declined_color), (0, 0));
    assert!(
        raster
            .bounds(&request(1, COLLECTION, 2, GLYPH_A, &[]))
            .is_none()
    );
    assert_eq!(statistics.counts().declined_unsupported, 1);
}

#[test]
fn variation_instances_match_the_shaper_normalized_coordinates() {
    let mut raster = CoreTextRasterizer::default();
    let statistics = raster.statistics();
    let width = |raster: &mut CoreTextRasterizer, coords: &[i16]| {
        ink(&draw(raster, &request(3, VARIABLE, 0, GLYPH_B, coords))).0
    };
    // B is a 520-unit diamond whose right half moves +160 units at the
    // heavy end and -90 at the light end.
    let default = width(&mut raster, &[]);
    assert!((52..=54).contains(&default), "{default}");
    assert_eq!(width(&mut raster, &[0]), default);
    let heavy = width(&mut raster, &[16384]);
    assert!((68..=70).contains(&heavy), "{heavy}");
    let light = width(&mut raster, &[-16384]);
    assert!((43..=45).contains(&light), "{light}");
    // Normalized 0.75 is user 650 through the fixture's avar map; a linear
    // reading (user 775) would normalize to 0.875 and widen by 14 px.
    let mapped = width(&mut raster, &[12288]);
    assert!((64..=66).contains(&mapped), "{mapped}");
    let counts = statistics.counts();
    assert_eq!(counts.variation_instances, 3);
    assert_eq!(counts.declined_unsupported, 0);
    // A repeated instance is reused.
    width(&mut raster, &[16384]);
    assert_eq!(statistics.counts().variation_instances, 3);
    // More coordinates than axes are refused, not truncated.
    assert!(
        raster
            .bounds(&request(3, VARIABLE, 0, GLYPH_B, &[1, 2]))
            .is_none()
    );
    assert_eq!(statistics.counts().declined_unsupported, 1);
}

#[test]
fn variant_instances_are_bounded_per_face_and_by_the_shared_allowance() {
    let mut raster = CoreTextRasterizer::default();
    let coords = (1..=MAX_FACE_VARIANTS as i16 + 4)
        .map(|i| i * 512)
        .collect::<Vec<_>>();
    for coord in &coords {
        assert!(
            raster
                .bounds(&request(
                    4,
                    VARIABLE,
                    0,
                    GLYPH_A,
                    std::slice::from_ref(coord)
                ))
                .is_some()
        );
    }
    let face = &raster.faces[&(4, 0)];
    assert_eq!(
        face.variable.as_ref().unwrap().variants.len(),
        MAX_FACE_VARIANTS
    );
    assert_eq!(face.bytes, raster.bytes);
    assert_eq!(
        raster.bytes,
        std::mem::size_of::<Face>() + 2 * VARIABLE.len() + MAX_FACE_VARIANTS * added_for(&[0])
    );
    let mut small = request(5, VARIABLE, 0, GLYPH_A, &[]);
    small.cache_bytes = VARIABLE.len();
    let mut refused = CoreTextRasterizer::default();
    assert!(refused.bounds(&small).is_none());
    assert_eq!(refused.statistics().counts().declined_resource, 1);
    assert_eq!(refused.retained_bytes(), 0);
}

/// Replace one table tag in the directory of a single face.
fn retag(data: &[u8], from: [u8; 4], to: [u8; 4]) -> Vec<u8> {
    let mut bytes = data.to_vec();
    let count = usize::from(u16::from_be_bytes([bytes[4], bytes[5]]));
    let at = (0..count)
        .map(|i| 12 + i * 16)
        .find(|at| bytes[*at..at + 4] == from)
        .unwrap();
    bytes[at..at + 4].copy_from_slice(&to);
    bytes
}

#[test]
fn color_optical_size_and_malformed_faces_keep_outlines_with_explicit_counts() {
    let mut raster = CoreTextRasterizer::default();
    let statistics = raster.statistics();
    let color = retag(VARIABLE, *b"post", *b"COLR");
    assert!(
        raster
            .bounds(&request(6, &color, 0, GLYPH_A, &[]))
            .is_none()
    );
    assert!(
        raster
            .bounds(&request(6, &color, 0, GLYPH_B, &[]))
            .is_none()
    );
    // The declined face is retained, so each request is counted once more
    // without parsing again.
    assert_eq!(statistics.counts().declined_color, 2);
    let mut optical = VARIABLE.to_vec();
    let fvar = optical.windows(4).position(|w| w == b"wght").unwrap();
    optical[fvar..fvar + 4].copy_from_slice(&OPTICAL_SIZE_AXIS);
    assert!(
        raster
            .bounds(&request(7, &optical, 0, GLYPH_A, &[]))
            .is_none()
    );
    assert_eq!(statistics.counts().declined_optical_size, 1);
    assert!(
        raster
            .bounds(&request(8, &VARIABLE[..40], 0, GLYPH_A, &[]))
            .is_none()
    );
    let mut invalid = request(9, VARIABLE, 0, GLYPH_A, &[]);
    invalid.phase_x = MAX_PHASE + 1;
    assert!(raster.bounds(&invalid).is_none());
    let counts = statistics.counts();
    assert_eq!(
        (counts.declined_unsupported, counts.declined_request),
        (1, 1)
    );
    assert_eq!(counts.variation_instances + counts.collection_members, 0);
}

struct Denied;
impl varta_view::Broker for Denied {
    fn link_requested(&self, _: &str) -> bool {
        false
    }
    fn open_url(&mut self, _: &str) -> varta_view::Access {
        varta_view::Access::Denied
    }
    fn clipboard_write(&mut self) -> varta_view::Access {
        varta_view::Access::Denied
    }
}

/// Render text in `family` at each weight through the ordinary renderer.
fn render(family: &str, weights: &[&str]) -> (varta_view::RenderStats, CoreTextCounts) {
    use varta_common::view::{CssProperty as P, Tag};
    let font = varta_view::TextMeasurer::new_with_fonts(vec![
        varta_view::TextFont {
            data: VARIABLE.to_vec(),
            family: "Varta Variable Fixture".into(),
            aliases: Vec::new(),
        },
        varta_view::TextFont {
            data: COLLECTION.to_vec(),
            family: "Varta Collection One".into(),
            aliases: Vec::new(),
        },
    ])
    .unwrap();
    let mut b = varta_view::BatchWriter::new();
    for (index, weight) in weights.iter().enumerate() {
        let id = 2 + 2 * index as u32;
        b.create_element(id, Tag::Div)
            .insert(1, id, 0)
            .set_style(id, P::FontFamily, &format!("\"{family}\""))
            .set_style(id, P::FontSize, "20px")
            .set_style(id, P::LineHeight, "24px")
            .set_style(id, P::FontWeight, weight)
            .create_text(id + 1, "AB BA")
            .insert(id, id + 1, 0);
    }
    let mut view = varta_view::View::default();
    view.commit(&b.finish(), false, &mut Denied).unwrap();
    let mut renderer = varta_view::Renderer::new(Some(font), 16.0).unwrap();
    let raster = CoreTextRasterizer::default();
    let statistics = raster.statistics();
    renderer.set_glyph_rasterizer(Some(Box::new(raster)));
    renderer.render(&view, 200, 120, 1.0).unwrap();
    (renderer.stats(), statistics.counts())
}

#[test]
fn renderer_uses_native_coverage_for_variable_weights_and_collection_members() {
    let (stats, counts) = render("Varta Variable Fixture", &["400", "500", "700"]);
    assert!(stats.raster_glyphs > 0);
    assert_eq!(stats.outline_glyphs, 0);
    // 500 and 700 select `wght` instances; 400 is the default instance.
    assert_eq!(
        counts,
        CoreTextCounts {
            variation_instances: 2,
            ..CoreTextCounts::default()
        }
    );
    let (stats, counts) = render("Varta Collection One", &["400"]);
    assert!(stats.raster_glyphs > 0);
    assert_eq!(stats.outline_glyphs, 0);
    assert_eq!(
        counts,
        CoreTextCounts {
            collection_members: 1,
            ..CoreTextCounts::default()
        }
    );
}
