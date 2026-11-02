//! Native view oracle for the independently executed generated browser model.
use varta_common::view::{Attribute as A, Command, CssProperty, EventKind, ListenOption, Tag};
use varta_view::{BatchWriter as B, Broker, Filter, Input, Target, View};

struct Denied;
impl Broker for Denied {
    fn link_requested(&self, _url: &str) -> bool {
        false
    }
    fn open_url(&mut self, _url: &str) -> varta_view::Access {
        varta_view::Access::Denied
    }
    fn clipboard_write(&mut self) -> varta_view::Access {
        varta_view::Access::Denied
    }
}

fn base() -> B {
    let mut b = B::new();
    b.create_element(2, Tag::Div)
        .insert(1, 2, 0)
        .create_element(3, Tag::Input)
        .insert(2, 3, 0)
        .create_text(4, "Привіт 😀")
        .insert(2, 4, 0);
    b
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut cases: Vec<(&str, Vec<Vec<u8>>)> = Vec::new();
    let mut add = |name, batch: B| cases.push((name, vec![base().finish(), batch.finish()]));
    add("text", B::new().set_text(4, "\u{feff}changed").clone());
    add(
        "duplicate-final-id",
        B::new()
            .set_attribute(2, A::Id, "same")
            .set_attribute(3, A::Id, "same")
            .clone(),
    );
    add(
        "id-reassignment",
        B::new()
            .set_attribute(2, A::Id, "same")
            .set_attribute(3, A::Id, "same")
            .remove_attribute(2, A::Id)
            .clone(),
    );
    add("cycle", B::new().insert(2, 2, 0).clone());
    add("void-child", B::new().insert(3, 4, 0).clone());
    add("root-remove", B::new().remove(1).clone());
    add(
        "insert-operand-order",
        B::new()
            .create_element(5, Tag::Div)
            .insert(1, 5, 2)
            .insert(5, 3, 0)
            .clone(),
    );
    add("unknown-node", B::new().set_text(999, "bad").clone());
    add(
        "late-rollback",
        B::new()
            .set_attribute(2, A::Id, "changed")
            .set_value(3, "changed", 0)
            .set_text(999, "bad")
            .clone(),
    );
    add(
        "number-leading-dot",
        B::new()
            .create_element(5, Tag::Progress)
            .set_attribute(5, A::Max, ".5")
            .clone(),
    );
    add(
        "resource-dot",
        B::new()
            .create_element(5, Tag::Img)
            .set_attribute(5, A::Src, "./image.png")
            .clone(),
    );
    add(
        "select-text",
        B::new()
            .create_element(5, Tag::Select)
            .insert(2, 5, 0)
            .insert(5, 4, 0)
            .clone(),
    );
    add(
        "select-option",
        B::new()
            .create_element(5, Tag::Select)
            .create_element(6, Tag::Option)
            .insert(2, 5, 0)
            .insert(5, 6, 0)
            .insert(6, 4, 0)
            .clone(),
    );
    add(
        "checkbox-value",
        B::new()
            .set_attribute(3, A::Type, "checkbox")
            .set_value(3, "bad", 0)
            .clone(),
    );
    add("checked-text", B::new().set_checked(3, 1, 0).clone());
    add(
        "unnamed-radios",
        B::new()
            .set_attribute(3, A::Type, "radio")
            .create_element(5, Tag::Input)
            .set_attribute(5, A::Type, "radio")
            .set_checked(3, 1, 0)
            .set_checked(5, 1, 0)
            .insert(2, 5, 0)
            .clone(),
    );
    add(
        "named-radios",
        B::new()
            .set_attribute(3, A::Type, "radio")
            .set_attribute(3, A::Name, "r")
            .create_element(5, Tag::Input)
            .set_attribute(5, A::Type, "radio")
            .set_attribute(5, A::Name, "r")
            .set_checked(3, 1, 0)
            .set_checked(5, 1, 0)
            .insert(2, 5, 0)
            .clone(),
    );
    add("stale-value", B::new().set_value(3, "ignored", 1).clone());
    add(
        "duplicate-filter",
        B::new()
            .listen(2, EventKind::Click, 1, Filter::default())
            .listen(2, EventKind::Click, 2, Filter::default())
            .clone(),
    );
    add(
        "duplicate-handler",
        B::new()
            .listen(2, EventKind::Click, 1, Filter::default())
            .listen(3, EventKind::Input, 1, Filter::default())
            .clone(),
    );
    add(
        "unlisten",
        B::new()
            .listen(2, EventKind::Click, 1, Filter::default())
            .unlisten(2, EventKind::Click, 1)
            .clone(),
    );
    add(
        "negative-zero-command",
        B::new().command(3, Command::Focus, -0.0).clone(),
    );
    add(
        "detached-modal",
        B::new()
            .create_element(5, Tag::Dialog)
            .command(5, Command::ShowModal, 0.0)
            .clone(),
    );
    add(
        "modal",
        B::new()
            .create_element(5, Tag::Dialog)
            .insert(2, 5, 0)
            .command(5, Command::ShowModal, 0.0)
            .clone(),
    );
    add(
        "inline-style",
        B::new().set_style(2, CssProperty::Color, "#abc").clone(),
    );
    add("remove-subtree", B::new().remove(2).clone());
    add(
        "stylesheet",
        B::new()
            .stylesheet(1, ".a { color: #abc; }")
            .set_class(2, "a")
            .clone(),
    );
    add(
        "bad-stylesheet",
        B::new()
            .stylesheet(1, ".a { background-image: url(https://evil.test); }")
            .clone(),
    );
    add("title", B::new().set_title("Varta 😀").clone());
    add(
        "remove-style",
        B::new()
            .set_style(2, CssProperty::Color, "#abc")
            .remove_style(2, CssProperty::Color)
            .clone(),
    );
    add("copy", B::new().copy_text(1, "copy").clone());
    add(
        "duplicate-copy",
        B::new().copy_text(1, "copy").copy_text(1, "again").clone(),
    );
    add(
        "bad-filter",
        B::new()
            .listen(
                2,
                EventKind::Click,
                1,
                Filter {
                    options: u32::MAX,
                    ..Default::default()
                },
            )
            .clone(),
    );
    add(
        "close",
        B::new()
            .create_element(5, Tag::Dialog)
            .insert(2, 5, 0)
            .command(5, Command::ShowModal, 0.0)
            .command(5, Command::Close, 0.0)
            .clone(),
    );
    add("focus", B::new().command(3, Command::Focus, 0.0).clone());
    add(
        "blur",
        B::new()
            .command(3, Command::Focus, 0.0)
            .command(3, Command::Blur, 0.0)
            .clone(),
    );
    let mut radios = B::new();
    radios
        .create_element(5, Tag::Form)
        .insert(1, 5, 0)
        .create_element(6, Tag::Form)
        .insert(1, 6, 0);
    for (id, parent, name) in [
        (7, 5, "g"),
        (8, 6, "g"),
        (9, 1, "g"),
        (11, 1, ""),
        (12, 1, ""),
        (13, 5, "g"),
    ] {
        radios
            .create_element(id, Tag::Input)
            .set_attribute(id, A::Type, "radio")
            .set_attribute(id, A::Name, name)
            .insert(parent, id, 0)
            .set_checked(id, 1, 0);
    }
    radios
        .create_element(10, Tag::Input)
        .set_attribute(10, A::Type, "radio")
        .set_attribute(10, A::Name, "g")
        .set_checked(10, 1, 0);
    cases.push((
        "radio-owner-and-dynamic-membership",
        vec![
            base().finish(),
            radios.finish(),
            B::new().insert(6, 13, 0).finish(),
            B::new()
                .set_attribute(13, A::Name, "")
                .set_checked(8, 1, 0)
                .finish(),
            B::new().insert(1, 10, 0).finish(),
            B::new().set_checked(7, 1, 0).remove(999).finish(),
        ],
    ));
    for (name, value, fallback) in [
        ("select-work-ascii", "X".repeat(4096), false),
        ("select-work-unicode", "😀".repeat(1024), false),
        ("select-work-mutated-text", "😀".repeat(1024), true),
    ] {
        let mut setup = B::new();
        setup.create_element(2, Tag::Select).insert(1, 2, 0);
        for id in 3..23 {
            setup
                .create_element(id, Tag::Option)
                .create_text(id + 100, "A")
                .insert(id, id + 100, 0)
                .insert(2, id, 0);
        }
        let mut grow = B::new();
        for id in 3..23 {
            if fallback {
                grow.set_text(id + 100, &value);
            } else {
                grow.set_attribute(id, A::Value, &value);
            }
        }
        let mut reset = B::new();
        reset.set_value(2, &value, 0);
        let mut duplicates = B::new();
        for _ in 0..256 {
            duplicates.set_value(2, "missing", 0);
        }
        duplicates
            .stylesheet(7, ".budget-probe { color:#123456; }")
            .set_class(2, "budget-probe")
            .set_value(2, "A", 0);
        let scans = |count, rollback| {
            let mut batch = B::new();
            if rollback {
                batch.set_title("must roll back");
            }
            for index in 0..count {
                batch.set_value(2, &format!("missing{index}"), 0);
            }
            batch.finish()
        };
        let mut shrink = B::new();
        for id in 3..23 {
            if fallback {
                shrink.set_text(id + 100, "A");
            } else {
                shrink.remove_attribute(id, A::Value);
            }
        }
        cases.push((
            name,
            vec![
                setup.finish(),
                duplicates.finish(),
                grow.finish(),
                scans(100, false),
                reset.finish(),
                scans(101, true),
                scans(128, true),
                scans(100, false),
                shrink.finish(),
                scans(1000, false),
            ],
        ));
    }
    let mut deep = B::new();
    deep.set_title("before");
    for id in 2..402 {
        deep.create_element(id, Tag::Div).insert(id - 1, id, 0);
    }
    deep.create_text(402, "A").insert(401, 402, 0);
    let mut scans = B::new();
    scans.set_title("must roll back");
    for _ in 0..2000 {
        scans.set_text(402, "B");
    }
    cases.push((
        "select-work-deep-owner",
        vec![
            deep.finish(),
            scans.finish(),
            B::new().set_text(402, "C").finish(),
        ],
    ));
    let observations=cases.into_iter().map(|(name,batches)|{
        let mut view=View::default();let mut broker=Denied;
        let results=batches.iter().map(|batch|{
            let result=view.commit(batch,false,&mut broker);
            let (budget, operation) = match result.as_ref().err() {
                Some(varta_view::ViewError::Invalid {operation, violation:varta_view::Violation::SelectWorkLimit}) => (true, *operation),
                _ => (false, None),
            };
            serde_json::json!({"ok":result.is_ok(),"error":result.err().map(|e|e.to_string()),"budget":budget,"operation":operation,"snapshot":view.snapshot()})
        }).collect::<Vec<_>>();
        serde_json::json!({"name":name,"batches":batches,"results":results})
    }).collect::<Vec<_>>();
    let css=CssProperty::ALL.iter().flat_map(|property|{
        ["0","1","-1","1px","-1px","10%","auto","none","inherit","var(--size)","rgb(1, 2, 3)","red","1e2","0.5","#abc","-0","0.0",".0","1PX","AUTO","CURRENTCOLOR","RGB(1, 2, 3)","VAR(--size)","var(--size, auto)","var(--size, 1px 2px)","var(--size, rgb(1,2,3))","var(--size, -0)","var(--size, 0.5)",
                    "minmax(1px, 2fr)","repeat(2, 1fr)","\"hello\""]
            .into_iter().map(move |value|serde_json::json!({"property":property.name(),"value":value,"valid":varta_common::view::css::property_value(*property,value)}))
    }).collect::<Vec<_>>();
    let events=EventKind::ALL.iter().map(|kind|{
        let record=varta_view::EventRecord{kind:*kind,handler:17,target:23,seq:31,flags:3,status:0,scroll_top:12.5,scroll_height:400.25,client_height:200.0,key:"Enter".into(),value:"\u{feff}Привіт 😀".into()};
        serde_json::json!({"kind":kind.name(),"values":{"handler":record.handler,"target":record.target,"seq":record.seq,"flags":record.flags,"status":record.status,"scroll_top":record.scroll_top,"scroll_height":record.scroll_height,"client_height":record.client_height,"key":record.key,"value":record.value},"bytes":record.encode()})
    }).collect::<Vec<_>>();
    let mut flex_view = View::default();
    let mut flex_batch = B::new();
    flex_batch
        .create_element(2, Tag::Div)
        .create_element(3, Tag::Span)
        .set_style(2, varta_common::view::CssProperty::Display, "flex")
        .set_style(3, varta_common::view::CssProperty::Display, "inline")
        .insert(1, 2, 0)
        .insert(2, 3, 0);
    let flex_bytes = flex_batch.finish();
    flex_view.commit(&flex_bytes, false, &mut Denied)?;
    let flex = serde_json::json!({"batch":flex_bytes,"cascade":flex_view.snapshot(),"projected":flex_view.snapshot_flex_blockified()});
    let mut switches = B::new();
    for (id, kind, name) in [(2, "checkbox", ""), (3, "radio", "g"), (4, "radio", "g")] {
        switches
            .create_element(id, Tag::Input)
            .set_attribute(id, A::Type, kind)
            .set_attribute(id, A::Name, name)
            .insert(1, id, 0);
        for (kind, slot) in [
            (EventKind::Click, 0),
            (EventKind::Input, 1),
            (EventKind::Change, 2),
            (EventKind::Keydown, 3),
        ] {
            switches.listen(id, kind, id * 10 + slot, Filter::default());
        }
    }
    let switch_batch = switches.finish();
    let mut switch_view = View::default();
    switch_view.commit(&switch_batch, false, &mut Denied)?;
    let mut steps = Vec::new();
    for (index, (node, arrow)) in [
        (2, false),
        (2, false),
        (3, false),
        (3, true),
        (2, false),
        (4, true),
    ]
    .into_iter()
    .enumerate()
    {
        let mut patch = B::new();
        if index >= 4 {
            let cancelled = if index == 4 { 2 } else { 3 };
            patch
                .unlisten(cancelled, EventKind::Click, cancelled * 10)
                .listen(
                    cancelled,
                    EventKind::Click,
                    cancelled * 10,
                    Filter {
                        options: ListenOption::PreventDefault as u32,
                        ..Filter::default()
                    },
                );
        }
        let patch = patch.finish();
        switch_view.commit(&patch, false, &mut Denied)?;
        let input = if arrow {
            Input::Key {
                target: Target::Node(node),
                key: "ArrowRight".into(),
                shift: false,
                ctrl: false,
                alt: false,
                meta: false,
                composing: false,
            }
        } else {
            Input::Click {
                target: Target::Node(node),
            }
        };
        switch_view.input(&input, &mut Denied)?;
        let events: Vec<_> = std::iter::from_fn(|| switch_view.next_delivery())
            .map(|item| item.record.encode())
            .collect();
        let snapshot = switch_view.snapshot();
        let fields=snapshot["tree"]["children"].as_array().expect("three fields").iter()
            .map(|field|serde_json::json!({"id":field["id"],"checked":field["checked"],"seq":field["seq"]})).collect::<Vec<_>>();
        steps.push(serde_json::json!({"node":node,"arrow":arrow,"patch":patch,"events":events,"fields":fields}));
    }
    let checkables = serde_json::json!({"batch":switch_batch,"steps":steps});
    let mut choices = B::new();
    choices
        .create_element(2, Tag::Select)
        .insert(1, 2, 0)
        .listen(2, EventKind::Input, 10, Filter::default())
        .listen(2, EventKind::Change, 11, Filter::default());
    for (id, value, label) in [(3, "blocked", "A"), (4, "same", "AA"), (5, "same", "AAA")] {
        choices
            .create_element(id, Tag::Option)
            .create_text(id + 100, label)
            .insert(id, id + 100, 0)
            .set_attribute(id, A::Value, value);
        if id == 3 {
            choices.set_attribute(id, A::Disabled, "");
        }
        choices.insert(2, id, 0);
    }
    let choice_batch = choices.finish();
    let mut choice_view = View::default();
    choice_view.commit(&choice_batch, false, &mut Denied)?;
    let mut steps = Vec::new();
    for (index, choice) in [
        Some(5),
        Some(5),
        Some(5),
        Some(4),
        None,
        Some(6),
        None,
        None,
        None,
        None,
        None,
    ]
    .into_iter()
    .enumerate()
    {
        let mut patch = B::new();
        match index {
            1 => {
                patch.set_value(2, "same", 1);
            }
            2 => {
                patch
                    .set_attribute(5, A::Selected, "")
                    .remove_attribute(5, A::Selected);
            }
            3 => {
                patch.set_attribute(5, A::Value, "changed");
            }
            4 => {
                patch.set_value(2, "missing", 2);
            }
            5 => {
                patch
                    .create_element(6, Tag::Option)
                    .create_text(106, "AAAA")
                    .insert(6, 106, 0)
                    .set_attribute(6, A::Value, "last")
                    .insert(2, 6, 0);
            }
            6 => {
                patch.remove(6);
            }
            7 => {
                for (id, label) in [(7, "A"), (8, "AA")] {
                    patch
                        .create_element(id, Tag::Option)
                        .create_text(id + 100, label)
                        .insert(id, id + 100, 0)
                        .set_attribute(id, A::Value, "fresh")
                        .set_attribute(id, A::Selected, "")
                        .insert(2, id, 0);
                }
            }
            8 => {
                patch.set_attribute(7, A::Selected, "");
            }
            9 => {
                patch
                    .set_value(2, "missing", 3)
                    .set_attribute(7, A::Selected, "");
            }
            10 => {
                patch
                    .remove_attribute(7, A::Selected)
                    .set_attribute(7, A::Selected, "");
            }
            _ => {}
        }
        let patch = patch.finish();
        choice_view.commit(&patch, false, &mut Denied)?;
        if let Some(option) = choice {
            choice_view.input(
                &Input::SelectOption {
                    target: Target::Node(2),
                    option,
                },
                &mut Denied,
            )?;
        }
        let events: Vec<_> = std::iter::from_fn(|| choice_view.next_delivery())
            .map(|event| event.record.encode())
            .collect();
        let snapshot = choice_view.snapshot();
        let field = &snapshot["tree"]["children"][0];
        let selected = field["children"]
            .as_array()
            .expect("options")
            .iter()
            .filter(|option| option["selected"] == true)
            .map(|option| option["id"].clone())
            .collect::<Vec<_>>();
        steps.push(
            serde_json::json!({"patch":patch,"choice":choice,"events":events,
            "field":{"value":field["value"],"seq":field["seq"],"selected":selected}}),
        );
    }
    let selects = serde_json::json!({"batch":choice_batch,"steps":steps});
    let output = serde_json::to_vec(
        &serde_json::json!({"cases":observations,"css":css,"events":events,"flex":flex,"checkables":checkables,"selects":selects}),
    )?;
    if let Some(path) = std::env::args_os().nth(1) {
        std::fs::write(path, output)?;
    } else {
        use std::io::Write;
        std::io::stdout().write_all(&output)?;
    }
    Ok(())
}
