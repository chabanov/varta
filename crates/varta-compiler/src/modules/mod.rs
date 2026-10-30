//! A bounded, deterministic graph of explicit source modules. No source text is
//! concatenated and the compiler never performs ambient filesystem/network I/O.
mod defaults;
pub(crate) mod engine;
pub(crate) mod engine_globals;
mod global_members;
mod syntax;
#[cfg(test)]
mod tests;
mod text;
use crate::{
    CompilerError, parse,
    source::{self, Source},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use swc_common::{Span, Spanned};
use swc_ecma_ast as ast;

pub trait ModuleLoader {
    /// Return one stable identity for each module. The host enforces path policy.
    fn resolve(&self, importer: &Path, specifier: &str) -> Result<PathBuf, CompilerError>;
    fn load(&self, path: &Path) -> Result<String, CompilerError>;
    /// Return the identity of a text resource (v38 `text_imports()`:
    /// `import x from "./x.css" with { type: "text" }`), loaded with
    /// [`Self::load`] like a module. The host enforces the same path policy;
    /// a loader that does not admit text resources keeps this default.
    fn resolve_text(&self, importer: &Path, specifier: &str) -> Result<PathBuf, CompilerError> {
        let _ = (importer, specifier);
        Err(module_error(
            "text resource imports require a module loader that admits them",
        ))
    }
}

#[derive(Debug, Clone)]
pub struct ModuleSource {
    pub path: PathBuf,
    pub text: String,
}
#[derive(Debug, Clone)]
pub struct Compilation {
    pub wasm: Vec<u8>,
    pub modules: Vec<varta_common::ModuleInfo>,
}

/// The exported name of a default export (v32). `export *` never forwards it.
pub(crate) const DEFAULT_EXPORT: &str = "default";

pub(crate) struct ImportBinding {
    pub imported: String,
    pub local: String,
    pub span: Span,
    /// `import type` or `import { type T }`: binds only the type namespace.
    pub type_only: bool,
}
pub(crate) struct Dependency {
    pub target: PathBuf,
    pub bindings: Vec<ImportBinding>,
    pub span: Span,
    /// A text resource (`with { type: "text" }`, v38), not a module.
    pub text: bool,
}
pub(crate) struct Export {
    pub local: String,
    pub exported: String,
    pub span: Span,
    /// Declared through `export interface/type` or `export type { … }`.
    pub type_only: bool,
    /// `export { local as exported } from` (v32): `local` names an export of
    /// this dependency and is not bound in the re-exporting module.
    pub from: Option<PathBuf>,
}
pub(crate) struct Unit {
    pub source: Source,
    pub ast: ast::Module,
    /// Import and re-export dependencies; re-export entries have no bindings.
    pub imports: Vec<Dependency>,
    pub exports: Vec<Export>,
    /// `export * from` sources (v32), in source order.
    pub stars: Vec<PathBuf>,
    /// A text resource (v38): its source is the resource text and its AST
    /// the one constant `default`.
    pub text: bool,
}
pub(crate) struct Graph {
    pub entry: PathBuf,
    pub units: BTreeMap<PathBuf, Unit>,
    pub order: Vec<PathBuf>,
}

struct Builder<'a> {
    loader: &'a dyn ModuleLoader,
    profile: varta_common::language::LanguageProfile,
    graph: Graph,
    visiting: Vec<PathBuf>,
    bytes: usize,
    imports: usize,
}
const MAX_MODULES: usize = 256;
const MAX_IMPORTS: usize = 2048;
const MAX_DEPTH: usize = 32;
const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;

