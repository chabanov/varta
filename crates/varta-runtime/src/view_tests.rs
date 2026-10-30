//! V38 view protocol: the broker decisions of the view capabilities.
use crate::policy::Policy;
use varta_common::{CapabilityGrant, CapabilityValue, view};
use varta_view::Access;

fn links(patterns: &[&str]) -> CapabilityGrant {
    CapabilityGrant {
        id: view::OPEN_URL_CAPABILITY.into(),
        value: CapabilityValue::UrlGlobs(patterns.iter().map(|p| (*p).into()).collect()),
    }
}
fn clipboard(enabled: bool) -> CapabilityGrant {
    CapabilityGrant {
        id: view::CLIPBOARD_WRITE_CAPABILITY.into(),
        value: CapabilityValue::Boolean(enabled),
    }
}

#[test]
fn open_url_decisions_follow_request_ceiling_and_the_any_https_form() {
    let docs = "https://docs.test/guide#intro";
    let other = "https://other.test/";
    let request = [links(&["https://docs.test/**", view::ANY_HTTPS_PATTERN])];
    let policy = Policy::with_ceiling(&request, None);
    assert!(policy.link_requested(docs) && policy.link_requested(other));
    assert_eq!(policy.open_url(docs), Access::Granted);
    assert_eq!(policy.open_url(other), Access::Prompt);
    for invalid in [
        "http://docs.test/",
        "https://user@docs.test/",
        "https://docs.test/a b",
        "javascript:alert(1)",
        "https://docs.test/\\x",
    ] {
        assert_eq!(policy.open_url(invalid), Access::Denied, "{invalid}");
        assert!(!policy.link_requested(invalid) || invalid.starts_with("http://"));
    }
    // A ceiling without the any-HTTPS form refuses other links without a prompt.
    let ceiling = [links(&["https://docs.test/**"])];
    let narrowed = Policy::with_ceiling(&request, Some(&ceiling));
    assert_eq!(narrowed.open_url(docs), Access::Granted);
    assert_eq!(narrowed.open_url(other), Access::Denied);
    // A ceiling that only has the any-HTTPS form makes every link prompt.
    let any = [links(&[view::ANY_HTTPS_PATTERN])];
    let prompting = Policy::with_ceiling(&request, Some(&any));
    assert_eq!(prompting.open_url(docs), Access::Prompt);
    // An empty ceiling denies; nothing is requested without a grant.
    assert_eq!(
        Policy::with_ceiling(&request, Some(&[])).open_url(docs),
        Access::Denied
    );
    assert!(!Policy::new(&[]).link_requested(docs));
    // `http://*` is never a wildcard: only the exact any-HTTPS form is.
    assert!(!Policy::new(&[links(&["http://plain.test/**"])]).link_requested(other));
}

#[test]
fn clipboard_grants_are_exact_booleans_of_both_parties() {
    assert!(Policy::new(&[clipboard(true)]).enabled(view::CLIPBOARD_WRITE_CAPABILITY));
    assert!(!Policy::new(&[clipboard(false)]).enabled(view::CLIPBOARD_WRITE_CAPABILITY));
    assert!(
        !Policy::with_ceiling(&[clipboard(true)], Some(&[clipboard(false)]))
            .enabled(view::CLIPBOARD_WRITE_CAPABILITY)
    );
}

use crate::VartaRuntime;
use varta_common::language::LanguageProfile as P;

const V38: P = P::CheckedF64Utf16V38;

fn compile(source: &str) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, V38)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}

fn script(json: &str) -> crate::ViewScript {
    crate::ViewScript::parse(json.as_bytes()).unwrap()
}

/// A scripted headless view session of an application instance.
struct ViewRun {
    output: crate::RunOutput,
    snapshot: serde_json::Value,
    commits: usize,
    report: crate::RunReport,
}

