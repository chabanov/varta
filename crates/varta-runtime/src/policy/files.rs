use super::glob::glob;
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt, OpenOptionsSyncExt};
use cap_std::fs::{Dir, OpenOptions};
use std::{
    io::Read,
    path::{Component, Path, PathBuf},
};
use varta_common::{CapabilityGrant, CapabilityValue, abi::MAX_RESULT_BYTES};

pub(super) struct FilePolicy {
    requested: Vec<CapabilityGrant>,
    authority: Vec<FileRule>,
}
struct FileRule {
    operation: String,
    root: PathBuf,
    directory: Dir,
    pattern: String,
}

fn scope(pattern: &str) -> Option<(PathBuf, &str)> {
    let boundary = pattern.find(['*', '?']).unwrap_or(pattern.len());
    let prefix = &pattern[..boundary];
    let slash = prefix.rfind('/')?;
    Some((
        PathBuf::from(if slash == 0 { "/" } else { &prefix[..slash] }),
        &pattern[slash + 1..],
    ))
}

fn matches(root: &Path, pattern: &str, path: &Path) -> bool {
    path.strip_prefix(root)
        .ok()
        .and_then(Path::to_str)
        .is_some_and(|text| glob(pattern, text, true))
}

impl FilePolicy {
    pub(super) fn new(requested: &[CapabilityGrant], authority: &[CapabilityGrant]) -> Self {
        let mut rules = Vec::new();
        for grant in authority
            .iter()
            .filter(|grant| matches!(grant.id.as_str(), "fs.read" | "fs.print"))
        {
            // A ceiling is never an additional grant. Do not acquire filesystem
            // authority for operations that the package did not request.
            if !requested.iter().any(|g| {
                g.id == grant.id
                    && matches!(&g.value, CapabilityValue::PathGlobs(p) if !p.is_empty())
            }) {
                continue;
            }
            let CapabilityValue::PathGlobs(patterns) = &grant.value else {
                continue;
            };
            for pattern in patterns {
                let Some((root, pattern)) = scope(pattern) else {
                    continue;
                };
                // Pin the HOST's root when constrained, not a root supplied by the
                // untrusted package. All subsequent opens stay under this descriptor.
                if let Ok(directory) = Dir::open_ambient_dir(&root, cap_std::ambient_authority()) {
                    rules.push(FileRule {
                        operation: grant.id.clone(),
                        root,
                        directory,
                        pattern: pattern.into(),
                    });
                }
            }
        }
        Self {
            requested: requested.to_vec(),
            authority: rules,
        }
    }

    /// Whether the package's lexical scopes and the pinned authority both
    /// cover `input`, without opening anything (v38 permission states).
    pub(super) fn covers(&self, id: &str, input: &str) -> bool {
        let path = Path::new(input);
        if input.len() > 4096
            || !path.is_absolute()
            || path.components().any(|c| matches!(c, Component::ParentDir))
        {
            return false;
        }
        let requested = self.requested.iter().filter(|g| g.id == id).any(|g| {
            matches!(&g.value, CapabilityValue::PathGlobs(patterns) if patterns.iter().any(|pattern| {
                scope(pattern).is_some_and(|(root, pattern)| matches(&root, pattern, path))
            }))
        });
        requested
            && self
                .authority
                .iter()
                .any(|rule| rule.operation == id && matches(&rule.root, &rule.pattern, path))
    }

    pub(super) fn read(&self, id: &str, input: &str) -> Result<Vec<u8>, crate::HostFailure> {
        let path = Path::new(input);
        if input.len() > 4096
            || !path.is_absolute()
            || path.components().any(|c| matches!(c, Component::ParentDir))
        {
            return Err(crate::HostFailure::denied(id, input));
        }
        let requested = self.requested.iter().filter(|g| g.id == id).any(|g| {
            matches!(&g.value, CapabilityValue::PathGlobs(patterns) if patterns.iter().any(|pattern| {
                scope(pattern).is_some_and(|(root, pattern)| matches(&root, pattern, path))
            }))
        });
        if !requested {
            return Err(crate::HostFailure::denied(id, input));
        }
        for rule in self.authority.iter().filter(|rule| rule.operation == id) {
            if !matches(&rule.root, &rule.pattern, path) {
                continue;
            }
            let relative = path
                .strip_prefix(&rule.root)
                .map_err(crate::HostFailure::io)?;
            // Walk pinned descriptors with no-follow at every component. A package
            // root alias must never bypass the host's directory authority.
            let mut directory = rule.directory.try_clone().map_err(crate::HostFailure::io)?;
            let mut parts = relative.components().peekable();
            while let Some(part) = parts.next() {
                let Component::Normal(name) = part else {
                    return Err(crate::HostFailure::denied(id, input));
                };
                if parts.peek().is_some() {
                    directory = directory
                        .open_dir_nofollow(name)
                        .map_err(crate::HostFailure::io)?;
                } else {
                    let mut options = OpenOptions::new();
                    options.read(true).follow(FollowSymlinks::No).nonblock(true);
                    let file = directory
                        .open_with(name, &options)
                        .map_err(crate::HostFailure::io)?;
                    if !file.metadata().map_err(crate::HostFailure::io)?.is_file() {
                        return Err(crate::HostFailure::io("only regular files are readable"));
                    }
                    let mut result = Vec::new();
                    file.take((MAX_RESULT_BYTES + 1) as u64)
                        .read_to_end(&mut result)
                        .map_err(crate::HostFailure::io)?;
                    if result.len() > MAX_RESULT_BYTES {
                        return Err(crate::HostFailure::too_large());
                    }
                    return Ok(result);
                }
            }
        }
        Err(crate::HostFailure::denied(id, input))
    }
}
