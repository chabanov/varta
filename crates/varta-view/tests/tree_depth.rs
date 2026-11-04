use varta_common::view::{Attribute, DRAFT_MAX_TREE_DEPTH as MAX, Tag};
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
fn chain(first: u32, count: usize, parent: Option<u32>) -> B {
    let mut batch = B::new();
    for offset in 0..count as u32 {
        let id = first + offset;
        batch.create_element(id, Tag::Span);
        if offset > 0 {
            batch.insert(id - 1, id, 0);
        } else if let Some(parent) = parent {
            batch.insert(parent, id, 0);
        }
    }
    batch
}
fn reject(view: &mut View, batch: B, operation: usize) {
    let before = view.snapshot();
    assert!(matches!(view.commit(&batch.finish(),false,&mut Denied),
        Err(ViewError::Invalid{operation:Some(at),violation:Violation::TreeDepthLimit}) if at==operation));
    assert_eq!(view.snapshot(), before);
    commit(view, B::new());
}
#[test]
fn depth_boundary_survives_snapshot_naming_serialization_and_drop() {
    const CHILD: &str = "VARTA_DEPTH_SNAPSHOT_CHILD";
    if std::env::var_os(CHILD).is_some() {
        // Isolate a potential stack abort from the parent harness. This is the
        // normal test-thread stack size, not an increased renderer allowance.
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                let mut view = View::default();
                let mut batch = B::new();
                batch.create_element(2, Tag::Button).insert(1, 2, 0);
                for id in 3..=MAX as u32 {
                    batch.create_element(id, Tag::Span).insert(id - 1, id, 0);
                }
                batch.create_text(MAX as u32 + 1, "deep name").insert(
                    MAX as u32,
                    MAX as u32 + 1,
                    0,
                );
                commit(&mut view, batch);
                let snapshot = view.snapshot();
                assert_eq!(
                    snapshot["accessibility"]["children"][0]["name"],
                    "deep name"
                );
                let mut node = &snapshot["tree"];
                for _ in 0..MAX {
                    node = &node["children"][0];
                }
                assert_eq!(node["text"], "deep name");
                assert!(!serde_json::to_vec(&snapshot).unwrap().is_empty());
                drop(snapshot);
                // Roles keep every ancestor in the accessibility projection,
                // rather than exercising only flattened generic containers.
                let mut roles = B::new();
                for id in 3..=MAX as u32 {
                    roles.set_attribute(id, Attribute::Role, "list");
                }
                commit(&mut view, roles);
                let snapshot = view.snapshot();
                let mut accessible = &snapshot["accessibility"]["children"][0];
                for _ in 1..MAX - 1 {
                    accessible = &accessible["children"][0];
                }
                assert_eq!(accessible["role"], "list");
                assert_eq!(accessible["children"][0]["name"], "deep name");
                assert!(!serde_json::to_vec(&snapshot).unwrap().is_empty());
                drop(snapshot.clone());
            })
            .unwrap()
            .join()
            .unwrap();
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "tree_depth::depth_boundary_survives_snapshot_naming_serialization_and_drop",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "subprocess must execute its exact case: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        output.status.success(),
        "depth boundary must not abort its host: {}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn depth_accumulates_across_commits_and_checks_detached_subtree_height() {
    let mut view = View::default();
    commit(&mut view, chain(2, MAX / 2 + 1, None));
    commit(
        &mut view,
        chain(3 + MAX as u32 / 2, MAX / 2, Some(2 + MAX as u32 / 2)),
    );
    // Detached component depth is exactly MAX; attachment would add one edge.
    reject(
        &mut view,
        B::new().set_title("rollback").insert(1, 2, 0).clone(),
        1,
    );
    commit(
        &mut view,
        B::new().remove(MAX as u32 + 2).insert(1, 2, 0).clone(),
    );
    reject(
        &mut view,
        B::new()
            .create_text(9000, "too deep")
            .insert(MAX as u32 + 1, 9000, 0)
            .clone(),
        1,
    );
    commit(&mut view, B::new().remove(2).set_title("reclaimed").clone());
    assert_eq!(view.snapshot()["title"], "reclaimed");
}
#[test]
fn reparenting_checks_descendants_and_preserves_prior_order_on_rejection() {
    let mut view = View::default();
    commit(&mut view, chain(2, MAX / 2, Some(1)));
    commit(&mut view, chain(1000, MAX / 2, Some(1)));
    let parent = 1000 + MAX as u32 / 2 - 1;
    commit(&mut view, B::new().insert(parent, 2, 0).clone());
    commit(
        &mut view,
        B::new()
            .create_element(2000, Tag::Span)
            .insert(parent, 2000, 2)
            .clone(),
    );
    reject(
        &mut view,
        B::new().set_title("rollback").insert(2000, 2, 0).clone(),
        1,
    );
    commit(&mut view, B::new().insert(1, 2, 1000).clone());
    assert_eq!(view.snapshot()["tree"]["children"][0]["id"], 2);
}

#[test]
fn over_deep_transient_insertion_is_not_repaired_by_later_removal() {
    let mut view = View::default();
    commit(&mut view, chain(2, MAX, Some(1)));
    reject(
        &mut view,
        B::new()
            .create_element(9000, Tag::Span)
            .insert(MAX as u32 + 1, 9000, 0)
            .remove(9000)
            .clone(),
        1,
    );
}
