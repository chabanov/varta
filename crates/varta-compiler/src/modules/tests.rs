use super::*;
use std::cell::RefCell;

#[derive(Default)]
struct MemoryLoader {
    sources: BTreeMap<PathBuf, String>,
    loads: RefCell<Vec<PathBuf>>,
}
impl MemoryLoader {
    fn insert(&mut self, name: &str, source: String) {
        self.sources.insert(PathBuf::from(name), source);
    }
}
impl ModuleLoader for MemoryLoader {
    fn resolve(&self, importer: &Path, specifier: &str) -> Result<PathBuf, CompilerError> {
        Ok(importer
            .parent()
            .unwrap()
            .join(specifier.trim_start_matches("./")))
    }
    fn load(&self, path: &Path) -> Result<String, CompilerError> {
        self.loads.borrow_mut().push(path.to_owned());
        self.sources
            .get(path)
            .cloned()
            .ok_or_else(|| module_error("unknown virtual module"))
    }
}
fn entry(source: String) -> ModuleSource {
    ModuleSource {
        path: "main.ts".into(),
        text: source,
    }
}
fn star(count: usize) -> String {
    let mut source = String::new();
    for i in 0..count {
        source.push_str(&format!("import {{f as f{i}}} from './m{i:03}.ts';\n"));
    }
    source.push_str("export function main():void{}");
    source
}

#[test]
fn shared_dependencies_load_once_and_module_limit_stops_before_extra_io() {
    let mut loader = MemoryLoader::default();
    for i in 0..MAX_MODULES {
        loader.insert(&format!("m{i:03}.ts"), "export function f():void{}".into());
    }
    Graph::load(entry(star(MAX_MODULES - 1)), &loader).unwrap();
    assert_eq!(loader.loads.borrow().len(), MAX_MODULES - 1);
    loader.loads.borrow_mut().clear();
    let error = Graph::load(entry(star(MAX_MODULES)), &loader)
        .err()
        .unwrap();
    assert!(error.to_string().contains("module/depth limits"));
    assert_eq!(loader.loads.borrow().len(), MAX_MODULES - 1);

    loader.loads.borrow_mut().clear();
    Graph::load(
        entry("import {f as a} from './m000.ts'; import {f as b} from './m000.ts'; export function main():void{}".into()),
        &loader,
    ).unwrap();
    assert_eq!(*loader.loads.borrow(), [PathBuf::from("m000.ts")]);
}

#[test]
fn depth_limit_stops_before_loading_the_next_dependency() {
    let mut loader = MemoryLoader::default();
    for i in 0..MAX_DEPTH {
        loader.insert(
            &format!("m{i:03}.ts"),
            format!(
                "import {{f as next}} from './m{:03}.ts'; export function f():void{{next();}}",
                i + 1
            ),
        );
    }
    let error = Graph::load(entry(star(1)), &loader).err().unwrap();
    assert!(error.to_string().contains("module/depth limits"));
    assert_eq!(loader.loads.borrow().len(), MAX_DEPTH - 1);
    assert_eq!(error.diagnostic().unwrap().file, "m030.ts");
}

#[test]
fn imports_and_aggregate_source_are_bounded() {
    let loader = MemoryLoader::default();
    let mut imports = String::new();
    for i in 0..=MAX_IMPORTS {
        imports.push_str(&format!("import {{f as f{i}}} from './same.ts';\n"));
    }
    let error = Graph::load(entry(imports), &loader).err().unwrap();
    assert!(error.to_string().contains("import limit"));
    assert!(loader.loads.borrow().is_empty());

    let mut loader = MemoryLoader::default();
    for i in 0..5 {
        let mut source = "export function f():void{}\n//".to_owned();
        source.push_str(&"x".repeat(1024 * 1024 - source.len()));
        loader.insert(&format!("m{i:03}.ts"), source);
    }
    let error = Graph::load(entry(star(5)), &loader).err().unwrap();
    assert!(error.to_string().contains("total source budget"));
    assert_eq!(loader.loads.borrow().len(), 4);
}

#[test]
fn static_data_diagnostics_retain_the_dependency_source() {
    let mut loader = MemoryLoader::default();
    let source = format!(
        "export function f():void{{\n console.log({:?});\n}}",
        "X".repeat(65_537)
    );
    loader.insert("lib.ts", source.clone());
    let error = crate::compile_modules(
        entry("import {f} from './lib.ts'; export function main():void{f();}".into()),
        &loader,
    )
    .unwrap_err();
    let diagnostic = error.diagnostic().unwrap();
    assert_eq!(diagnostic.file, "lib.ts");
    assert_eq!(diagnostic.range.start.line, 2);
    assert!(diagnostic.message.contains("static string data"));
    assert_eq!(source.as_bytes()[diagnostic.range.start.byte_offset], b'"');
}

