//! Numeric OpenType shaping, inheritance/reset/cache behavior and native accents.
use varta_common::view::{Attribute as A, CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, Renderer, TextMeasurer, View};
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
fn font() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("fixtures/numeric.ttf").to_vec(),
        "Varta Numeric Fixture",
    )
    .unwrap()
}
#[test]
fn numeric_variants_shape_digits_inherit_and_reset_with_font_shorthand() {
    let mut b = B::new();
    b.stylesheet(1,".parent{font-family:'Varta Numeric Fixture';font-size:20px;font-variant-numeric:tabular-nums}.digits{width:max-content}.reset{font:20px 'Varta Numeric Fixture'}")
        .create_element(2,Tag::Div).set_class(2,"parent").insert(1,2,0);
    for (id, text, class) in [
        (3, "000", "digits"),
        (5, "111", "digits"),
        (7, "000", "digits reset"),
        (9, "111", "digits reset"),
    ] {
        b.create_element(id, Tag::Div)
            .set_class(id, class)
            .insert(2, id, 0)
            .create_text(id + 1, text)
            .insert(id, id + 1, 0);
    }
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    let sizes = v
        .layout_boxes_with_text(200.0, 200.0, &mut font(), 16.0)
        .unwrap();
    assert_eq!(sizes[&3].width, 36.0);
    assert_eq!(sizes[&5].width, 36.0);
    assert_eq!(sizes[&7].width, 24.0);
    assert_eq!(sizes[&9].width, 12.0);
    let mut renderer = Renderer::new(Some(font()), 16.0).unwrap();
    let before = renderer.render(&v, 200, 200, 1.0).unwrap();
    let mut b = B::new();
    b.set_style(2, P::FontVariantNumeric, "initial");
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    let after = renderer.render(&v, 200, 200, 1.0).unwrap();
    assert_ne!(before.pixels, after.pixels);
    assert_eq!(
        after.pixels,
        v.rasterize_with_text(200, 200, &mut font(), 16.0)
            .unwrap()
            .pixels
    );
    let sizes = v
        .layout_boxes_with_text(200.0, 200.0, &mut font(), 16.0)
        .unwrap();
    assert_eq!(sizes[&3].width, 24.0);
    assert_eq!(sizes[&5].width, 12.0);
}
fn checkable(kind: &str, extra: &str, disabled: bool) -> View {
    let mut b = B::new();
    b.stylesheet(1,&format!(".parent{{accent-color:rgb(32,64,128)}}input{{position:absolute;left:10px;top:10px;width:20px;height:20px;margin:0;{extra}}}"))
        .create_element(2,Tag::Div).set_class(2,"parent").insert(1,2,0)
        .create_element(3,Tag::Input).set_attribute(3,A::Type,kind).set_checked(3,1,0).insert(2,3,0);
    if disabled {
        b.set_attribute(3, A::Disabled, "");
    }
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    v
}
fn pixel(v: &View, x: u32, y: u32) -> [u8; 4] {
    let p = v.rasterize_boxes(60, 60).unwrap();
    let at = ((y * p.width + x) * 4) as usize;
    p.pixels[at..at + 4].try_into().unwrap()
}
#[test]
fn accent_colors_inherit_reset_and_keep_disabled_theme_and_contrasting_marks() {
    assert_eq!(
        pixel(&checkable("checkbox", "", false), 12, 16),
        [32, 64, 128, 255]
    );
    assert_eq!(
        pixel(&checkable("radio", "", false), 20, 20),
        [32, 64, 128, 255]
    );
    assert_eq!(
        pixel(
            &checkable("checkbox", "accent-color:initial", false),
            12,
            16
        ),
        [0, 117, 255, 255]
    );
    assert_eq!(
        pixel(
            &checkable("checkbox", "color:magenta;accent-color:currentcolor", false),
            12,
            16
        ),
        [255, 0, 255, 255]
    );
    assert_eq!(
        pixel(&checkable("checkbox", "accent-color:yellow", false), 18, 23),
        [0, 0, 0, 255]
    );
    assert_eq!(
        pixel(&checkable("checkbox", "accent-color:red", true), 12, 16),
        pixel(&checkable("checkbox", "accent-color:blue", true), 12, 16)
    );
}

#[test]
fn pretty_wrap_improves_a_short_last_line_and_whitespace_shorthands_keep_cascade_order() {
    let mut b = B::new();
    b.stylesheet(1,".flow{width:100px;font-family:'Varta Numeric Fixture';font-size:20px;line-height:20px;text-wrap:pretty}.nowrap{white-space:nowrap}.again{white-space:nowrap;text-wrap:pretty}")
        .create_element(2,Tag::Div).set_class(2,"flow").insert(1,2,0);
    for i in 0..7 {
        let id = 3 + i * 2;
        b.create_element(id, Tag::Span)
            .insert(2, id, 0)
            .create_text(id + 1, if i == 6 { "111" } else { "111 " })
            .insert(id, id + 1, 0);
    }
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    let pretty = v
        .layout_boxes_with_text(200.0, 200.0, &mut font(), 16.0)
        .unwrap();
    assert!(pretty[&11].y > pretty[&3].y);
    let mut b = B::new();
    b.set_style(2, P::TextWrap, "wrap auto");
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    let normal = v
        .layout_boxes_with_text(200.0, 200.0, &mut font(), 16.0)
        .unwrap();
    assert_eq!(normal[&11].y, normal[&3].y);
    // Revert the inline shorthand before testing stylesheet declaration order.
    let mut b = B::new();
    b.remove_style(2, P::TextWrap).set_class(2, "flow nowrap");
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    let nowrap = v
        .layout_boxes_with_text(200.0, 200.0, &mut font(), 16.0)
        .unwrap();
    assert_eq!(nowrap[&15].y, nowrap[&3].y);
    let mut b = B::new();
    b.set_class(2, "flow again");
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    let again = v
        .layout_boxes_with_text(200.0, 200.0, &mut font(), 16.0)
        .unwrap();
    assert!(again[&11].y > again[&3].y);
}
