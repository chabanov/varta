//! Host-authorized source loading. Dependencies remain under the configuration
//! directory and are opened through pinned descriptors, never ambient paths.
//! V38 bare package specifiers resolve to shipped TypeScript sources under
//! `<config dir>/node_modules` ([`packages`]).
use crate::{ProjectError, Result};
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt, OpenOptionsSyncExt};
use cap_std::fs::{Dir, OpenOptions};
use std::{
    io::Read,
    path::{Component, Path, PathBuf},
};
use varta_compiler::{CompilerError, ModuleLoader, ModuleSource};
mod packages;
mod watch;

pub(crate) const MAX_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_SPECIFIER_BYTES: usize = 1024;
const TS_EXTENSION: &str = ".ts";
const TSX_EXTENSION: &str = ".tsx";

/// The TypeScript source extensions a JavaScript specifier may name.
#[derive(Debug, Clone, Copy)]
enum Sources {
    One(&'static str),
    /// Probed in this order; exactly one must exist.
    Either(&'static str, &'static str),
}

pub(crate) struct SourceLoader {
    root: PathBuf,
    directory: Dir,
    /// TypeScript's ESM convention: `./x.js` names the `./x.ts` source (v31+).
    js_specifiers: bool,
    /// V38 (`tsx()`): `./x.tsx` names a TSX module, `./x.js` names `./x.ts`
    /// or `./x.tsx` (both existing is an error) and `./x.jsx` names `./x.tsx`.
    tsx_specifiers: bool,
    /// V38 (`text_imports()`): `with { type: "text" }` imports of any file.
    text_resources: bool,
    /// V38 (`package_specifiers()`): `"pkg"` and `"@scope/pkg/sub"` name the
    /// TypeScript source of a package under `<root>/node_modules`.
    package_specifiers: bool,
    packages: packages::Packages,
    watch: Option<watch::Observations>,
    /// Text resources whose text the build generates (the Tailwind
    /// stylesheet): `load` returns this text instead of the file's.
    generated: std::cell::RefCell<std::collections::BTreeMap<PathBuf, String>>,
}
impl SourceLoader {
    pub(crate) fn new(
        root: &Path,
        profile: varta_common::language::LanguageProfile,
    ) -> Result<Self> {
        let root = root.canonicalize()?;
        let directory = Dir::open_ambient_dir(&root, cap_std::ambient_authority())?;
        Ok(Self {
            root,
            directory,
            js_specifiers: profile.named_types(),
            tsx_specifiers: profile.tsx(),
            text_resources: profile.text_imports(),
            package_specifiers: profile.package_specifiers(),
            packages: packages::Packages::default(),
            watch: None,
            generated: Default::default(),
        })
    }
    /// The identity of a project file named relative to the configuration
    /// directory, normalized like a text resource.
    pub(crate) fn project_file(&self, relative: &str) -> Result<PathBuf> {
        Ok(self.normalize(&self.root.join(relative))?)
    }
    /// Serve `text` as the contents of the text resource `path`.
    pub(crate) fn set_generated(&self, path: PathBuf, text: String) {
        self.generated.borrow_mut().insert(path, text);
    }
    pub(crate) fn entry(&self, path: &Path) -> Result<ModuleSource> {
        // The entry is explicit host configuration, including legacy layouts
        // outside the config directory. All discovered dependencies are confined.
        let text = if path.starts_with(&self.root) {
            self.load(path)?
        } else {
            let bytes = if self.watch.is_some() {
                self.read_external(path, MAX_SOURCE_BYTES)?
            } else {
                crate::artifact::read_bounded(path, MAX_SOURCE_BYTES)?
            };
            String::from_utf8(bytes)
                .map_err(|e| ProjectError::Compile(CompilerError::Module(e.to_string())))?
        };
        Ok(ModuleSource {
            path: path.to_owned(),
            text,
        })
    }
    fn read(&self, path: &Path) -> std::io::Result<String> {
        String::from_utf8(self.read_bytes(path, MAX_SOURCE_BYTES)?)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
    pub(crate) fn read_bytes(&self, path: &Path, maximum: usize) -> std::io::Result<Vec<u8>> {
        let result = self.read_bytes_untracked(path, maximum);
        if let Some(watch) = &self.watch {
            watch.read(path, maximum, false, &result)?;
        }
        result
    }
    fn read_bytes_untracked(&self, path: &Path, maximum: usize) -> std::io::Result<Vec<u8>> {
        let relative = path.strip_prefix(&self.root).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "module is outside project root",
            )
        })?;
        let mut directory = self.directory.try_clone()?;
        let mut parts = relative.components().peekable();
        while let Some(part) = parts.next() {
            let Component::Normal(name) = part else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "invalid module path component",
                ));
            };
            if parts.peek().is_some() {
                directory = directory.open_dir_nofollow(name)?;
            } else {
                let mut options = OpenOptions::new();
                options.read(true).follow(FollowSymlinks::No).nonblock(true);
                let file = directory.open_with(name, &options)?;
                if !file.metadata()?.is_file() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "module must be a regular file",
                    ));
                }
                let mut bytes = Vec::new();
                file.take((maximum + 1) as u64).read_to_end(&mut bytes)?;
                if bytes.len() > maximum {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "project file exceeds byte limit",
                    ));
                }
                return Ok(bytes);
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "module filename is missing",
        ))
    }
    /// The TypeScript sources a JavaScript specifier names, in precedence
    /// order: `.js` names `.ts` (v31+) and then `.tsx` (v38); `.jsx` names
    /// `.tsx` (v38), as TypeScript's extension substitution does.
    fn sources<'a>(&self, specifier: &'a str) -> Option<(&'a str, Sources)> {
        if let Some(stem) = specifier.strip_suffix(".jsx") {
            return Some((stem, Sources::One(TSX_EXTENSION)));
        }
        let stem = specifier.strip_suffix(".js")?;
        Some((
            stem,
            if self.tsx_specifiers {
                Sources::Either(TS_EXTENSION, TSX_EXTENSION)
            } else {
                Sources::One(TS_EXTENSION)
            },
        ))
    }
    /// The one existing source of a `.js` specifier with two candidates; none
    /// or both existing is a module error at the import site.
    fn pick(
        &self,
        specifier: &str,
        first: PathBuf,
        second: PathBuf,
    ) -> std::result::Result<PathBuf, CompilerError> {
        let exists = |candidate: &Path| {
            self.exists(candidate).map_err(|e| {
                CompilerError::Module(format!(
                    "cannot resolve module {}: {e}",
                    candidate.display()
                ))
            })
        };
        match (exists(&first)?, exists(&second)?) {
            (true, false) => Ok(first),
            (false, true) => Ok(second),
            (false, false) => Err(CompilerError::Module(format!(
                "cannot resolve module {specifier}: neither {} nor {} exists",
                first.display(),
                second.display()
            ))),
            (true, true) => Err(CompilerError::Module(format!(
                "ambiguous import {specifier}: both {} and {} exist; rename one or import it with its .ts or .tsx extension",
                first.display(),
                second.display()
            ))),
        }
    }
    /// Lexical dot-segment normalization, confined to the project root.
    fn normalize(&self, joined: &Path) -> std::result::Result<PathBuf, CompilerError> {
        let mut normalized = PathBuf::new();
        for component in joined.components() {
            match component {
                Component::CurDir => {}
                Component::ParentDir => {
                    if !normalized.pop() {
                        return Err(CompilerError::Module(
                            "module path escapes project root".into(),
                        ));
                    }
                }
                component => normalized.push(component.as_os_str()),
            }
        }
        if !normalized.starts_with(&self.root) {
            return Err(CompilerError::Module(
                "module path escapes project root".into(),
            ));
        }
        Ok(normalized)
    }
    /// Whether a directory entry named by `path` exists, walked through the
    /// pinned root with no-follow at every component like `read`. A symlink
    /// counts as existing (loading it is then rejected); a missing directory
    /// on the way means the entry does not exist.
    fn exists(&self, path: &Path) -> std::io::Result<bool> {
        let result = self.exists_untracked(path);
        if let Some(watch) = &self.watch {
            watch.probe(path, &result)?;
        }
        result
    }
    fn exists_untracked(&self, path: &Path) -> std::io::Result<bool> {
        let not_found = |e: std::io::Error| match e.kind() {
            std::io::ErrorKind::NotFound => Ok(false),
            _ => Err(e),
        };
        let relative = path.strip_prefix(&self.root).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "module is outside project root",
            )
        })?;
        let mut directory = self.directory.try_clone()?;
        let mut parts = relative.components().peekable();
        while let Some(part) = parts.next() {
            let Component::Normal(name) = part else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "invalid module path component",
                ));
            };
            if parts.peek().is_none() {
                return directory
                    .symlink_metadata(name)
                    .map(|_| true)
                    .or_else(not_found);
            }
            directory = match directory.open_dir_nofollow(name) {
                Ok(next) => next,
                Err(e) => return not_found(e),
            };
        }
        Ok(false)
    }
}
/// Project stylesheets imported by the Tailwind entry (K8): resolved like
/// text resources (relative, no URL syntax, inside the configuration
/// directory) and read through the pinned root with no-follow at every
/// component, bounded and UTF-8, never the generated stylesheet.
impl varta_tailwind::StylesheetLoader for SourceLoader {
    fn resolve(
        &self,
        importer: &Path,
        specifier: &str,
    ) -> std::result::Result<PathBuf, varta_tailwind::LoadError> {
        ModuleLoader::resolve_text(self, importer, specifier)
            .map_err(|e| varta_tailwind::LoadError(e.to_string()))
    }
    fn load(&self, path: &Path) -> std::result::Result<String, varta_tailwind::LoadError> {
        self.read(path)
            .map_err(|e| varta_tailwind::LoadError(format!("cannot load {}: {e}", path.display())))
    }
}
impl ModuleLoader for SourceLoader {
    fn resolve(
        &self,
        importer: &Path,
        specifier: &str,
    ) -> std::result::Result<PathBuf, CompilerError> {
        if specifier.len() <= MAX_SPECIFIER_BYTES {
            match packages::Specifier::parse(specifier) {
                Some(Ok(package)) if self.package_specifiers => {
                    return Ok(self.resolve_package(specifier, &package)?);
                }
                Some(Err(error)) if self.package_specifiers => return Err(error.into()),
                // Earlier profiles name the profile that admits packages.
                Some(Ok(_)) => {
                    return Err(packages::PackageError::Profile {
                        specifier: specifier.to_owned(),
                        profile: packages::required_profile(),
                    }
                    .into());
                }
                Some(Err(_)) | None => {}
            }
        }
        if specifier.len() > MAX_SPECIFIER_BYTES
            || !(specifier.starts_with("./") || specifier.starts_with("../"))
            || specifier.chars().any(|c| c.is_control())
            || specifier.contains(['\\', '?', '#'])
            || !(specifier.ends_with(".ts")
                || self.js_specifiers && specifier.ends_with(".js")
                || self.tsx_specifiers
                    && (specifier.ends_with(".tsx") || specifier.ends_with(".jsx")))
        {
            return Err(CompilerError::Module(
                if self.package_specifiers {
                    "imports require explicit relative .ts, .tsx, .js or .jsx paths or package names without URL/query syntax; import another file as text with `with { type: \"text\" }`"
                } else if self.tsx_specifiers {
                    "imports require explicit relative .ts, .tsx, .js or .jsx paths without URL/query syntax; import another file as text with `with { type: \"text\" }`"
                } else if self.js_specifiers {
                    "imports require explicit relative .ts or .js paths without URL/query syntax"
                } else {
                    "imports require explicit relative .ts paths without URL/query syntax"
                }
                .into(),
            ));
        }
        let parent = importer
            .parent()
            .ok_or_else(|| CompilerError::Module("importer directory is missing".into()))?;
        // Only a source file is ever loaded; a `.js` or `.jsx` specifier never
        // reads a JavaScript file.
        let Some((stem, sources)) = self.sources(specifier) else {
            return self.normalize(&parent.join(specifier));
        };
        let candidate =
            |extension: &str| self.normalize(&parent.join(format!("{stem}{extension}")));
        match sources {
            Sources::One(extension) => candidate(extension),
            Sources::Either(first, second) => {
                self.pick(specifier, candidate(first)?, candidate(second)?)
            }
        }
    }
    /// A text resource resolves like a module specifier, without the
    /// extension rule: relative, without URL/query syntax, normalized
    /// lexically inside the project root; it is then loaded with `load`
    /// through the pinned no-follow root like every module.
    fn resolve_text(
        &self,
        importer: &Path,
        specifier: &str,
    ) -> std::result::Result<PathBuf, CompilerError> {
        if !self.text_resources {
            return Err(CompilerError::Module(
                "text resource imports require profile checked-f64-utf16-v38".into(),
            ));
        }
        if specifier.len() > MAX_SPECIFIER_BYTES
            || !(specifier.starts_with("./") || specifier.starts_with("../"))
            || specifier.chars().any(|c| c.is_control())
            || specifier.contains(['\\', '?', '#'])
            || specifier.ends_with('/')
        {
            return Err(CompilerError::Module(
                "text resource imports require explicit relative file paths without URL/query syntax"
                    .into(),
            ));
        }
        let parent = importer
            .parent()
            .ok_or_else(|| CompilerError::Module("importer directory is missing".into()))?;
        self.normalize(&parent.join(specifier))
    }
    fn load(&self, path: &Path) -> std::result::Result<String, CompilerError> {
        if let Some(text) = self.generated.borrow().get(path) {
            return Ok(text.clone());
        }
        self.read(path).map_err(|e| {
            CompilerError::Module(format!("cannot load module {}: {e}", path.display()))
        })
    }
}