#[test]
fn only_the_entry_may_supply_the_start_function_and_exports_are_checked() {
    let mut loader = MemoryLoader::default();
    loader.insert("lib.ts", "export function main():void{}".into());
    let error = crate::compile_modules(entry("import {main} from './lib.ts';".into()), &loader)
        .unwrap_err();
    assert!(error.to_string().contains("entry module must declare"));
    for source in [
        "function f():void{} export {f, f}; export function main():void{}",
        "export {missing}; export function main():void{}",
        "export function main():void{} export {main as default};",
    ] {
        assert!(crate::compile_modules(entry(source.into()), &loader).is_err());
    }
}

const REEXPORTS: varta_common::language::LanguageProfile =
    varta_common::language::LanguageProfile::CheckedF64Utf16V32;
const NAMED_TYPES: varta_common::language::LanguageProfile =
    varta_common::language::LanguageProfile::CheckedF64Utf16V31;

fn reexport_loader() -> MemoryLoader {
    let mut loader = MemoryLoader::default();
    loader.insert(
        "a.ts",
        "export interface Tag{name:string}export function f():number{return 1;}\
         export function shared():number{return 3;}export function only():number{return 4;}"
            .into(),
    );
    loader.insert(
        "b.ts",
        "export type Tag={name:string;weight:number};export function f():number{return 2;}\
         export {shared} from './a.ts';"
            .into(),
    );
    loader.insert(
        "stars.ts",
        "export * from './a.ts';export * from './b.ts';".into(),
    );
    loader.insert(
        "pick.ts",
        "export * from './a.ts';export * from './b.ts';export {f} from './b.ts';\
         export type {Tag} from './a.ts';"
            .into(),
    );
    loader.insert(
        "named.ts",
        "export {f as first, only} from './a.ts';export type {Tag as Label} from './a.ts';\
         export {type Tag} from './b.ts';"
            .into(),
    );
    loader.insert("chain.ts", "export * from './stars.ts';".into());
    loader
}
fn reexport(
    source: &str,
    profile: varta_common::language::LanguageProfile,
) -> Result<Compilation, CompilerError> {
    crate::compile_modules_with_profile(entry(source.into()), &reexport_loader(), profile)
}
fn reexport_error(source: &str, profile: varta_common::language::LanguageProfile) -> CompilerError {
    reexport(source, profile)
        .err()
        .unwrap_or_else(|| panic!("accepted under {profile:?}: {source}"))
}

#[test]
fn reexports_forward_functions_and_types_without_binding_them_locally() {
    let compiled = reexport(
        "import {first,only,type Label,Tag} from './named.ts';export function main():void{\
         const t:Label={name:'x'};const u:Tag={name:'y',weight:1};\
         console.log(first()+only()+u.weight);console.log(t.name);}",
        REEXPORTS,
    )
    .unwrap();
    // Re-export sources are graph dependencies, linked before the re-exporter.
    let order: Vec<_> = compiled.modules.iter().map(|m| m.path.clone()).collect();
    assert_eq!(
        order,
        ["a.ts", "b.ts", "named.ts", "main.ts"].map(PathBuf::from)
    );
    assert_eq!(
        compiled.modules[2].imports,
        ["a.ts", "b.ts"].map(PathBuf::from)
    );
    for (source, expected) in [
        // A re-exported name is not a local binding of the re-exporter.
        (
            "export {only} from './a.ts';export function main():void{only();}",
            "unsupported call",
        ),
        (
            "export {only} from './a.ts';export {only as again};export function main():void{}",
            "export only is not a function binding",
        ),
        (
            "export {nope} from './a.ts';export function main():void{}",
            // v32 also has module constants in the value namespace.
            "module a.ts does not export function or constant nope",
        ),
        (
            "export type {only} from './a.ts';export function main():void{}",
            "module a.ts does not export type only",
        ),
        (
            "export {only} from './a.ts';export {only} from './a.ts';export function main():void{}",
            "duplicate export: only",
        ),
        (
            "export function main():void{} export {f as main} from './a.ts';",
            "duplicate export: main",
        ),
        (
            "import {Label} from './named.ts';export function main():void{Label();}",
            "unsupported call",
        ),
    ] {
        let error = reexport_error(source, REEXPORTS);
        assert!(error.to_string().contains(expected), "{source}: {error}");
        assert_eq!(error.diagnostic().unwrap().file, "main.ts");
    }
}

