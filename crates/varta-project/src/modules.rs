//! Host-authorized source loading. Dependencies remain under the configuration
//! directory and are opened through pinned descriptors, never ambient paths.
use crate::{ProjectError, Result};
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt, OpenOptionsSyncExt};
use cap_std::fs::{Dir, OpenOptions};
use std::{
    io::Read,
    path::{Component, Path, PathBuf},
};
use varta_compiler::{CompilerError, ModuleLoader, ModuleSource};

const MAX_SOURCE_BYTES: usize = 1024 * 1024;
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
        })
    }
    pub(crate) fn entry(&self, path: &Path) -> Result<ModuleSource> {
        // The entry is explicit host configuration, including legacy layouts
        // outside the config directory. All discovered dependencies are confined.
        let text = if path.starts_with(&self.root) {
            self.load(path)?
        } else {
            let bytes = crate::artifact::read_bounded(path, MAX_SOURCE_BYTES)?;
            String::from_utf8(bytes)
                .map_err(|e| ProjectError::Compile(CompilerError::Module(e.to_string())))?
        };
        Ok(ModuleSource {
            path: path.to_owned(),
            text,
        })
    }
    fn read(&self, path: &Path) -> std::io::Result<String> {
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
                file.take((MAX_SOURCE_BYTES + 1) as u64)
                    .read_to_end(&mut bytes)?;
                if bytes.len() > MAX_SOURCE_BYTES {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "module exceeds 1 MiB",
                    ));
                }
                return String::from_utf8(bytes)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e));
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
impl ModuleLoader for SourceLoader {
    fn resolve(
        &self,
        importer: &Path,
        specifier: &str,
    ) -> std::result::Result<PathBuf, CompilerError> {
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
                if self.tsx_specifiers {
                    "imports require explicit relative .ts, .tsx, .js or .jsx paths without URL/query syntax"
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
    fn load(&self, path: &Path) -> std::result::Result<String, CompilerError> {
        self.read(path).map_err(|e| {
            CompilerError::Module(format!("cannot load module {}: {e}", path.display()))
        })
    }
}
