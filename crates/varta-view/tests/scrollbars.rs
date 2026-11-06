//! Classic (always-shown) scrollbars: gutters that overflowing scroll
//! containers reserve, the scrollport they leave and their painted tracks
//! and thumbs; overlay scrollbars: frames that take no space, a thumb shown
//! by a scroll until it fades on the animation clock, and the pointer's
//! effect. `native_scrollbar_probe` and `native_overlay_scrollbar_probe`
//! compare the same with Chrome.
use varta_common::view::{Command, CssProperty as P, Tag};
use varta_view::{
    Access, BatchWriter, Broker, CLASSIC_SCROLLBAR_THICKNESS, Environment, Input, LayoutError,
    OVERLAY_SCROLLBAR_THICKNESS, Platform, RasterImage, Scrollbars, Target, View,
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

const SIDE: f32 = 200.0;
const SCROLLER: u32 = 2;
const CONTENT: u32 = 3;

/// A 100px square `overflow: auto` box holding one block of this size.
fn view(scrollbars: Scrollbars, content: (&str, &str), scroll: Option<f64>) -> View {
    let mut b = BatchWriter::new();
    b.create_element(SCROLLER, Tag::Div)
        .insert(1, SCROLLER, 0)
        .set_style(SCROLLER, P::Overflow, "auto")
        .set_style(SCROLLER, P::Width, "100px")
        .set_style(SCROLLER, P::Height, "100px")
        .set_style(SCROLLER, P::BackgroundColor, "#ffffff")
        .create_element(CONTENT, Tag::Div)
        .insert(SCROLLER, CONTENT, 0)
        .set_style(CONTENT, P::Width, content.0)
        .set_style(CONTENT, P::Height, content.1)
        .set_style(CONTENT, P::BackgroundColor, "#3050e0");
    if let Some(top) = scroll {
        b.command(SCROLLER, Command::ScrollTo, top);
    }
    let environment = Environment {
        width: f64::from(SIDE),
        scrollbars,
        ..Environment::default()
    };
    let mut view = View::new(environment, Platform::default());
    view.commit_with_layout(&b.finish(), false, &mut Denied, SIDE, SIDE, None)
        .unwrap();
    view
}

#[test]
fn an_overflowing_axis_reserves_its_gutter_only_with_classic_scrollbars() {
    // Overlay scrollbars take no space: a frame over the padding edge.
    let overlay = view(Scrollbars::Overlay, ("auto", "300px"), None);
    let geometry = overlay.layout_geometry(SIDE, SIDE).unwrap();
    assert_eq!(geometry[&CONTENT].rect.width, 100.0);
    assert_eq!(geometry[&SCROLLER].scroll.unwrap().viewport.width, 100.0);
    let bars = geometry[&SCROLLER].scrollbars.unwrap();
    assert!(bars.overlay);
    let vertical = bars.vertical.unwrap();
    assert_eq!(
        (vertical.x, vertical.width, vertical.height),
        (84.0, OVERLAY_SCROLLBAR_THICKNESS, 100.0)
    );
    assert_eq!(bars.horizontal, None);

    let classic = view(Scrollbars::Classic, ("auto", "300px"), None);
    let geometry = classic.layout_geometry(SIDE, SIDE).unwrap();
    assert_eq!(
        geometry[&CONTENT].rect.width,
        100.0 - CLASSIC_SCROLLBAR_THICKNESS
    );
    let bars = geometry[&SCROLLER].scrollbars.unwrap();
    let vertical = bars.vertical.unwrap();
    assert_eq!(
        (vertical.x, vertical.width, vertical.height),
        (85.0, CLASSIC_SCROLLBAR_THICKNESS, 100.0)
    );
    assert_eq!(bars.horizontal, None);
    assert_eq!(bars.vertical_range, [100.0, 200.0, 0.0]);
    // The scrollport, and so the content clip, leaves the gutter out.
    let scroll = geometry[&SCROLLER].scroll.unwrap();
    assert_eq!(scroll.viewport.width, 85.0);

    // Content that fits reserves nothing.
    let fits = view(Scrollbars::Classic, ("auto", "50px"), None);
    let geometry = fits.layout_geometry(SIDE, SIDE).unwrap();
    assert_eq!(geometry[&CONTENT].rect.width, 100.0);
    assert_eq!(geometry[&SCROLLER].scrollbars, None);
}

#[test]
fn a_gutter_that_makes_content_overflow_adds_the_other_bar() {
    // 90px fits the 100px box until the vertical gutter leaves 85px.
    let view = view(Scrollbars::Classic, ("90px", "300px"), None);
    let geometry = view.layout_geometry(SIDE, SIDE).unwrap();
    let bars = geometry[&SCROLLER].scrollbars.unwrap();
    let (vertical, horizontal) = (bars.vertical.unwrap(), bars.horizontal.unwrap());
    // Each track stops at the corner.
    assert_eq!(vertical.height, 85.0);
    assert_eq!((horizontal.y, horizontal.width), (85.0, 85.0));
    assert_eq!(bars.horizontal_range, [85.0, 5.0, 0.0]);
    // The horizontal bar shortens the vertical scrollport.
    assert_eq!(bars.vertical_range, [85.0, 215.0, 0.0]);
}

#[test]
fn the_gutter_hits_the_scroller_and_paints_the_track_and_thumb() {
    let view = view(Scrollbars::Classic, ("auto", "300px"), Some(100.0));
    let image = view.rasterize_scaled(200, 200, 1.0, None).unwrap();
    // Content in the scrollport, the scroller itself over its scrollbar.
    assert_eq!(image.hit_test(40.5, 40.5), Some(CONTENT));
    assert_eq!(image.hit_test(92.5, 40.5), Some(SCROLLER));
    let pixel = |x: usize, y: usize| {
        let at = (y * image.width as usize + x) * 4;
        image.pixels[at..at + 4].to_vec()
    };
    // Inner border, track, outer border.
    assert_eq!(pixel(85, 5), [0xE8, 0xE8, 0xE8, 0xFF]);
    assert_eq!(pixel(87, 5), [0xFA, 0xFA, 0xFA, 0xFF]);
    assert_eq!(pixel(99, 5), [0xED, 0xED, 0xED, 0xFF]);
    // 100 of 300 at offset 100: thumb [33, 66), painted inset 3 and 4.
    assert_eq!(pixel(92, 50), [0xC1, 0xC1, 0xC1, 0xFF]);
    assert_eq!(pixel(92, 31), [0xFA, 0xFA, 0xFA, 0xFF]);
    assert_eq!(pixel(87, 50), [0xFA, 0xFA, 0xFA, 0xFF]);
}

#[test]
fn a_rounded_scroller_with_classic_scrollbars_is_refused() {
    let mut b = BatchWriter::new();
    b.create_element(SCROLLER, Tag::Div)
        .insert(1, SCROLLER, 0)
        .set_style(SCROLLER, P::Overflow, "auto")
        .set_style(SCROLLER, P::Width, "100px")
        .set_style(SCROLLER, P::Height, "100px")
        .set_style(SCROLLER, P::BorderRadius, "8px")
        .create_element(CONTENT, Tag::Div)
        .insert(SCROLLER, CONTENT, 0)
        .set_style(CONTENT, P::Height, "300px");
    let environment = Environment {
        width: f64::from(SIDE),
        scrollbars: Scrollbars::Classic,
        ..Environment::default()
    };
    let mut view = View::new(environment, Platform::default());
    view.commit_with_layout(&b.finish(), false, &mut Denied, SIDE, SIDE, None)
        .unwrap();
    assert!(view.rasterize_scaled(200, 200, 1.0, None).is_err());
    // Layout itself is admitted.
    assert!(!matches!(
        view.layout_geometry(SIDE, SIDE),
        Err(LayoutError::Unsupported(_))
    ));
}

fn pixel(image: &RasterImage, x: usize, y: usize) -> [u8; 4] {
    let at = (y * image.width as usize + x) * 4;
    image.pixels[at..at + 4].try_into().unwrap()
}

/// The content's blue under the overlay thumb.
const CONTENT_BLUE: [u8; 4] = [0x30, 0x50, 0xE0, 0xFF];

fn scroll_to(view: &mut View, top: f64) {
    view.input(
        &Input::ScrollMeasured {
            target: Target::Node(SCROLLER),
            top,
            scroll_height: 300.0,
            client_height: 100.0,
        },
        &mut Denied,
    )
    .unwrap();
}

#[test]
fn overlay_frames_stop_at_the_corner_of_a_horizontal_bar() {
    let both = view(Scrollbars::Overlay, ("300px", "300px"), None);
    let geometry = both.layout_geometry(SIDE, SIDE).unwrap();
    let bars = geometry[&SCROLLER].scrollbars.unwrap();
    assert_eq!(bars.vertical.unwrap().height, 84.0);
    assert_eq!(bars.horizontal.unwrap().width, 84.0);
    // The thumb's range keeps the whole scrollport.
    assert_eq!(bars.vertical_range, [100.0, 200.0, 0.0]);
    // Content that fits has no bar.
    let fits = view(Scrollbars::Overlay, ("auto", "50px"), None);
    let geometry = fits.layout_geometry(SIDE, SIDE).unwrap();
    assert_eq!(geometry[&SCROLLER].scrollbars, None);
}

#[test]
fn a_scroll_shows_the_overlay_thumb_until_it_fades() {
    let mut view = view(Scrollbars::Overlay, ("auto", "300px"), None);
    let image = view.rasterize_scaled(200, 200, 1.0, None).unwrap();
    // Nothing shows before a scroll; the pointer reaches the content.
    assert_eq!(pixel(&image, 94, 50), CONTENT_BLUE);
    assert_eq!(image.hit_test(94.5, 50.5), Some(CONTENT));
    view.set_animation_time(1000.0);
    scroll_to(&mut view, 100.0);
    assert!(view.animating());
    let image = view.rasterize_scaled(200, 200, 1.0, None).unwrap();
    // 100 of 300 at offset 100: thumb box [88, 100) by [33, 66), the pill
    // inset to [91, 98) by [35, 64), black at alpha 0x80 over the content.
    let thumb = pixel(&image, 94, 50);
    assert!(
        thumb[0] <= 25 && (39..=41).contains(&thumb[1]) && (111..=113).contains(&thumb[2]),
        "{thumb:?}"
    );
    assert_eq!(pixel(&image, 89, 50), CONTENT_BLUE);
    assert_eq!(pixel(&image, 94, 33), CONTENT_BLUE);
    // The shown bar takes the pointer for the scroller; content stays.
    assert_eq!(image.hit_test(85.5, 10.5), Some(SCROLLER));
    assert_eq!(image.hit_test(80.5, 50.5), Some(CONTENT));
    assert_eq!(image.scrollbar_at(85.5, 10.5), Some(SCROLLER));
    // The delay changes nothing, so the host presents no frame for it.
    assert!(!view.set_animation_time(1499.0));
    assert!(view.set_animation_time(1600.0));
    let fading = pixel(&view.rasterize_scaled(200, 200, 1.0, None).unwrap(), 94, 50);
    assert!(
        fading[2] > thumb[2] && fading[2] < CONTENT_BLUE[2],
        "{fading:?}"
    );
    view.set_animation_time(1750.0);
    assert!(!view.animating());
    let image = view.rasterize_scaled(200, 200, 1.0, None).unwrap();
    assert_eq!(pixel(&image, 94, 50), CONTENT_BLUE);
    assert_eq!(image.hit_test(94.5, 50.5), Some(CONTENT));
    // A hidden frame still tracks the pointer.
    assert_eq!(image.scrollbar_at(94.5, 50.5), Some(SCROLLER));
    assert_eq!(image.scrollbar_at(50.5, 50.5), None);
    // A scroll that leaves the offset unchanged shows nothing.
    scroll_to(&mut view, 100.0);
    assert!(!view.animating());
}

#[test]
fn the_pointer_widens_the_thumb_shows_the_track_and_holds_the_fade() {
    // A programmatic scroll in a commit shows the bar too.
    let mut view = view(Scrollbars::Overlay, ("auto", "300px"), Some(100.0));
    view.set_animation_time(100.0);
    assert!(view.set_scrollbar_hover(Some(SCROLLER)));
    view.set_animation_time(400.0);
    let image = view.rasterize_scaled(200, 200, 1.0, None).unwrap();
    // The expanded thumb starts 3px into a 16px box: [87, 98).
    assert_ne!(pixel(&image, 88, 50), CONTENT_BLUE);
    // The track: inner border, gradient and outer border over the content.
    let (inner, middle, outer) = (
        pixel(&image, 84, 90),
        pixel(&image, 90, 90),
        pixel(&image, 99, 90),
    );
    assert!(middle[0] > 200 && middle[2] > CONTENT_BLUE[2], "{middle:?}");
    assert!(inner != middle && outer != middle, "{inner:?} {outer:?}");
    // No fade while the pointer stays.
    view.set_animation_time(5000.0);
    assert!(!view.animating());
    let held = view.rasterize_scaled(200, 200, 1.0, None).unwrap();
    assert_eq!(held.pixels, image.pixels);
    // Leaving restarts the delay.
    assert!(view.set_scrollbar_hover(None));
    view.set_animation_time(5499.0);
    let waiting = view.rasterize_scaled(200, 200, 1.0, None).unwrap();
    assert_eq!(waiting.pixels, image.pixels);
    view.set_animation_time(5750.0);
    let image = view.rasterize_scaled(200, 200, 1.0, None).unwrap();
    assert_eq!(pixel(&image, 90, 90), CONTENT_BLUE);
}

#[test]
fn retained_and_vector_frames_paint_the_overlay_bar_like_a_raster() {
    let mut view = view(Scrollbars::Overlay, ("auto", "300px"), Some(100.0));
    let mut renderer = varta_view::Renderer::new(None, 16.0).unwrap();
    view.set_scrollbar_hover(Some(SCROLLER));
    for now in [0.0, 120.0, 400.0] {
        view.set_animation_time(now);
        let raster = view.rasterize_scaled(200, 200, 1.25, None).unwrap();
        let retained = renderer.render(&view, 200, 200, 1.25).unwrap();
        assert_eq!(retained.pixels, raster.pixels, "at {now}ms");
        let varta_view::PreparedFrame::Vector { scene, metadata } =
            renderer.prepare(&view, 200, 200, 1.25).unwrap()
        else {
            panic!("the overlay bar needs no raster fallback");
        };
        assert!(metadata.same_input_geometry(&raster));
        assert!(!scene.commands().is_empty());
    }
}

#[test]
fn a_dark_scroller_takes_the_light_thumb() {
    let mut b = BatchWriter::new();
    b.create_element(SCROLLER, Tag::Div)
        .insert(1, SCROLLER, 0)
        .set_style(SCROLLER, P::Overflow, "auto")
        .set_style(SCROLLER, P::Width, "100px")
        .set_style(SCROLLER, P::Height, "100px")
        .set_style(SCROLLER, P::BackgroundColor, "#202020")
        .create_element(CONTENT, Tag::Div)
        .insert(SCROLLER, CONTENT, 0)
        .set_style(CONTENT, P::Height, "300px");
    b.command(SCROLLER, Command::ScrollTo, 100.0);
    let environment = Environment {
        width: f64::from(SIDE),
        ..Environment::default()
    };
    let mut view = View::new(environment, Platform::default());
    view.commit_with_layout(&b.finish(), false, &mut Denied, SIDE, SIDE, None)
        .unwrap();
    let image = view.rasterize_scaled(200, 200, 1.0, None).unwrap();
    // White at alpha 0x80 over #202020.
    let thumb = pixel(&image, 94, 50);
    assert!((143..=145).contains(&thumb[0]), "{thumb:?}");
}

#[test]
fn classic_scrollbars_ignore_the_overlay_clock() {
    let mut view = view(Scrollbars::Classic, ("auto", "300px"), Some(100.0));
    let before = view.rasterize_scaled(200, 200, 1.0, None).unwrap();
    view.set_animation_time(10_000.0);
    view.set_scrollbar_hover(Some(SCROLLER));
    let after = view.rasterize_scaled(200, 200, 1.0, None).unwrap();
    assert_eq!(before.pixels, after.pixels);
    assert_eq!(after.scrollbar_at(92.5, 40.5), None);
}