#[test]
fn star_reexports_follow_ecmascript_ambiguity_rules() {
    // `shared` reaches `stars.ts` from both sources as the same binding and is
    // not ambiguous; star exports chain transitively through `chain.ts`.
    reexport(
        "import {shared,only} from './chain.ts';export function main():void{console.log(shared()+only());}",
        REEXPORTS,
    )
    .unwrap();
    // Explicit exports win over names that would be ambiguous.
    reexport(
        "import {f,type Tag} from './pick.ts';export function main():void{\
         const t:Tag={name:'a'};console.log(f());console.log(t.name);}",
        REEXPORTS,
    )
    .unwrap();
    // Ambiguous names are excluded silently; only naming one is an error.
    for (source, module) in [
        ("import {f} from './stars.ts';", "stars.ts"),
        ("import type {Tag} from './stars.ts';", "stars.ts"),
        ("import {Tag} from './stars.ts';", "stars.ts"),
        ("import {f} from './chain.ts';", "chain.ts"),
        ("export {f} from './stars.ts';", "stars.ts"),
        ("export type {Tag} from './chain.ts';", "chain.ts"),
    ] {
        let error = reexport_error(
            &format!("{source}export function main():void{{}}"),
            REEXPORTS,
        );
        let expected = format!("module {module} exports ");
        assert!(
            error.to_string().contains(&expected)
                && error.to_string().contains("ambiguously through export *"),
            "{source}: {error}"
        );
        assert_eq!(error.diagnostic().unwrap().file, "main.ts");
    }
    // A local export shadows the star name of the same module.
    reexport(
        "export * from './a.ts';export function f():number{return 9;}export function main():void{console.log(f());}",
        REEXPORTS,
    )
    .unwrap();
}

#[test]
fn reexport_sources_are_graph_edges_with_cycles_and_limits() {
    let mut loader = MemoryLoader::default();
    loader.insert("a.ts", "export * from './b.ts';".into());
    loader.insert("b.ts", "export {main} from './main.ts';".into());
    let error = Graph::load_with_profile(
        entry("export * from './a.ts';export function main():void{}".into()),
        &loader,
        REEXPORTS,
    )
    .err()
    .unwrap();
    assert!(
        error
            .to_string()
            .contains("cyclic module dependency: main.ts -> a.ts -> b.ts -> main.ts"),
        "{error}"
    );

    let mut source = String::new();
    for i in 0..=MAX_IMPORTS {
        source.push_str(&format!("export * from './m{i:03}.ts';\n"));
    }
    let loader = MemoryLoader::default();
    let error = Graph::load_with_profile(entry(source), &loader, REEXPORTS)
        .err()
        .unwrap();
    assert!(error.to_string().contains("import limit"), "{error}");
    assert!(loader.loads.borrow().is_empty());
}

#[test]
fn reexport_forms_outside_the_subset_and_earlier_profiles_are_rejected() {
    for (source, expected) in [
        ("export * as ns from './a.ts';", "namespace re-export"),
        ("export {} from './a.ts';", "side-effect re-export"),
        (
            "export {f as \"f-1\"} from './a.ts';",
            "string or default export name",
        ),
        (
            "export type * from './a.ts';",
            "type-only or attributed star re-export",
        ),
        (
            "export * from './a.ts' with {type:'json'};",
            "type-only or attributed star re-export",
        ),
        (
            "export {f} from './a.ts' with {type:'json'};",
            "re-export or type-only export",
        ),
    ] {
        let error = reexport_error(
            &format!("{source}export function main():void{{}}"),
            REEXPORTS,
        );
        assert!(error.to_string().contains(expected), "{source}: {error}");
    }
    // v31 and earlier keep their exact diagnostics.
    for profile in [NAMED_TYPES, Default::default()] {
        for (source, expected) in [
            ("export {f} from './a.ts';", "re-export or type-only export"),
            ("export * from './a.ts';", "module export form"),
            (
                "export * as ns from './a.ts';",
                "re-export or type-only export",
            ),
            (
                "export {default} from './a.ts';",
                "re-export or type-only export",
            ),
        ] {
            let error =
                reexport_error(&format!("{source}export function main():void{{}}"), profile);
            assert!(error.to_string().contains(expected), "{source}: {error}");
        }
    }
    let error = reexport_error(
        "export type {Tag} from './a.ts';export function main():void{}",
        NAMED_TYPES,
    );
    assert!(error.to_string().contains("re-export or type-only export"));
}

const DEFAULTS: varta_common::language::LanguageProfile =
    varta_common::language::LanguageProfile::CheckedF64Utf16V32;

