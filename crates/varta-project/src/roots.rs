//! Pin the configuration's v38 file roots (`file_roots`) for `varta dev`.
//! Each root directory stays inside the configuration directory and is
//! opened through pinned descriptors with no-follow at every component, the
//! way the source loader opens modules; never through an ambient path.
use crate::{Result, VartaConfig};
use cap_fs_ext::DirExt;
use cap_std::fs::Dir;
use std::path::Path;
use varta_common::files::{FileRootError, ProjectRoots, RootDirectory};
use varta_runtime::FileRoots;

/// Open every configured root below `base`, the configuration directory.
pub(crate) fn pin(base: &Path, config: &VartaConfig) -> Result<FileRoots> {
    let mut roots = FileRoots::new();
    if config.roots().is_empty() {
        return Ok(roots);
    }
    let base = base.canonicalize()?;
    let directory = Dir::open_ambient_dir(&base, cap_std::ambient_authority())?;
    for root in config.roots() {
        // The host path lets database files (v38 `kv`) be opened under the
        // root; the runtime verifies it against the pinned descriptor.
        let path = ProjectRoots::components(&root.directory)
            .into_iter()
            .fold(base.clone(), |path, component| path.join(component));
        roots.insert_at(&root.name, open(&directory, root)?.into_std_file(), &path)?;
    }
    Ok(roots)
}

fn open(base: &Dir, root: &RootDirectory) -> std::result::Result<Dir, FileRootError> {
    let failure = |error: std::io::Error| FileRootError::Open {
        name: root.name.clone(),
        directory: root.directory.clone(),
        message: error.to_string(),
    };
    let mut directory = base.try_clone().map_err(failure)?;
    for component in ProjectRoots::components(&root.directory) {
        directory = directory.open_dir_nofollow(component).map_err(failure)?;
    }
    Ok(directory)
}
