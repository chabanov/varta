//! Retained rendering must remain pixel- and input-equivalent to the reference.
use varta_common::view::{Command, CssProperty as P, Tag};
use varta_view::{
    Access, BatchWriter as B, Broker, Environment, Input, Renderer, Target, TextMeasurer, View,
};
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
fn font() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap()
}
fn view() -> View {
    varta_common::view::stylesheet::parse(":root { --ink:#00f; font-family:\"Varta Fixture 400\"; font-size:20px; line-height:20px } .box {width:60px; height:40px; background-color:#ffffff; color:var(--ink); border:1px solid #080; padding:3px; overflow:hidden } .box:focus-visible {outline:2px solid #f00} .accent { color:#f00; background-color:#ffcccc } .wide {width:90px} @media (min-width:150px) {.box { --ink:#080 }}").unwrap();
    let mut b = B::new();
    b.stylesheet(1,":root { --ink:#00f; font-family:\"Varta Fixture 400\"; font-size:20px; line-height:20px } .box {width:60px; height:40px; background-color:#ffffff; color:var(--ink); border:1px solid #080; padding:3px; overflow:hidden } .box:focus-visible {outline:2px solid #f00} .accent { color:#f00; background-color:#ffcccc } .wide {width:90px} @media (min-width:150px) {.box { --ink:#080 }}")
        .create_element(2,Tag::Div).set_class(2,"box").insert(1,2,0)
        .create_text(3,"AA ").insert(2,3,0)
        .create_element(4,Tag::Span).set_class(4,"accent").insert(2,4,0)
        .create_text(5,"A A A").insert(4,5,0);
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    v
}
fn equivalent(v: &View, renderer: &mut Renderer, width: u32, height: u32, scale: f32) {
    let snapshot = v.snapshot();
    let expected = v
        .rasterize_scaled(width, height, scale, Some((&mut font(), 20.0)))
        .unwrap();
    let actual = renderer.render(v, width, height, scale).unwrap();
    let differences: Vec<_> = actual
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(expected.pixels.as_chunks::<4>().0)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .take(10)
        .map(|(i, (a, b))| (i % width as usize, i / width as usize, *a, *b))
        .collect();
    assert!(
        differences.is_empty(),
        "{width}x{height} at {scale}: {differences:?}"
    );
    let mut a = actual;
    let mut b = expected;
    a.pixels.clear();
    b.pixels.clear();
    assert_eq!(a, b);
    assert_eq!(v.snapshot(), snapshot);
}
#[test]
fn vector_preparation_matches_software_and_does_not_reuse_pixels_after_backend_switch() {
    let mut v = view();
    let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
    let old = renderer.render(&v, 180, 130, 1.0).unwrap();
    let mut b = B::new();
    b.set_text(3, "AAAA ");
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    for scale in [1.0, 1.25, 2.0] {
        let width = (180.0 * scale) as u32;
        let height = (130.0 * scale) as u32;
        let prepared = renderer.prepare(&v, width, height, scale).unwrap();
        let varta_view::PreparedFrame::Vector { metadata, scene } = prepared else {
            panic!("fixture should fit vector bounds")
        };
        assert!(!scene.commands().is_empty());
        let replay = scene.rasterize(metadata).unwrap();
        let reference = v
            .rasterize_scaled(width, height, scale, Some((&mut font(), 20.0)))
            .unwrap();
        let first_difference = |a: &[u8], b: &[u8]| {
            a.iter()
                .zip(b)
                .enumerate()
                .find(|(_, (x, y))| x != y)
                .map(|(i, (x, y))| (i, *x, *y))
        };
        assert!(
            replay == reference,
            "vector replay at {scale}: {:?}",
            first_difference(&replay.pixels, &reference.pixels)
        );
        let varta_view::PreparedFrame::Vector { metadata, scene } =
            renderer.prepare(&v, width, height, scale).unwrap()
        else {
            panic!("cached vector frame")
        };
        assert!(
            renderer.stats().drawing_cache_hits > 0,
            "unchanged glyph paths should be reused"
        );
        let reused = scene.rasterize(metadata).unwrap();
        assert!(
            reused == reference,
            "cached drawing data changed pixels or input geometry"
        );
        let cpu = renderer.render(&v, width, height, scale).unwrap();
        assert!(
            cpu == reference,
            "CPU after vector frame at {scale}: {:?}",
            first_difference(&cpu.pixels, &reference.pixels)
        );
    }
    assert!(
        old.pixels != renderer.render(&v, 180, 130, 1.0).unwrap().pixels,
        "changed text must replace old pixels"
    );
}
#[test]
fn retained_text_damage_and_style_changes_match_the_reference() {
    let mut v = view();
    let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
    for text in ["AA ", "A ", "AAAA ", "", "AA A "] {
        let mut b = B::new();
        b.set_text(3, text);
        v.commit(&b.finish(), false, &mut Denied).unwrap();
        equivalent(&v, &mut renderer, 140, 110, 1.0);
        equivalent(&v, &mut renderer, 280, 220, 2.0);
        equivalent(&v, &mut renderer, 140, 110, 1.0);
    }
    for (property, value) in [
        (P::Width, "90px"),
        (P::Color, "#f00"),
        (P::FontSize, "16px"),
        (P::LineHeight, "28px"),
        (P::Padding, "5px"),
        (P::Display, "none"),
        (P::Display, "block"),
    ] {
        let mut b = B::new();
        b.set_style(2, property, value);
        v.commit(&b.finish(), false, &mut Denied).unwrap();
        equivalent(&v, &mut renderer, 180, 130, 1.0);
        equivalent(&v, &mut renderer, 225, 163, 1.25);
    }
}
#[test]
fn retained_source_invalidation_covers_sheets_ids_ancestors_media_and_scroll() {
    let mut v = view();
    let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
    equivalent(&v, &mut renderer, 140, 110, 1.0);
    let mut b = B::new();
    b.stylesheet(2, ":root {--ink:#f00} .accent {font-size:25px}");
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    equivalent(&v, &mut renderer, 140, 110, 1.0);
    let mut b = B::new();
    b.set_class(2, "box wide")
        .command(2, Command::ScrollTo, 10.0);
    v.commit_with_layout(
        &b.finish(),
        false,
        &mut Denied,
        140.0,
        110.0,
        Some((&mut font(), 20.0)),
    )
    .unwrap();
    equivalent(&v, &mut renderer, 140, 110, 1.0);
    v.set_environment(Environment::default().with_root_font_size(20.0).unwrap())
        .unwrap();
    equivalent(&v, &mut renderer, 160, 120, 1.0);
    let mut b = B::new();
    b.remove(4)
        .create_element(4, Tag::Span)
        .insert(2, 4, 3)
        .create_text(5, "AAAA")
        .insert(4, 5, 0);
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    equivalent(&v, &mut renderer, 160, 120, 1.0);
}
#[test]
fn refused_frame_does_not_publish_or_poison_the_retained_scene() {
    let mut v = view();
    let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
    equivalent(&v, &mut renderer, 140, 110, 1.0);
    // A fixed box escaping the clip of its opacity group is refused.
    let mut b = B::new();
    b.create_element(6, Tag::Div)
        .insert(1, 6, 0)
        .set_style(6, P::Width, "20px")
        .set_style(6, P::Height, "20px")
        .set_style(6, P::Overflow, "hidden")
        .create_element(7, Tag::Div)
        .insert(6, 7, 0)
        .set_style(7, P::Opacity, "0.5")
        .create_element(8, Tag::Div)
        .insert(7, 8, 0)
        .set_style(8, P::Position, "fixed")
        .set_style(8, P::Width, "30px")
        .set_style(8, P::Height, "30px")
        .set_style(8, P::BackgroundColor, "#f00");
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    let snapshot = v.snapshot();
    assert!(matches!(
        renderer.render(&v, 140, 110, 1.0),
        Err(varta_view::PaintError::Unsupported(8))
    ));
    assert_eq!(v.snapshot(), snapshot);
    let mut b = B::new();
    b.remove(6).set_text(3, "AAA ");
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    equivalent(&v, &mut renderer, 140, 110, 1.0);
}
#[test]
fn retained_focus_and_checked_state_do_not_reuse_stale_styles() {
    let mut b = B::new();
    b.stylesheet(1,".check {display:block; width:24px; height:24px; min-width:0; min-height:0; padding:3px; border:1px solid #000; outline:none; border-radius:0; background-color:#fff; color:#00f} .check:checked {color:#080} .check:focus-visible {outline:2px solid #f00}")
        .create_element(2,Tag::Input).set_attribute(2,varta_common::view::Attribute::Type,"checkbox").set_class(2,"check").insert(1,2,0);
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
    equivalent(&v, &mut renderer, 100, 100, 1.0);
    v.input(
        &Input::Click {
            target: Target::Node(2),
        },
        &mut Denied,
    )
    .unwrap();
    equivalent(&v, &mut renderer, 100, 100, 1.0);
    v.input(
        &Input::Key {
            target: Target::Node(2),
            key: "Tab".into(),
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
            composing: false,
        },
        &mut Denied,
    )
    .unwrap();
    equivalent(&v, &mut renderer, 100, 100, 1.0);
}