#[test]
fn default_exports_scan_as_bindings_named_default() {
    let mut loader = MemoryLoader::default();
    loader.insert(
        "lib.ts",
        "export default function make():number{return 1;}export function other():number{return 2;}"
            .into(),
    );
    loader.insert("anon.ts", "export default class{}".into());
    loader.insert("value.ts", "export default 3;".into());
    loader.insert(
        "alias.ts",
        "function f():void{}export {f as default};".into(),
    );
    loader.insert(
        "forward.ts",
        "export {default} from './lib.ts';export {default as anon} from './anon.ts';".into(),
    );
    let graph = Graph::load_with_profile(
        entry(
            "import make,{other} from './lib.ts';import type A from './anon.ts';\
             import v from './value.ts';import {default as f} from './alias.ts';\
             import fwd,{anon} from './forward.ts';export function main():void{}"
                .into(),
        ),
        &loader,
        DEFAULTS,
    )
    .unwrap();
    let unit = |name: &str| &graph.units[Path::new(name)];
    let exports = |name: &str| -> Vec<(String, String, bool)> {
        unit(name)
            .exports
            .iter()
            .map(|e| (e.local.clone(), e.exported.clone(), e.from.is_some()))
            .collect()
    };
    let export = |local: &str, exported: &str, from: bool| (local.into(), exported.into(), from);
    assert_eq!(
        exports("lib.ts"),
        [
            export("make", "default", false),
            export("other", "other", false)
        ]
    );
    // Anonymous declarations and expressions bind ECMAScript's `*default*`.
    assert_eq!(exports("anon.ts"), [export("*default*", "default", false)]);
    assert_eq!(exports("value.ts"), [export("*default*", "default", false)]);
    assert_eq!(exports("alias.ts"), [export("f", "default", false)]);
    assert_eq!(
        exports("forward.ts"),
        [
            export("default", "default", true),
            export("default", "anon", true)
        ]
    );
    // The default declaration is rewritten into an ordinary declaration.
    assert!(matches!(
        &unit("anon.ts").ast.body[0],
        ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Class(class))) if class.ident.sym == "*default*"
    ));
    let bindings: Vec<_> = unit("main.ts")
        .imports
        .iter()
        .flat_map(|i| &i.bindings)
        .map(|b| (b.imported.as_str(), b.local.as_str(), b.type_only))
        .collect();
    assert_eq!(
        bindings,
        // Dependencies are sorted by target path.
        [
            ("default", "f", false),
            ("default", "A", true),
            ("default", "fwd", false),
            ("anon", "anon", false),
            ("default", "make", false),
            ("other", "other", false),
            ("default", "v", false),
        ]
    );
}

#[test]
fn default_exports_keep_namespace_forms_and_earlier_profiles_rejected() {
    let mut loader = MemoryLoader::default();
    loader.insert("lib.ts", "export default function f():void{}".into());
    let error = |source: &str, profile| {
        Graph::load_with_profile(entry(source.into()), &loader, profile)
            .err()
            .unwrap_or_else(|| panic!("accepted under {profile:?}: {source}"))
            .to_string()
    };
    for (source, expected) in [
        (
            "import * as lib from './lib.ts';",
            "namespace import (import * as name)",
        ),
        (
            "import f, * as lib from './lib.ts';",
            "namespace import (import * as name)",
        ),
        ("export * as lib from './lib.ts';", "namespace re-export"),
        (
            "export default 1;export default 2;",
            "duplicate export: default",
        ),
        (
            "export default X;const X=1;",
            "default export of X precedes its declaration",
        ),
    ] {
        let message = error(source, DEFAULTS);
        assert!(message.contains(expected), "{source}: {message}");
    }
    for profile in [NAMED_TYPES, Default::default()] {
        for (source, expected) in [
            ("import f from './lib.ts';", "default or namespace import"),
            ("export default function f():void{}", "module export form"),
            ("export default 1;", "module export form"),
        ] {
            let message = error(source, profile);
            assert!(message.contains(expected), "{source}: {message}");
        }
    }
}

const V34: varta_common::language::LanguageProfile =
    varta_common::language::LanguageProfile::CheckedF64Utf16V34;
const V35: varta_common::language::LanguageProfile =
    varta_common::language::LanguageProfile::CheckedF64Utf16V35;

