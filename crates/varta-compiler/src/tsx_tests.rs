//! V38 TSX templates (`tsx()`) and the view prelude (`view_protocol()`):
//! compile-and-validate, template type errors, profile gating and the typed
//! `JSX` namespace of `varta.d.ts`.
use crate::{CompilerError, ModuleLoader, ModuleSource};
use std::path::{Path, PathBuf};
use varta_common::language::LanguageProfile as P;

const V37: P = P::CheckedF64Utf16V37;
const V38: P = P::CheckedF64Utf16V38;

/// One in-memory entry module and no dependencies.
struct Single;
impl ModuleLoader for Single {
    fn resolve(&self, _: &Path, specifier: &str) -> Result<PathBuf, CompilerError> {
        Err(CompilerError::Module(format!("no module {specifier}")))
    }
    fn load(&self, path: &Path) -> Result<String, CompilerError> {
        Err(CompilerError::Module(format!(
            "no module {}",
            path.display()
        )))
    }
}

fn compile(path: &str, text: &str, profile: P) -> Result<Vec<u8>, CompilerError> {
    crate::compile_modules_with_profile(
        ModuleSource {
            path: path.into(),
            text: text.into(),
        },
        &Single,
        profile,
    )
    .map(|compilation| compilation.wasm)
}

fn valid(text: &str) -> Vec<u8> {
    let bytes = compile("app.tsx", text, V38).unwrap_or_else(|e| panic!("{e}\n{text}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}

fn rejected(text: &str) -> String {
    compile("app.tsx", text, V38)
        .err()
        .unwrap_or_else(|| panic!("accepted:\n{text}"))
        .to_string()
}

fn imports_and_exports(bytes: &[u8]) -> (Vec<String>, Vec<String>) {
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        match payload.unwrap() {
            wasmparser::Payload::ImportSection(reader) => {
                for import in reader {
                    imports.push(import.unwrap().name.to_owned());
                }
            }
            wasmparser::Payload::ExportSection(reader) => {
                for export in reader {
                    exports.push(export.unwrap().name.to_owned());
                }
            }
            _ => {}
        }
    }
    (imports, exports)
}

const APP: &str = r#"
function Greeting(props: { name: string }): number {
  return <p class="greeting">Hello {props.name}</p>;
}
export function main(): void {
  let clicks = 0;
  View.mount(
    <main id="app" aria-label="Demo">
      <h1 style="color: #111; font-size: 2rem">Title</h1>
      <Greeting name="Varta" />
      <button
        id="go"
        type="button"
        disabled={clicks > 3}
        style:opacity={clicks === 0 ? '1' : '0.5'}
        onClick={() => { clicks += 1; }}
        ref={(node: number) => View.command(node, 1, 0)}
      >
        Go
      </button>
      <input id="field" type="text" value={`${clicks}`} onKeyDown:Enter-noShift-preventDefault={(event: ViewEvent) => console.log(event.key)} />
      <a href="/docs#intro">Docs</a>
    </main>,
  );
}
"#;

#[test]
fn templates_compile_to_validated_modules_with_the_view_contract() {
    let bytes = valid(APP);
    let (imports, exports) = imports_and_exports(&bytes);
    assert!(imports.contains(&varta_common::view::COMMIT_IMPORT.to_string()));
    assert!(exports.contains(&varta_common::view::EVENT_EXPORT.to_string()));
}

#[test]
fn a_v38_program_without_views_has_no_view_import_or_export() {
    let bytes = compile(
        "main.ts",
        "export function main(): void { console.log(1); }",
        V38,
    )
    .unwrap();
    let (imports, exports) = imports_and_exports(&bytes);
    assert!(!imports.iter().any(|name| name == "view_commit"));
    assert!(
        !exports
            .iter()
            .any(|name| name.starts_with("varta.view_abi"))
    );
}

#[test]
fn template_errors_name_the_construct_and_its_rule() {
    for (source, expected) in [
        ("<blink>x</blink>", "unknown intrinsic element `<blink>`"),
        (
            "<div href=\"/x\">x</div>",
            "attribute `href` is not allowed on `<div>`",
        ),
        (
            "<div onclick={() => {}}>x</div>",
            "unknown attribute `onclick`",
        ),
        (
            "<div className=\"a\">x</div>",
            "use `class` instead of `className`",
        ),
        (
            "<button disabled=\"false\">x</button>",
            "boolean attribute `disabled` takes no string value",
        ),
        (
            "<textarea rows=\"0\" />",
            "invalid value `0` for attribute `rows`",
        ),
        (
            "<input type=\"date\" />",
            "invalid value `date` for attribute `type`",
        ),
        (
            "<a href=\"//evil.test\">x</a>",
            "invalid value `//evil.test` for attribute `href`",
        ),
        (
            "<div style=\"color: red\">x</div>",
            "invalid value `red` for CSS property `color`",
        ),
        (
            "<div style=\"colour: #fff\">x</div>",
            "unknown CSS property `colour`",
        ),
        (
            "<div style=\"field-sizing: content\">x</div>",
            "applies to `<textarea>` only",
        ),
        (
            "<div class=\"a.b\">x</div>",
            "invalid value `a.b` for attribute `class`",
        ),
        (
            "<div onClick:Enter={() => {}}>x</div>",
            "unknown `click` filter `Enter`",
        ),
        (
            "<div onKeyDown:F13={() => {}}>x</div>",
            "unknown `keydown` filter `F13`",
        ),
        (
            "<div onKeyDown:shift-noShift={() => {}}>x</div>",
            "both required and forbidden",
        ),
        ("<><div /></>", "JSX fragments are not supported"),
        (
            "<div {...rest}>x</div>",
            "spread attributes are not supported",
        ),
        ("<div onClick={42}>x</div>", ""),
    ] {
        let text =
            format!("export function main(): void {{ const rest = {{}}; View.mount({source}); }}");
        let message = rejected(&text);
        assert!(message.contains(expected), "{source}: {message}");
    }
}

#[test]
fn hole_types_are_checked_by_the_type_checker() {
    // A text hole takes any template-literal operand; a boolean attribute needs
    // a boolean, an event handler a function of the event, a ref a function
    // of the node id.
    for source in [
        "<button disabled={'yes'}>x</button>",
        "<button onClick={(event: string) => {}}>x</button>",
        "<div ref={42}>x</div>",
        "<input checked={1} type=\"checkbox\" />",
    ] {
        let text = format!("export function main(): void {{ View.mount({source}); }}");
        let message = rejected(&text);
        assert!(!message.is_empty(), "{source}");
    }
    valid("export function main(): void { const n = 3; View.mount(<p>{n} items, {n > 1}</p>); }");
}

#[test]
fn components_take_typed_props_and_reject_children() {
    valid(
        "function Item(props: { label: string; count: number }): number { return <li>{props.label}</li>; }
         export function main(): void { View.mount(<ul><Item label=\"a\" count={1} /></ul>); }",
    );
    let message = rejected(
        "function Item(props: { label: string }): number { return <li>{props.label}</li>; }
         export function main(): void { View.mount(<ul><Item label={3} /></ul>); }",
    );
    assert!(!message.is_empty());
    let message = rejected(
        "function Item(): number { return <li />; }
         export function main(): void { View.mount(<Item>child</Item>); }",
    );
    assert!(
        message.contains("children of component `<Item>` are not supported"),
        "{message}"
    );
}

#[test]
fn generated_names_are_reserved() {
    let message =
        rejected("function __vartaJsx0(): number { return 0; } export function main(): void {}");
    assert!(message.contains("reserved for TSX templates"), "{message}");
}

#[test]
fn tsx_is_gated_by_profile_and_extension() {
    let text = "export function main(): void { View.mount(<div>x</div>); }";
    // V37 parses `.tsx` as TypeScript, as before v38: JSX is a syntax error.
    assert!(
        matches!(compile("app.tsx", text, V37), Err(error) if error.to_string().contains("Expression expected") || error.diagnostic().is_some())
    );
    // V38 parses JSX only in `.tsx` modules.
    assert!(compile("app.ts", text, V38).is_err());
    valid(text);
    // `View` and `ViewEvent` are v38 names only.
    assert!(
        compile(
            "main.ts",
            "export function main(): void { View.commit(); }",
            V37
        )
        .is_err()
    );
    // The view prelude intrinsics are not callable from program code.
    assert!(
        compile(
            "main.ts",
            "export function main(): void { varta.view.commit('x'); }",
            V38
        )
        .is_err()
    );
}

#[test]
fn the_jsx_namespace_of_the_host_declarations_matches_the_registry() {
    let declarations = varta_common::HOST_DECLARATIONS;
    let begin = "// generated:jsx-namespace:begin\n";
    let end = "// generated:jsx-namespace:end";
    let start = declarations.find(begin).expect("JSX namespace block") + begin.len();
    let stop = declarations[start..].find(end).expect("JSX namespace end") + start;
    let expected = crate::jsx::declarations();
    assert!(
        declarations[start..stop] == expected,
        "varta.d.ts JSX namespace is stale; replace it with:\n\n{expected}"
    );
}
