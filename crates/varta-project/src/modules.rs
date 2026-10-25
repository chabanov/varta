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

pub(crate) struct SourceLoader {
    root: PathBuf,
    directory: Dir,
}
impl SourceLoader {
    pub(crate) fn new(root: &Path) -> Result<Self> {
        let root = root.canonicalize()?;
        let directory = Dir::open_ambient_dir(&root, cap_std::ambient_authority())?;
        Ok(Self { root, directory })
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
            || !specifier.ends_with(".ts")
        {
            return Err(CompilerError::Module(
                "imports require explicit relative .ts paths without URL/query syntax".into(),
            ));
        }
        let parent = importer
            .parent()
            .ok_or_else(|| CompilerError::Module("importer directory is missing".into()))?;
        let joined = parent.join(specifier);
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
    fn load(&self, path: &Path) -> std::result::Result<String, CompilerError> {
        self.read(path).map_err(|e| {
            CompilerError::Module(format!("cannot load module {}: {e}", path.display()))
        })
    }
}
