//! K8 T1: build-time Tailwind stylesheets. The project's `tailwind.entry`
//! is compiled from the program's class tokens and served as the text of the
//! entry's text resource, which the program applies with `View.stylesheet`.
//! End-to-end application of full Tailwind output waits for the T2 engine
//! grammar; those checks assert the precise first T2 violation today.
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

const CONFIG: &str = r#"{"entry":"src/main.tsx","target":"native","out":"dist/app","languageProfile":"checked-f64-utf16-v38","capabilities":{},"tailwind":{"entry":"src/app.css"}}"#;

struct Project {
    workspace: PathBuf,
    root: PathBuf,
}
impl Project {
    fn new(main: &str, css: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let workspace = std::env::temp_dir().join(format!(
            "varta-tailwind-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let root = workspace.join("app");
        fs::create_dir_all(root.join("src")).unwrap();
        let project = Self { workspace, root };
        project.write("varta.config.json", CONFIG);
        project.write("src/main.tsx", main);
        project.write("src/app.css", css);
        project
    }
    fn write(&self, path: &str, source: &str) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, source).unwrap();
    }
    fn command(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_varta"))
            .current_dir(&self.root)
            .args(args)
            .output()
            .unwrap()
    }
    fn diagnostic(&self) -> serde_json::Value {
        let output = self.command(&["check", "--json"]);
        assert_eq!(output.status.code(), Some(1), "{}", text(&output));
        let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        json["diagnostics"][0].clone()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.workspace);
    }
}
fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
fn success(output: Output) -> String {
    assert!(output.status.success(), "{}", text(&output));
    text(&output)
}

/// A program whose classes the L0 grammar admits today: `flex` and
/// `hidden` (bare identifiers) with `@tailwind utilities` (no layers).
const UTILITIES_MAIN: &str = r#"import css from "./app.css" with { type: "text" };
const tone: "flex" | "hidden" = "flex";
export function main(): void {
  View.stylesheet(1, css);
  console.log(JSON.stringify(css));
  View.mount(<main className={tone}>Tailwind</main>);
}"#;

#[test]
fn the_generated_stylesheet_is_the_text_resource_and_reproducible() {
    let project = Project::new(UTILITIES_MAIN, "@tailwind utilities;\n");
    let expected = r#"varta log: ".flex {\n  display: flex;\n}\n.hidden {\n  display: none;\n}\n""#;
    assert!(
        success(project.command(&["dev"])).contains(expected),
        "{}",
        text(&project.command(&["dev"]))
    );
    success(project.command(&["build", "--single"]));
    let first = fs::read(project.root.join("dist/app")).unwrap();
    let wasm = fs::read(project.root.join("dist/app.wasm")).unwrap();
    success(project.command(&["build", "--single"]));
    assert_eq!(first, fs::read(project.root.join("dist/app")).unwrap());
    assert_eq!(wasm, fs::read(project.root.join("dist/app.wasm")).unwrap());
    // The artifact carries the generated stylesheet: no source or Tailwind
    // input is read when it runs.
    fs::remove_dir_all(project.root.join("src")).unwrap();
    let native = Command::new(project.root.join("dist/app"))
        .current_dir(&project.root)
        .output()
        .unwrap();
    assert!(text(&native).contains(expected), "{}", text(&native));
}

