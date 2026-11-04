//! Overlapping static opacity contexts for the shared real-Chrome paint oracle.
//! Run scripts/test-native-inline.mjs against this example's JSON output.
use varta_common::view::{CssProperty as P, Tag};
use varta_view::{Access, BatchWriter as B, Broker, View};

const WIDTH: u32 = 40;
const HEIGHT: u32 = 40;
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
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut cases = Vec::new();
    for (name, wrapper, outer_alpha, first_alpha, second_alpha, curved) in [
        ("ordinary", None, "1", "1", "1", false),
        ("first-context", None, "1", "0.5", "1", false),
        ("zero-context", None, "1", "0", "1", false),
        ("two-contexts", None, "1", "0.5", "0.5", false),
        ("through-block", Some("block"), "1", "0.5", "1", false),
        ("through-flex", Some("flex"), "1", "0.5", "1", false),
        ("through-grid", Some("grid"), "1", "0.5", "1", false),
        ("nested-context", Some("block"), "0.5", "0.5", "1", false),
        ("nested-siblings", Some("block"), "0.5", "0.5", "0.5", false),
        ("curved-ancestor", Some("block"), "1", "0.5", "1", true),
        ("curved-zero", Some("block"), "1", "0", "1", true),
    ] {
        let mut batch = B::new();
        batch.set_style(1, P::BackgroundColor, "#ffffff");
        if let Some(display) = wrapper {
            batch
                .create_element(4, Tag::Div)
                .insert(1, 4, 0)
                .set_style(4, P::Width, "40px")
                .set_style(4, P::Height, "20px")
                .set_style(4, P::Display, display)
                .set_style(4, P::Opacity, outer_alpha);
            if curved {
                batch
                    .set_style(4, P::Overflow, "hidden")
                    .set_style(4, P::BorderRadius, "50%");
            }
        }
        batch
            .create_element(2, Tag::Div)
            .insert(if wrapper.is_some() { 4 } else { 1 }, 2, 0)
            .set_style(2, P::Width, "40px")
            .set_style(2, P::Height, "20px")
            .set_style(2, P::BackgroundColor, "#ff0000")
            .set_style(2, P::Opacity, first_alpha)
            .create_element(3, Tag::Div)
            .insert(1, 3, 0)
            .set_style(3, P::Width, "40px")
            .set_style(3, P::Height, "20px")
            .set_style(3, P::MarginTop, "-20px")
            .set_style(3, P::BackgroundColor, "#0000ff")
            .set_style(3, P::Opacity, second_alpha);
        let mut view = View::default();
        view.commit(&batch.finish(), false, &mut Denied)?;
        let geometry = view.layout_boxes(WIDTH as f32, HEIGHT as f32)?;
        let image = view.rasterize_boxes(WIDTH, HEIGHT)?;
        let boxes = geometry
            .iter()
            .filter(|(id, _)| **id != 1)
            .map(|(&id, r)| {
                (
                    id.to_string(),
                    serde_json::json!({"x":r.x,"y":r.y,"width":r.width,"height":r.height}),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let mut hits = Vec::new();
        for y in (0..HEIGHT).step_by(3) {
            for x in (0..WIDTH).step_by(3) {
                let px = x as f32 + 0.25;
                let py = y as f32 + 0.25;
                let target = image.hit_test(px, py);
                let boundary = [-1.0, 1.0].into_iter().any(|dx| {
                    [-1.0, 1.0]
                        .into_iter()
                        .any(|dy| image.hit_test(px + dx, py + dy) != target)
                });
                hits.push(serde_json::json!({"x":x,"y":y,"id":target,"boundary":boundary}));
            }
        }
        cases.push(serde_json::json!({
            "name":name,"tree":view.snapshot()["tree"],"boxes":boxes,"hits":hits,
            "samples":[],"raster":image.pixels,"raster_size":[WIDTH,HEIGHT],
            "roi":{"x":0,"y":0,"width":WIDTH,"height":20}
        }));
    }
    let output = serde_json::to_vec(&serde_json::json!({
        "family":"Varta Inline Fixture",
        "font":include_bytes!("../tests/fixtures/inline.ttf").as_slice(),"cases":cases
    }))?;
    use std::io::Write;
    std::io::stdout().lock().write_all(&output)?;
    Ok(())
}
