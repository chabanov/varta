//! Generated linear images, premultiplied interpolation and retained composition.
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

fn view(animation: &str, frames: &str) -> View {
    let mut b = B::new();
    b.stylesheet(1,&format!("@keyframes pulse{{50%{{opacity:.5}}}}@keyframes spin{{to{{transform:rotate(360deg)}}}}{frames}.b{{position:absolute;left:40px;top:40px;width:40px;height:20px;background-color:red;animation:{animation}}}"))
 .create_element(2,Tag::Div).set_class(2,"b").insert(1,2,0);
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    v
}
fn alpha(v: &View) -> u8 {
    let image = v.rasterize_boxes(120, 120).unwrap();
    image.pixels[((50 * 120 + 60) * 4 + 3) as usize]
}
#[test]
fn pulse_uses_implicit_endpoints_and_repeats_on_the_supplied_clock() {
    let mut v = view("pulse 2s cubic-bezier(.4,0,.6,1) infinite", "");
    assert!(v.animating());
    assert_eq!(alpha(&v), 255);
    v.set_animation_time(1000.0);
    assert_eq!(alpha(&v), 128);
    v.set_animation_time(1500.0);
    assert!((190..=192).contains(&alpha(&v)));
    v.set_animation_time(2000.0);
    assert_eq!(alpha(&v), 255);
    let before = v.snapshot().to_string();
    assert!(!before.contains("0.500000"));
}
#[test]
fn spin_maps_paint_geometry_and_hits_and_reuses_quiet_frames() {
    let mut v = view("spin 1s linear infinite", "");
    let mut renderer = Renderer::new(None, 16.0).unwrap();
    v.set_animation_time(250.0);
    let rect = v.layout_boxes(120.0, 120.0).unwrap()[&2];
    assert!((rect.x - 50.0).abs() < 0.001);
    assert!((rect.y - 30.0).abs() < 0.001);
    assert!((rect.width - 20.0).abs() < 0.001);
    assert!((rect.height - 40.0).abs() < 0.001);
    for scale in [1.0, 1.25, 2.0] {
        let image = renderer.render(&v, 240, 240, scale).unwrap();
        assert_eq!(
            image.pixels,
            v.rasterize_scaled(240, 240, scale, None).unwrap().pixels
        );
        assert_eq!(image.hit_test(60.0 * scale, 35.0 * scale), Some(2));
        assert_ne!(image.hit_test(45.0 * scale, 50.0 * scale), Some(2));
        assert_eq!(
            renderer.render(&v, 240, 240, scale).unwrap().pixels,
            image.pixels
        );
        assert_eq!(renderer.stats().dirty_pixels, 0);
    }
    v.set_animation_time(125.0); // Clock is monotonic.
    v.set_animation_time(375.0);
    let image = renderer.render(&v, 120, 120, 1.0).unwrap();
    assert_ne!(image.hit_test(40.0, 30.0), Some(2));
    assert_eq!(image.pixels[((30 * 120 + 40) * 4 + 3) as usize], 0);
}
#[test]
fn pause_delay_fill_direction_and_important_obey_cascade() {
    let mut v = view("pulse 1s linear 500ms 2 alternate both", "");
    assert_eq!(alpha(&v), 255);
    v.set_animation_time(1000.0);
    assert_eq!(alpha(&v), 128);
    let mut b = B::new();
    b.set_style(2, P::AnimationPlayState, "paused");
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    v.set_animation_time(5000.0);
    assert_eq!(alpha(&v), 128);
    assert!(!v.animating());
    let mut b = B::new();
    b.set_style(2, P::AnimationPlayState, "running");
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    v.set_animation_time(6500.0);
    assert_eq!(alpha(&v), 255);
    assert!(!v.animating());
    let mut b = B::new();
    b.stylesheet(2, ".b{opacity:.25!important}");
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    assert_eq!(alpha(&v), 64);
    let mut b = B::new();
    b.set_style(2, P::AnimationName, "none");
    v.commit(&b.finish(), false, &mut Deny).unwrap();
    assert_eq!(alpha(&v), 64);
}
#[test]
fn keyframes_select_last_rule_and_later_animations_win_per_property() {
    let mut v = view(
        "pulse 1s linear infinite, second 1s linear infinite",
        "@keyframes pulse{50%{opacity:.2}}@keyframes second{50%{opacity:.7}} @media (min-width:10000px){@keyframes second{50%{opacity:0}}}",
    );
    v.set_animation_time(500.0);
    assert!((178..=179).contains(&alpha(&v)));
}

#[test]
fn duplicate_offsets_cascade_and_zero_duration_fill_keeps_the_final_keyframe() {
    let mut v = view(
        "custom 1s linear infinite",
        "@keyframes custom{0%{opacity:.2}0%{animation-timing-function:linear}50%{opacity:.8}}",
    );
    v.set_animation_time(250.0);
    assert!((127..=128).contains(&alpha(&v)));
    let v = view("instant 0s forwards", "@keyframes instant{to{opacity:.2}}");
    assert!(!v.animating());
    assert_eq!(alpha(&v), 51);
}

#[test]
fn keyframes_that_need_general_matrix_interpolation_fail_explicitly() {
    let v = view(
        "matrix 1s linear infinite",
        "@keyframes matrix{from{transform:translateX(1px)}to{transform:rotate(90deg)}}",
    );
    assert!(v.rasterize_boxes(120, 120).is_err());
}
