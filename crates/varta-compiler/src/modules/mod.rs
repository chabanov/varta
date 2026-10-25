//! A bounded, deterministic graph of explicit source modules. No source text is
//! concatenated and the compiler never performs ambient filesystem/network I/O.
mod syntax;
#[cfg(test)]
mod tests;
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

pub(crate) struct ImportBinding {
    pub imported: String,
    pub local: String,
    pub span: Span,
}
pub(crate) struct Dependency {
    pub target: PathBuf,
    pub bindings: Vec<ImportBinding>,
    pub span: Span,
}
pub(crate) struct Export {
    pub local: String,
    pub exported: String,
    pub span: Span,
}
pub(crate) struct Unit {
    pub source: Source,
    pub ast: ast::Module,
    pub imports: Vec<Dependency>,
    pub exports: Vec<Export>,
}
pub(crate) struct Graph {
    pub entry: PathBuf,
    pub units: BTreeMap<PathBuf, Unit>,
    pub order: Vec<PathBuf>,
}

struct Builder<'a> {
    loader: &'a dyn ModuleLoader,
    graph: Graph,
    visiting: Vec<PathBuf>,
    bytes: usize,
    imports: usize,
}
const MAX_MODULES: usize = 64;
const MAX_IMPORTS: usize = 256;
const MAX_DEPTH: usize = 32;
const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;

impl Graph {
    pub(crate) fn load(
        entry: ModuleSource,
        loader: &dyn ModuleLoader,
    ) -> Result<Self, CompilerError> {
        let mut builder = Builder {
            loader,
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
        self.order
            .iter()
            .map(|path| {
                let unit = &self.units[path];
                varta_common::ModuleInfo {
                    path: path.clone(),
                    imports: unit
                        .imports
                        .iter()
                        .map(|i| i.target.clone())
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
        let ast = parse::parse_module(&source)?;
        let (mut imports, exports) = syntax::scan(&ast, &source, &input.path, self.loader)?;
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
                if !self.graph.units.contains_key(&import.target) {
                    if self.graph.units.len() + self.visiting.len() >= MAX_MODULES
                        || self.visiting.len() >= MAX_DEPTH
                    {
                        return Err(module_error("module graph exceeds module/depth limits"));
                    }
                    self.visit(ModuleSource {
                        path: import.target.clone(),
                        text: self.loader.load(&import.target)?,
                    })?;
                }
                Ok(())
            })();
            visit.map_err(|e| source.locate(e, import.span))?;
        }
        self.visiting.pop();
        self.graph.order.push(input.path.clone());
        self.graph.units.insert(
            input.path,
            Unit {
                source,
                ast,
                imports,
                exports,
            },
        );
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