#[test]
fn engine_preludes_load_only_for_named_web_types_and_stay_out_of_metadata() {
    let loader = MemoryLoader::default();
    let preludes = |source: &str, profile| {
        let graph = Graph::load_with_profile(entry(source.into()), &loader, profile).unwrap();
        // Preludes are compiler-owned: module metadata lists program sources only.
        let metadata = graph.metadata();
        assert_eq!(metadata.len(), 1, "{source}");
        assert!(metadata[0].imports.is_empty(), "{source}");
        let mut loaded: Vec<String> = graph
            .order
            .iter()
            .filter(|path| engine::is_prelude(path))
            .map(|path| path.display().to_string())
            .collect();
        loaded.sort();
        loaded
    };
    let headers = engine::HEADERS.path.to_owned();
    let abort = engine::ABORT.path.to_owned();
    for (source, mut expected) in [
        ("export function main():void{}", vec![]),
        // Property names, object keys and strings are not references.
        (
            "export function main():void{const o={Headers:1};console.log(o.Headers+'AbortSignal');}",
            vec![],
        ),
        (
            "export function main():void{const h=new Headers();}",
            vec![headers.clone()],
        ),
        (
            "function f(init:HeadersInit):void{} export function main():void{}",
            vec![headers.clone()],
        ),
        (
            "function f(s:AbortSignal):void{} export function main():void{}",
            vec![abort.clone()],
        ),
        (
            "export function main():void{const e=new DOMException('m');const h=new Headers();}",
            vec![abort.clone(), headers.clone()],
        ),
        // A module's own declaration of the name takes precedence.
        (
            "class Headers{} export function main():void{const h=new Headers();}",
            vec![],
        ),
    ] {
        expected.sort();
        assert_eq!(preludes(source, V35), expected, "{source}");
        // Earlier profiles never load a prelude and keep their diagnostics.
        if let Ok(graph) = Graph::load_with_profile(entry(source.into()), &loader, V34) {
            assert!(!graph.order.iter().any(|path| engine::is_prelude(path)));
        }
    }
}

#[test]
fn v36_selects_the_abort_listener_prelude_and_earlier_profiles_keep_theirs() {
    use varta_common::language::{LanguageProfile as P, SUPPORTED_PROFILES};
    let listeners = engine::ABORT.text(P::CheckedF64Utf16V36);
    assert_ne!(listeners, engine::ABORT.source);
    assert!(listeners.contains("addEventListener"));
    assert!(!engine::ABORT.source.contains("addEventListener"));
    for &profile in SUPPORTED_PROFILES {
        // Only `function_fields` profiles compile the extended source, under
        // the same reserved path; Headers has a single source.
        assert_eq!(
            engine::ABORT.text(profile) == listeners,
            profile.function_fields(),
            "{profile:?}"
        );
        assert_eq!(engine::HEADERS.text(profile), engine::HEADERS.source);
    }
    let loader = MemoryLoader::default();
    let graph = Graph::load_with_profile(
        entry("export function main():void{const c=new AbortController();}".into()),
        &loader,
        P::CheckedF64Utf16V36,
    )
    .unwrap();
    assert!(
        graph
            .order
            .iter()
            .any(|path| path == Path::new(engine::ABORT.path))
    );
}

#[test]
fn engine_prelude_paths_are_reserved_from_host_loaders() {
    struct Reserved;
    impl ModuleLoader for Reserved {
        fn resolve(&self, _: &Path, _: &str) -> Result<PathBuf, CompilerError> {
            Ok(PathBuf::from(engine::HEADERS.path))
        }
        fn load(&self, _: &Path) -> Result<String, CompilerError> {
            Err(module_error("must not load a reserved path"))
        }
    }
    let error = Graph::load_with_profile(
        entry("import {Headers} from './web.ts';export function main():void{}".into()),
        &Reserved,
        V35,
    )
    .err()
    .unwrap()
    .to_string();
    assert!(error.contains("reserved for an engine prelude"), "{error}");
}

#[test]
fn programs_that_do_not_name_web_types_compile_as_without_preludes() {
    // A v35 program that names no Web type compiles to exactly the bytes the
    // same profile produces when the engine has no preludes at all.
    let source = "class Box{value:string;constructor(value:string){this.value=value;}}\
        export function main():void{const o={Headers:'x'};const b=new Box(o.Headers+'Headers');\
        console.log(b.value);const list=[1,2,3];list.forEach((n)=>console.log(String(n)));}";
    let compile = |source: &str| crate::compile_typescript_with_profile(source, V35);
    let with = compile(source).unwrap();
    wasmtime::Module::new(&wasmtime::Engine::default(), &with).unwrap();
    assert_eq!(with, engine::without_preludes(|| compile(source)).unwrap());
    // The switch removes the preludes: a program naming one then fails.
    let named =
        "export function main():void{const h=new Headers();console.log(String(h.has('a')));}";
    assert!(compile(named).is_ok());
    assert!(engine::without_preludes(|| compile(named)).is_err());
}