#[test]
fn retained_fractional_alpha_and_transformed_backgrounds_remain_pixel_exact() {
    let mut v = view();
    let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
    let mut b = B::new();
    b.set_style(2, P::MarginLeft, "1.3px")
        .set_style(2, P::MarginTop, "2.3px")
        .set_style(2, P::BackgroundColor, "#ffffff88")
        .set_style(2, P::Padding, "3.7px")
        .set_style(2, P::Border, "1.5px solid #33445566");
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    for scale in [1.0, 1.25, 2.0] {
        for text in ["AA ", "A ", "AAAA ", "AA "] {
            let mut b = B::new();
            b.set_text(3, text);
            v.commit(&b.finish(), false, &mut Denied).unwrap();
            equivalent(
                &v,
                &mut renderer,
                (140.0 * scale) as u32,
                (110.0 * scale) as u32,
                scale,
            );
        }
    }
}

#[test]
fn reapplying_equal_shorthands_invalidates_their_changed_side_precedence() {
    let mut v = view();
    let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
    for (property, value) in [
        (P::Border, "2px solid #080"),
        (P::BorderLeft, "6px solid #080"),
        (P::Border, "2px solid #080"),
        (P::Padding, "3px"),
        (P::PaddingLeft, "9px"),
        (P::Padding, "3px"),
        (P::Margin, "2px"),
        (P::MarginTop, "8px"),
        (P::Margin, "2px"),
    ] {
        let mut batch = B::new();
        batch.set_style(2, property, value);
        v.commit(&batch.finish(), false, &mut Denied).unwrap();
        equivalent(&v, &mut renderer, 180, 130, 1.0);
        equivalent(&v, &mut renderer, 360, 260, 2.0);
    }
}

