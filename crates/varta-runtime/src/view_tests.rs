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
mod native_clipboard;
mod network;
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
    fn resolve_text(
        &self,
        importer: &std::path::Path,
        specifier: &str,
    ) -> Result<std::path::PathBuf, varta_compiler::CompilerError> {
        self.resolve(importer, specifier)
    }
}

pub(crate) const APP: &str = include_str!("view_cases/app.tsx");
pub(crate) const SIGNALS: &str = include_str!("view_cases/signals.ts");

pub(crate) fn compile_tsx(
    entry: &str,
    modules: &'static [(&'static str, &'static str)],
) -> Vec<u8> {
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

/// A scripted session's budget report is deterministic too: N runs give the
/// same output, tree, commits, turns and idle maintenance with their fuel
/// (only wall-clock times differ), on virtual time with arrival-ordered
/// tasks.
#[test]
fn scripted_budget_reports_are_deterministic_across_runs() {
    let bytes = compile_tsx(APP, &[("signals.ts", SIGNALS)]);
    let inputs = r##"{"inputs":[
        {"input":"click","target":"#inc"},
        {"input":"type","target":"#box","text":"hello"},
        {"input":"key","target":"#box","key":"Enter"},
        {"input":"wait","ms":5},
        {"input":"click","target":"#inc"}
    ]}"##;
    let grants = [clipboard(true)];
    let session = || {
        let run =
            run_view(&bytes, &grants, None, &script(inputs)).unwrap_or_else(|e| panic!("{e}"));
        let mut budget = run.report.budget.clone().unwrap();
        for turn in &mut budget.turns {
            turn.wall = std::time::Duration::ZERO;
        }
        for period in &mut budget.maintenance {
            period.wall = std::time::Duration::ZERO;
        }
        (run.output.lines, run.snapshot, run.commits, budget)
    };
    let first = session();
    for _ in 0..3 {
        assert_eq!(session(), first);
    }
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
fn select_work_exhaustion_is_fatal_and_keeps_the_previous_committed_view() {
    let source = format!(
        r#"
export function main():void {{
  const select=View.node();View.createElement(select,{select});
  const value='X'.repeat(4096);
  for(let i=0;i<20;i++){{
    const option=View.node();View.createElement(option,{option});
    View.setAttribute(option,{value_attr},value);View.insert(select,option,0);
  }}
  const button=View.node();View.createElement(button,{button});View.setAttribute(button,{id_attr},'trigger');
  View.on(button,{click},(event:ViewEvent):void=>{{
    View.setTitle('must roll back');
    for(let i=0;i<128;i++)View.setValue(select,`missing${{i}}`);
    try{{View.commit();console.log('AFTER');}}catch(error){{console.log('CAUGHT');}}
  }},0,0,0,0);
  View.insert({root},select,0);View.insert({root},button,0);View.setTitle('before');View.commit();
}}
"#,
        select = view::Tag::Select.code(),
        option = view::Tag::Option.code(),
        value_attr = view::Attribute::Value.code(),
        button = view::Tag::Button.code(),
        id_attr = view::Attribute::Id.code(),
        click = view::EventKind::Click.code(),
        root = view::ROOT_NODE,
    );
    let bytes = compile(&source);
    let mut result = VartaRuntime::new().run_view(
        &bytes,
        crate::ApplicationOptions::default(),
        &script(r##"{"inputs":[{"input":"click","target":"#trigger"}]}"##),
    );
    assert_eq!(
        invalid(result.report.failure.take().expect("fatal work limit")),
        varta_view::Violation::SelectWorkLimit
    );
    assert!(
        result.report.output.lines.is_empty(),
        "{:?}",
        result.report.output.lines
    );
    assert_eq!(result.commits, 1);
    let snapshot = result.snapshot.expect("retained committed view");
    assert_eq!(snapshot["title"], "before");
    assert_eq!(snapshot["tree"]["children"][0]["value"], "X".repeat(4096));
}

#[test]
fn radio_work_exhaustion_is_fatal_and_retains_the_committed_group() {
    let source = format!(
        r#"
export function main():void {{
  const name='Ж'.repeat(512);
  let first=0;
  for(let i=0;i<100;i++){{
    const radio=View.node();if(i===0)first=radio;
    View.createElement(radio,{input});View.setAttribute(radio,{type_attr},'radio');
    View.setAttribute(radio,{name_attr},name);View.insert({root},radio,0);
  }}
  View.setChecked(first,true);
  const button=View.node();View.createElement(button,{button});View.setAttribute(button,{id_attr},'trigger');
  View.on(button,{click},(event:ViewEvent):void=>{{
    View.setTitle('must roll back');
    for(let i=0;i<128;i++)View.setChecked(first,true);
    try{{View.commit();console.log('AFTER');}}catch(error){{console.log('CAUGHT');}}
  }},0,0,0,0);
  View.insert({root},button,0);View.setTitle('before');View.commit();
}}
"#,
        input = view::Tag::Input.code(),
        type_attr = view::Attribute::Type.code(),
        name_attr = view::Attribute::Name.code(),
        button = view::Tag::Button.code(),
        id_attr = view::Attribute::Id.code(),
        click = view::EventKind::Click.code(),
        root = view::ROOT_NODE,
    );
    let bytes = compile(&source);
    let mut result = VartaRuntime::new().run_view(
        &bytes,
        crate::ApplicationOptions::default(),
        &script(r##"{"inputs":[{"input":"click","target":"#trigger"}]}"##),
    );
    assert_eq!(
        invalid(
            result
                .report
                .failure
                .take()
                .expect("fatal radio work limit")
        ),
        varta_view::Violation::RadioWorkLimit
    );
    assert!(
        result.report.output.lines.is_empty(),
        "{:?}",
        result.report.output.lines
    );
    assert_eq!(result.commits, 1);
    let snapshot = result.snapshot.expect("retained committed view");
    assert_eq!(snapshot["title"], "before");
    assert_eq!(snapshot["tree"]["children"][0]["checked"], true);
    assert_eq!(snapshot["tree"]["children"][1]["checked"], false);
}

#[test]
fn retained_view_text_exhaustion_is_fatal_and_keeps_the_prior_payload() {
    let fields = view::DRAFT_MAX_RETAINED_TEXT_BYTES / view::DRAFT_FIELD_VALUE_RESERVATION_BYTES;
    let filler = view::DRAFT_MAX_RETAINED_TEXT_BYTES
        - fields * view::DRAFT_FIELD_VALUE_RESERVATION_BYTES
        - "before".len()
        - "trigger".len();
    let source = format!(
        r#"
export function main():void {{
  for(let i=0;i<{fields};i++){{ const field=View.node();View.createElement(field,{input});View.insert({root},field,0); }}
  const text=View.node();View.createText(text,'X'.repeat({filler}));View.insert({root},text,0);
  const button=View.node();View.createElement(button,{button});View.setAttribute(button,{id_attr},'trigger');
  View.on(button,{click},(event:ViewEvent):void=>{{
    View.setTitle('before!');
    try{{View.commit();console.log('AFTER');}}catch(error){{console.log('CAUGHT');}}
  }},0,0,0,0);
  View.insert({root},button,0);View.setTitle('before');View.commit();
}}
"#,
        input = view::Tag::Input.code(),
        root = view::ROOT_NODE,
        button = view::Tag::Button.code(),
        id_attr = view::Attribute::Id.code(),
        click = view::EventKind::Click.code()
    );
    let bytes = compile(&source);
    let mut result = VartaRuntime::new().run_view(
        &bytes,
        crate::ApplicationOptions::default(),
        &script(r##"{"inputs":[{"input":"click","target":"#trigger"}]}"##),
    );
    assert_eq!(
        invalid(
            result
                .report
                .failure
                .take()
                .expect("fatal retained text limit")
        ),
        varta_view::Violation::RetainedTextLimit
    );
    assert!(
        result.report.output.lines.is_empty(),
        "{:?}",
        result.report.output.lines
    );
    assert_eq!(result.commits, 1);
    assert_eq!(result.snapshot.expect("retained view")["title"], "before");
}

#[test]
fn stylesheet_expansion_work_is_fatal_before_effects_and_keeps_the_previous_view() {
    let source = format!(
        r#"
export function main():void {{
  const button=View.node();View.createElement(button,{button});View.setAttribute(button,{id_attr},'trigger');
  View.on(button,{click},(event:ViewEvent):void=>{{
    const count=event.seq===0?1100:1101;
    const conditions='(min-width:0px) and '.repeat(499)+'(min-width:0px)';
    View.stylesheet(1,'@media '+conditions+'{{'+'.a{{}}'.repeat(count)+'}}');
    View.setTitle('after');
    try{{View.commit();console.log('AFTER');}}catch(error){{console.log('CAUGHT');}}
  }},0,0,0,0);
  View.insert({root},button,0);View.setTitle('before');View.commit();
}}
"#,
        root = view::ROOT_NODE,
        button = view::Tag::Button.code(),
        id_attr = view::Attribute::Id.code(),
        click = view::EventKind::Click.code()
    );
    let bytes = compile(&source);
    let mut result = VartaRuntime::new().run_view(
        &bytes,
        crate::ApplicationOptions::default(),
        &script(r##"{"inputs":[{"input":"click","target":"#trigger"}]}"##),
    );
    assert_eq!(
        invalid(
            result
                .report
                .failure
                .take()
                .expect("fatal stylesheet work limit")
        ),
        varta_view::Violation::StylesheetWorkLimit
    );
    assert!(
        result.report.output.lines.is_empty(),
        "{:?}",
        result.report.output.lines
    );
    assert_eq!(result.commits, 1);
    let snapshot = result.snapshot.expect("prior view");
    assert_eq!(snapshot["title"], "before");
    assert!(snapshot["stylesheets"].as_array().unwrap().is_empty());
}

#[test]
fn expanded_component_budget_is_fatal_before_later_repair_and_guest_catch() {
    let source = format!(
        r#"
export function main():void {{
 const button=View.node();View.createElement(button,{button});View.setAttribute(button,{id},'trigger');View.setClass(button,'a');View.insert(1,button,0);
 for(let i=0;i<62;i++){{const node=View.node();View.createElement(node,{button});View.setClass(node,'a');View.insert(1,node,0);}}
 const seed='"'+'a'.repeat({length})+'"';View.stylesheet(1,':root{{--seed:'+seed+'}}.a{{--copy:var(--seed)/}}');
 View.on(button,{click},(event:ViewEvent):void=>{{
  View.command(button,{focus},0);View.command(button,{focus},0);View.command(button,{focus},0);
  View.stylesheet(1,'');View.setTitle('after');
  try{{View.commit();console.log('AFTER');}}catch(error){{console.log('CAUGHT');}}
 }},0,0,0,0);
 View.setTitle('before');View.commit();
}}
"#,
        button = view::Tag::Button.code(),
        id = view::Attribute::Id.code(),
        click = view::EventKind::Click.code(),
        focus = view::Command::Focus.code(),
        length = view::DRAFT_MAX_VALUE_BYTES / 2 - 3
    );
    let bytes = compile(&source);
    let mut result = VartaRuntime::new().run_view(
        &bytes,
        crate::ApplicationOptions::default(),
        &script(r##"{"inputs":[{"input":"click","target":"#trigger"}]}"##),
    );
    assert_eq!(
        invalid(
            result
                .report
                .failure
                .take()
                .expect("fatal expanded component quota")
        ),
        varta_view::Violation::ExpandedValueLimit
    );
    assert!(
        result.report.output.lines.is_empty(),
        "{:?}",
        result.report.output.lines
    );
    assert_eq!(result.commits, 1);
    let snapshot = result.snapshot.expect("prior view");
    assert_eq!(snapshot["title"], "before");
    assert_eq!(snapshot["tree"]["children"].as_array().unwrap().len(), 63);
    assert_eq!(snapshot["stylesheets"].as_array().unwrap().len(), 1);
}

#[test]
fn accessible_name_budget_is_fatal_before_publication_and_guest_catch() {
    let before = vec!["field"; 80].join(" ");
    let excessive = vec!["field"; 86].join(" ");
    let source = format!(
        r#"
export function main():void {{
 const button=View.node();View.createElement(button,{button});View.setAttribute(button,{id},'trigger');
 View.setAttribute(button,{refs},'{before}');View.insert(1,button,0);
 const field=View.node();View.createElement(field,{input});View.setAttribute(field,{id},'field');View.setAttribute(field,{kind},'password');View.insert(1,field,0);
 View.on(button,{click},(event:ViewEvent):void=>{{
  View.setAttribute(button,{refs},'{excessive}');View.setTitle('after');
  try{{View.commit();console.log('AFTER');}}catch(error){{console.log('CAUGHT');}}
 }},0,0,0,0);
 View.setTitle('before');View.commit();
}}
"#,
        button = view::Tag::Button.code(),
        input = view::Tag::Input.code(),
        id = view::Attribute::Id.code(),
        refs = view::Attribute::AriaLabelledby.code(),
        kind = view::Attribute::Type.code(),
        click = view::EventKind::Click.code()
    );
    let mut result = VartaRuntime::new().run_view(
        &compile(&source),
        crate::ApplicationOptions::default(),
        &script(r##"{"inputs":[{"input":"click","target":"#trigger"}]}"##),
    );
    assert_eq!(
        invalid(result.report.failure.take().expect("fatal naming quota")),
        varta_view::Violation::NamingWorkLimit
    );
    assert!(result.report.output.lines.is_empty());
    assert_eq!(result.commits, 1);
    let snapshot = result.snapshot.unwrap();
    assert_eq!(snapshot["title"], "before");
    assert_eq!(
        snapshot["tree"]["children"][0]["attributes"]["aria-labelledby"],
        before
    );
}

#[test]
fn snapshot_export_limits_do_not_terminate_the_application_or_prevent_a_later_turn() {
    let bytes = compile_tsx(include_str!("view_cases/snapshot-export.tsx"), &[]);
    let mut app = VartaRuntime::new()
        .start_application(
            &bytes,
            crate::ApplicationOptions {
                run: crate::RunOptions {
                    requested: vec![timers()],
                    ..crate::RunOptions::default()
                },
                ..crate::ApplicationOptions::default()
            },
        )
        .unwrap();
    for _ in 0..200 {
        if app.view_commits() >= 110 {
            break;
        }
        match app.step(Some(std::time::Duration::from_millis(20))) {
            crate::Step::Turn(_) | crate::Step::Idle => {}
            other => panic!("snapshot fixture stopped: {other:?}; {:?}", app.failure()),
        }
    }
    assert_eq!(app.view_commits(), 110, "{:?}", app.failure());
    let state = app.state();
    assert_eq!(state, crate::InstanceState::Running);
    assert!(matches!(
        app.view_snapshot_json(crate::ViewSnapshotFormat::Pretty),
        Err(crate::ViewSnapshotError::Limit { .. })
    ));
    assert_eq!(app.state(), state);
    assert!(app.failure().is_none());
    assert!(
        app.view_snapshot_json(crate::ViewSnapshotFormat::Compact)
            .unwrap()
            .len()
            < 1024 * 1024
    );
    app.view_input(&varta_view::Input::Click {
        target: varta_view::Target::Ident("#clear".into()),
    })
    .unwrap();
    assert!(matches!(
        app.step(Some(std::time::Duration::ZERO)),
        crate::Step::Turn(_)
    ));
    let bytes = app
        .view_snapshot_json(crate::ViewSnapshotFormat::Pretty)
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["title"],
        "recovered"
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

#[test]
fn native_layout_root_font_applies_before_main_and_survives_resize() {
    let source = format!(
        r##"
export function main(): void {{
  View.stylesheet(1, '.pane {{ width:4rem; height:2rem; overflow:auto }} .tall {{ height:6rem; background-color:#0f0 }}');
  const pane=<div class="pane"><div class="tall" /></div>;
  View.insert({},pane,0);
  View.command(pane,{},1000);
  View.commit();
}}
"##,
        view::ROOT_NODE,
        view::Command::ScrollTo.code()
    );
    let bytes = compile_tsx(&source, &[]);
    let mut app = VartaRuntime::new()
        .start_application(
            &bytes,
            crate::ApplicationOptions {
                view_layout: Some(
                    crate::ViewLayout::new(100.0, 100.0, None, 16.0)
                        .unwrap()
                        .with_root_font_size(20.0)
                        .unwrap(),
                ),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(app.failure().is_none(), "{:?}", app.failure());
    assert_eq!(
        app.view_snapshot()["tree"]["children"][0]["scrollTop"],
        80.0
    );
    let ink = |image: &varta_view::RasterImage, x: usize| {
        image.pixels[(10 * image.width as usize + x) * 4..(10 * image.width as usize + x + 1) * 4]
            .to_vec()
    };
    assert_eq!(
        ink(&app.view_rasterize(100, 100, None).unwrap(), 70),
        [0, 255, 0, 255]
    );
    app.view_set_layout_size(200.0, 100.0).unwrap();
    assert_eq!(
        ink(&app.view_rasterize(200, 100, None).unwrap(), 70),
        [0, 255, 0, 255]
    );
    app.view_set_environment(
        varta_view::Environment {
            width: 200.0,
            ..Default::default()
        }
        .with_root_font_size(24.0)
        .unwrap(),
    )
    .unwrap();
    let image = app.view_rasterize_scaled(400, 200, 2.0, None).unwrap();
    assert_eq!(ink(&image, 180), [0, 255, 0, 255]);
    assert_eq!(ink(&image, 195), [0, 0, 0, 0]);
    assert!(matches!(
        crate::ViewLayout::new(100.0, 100.0, None, 16.0)
            .unwrap()
            .with_root_font_size(f64::NAN),
        Err(varta_view::LayoutError::RootFont)
    ));
}

#[test]
fn native_layout_applies_initial_theme_preserves_it_on_resize_and_updates_media() {
    let bytes = compile(
        r##"
export function main(): void {
  View.stylesheet(1, '.box { width:20px; height:20px; background-color:#f00 } @media (prefers-color-scheme: dark) { .box { background-color:#0f0 } }');
  const root = View.node(); View.createElement(root, 1); View.setClass(root, 'box'); View.mount(root);
}
"##,
    );
    let mut app = VartaRuntime::new()
        .start_application(
            &bytes,
            crate::ApplicationOptions {
                view_layout: Some(
                    crate::ViewLayout::new(100.0, 100.0, None, 16.0)
                        .unwrap()
                        .with_dark_mode(true),
                ),
                ..crate::ApplicationOptions::default()
            },
        )
        .unwrap();
    assert!(app.failure().is_none(), "{:?}", app.failure());
    assert_eq!(
        &app.view_rasterize(100, 100, None).unwrap().pixels[..4],
        &[0, 255, 0, 255]
    );
    app.view_set_layout_size(200.0, 100.0).unwrap();
    assert_eq!(
        &app.view_rasterize(200, 100, None).unwrap().pixels[..4],
        &[0, 255, 0, 255]
    );
    app.view_set_environment(varta_view::Environment {
        width: 200.0,
        dark: false,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        &app.view_rasterize(200, 100, None).unwrap().pixels[..4],
        &[255, 0, 0, 255]
    );
}

#[test]
fn native_password_and_search_frames_deliver_original_guest_values() {
    let source = r#"
export function main(): void {
  View.stylesheet(1, '.field { display:block; width:80px; height:30px; min-width:0; min-height:0; padding:4px; border:2px solid #000; border-radius:0; outline:none; background-color:#fff; color:#00f; font-family:"Varta Password Fixture"; font-size:20px; font-weight:normal; font-style:normal; line-height:20px }');
  const password = View.node();
  View.createElement(password, 24);
  View.setAttribute(password, 1, 'key');
  View.setAttribute(password, 2, 'password');
  View.setClass(password, 'field');
  View.setValue(password, 'AФé😀B');
  View.on(password, 2, (event: ViewEvent): void => console.log(`key ${event.value}`), 0, 0, 0, 0);
  const search = View.node();
  View.createElement(search, 24);
  View.setAttribute(search, 1, 'search');
  View.setAttribute(search, 2, 'search');
  View.setClass(search, 'field');
  View.setValue(search, 'AAAA');
  View.on(search, 2, (event: ViewEvent): void => console.log(`search ${event.value}`), 0, 0, 0, 0);
  View.insert(1, password, 0);
  View.insert(1, search, 0);
  View.commit();
}
"#;
    let bytes = compile(source);
    let mut app = VartaRuntime::new()
        .start_application(&bytes, crate::ApplicationOptions::default())
        .unwrap();
    assert!(app.failure().is_none(), "{:?}", app.failure());
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("../../varta-view/tests/fixtures/password.ttf").to_vec(),
        "Varta Password Fixture",
    )
    .unwrap();
    let frame = app
        .view_rasterize_scaled(200, 150, 2.0, Some((&mut font, 20.0)))
        .unwrap();
    let password = frame.hit_test(16.0, 30.0).unwrap();
    let search = frame.hit_test(16.0, 90.0).unwrap();
    assert_ne!(password, search);
    assert_eq!(frame.text_position_at(password, 40.0, 30.0), Some(3));
    assert_eq!(frame.text_position_at(password, 72.0, 30.0), Some(10));
    for (target, anchor, focus, text) in [(password, 3, 10, "X"), (search, 1, 3, "Ф")] {
        app.view_input(&varta_view::Input::TextSelect {
            target: varta_view::Target::Node(target),
            anchor,
            focus,
            activate: false,
        })
        .unwrap();
        app.view_input(&varta_view::Input::Type {
            target: varta_view::Target::Node(target),
            text: text.into(),
        })
        .unwrap();
        assert_eq!(app.run(), crate::Step::Idle);
    }
    assert_eq!(
        app.take_output(),
        ["varta log: key AФXB", "varta log: search AФA"]
    );
    let snapshot = app.view_snapshot();
    assert_eq!(snapshot["tree"]["children"][0]["value"], "AФXB");
    assert_eq!(snapshot["tree"]["children"][1]["value"], "AФA");
    app.view_rasterize_scaled(200, 150, 2.0, Some((&mut font, 20.0)))
        .unwrap();
}

#[test]
fn native_checkable_frames_deliver_checked_events_and_isolate_radio_forms() {
    let source = r#"
function choice(kind: string, name: string): number {
  const field = View.node(); View.createElement(field, 24);
  View.setAttribute(field, 2, kind); View.setAttribute(field, 3, name);
  View.setClass(field, 'switch'); return field;
}
export function main(): void {
  View.stylesheet(1, '.switch { display:block; width:28px; height:28px; min-width:0; min-height:0; padding:4px; border:2px solid #000; border-radius:0; outline:none; background-color:#fff; color:#00f }');
  const check = choice('checkbox','');
  View.on(check, 1, (event: ViewEvent): void => console.log(`click ${event.checked()} ${event.seq}`), 0, 0, 0, 0);
  View.on(check, 2, (event: ViewEvent): void => console.log(`input ${event.checked()} ${event.seq} [${event.value}]`), 0, 0, 0, 0);
  View.on(check, 3, (event: ViewEvent): void => console.log(`change ${event.checked()} ${event.seq}`), 0, 0, 0, 0);
  const a = choice('radio','g'); const b = choice('radio','g');
  View.insert(1,check,0); View.insert(1,a,0); View.insert(1,b,0);
  const form = View.node(); View.createElement(form,22); View.insert(1,form,0);
  const other = choice('radio','g'); View.insert(form,other,0);
  View.setChecked(a,true); View.setChecked(other,true); View.commit();
}
"#;
    let bytes = compile(source);
    let mut app = VartaRuntime::new()
        .start_application(&bytes, crate::ApplicationOptions::default())
        .unwrap();
    assert!(app.failure().is_none(), "{:?}", app.failure());
    let frame = app.view_rasterize_scaled(160, 240, 2.0, None).unwrap();
    let check = frame.hit_test(28.0, 28.0).unwrap();
    let a = frame.hit_test(28.0, 84.0).unwrap();
    let b = frame.hit_test(28.0, 140.0).unwrap();
    app.view_input(&varta_view::Input::Click {
        target: varta_view::Target::Node(check),
    })
    .unwrap();
    assert_eq!(app.run(), crate::Step::Idle);
    assert_eq!(
        app.take_output(),
        [
            "varta log: click true 0",
            "varta log: input true 1 []",
            "varta log: change true 1"
        ]
    );
    app.view_input(&varta_view::Input::Key {
        target: varta_view::Target::Node(a),
        key: "ArrowRight".into(),
        shift: false,
        ctrl: false,
        alt: false,
        meta: false,
        composing: false,
    })
    .unwrap();
    assert_eq!(app.run(), crate::Step::Idle);
    let snapshot = app.view_snapshot();
    assert_eq!(snapshot["focus"], b);
    assert_eq!(snapshot["tree"]["children"][1]["checked"], false);
    assert_eq!(snapshot["tree"]["children"][2]["checked"], true);
    assert_eq!(
        snapshot["tree"]["children"][3]["children"][0]["checked"],
        true
    );
    app.view_rasterize_scaled(160, 240, 2.0, None).unwrap();
}

#[test]
fn native_select_popup_choice_and_equal_guest_echo_preserve_option_identity() {
    let source = r#"
export function main():void {
  View.stylesheet(1,'.choice { display:block; width:80px; height:30px; min-width:0; min-height:0; padding:4px; border:2px solid #000; border-radius:0; outline:none; background-color:#fff; color:#00f; font-family:"Varta Fixture 400"; font-size:20px; font-weight:normal; font-style:normal; line-height:20px }');
  const select=View.node();View.createElement(select,26);View.setClass(select,'choice');
  for(let i=0;i<3;i++){
    const option=View.node();View.createElement(option,27);View.setAttribute(option,4,i===0?'blocked':'same');
    if(i===0)View.setAttribute(option,6,'');
    const label=View.node();View.createText(label,'A'.repeat(i+1));View.insert(option,label,0);View.insert(select,option,0);
  }
  View.on(select,2,(event:ViewEvent):void=>{console.log(`input ${event.value} ${event.seq}`);View.setValue(select,event.value);},0,0,0,0);
  View.on(select,3,(event:ViewEvent):void=>console.log(`change ${event.value} ${event.seq}`),0,0,0,0);
  View.mount(select);
}
"#;
    let bytes = compile(source);
    let mut app = VartaRuntime::new()
        .start_application(&bytes, crate::ApplicationOptions::default())
        .unwrap();
    assert!(app.failure().is_none(), "{:?}", app.failure());
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("../../varta-view/tests/fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    let closed = app
        .view_rasterize_scaled(200, 400, 2.0, Some((&mut font, 20.0)))
        .unwrap();
    let select = closed.hit_test(20.0, 30.0).unwrap();
    app.view_input(&varta_view::Input::Click {
        target: varta_view::Target::Node(select),
    })
    .unwrap();
    let open = app
        .view_rasterize_scaled(200, 400, 2.0, Some((&mut font, 20.0)))
        .unwrap();
    let choice = open.select_hit(20.0, 200.0).unwrap();
    let varta_view::SelectHit::Choice { option, .. } = choice else {
        panic!("enabled third option")
    };
    app.view_input(&varta_view::Input::SelectOption {
        target: varta_view::Target::Node(select),
        option,
    })
    .unwrap();
    assert_eq!(app.run(), crate::Step::Idle);
    assert_eq!(
        app.take_output(),
        ["varta log: input same 1", "varta log: change same 1"]
    );
    let snapshot = app.view_snapshot();
    let field = &snapshot["tree"]["children"][0];
    assert_eq!(field["children"][2]["selected"], true);
    assert_eq!(field["children"][1]["selected"], false);
    // Non-text native controls may receive a logical character without any
    // committed text. Typeahead still moves to the next enabled matching label.
    app.view_input(&varta_view::Input::TextKey {
        target: varta_view::Target::Node(select),
        key: "a".into(),
        text: String::new(),
        shift: false,
        ctrl: false,
        alt: false,
        meta: false,
        select_all: false,
        history: None,
    })
    .unwrap();
    assert_eq!(app.run(), crate::Step::Idle);
    assert_eq!(
        app.take_output(),
        ["varta log: input same 2", "varta log: change same 2"]
    );
    let typed = app.view_snapshot();
    assert_eq!(
        typed["tree"]["children"][0]["children"][1]["selected"],
        true
    );
    assert_eq!(
        typed["tree"]["children"][0]["children"][2]["selected"],
        false
    );
    assert!(
        app.view_rasterize_scaled(200, 400, 2.0, Some((&mut font, 20.0)))
            .unwrap()
            .select_menu()
            .is_none()
    );
}

#[test]
fn tsx_select_values_follow_options_without_duplicate_reads_or_lost_dependencies() {
    let source = r#"
import {TextSignal,effect} from './signals.ts';
export function main():void {
  View.setEffect(effect);const current=new TextSignal('b');let reads=0;
  function read():string {reads++;console.log(`read ${reads}`);return current.get();}
  function child():string {console.log('child');return 'A';}
  const dynamic=<select id="dynamic" value={read()} ref={(node:number):void=>console.log('ref')}>
    <option value="a">{child()}</option><option value="b">B</option>
  </select>;
  const fixed=<select id="fixed" value="b"><option value="a">A</option><option value="b">B</option></select>;
  const button=<button id="change" onClick={():void=>current.set('a')}>Next</button>;
  View.insert(1,dynamic,0);View.insert(1,fixed,0);View.insert(1,button,0);View.commit();
}
"#;
    let bytes = compile_tsx(source, &[("signals.ts", SIGNALS)]);
    let mut app = VartaRuntime::new()
        .start_application(&bytes, crate::ApplicationOptions::default())
        .unwrap();
    assert!(app.failure().is_none(), "{:?}", app.failure());
    assert_eq!(
        app.take_output(),
        ["varta log: read 1", "varta log: child", "varta log: ref"]
    );
    let snapshot = app.view_snapshot();
    assert_eq!(by_id(&snapshot["tree"], "dynamic").unwrap()["value"], "b");
    assert_eq!(by_id(&snapshot["tree"], "fixed").unwrap()["value"], "b");
    app.view_input(&varta_view::Input::Click {
        target: varta_view::Target::Ident("#change".into()),
    })
    .unwrap();
    assert_eq!(app.run(), crate::Step::Idle);
    assert_eq!(app.take_output(), ["varta log: read 2"]);
    assert_eq!(
        by_id(&app.view_snapshot()["tree"], "dynamic").unwrap()["value"],
        "a"
    );
}

#[test]
fn native_layout_executes_guest_scroll_from_first_turn_and_after_resize() {
    let source = r#"
export function main(): void {
  View.stylesheet(1, '.spacer { height: 120px } @media (max-width: 300px) { .spacer { height: 160px } }');
  const root = View.node();
  View.createElement(root, 1);
  View.setStyle(root, 2, '100px');
  View.setStyle(root, 3, '50px');
  View.setStyle(root, 39, 'auto');
  View.on(root, 8, (event: ViewEvent): void => console.log(`scroll ${event.scrollTop}`), 0, 0, 0, 0);
  const spacer = View.node();
  View.createElement(spacer, 1);
  View.setClass(spacer, 'spacer');
  const target = View.node();
  View.createElement(target, 1);
  View.setStyle(target, 3, '10px');
  const label = View.node();
  View.createText(label, 'A');
  View.insert(target, label, 0);
  View.insert(root, spacer, 0);
  View.insert(root, target, 0);
  View.on(root, 1, (event: ViewEvent): void => {
    View.command(root, 5, 0);
    View.command(target, 4, 0);
  }, 0, 0, 0, 0);
  View.insert(1, root, 0);
  View.command(target, 4, 0);
  View.commit();
}
"#;
    let bytes = compile(source);
    let font = varta_view::TextMeasurer::new(
        include_bytes!("../../varta-view/tests/fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    let mut app = VartaRuntime::new()
        .start_application(
            &bytes,
            crate::ApplicationOptions {
                view_layout: Some(crate::ViewLayout::new(200.0, 100.0, Some(font), 16.0).unwrap()),
                ..crate::ApplicationOptions::default()
            },
        )
        .unwrap();
    assert!(app.failure().is_none(), "{:?}", app.failure());
    assert_eq!(
        app.view_snapshot()["tree"]["children"][0]["scrollTop"],
        120.0
    );
    assert_eq!(app.run(), crate::Step::Idle);
    assert_eq!(app.take_output(), ["varta log: scroll 120"]);
    app.view_set_layout_size(400.0, 100.0).unwrap();
    let root = app.view_snapshot()["tree"]["children"][0]["id"]
        .as_u64()
        .unwrap() as u32;
    app.view_input(&varta_view::Input::Click {
        target: varta_view::Target::Node(root),
    })
    .unwrap();
    assert_eq!(app.run(), crate::Step::Idle);
    assert_eq!(app.take_output(), ["varta log: scroll 80"]);
    assert_eq!(
        app.view_snapshot()["tree"]["children"][0]["scrollTop"],
        80.0
    );
}

#[test]
fn live_application_host_scroll_dispatches_guest_turn() {
    let source = r#"
export function main(): void {
  const root = View.node();
  View.createElement(root, 1);
  View.setStyle(root, 2, '100px');
  View.setStyle(root, 3, '50px');
  View.setStyle(root, 39, 'auto');
  View.on(root, 8, (event: ViewEvent): void => console.log(`scroll ${event.scrollTop} ${event.scrollHeight} ${event.clientHeight}`), 0, 0, 0, 0);
  const spacer = View.node();
  View.createElement(spacer, 1);
  View.setStyle(spacer, 3, '120px');
  const target = View.node();
  View.createElement(target, 1);
  View.setStyle(target, 3, '10px');
  View.insert(root, spacer, 0);
  View.insert(root, target, 0);
  View.mount(root);
}
"#;
    let mut app = instance(source, vec![], None);
    let snapshot = app.view_snapshot();
    let target = snapshot["tree"]["children"][0]["children"][1]["id"]
        .as_u64()
        .unwrap() as u32;
    assert_eq!(
        app.view_scroll_into_view(target, 200.0, 100.0, None)
            .unwrap(),
        1
    );
    assert!(app.take_output().is_empty());
    assert_eq!(app.run(), crate::Step::Idle);
    assert_eq!(app.take_output(), vec!["varta log: scroll 80 130 50"]);
    assert_eq!(
        app.view_scroll_into_view(target, 200.0, 100.0, None)
            .unwrap(),
        0
    );
    assert_eq!(app.run(), crate::Step::Idle);
    assert!(app.take_output().is_empty());
    let root = snapshot["tree"]["children"][0]["id"].as_u64().unwrap() as u32;
    let frame = app.view_rasterize_scaled(400, 200, 2.0, None).unwrap();
    let metrics = frame.scroll_metrics(root).unwrap();
    app.view_input(&varta_view::Input::ScrollMeasured {
        target: varta_view::Target::Node(root),
        top: 0.0,
        scroll_height: f64::from(metrics.viewport.height) + f64::from(metrics.max),
        client_height: f64::from(metrics.viewport.height),
    })
    .unwrap();
    assert_eq!(app.run(), crate::Step::Idle);
    assert_eq!(app.take_output(), ["varta log: scroll 0 130 50"]);
}

#[test]
fn live_application_exposes_cpu_frames_between_view_turns() {
    let source = r##"
export function main(): void {
  View.setTitle('Before click');
  const root = View.node();
  View.createElement(root, 1);
  View.setAttribute(root, 1, 'frame');
  View.setStyle(root, 2, '20px');
  View.setStyle(root, 3, '20px');
  View.setStyle(root, 45, '#f00');
  View.setStyle(root, 54, '20px');
  View.setStyle(root, 57, '20px');
  const text = View.node();
  View.createText(text, 'A');
  View.insert(root, text, 0);
  View.on(root, 1, (event: ViewEvent): void => { View.setStyle(root, 45, '#00f'); View.setTitle('After click'); }, 0, 0, 0, 0);
  View.mount(root);
}
"##;
    let mut app = instance(source, vec![], None);
    assert_eq!(app.view_title(), "Before click");
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("../../varta-view/tests/fixtures/advance-400.ttf").to_vec(),
        "Varta Fixture 400",
    )
    .unwrap();
    let frame = app.view_rasterize(40, 40, Some((&mut font, 20.0))).unwrap();
    let mut renderer = varta_view::Renderer::new(
        Some(
            varta_view::TextMeasurer::new(
                include_bytes!("../../varta-view/tests/fixtures/advance-400.ttf").to_vec(),
                "Varta Fixture 400",
            )
            .unwrap(),
        ),
        20.0,
    )
    .unwrap();
    assert_eq!(app.view_render(&mut renderer, 40, 40, 1.0).unwrap(), frame);
    assert_eq!(app.view_render(&mut renderer, 40, 40, 1.0).unwrap(), frame);
    assert_eq!(renderer.stats().dirty_pixels, 0);
    let scaled = app
        .view_rasterize_scaled(80, 80, 2.0, Some((&mut font, 20.0)))
        .unwrap();
    assert_eq!(scaled.scale(), 2.0);
    assert_eq!(
        &scaled.pixels[(36 * 80 + 36) * 4..(36 * 80 + 37) * 4],
        &[255, 0, 0, 255]
    );
    let target = frame
        .hit_test(18.0, 18.0)
        .expect("visible root is clickable");
    assert_eq!(scaled.hit_test(36.0, 36.0), Some(target));
    assert_eq!(
        &frame.pixels[(18 * 40 + 18) * 4..(18 * 40 + 19) * 4],
        &[255, 0, 0, 255]
    );
    assert!(app.view_rasterize(40, 40, None).is_err());
    app.view_input(&varta_view::Input::Click {
        target: varta_view::Target::Node(target),
    })
    .unwrap();
    assert_eq!(app.run(), crate::Step::Idle);
    assert_eq!(app.view_title(), "After click");
    let frame = app.view_rasterize(40, 40, Some((&mut font, 20.0))).unwrap();
    assert_eq!(app.view_render(&mut renderer, 40, 40, 1.0).unwrap(), frame);
    assert!(renderer.stats().dirty_pixels > 0);
    assert_eq!(
        app.view_render(&mut renderer, 80, 80, 2.0).unwrap(),
        app.view_rasterize_scaled(80, 80, 2.0, Some((&mut font, 20.0)))
            .unwrap()
    );
    assert_eq!(
        &frame.pixels[(18 * 40 + 18) * 4..(18 * 40 + 19) * 4],
        &[0, 0, 255, 255]
    );
}

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
fn assistive_frame_scroll_events_run_later_with_fresh_budgets() {
    use varta_common::view::{CssProperty as P, EventKind, Tag};
    use varta_view::{
        AccessibilityAction as A, AccessibilityScrollAction as S, AccessibilityScrollUnit as U,
    };
    let source = format!(
        r#"
export function main(): void {{
 const pane=View.node();View.createElement(pane,{div});View.setAttribute(pane,{id},'pane');
 View.setStyle(pane,{width},'100px');View.setStyle(pane,{height},'50px');View.setStyle(pane,{overflow},'auto');
 View.on(pane,{scroll},(event:ViewEvent):void => {{console.log('scrolled');}},0,0,0,0);
 const child=View.node();View.createElement(child,{div});View.setStyle(child,{height},'200px');
 View.insert(pane,child,0);View.insert(1,pane,0);View.commit();
}}
"#,
        div = Tag::Div.code(),
        id = varta_common::view::Attribute::Id.code(),
        width = P::Width.code(),
        height = P::Height.code(),
        overflow = P::Overflow.code(),
        scroll = EventKind::Scroll.code()
    );
    let mut app = instance(&source, vec![], None);
    let semantics = app.view_accessibility_tree().unwrap();
    let pane = semantics
        .nodes
        .values()
        .find(|node| node.author_id.as_deref() == Some("pane"))
        .unwrap();
    let frame = app.view_rasterize(150, 100, None).unwrap().into_metadata();
    let turns = app.budget_report().turns.len();
    let action = A::Scroll(S::Down(U::Page));
    assert!(
        !app.view_accessibility_input(&pane.identity, &action)
            .unwrap()
    );
    assert!(
        app.view_accessibility_input_from_frame(&pane.identity, &action, &frame)
            .unwrap()
    );
    assert!(app.take_output().is_empty());
    assert_eq!(app.budget_report().turns.len(), turns);
    assert_eq!(app.run(), crate::Step::Idle);
    assert_eq!(app.take_output(), ["varta log: scrolled"]);
    assert_eq!(app.budget_report().turns.len(), turns + 1);
    assert!(app.failure().is_none());
}

#[test]
fn assistive_inputs_deliver_guest_events_on_later_budgeted_turns() {
    use varta_view::AccessibilityAction as Action;
    let mut app = instance(FIELD, vec![], None);
    let semantics = app.view_accessibility_tree().unwrap();
    let field = &semantics
        .nodes
        .values()
        .find(|node| node.author_id.as_deref() == Some("name"))
        .unwrap()
        .identity;
    let button = &semantics
        .nodes
        .values()
        .find(|node| node.author_id.as_deref() == Some("go"))
        .unwrap()
        .identity;
    let turns = app.budget_report().turns.len();
    assert!(
        app.view_accessibility_input(field, &Action::SetValue("assistive".into()))
            .unwrap()
    );
    assert!(
        app.view_accessibility_input(button, &Action::Click)
            .unwrap()
    );
    assert!(app.take_output().is_empty());
    assert_eq!(app.budget_report().turns.len(), turns);
    assert!(
        !app.view_accessibility_input(button, &Action::SetValue("invalid".into()))
            .unwrap()
    );
    assert_eq!(app.run(), crate::Step::Idle);
    assert_eq!(
        app.take_output(),
        ["varta log: input assistive", "varta log: click"]
    );
    let report = app.budget_report();
    assert_eq!(report.turns.len(), turns + 2);
    assert!(report.per_turn);
    assert!(
        report
            .turns
            .iter()
            .all(|turn| turn.fuel < varta_common::abi::FUEL)
    );
}

#[test]
fn assistive_text_selection_and_replacement_keep_unicode_offsets_and_event_turns() {
    use varta_view::AccessibilityAction as Action;
    let mut app = instance(FIELD, vec![], None);
    let tree = app.view_accessibility_tree().unwrap();
    let identity = &tree
        .nodes
        .values()
        .find(|node| node.author_id.as_deref() == Some("name"))
        .unwrap()
        .identity;
    assert!(
        app.view_accessibility_input(identity, &Action::SetValue("A😀e\u{301}\r\nБ".into()))
            .unwrap()
    );
    assert_eq!(app.run(), crate::Step::Idle);
    app.take_output();
    let tree = app.view_accessibility_tree().unwrap();
    let node = tree
        .nodes
        .values()
        .find(|node| node.author_id.as_deref() == Some("name"))
        .unwrap();
    let model = node.text.as_ref().unwrap();
    let selection = Action::SelectText {
        revision: model.revision.clone(),
        anchor: 8,
        focus: 1,
    };
    let turns = app.budget_report().turns.len();
    assert!(
        app.view_accessibility_input(&node.identity, &selection)
            .unwrap()
    );
    assert!(app.take_output().is_empty());
    assert_eq!(app.run(), crate::Step::Idle);
    assert_eq!(app.budget_report().turns.len(), turns);
    assert!(
        app.view_accessibility_input(&node.identity, &Action::ReplaceSelectedText("X".into()))
            .unwrap()
    );
    assert!(app.take_output().is_empty());
    assert_eq!(app.budget_report().turns.len(), turns);
    assert_eq!(app.run(), crate::Step::Idle);
    assert_eq!(app.take_output(), ["varta log: input AX\r\nБ"]);
    assert_eq!(app.budget_report().turns.len(), turns + 1);
    let before = app.view_snapshot();
    assert!(
        !app.view_accessibility_input(&node.identity, &selection)
            .unwrap()
    );
    assert_eq!(app.view_snapshot(), before);
    assert!(app.failure().is_none());
}

#[test]
fn assistive_observer_refusal_keeps_the_instance_running_and_recovers_after_input() {
    use varta_common::view::{Attribute as A, Tag};
    let source = format!(
        r#"
export function main(): void {{
 const button = View.node(); View.createElement(button, {button});
 View.setAttribute(button, {id}, 'go');
 View.setAttribute(button, {describedby}, 'help '.repeat(254) + 'help');
 View.on(button, {click}, (event: ViewEvent): void => {{
   View.setAttribute(button, {describedby}, 'help'); View.setTitle('recovered'); View.commit();
 }}, 0, 0, 0, 0);
 const help = View.node(); View.createElement(help, {div});
 View.setAttribute(help, {id}, 'help'); View.setAttribute(help, {hidden}, 'true');
 const text = View.node(); View.createText(text, 'é'.repeat({units}));
 View.insert(help, text, 0); View.insert(1, button, 0); View.insert(1, help, 0); View.commit();
}}
"#,
        button = Tag::Button.code(),
        div = Tag::Div.code(),
        id = A::Id.code(),
        describedby = A::AriaDescribedby.code(),
        hidden = A::AriaHidden.code(),
        click = view::EventKind::Click.code(),
        units = view::DRAFT_MAX_VALUE_BYTES / 2
    );
    let mut app = instance(&source, vec![], None);
    assert!(app.failure().is_none(), "{:?}", app.failure());
    let before = app.view_snapshot();
    assert!(matches!(
        app.view_accessibility_tree(),
        Err(varta_view::AccessibilityError::WorkLimit { .. })
    ));
    assert_eq!(app.view_snapshot(), before);
    assert_eq!(app.state(), crate::InstanceState::Running);
    assert!(app.failure().is_none());
    app.view_input(&varta_view::Input::Click {
        target: varta_view::Target::Ident("#go".into()),
    })
    .unwrap();
    assert_eq!(app.run(), crate::Step::Idle);
    assert_eq!(app.view_title(), "recovered");
    let semantics = app.view_accessibility_tree().unwrap();
    let button = semantics
        .nodes
        .values()
        .find(|node| node.author_id.as_deref() == Some("go"))
        .unwrap();
    assert_eq!(
        button.description.as_ref().unwrap().len(),
        view::DRAFT_MAX_VALUE_BYTES
    );
    assert_eq!(app.state(), crate::InstanceState::Running);
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

fn timers() -> CapabilityGrant {
    CapabilityGrant {
        id: varta_common::scheduler::TIMER_CAPABILITY.into(),
        value: CapabilityValue::Boolean(true),
    }
}

/// Nodes appear in timer turns: a zero-delay timer, its zero-delay chain,
/// a 50 ms timer and a 10 s timer no script waits for; every click arms a
/// zero-delay timer of its own.
const TIMED: &str = r#"
export function main(): void {
  const root = View.node();
  View.createElement(root, 1);
  View.mount(root);
  const add = (id: string): void => {
    const node = View.node();
    View.createElement(node, 1);
    View.setAttribute(node, 1, id);
    View.on(node, 1, (event: ViewEvent): void => {
      console.log(`click ${id}`);
      setTimeout(() => console.log(`after ${id}`), 0);
    }, 0, 0, 0, 0);
    View.insert(root, node, 0);
    View.commit();
  };
  setTimeout(() => {
    add('late');
    setTimeout(() => {
      add('chained');
      setTimeout(() => add('third'), 0);
    }, 0);
  }, 0);
  setTimeout(() => add('delayed'), 50);
  setTimeout(() => console.log('never'), 10000);
  console.log('main');
}
"#;

fn turn_kinds(run: &ViewRun) -> Vec<crate::TurnKind> {
    run.report
        .budget
        .as_ref()
        .map(|report| report.turns.iter().map(|turn| turn.kind).collect())
        .unwrap_or_default()
}

#[test]
fn scripted_inputs_wait_for_zero_delay_timers_and_their_chains() {
    let started = std::time::Instant::now();
    let run = run(
        TIMED,
        r##"{"inputs":[
            {"input":"click","target":"#late"},
            {"input":"click","target":"#third"},
            {"input":"click","target":"#chained"}
        ]}"##,
        &[timers()],
    );
    // A click's zero-delay timer fires before the next input.
    assert_eq!(
        run.output.lines,
        [
            "varta log: main",
            "varta log: click late",
            "varta log: after late",
            "varta log: click third",
            "varta log: after third",
            "varta log: click chained",
            "varta log: after chained",
        ]
    );
    let ids: Vec<_> = run.snapshot["tree"]["children"][0]["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| node["attributes"]["id"].clone())
        .collect();
    assert_eq!(ids, ["late", "chained", "third"]);
    // main, three chained timers, then per click a view turn and a timer.
    use crate::TurnKind::{Main, Timer, View};
    assert_eq!(
        turn_kinds(&run),
        [
            Main, Timer, Timer, Timer, View, Timer, View, Timer, View, Timer
        ]
    );
    // Virtual time: the 50 ms and 10 s timers never fire and nothing waited
    // for the wall clock.
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

pub(crate) const TIMERS: &str = include_str!("view_cases/timers.ts");
pub(crate) const TIMERS_SCRIPT: &str = r##"{"inputs":[
    {"input":"type","target":"#box","text":"a"},
    {"input":"key","target":"#box","key":"Enter"},
    {"input":"click","target":"#later"},
    {"input":"wait","ms":30},
    {"input":"type","target":"#box","text":"b"}
]}"##;
/// Runs of the determinism cases; at least 20.
const DETERMINISM_RUNS: usize = 25;

/// A scripted session is a function of the artifact and the script: the
/// same turns, output and final tree on every run, however fast the host
/// is (cb6795d decided the input/timer boundary by the wall clock).
#[test]
fn scripted_sessions_are_deterministic_across_runs() {
    let bytes = compile(TIMERS);
    let grants = [timers()];
    let first = run_view(&bytes, &grants, None, &script(TIMERS_SCRIPT)).unwrap();
    use crate::TurnKind::{Main, Timer, View};
    assert_eq!(
        turn_kinds(&first),
        [
            Main, Timer, Timer, Timer, // the m chain
            View, Timer, Timer, Timer, // type a: the i chain
            // Enter: keydown and the newline's input, then the k chain, the
            // button and the i chain interleaved in timer order.
            View, View, Timer, Timer, Timer, Timer, Timer, Timer, Timer, Timer, Timer, View, Timer,
            Timer, Timer, Timer, // click: the c chain
            Timer, // wait 30: delayed
            View, Timer, Timer, Timer, // type b
        ]
    );
    assert_eq!(
        first.output.lines,
        [
            "varta log: input a",
            "varta log: key Enter",
            "varta log: input a\n",
            "varta log: click later",
            "varta log: input a\nb"
        ]
    );
    let log = text_of(by_id(&first.snapshot["tree"], "log").unwrap());
    assert_eq!(
        log,
        "1:m3;2:m2;3:m1;4:i3;5:i2;6:i1;7:k5;8:i3;9:k4;10:i2;11:k3;12:i1;13:k2;14:k1;\
         15:c4;16:c3;17:c2;18:c1;19:delayed;20:i3;21:i2;22:i1;"
    );
    for _ in 1..DETERMINISM_RUNS {
        let again = run_view(&bytes, &grants, None, &script(TIMERS_SCRIPT)).unwrap();
        assert_eq!(turn_kinds(&first), turn_kinds(&again));
        assert_eq!(first.output.lines, again.output.lines);
        assert_eq!(first.snapshot, again.snapshot);
    }
}

/// Zero-delay timers keep Node's 1 ms minimum on the virtual clock: a chain
/// interleaves with a 2 ms timer in Node's order (Node 24 prints
/// `c0 a c1 c2 b`), all before the input; the 50 ms timer needs a wait.
#[test]
fn zero_delay_chains_keep_node_order_with_delayed_timers() {
    let source = r#"
export function main(): void {
  const root = View.node();
  View.createElement(root, 1);
  View.mount(root);
  setTimeout(() => console.log('a'), 2);
  setTimeout(() => {
    console.log('c0');
    setTimeout(() => {
      console.log('c1');
      setTimeout(() => console.log('c2'), 0);
    }, 0);
  }, 0);
  setTimeout(() => console.log('b'), 50);
}
"#;
    let run = run(
        source,
        r#"{"inputs":[{"input":"prompts","answer":"allow"}]}"#,
        &[timers()],
    );
    assert_eq!(
        run.output.lines,
        [
            "varta log: c0",
            "varta log: a",
            "varta log: c1",
            "varta log: c2"
        ]
    );
}

#[test]
fn wait_steps_advance_virtual_time_to_fire_delayed_timers() {
    let bytes = compile(TIMED);
    let grants = [timers()];
    // 49 ms is not enough for the 50 ms timer.
    let early = run_view(
        &bytes,
        &grants,
        None,
        &script(r##"{"inputs":[{"input":"wait","ms":49},{"input":"click","target":"#delayed"}]}"##),
    )
    .err()
    .unwrap();
    assert!(early.to_string().contains("attached"), "{early}");
    // Two waits add up; the timer fires in its own turn before the input.
    let session = r##"{"inputs":[
        {"input":"wait","ms":20},
        {"input":"wait","ms":30},
        {"input":"click","target":"#delayed"},
        {"input":"wait","ms":0}
    ]}"##;
    let first = run_view(&bytes, &grants, None, &script(session)).unwrap();
    assert_eq!(
        first.output.lines,
        [
            "varta log: main",
            "varta log: click delayed",
            "varta log: after delayed"
        ]
    );
    // Deterministic across runs: the same output, snapshot and turns.
    for _ in 0..3 {
        let again = run_view(&bytes, &grants, None, &script(session)).unwrap();
        assert_eq!(first.output.lines, again.output.lines);
        assert_eq!(first.snapshot, again.snapshot);
        assert_eq!(turn_kinds(&first), turn_kinds(&again));
    }
}

#[test]
fn wait_steps_are_bounded_and_endless_zero_delay_timers_fail_the_step() {
    for invalid in [
        r#"{"inputs":[{"input":"wait","ms":-1}]}"#,
        r#"{"inputs":[{"input":"wait","ms":2147483648}]}"#,
        r#"{"inputs":[{"input":"wait"}]}"#,
        r##"{"inputs":[{"input":"wait","ms":1,"target":"#x"}]}"##,
    ] {
        assert!(
            crate::ViewScript::parse(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
    let source = r#"
export function main(): void {
  const root = View.node();
  View.createElement(root, 1);
  View.mount(root);
  setInterval(() => {}, 0);
}
"#;
    let error = run_view(
        &compile(source),
        &[timers()],
        None,
        &script(r#"{"inputs":[{"input":"wait","ms":0},{"input":"prompts","answer":"allow"}]}"#),
    )
    .err()
    .unwrap();
    // The wait reaches nothing (the interval is due at 1 ms); the input's
    // settling never ends.
    assert!(
        matches!(
            error,
            crate::RuntimeError::Application(crate::ApplicationError::Unsettled { step: 2 })
        ),
        "{error}"
    );
}

const STYLED: &str = r#"import css from './console.css' with { type: 'text' };
export function main(): void {
  View.stylesheet(1, css);
  console.log(`${css.length} ${css.startsWith(':root')}`);
  View.mount(<main id="app" class="card">hi</main>);
}"#;
const CONSOLE_CSS: &str =
    ":root { --accent: #0a84ff }\n.card { color: var(--accent); padding: 8px }\n";

#[test]
fn text_imports_reach_the_view_verbatim() {
    let sheet = CONSOLE_CSS;
    let bytes = compile_tsx(STYLED, &[("console.css", CONSOLE_CSS)]);
    let run = run_view(&bytes, &[], None, &script(NO_INPUTS)).unwrap();
    assert_eq!(
        run.output.lines,
        [format!("varta log: {} true", sheet.encode_utf16().count())]
    );
    assert_eq!(run.snapshot["stylesheets"][0]["text"], sheet);
    let app = by_id(&run.snapshot["tree"], "app").unwrap();
    assert_eq!(app["style"]["color"], "#0a84ff");
}

/// TSX holes are getter closures: a parameter narrowed by a `switch` keeps
/// its narrowing inside them (v38 `closure_narrowing`).
const NARROWED_HOLES: &str = r#"type Inline = { kind: 'text'; text: string } | { kind: 'code'; text: string; lang: string };
function render(n: Inline): number {
  switch (n.kind) {
    case 'code':
      return <code class={n.lang}>{n.text}</code>;
    default:
      return <span>{n.text}</span>;
  }
}
function Item(props: { n: Inline }): number {
  return render(props.n);
}
export function main(): void {
  const code: Inline = { kind: 'code', text: 'let x', lang: 'ts' };
  const plain: Inline = { kind: 'text', text: 'plain' };
  View.mount(<main id="app"><Item n={code} /><Item n={plain} /></main>);
}"#;

#[test]
fn narrowing_reaches_tsx_holes() {
    let bytes = compile_tsx(NARROWED_HOLES, &[]);
    let run = run_view(&bytes, &[], None, &script(NO_INPUTS)).unwrap();
    let app = by_id(&run.snapshot["tree"], "app").unwrap();
    let children = app["children"].as_array().unwrap();
    assert_eq!(children[0]["tag"], "code");
    assert_eq!(children[0]["class"], "ts");
    assert_eq!(text_of(&children[0]), "let x");
    assert_eq!(children[1]["tag"], "span");
    assert_eq!(text_of(&children[1]), "plain");
}

#[test]
fn tree_depth_exhaustion_is_fatal_before_effects_and_retains_the_prior_view() {
    let source = format!(
        r#"
export function main():void {{
  const button=View.node();View.createElement(button,{button});View.setAttribute(button,{id_attr},'trigger');
  View.on(button,{click},(event:ViewEvent):void=>{{
    let parent={root};const count={depth}+(event.seq===0?1:2);
    for(let i=0;i<count;i++){{const node=View.node();View.createElement(node,{span});View.insert(parent,node,0);parent=node;}}
    View.setTitle('after');
    try{{View.commit();console.log('AFTER');}}catch(error){{console.log('CAUGHT');}}
  }},0,0,0,0);
  View.insert({root},button,0);View.setTitle('before');View.commit();
}}
"#,
        root = view::ROOT_NODE,
        button = view::Tag::Button.code(),
        span = view::Tag::Span.code(),
        id_attr = view::Attribute::Id.code(),
        click = view::EventKind::Click.code(),
        depth = view::DRAFT_MAX_TREE_DEPTH
    );
    let bytes = compile(&source);
    let mut result = VartaRuntime::new().run_view(
        &bytes,
        crate::ApplicationOptions::default(),
        &script(r##"{"inputs":[{"input":"click","target":"#trigger"}]}"##),
    );
    assert_eq!(
        invalid(result.report.failure.take().expect("fatal tree depth")),
        varta_view::Violation::TreeDepthLimit
    );
    assert!(
        result.report.output.lines.is_empty(),
        "{:?}",
        result.report.output.lines
    );
    assert_eq!(result.commits, 1);
    let snapshot = result.snapshot.expect("retained view");
    assert_eq!(snapshot["title"], "before");
    assert_eq!(snapshot["tree"]["children"].as_array().unwrap().len(), 1);
}

#[test]
fn cascade_read_exhaustion_is_fatal_before_transient_state_can_be_repaired() {
    let source = format!(
        r#"
export function main():void {{
  const button=View.node();View.createElement(button,{button});View.setAttribute(button,{id_attr},'trigger');
  View.on(button,{click},(event:ViewEvent):void=>{{
    const count=event.seq===0?1338:1339;View.stylesheet(1,'.a{{}}'.repeat(count));
    for(let i=0;i<128;i++){{const node=View.node();View.createElement(node,{span});View.setClass(node,'a');View.insert({root},node,0);}}
    View.command(button,{focus},0);View.stylesheet(1,'');View.setTitle('after');
    try{{View.commit();console.log('AFTER');}}catch(error){{console.log('CAUGHT');}}
  }},0,0,0,0);
  View.insert({root},button,0);View.setTitle('before');View.commit();
}}
"#,
        root = view::ROOT_NODE,
        button = view::Tag::Button.code(),
        span = view::Tag::Span.code(),
        id_attr = view::Attribute::Id.code(),
        click = view::EventKind::Click.code(),
        focus = view::Command::Focus.code()
    );
    let bytes = compile(&source);
    let mut result = VartaRuntime::new().run_view(
        &bytes,
        crate::ApplicationOptions::default(),
        &script(r##"{"inputs":[{"input":"click","target":"#trigger"}]}"##),
    );
    assert_eq!(
        invalid(result.report.failure.take().expect("fatal cascade work")),
        varta_view::Violation::CascadeWorkLimit
    );
    assert!(
        result.report.output.lines.is_empty(),
        "{:?}",
        result.report.output.lines
    );
    assert_eq!(result.commits, 1);
    let snapshot = result.snapshot.expect("retained view");
    assert_eq!(snapshot["title"], "before");
    assert_eq!(snapshot["tree"]["children"].as_array().unwrap().len(), 1);
}

#[test]
fn compiled_variable_scopes_resolve_aliases_cycles_and_numeric_fallbacks_across_turns() {
    let mut empty = ":root{--empty0:var(--missing,);".to_owned();
    for i in 1..32 {
        empty.push_str(&format!(
            "--empty{i}:var(--empty{})var(--empty{});",
            i - 1,
            i - 1
        ));
    }
    empty.push('}');
    let initial = format!(
        "{empty}:root{{--base:#123;--alias:var(--base);--alpha:.5}}.target{{--base:#abc;color:VAR(--alias);padding:var(--gap,1px 2px);margin:var(--empty31,7px) 3px;opacity:var(--alpha,2)}}"
    );
    let changed = format!(
        "{empty}:root{{--base:#456;--alias:var(--base);--alpha:.5;--a:var(--b);--b:var(--a)}}.target{{--base:#abc;color:VAR(--alias);background-color:var(--a,#789);padding:var(--gap,1px 2px);margin:var(--empty31,7px) 3px;opacity:var(--alpha,2)}}"
    );
    let source = format!(
        r#"
export function main():void {{
 const button=View.node();View.createElement(button,{button});View.setAttribute(button,{id_attr},'trigger');View.setClass(button,'target');
 View.stylesheet(1,{initial:?});
 View.on(button,{click},(event:ViewEvent):void=>{{View.stylesheet(1,{changed:?});View.setTitle('after');View.commit();}},0,0,0,0);
 View.insert({root},button,0);View.setTitle('before');View.commit();
}}
"#,
        button = view::Tag::Button.code(),
        id_attr = view::Attribute::Id.code(),
        click = view::EventKind::Click.code(),
        root = view::ROOT_NODE
    );
    let bytes = compile(&source);
    let result = VartaRuntime::new().run_view(
        &bytes,
        crate::ApplicationOptions::default(),
        &script(r##"{"inputs":[{"input":"click","target":"#trigger"}]}"##),
    );
    assert!(
        result.report.failure.is_none(),
        "{:?}",
        result.report.failure
    );
    assert_eq!(result.commits, 2);
    let snapshot = result.snapshot.expect("variable frame");
    let style = &snapshot["tree"]["children"][0]["style"];
    assert_eq!(snapshot["title"], "after");
    assert_eq!(style["color"], "#456");
    assert_eq!(style["background-color"], "#789");
    assert_eq!(style["padding"], "1px 2px");
    assert_eq!(style["opacity"], ".5");
    assert_eq!(style["margin"], "3px");
}

#[test]
fn native_pointer_styles_do_not_run_guest_turns_or_activate_controls() {
    let source = r##"
export function main():void {
 View.stylesheet(1,'.card {width:40px; height:40px; background-color:#f00} .card:hover {background-color:#00f} .card:active {background-color:#0f0}');
 const root=View.node(); View.createElement(root,1); View.setClass(root,'card');
 View.on(root,1,(event:ViewEvent):void=>{console.log('activated');},0,0,0,0);
 View.mount(root);
}
"##;
    let mut app = instance(source, vec![], None);
    let commits = app.view_commits();
    let mut renderer = varta_view::Renderer::new(None, 16.0).unwrap();
    let before = app.view_render(&mut renderer, 80, 80, 1.0).unwrap();
    let target = before.hit_test(5.0, 5.0).unwrap();
    assert!(app.view_set_pointer_state(Some(target), None));
    assert!(!app.view_set_pointer_state(Some(target), None));
    let hovered = app.view_render(&mut renderer, 80, 80, 1.0).unwrap();
    assert_eq!(
        &hovered.pixels[(5 * 80 + 5) * 4..(5 * 80 + 6) * 4],
        [0, 0, 255, 255]
    );
    assert!(before.same_input_geometry(&hovered));
    app.view_set_pointer_state(Some(target), Some(target));
    let pressed = app.view_render(&mut renderer, 80, 80, 1.0).unwrap();
    assert_eq!(
        &pressed.pixels[(5 * 80 + 5) * 4..(5 * 80 + 6) * 4],
        [0, 255, 0, 255]
    );
    assert_eq!(app.view_commits(), commits);
    assert!(app.take_output().is_empty());
    app.view_input(&varta_view::Input::Click {
        target: varta_view::Target::Node(target),
    })
    .unwrap();
    assert_eq!(app.run(), crate::Step::Idle);
    assert_eq!(app.take_output(), ["varta log: activated"]);
}

#[test]
fn assistive_text_reveal_changes_only_display_and_schedules_no_guest_turn() {
    use varta_view::{
        AccessibilityAction as A, AccessibilityScrollAction as S, AccessibilityScrollHint as H,
    };
    let source = format!(
        r#"
export function main():void {{
 View.stylesheet(1,'.field {{display:block;width:60px;height:40px;min-width:0;min-height:0;padding:4px;border:1px solid #000;outline:none;border-radius:0;background-color:#fff;color:#000;font-family:"Varta Password Fixture";font-size:20px;font-weight:400;font-style:normal;line-height:20px}}');
 const field=View.node();View.createElement(field,{input});View.setClass(field,'field');
 View.setValue(field,'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA');
 View.on(field,{event},(event:ViewEvent):void=>{{console.log('input');}},0,0,0,0);
 View.insert(1,field,0);View.commit();
}}
"#,
        input = view::Tag::Input.code(),
        event = view::EventKind::Input.code()
    );
    let mut app = instance(&source, vec![], None);
    let semantics = app.view_accessibility_tree().unwrap();
    let field = semantics
        .nodes
        .values()
        .find(|node| node.text.is_some())
        .unwrap();
    let text = field.text.as_ref().unwrap();
    let mut font = varta_view::TextMeasurer::new(
        include_bytes!("../../varta-view/tests/fixtures/password.ttf").to_vec(),
        "Varta Password Fixture",
    )
    .unwrap();
    let frame = app
        .view_rasterize_scaled(400, 200, 2.0, Some((&mut font, 20.0)))
        .unwrap()
        .into_metadata();
    let before = frame
        .field_text_geometry(&field.identity, &text.revision)
        .unwrap()
        .clusters[0]
        .bounds
        .x;
    let turns = app.budget_report().turns.len();
    let action = A::Scroll(S::TextIntoView {
        revision: text.revision.clone(),
        start: 0,
        end: 1,
        hint: H::Nearest,
    });
    assert!(
        !app.view_accessibility_input(&field.identity, &action)
            .unwrap()
    );
    assert!(
        app.view_accessibility_input_from_frame(&field.identity, &action, &frame)
            .unwrap()
    );
    assert_eq!(app.run(), crate::Step::Idle);
    assert!(app.take_output().is_empty());
    assert_eq!(app.budget_report().turns.len(), turns);
    let after = app.view_accessibility_tree().unwrap();
    assert_eq!(after.focus, semantics.focus);
    assert_eq!(after.nodes[&field.id].value, field.value);
    assert_eq!(
        after.nodes[&field.id].text.as_ref().unwrap().selection,
        text.selection
    );
    let fresh = app
        .view_rasterize_scaled(400, 200, 2.0, Some((&mut font, 20.0)))
        .unwrap();
    let g = fresh
        .field_text_geometry(&field.identity, &text.revision)
        .unwrap();
    assert!(g.clusters[0].bounds.x > before);
    assert!(g.clusters[0].bounds.x >= g.clip.x);
    assert!(app.failure().is_none());
}