/// Run `script` against an application instance with the package requests
/// and, when supplied, the host ceiling; its failure, or what it produced.
fn run_view(
    bytes: &[u8],
    requested: &[CapabilityGrant],
    ceiling: Option<&[CapabilityGrant]>,
    script: &crate::ViewScript,
) -> Result<ViewRun, crate::RuntimeError> {
    let options = crate::ApplicationOptions {
        run: crate::RunOptions {
            requested: requested.to_vec(),
            ceiling: ceiling.map(<[CapabilityGrant]>::to_vec),
            ..crate::RunOptions::default()
        },
        ..crate::ApplicationOptions::default()
    };
    let crate::ViewReport {
        mut report,
        snapshot,
        commits,
    } = VartaRuntime::new().run_view(bytes, options, script);
    if let Some(failure) = report.failure.take() {
        return Err(failure);
    }
    Ok(ViewRun {
        output: std::mem::take(&mut report.output),
        snapshot: snapshot.unwrap(),
        commits,
        report,
    })
}

fn run(source: &str, inputs: &str, grants: &[CapabilityGrant]) -> ViewRun {
    let bytes = compile(source);
    run_view(&bytes, grants, None, &script(inputs)).unwrap_or_else(|e| panic!("{e}\n{source}"))
}

/// In-memory TSX modules: the entry `app.tsx` and its imports.
struct Modules(&'static [(&'static str, &'static str)]);
impl varta_compiler::ModuleLoader for Modules {
    fn resolve(
        &self,
        _: &std::path::Path,
        specifier: &str,
    ) -> Result<std::path::PathBuf, varta_compiler::CompilerError> {
        Ok(std::path::PathBuf::from(specifier.trim_start_matches("./")))
    }
    fn load(&self, path: &std::path::Path) -> Result<String, varta_compiler::CompilerError> {
        self.0
            .iter()
            .find(|(name, _)| std::path::Path::new(name) == path)
            .map(|(_, text)| (*text).to_owned())
            .ok_or_else(|| varta_compiler::CompilerError::Module(format!("{}", path.display())))
    }
}

pub(crate) const APP: &str = include_str!("view_cases/app.tsx");
pub(crate) const SIGNALS: &str = include_str!("view_cases/signals.ts");

fn compile_tsx(entry: &str, modules: &'static [(&'static str, &'static str)]) -> Vec<u8> {
    let bytes = varta_compiler::compile_modules_with_profile(
        varta_compiler::ModuleSource {
            path: "app.tsx".into(),
            text: entry.into(),
        },
        &Modules(modules),
        V38,
    )
    .unwrap_or_else(|e| panic!("{e}"))
    .wasm;
    wasmtime::Module::new(&VartaRuntime::new().engine, &bytes).unwrap();
    bytes
}

fn by_id<'a>(node: &'a serde_json::Value, ident: &str) -> Option<&'a serde_json::Value> {
    if node["attributes"]["id"] == ident {
        return Some(node);
    }
    node["children"]
        .as_array()?
        .iter()
        .find_map(|child| by_id(child, ident))
}

fn text_of(node: &serde_json::Value) -> String {
    if let Some(text) = node["text"].as_str() {
        return text.into();
    }
    node["children"]
        .as_array()
        .map(|children| children.iter().map(text_of).collect())
        .unwrap_or_default()
}

#[test]
fn a_tsx_app_runs_headless_with_scripted_inputs() {
    let bytes = compile_tsx(APP, &[("signals.ts", SIGNALS)]);
    let inputs = r##"{"inputs":[
        {"input":"click","target":"#inc"},
        {"input":"click","target":"#inc"},
        {"input":"type","target":"#box","text":"hello"},
        {"input":"key","target":"#box","key":"Enter"},
        {"input":"type","target":"#box","text":"two"},
        {"input":"key","target":"#box","key":"Enter","shift":true},
        {"input":"key","target":"#box","key":"Enter","composing":true},
        {"input":"click","target":"#copy"}
    ]}"##;
    let grants = [clipboard(true)];
    let run = run_view(&bytes, &grants, None, &script(inputs)).unwrap_or_else(|e| panic!("{e}"));
    let tree = &run.snapshot["tree"];
    assert_eq!(text_of(by_id(tree, "count").unwrap()), "3");
    assert_eq!(text_of(by_id(tree, "sent").unwrap()), "hello");
    let field = by_id(tree, "box").unwrap();
    assert_eq!(field["value"], "two\n");
    assert_eq!(field["style"]["field-sizing"], "content");
    assert_eq!(text_of(by_id(tree, "status").unwrap()), "copied");
    assert_eq!(run.snapshot["clipboard"], "hello");
    assert_eq!(by_id(tree, "inc").unwrap()["style"]["color"], "#fff");
    let a11y = &run.snapshot["accessibility"]["children"][0];
    assert_eq!(a11y["role"], "main");
}

