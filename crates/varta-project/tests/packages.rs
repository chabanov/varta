//! V38 bare package specifiers (`package_specifiers()`): resolution through
//! `<config dir>/node_modules`, the `types` export and the adjacent
//! declaration source map, and every refusal of that policy.
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use varta_common::{Diagnostic, DiagnosticCode};
use varta_project::CheckedProject;

const V38: &str = "checked-f64-utf16-v38";
const V37: &str = "checked-f64-utf16-v37";
const PRIVATE: &str = "PRIVATE_SOURCE_MUST_NOT_BE_READ";
const JAVASCRIPT: &str = "PACKAGE_JAVASCRIPT_MUST_NOT_BE_READ";
const MAIN: &str = r#"import { twice } from "plain";
import { greet } from "@fixture/scoped";
import { EXTRA, greet as again } from "@fixture/scoped/extra";
import { twice as same } from "../node_modules/plain/src/index.js";
import { label } from "types-entry";
export function main(): void {
  console.log(`${twice(21)} ${greet("pkg")} ${again("x")} ${EXTRA} ${same(1)} ${label()}`);
}
"#;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/packages")
}

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

struct Workspace {
    workspace: PathBuf,
    root: PathBuf,
}
impl Workspace {
    fn new(profile: &str, main: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let workspace = std::env::temp_dir().join(format!(
            "varta-packages-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&workspace);
        let root = workspace.join("app");
        fs::create_dir_all(root.join("src")).unwrap();
        let root = root.canonicalize().unwrap();
        let project = Self { workspace, root };
        project.write(
            "varta.config.json",
            &format!(
                r#"{{"entry":"src/main.ts","target":"native","out":"dist/app","languageProfile":"{profile}","capabilities":{{}}}}"#
            ),
        );
        project.write("src/main.ts", main);
        project
    }
    /// A fixture with every package `MAIN` imports.
    fn installed(profile: &str, main: &str) -> Self {
        let project = Self::new(profile, main);
        project.install("plain", "plain");
        project.install("scoped", "@fixture/scoped");
        project.install("types-entry", "types-entry");
        project
    }
    fn install(&self, fixture: &str, name: &str) {
        copy(&fixtures().join(fixture), &self.package(name));
    }
    fn package(&self, name: &str) -> PathBuf {
        self.root.join("node_modules").join(name)
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn import(&self, specifier: &str) {
        self.write(
            "src/main.ts",
            &format!(
                "import {{ f }} from {specifier:?};\nexport function main(): void {{ console.log(f()); }}\n"
            ),
        );
    }
    fn load(&self) -> varta_project::Result<CheckedProject> {
        CheckedProject::load(&self.root.join("varta.config.json"))
    }
    /// The module error at the first-line import of `importer`.
    fn error(&self, importer: &Path) -> Diagnostic {
        let error = match self.load() {
            Ok(_) => panic!("{} must not load", self.root.display()),
            Err(error) => error,
        };
        let diagnostic = error
            .diagnostic()
            .unwrap_or_else(|| panic!("{error}"))
            .clone();
        assert_eq!(diagnostic.code, DiagnosticCode::ModuleError, "{diagnostic}");
        assert_eq!(Path::new(&diagnostic.file), importer, "{diagnostic}");
        assert_eq!(diagnostic.range.start.line, 1, "{diagnostic}");
        for marker in [PRIVATE, JAVASCRIPT] {
            assert!(!diagnostic.to_string().contains(marker), "{diagnostic}");
        }
        diagnostic
    }
    fn refuses(&self, expected: &str) {
        self.refuses_in(&self.root.join("src/main.ts"), expected);
    }
    fn refuses_in(&self, importer: &Path, expected: &str) {
        let diagnostic = self.error(importer);
        assert!(
            diagnostic.message.contains(expected),
            "{expected}: {diagnostic}"
        );
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.workspace);
    }
}

#[test]
fn package_metadata_shares_one_aggregate_read_budget() {
    // Each manifest is under the per-file bound; together they exceed the
    // loader's metadata budget at the fifth package.
    const PACKAGES: usize = 5;
    const MANIFEST_BYTES: usize = 1_000_000;
    let imports: String = (0..PACKAGES)
        .map(|n| format!("import {{ twice as t{n} }} from \"p{n}\"; "))
        .collect();
    let project = Workspace::new(
        V38,
        &format!("{imports}\nexport function main(): void {{ console.log(t0(1)); }}\n"),
    );
    for n in 0..PACKAGES {
        let name = format!("p{n}");
        project.install("plain", &name);
        let manifest = format!(r#"{{"name":"{name}","types":"./dist/index.d.ts"}}"#);
        let padded = format!("{manifest}{}", " ".repeat(MANIFEST_BYTES - manifest.len()));
        fs::write(project.package(&name).join("package.json"), padded).unwrap();
    }
    project.refuses("package metadata exceeds the loader's 4194304-byte budget at package p4");
}

#[test]
fn plain_scoped_subpath_and_top_level_types_packages_link_and_run_in_v38() {
    let project = Workspace::installed(V38, MAIN);
    let checked = project.load().unwrap_or_else(|e| panic!("{e}"));
    let modules: Vec<_> = checked
        .modules
        .iter()
        .map(|module| {
            module
                .path
                .strip_prefix(&project.root)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    // One identity per source, whether named by package or by relative path;
    // dependencies before dependents in sorted order; no declaration or
    // JavaScript file is ever a module.
    assert_eq!(
        modules,
        [
            "node_modules/@fixture/scoped/source/main.ts",
            "node_modules/@fixture/scoped/source/extra.ts",
            "node_modules/plain/src/math.ts",
            "node_modules/plain/src/index.ts",
            "node_modules/types-entry/index.ts",
            "src/main.ts",
        ]
    );
    // Running the linked program instantiates and executes the Wasm module.
    let output = checked.run().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(output.exit_code, 0, "{:?}", output.lines);
    assert!(
        output
            .lines
            .iter()
            .any(|line| line.contains("42 hi pkg hi x 7 2 typed")),
        "{:?}",
        output.lines
    );
}

#[test]
fn the_package_root_falls_back_to_top_level_types_without_a_types_condition() {
    let project = Workspace::new(V38, "");
    project.install("plain", "plain");
    project.write(
        "node_modules/plain/package.json",
        r#"{"name":"plain","exports":{".":{"import":"./dist/index.js"}},"types":"./dist/index.d.ts"}"#,
    );
    project.write(
        "src/main.ts",
        "import { twice } from \"plain\";\nexport function main(): void { console.log(twice(4)); }\n",
    );
    let checked = project.load().unwrap_or_else(|e| panic!("{e}"));
    assert!(
        checked
            .modules
            .iter()
            .any(|module| module.path == project.package("plain").join("src/index.ts"))
    );
    // A subpath has no top-level fallback.
    project.write(
        "node_modules/plain/package.json",
        r#"{"name":"plain","exports":{".":{"types":"./dist/index.d.ts"},"./js":"./dist/index.js"},"types":"./dist/index.d.ts"}"#,
    );
    project.import("plain/js");
    project.refuses("unsupported untyped package plain: ./js has no 'types' export condition");
}

#[test]
fn earlier_profiles_reject_bare_specifiers_and_name_the_profile() {
    for profile in [V37, "checked-i32-utf8-v1"] {
        let project = Workspace::installed(profile, MAIN);
        project.refuses(&format!("package specifier plain requires profile {V38}"));
    }
}

/// Each case starts from a workspace with the fixture packages installed.
fn refused(prepare: impl FnOnce(&Workspace), expected: &str) {
    let project = Workspace::installed(V38, MAIN);
    project.install("declarations-only", "declarations-only");
    project.install("untyped", "untyped");
    fs::write(project.workspace.join("outside.ts"), PRIVATE).unwrap();
    prepare(&project);
    project.refuses(expected);
}

#[test]
fn package_manifests_and_exports_are_refused_outside_the_policy() {
    let manifest = |text: &'static str| {
        move |project: &Workspace| {
            project.write("node_modules/plain/package.json", text);
            project.import("plain");
        }
    };
    refused(
        manifest(r#"{"name":"plain","exports":{".":{"types":"./../outside.d.ts"}}}"#),
        "cannot leave the package directory",
    );
    refused(
        manifest(
            r#"{"name":"plain","exports":{".":{"types":"./dist/../../types-entry/types/index.d.ts"}}}"#,
        ),
        "cannot leave the package directory",
    );
    refused(
        manifest(r#"{"name":"plain","types":"../types-entry/types/index.d.ts"}"#),
        "cannot leave the package directory",
    );
    refused(
        manifest(r#"{"name":"plain","main":"./dist/index.js"}"#),
        "unsupported untyped package plain: . has no 'types' export condition and no top-level 'types'",
    );
    refused(
        manifest(r#"{"name":"plain","exports":{".":{"types":"dist/index.d.ts"}}}"#),
        "export targets start with ./",
    );
    refused(
        manifest(r#"{"name":"plain","exports":{".":{"types":"./src/index.ts"}}}"#),
        "unsupported types target",
    );
    refused(
        manifest(r#"{"name":"plain","exports":{".":{"types":["./dist/index.d.ts"]}}}"#),
        "unsupported fallback array",
    );
    refused(
        manifest(r#"{"name":"plain","exports":{".":{"types":{"import":"./dist/index.d.ts"}}}}"#),
        "unsupported nested condition under 'types'",
    );
    refused(manifest("[]"), "package.json must be a JSON object");
    refused(manifest("{"), "invalid");
    refused(
        |project| {
            let padding = " ".repeat(1024 * 1024);
            project.write(
                "node_modules/plain/package.json",
                &format!(r#"{{"name":"plain",{padding}"types":"./dist/index.d.ts"}}"#),
            );
            project.import("plain");
        },
        "exceeds",
    );
    refused(
        |project| {
            project.write(
                "node_modules/plain/package.json",
                r#"{"name":"plain","exports":{".":{"types":"./dist/index.d.ts"},"./features/*":{"types":"./dist/*.d.ts"}}}"#,
            );
            project.import("plain/features/x");
        },
        "unsupported exports pattern in package plain: ./features/x",
    );
    refused(
        |project| project.import("plain/missing"),
        "package plain does not export ./missing",
    );
    refused(
        |project| project.import("types-entry/index.js"),
        "unsupported deep import types-entry/index.js",
    );
    refused(
        |project| project.import("declarations-only"),
        "unsupported declarations-only package declarations-only",
    );
    refused(
        |project| project.import("untyped"),
        "unsupported untyped package untyped",
    );
    refused(
        |project| project.import("Plain"),
        "invalid package specifier Plain",
    );
    refused(
        |project| project.import("node:fs"),
        "or package names without URL/query syntax",
    );
}

#[test]
fn packages_resolve_only_from_the_configuration_directory() {
    // Never a parent directory's node_modules, never a nested one.
    refused(
        |project| {
            copy(
                &fixtures().join("plain"),
                &project.workspace.join("node_modules/outer"),
            );
            project.import("outer");
        },
        "cannot resolve package outer",
    );
    // A package's own bare imports resolve from the same directory, so a
    // nested node_modules is never searched; the diagnostic is at the import
    // inside the package source.
    let project = Workspace::installed(V38, MAIN);
    copy(
        &fixtures().join("types-entry"),
        &project.package("plain").join("node_modules/nested"),
    );
    let importer = project.package("plain").join("src/index.ts");
    fs::write(
        &importer,
        "import { label } from \"nested\";\nexport function twice(n: number): number { return n * 2 + label().length; }\n",
    )
    .unwrap();
    project.refuses_in(&importer, "cannot resolve package nested");
    // Installed at the top level, the same import links.
    project.install("types-entry", "nested");
    project.load().unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn declaration_source_maps_must_name_one_shipped_typescript_source_inside_the_package() {
    let map = |sources: &'static str| {
        move |project: &Workspace| {
            project.write(
                "node_modules/plain/dist/index.d.ts.map",
                &format!(r#"{{"version":3,"file":"index.d.ts","sourceRoot":"","sources":{sources},"names":[],"mappings":""}}"#),
            );
            project.import("plain");
        }
    };
    refused(
        map(r#"["../../types-entry/index.ts"]"#),
        "which is outside the package",
    );
    refused(
        map(r#"["../../../../outside.ts"]"#),
        "which is outside the package",
    );
    refused(map(r#"["/etc/hosts.ts"]"#), "which is outside the package");
    refused(
        map(r#"["file:///outside.ts"]"#),
        "which is outside the package",
    );
    refused(
        map(r#"["../dist/index.js"]"#),
        "unsupported untyped package plain",
    );
    refused(
        map(r#"["./index.d.ts"]"#),
        "unsupported declarations-only package plain",
    );
    refused(
        map(r#"["../src/absent.ts"]"#),
        "unsupported declarations-only package plain",
    );
    refused(
        map(r#"["../src/index.ts","../src/math.ts"]"#),
        "exactly one source",
    );
    refused(map("[]"), "exactly one source");
    refused(
        |project| {
            project.write(
                "node_modules/plain/dist/index.d.ts.map",
                r#"{"version":3,"sourceRoot":"../src","sources":["index.ts"],"mappings":""}"#,
            );
            project.import("plain");
        },
        "sourceRoot is unsupported",
    );
    refused(
        |project| {
            project.write(
                "node_modules/plain/dist/index.d.ts.map",
                r#"{"version":2,"sources":["../src/index.ts"],"mappings":""}"#,
            );
            project.import("plain");
        },
        "revision 3",
    );
    refused(
        |project| {
            fs::remove_file(project.package("plain").join("dist/index.d.ts")).unwrap();
            project.import("plain");
        },
        "the declaration file does not exist",
    );
}

#[test]
#[cfg(unix)]
fn symlinked_package_directories_and_files_are_rejected() {
    use std::os::unix::fs::symlink;
    let outside = |project: &Workspace| {
        let copy_root = project.workspace.join("outside-plain");
        copy(&fixtures().join("plain"), &copy_root);
        fs::write(copy_root.join("src/index.ts"), PRIVATE).unwrap();
        copy_root
    };
    // The package directory, outside or inside the configuration directory.
    refused(
        |project| {
            let target = outside(project);
            fs::remove_dir_all(project.package("plain")).unwrap();
            symlink(target, project.package("plain")).unwrap();
            project.import("plain");
        },
        "cannot read",
    );
    refused(
        |project| {
            copy(
                &fixtures().join("plain"),
                &project.root.join("vendor/plain"),
            );
            fs::remove_dir_all(project.package("plain")).unwrap();
            symlink(project.root.join("vendor/plain"), project.package("plain")).unwrap();
            project.import("plain");
        },
        "cannot read",
    );
    // node_modules itself.
    refused(
        |project| {
            let target = project.workspace.join("modules");
            fs::rename(project.root.join("node_modules"), &target).unwrap();
            symlink(&target, project.root.join("node_modules")).unwrap();
            project.import("plain");
        },
        "cannot read",
    );
    // A manifest, a source map and a source file.
    for (file, expected) in [
        ("package.json", "cannot read"),
        ("dist/index.d.ts.map", "cannot read"),
        ("src/index.ts", "cannot load module"),
    ] {
        refused(
            |project| {
                let target = outside(project).join(file);
                let link = project.package("plain").join(file);
                fs::remove_file(&link).unwrap();
                symlink(target, link).unwrap();
                project.import("plain");
            },
            expected,
        );
    }
}