#[test]
fn rounded_layout_media_width_preserves_breakpoints_between_f32_and_f64() {
    for threshold in ["130.39", "130.4", "130.41"] {
        let mut v = view();
        let mut batch = B::new();
        batch.stylesheet(
            2,
            &format!(
                "@media (min-width:{threshold}px) {{.box {{width:70px; background-color:#f00}}}}"
            ),
        );
        v.commit(&batch.finish(), false, &mut Denied).unwrap();
        let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
        for _ in 0..2 {
            equivalent(&v, &mut renderer, 163, 141, 1.25);
        }
    }
}

#[test]
fn inline_pointer_cursor_and_hover_keep_fragment_hit_geometry() {
    let mut v = view();
    let mut batch = B::new();
    batch.stylesheet(
        2,
        ".accent {cursor:pointer} .accent:hover {background-color:#ffaaaa}",
    );
    v.commit(&batch.finish(), false, &mut Denied).unwrap();
    let mut renderer = Renderer::new(Some(font()), 20.0).unwrap();
    let before = renderer.render(&v, 140, 110, 1.0).unwrap();
    let position = (0..110)
        .flat_map(|y| (0..140).map(move |x| (x, y)))
        .find(|(x, y)| before.hit_test(*x as f32 + 0.5, *y as f32 + 0.5) == Some(4))
        .unwrap();
    assert_eq!(
        before.cursor_at(position.0 as f32 + 0.5, position.1 as f32 + 0.5),
        varta_view::PointerCursor::Pointer
    );
    v.set_pointer_state(Some(4), None);
    equivalent(&v, &mut renderer, 140, 110, 1.0);
    let after = renderer.render(&v, 140, 110, 1.0).unwrap();
    assert!(before.same_input_geometry(&after));
    assert!(before.pixels != after.pixels);
}

