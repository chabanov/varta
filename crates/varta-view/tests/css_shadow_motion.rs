//! Shadow/ring interpolation must affect actual ink and retained damage, not
//! layout/hits or the static snapshot. Chrome supplies the independent oracle.
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, PreparedFrame, Renderer, View};
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
const GEOMETRY: &str = ".box{position:absolute;left:30px;top:30px;width:30px;height:20px}";
fn view(sheet: &str) -> View {
    let mut b = B::new();
    b.stylesheet(1, &format!("{GEOMETRY}{sheet}"))
        .create_element(2, Tag::Div)
        .set_class(2, "box")
        .insert(1, 2, 0);
    let mut view = View::default();
    view.commit(&b.finish(), false, &mut Deny).unwrap();
    view
}
fn set(v: &mut View, p: P, text: &str) {
    let mut b = B::new();
    b.set_style(2, p, text);
    v.commit(&b.finish(), false, &mut Deny).unwrap();
}
fn expected(v: &View, shadow: &str, extra: &str) {
    let other = view(&format!(".box{{box-shadow:{shadow};{extra}}}"));
    let actual = v.rasterize_boxes(100, 100).unwrap();
    assert_eq!(
        actual.pixels,
        other.rasterize_boxes(100, 100).unwrap().pixels
    );
    assert_eq!(actual.hit_test(28.0, 40.0), None);
    assert_eq!(actual.hit_test(40.0, 40.0), Some(2));
}
const LINEAR: &str = ".box{transition:box-shadow 100ms linear}";
#[test]
fn hover_from_none_pads_outer_and_inset_shadows_without_changing_snapshots() {
    let mut v = view(&format!(
        "{LINEAR}.box:hover{{box-shadow:0 0 0 8px red,inset 8px 0 blue}}"
    ));
    assert!(!v.animating());
    v.set_pointer_state(Some(2), None);
    v.set_animation_time(50.0);
    assert!(v.animating());
    expected(&v, "0 0 0 4px #ff000080,inset 4px 0 #0000ff80", "");
    assert_eq!(v.layout_boxes(100.0, 100.0).unwrap()[&2].width, 30.0);
    assert!(v.snapshot().to_string().contains("8px"));
    v.set_animation_time(100.0);
    assert!(!v.animating());
    expected(&v, "0 0 0 8px red,inset 8px 0 blue", "");
}
#[test]
fn ring_lists_reverse_from_current_ink_then_cancel_without_stale_damage() {
    let mut v = view(&format!("{LINEAR}.box:hover{{box-shadow:0 0 0 8px red}}"));
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    v.set_pointer_state(Some(2), None);
    v.set_animation_time(40.0);
    renderer.render(&v, 100, 100, 1.0).unwrap();
    v.set_pointer_state(None, None);
    v.set_animation_time(60.0);
    expected(&v, "0 0 0 1.6px rgba(255,0,0,.2)", "");
    let reverse = renderer.render(&v, 100, 100, 1.0).unwrap();
    assert_eq!(reverse.pixels, v.rasterize_boxes(100, 100).unwrap().pixels);
    assert!(renderer.stats().dirty_pixels > 0);
    set(&mut v, P::TransitionProperty, "none");
    assert!(!v.animating());
    let cancelled = renderer.render(&v, 100, 100, 1.0).unwrap();
    assert!(cancelled.pixels.iter().all(|p| *p == 0));
    renderer.render(&v, 100, 100, 1.0).unwrap();
    assert_eq!(renderer.stats().dirty_pixels, 0);
}
#[test]
fn differing_inset_pairs_do_not_start_a_transition_but_keyframes_switch_discretely() {
    let mut v = view(&format!("{LINEAR}.box{{box-shadow:0 0 0 4px red}}"));
    set(&mut v, P::BoxShadow, "inset 4px 0 blue");
    assert!(!v.animating());
    expected(&v, "inset 4px 0 blue", "");
    let mut v = view(
        "@keyframes shadow{from{box-shadow:0 0 0 4px red}to{box-shadow:inset 4px 0 blue}}.box{animation:shadow 100ms linear forwards}",
    );
    v.set_animation_time(49.0);
    expected(&v, "0 0 0 4px red", "");
    v.set_animation_time(50.0);
    expected(&v, "inset 4px 0 blue", "");
}
#[test]
fn currentcolor_tracks_animated_color_and_its_change_alone_starts_no_shadow_transition() {
    let mut v = view(&format!("{LINEAR}.box{{color:red;box-shadow:0 0 0 4px}}"));
    set(&mut v, P::Color, "blue");
    assert!(!v.animating());
    expected(&v, "0 0 0 4px blue", "");
    let mut v = view(".box{color:red;box-shadow:0 0 0 4px;transition:all 100ms linear}");
    set(&mut v, P::Color, "blue");
    v.set_animation_time(50.0);
    expected(&v, "0 0 0 4px rgb(128,0,128)", "");
    // A running shadow keeps currentcolor live too, including after retargeting.
    let mut v = view(&format!(
        "{LINEAR}.box{{color:red;box-shadow:0 0 0 4px currentcolor}}"
    ));
    set(&mut v, P::BoxShadow, "0 0 0 4px blue");
    v.set_animation_time(50.0);
    set(&mut v, P::Color, "lime");
    expected(&v, "0 0 0 4px rgb(0,128,128)", "");
    set(&mut v, P::BoxShadow, "0 0 0 4px currentcolor");
    v.set_animation_time(75.0);
    expected(&v, "0 0 0 4px rgb(0,191,64)", "");
}
#[test]
fn delayed_unequal_lists_keep_blur_alpha_and_scaled_retained_fallback() {
    let mut v = view(
        ".box{background-color:white;border-radius:6px;box-shadow:0 0 0 2px red;transition:box-shadow 100ms linear 20ms}",
    );
    set(
        &mut v,
        P::BoxShadow,
        "4px -2px 8px -2px rgba(0,0,255,.5),inset 4px 0 2px blue",
    );
    v.set_animation_time(10.0);
    expected(
        &v,
        "0 0 0 2px red",
        "background-color:white;border-radius:6px",
    );
    v.set_animation_time(70.0);
    expected(
        &v,
        "2px -1px 4px 0 rgba(170,0,85,.75),inset 2px 0 1px rgba(0,0,255,.5)",
        "background-color:white;border-radius:6px",
    );
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    for scale in [1.0, 1.25, 2.0] {
        let reference = v.rasterize_scaled(100, 100, scale, None).unwrap();
        assert_eq!(
            renderer.render(&v, 100, 100, scale).unwrap().pixels,
            reference.pixels
        );
        assert!(matches!(
            renderer.prepare(&v, 100, 100, scale).unwrap(),
            PreparedFrame::Raster(_)
        ));
    }
}
#[test]
fn shadow_keyframes_use_underlying_endpoints_fill_and_important() {
    let mut v = view(
        "@keyframes ring{50%{box-shadow:0 0 0 8px red}}.box{animation:ring 100ms linear forwards}",
    );
    v.set_animation_time(25.0);
    expected(&v, "0 0 0 4px rgba(255,0,0,.5)", "");
    v.set_animation_time(75.0);
    expected(&v, "0 0 0 4px rgba(255,0,0,.5)", "");
    v.set_animation_time(100.0);
    expected(&v, "none", "");
    assert!(!v.animating());
    let mut b = B::new();
    b.stylesheet(2, ".box{box-shadow:0 0 0 2px blue!important}");
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    expected(&v, "0 0 0 2px blue", "");
}

#[test]
fn unresolved_currentcolor_mixes_in_shadow_keyframes_are_refused_explicitly() {
    let v = view(
        "@keyframes mixed{to{box-shadow:0 0 0 4px color-mix(in oklab,currentcolor,red)}}.box{animation:mixed 100ms linear}",
    );
    assert!(v.rasterize_boxes(100, 100).is_err());
}

#[test]
fn variable_transition_shorthand_preserves_longhand_cascade() {
    let mut v = view(
        ":root{--motion:box-shadow 100ms linear}.box{transition:var(--motion);transition-duration:200ms}.box:hover{box-shadow:0 0 0 8px red}",
    );
    v.set_pointer_state(Some(2), None);
    v.set_animation_time(100.0);
    expected(&v, "0 0 0 4px rgba(255,0,0,.5)", "");
    v.set_animation_time(200.0);
    assert!(!v.animating());
}
