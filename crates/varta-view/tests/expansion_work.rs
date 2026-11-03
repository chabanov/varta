use varta_common::view::{Command, CssProperty, DRAFT_MAX_VALUE_BYTES, Tag};
use varta_view::{Access, BatchWriter as B, Broker, View, ViewError, Violation};
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
fn commit(view: &mut View, b: B) {
    view.commit(&b.finish(), false, &mut Denied).unwrap();
}
fn setup(count: u32, extra: &str) -> B {
    let literal = format!("\"{}\"", "a".repeat(DRAFT_MAX_VALUE_BYTES / 2 - 2));
    let mut b = B::new();
    b.stylesheet(
        1,
        &format!(":root{{--big:{literal};{extra}}}.a{{font-family:var(--big)}}"),
    );
    for id in 2..count + 2 {
        b.create_element(id, Tag::Button)
            .set_class(id, "a")
            .insert(1, id, 0);
    }
    b
}
fn reject(view: &mut View, b: B, operation: Option<usize>) {
    let before = view.snapshot();
    assert!(
        matches!(view.commit(&b.finish(),false,&mut Denied),Err(ViewError::Invalid{operation:at,violation:Violation::ExpandedValueLimit}) if at==operation)
    );
    assert_eq!(view.snapshot(), before);
    commit(view, B::new());
    assert_eq!(view.snapshot(), before);
}
#[test]
fn exact_expanded_allowance_and_one_utf16_unit_overflow_preserve_prior_state() {
    let mut view = View::default();
    commit(&mut view, setup(127, ""));
    reject(
        &mut view,
        B::new().set_style(2, CssProperty::Width, "0").clone(),
        None,
    );
    commit(
        &mut view,
        B::new()
            .set_style(2, CssProperty::Width, "0")
            .remove_style(2, CssProperty::Width)
            .clone(),
    );
    commit(
        &mut view,
        B::new()
            .remove(2)
            .set_style(3, CssProperty::Width, "0")
            .clone(),
    );
}
#[test]
fn repeated_equal_commands_charge_full_inventory_and_report_command_or_final_position() {
    let mut view = View::default();
    commit(&mut view, setup(63, ""));
    commit(&mut view, B::new().command(2, Command::Focus, 0.0).clone());
    reject(
        &mut view,
        B::new()
            .command(2, Command::Focus, 0.0)
            .command(2, Command::Focus, 0.0)
            .clone(),
        None,
    );
    reject(
        &mut view,
        B::new()
            .command(2, Command::Focus, 0.0)
            .command(2, Command::Focus, 0.0)
            .command(2, Command::Focus, 0.0)
            .clone(),
        Some(2),
    );
}
#[test]
fn excessive_transient_read_cannot_be_repaired_after_the_command() {
    let mut view = View::default();
    commit(&mut view, setup(127, ""));
    reject(
        &mut view,
        B::new()
            .set_style(2, CssProperty::Width, "0")
            .command(2, Command::Focus, 0.0)
            .remove_style(2, CssProperty::Width)
            .clone(),
        Some(1),
    );
}
#[test]
fn reference_only_aliases_do_not_duplicate_fresh_component_text() {
    let aliases = (0..100)
        .map(|i| {
            format!(
                "--alias{i}:var(--{});",
                if i == 0 {
                    "big".to_owned()
                } else {
                    format!("alias{}", i - 1)
                }
            )
        })
        .collect::<String>();
    let mut view = View::default();
    commit(&mut view, setup(127, &aliases));
}

#[test]
fn inactive_candidates_and_detached_scopes_do_not_hide_expansion_capacity() {
    let literal = format!("\"{}\"", "a".repeat(DRAFT_MAX_VALUE_BYTES / 2 - 2));
    let mut view = View::default();
    commit(&mut view, setup(127, ""));
    let inactive = format!(
        "@media (min-width:999999px){{:root{{--big:{literal}}}}}.a.never:hover{{font-family:var(--big)}}"
    );
    commit(&mut view, B::new().stylesheet(1, &inactive).clone());
    reject(
        &mut view,
        B::new().set_style(2, CssProperty::Width, "0").clone(),
        None,
    );
    view.set_environment(varta_view::Environment {
        width: 1_000_000.0,
        dark: false,
        ..Default::default()
    })
    .unwrap();
    commit(&mut view, B::new());
    let mut detached = View::default();
    commit(&mut detached, setup(126, ""));
    let source = format!(":root,.seed{{--big:{literal}}}.a{{font-family:var(--big)}}");
    commit(&mut detached, B::new().stylesheet(1, &source).clone());
    reject(
        &mut detached,
        B::new()
            .create_element(9000, Tag::Div)
            .set_class(9000, "seed")
            .create_element(9001, Tag::Button)
            .set_class(9001, "a")
            .insert(9000, 9001, 0)
            .clone(),
        None,
    );
    commit(
        &mut detached,
        B::new()
            .create_element(9000, Tag::Div)
            .set_class(9000, "seed")
            .clone(),
    );
    commit(&mut detached, B::new().insert(1, 9000, 0).clone());
}

#[test]
fn uncapped_standard_literals_are_not_mistaken_for_invalid_custom_values() {
    let mut view = View::default();
    let mut batch = B::new();
    batch.stylesheet(
        1,
        &format!(
            ".a{{font-family:\"{}\"}}",
            "a".repeat(DRAFT_MAX_VALUE_BYTES / 2)
        ),
    );
    for node in 2..129 {
        batch
            .create_element(node, Tag::Button)
            .set_class(node, "a")
            .insert(1, node, 0);
    }
    commit(&mut view, batch);
    reject(
        &mut view,
        B::new()
            .create_element(9000, Tag::Button)
            .set_class(9000, "a")
            .insert(1, 9000, 0)
            .clone(),
        None,
    );
}
