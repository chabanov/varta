//! Clipboard I/O is represented by host tasks, never an ambient guest import.
use super::*;
use varta_view::{
    ClipboardAction as Action, ClipboardData as Data, ClipboardError, ClipboardKey,
    ClipboardTask as Task,
};
fn fixture(change: &str, prevent: bool) -> crate::Application {
    let source = format!(
        r#"
export function main(): void {{
 const field=View.node();View.createElement(field,{input});View.setAttribute(field,{id},'field');View.setValue(field,'AB');
 View.on(field,{keydown},(event:ViewEvent):void=>{{
  console.log('key');{change}
  View.copyText('from key',(status:number):void=>console.log(`key copy ${{status}}`));
 }},{prevent},0,0,0);
 View.on(field,{edit},(event:ViewEvent):void=>{{
  console.log(`input ${{event.value}}`);
  View.copyText('from input',(status:number):void=>console.log(`input copy ${{status}}`));
 }},0,0,0,0);
 View.insert({root},field,0);View.commit();
}}
"#,
        input = view::Tag::Input.code(),
        id = view::Attribute::Id.code(),
        keydown = view::EventKind::Keydown.code(),
        edit = view::EventKind::Input.code(),
        root = view::ROOT_NODE,
        prevent = if prevent {
            view::ListenOption::PreventDefault as u32
        } else {
            0
        }
    );
    let mut app = instance(&source, vec![clipboard(true)], None);
    assert!(app.failure().is_none(), "{:?}", app.failure());
    app.view_input(&varta_view::Input::TextSelect {
        target: varta_view::Target::Ident("#field".into()),
        anchor: 0,
        focus: 2,
        activate: false,
    })
    .unwrap();
    app.run();
    app.take_output();
    app
}
fn begin(app: &mut crate::Application, action: Action) -> Option<Task> {
    let target = app.view_clipboard_target();
    app.view_begin_clipboard(
        target.as_ref(),
        &ClipboardKey {
            action,
            key: "v".into(),
            shift: false,
            ctrl: false,
            alt: false,
            meta: true,
        },
    )
    .unwrap()
}
fn value(app: &crate::Application) -> String {
    by_id(&app.view_snapshot()["tree"], "field").unwrap()["value"]
        .as_str()
        .unwrap()
        .into()
}
#[test]
fn clipboard_completion_delivers_input_later_without_fresh_gesture_authority() {
    let mut app = fixture("", false);
    let task = begin(&mut app, Action::Paste).unwrap();
    assert!(app.view_clipboard_pending());
    assert_eq!(value(&app), "AB");
    assert!(app.take_output().is_empty());
    assert_eq!(app.run(), crate::Step::Idle);
    assert_eq!(
        app.take_output(),
        ["varta log: key", "varta log: key copy 0"]
    );
    assert_eq!(app.view_snapshot()["clipboard"], "from key");
    assert!(
        app.view_complete_clipboard(task.token(), Ok(Data::Text(Some("Привіт😀".into()))))
            .unwrap()
    );
    assert!(!app.view_clipboard_pending());
    assert!(app.take_output().is_empty());
    assert_eq!(app.run(), crate::Step::Idle);
    assert_eq!(
        app.take_output(),
        ["varta log: input Привіт😀", "varta log: input copy 3"]
    );
    assert_eq!(app.view_snapshot()["clipboard"], "from key");
}
#[test]
fn guest_changes_during_io_invalidate_the_original_cut_and_paste_bindings() {
    for action in [Action::Cut, Action::Paste] {
        let mut app = fixture("View.setValue(field,'server');", false);
        let task = begin(&mut app, action).unwrap();
        assert_eq!(value(&app), "AB");
        app.run();
        assert_eq!(value(&app), "server");
        app.take_output();
        let result = if action == Action::Paste {
            Data::Text(Some("late".into()))
        } else {
            Data::Written
        };
        assert!(
            !app.view_complete_clipboard(task.token(), Ok(result))
                .unwrap()
        );
        app.run();
        assert_eq!(value(&app), "server");
        assert!(app.take_output().is_empty());
        assert!(!app.view_clipboard_pending());
    }
}
#[test]
fn late_completion_cannot_restart_a_trapping_instance() {
    let mut app = fixture("throw new Error('stopped');", false);
    let task = begin(&mut app, Action::Paste).unwrap();
    app.run();
    assert!(app.failure().is_some());
    assert!(app.view_clipboard_target().is_none());
    assert!(matches!(
        app.view_complete_clipboard(task.token(), Ok(Data::Text(Some("late".into())))),
        Err(crate::ApplicationError::Terminated)
    ));
    assert_eq!(value(&app), "AB");
}
#[test]
fn prevented_default_never_creates_a_platform_task() {
    let mut app = fixture("", true);
    assert!(begin(&mut app, Action::Cut).is_none());
    assert!(!app.view_clipboard_pending());
    app.run();
    assert_eq!(value(&app), "AB");
}
#[test]
fn failed_io_is_recoverable_and_does_not_edit_or_grant_a_new_gesture() {
    let mut app = fixture("", false);
    let task = begin(&mut app, Action::Cut).unwrap();
    app.run();
    app.take_output();
    assert!(
        !app.view_complete_clipboard(task.token(), Err(ClipboardError::Failed))
            .unwrap()
    );
    assert!(!app.view_clipboard_pending());
    assert_eq!(value(&app), "AB");
    assert_eq!(app.state(), crate::InstanceState::Running);
    assert!(app.failure().is_none());
    let task = begin(&mut app, Action::Paste).unwrap();
    app.run();
    app.take_output();
    assert!(
        app.view_complete_clipboard(task.token(), Ok(Data::Text(Some("recovered".into()))))
            .unwrap()
    );
    app.run();
    assert_eq!(value(&app), "recovered");
}
#[test]
fn completions_remain_bound_to_the_original_application() {
    let mut a = fixture("", false);
    let mut b = fixture("", false);
    let first = begin(&mut a, Action::Paste).unwrap();
    let second = begin(&mut b, Action::Paste).unwrap();
    assert!(
        !b.view_complete_clipboard(first.token(), Ok(Data::Text(Some("foreign".into()))))
            .unwrap()
    );
    assert!(b.view_clipboard_pending());
    assert_eq!(value(&b), "AB");
    assert!(
        b.view_complete_clipboard(second.token(), Ok(Data::Text(Some("own".into()))))
            .unwrap()
    );
    assert_eq!(value(&b), "own");
}
