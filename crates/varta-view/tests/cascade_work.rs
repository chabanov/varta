use varta_common::view::{
    Command, DRAFT_MAX_CASCADE_WORK as LIMIT, DRAFT_STYLESHEET_NODE_WORK as WEIGHT, Tag,
};
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
fn commit(view: &mut View, batch: B) {
    view.commit(&batch.finish(), false, &mut Denied).unwrap();
}
fn setup(count: usize, source: &str, class: &str) -> B {
    let mut b = B::new();
    b.stylesheet(1, source);
    for id in 2..count as u32 + 2 {
        b.create_element(id, Tag::Button)
            .set_class(id, class)
            .insert(1, id, 0);
    }
    b
}
fn reject(view: &mut View, b: B, at: Option<usize>) {
    let before = view.snapshot();
    assert!(
        matches!(view.commit(&b.finish(),false,&mut Denied),Err(ViewError::Invalid{operation,violation:Violation::CascadeWorkLimit}) if operation==at)
    );
    assert_eq!(view.snapshot(), before);
    commit(view, B::new());
}
fn work(count: usize, source: &str, class: &str) -> usize {
    let rules = varta_common::view::stylesheet::parse(source).unwrap();
    WEIGHT + count * (WEIGHT + class.len() + rules.iter().map(|r| r.cascade_work()).sum::<usize>())
}
#[test]
fn exact_inventory_budget_rolls_back_and_deletion_reclaims_capacity() {
    let source = ".a{}".repeat(1337);
    let count = 128;
    let padding = LIMIT - work(count, &source, "a");
    let class = format!("a{}", " ".repeat(padding));
    let mut b = setup(count, &source, "a");
    b.set_class(2, &class);
    let mut view = View::default();
    commit(&mut view, b);
    reject(
        &mut view,
        B::new()
            .set_title("rollback")
            .set_class(2, &format!("{class} "))
            .clone(),
        None,
    );
    commit(
        &mut view,
        B::new()
            .remove(2)
            .set_class(3, &format!("{class} "))
            .clone(),
    );
}
#[test]
fn duplicate_candidate_anchors_are_counted_once_and_failed_sheet_replacement_cannot_poison_indices()
{
    let source = ".a,.a.b,.b{}".repeat(496);
    let count = 128;
    let class = "a a b";
    let padding = LIMIT - work(count, &source, class);
    assert!(padding < 16000);
    let mut b = setup(count, &source, class);
    b.set_class(2, &format!("{class}{}", " ".repeat(padding)));
    let mut view = View::default();
    commit(&mut view, b);
    reject(
        &mut view,
        B::new().stylesheet(1, &format!("{source}.a{{}}")).clone(),
        None,
    );
    commit(
        &mut view,
        B::new()
            .stylesheet(1, "")
            .stylesheet(2, ".unused {color:#abc}")
            .clone(),
    );
    commit(&mut view, B::new().stylesheet(1, ".a {color:#123}").clone());
    assert_eq!(
        view.snapshot()["tree"]["children"][0]["style"]["color"],
        "#123"
    );
}
#[test]
fn inherited_scopes_charge_ancestor_candidates_and_include_detached_components() {
    let source = ".a{color:var(--tone)}".repeat(100);
    let mut view = View::default();
    let mut b = setup(64, &source, "a");
    b.stylesheet(2, ":root{--tone:#abc}");
    commit(&mut view, b);
    let mut chain = B::new();
    for id in 3..66 {
        chain.insert(id - 1, id, 0);
    }
    reject(&mut view, chain.clone(), None);
    let mut b = B::new();
    b.stylesheet(2, "");
    for id in 3..66 {
        b.insert(id - 1, id, 0);
    }
    commit(&mut view, b);
    reject(
        &mut view,
        B::new().stylesheet(2, ":root{--tone:#abc}").clone(),
        None,
    );
    // The installed custom declarations are conservative even if unrelated to
    // the detached branch's actual inherited bindings.
    commit(&mut view, B::new().insert(1, 3, 0).clone());
    // Build the same high-cost state entirely detached in a fresh instance.
    let mut detached = B::new().stylesheet(1, &".a{}".repeat(1338)).clone();
    for id in 2..130 {
        detached.create_element(id, Tag::Span).set_class(id, "a");
    }
    reject(&mut View::default(), detached, None);
}
#[test]
fn command_reads_share_the_allowance_even_for_equal_focus_and_final_inventory() {
    let source = ".a{}".repeat(100);
    let mut view = View::default();
    commit(&mut view, setup(64, &source, "a"));
    let reads = LIMIT / work(64, &source, "a");
    let mut accepted = B::new();
    for _ in 0..reads - 1 {
        accepted.command(2, Command::Focus, 0.0);
    }
    commit(&mut view, accepted);
    let mut final_overflow = B::new();
    for _ in 0..reads {
        final_overflow.command(2, Command::Focus, 0.0);
    }
    reject(&mut view, final_overflow, None);
    let mut command_overflow = B::new();
    for _ in 0..=reads {
        command_overflow.command(2, Command::Focus, 0.0);
    }
    reject(&mut view, command_overflow, Some(reads));
}
#[test]
fn transient_excess_can_be_reclaimed_but_not_read_by_a_command() {
    let initial = ".a{}".repeat(1337);
    let expanded = format!("{initial}.a{{}}");
    let mut view = View::default();
    commit(&mut view, setup(128, &initial, "a"));
    reject(
        &mut view,
        B::new()
            .stylesheet(1, &expanded)
            .command(2, Command::Focus, 0.0)
            .stylesheet(1, "")
            .clone(),
        Some(1),
    );
    commit(
        &mut view,
        B::new().stylesheet(1, &expanded).stylesheet(1, "").clone(),
    );
}

#[test]
fn inactive_media_and_additional_class_tests_cannot_hide_potential_cost() {
    let source = format!(
        "@media (min-width:999999px){{{}}}",
        ".a.absent:checked{}".repeat(1338)
    );
    reject(&mut View::default(), setup(128, &source, "a"), None);
    let mut view = View::default();
    commit(&mut view, setup(128, &source, "unrelated"));
    commit(&mut view, B::new().stylesheet(1, "").clone());
}
