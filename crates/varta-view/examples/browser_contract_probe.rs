//! Native view oracle for the independently executed generated browser model.
use varta_common::view::{Attribute as A, Command, CssProperty, EventKind, Tag};
use varta_view::{BatchWriter as B, Broker, Filter, View};

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
    let observations=cases.into_iter().map(|(name,batches)|{
        let mut view=View::default();let mut broker=Denied;
        let results=batches.iter().map(|batch|{
            let result=view.commit(batch,false,&mut broker);
            serde_json::json!({"ok":result.is_ok(),"error":result.err().map(|e|e.to_string()),"snapshot":view.snapshot()})
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
    let output = serde_json::to_vec(
        &serde_json::json!({"cases":observations,"css":css,"events":events,"flex":flex}),
    )?;
    if let Some(path) = std::env::args_os().nth(1) {
        std::fs::write(path, output)?;
    } else {
        use std::io::Write;
        std::io::stdout().write_all(&output)?;
    }
    Ok(())
}