#[test]
fn native_layout_depth_boundary_fits_the_normal_thread_stack() {
    const CHILD: &str = "VARTA_NATIVE_LAYOUT_DEPTH_CHILD";
    if std::env::var_os(CHILD).is_some() {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                for depth in [128u32, 129u32] {
                    let mut view = View::default();
                    let mut batch = B::new();
                    for id in 2..=depth + 1 {
                        batch
                            .create_element(id, Tag::Div)
                            .insert(id - 1, id, 0)
                            .set_style(id, P::Width, "20px")
                            .set_style(id, P::Height, "20px")
                            .set_style(id, P::BackgroundColor, "#0000ff");
                    }
                    view.commit(&batch.finish(), false, &mut Denied).unwrap();
                    let actual = view.rasterize_boxes(32, 32);
                    if depth == 128 {
                        let actual = actual.unwrap();
                        assert_eq!(actual.hit_test(10.0, 10.0), Some(depth + 1));
                        let mut renderer = Renderer::new(None, 16.0).unwrap();
                        assert_eq!(renderer.render(&view, 32, 32, 1.0).unwrap(), actual);
                        let varta_view::PreparedFrame::Vector { metadata, scene } =
                            renderer.prepare(&view, 32, 32, 1.0).unwrap()
                        else {
                            panic!("bounded deep vector scene")
                        };
                        assert_eq!(scene.rasterize(metadata).unwrap(), actual);
                        let mut opacity = B::new();
                        for id in 1..=depth + 1 {
                            opacity.set_style(id, P::Opacity, "0.5");
                        }
                        view.commit(&opacity.finish(), false, &mut Denied).unwrap();
                        let faded = view.rasterize_boxes(32, 32).unwrap();
                        assert!(faded.same_input_geometry(&actual));
                        let offset = (10 * 32 + 10) * 4;
                        assert_eq!(&faded.pixels[offset..offset + 4], &[0, 0, 64, 64]);
                        assert_eq!(renderer.render(&view, 32, 32, 1.0).unwrap(), faded);
                        let varta_view::PreparedFrame::Raster(prepared) =
                            renderer.prepare(&view, 32, 32, 1.0).unwrap()
                        else {
                            panic!("maximum native opacity depth uses explicit raster fallback")
                        };
                        assert_eq!(prepared, faded);
                        let mut border = B::new();
                        border
                            .set_style(depth + 1, P::Border, "1px solid #ff000080")
                            .set_style(depth + 1, P::BorderRight, "1px solid #0000ff80");
                        view.commit(&border.finish(), false, &mut Denied).unwrap();
                        let colored = view.rasterize_boxes(32, 32).unwrap();
                        assert_eq!(renderer.render(&view, 32, 32, 1.0).unwrap(), colored);
                    } else {
                        assert!(matches!(
                            actual,
                            Err(varta_view::PaintError::Layout(
                                varta_view::LayoutError::Limit
                            ))
                        ));
                    }
                }
            })
            .unwrap()
            .join()
            .unwrap();
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "renderer::native_layout_depth_boundary_fits_the_normal_thread_stack",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert!(
        output.status.success() && String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "normal-stack layout must run and pass its exact subprocess: {}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
