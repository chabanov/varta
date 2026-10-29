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
            "export {default} from './a.ts';",
            "string or default export name",
        ),
        (
            "export {f as default} from './a.ts';",
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
