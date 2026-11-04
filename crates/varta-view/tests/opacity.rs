use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, Renderer, View};
struct Deny;
impl Broker for Deny {
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
fn commit(view: &mut View, b: &B) {
    view.commit(&b.finish(), false, &mut Deny).unwrap();
}
fn set(view: &mut View, id: u32, p: P, value: &str) {
    let mut b = B::new();
    b.set_style(id, p, value);
    commit(view, &b);
}
fn boxes() -> View {
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#ffffff")
        .create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "40px")
        .set_style(2, P::Height, "40px")
        .set_style(2, P::Margin, "10px")
        .set_style(2, P::Padding, "1px")
        .set_style(2, P::BackgroundColor, "#ff0000")
        .create_element(3, Tag::Div)
        .insert(2, 3, 0)
        .set_style(3, P::Width, "20px")
        .set_style(3, P::Height, "20px")
        .set_style(3, P::Margin, "10px")
        .set_style(3, P::BackgroundColor, "#0000ff");
    let mut v = View::default();
    commit(&mut v, &b);
    v
}
fn pixel(image: &varta_view::RasterImage, x: u32, y: u32) -> [u8; 4] {
    let offset = (y as usize * image.width as usize + x as usize) * 4;
    image.pixels[offset..offset + 4].try_into().unwrap()
}
fn near(actual: [u8; 4], expected: [u8; 4]) {
    assert!(
        actual
            .into_iter()
            .zip(expected)
            .all(|(a, b)| a.abs_diff(b) <= 1),
        "{actual:?} != {expected:?}"
    );
}
fn overlap(nested: bool) -> View {
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#ffffff");
    if nested {
        b.create_element(4, Tag::Div)
            .insert(1, 4, 0)
            .set_style(4, P::Width, "40px")
            .set_style(4, P::Height, "20px");
    }
    b.create_element(2, Tag::Div)
        .insert(if nested { 4 } else { 1 }, 2, 0)
        .set_style(2, P::Width, "40px")
        .set_style(2, P::Height, "20px")
        .set_style(2, P::BackgroundColor, "#ff0000")
        .set_style(2, P::Opacity, "0.5")
        .create_element(3, Tag::Div)
        .insert(1, 3, 0)
        .set_style(3, P::Width, "40px")
        .set_style(3, P::Height, "20px")
        .set_style(3, P::BackgroundColor, "#0000ff")
        .set_style(3, P::MarginTop, "-20px");
    let mut v = View::default();
    commit(&mut v, &b);
    v
}
#[test]
fn static_opacity_context_paints_and_hits_above_later_ordinary_blocks() {
    for nested in [false, true] {
        let mut v = overlap(nested);
        let before = v.snapshot();
        let image = v.rasterize_boxes(40, 40).unwrap();
        near(pixel(&image, 10, 10), [128, 0, 128, 255]);
        assert_eq!(image.hit_test(10.0, 10.0), Some(2));
        assert_eq!(v.snapshot(), before);
        set(&mut v, 2, P::Opacity, "0");
        let hidden = v.rasterize_boxes(40, 40).unwrap();
        assert_eq!(pixel(&hidden, 10, 10), [0, 0, 255, 255]);
        assert_eq!(hidden.hit_test(10.0, 10.0), Some(2));
        assert!(image.same_input_geometry(&hidden));
        set(&mut v, 2, P::Opacity, "1");
        let ordinary = v.rasterize_boxes(40, 40).unwrap();
        assert_eq!(pixel(&ordinary, 10, 10), [0, 0, 255, 255]);
        assert_eq!(ordinary.hit_test(10.0, 10.0), Some(3));
        assert!(!ordinary.same_input_geometry(&hidden));
    }
}
#[test]
fn context_lifting_preserves_ancestor_curves_and_cannot_escape_an_opacity_context() {
    let mut v = overlap(true);
    set(&mut v, 4, P::Overflow, "hidden");
    set(&mut v, 4, P::BorderRadius, "50%");
    let image = v.rasterize_boxes(40, 40).unwrap();
    near(pixel(&image, 20, 10), [128, 0, 128, 255]);
    assert_eq!(image.hit_test(20.0, 10.0), Some(2));
    assert_eq!(pixel(&image, 0, 0), [0, 0, 255, 255]);
    assert_eq!(image.hit_test(0.5, 0.5), Some(3));
    set(&mut v, 4, P::Opacity, "0.5");
    // The outer context contains its red context atomically and lifts as one.
    let nested = v.rasterize_boxes(40, 40).unwrap();
    near(pixel(&nested, 20, 10), [64, 0, 191, 255]);
    assert_eq!(nested.hit_test(20.0, 10.0), Some(2));
    set(&mut v, 3, P::Opacity, "0.5");
    let later = v.rasterize_boxes(40, 40).unwrap();
    near(pixel(&later, 20, 10), [127, 95, 223, 255]);
    assert_eq!(later.hit_test(20.0, 10.0), Some(3));
}
#[test]
fn retained_context_reordering_handles_opacity_changes_and_vector_recovery() {
    let mut v = overlap(true);
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    for alpha in ["0.5", "0", "1", "0.75", "1"] {
        set(&mut v, 2, P::Opacity, alpha);
        let expected = v.rasterize_boxes(40, 40).unwrap();
        assert_eq!(renderer.render(&v, 40, 40, 1.0).unwrap(), expected);
        assert_eq!(renderer.render(&v, 40, 40, 1.0).unwrap(), expected);
        assert_eq!(renderer.stats().dirty_pixels, 0);
        let prepared = renderer.prepare(&v, 40, 40, 1.0).unwrap();
        let actual = match prepared {
            varta_view::PreparedFrame::Raster(image) => image,
            varta_view::PreparedFrame::Vector { metadata, scene } => {
                scene.rasterize(metadata).unwrap()
            }
        };
        assert_eq!(actual, expected);
    }
}
#[test]
fn parent_opacity_applies_once_after_opaque_and_translucent_child_overlap() {
    let mut v = boxes();
    set(&mut v, 2, P::Opacity, "0.5");
    let before = v.snapshot();
    let image = v.rasterize_boxes(80, 80).unwrap();
    assert_eq!(
        image.hit_test(30.0, 30.0),
        Some(3),
        "overlap fixture must sample the child"
    );
    near(pixel(&image, 30, 30), [127, 127, 255, 255]);
    near(pixel(&image, 15, 15), [255, 127, 127, 255]);
    assert_eq!(pixel(&image, 0, 0), [255; 4]);
    assert_eq!(v.snapshot(), before);
    set(&mut v, 3, P::BackgroundColor, "#0000ff80");
    near(
        pixel(&v.rasterize_boxes(80, 80).unwrap(), 30, 30),
        [191, 127, 191, 255],
    );
}
#[test]
fn nested_opacity_composes_groups_and_zero_keeps_pointer_geometry() {
    let mut v = boxes();
    let full = v.rasterize_boxes(80, 80).unwrap();
    set(&mut v, 2, P::Opacity, "0.5");
    set(&mut v, 3, P::Opacity, "0.5");
    let nested = v.rasterize_boxes(80, 80).unwrap();
    near(pixel(&nested, 30, 30), [191, 127, 191, 255]);
    assert!(full.same_input_geometry(&nested));
    set(&mut v, 2, P::Opacity, "0");
    let hidden = v.rasterize_boxes(80, 80).unwrap();
    assert_eq!(pixel(&hidden, 30, 30), [255; 4]);
    assert_eq!(hidden.hit_test(30.0, 30.0), Some(3));
    assert!(full.same_input_geometry(&hidden));
}
#[test]
fn root_group_includes_its_background_without_inventing_an_opaque_canvas() {
    let mut v = boxes();
    set(&mut v, 1, P::Opacity, "0.5");
    let image = v.rasterize_boxes(80, 80).unwrap();
    near(pixel(&image, 0, 0), [128, 128, 128, 128]);
    near(pixel(&image, 30, 30), [0, 0, 128, 128]);
}
fn clipped(curved: bool) -> View {
    let mut b = B::new();
    b.create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, if curved { "40px" } else { "40.5px" })
        .set_style(2, P::Height, "40px")
        .set_style(2, P::Margin, "10.25px")
        .set_style(2, P::Overflow, "hidden")
        .set_style(2, P::BorderRadius, if curved { "50%" } else { "0" })
        .create_element(3, Tag::Div)
        .insert(2, 3, 0)
        .set_style(3, P::Width, "60px")
        .set_style(3, P::Height, "60px")
        .set_style(3, P::Margin, "-5px")
        .set_style(3, P::BackgroundColor, "#ff0000")
        .create_element(4, Tag::Div)
        .insert(3, 4, 0)
        .set_style(4, P::Width, "60px")
        .set_style(4, P::Height, "60px")
        .set_style(4, P::BackgroundColor, "#0000ff");
    let mut v = View::default();
    commit(&mut v, &b);
    v
}
#[test]
fn inherited_curved_and_fractional_rect_clips_apply_after_overlap_once() {
    for curved in [false, true] {
        let mut v = clipped(curved);
        set(&mut v, 3, P::Opacity, "0.5");
        let actual = v.rasterize_boxes(90, 90).unwrap();
        let mut b = B::new();
        b.remove(4)
            .set_style(3, P::Opacity, "1")
            .set_style(3, P::BackgroundColor, "#0000ff");
        commit(&mut v, &b);
        let reference = v.rasterize_boxes(90, 90).unwrap();
        for (a, b) in actual
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(reference.pixels.as_chunks::<4>().0)
        {
            assert_eq!(
                a[0], 0,
                "red cannot leak through overlapping blue inside one group"
            );
            assert_eq!(a[1], 0);
            assert!(
                a[2].abs_diff(u16::from(b[2]).div_ceil(2) as u8) <= 1,
                "curved={curved} blue {a:?}/{b:?}"
            );
            assert!(
                a[3].abs_diff(u16::from(b[3]).div_ceil(2) as u8) <= 1,
                "curved={curved} alpha {a:?}/{b:?}"
            );
        }
    }
}
#[test]
fn empty_inherited_viewport_cannot_become_visible_when_a_group_unclips_its_children() {
    let mut v = clipped(false);
    set(&mut v, 2, P::Height, "0");
    set(&mut v, 3, P::Opacity, "0.5");
    assert!(
        v.rasterize_boxes(90, 90)
            .unwrap()
            .pixels
            .iter()
            .all(|b| *b == 0)
    );
}
#[test]
fn retained_groups_cross_tiles_repaint_damage_and_switch_vector_fallback_without_stale_pixels() {
    let mut v = boxes();
    set(&mut v, 2, P::Width, "240px");
    set(&mut v, 2, P::Height, "180px");
    set(&mut v, 2, P::Opacity, "0.375");
    set(&mut v, 3, P::Width, "220px");
    set(&mut v, 3, P::Height, "100px");
    set(&mut v, 3, P::Margin, "30.25px");
    set(&mut v, 3, P::BackgroundColor, "#0000ffbf");
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    for scale in [1.0, 2.0] {
        let (w, h) = ((320.0 * scale) as u32, (280.0 * scale) as u32);
        let expected = v.rasterize_scaled(w, h, scale, None).unwrap();
        assert_eq!(renderer.render(&v, w, h, scale).unwrap(), expected);
        assert_eq!(renderer.render(&v, w, h, scale).unwrap(), expected);
        assert_eq!(renderer.stats().dirty_pixels, 0);
        let varta_view::PreparedFrame::Raster(actual) = renderer.prepare(&v, w, h, scale).unwrap()
        else {
            panic!("group uses explicit raster fallback under old GPU limits")
        };
        assert_eq!(actual, expected);
        assert_eq!(renderer.stats().dirty_pixels, 0);
        assert_eq!(renderer.damage(), None);
        assert_eq!(renderer.render(&v, w, h, scale).unwrap(), expected);
        assert_eq!(renderer.stats().dirty_pixels, 0);
    }
    set(&mut v, 3, P::BackgroundColor, "#00ff00bf");
    let actual = renderer.render(&v, 320, 280, 1.0).unwrap();
    assert_eq!(actual, v.rasterize_boxes(320, 280).unwrap());
    set(&mut v, 3, P::BackgroundColor, "#0000ffbf");
    let actual = renderer.render(&v, 320, 280, 1.0).unwrap();
    assert_eq!(actual, v.rasterize_boxes(320, 280).unwrap());
    assert!(renderer.stats().dirty_pixels > 0 && renderer.stats().dirty_pixels < 320 * 280);
    set(&mut v, 3, P::BackgroundColor, "#00ff00bf");
    let varta_view::PreparedFrame::Raster(actual) = renderer.prepare(&v, 320, 280, 1.0).unwrap()
    else {
        panic!("group mutation must preserve software damage replay")
    };
    assert_eq!(actual, v.rasterize_boxes(320, 280).unwrap());
    assert!(renderer.stats().dirty_pixels > 0 && renderer.stats().dirty_pixels < 320 * 280);
    assert_eq!(renderer.render(&v, 320, 280, 1.0).unwrap(), actual);
    assert_eq!(renderer.stats().dirty_pixels, 0);
    set(&mut v, 2, P::Opacity, "1");
    let varta_view::PreparedFrame::Vector { metadata, scene } =
        renderer.prepare(&v, 320, 280, 1.0).unwrap()
    else {
        panic!("ordinary vector scene after groups")
    };
    let expected = v.rasterize_boxes(320, 280).unwrap();
    assert_eq!(scene.rasterize(metadata).unwrap(), expected);
    assert_eq!(renderer.render(&v, 320, 280, 1.0).unwrap(), expected);
}

