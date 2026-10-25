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