/// The guest encoder keeps an L0 batch cheap: until per-turn budgets land,
/// every turn of a view shares one execution's production fuel, so the
/// example app's first render must stay well inside it.
#[test]
fn the_initial_render_of_the_example_app_stays_inside_production_fuel() {
    let bytes = compile_tsx(APP, &[("signals.ts", SIGNALS)]);
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&bytes).unwrap();
    let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
    let instance = execution.instantiate_prepared(&module).unwrap();
    let start = instance.get_func(&mut execution.store, "_start").unwrap();
    start.call(&mut execution.store, &[], &mut []).unwrap();
    let used = varta_common::abi::FUEL - execution.store.get_fuel().unwrap();
    assert!(
        used < varta_common::abi::FUEL / 2,
        "first render used {used} fuel"
    );
    assert_eq!(execution.store.data().view.commits(), 1);
}

const COUNTER: &str = r#"
export function main(): void {
  const root = View.node();
  View.createElement(root, 1);
  View.setAttribute(root, 1, 'app');
  const label = View.node();
  View.createText(label, '0');
  View.insert(root, label, 0);
  let count = 0;
  View.on(root, 1, (event: ViewEvent): void => {
    count += 1;
    View.setText(label, `${count}`);
    console.log(`click ${event.target} ${count}`);
  }, 0, 0, 0, 0);
  View.mount(root);
}
"#;

#[test]
fn an_l0_program_renders_and_handles_scripted_clicks() {
    let run = run(
        COUNTER,
        r##"{"inputs":[{"input":"click","target":"#app"},{"input":"click","target":"#app"}]}"##,
        &[],
    );
    assert_eq!(run.commits, 3);
    assert_eq!(
        run.snapshot["tree"]["children"][0]["children"][0]["text"],
        "2"
    );
    assert_eq!(
        run.output.lines,
        ["varta log: click 2 1", "varta log: click 2 2"]
    );
}

fn failure(source: &str, inputs: &str, grants: &[CapabilityGrant]) -> crate::RuntimeError {
    let bytes = compile(source);
    run_view(&bytes, grants, None, &script(inputs))
        .err()
        .unwrap_or_else(|| panic!("ran:\n{source}"))
}

fn invalid(error: crate::RuntimeError) -> varta_view::Violation {
    match error {
        crate::RuntimeError::View(varta_view::ViewError::Invalid { violation, .. }) => violation,
        other => panic!("expected an invalid batch, got {other}"),
    }
}

const NO_INPUTS: &str = r#"{"inputs":[]}"#;

#[test]
fn invalid_guest_batches_end_the_instance_before_any_effect() {
    // The root id is host-owned; a guest cannot create it.
    let source = "export function main(): void { View.createElement(1, 1); View.commit(); }";
    assert_eq!(
        invalid(failure(source, NO_INPUTS, &[])),
        varta_view::Violation::DuplicateNode(1)
    );
    // Lone surrogates never enter the tree.
    let source =
        "export function main(): void { View.createText(View.node(), '\\uD800'); View.commit(); }";
    assert_eq!(
        invalid(failure(source, NO_INPUTS, &[])),
        varta_view::Violation::LoneSurrogate
    );
    // Unknown codes fail closed.
    let source =
        "export function main(): void { View.createElement(View.node(), 99); View.commit(); }";
    assert_eq!(
        invalid(failure(source, NO_INPUTS, &[])),
        varta_view::Violation::UnknownTag
    );
    // An external link must match a `ui.open_url` pattern of the request.
    let source = "export function main(): void { const a = View.node(); View.createElement(a, 21); View.setAttribute(a, 12, 'https://evil.test/'); View.mount(a); }";
    assert_eq!(
        invalid(failure(source, NO_INPUTS, &[])),
        varta_view::Violation::InvalidAttributeValue("href")
    );
}