fn field_font() -> varta_view::TextMeasurer {
    varta_view::TextMeasurer::new(
        include_bytes!("fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap()
}
fn add_field(b: &mut B, id: u32, parent: u32, value: &str) {
    b.create_element(id, Tag::Input)
        .insert(parent, id, 0)
        .set_value(id, value, 0);
    for (property, value) in [
        (P::Display, "block"),
        (P::Width, "120px"),
        (P::Height, "30px"),
        (P::MinWidth, "0"),
        (P::MinHeight, "0"),
        (P::Padding, "2px"),
        (P::Border, "1px solid #000000"),
        (P::BorderRadius, "0"),
        (P::Outline, "none"),
        (P::BackgroundColor, "#ffffff"),
        (P::Color, "#000000"),
        (P::FontFamily, "\"Varta Fixture 400\""),
        (P::FontSize, "20px"),
        (P::FontWeight, "400"),
        (P::FontStyle, "normal"),
        (P::LineHeight, "20px"),
    ] {
        b.set_style(id, property, value);
    }
}
#[test]
fn full_size_fields_overflow_the_command_cache_without_partial_group_alpha_or_lost_following_draws()
{
    let mut b = B::new();
    b.create_element(10, Tag::Div)
        .insert(1, 10, 0)
        .set_style(10, P::Width, "160px")
        .set_style(10, P::Height, "5px")
        .set_style(10, P::BackgroundColor, "#00ff00")
        .create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "140px")
        .set_style(2, P::Height, "70px");
    let value = "A".repeat(32768);
    add_field(&mut b, 3, 2, &value);
    add_field(&mut b, 4, 2, &value);
    b.create_element(11, Tag::Div)
        .insert(1, 11, 0)
        .set_style(11, P::Width, "160px")
        .set_style(11, P::Height, "5px")
        .set_style(11, P::BackgroundColor, "#0000ff");
    let mut v = View::default();
    commit(&mut v, &b);
    let reference = v
        .rasterize_with_text(160, 90, &mut field_font(), 20.0)
        .unwrap();
    set(&mut v, 2, P::Opacity, "0.5");
    let before = v.snapshot();
    let mut renderer = Renderer::new(Some(field_font()), 20.0).unwrap();
    let actual = renderer.render(&v, 160, 90, 1.0).unwrap();
    // Forming a context changes the stored hit order conservatively. These
    // disjoint fixtures must nevertheless retain every bound and actual target.
    assert_eq!(reference.node_bounds_map(), actual.node_bounds_map());
    for y in 0..90 {
        for x in 0..160 {
            assert_eq!(
                reference.hit_test(x as f32 + 0.25, y as f32 + 0.25),
                actual.hit_test(x as f32 + 0.25, y as f32 + 0.25)
            );
        }
    }
    assert_eq!(v.snapshot(), before);
    for y in 5..75 {
        for x in 0..160 {
            for (a, b) in pixel(&actual, x, y)
                .into_iter()
                .zip(pixel(&reference, x, y))
            {
                assert!(a.abs_diff(u16::from(b).div_ceil(2) as u8) <= 1, "{x},{y}");
            }
        }
    }
    assert_eq!(pixel(&actual, 3, 2), [0, 255, 0, 255]);
    assert_eq!(pixel(&actual, 3, 77), [0, 0, 255, 255]);
    let varta_view::PreparedFrame::Raster(prepared) = renderer.prepare(&v, 160, 90, 1.0).unwrap()
    else {
        panic!("uncached isolated program returns pixels")
    };
    assert_eq!(prepared, actual);
    assert_eq!(renderer.render(&v, 160, 90, 1.0).unwrap(), actual);
    assert_eq!(
        renderer.stats().dirty_pixels,
        160 * 90,
        "oversize program is not retained as a larger cache"
    );
    let mut small = B::new();
    small.set_value(3, "A", 0).set_value(4, "A", 0);
    commit(&mut v, &small);
    renderer.render(&v, 160, 90, 1.0).unwrap();
    renderer.render(&v, 160, 90, 1.0).unwrap();
    assert_eq!(
        renderer.stats().dirty_pixels,
        0,
        "cache reuse recovers on a smaller program"
    );
}
#[test]
fn shared_clip_payload_overflow_preserves_uncached_group_pixels_and_field_geometry() {
    let mut b = B::new();
    b.create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "140px")
        .set_style(2, P::Height, "60px");
    let mut parent = 2;
    for id in 3..43 {
        b.create_element(id, Tag::Div)
            .insert(parent, id, 0)
            .set_style(id, P::Width, "140px")
            .set_style(id, P::Height, "60px")
            .set_style(id, P::Overflow, "hidden")
            .set_style(
                id,
                P::BorderRadius,
                &format!("{}%", 50.0 - (id - 3) as f32 / 100.0),
            );
        parent = id;
    }
    add_field(&mut b, 50, parent, &"A".repeat(2000));
    let mut v = View::default();
    commit(&mut v, &b);
    let reference = v
        .rasterize_with_text(160, 90, &mut field_font(), 20.0)
        .unwrap();
    set(&mut v, 2, P::Opacity, "0.5");
    let mut renderer = Renderer::new(Some(field_font()), 20.0).unwrap();
    let actual = renderer.render(&v, 160, 90, 1.0).unwrap();
    assert!(reference.same_input_geometry(&actual));
    for (a, b) in actual.pixels.iter().zip(&reference.pixels) {
        assert!(a.abs_diff(u16::from(*b).div_ceil(2) as u8) <= 1);
    }
    assert_eq!(renderer.render(&v, 160, 90, 1.0).unwrap(), actual);
    assert_eq!(
        renderer.stats().dirty_pixels,
        160 * 90,
        "shared clip payload overflow cannot become a retained larger scene"
    );
}

