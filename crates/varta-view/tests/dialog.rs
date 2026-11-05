//! Native modal dialogs: the top layer paints above every page layer (a
//! z-indexed box included), its backdrop takes the pointer for the dialog,
//! and a closed dialog renders nothing.
use varta_common::view::{Command, CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, TextMeasurer, View};
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
const SIDE: u32 = 120;
const TEXT_SIZE: f32 = 20.0;
fn font() -> TextMeasurer {
    TextMeasurer::new(
        include_bytes!("fixtures/inline.ttf").to_vec(),
        "Varta Inline Fixture",
    )
    .unwrap()
}
fn pixel(image: &varta_view::RasterImage, x: u32, y: u32) -> [u8; 3] {
    image.pixels[((y * image.width + x) * 4) as usize..][..3]
        .try_into()
        .unwrap()
}
/// A page with a z-indexed red box over its centre, a modal dialog (4)
/// holding a 30x20 blue box (5), and a second dialog (6) holding a 20x20
/// green box (7); `shown` dialogs are shown modally in that order.
fn page(shown: &[u32]) -> View {
    let mut b = B::new();
    b.set_style(1, P::BackgroundColor, "#ffffff")
        .set_style(1, P::FontFamily, "\"Varta Inline Fixture\"")
        .set_style(1, P::FontSize, "20px")
        .set_style(1, P::LineHeight, "20px")
        .create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .set_style(2, P::Position, "absolute")
        .set_style(2, P::ZIndex, "10")
        .set_style(2, P::Top, "0px")
        .set_style(2, P::Left, "0px")
        .set_style(2, P::Width, "120px")
        .set_style(2, P::Height, "120px")
        .set_style(2, P::BackgroundColor, "#ff0000")
        .create_element(4, Tag::Dialog)
        .insert(1, 4, 0)
        .create_element(5, Tag::Div)
        .insert(4, 5, 0)
        .set_style(5, P::Width, "30px")
        .set_style(5, P::Height, "20px")
        .set_style(5, P::BackgroundColor, "#0000ff")
        .create_element(6, Tag::Dialog)
        .insert(1, 6, 0)
        .create_element(7, Tag::Div)
        .insert(6, 7, 0)
        .set_style(7, P::Width, "20px")
        .set_style(7, P::Height, "20px")
        .set_style(7, P::BackgroundColor, "#00ff00");
    for &dialog in shown {
        b.command(dialog, Command::ShowModal, 0.0);
    }
    let mut view = View::default();
    view.commit(&b.finish(), false, &mut Denied).unwrap();
    view
}

#[test]
fn closed_dialogs_render_nothing() {
    let view = page(&[]);
    let image = view
        .rasterize_with_text(SIDE, SIDE, &mut font(), TEXT_SIZE)
        .unwrap();
    assert_eq!(pixel(&image, 60, 60), [255, 0, 0]);
    assert_eq!(image.hit_test(60.0, 60.0), Some(2));
}

/// A closed dialog's contents are never laid out or measured: content the
/// native path refuses (here a transition) cannot block the page.
#[test]
fn a_closed_dialog_with_refused_content_does_not_block_the_page() {
    let mut view = page(&[]);
    let mut b = B::new();
    b.create_element(20, Tag::Div)
        .insert(4, 20, 0)
        .set_style(20, P::TransitionDuration, "120ms")
        .create_text(21, "hidden")
        .insert(20, 21, 0);
    view.commit(&b.finish(), false, &mut Denied).unwrap();
    let image = view
        .rasterize_with_text(SIDE, SIDE, &mut font(), TEXT_SIZE)
        .unwrap();
    assert_eq!(pixel(&image, 60, 60), [255, 0, 0]);
}

#[test]
fn a_modal_dialog_and_its_backdrop_paint_above_every_page_layer() {
    let view = page(&[4]);
    let image = view
        .rasterize_with_text(SIDE, SIDE, &mut font(), TEXT_SIZE)
        .unwrap();
    // Centred, fit-content: the dialog box is exactly its 30x20 content.
    assert_eq!(pixel(&image, 60, 60), [0, 0, 255]);
    assert_eq!(image.hit_test(60.0, 60.0), Some(5));
    // The light backdrop, rgba(0, 0, 0, 0.4), darkens the z-indexed box and
    // takes its pointer for the dialog.
    let red = pixel(&image, 5, 5);
    assert_eq!((red[1], red[2]), (0, 0));
    assert!((152..=154).contains(&red[0]), "{red:?}");
    assert_eq!(image.hit_test(5.0, 5.0), Some(4));
}

#[test]
fn a_later_modal_dialog_stacks_above_an_earlier_one_and_its_backdrop() {
    let view = page(&[4, 6]);
    let image = view
        .rasterize_with_text(SIDE, SIDE, &mut font(), TEXT_SIZE)
        .unwrap();
    // The second dialog is centred over the first: its green box wins.
    assert_eq!(pixel(&image, 60, 60), [0, 255, 0]);
    assert_eq!(image.hit_test(60.0, 60.0), Some(7));
    // The first dialog's uncovered edge is under the second backdrop.
    let blue = pixel(&image, 47, 60);
    assert_eq!((blue[0], blue[1]), (0, 0));
    assert!((152..=154).contains(&blue[2]), "{blue:?}");
    assert_eq!(image.hit_test(47.0, 60.0), Some(6));
}