#[test]
fn a_turn_commits_at_most_one_batch() {
    let source = "export function main(): void { View.setTitle('a'); View.commit(); View.setTitle('b'); View.commit(); }";
    assert!(matches!(
        failure(source, NO_INPUTS, &[]),
        crate::RuntimeError::View(varta_view::ViewError::SecondCommit)
    ));
    // Each event turn gets its own commit.
    let run = run(
        COUNTER,
        r##"{"inputs":[{"input":"click","target":"#app"}]}"##,
        &[],
    );
    assert_eq!(run.commits, 2);
}

#[test]
fn handler_exceptions_and_bad_script_targets_are_errors() {
    let source = "export function main(): void { const b = View.node(); View.createElement(b, 20); View.setAttribute(b, 1, 'b'); View.on(b, 1, (event: ViewEvent): void => { throw new Error('boom'); }, 0, 0, 0, 0); View.mount(b); }";
    let error = failure(
        source,
        r##"{"inputs":[{"input":"click","target":"#b"}]}"##,
        &[],
    );
    assert!(matches!(error, crate::RuntimeError::Uncaught(_)), "{error}");
    let error = failure(
        COUNTER,
        r##"{"inputs":[{"input":"click","target":"#missing"}]}"##,
        &[],
    );
    assert!(
        matches!(error, crate::RuntimeError::ViewInput(_)),
        "{error}"
    );
}

const COPY: &str = r#"
export function main(): void {
  const b = View.node();
  View.createElement(b, 20);
  View.setAttribute(b, 1, 'copy');
  View.on(b, 1, (event: ViewEvent): void => {
    View.copyText('copied text', (status: number): void => console.log(`click copy ${status}`));
  }, 0, 0, 0, 0);
  View.copyText('from main', (status: number): void => console.log(`main copy ${status}`));
  View.mount(b);
}
"#;

#[test]
fn copy_text_results_reach_the_guest_and_denials_are_recoverable() {
    let click = r##"{"inputs":[{"input":"click","target":"#copy"}]}"##;
    // Granted: the main turn is not a gesture (invalid_context), the click is.
    let granted = run(COPY, click, &[clipboard(true)]);
    eprintln!(
        "DEBUG {:?} {:?}",
        granted.report.budget.as_ref().map(|b| b.lines()),
        granted.commits
    );
    assert_eq!(
        granted.output.lines,
        ["varta log: main copy 3", "varta log: click copy 0"]
    );
    assert_eq!(granted.snapshot["clipboard"], "copied text");
    // Denied by default, and by a ceiling: `not_allowed`, never a trap.
    for (requested, ceiling) in [
        (vec![], None),
        (vec![clipboard(true)], Some(vec![clipboard(false)])),
    ] {
        let bytes = compile(COPY);
        let denied = run_view(&bytes, &requested, ceiling.as_deref(), &script(click)).unwrap();
        assert_eq!(
            denied.output.lines,
            ["varta log: main copy 3", "varta log: click copy 1"]
        );
        assert!(denied.snapshot["clipboard"].is_null());
    }
}

const LINK: &str = r#"
export function main(): void {
  const a = View.node();
  View.createElement(a, 21);
  View.setAttribute(a, 1, 'docs');
  View.setAttribute(a, 12, 'https://docs.test/guide');
  const text = View.node();
  View.createText(text, 'Docs');
  View.insert(a, text, 0);
  View.onLinkResult((event: ViewEvent): void => console.log(`${event.key} ${event.target} ${event.status}`));
  View.mount(a);
}
"#;