#[test]
fn a_fractional_coplanar_edge_does_not_multiply_coverage_when_other_content_requires_clipping() {
    let mut v = clipped(false);
    set(&mut v, 3, P::Margin, "0");
    let mut b = B::new();
    b.remove(4).set_style(3, P::BackgroundColor, "#0000ff");
    commit(&mut v, &b);
    let reference = v.rasterize_boxes(90, 90).unwrap();
    set(&mut v, 3, P::Opacity, "0.5");
    let actual = v.rasterize_boxes(90, 90).unwrap();
    for (a, b) in actual.pixels.iter().zip(&reference.pixels) {
        assert!(
            a.abs_diff(u16::from(*b).div_ceil(2) as u8) <= 1,
            "coplanar clip {a}/{b}"
        );
    }
}

#[test]
fn an_ordinary_cache_overflow_before_a_later_group_keeps_its_materialized_prefix() {
    let mut b = B::new();
    b.create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Width, "140px")
        .set_style(2, P::Height, "65px");
    let value = "A".repeat(32768);
    add_field(&mut b, 3, 2, &value);
    add_field(&mut b, 4, 2, &value);
    b.create_element(5, Tag::Div)
        .insert(1, 5, 0)
        .set_style(5, P::Width, "160px")
        .set_style(5, P::Height, "10px")
        .set_style(5, P::BackgroundColor, "#0000ff");
    let mut v = View::default();
    commit(&mut v, &b);
    let reference = v
        .rasterize_with_text(160, 90, &mut field_font(), 20.0)
        .unwrap();
    set(&mut v, 5, P::Opacity, "0.5");
    let mut renderer = Renderer::new(Some(field_font()), 20.0).unwrap();
    let actual = renderer.render(&v, 160, 90, 1.0).unwrap();
    // A new zero-level context changes metadata ordering: the blue block now
    // follows the atomic fields. These disjoint boxes still have identical
    // observable input targets; conservative geometry-cache invalidation is valid.
    assert!(!reference.same_input_geometry(&actual));
    assert_eq!(reference.node_bounds_map(), actual.node_bounds_map());
    for y in 0..90 {
        for x in 0..160 {
            assert_eq!(
                actual.hit_test(x as f32 + 0.25, y as f32 + 0.25),
                reference.hit_test(x as f32 + 0.25, y as f32 + 0.25)
            );
        }
    }
    for y in 0..65 {
        for x in 0..160 {
            assert_eq!(pixel(&actual, x, y), pixel(&reference, x, y));
        }
    }
    near(pixel(&actual, 3, 68), [0, 0, 128, 128]);
    let varta_view::PreparedFrame::Raster(prepared) = renderer.prepare(&v, 160, 90, 1.0).unwrap()
    else {
        panic!("materialized prefix returns a full raster")
    };
    assert_eq!(prepared, actual);
}

