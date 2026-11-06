//! Classic (always-shown) scrollbars: gutters that overflowing scroll
//! containers reserve, the scrollport they leave and their painted tracks
//! and thumbs. `native_scrollbar_probe` compares the same with Chrome.
use varta_common::view::{Command, CssProperty as P, Tag};
use varta_view::{
    Access, BatchWriter, Broker, CLASSIC_SCROLLBAR_THICKNESS, Environment, LayoutError, Platform,
    Scrollbars, View,
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
    // Overlay scrollbars take no space.
    let overlay = view(Scrollbars::Overlay, ("auto", "300px"), None);
    let geometry = overlay.layout_geometry(SIDE, SIDE).unwrap();
    assert_eq!(geometry[&CONTENT].rect.width, 100.0);
    assert_eq!(geometry[&SCROLLER].scrollbars, None);

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
