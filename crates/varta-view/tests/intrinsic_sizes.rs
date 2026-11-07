//! CSS Sizing intrinsic sizes use content measurement, not `auto`.
use varta_common::view::Tag;
use varta_view::{Access, BatchWriter as B, Broker, View};
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
#[test]
fn fit_content_shrinks_to_content_and_stays_within_the_available_width() {
    let mut b = B::new();
    b.stylesheet(1, ".parent{width:200px}.fit{width:fit-content;padding:4px;border:2px solid black}.child{width:60px;height:10px}")
        .create_element(2,Tag::Div).set_class(2,"parent").insert(1,2,0)
        .create_element(3,Tag::Div).set_class(3,"fit").insert(2,3,0)
        .create_element(4,Tag::Div).set_class(4,"child").insert(3,4,0);
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    let boxes = v.layout_boxes(300.0, 200.0).unwrap();
    assert_eq!(boxes[&2].width, 200.0);
    assert_eq!(boxes[&3].width, 72.0);
    assert_eq!(boxes[&4].width, 60.0);
}
#[test]
fn intrinsic_text_sizes_preserve_min_max_and_fit_content_relationships() {
    let mut b = B::new();
    b.stylesheet(1,".parent{width:40px;font-family:'Varta Case';font-size:16px}.min{width:min-content}.max{width:max-content}.fit{width:fit-content}")
        .create_element(2,Tag::Div).set_class(2,"parent").insert(1,2,0);
    for (id, class) in [(3, "min"), (5, "max"), (7, "fit")] {
        b.create_element(id, Tag::Div)
            .set_class(id, class)
            .insert(2, id, 0)
            .create_text(id + 1, "aaa aaa aaa")
            .insert(id, id + 1, 0);
    }
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("../../varta-style/tests/fixtures/case.ttf").to_vec(),
        "Varta Case",
    )
    .unwrap();
    let boxes = v
        .layout_boxes_with_text(300.0, 200.0, &mut font, 16.0)
        .unwrap();
    let (min, max, fit) = (boxes[&3], boxes[&5], boxes[&7]);
    assert!(min.width > 0.0 && max.width > 40.0 && min.width < max.width);
    assert_eq!(fit.width, 40.0_f32.max(min.width).min(max.width));
    assert!(fit.height >= max.height);
}

#[test]
fn fit_content_uses_the_same_clamp_in_flex_grid_and_absolute_layout() {
    for parent in [
        "display:flex",
        "display:flex;flex-direction:column",
        "display:grid;grid-template-columns:40px",
        "position:relative",
    ] {
        let absolute = parent == "position:relative";
        let mut b = B::new();
        b.stylesheet(1,&format!(".parent{{width:40px;height:100px;font-family:'Varta Case';font-size:16px;{parent}}}.fit{{width:fit-content;{}}}",if absolute {"position:absolute;left:0;top:0"} else {"flex-shrink:0"}))
            .create_element(2,Tag::Div).set_class(2,"parent").insert(1,2,0)
            .create_element(3,Tag::Div).set_class(3,"fit").insert(2,3,0)
            .create_text(4,"aaa aaa aaa").insert(3,4,0);
        let mut v = View::default();
        v.commit(&b.finish(), false, &mut Deny).unwrap();
        let mut font = varta_view::TextMeasurer::new(
            include_bytes!("../../varta-style/tests/fixtures/case.ttf").to_vec(),
            "Varta Case",
        )
        .unwrap();
        let boxes = v
            .layout_boxes_with_text(300.0, 200.0, &mut font, 16.0)
            .unwrap();
        assert_eq!(boxes[&3].width, 40.0, "{parent}");
    }
}

#[test]
fn intrinsic_heights_and_stretch_preserve_collapsing_margin_rules() {
    for (keyword, expected) in [
        ("min-content", 14.0),
        ("max-content", 14.0),
        ("fit-content", 14.0),
        ("stretch", 50.0),
    ] {
        let mut b = B::new();
        b.stylesheet(1,&format!(".parent{{width:60px;height:50px}}.box{{width:stretch;height:{keyword};margin:3px;padding:2px;border:1px solid black}}.child{{width:8px;height:8px}}"))
            .create_element(2,Tag::Div).set_class(2,"parent").insert(1,2,0)
            .create_element(3,Tag::Div).set_class(3,"box").insert(2,3,0)
            .create_element(4,Tag::Div).set_class(4,"child").insert(3,4,0);
        let mut v = View::default();
        v.commit(&b.finish(), false, &mut Deny).unwrap();
        let boxes = v.layout_boxes(300.0, 200.0).unwrap();
        assert_eq!(boxes[&3].width, 54.0, "{keyword}");
        // In normal block flow the unpadded parent's block-axis margins
        // collapse; stretch treats those margins as zero (Sizing 4).
        assert_eq!(boxes[&3].height, expected, "{keyword}");
        let mut update = B::new();
        update.set_style(2, varta_common::view::CssProperty::Padding, "1px");
        v.commit(&update.finish(), false, &mut Deny).unwrap();
        let boxes = v.layout_boxes(300.0, 200.0).unwrap();
        assert_eq!(
            boxes[&3].height,
            if keyword == "stretch" { 44.0 } else { 14.0 },
            "noncollapsing {keyword}"
        );
    }
}
