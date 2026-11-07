//! Sticky remains in flow, follows its scrollport and stops at its container.
use varta_common::view::{Command, Tag};
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
fn scene(wrapper: bool) -> View {
    let mut b = B::new();
    b.stylesheet(1,".port{position:relative;width:100px;height:80px;overflow:auto}.wrapper{height:80px}.space{height:40px}.sticky{position:sticky;top:0;height:20px;background-color:red}.tail{height:300px}")
        .create_element(2,Tag::Div).set_class(2,"port").insert(1,2,0);
    let parent = if wrapper {
        b.create_element(3, Tag::Div)
            .set_class(3, "wrapper")
            .insert(2, 3, 0);
        3
    } else {
        2
    };
    b.create_element(4, Tag::Div)
        .set_class(4, "space")
        .insert(parent, 4, 0)
        .create_element(5, Tag::Div)
        .set_class(5, "sticky")
        .insert(parent, 5, 0)
        .create_element(6, Tag::Div)
        .set_class(6, "tail")
        .insert(2, 6, 0);
    let mut v = View::default();
    v.commit_with_layout(&b.finish(), false, &mut Deny, 200.0, 200.0, None)
        .unwrap();
    v
}
fn scroll(v: &mut View, offset: f64) {
    let mut b = B::new();
    b.command(2, Command::ScrollTo, offset);
    v.commit_with_layout(&b.finish(), false, &mut Deny, 200.0, 200.0, None)
        .unwrap();
}
#[test]
fn sticky_keeps_flow_and_sticks_to_the_scrollport() {
    let mut v = scene(false);
    let initial = v.layout_boxes(200.0, 200.0).unwrap();
    assert_eq!(initial[&5].y, 40.0);
    assert_eq!(initial[&6].y, 60.0);
    scroll(&mut v, 20.0);
    let boxes = v.layout_boxes(200.0, 200.0).unwrap();
    assert_eq!(boxes[&5].y, 20.0);
    assert_eq!(boxes[&6].y, 40.0);
    scroll(&mut v, 100.0);
    let boxes = v.layout_boxes(200.0, 200.0).unwrap();
    assert_eq!(boxes[&5].y, 0.0);
    assert_eq!(boxes[&6].y, -40.0);
    assert_eq!(
        v.rasterize_boxes(200, 200).unwrap().hit_test(20.0, 10.0),
        Some(5)
    );
}
#[test]
fn sticky_stops_at_its_containing_blocks_end() {
    let mut v = scene(true);
    scroll(&mut v, 100.0);
    let boxes = v.layout_boxes(200.0, 200.0).unwrap();
    assert_eq!(boxes[&5].y, -40.0);
    assert_ne!(
        v.rasterize_boxes(200, 200).unwrap().hit_test(20.0, 10.0),
        Some(5)
    );
}