#[test]
fn rounded_field_selection_and_ime_geometry_stay_logical_when_its_painted_group_fades() {
    let mut b = B::new();
    add_field(&mut b, 2, 1, "AAAA");
    b.set_style(2, P::BorderRadius, "50%");
    let mut v = View::default();
    commit(&mut v, &b);
    v.input(
        &varta_view::Input::TextSelect {
            target: varta_view::Target::Node(2),
            anchor: 0,
            focus: 2,
            activate: true,
        },
        &mut Deny,
    )
    .unwrap();
    let reference = v
        .rasterize_with_text(160, 90, &mut field_font(), 20.0)
        .unwrap();
    assert!(reference.ime_cursor_area().is_some());
    set(&mut v, 2, P::Opacity, "0.5");
    let before = v.snapshot();
    let mut renderer = Renderer::new(Some(field_font()), 20.0).unwrap();
    let actual = renderer.render(&v, 160, 90, 1.0).unwrap();
    assert!(actual.same_input_geometry(&reference));
    assert_eq!(actual.ime_cursor_area(), reference.ime_cursor_area());
    assert_eq!(v.snapshot(), before);
    for (a, b) in actual.pixels.iter().zip(&reference.pixels) {
        assert!(
            a.abs_diff(u16::from(*b).div_ceil(2) as u8) <= 1,
            "field fade {a}/{b}"
        );
    }
}
