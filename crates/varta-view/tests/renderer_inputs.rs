//! Retained images must not stale field, composition, popup or hidden-tree state.
use varta_common::view::{Attribute as A, Tag};
use varta_view::{Access, BatchWriter as B, Broker, Input, Renderer, Target, TextMeasurer, View};
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
fn field(tag: Tag) -> View {
    let mut b = B::new();
    b.stylesheet(1,".field {display:block; width:88px; height:44px; min-width:0; min-height:0; padding:3px; border:1px solid #000; border-radius:0; outline:none; background-color:#fff; color:#00f; font-family:\"Varta Fixture 400\"; font-size:20px; font-weight:400; font-style:normal; line-height:20px} .field:focus-visible {outline:2px solid #f00}")
  .create_element(2,tag).set_class(2,"field").insert(1,2,0);
    if tag == Tag::Select {
        for id in 3..7 {
            b.create_element(id, Tag::Option)
                .set_attribute(id, A::Value, &id.to_string())
                .insert(2, id, 0)
                .create_text(id + 4, &"A".repeat((id - 2) as usize))
                .insert(id, id + 4, 0);
        }
    } else {
        b.set_value(2, "AAAA\nAA", 0)
            .set_style(2, varta_common::view::CssProperty::Resize, "none");
    }
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    v
}
fn compare(v: &View, r: &mut Renderer, scale: f32) {
    let width = (160.0 * scale) as u32;
    let height = (130.0 * scale) as u32;
    let mut a = r.render(v, width, height, scale).unwrap();
    let mut b = v
        .rasterize_scaled(width, height, scale, Some((&mut font(), 20.0)))
        .unwrap();
    let diffs: Vec<_> = a
        .pixels
        .iter()
        .zip(&b.pixels)
        .enumerate()
        .filter(|(_, (a, b))| a != b)
        .take(5)
        .map(|(i, (a, b))| (i, *a, *b))
        .collect();
    assert!(diffs.is_empty(), "scale {scale}: {diffs:?}");
    // Keep the CPU renderer's previous pixels for the next input. Switching
    // that renderer to vector mode would discard its damage-replay cache.
    let mut vector = Renderer::new(Some(font()), 20.0).unwrap();
    let varta_view::PreparedFrame::Vector { metadata, scene } =
        vector.prepare(v, width, height, scale).unwrap()
    else {
        panic!("control fixture should fit vector bounds")
    };
    let replay = scene.rasterize(metadata).unwrap();
    assert!(
        replay == b,
        "vector control pixels or input metadata differ at scale {scale}"
    );
    a.pixels.clear();
    b.pixels.clear();
    assert_eq!(a, b);
}
fn key(name: &str) -> Input {
    Input::Key {
        target: Target::Node(2),
        key: name.into(),
        shift: false,
        ctrl: false,
        alt: false,
        meta: false,
        composing: false,
    }
}
#[test]
fn editing_selection_and_ime_match_reference_hits_and_pixels() {
    // Keep the viewport fixed across inputs so damage replay is exercised.
    for scale in [1.0, 2.0] {
        let mut v = field(Tag::Textarea);
        let mut r = Renderer::new(Some(font()), 20.0).unwrap();
        let inputs = [
            Input::Focus {
                target: Target::Node(2),
            },
            Input::TextSelect {
                target: Target::Node(2),
                anchor: 0,
                focus: 2,
                activate: true,
            },
            Input::Type {
                target: Target::Node(2),
                text: "A".into(),
            },
            key("End"),
            Input::ImePreedit {
                target: Target::Node(2),
                text: "AA".into(),
                cursor: Some((0, 2)),
            },
            Input::ImeCommit {
                target: Target::Node(2),
                text: "A".into(),
            },
            key("ArrowUp"),
            key("Home"),
        ];
        for input in inputs {
            v.input(&input, &mut Denied).unwrap();
            compare(&v, &mut r, scale);
        }
    }
}
#[test]
fn popup_open_preview_scroll_commit_and_close_restore_underlying_pixels() {
    for scale in [1.0, 1.25] {
        let mut v = field(Tag::Select);
        let mut r = Renderer::new(Some(font()), 20.0).unwrap();
        for input in [
            Input::Click {
                target: Target::Node(2),
            },
            key("ArrowDown"),
            key("ArrowDown"),
            Input::SelectScroll {
                target: Target::Node(2),
                first: 1,
            },
            key("Enter"),
            Input::Click {
                target: Target::Node(2),
            },
            Input::SelectClose {
                target: Target::Node(2),
            },
        ] {
            v.input(&input, &mut Denied).unwrap();
            compare(&v, &mut r, scale);
        }
    }
}
#[test]
fn hidden_subtrees_remain_equivalent_and_preserve_depth_limits() {
    let mut b = B::new();
    b.create_element(2, Tag::Div)
        .set_style(2, varta_common::view::CssProperty::Display, "none")
        .insert(1, 2, 0);
    for id in 3..64 {
        b.create_element(id, Tag::Div).insert(id - 1, id, 0);
    }
    b.create_text(64, "hidden").insert(63, 64, 0);
    let mut v = View::default();
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    let mut r = Renderer::new(Some(font()), 20.0).unwrap();
    compare(&v, &mut r, 1.0);
    let mut b = B::new();
    for id in 65..300 {
        b.create_element(id, Tag::Div)
            .insert(if id == 65 { 63 } else { id - 1 }, id, 0);
    }
    v.commit(&b.finish(), false, &mut Denied).unwrap();
    assert!(matches!(
        r.render(&v, 160, 130, 1.0),
        Err(varta_view::PaintError::Layout(
            varta_view::LayoutError::Limit
        ))
    ));
    assert!(matches!(
        v.rasterize_scaled(160, 130, 1.0, Some((&mut font(), 20.0))),
        Err(varta_view::PaintError::Layout(
            varta_view::LayoutError::Limit
        ))
    ));
}

#[test]
fn menu_pointer_hover_highlights_without_changing_selection_or_keyboard_preview() {
    let mut v = field(Tag::Select);
    v.input(
        &Input::Click {
            target: Target::Node(2),
        },
        &mut Denied,
    )
    .unwrap();
    let mut r = Renderer::new(Some(font()), 20.0).unwrap();
    let before = r.render(&v, 160, 130, 1.0).unwrap();
    let menu = before.select_menu().unwrap();
    let x = menu.viewport.x + 8.0;
    let y = menu.viewport.y + menu.row_height * 1.5;
    let Some(varta_view::SelectHit::Choice { option, .. }) = before.select_hit(x, y) else {
        panic!("second menu row missing")
    };
    let snapshot = v.snapshot();
    v.set_pointer_state(Some(option), None);
    compare(&v, &mut r, 1.0);
    let after = r.render(&v, 160, 130, 1.0).unwrap();
    assert_ne!(
        before.pixels, after.pixels,
        "hover needs visible row feedback"
    );
    assert!(before.same_input_geometry(&after));
    assert_eq!(after.cursor_at(x, y), varta_view::PointerCursor::Pointer);
    assert_eq!(v.snapshot(), snapshot);
    v.set_pointer_state(None, None);
    assert!(r.render(&v, 160, 130, 1.0).unwrap() == before);
}