#[test]
fn external_links_follow_the_broker_and_report_results() {
    let click = r##"{"inputs":[{"input":"click","target":"#docs"}]}"##;
    let explicit = [links(&["https://docs.test/**"])];
    let opened = run(LINK, click, &explicit);
    assert_eq!(opened.output.lines, ["varta log: open_url 2 0"]);
    assert_eq!(opened.snapshot["opened"][0], "https://docs.test/guide");
    // The any-HTTPS form prompts on every activation; headless prompts are
    // scripted, and an unanswered prompt refuses.
    let any = [links(&[view::ANY_HTTPS_PATTERN])];
    let refused = run(LINK, click, &any);
    assert_eq!(refused.output.lines, ["varta log: open_url 2 1"]);
    assert_eq!(refused.snapshot["prompts"], 1);
    let allowed = run(
        LINK,
        r##"{"platform":{"prompt":"allow"},"inputs":[{"input":"click","target":"#docs"}]}"##,
        &any,
    );
    assert_eq!(allowed.output.lines, ["varta log: open_url 2 0"]);
    // A ceiling without the pattern refuses without a prompt.
    let bytes = compile(LINK);
    let ceiling = run_view(&bytes, &explicit, Some(&[]), &script(click)).unwrap();
    assert_eq!(ceiling.output.lines, ["varta log: open_url 2 1"]);
    assert_eq!(ceiling.snapshot["prompts"], 0);
}

#[test]
fn earlier_profiles_cannot_reach_the_view() {
    let error = varta_compiler::compile_typescript_with_profile(
        "export function main(): void { View.commit(); }",
        P::CheckedF64Utf16V37,
    );
    assert!(error.is_err());
}

// Application hosting (v38 `turn_budgets` with `view_protocol`): view
// records are host tasks of the instance, and the view capabilities are
// decided by permission broker v1.

const FIELD: &str = r#"
export function main(): void {
  const f = View.node();
  View.createElement(f, 24);
  View.setAttribute(f, 1, 'name');
  View.on(f, 2, (event: ViewEvent): void => console.log(`input ${event.value}`), 0, 0, 0, 0);
  const b = View.node();
  View.createElement(b, 20);
  View.setAttribute(b, 1, 'go');
  View.on(b, 1, (event: ViewEvent): void => console.log('click'), 0, 0, 0, 0);
  const root = View.node();
  View.createElement(root, 1);
  View.insert(root, f, 0);
  View.insert(root, b, 0);
  View.mount(root);
}
"#;

fn instance(
    source: &str,
    requested: Vec<CapabilityGrant>,
    prompter: Option<Box<dyn crate::PermissionPrompter>>,
) -> crate::Application {
    VartaRuntime::new()
        .start_application(
            &compile(source),
            crate::ApplicationOptions {
                run: crate::RunOptions {
                    requested,
                    ..crate::RunOptions::default()
                },
                prompter,
                ..crate::ApplicationOptions::default()
            },
        )
        .unwrap()
}

fn input(json: &str) -> varta_view::Input {
    serde_json::from_str(json).unwrap()
}