#[test]
fn javascript_configuration_and_unused_entries_are_rejected() {
    let project = Project::new(UTILITIES_MAIN, "@plugin \"@tailwindcss/forms\";\n");
    let output = project.command(&["check"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(
        text(&output).contains("loads JavaScript and is not supported"),
        "{}",
        text(&output)
    );

    let unused = Project::new(
        "export function main(): void { View.mount(<p class=\"flex\">x</p>); }",
        "@tailwind utilities;\n",
    );
    let output = unused.command(&["check"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(
        text(&output).contains("the Tailwind entry is not imported by the program"),
        "{}",
        text(&output)
    );
    assert!(!unused.root.join("dist").exists());
}

#[test]
fn composed_class_names_are_located_errors() {
    let project = Project::new(
        r#"import css from "./app.css" with { type: "text" };
export function main(): void {
  View.stylesheet(1, css);
  let size = "lg";
  View.mount(<p class={`text-${size}`}>x</p>);
}"#,
        "@tailwind utilities;\n",
    );
    let diagnostic = project.diagnostic();
    assert_eq!(diagnostic["code"], "type_error", "{diagnostic}");
    assert!(
        diagnostic["file"].as_str().unwrap().ends_with("main.tsx"),
        "{diagnostic}"
    );
    assert_eq!(diagnostic["range"]["start"]["line"], 5, "{diagnostic}");
    assert!(
        diagnostic["message"]
            .as_str()
            .unwrap()
            .contains("composed at run time"),
        "{diagnostic}"
    );
}

/// The full `@import "tailwindcss"` stylesheet (layers, `@property`,
/// theme variables, Preflight, escaped class selectors) is generated and
/// stored, but today's L0 grammar rejects it. Its first violation is
/// Preflight's `[hidden]:where(:not([hidden="until-found"])) { display: none
/// !important; }`: `!important` belongs to T2 stage S8. This asserts that
/// precise violation (it moves as T2 stages land); afterwards
/// `full_tailwind_application_applies_after_t2` takes over.
#[test]
fn full_tailwind_output_waits_for_t2_sheet_structure() {
    let project = Project::new(
        r#"import css from "./app.css" with { type: "text" };
export function main(): void {
  View.stylesheet(1, css);
  const card = "p-4";
  View.mount(<p className={card}>x</p>);
}"#,
        "@import \"tailwindcss\";\n",
    );
    let diagnostic = project.diagnostic();
    assert_eq!(diagnostic["code"], "type_error", "{diagnostic}");
    assert!(
        diagnostic["file"].as_str().unwrap().ends_with("app.css"),
        "{diagnostic}"
    );
    assert_eq!(
        diagnostic["message"], "invalid stylesheet for View.stylesheet: forbidden CSS token",
        "{diagnostic}"
    );
    assert_eq!(
        diagnostic["snippet"]["text"], "    display: none !important;",
        "{diagnostic}"
    );
    assert_eq!(diagnostic["range"]["start"]["column"], 19, "{diagnostic}");
}

/// Static Tailwind class tokens such as `hover:bg-red-500` or `w-1/2` need
/// T2 stage S1 (class tokens and escapes); today the L0 class grammar
/// rejects them where the TSX attribute is checked.
#[test]
fn tailwind_class_tokens_wait_for_t2_class_grammar() {
    let project = Project::new(
        r#"import css from "./app.css" with { type: "text" };
export function main(): void {
  View.stylesheet(1, css);
  View.mount(<p className="w-1/2">x</p>);
}"#,
        "@tailwind utilities;\n",
    );
    let diagnostic = project.diagnostic();
    assert_eq!(
        diagnostic["message"], "invalid value `w-1/2` for attribute `className`",
        "{diagnostic}"
    );
}

/// The end-to-end goal of K8: ordinary TSX with Tailwind classes, no
/// hand-written CSS. Needs T2 stages S1 (class tokens), S2 (`@property`),
/// S3 (`calc`, `oklch`, `color-mix`), S5 (Preflight's selectors) and S8
/// (`!important`) at least; run with `--ignored` once they land.
#[test]
#[ignore = "waits for T2 stages S1, S2, S3, S5 and S8 of the L0 stylesheet grammar"]
fn full_tailwind_application_applies_after_t2() {
    let project = Project::new(
        r#"import css from "./app.css" with { type: "text" };
export function main(): void {
  View.stylesheet(1, css);
  View.mount(<main className="mx-auto max-w-md p-4"><button class="rounded-lg bg-indigo-600 px-4 py-2 text-white hover:bg-indigo-700">Save</button></main>);
}"#,
        "@import \"tailwindcss\";\n",
    );
    success(project.command(&["check"]));
    success(project.command(&["dev"]));
}