impl Graph {
    #[cfg(test)]
    pub(crate) fn load(
        entry: ModuleSource,
        loader: &dyn ModuleLoader,
    ) -> Result<Self, CompilerError> {
        Self::load_with_profile(entry, loader, Default::default())
    }
    /// Profiles with named types also admit type-only import/export forms;
    /// profiles with module re-exports admit `export … from` and `export *`.
    pub(crate) fn load_with_profile(
        entry: ModuleSource,
        loader: &dyn ModuleLoader,
        profile: varta_common::language::LanguageProfile,
    ) -> Result<Self, CompilerError> {
        let mut builder = Builder {
            loader,
            profile,
            graph: Self {
                entry: entry.path.clone(),
                units: BTreeMap::new(),
                order: Vec::new(),
            },
            visiting: Vec::new(),
            bytes: 0,
            imports: 0,
        };
        builder.visit(entry)?;
        Ok(builder.graph)
    }
    pub(crate) fn metadata(&self) -> Vec<varta_common::ModuleInfo> {
        // Engine preludes are compiler-owned, not program sources.
        self.order
            .iter()
            .filter(|path| !engine::is_prelude(path))
            .map(|path| {
                let unit = &self.units[path];
                varta_common::ModuleInfo {
                    path: path.clone(),
                    imports: unit
                        .imports
                        .iter()
                        .map(|i| i.target.clone())
                        .filter(|target| !engine::is_prelude(target))
                        .collect::<BTreeSet<_>>()
                        .into_iter()
                        .collect(),
                }
            })
            .collect()
    }
}
impl Builder<'_> {
    fn visit(&mut self, input: ModuleSource) -> Result<(), CompilerError> {
        crate::bounded_source(&input.text).map_err(|e| source::limit_error(&input.path, e))?;
        if self.graph.units.len() + self.visiting.len() >= MAX_MODULES
            || self.visiting.len() >= MAX_DEPTH
        {
            return Err(module_error("module graph exceeds module/depth limits"));
        }
        self.bytes += input.text.len();
        if self.bytes > MAX_SOURCE_BYTES {
            return Err(module_error("module graph exceeds total source budget"));
        }
        let source = Source::new(&input.path, &input.text);
        // A `.tsx` module parses JSX only where `tsx()` admits it; earlier
        // profiles keep parsing every module as TypeScript.
        let tsx = self.profile.tsx() && crate::jsx::is_tsx(&input.path);
        let mut ast = parse::parse_module(&source, tsx)?;
        if tsx {
            crate::jsx::desugar(&mut ast, &source)?;
        }
        if self.profile.web_bytes() && !engine::is_prelude(&input.path) {
            global_members::rewrite(&mut ast);
        }
        let syntax::Scan {
            mut imports,
            exports,
            stars,
        } = syntax::scan(&mut ast, &source, &input.path, self.loader, self.profile)?;
        if let Some(import) = imports.iter().find(|i| engine::is_prelude(&i.target)) {
            return Err(source.locate(
                module_error(format!(
                    "module path {} is reserved for an engine prelude",
                    import.target.display()
                )),
                import.span,
            ));
        }
        imports.extend(engine::implicit_imports(
            &input.path,
            &ast,
            &imports,
            self.profile,
        ));
        self.imports += imports.len();
        if self.imports > MAX_IMPORTS {
            return Err(source.locate(
                module_error("module graph exceeds import limit"),
                ast.span(),
            ));
        }
        imports.sort_by(|a, b| a.target.cmp(&b.target));
        self.visiting.push(input.path.clone());
        for import in &imports {
            let visit = (|| {
                if let Some(start) = self.visiting.iter().position(|path| path == &import.target) {
                    let chain = self.visiting[start..]
                        .iter()
                        .chain(std::iter::once(&import.target))
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(" -> ");
                    return Err(module_error(format!("cyclic module dependency: {chain}")));
                }
                if let Some(unit) = self.graph.units.get(&import.target)
                    && unit.text != import.text
                {
                    return Err(module_error(format!(
                        "{} is imported both as a module and as a text resource",
                        import.target.display()
                    )));
                }
                if !self.graph.units.contains_key(&import.target) {
                    if self.graph.units.len() + self.visiting.len() >= MAX_MODULES
                        || self.visiting.len() >= MAX_DEPTH
                    {
                        return Err(module_error("module graph exceeds module/depth limits"));
                    }
                    if import.text {
                        return self.text(&import.target);
                    }
                    let text = match engine::prelude(&import.target) {
                        Some(prelude) => prelude.text(self.profile).to_owned(),
                        None => self.loader.load(&import.target)?,
                    };
                    self.visit(ModuleSource {
                        path: import.target.clone(),
                        text,
                    })?;
                }
                Ok(())
            })();
            visit.map_err(|e| source.locate(e, import.span))?;
        }
        self.visiting.pop();
        text::check_stylesheets(&ast, &imports, &self.graph.units)?;
        self.graph.order.push(input.path.clone());
        self.graph.units.insert(
            input.path,
            Unit {
                source,
                ast,
                imports,
                exports,
                stars,
                text: false,
            },
        );
        Ok(())
    }
    /// Load a text resource once, within the graph's module and byte budgets.
    fn text(&mut self, path: &Path) -> Result<(), CompilerError> {
        let text = self.loader.load(path)?;
        self.bytes += text.len();
        if self.bytes > MAX_SOURCE_BYTES {
            return Err(module_error("module graph exceeds total source budget"));
        }
        let unit = text::unit(path, text)?;
        self.graph.order.push(path.to_owned());
        self.graph.units.insert(path.to_owned(), unit);
        Ok(())
    }
}

pub(crate) fn module_error(message: impl Into<String>) -> CompilerError {
    CompilerError::Module(message.into())
}

pub(crate) struct NoImports;
impl ModuleLoader for NoImports {
    fn resolve(&self, _: &Path, _: &str) -> Result<PathBuf, CompilerError> {
        Err(module_error(
            "named imports require an explicit module loader",
        ))
    }
    fn load(&self, _: &Path) -> Result<String, CompilerError> {
        Err(module_error("module loader is unavailable"))
    }
}