#[test]
fn view_records_are_host_tasks_with_fresh_turns_and_queue_coalescing() {
    let mut app = instance(FIELD, vec![], None);
    assert!(app.failure().is_none(), "{:?}", app.failure());
    // Two edits before any turn: the coalescible `input` records of one
    // listener keep only the newest; the click is its own task.
    app.view_input(&input(r##"{"input":"type","target":"#name","text":"a"}"##))
        .unwrap();
    app.view_input(&input(r##"{"input":"type","target":"#name","text":"b"}"##))
        .unwrap();
    app.view_input(&input(r##"{"input":"click","target":"#go"}"##))
        .unwrap();
    assert_eq!(app.run(), crate::Step::Idle);
    assert_eq!(
        app.take_output(),
        ["varta log: input ab", "varta log: click"]
    );
    let report = app.budget_report();
    let kinds: Vec<_> = report.turns.iter().map(|turn| turn.kind).collect();
    assert_eq!(
        kinds,
        [
            crate::TurnKind::Main,
            crate::TurnKind::View,
            crate::TurnKind::View
        ]
    );
    // Every view turn has its own allowance.
    assert!(report.per_turn);
    assert!(
        report
            .turns
            .iter()
            .all(|turn| turn.fuel < varta_common::abi::FUEL)
    );
}

#[test]
fn gesture_turns_come_from_the_task_and_gate_copy_text() {
    // Only the trusted click's turn is a gesture turn (copy granted, status
    // 0); the main turn's copy is `invalid_context` (3).
    let mut app = instance(COPY, vec![clipboard(true)], None);
    app.view_input(&input(r##"{"input":"click","target":"#copy"}"##))
        .unwrap();
    app.run();
    assert_eq!(
        app.take_output(),
        ["varta log: main copy 3", "varta log: click copy 0"]
    );
    assert_eq!(app.view_snapshot()["clipboard"], "copied text");
    // The broker audited the grant and the denial-free copy.
    let (audit, _) = app.audit();
    assert!(audit.iter().any(|entry| {
        entry.operation == view::CLIPBOARD_WRITE_CAPABILITY
            && entry.outcome == varta_common::permission::PermissionState::Granted
    }));
}

#[derive(Clone, Default)]
struct Recorder(std::sync::Arc<std::sync::Mutex<Vec<varta_common::permission::PermissionPrompt>>>);
impl crate::PermissionPrompter for Recorder {
    fn prompt(
        &mut self,
        prompt: &varta_common::permission::PermissionPrompt,
    ) -> varta_common::permission::PromptAnswer {
        self.0.lock().unwrap().push(prompt.clone());
        varta_common::permission::PromptAnswer::Allow
    }
}

#[test]
fn open_url_decisions_go_through_broker_v1() {
    let click = r##"{"input":"click","target":"#docs"}"##;
    // Explicit patterns of the request (no ceiling): granted by policy and
    // audited with the URL; a revocation denies later activations.
    let mut app = instance(LINK, vec![links(&["https://docs.test/**"])], None);
    app.view_input(&input(click)).unwrap();
    app.run();
    assert!(app.revoke(view::OPEN_URL_CAPABILITY));
    app.view_input(&input(click)).unwrap();
    app.run();
    assert_eq!(
        app.take_output(),
        ["varta log: open_url 2 0", "varta log: open_url 2 1"]
    );
    let (audit, _) = app.audit();
    let outcomes: Vec<_> = audit
        .iter()
        .filter(|entry| {
            entry.operation == view::OPEN_URL_CAPABILITY
                && entry.resource == "https://docs.test/guide"
        })
        .map(|entry| entry.outcome)
        .collect();
    use varta_common::permission::PermissionState as S;
    assert_eq!(outcomes, [S::Granted, S::Denied]);
    // Matched only by the any-HTTPS form: every activation prompts through
    // the prompt seam with the full URL, and no per-URL decision is stored.
    let recorder = Recorder::default();
    let mut app = instance(
        LINK,
        vec![links(&[view::ANY_HTTPS_PATTERN])],
        Some(Box::new(recorder.clone())),
    );
    for _ in 0..2 {
        app.view_input(&input(click)).unwrap();
        app.run();
    }
    assert_eq!(
        app.take_output(),
        ["varta log: open_url 2 0", "varta log: open_url 2 0"]
    );
    let prompts = recorder.0.lock().unwrap();
    let activations: Vec<_> = prompts
        .iter()
        .filter(|prompt| prompt.scope == ["https://docs.test/guide"])
        .collect();
    assert_eq!(activations.len(), 2);
    assert!(
        activations
            .iter()
            .all(|prompt| !prompt.required && prompt.capability == view::OPEN_URL_CAPABILITY)
    );
    assert!(app.decisions().keys().all(|id| !id.contains("docs.test")));
    // A headless host without a prompt seam never prompts: not_allowed.
    let mut app = instance(LINK, vec![links(&[view::ANY_HTTPS_PATTERN])], None);
    app.view_input(&input(click)).unwrap();
    app.run();
    assert_eq!(app.take_output(), ["varta log: open_url 2 1"]);
}

#[test]
fn bounded_runs_render_without_event_delivery() {
    // A bounded execution commits its view; records have no embedder to
    // deliver them and never reach the guest.
    let output = VartaRuntime::new()
        .run_wasm_bytes(&compile(COPY), &[clipboard(true)])
        .unwrap();
    assert!(output.lines.is_empty(), "{:?}", output.lines);
}
