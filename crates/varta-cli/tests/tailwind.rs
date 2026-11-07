//! K8 T1: build-time Tailwind stylesheets. The project's `tailwind.entry`
//! is compiled from the program's class tokens and served as the text of the
//! entry's text resource, which the program applies with `View.stylesheet`.
//! Full Tailwind output, Preflight included, applies through the T2
//! engine.
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

/// Static Tailwind class tokens such as `w-1/2` follow the HTML class
/// model (T2 stage S1), and their escaped selectors and math (S3) apply.
#[test]
fn tailwind_class_tokens_apply_with_the_t2_class_grammar() {
    let project = Project::new(
        r#"import css from "./app.css" with { type: "text" };
export function main(): void {
  View.stylesheet(1, css);
  console.log(JSON.stringify(css));
  View.mount(<p className="w-1/2">x</p>);
}"#,
        "@tailwind utilities;\n",
    );
    success(project.command(&["check"]));
    let output = success(project.command(&["dev"]));
    assert!(output.contains(r".w-1\\/2"), "{output}");
}

/// The end-to-end goal of K8: ordinary TSX with Tailwind classes, no
/// hand-written CSS. The full `@import "tailwindcss"` stylesheet (layers,
/// `@property`, theme variables, the whole of Preflight with its
/// pseudo-elements, the rules CSS error handling drops and the element
/// types outside the view, escaped class selectors) is admitted and applied.
#[test]
fn full_tailwind_application_applies() {
    let project = Project::new(
        r#"import css from "./app.css" with { type: "text" };
export function main(): void {
  View.stylesheet(1, css);
  View.mount(<main className="mx-auto max-w-md p-4 [text-wrap:pretty]"><button class="relative w-fit rounded-lg bg-indigo-600 px-4 py-2 text-white shadow-sm transition-shadow duration-150 hover:shadow-md focus:ring-2 hover:bg-indigo-700 after:absolute after:inset-x-0 after:bottom-0 after:h-0.5 after:bg-indigo-300">Save</button><input class="block ring-1 placeholder:text-gray-400 accent-indigo-600" placeholder="Name" /><label class="sr-only">Name</label><section class="sticky top-0"><div class="inline-block table-auto"><button class="inline-flex tabular-nums">123</button></div><p class="line-clamp-2">Two lines of text</p><p class="line-clamp-3">Three lines of text</p><div class="h-4 w-4 animate-pulse bg-gradient-to-b from-indigo-500 to-blue-500"></div><div class="h-4 w-4 animate-spin bg-indigo-600"></div><div class="backdrop-blur">Blurred backdrop</div></section></main>);
  console.log("tailwind-css-ready");
}"#,
        "@import \"tailwindcss\";\n",
    );
    success(project.command(&["check"]));
    assert!(success(project.command(&["dev"])).contains("tailwind-css-ready"));
    success(project.command(&["build", "--single"]));
    fs::remove_dir_all(project.root.join("src")).unwrap();
    fs::remove_file(project.root.join("varta.config.json")).unwrap();
    let output = Command::new(project.root.join("dist/app"))
        .current_dir(&project.workspace)
        .output()
        .unwrap();
    assert!(success(output).contains("tailwind-css-ready"));
}

/// Project stylesheets imported by the entry are read at build time through
/// the project loader: relative paths only, inside the configuration
/// directory, never through a symlink, never at run time.
#[test]
fn entry_imports_read_project_stylesheets_under_the_path_policy() {
    let project = Project::new(
        UTILITIES_MAIN,
        "@import \"./parts/buttons.css\";\n@tailwind utilities;\n",
    );
    project.write(
        "src/parts/buttons.css",
        "@import \"../shared/base.css\";\n.btn {\n  @apply hidden;\n}\n",
    );
    project.write("src/shared/base.css", ".base {\n  display: block;\n}\n");
    let expected = r#"varta log: ".base {\n  display: block;\n}\n.btn {\n  display: none;\n}\n.flex {\n  display: flex;\n}\n.hidden {\n  display: none;\n}\n""#;
    let output = success(project.command(&["dev"]));
    assert!(output.contains(expected), "{output}");

    project.write(
        "src/parts/buttons.css",
        "@import \"../../../outside.css\";\n",
    );
    let output = project.command(&["check"]);
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(
        text(&output).contains("escapes project root"),
        "{}",
        text(&output)
    );

    #[cfg(unix)]
    {
        fs::remove_file(project.root.join("src/parts/buttons.css")).unwrap();
        std::os::unix::fs::symlink(
            project.root.join("src/shared/base.css"),
            project.root.join("src/parts/buttons.css"),
        )
        .unwrap();
        let output = project.command(&["check"]);
        assert_eq!(output.status.code(), Some(1), "{}", text(&output));
        assert!(
            text(&output).contains("parts/buttons.css"),
            "{}",
            text(&output)
        );
    }
}

/// A class composed from a finite literal union enumerates every name (a
/// Varta extension beyond Tailwind's scanner).
#[test]
fn composed_class_names_with_literal_types_generate_every_combination() {
    let project = Project::new(
        r#"import css from "./app.css" with { type: "text" };
type Kind = "block" | "inline";
function Box(props: { kind: Kind }): number {
  return <p class={`${props.kind}-box`}>x</p>;
}
export function main(): void {
  View.stylesheet(1, css);
  console.log(JSON.stringify(css));
  View.mount(Box({ kind: "block" }));
}"#,
        "@utility block-box {\n  display: block;\n}\n@utility inline-box {\n  display: inline;\n}\n@tailwind utilities;\n",
    );
    // `block` and `inline` are tokens too (the literal type's members), and
    // Tailwind utilities, as Tailwind's own scanner would find them.
    let expected = r#"varta log: ".block {\n  display: block;\n}\n.block-box {\n  display: block;\n}\n.inline {\n  display: inline;\n}\n.inline-box {\n  display: inline;\n}\n""#;
    let output = success(project.command(&["dev"]));
    assert!(output.contains(expected), "{output}");
}
