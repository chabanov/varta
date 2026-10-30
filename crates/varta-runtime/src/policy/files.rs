use super::glob::glob;
use cap_fs_ext::{DirExt, FollowSymlinks, OpenOptionsFollowExt, OpenOptionsSyncExt};
use cap_std::fs::{Dir, OpenOptions};
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    io::Read,
    path::{Component, Path, PathBuf},
    sync::Arc,
};
use varta_common::files::{self, FileRootError};
use varta_common::{CapabilityGrant, CapabilityValue, Manifest, abi::MAX_RESULT_BYTES};

/// The virtual filesystem roots of a v38 execution (`file_roots`): each root
/// name with the directory descriptor the host pinned for it once, before
/// any guest code runs. Every read walks from that descriptor with no-follow
/// at every component, so neither a package pattern nor a guest path can
/// acquire authority outside the pinned directory. Earlier profiles ignore
/// these roots and keep absolute host paths.
#[derive(Clone, Default)]
pub struct FileRoots {
    roots: BTreeMap<String, Arc<Dir>>,
    /// The host directory each root was pinned from, when the host named
    /// it: a database file (v38 `kv`) is opened by SQLite through a path,
    /// which is verified against these descriptors (`kv::store`).
    directories: BTreeMap<String, PathBuf>,
}

impl std::fmt::Debug for FileRoots {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_set().entries(self.roots.keys()).finish()
    }
}

impl FileRoots {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pin the trusted absolute host directory `directory` (a host policy
    /// mapping) as root `name`. The host's own path is opened once, here.
    pub fn open(&mut self, name: &str, directory: &Path) -> Result<(), FileRootError> {
        self.check(name)?;
        let failure = |message: String| FileRootError::Open {
            name: name.into(),
            directory: directory.display().to_string(),
            message,
        };
        if !directory.is_absolute() {
            return Err(failure("not an absolute path".into()));
        }
        let pinned = Dir::open_ambient_dir(directory, cap_std::ambient_authority())
            .map_err(|e| failure(e.to_string()))?;
        self.roots.insert(name.into(), Arc::new(pinned));
        self.directories
            .insert(name.into(), directory.to_path_buf());
        Ok(())
    }

    /// [`Self::insert`] for a host that also names the absolute directory it
    /// opened, so database files (v38 `kv`) can be opened under the root.
    pub fn insert_at(
        &mut self,
        name: &str,
        directory: std::fs::File,
        path: &Path,
    ) -> Result<(), FileRootError> {
        if !path.is_absolute() {
            return Err(FileRootError::Open {
                name: name.into(),
                directory: path.display().to_string(),
                message: "not an absolute path".into(),
            });
        }
        self.insert(name, directory)?;
        self.directories.insert(name.into(), path.to_path_buf());
        Ok(())
    }

    /// The pinned descriptor and host directory of root `name`.
    pub(crate) fn pinned(&self, name: &str) -> Option<(&Arc<Dir>, Option<&Path>)> {
        let root = self.roots.get(name)?;
        Some((root, self.directories.get(name).map(PathBuf::as_path)))
    }

    /// Pin an already opened directory as root `name`, for a host that
    /// opened it itself (the project loader opens configuration roots
    /// no-follow at every component below the configuration directory).
    pub fn insert(&mut self, name: &str, directory: std::fs::File) -> Result<(), FileRootError> {
        self.check(name)?;
        let metadata = directory.metadata().map_err(|e| FileRootError::Open {
            name: name.into(),
            directory: String::new(),
            message: e.to_string(),
        })?;
        if !metadata.is_dir() {
            return Err(FileRootError::NotDirectory(name.into()));
        }
        self.roots
            .insert(name.into(), Arc::new(Dir::from_std_file(directory)));
        Ok(())
    }

    /// The roots of a package run under a host policy: every root the
    /// package declares (`fsRoots` names) that the host maps to a directory
    /// (`fsRoots` object), pinned now. A mapping for a root the package does
    /// not declare is ignored, and a declared root the host does not map
    /// stays absent, so every path under it is denied.
    pub fn from_manifests(
        package: &Manifest,
        host: Option<&Manifest>,
    ) -> Result<Self, FileRootError> {
        let mut roots = Self::new();
        let Some(host) = host else {
            return Ok(roots);
        };
        for name in package.root_names() {
            if let Some(mapping) = host
                .root_directories()
                .iter()
                .find(|mapping| &mapping.name == name)
            {
                roots.open(name, Path::new(&mapping.directory))?;
            }
        }
        Ok(roots)
    }

    /// The pinned root names.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.roots.keys().map(String::as_str)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    fn check(&self, name: &str) -> Result<(), FileRootError> {
        if !files::valid_root_name(name) {
            return Err(FileRootError::InvalidName(name.into()));
        }
        if self.roots.contains_key(name) {
            return Err(FileRootError::Duplicate(name.into()));
        }
        if self.roots.len() == files::MAX_FILE_ROOTS {
            return Err(FileRootError::TooMany);
        }
        Ok(())
    }
}

/// How guest `fs.*` paths are interpreted, chosen by the module's profile.
#[derive(Clone)]
pub(crate) enum FileMode {
    /// V1–v37: absolute host paths under pinned granted directories.
    HostPaths,
    /// V38 (`file_roots`): virtual paths under these host-pinned roots.
    Virtual(FileRoots),
}

impl FileMode {
    pub(crate) fn of(profile: varta_common::language::LanguageProfile, roots: &FileRoots) -> Self {
        if profile.file_roots() {
            Self::Virtual(roots.clone())
        } else {
            Self::HostPaths
        }
    }
}

pub(super) struct FilePolicy {
    requested: Vec<CapabilityGrant>,
    authority: Authority,
}

enum Authority {
    /// The frozen v1–v37 rules: one pinned directory per granted scope root.
    Host(Vec<FileRule>),
    /// V38: the host-pinned roots and the host ceiling's virtual patterns
    /// (`None`: the package's requests alone).
    Virtual {
        roots: FileRoots,
        ceiling: Option<Vec<CapabilityGrant>>,
    },
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

/// A virtual pattern matches a virtual path: its literal directory prefix
/// is a whole-component prefix of the path (`/project/**` never matches
/// `/project-x/a`), and the rest matches the remainder with the
/// component-aware glob. Both are `/`-separated strings on every host.
fn virtual_matches(pattern: &str, path: &str) -> bool {
    let boundary = pattern.find(['*', '?']).unwrap_or(pattern.len());
    let Some(slash) = pattern[..boundary].rfind(files::SEPARATOR) else {
        return false;
    };
    path.strip_prefix(&pattern[..slash])
        .and_then(|tail| tail.strip_prefix(files::SEPARATOR))
        .is_some_and(|tail| glob(&pattern[slash + 1..], tail, true))
}

/// Some `id` grant of `grants` has a pattern matching virtual `path`.
fn virtual_granted(grants: &[CapabilityGrant], id: &str, path: &str) -> bool {
    grants.iter().filter(|g| g.id == id).any(|g| {
        matches!(&g.value, CapabilityValue::PathGlobs(patterns)
            if patterns.iter().any(|pattern| virtual_matches(pattern, path)))
    })
}

/// A granted database path (v38 `kv`): the pinned root, the host directory
/// it was pinned from (when the host named one) and the path components
/// below it.
pub(crate) struct DatabaseTarget {
    pub root: Arc<Dir>,
    pub directory: Option<PathBuf>,
    pub components: Vec<String>,
}

/// Read the regular file `parts` below `directory`, opening every directory
/// and the file no-follow, nonblocking, capped before allocation.
fn read_below(directory: &Dir, parts: &[&OsStr]) -> Result<Vec<u8>, crate::HostFailure> {
    let Some((name, directories)) = parts.split_last() else {
        return Err(crate::HostFailure::io("only regular files are readable"));
    };
    let mut directory = directory.try_clone().map_err(crate::HostFailure::io)?;
    for part in directories {
        directory = directory
            .open_dir_nofollow(part)
            .map_err(crate::HostFailure::io)?;
    }
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
    Ok(result)
}

impl FilePolicy {
    pub(super) fn new(
        requested: &[CapabilityGrant],
        ceiling: Option<&[CapabilityGrant]>,
        mode: &FileMode,
    ) -> Self {
        let authority = match mode {
            FileMode::HostPaths => {
                Authority::Host(Self::host_rules(requested, ceiling.unwrap_or(requested)))
            }
            FileMode::Virtual(roots) => Authority::Virtual {
                roots: roots.clone(),
                ceiling: ceiling.map(<[CapabilityGrant]>::to_vec),
            },
        };
        Self {
            requested: requested.to_vec(),
            authority,
        }
    }

    fn host_rules(requested: &[CapabilityGrant], authority: &[CapabilityGrant]) -> Vec<FileRule> {
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
        rules
    }

    /// The pinned root and the components of a v38 path that the package
    /// request and the ceiling both cover; `None` denies.
    fn virtual_target<'a>(
        &'a self,
        roots: &'a FileRoots,
        ceiling: Option<&[CapabilityGrant]>,
        id: &str,
        input: &'a str,
    ) -> Option<(&'a Dir, files::VirtualPath<'a>)> {
        let path = files::parse_virtual_path(input)?;
        let granted = virtual_granted(&self.requested, id, input)
            && ceiling.is_none_or(|ceiling| virtual_granted(ceiling, id, input));
        // Checked before any effect; an unmapped root denies.
        let root = roots.roots.get(path.root).filter(|_| granted)?;
        Some((root, path))
    }

    /// The pinned root (descriptor and host directory) and the components of
    /// a database path (v38 `kv`) that grants `id` of the request and the
    /// ceiling both cover; `None` denies, before anything is opened.
    pub(super) fn database_target(&self, id: &str, input: &str) -> Option<DatabaseTarget> {
        let Authority::Virtual { roots, ceiling } = &self.authority else {
            return None;
        };
        let (_, path) = self.virtual_target(roots, ceiling.as_deref(), id, input)?;
        let (root, directory) = roots.pinned(path.root)?;
        Some(DatabaseTarget {
            root: Arc::clone(root),
            directory: directory.map(Path::to_path_buf),
            components: path.components.iter().map(|c| (*c).to_owned()).collect(),
        })
    }

    /// Whether the package's lexical scopes and the pinned authority both
    /// cover `input`, without opening anything (v38 permission states).
    pub(super) fn covers(&self, id: &str, input: &str) -> bool {
        let rules = match &self.authority {
            Authority::Virtual { roots, ceiling } => {
                return self
                    .virtual_target(roots, ceiling.as_deref(), id, input)
                    .is_some();
            }
            Authority::Host(rules) => rules,
        };
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
            && rules
                .iter()
                .any(|rule| rule.operation == id && matches(&rule.root, &rule.pattern, path))
    }

    pub(super) fn read(&self, id: &str, input: &str) -> Result<Vec<u8>, crate::HostFailure> {
        let rules = match &self.authority {
            Authority::Virtual { roots, ceiling } => {
                let (root, path) = self
                    .virtual_target(roots, ceiling.as_deref(), id, input)
                    .ok_or_else(|| crate::HostFailure::denied(id, input))?;
                let parts: Vec<&OsStr> = path.components.iter().map(OsStr::new).collect();
                return read_below(root, &parts);
            }
            Authority::Host(rules) => rules,
        };
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
        for rule in rules.iter().filter(|rule| rule.operation == id) {
            if !matches(&rule.root, &rule.pattern, path) {
                continue;
            }
            let relative = path
                .strip_prefix(&rule.root)
                .map_err(crate::HostFailure::io)?;
            // Walk pinned descriptors with no-follow at every component. A package
            // root alias must never bypass the host's directory authority.
            let mut parts = Vec::new();
            for part in relative.components() {
                let Component::Normal(name) = part else {
                    return Err(crate::HostFailure::denied(id, input));
                };
                parts.push(name);
            }
            if parts.is_empty() {
                continue;
            }
            return read_below(&rule.directory, &parts);
        }
        Err(crate::HostFailure::denied(id, input))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grant(id: &str, patterns: &[&str]) -> CapabilityGrant {
        CapabilityGrant {
            id: id.into(),
            value: CapabilityValue::PathGlobs(patterns.iter().map(|p| (*p).into()).collect()),
        }
    }

    struct Tree(PathBuf);
    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    /// A temporary host tree: `root/fixtures/x.json`, `root/secret.txt`
    /// and a sibling `outside/secret.txt`.
    fn tree() -> Tree {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let base = std::env::temp_dir().join(format!(
            "varta-file-roots-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(base.join("root/fixtures")).unwrap();
        std::fs::create_dir_all(base.join("outside")).unwrap();
        std::fs::write(base.join("root/fixtures/x.json"), "FIXTURE").unwrap();
        std::fs::write(base.join("root/secret.txt"), "ROOT_SECRET").unwrap();
        std::fs::write(base.join("outside/secret.txt"), "OUTSIDE").unwrap();
        Tree(base)
    }
    fn roots(tree: &Tree) -> FileRoots {
        let mut roots = FileRoots::new();
        roots.open("project", &tree.0.join("root")).unwrap();
        roots
    }
    fn policy(
        requested: &[CapabilityGrant],
        ceiling: Option<&[CapabilityGrant]>,
        roots: &FileRoots,
    ) -> FilePolicy {
        FilePolicy::new(requested, ceiling, &FileMode::Virtual(roots.clone()))
    }

    #[test]
    fn virtual_patterns_match_whole_components() {
        assert!(virtual_matches("/project/**", "/project/a/b.json"));
        assert!(virtual_matches("/project/*.json", "/project/a.json"));
        assert!(!virtual_matches("/project/*.json", "/project/a/b.json"));
        assert!(virtual_matches(
            "/project/fixtures/x.json",
            "/project/fixtures/x.json"
        ));
        assert!(!virtual_matches("/project/**", "/project-x/a"));
        assert!(!virtual_matches("/project/**", "/project"));
        assert!(!virtual_matches("/project/fix*/x", "/project/other/x"));
        assert!(virtual_matches("/project/fix*/x", "/project/fixtures/x"));
        assert!(!virtual_matches("project/**", "/project/a"));
    }

    #[test]
    fn virtual_reads_need_the_request_the_ceiling_and_a_mapped_root() {
        let tree = tree();
        let roots = roots(&tree);
        let requested = [grant("fs.read", &["/project/fixtures/**"])];
        let open = policy(&requested, None, &roots);
        assert_eq!(
            open.read("fs.read", "/project/fixtures/x.json").unwrap(),
            b"FIXTURE"
        );
        assert!(open.covers("fs.read", "/project/fixtures/x.json"));
        for denied in [
            "/project/secret.txt",
            "/project-x/fixtures/x.json",
            "/project/fixtures/../secret.txt",
            "/project/fixtures/./x.json",
            "/project//fixtures/x.json",
            "project/fixtures/x.json",
            "/other/fixtures/x.json",
        ] {
            assert!(open.read("fs.read", denied).is_err(), "{denied}");
            assert!(!open.covers("fs.read", denied), "{denied}");
        }
        // The absolute host path of the same file is not a virtual path.
        let host = tree.0.join("root/fixtures/x.json").display().to_string();
        assert!(open.read("fs.read", &host).is_err());
        // Operations are exact.
        assert!(open.read("fs.print", "/project/fixtures/x.json").is_err());
        // A narrower ceiling denies; an empty one denies everything.
        let ceiling = [grant("fs.read", &["/project/fixtures/y.json"])];
        let narrow = policy(&requested, Some(&ceiling), &roots);
        assert!(narrow.read("fs.read", "/project/fixtures/x.json").is_err());
        assert!(
            policy(&requested, Some(&[]), &roots)
                .read("fs.read", "/project/fixtures/x.json")
                .is_err()
        );
        // A wider ceiling cannot widen the request.
        let wide = [grant("fs.read", &["/project/**"])];
        let both = policy(&requested, Some(&wide), &roots);
        assert!(both.read("fs.read", "/project/fixtures/x.json").is_ok());
        assert!(both.read("fs.read", "/project/secret.txt").is_err());
        assert!(
            policy(&[], Some(&wide), &roots)
                .read("fs.read", "/project/secret.txt")
                .is_err()
        );
        // An unmapped root denies, whatever the grants say.
        let unmapped = policy(&requested, None, &FileRoots::new());
        assert!(
            unmapped
                .read("fs.read", "/project/fixtures/x.json")
                .is_err()
        );
        assert!(!unmapped.covers("fs.read", "/project/fixtures/x.json"));
    }

    #[test]
    #[cfg(unix)]
    fn virtual_reads_never_follow_a_symlink_below_the_root() {
        use std::os::unix::fs::symlink;
        let tree = tree();
        let root = tree.0.join("root");
        symlink(tree.0.join("outside"), root.join("fixtures/link")).unwrap();
        symlink(
            tree.0.join("outside/secret.txt"),
            root.join("fixtures/escape.txt"),
        )
        .unwrap();
        symlink("x.json", root.join("fixtures/inside.json")).unwrap();
        let roots = roots(&tree);
        let open = policy(&[grant("fs.read", &["/project/**"])], None, &roots);
        for path in [
            "/project/fixtures/link/secret.txt",
            "/project/fixtures/escape.txt",
            "/project/fixtures/inside.json",
        ] {
            assert!(open.read("fs.read", path).is_err(), "{path}");
        }
        // Reading a directory is an I/O failure, never a listing.
        assert!(open.read("fs.read", "/project/fixtures").is_err());
    }

    #[test]
    fn roots_are_pinned_once_with_exact_names() {
        let tree = tree();
        let mut roots = FileRoots::new();
        roots.open("project", &tree.0.join("root")).unwrap();
        assert!(matches!(
            roots.open("project", &tree.0.join("outside")),
            Err(FileRootError::Duplicate(_))
        ));
        assert!(matches!(
            roots.open("Project", &tree.0.join("root")),
            Err(FileRootError::InvalidName(_))
        ));
        assert!(matches!(
            roots.open("missing", &tree.0.join("missing")),
            Err(FileRootError::Open { .. })
        ));
        assert!(matches!(
            roots.open("relative", Path::new("relative")),
            Err(FileRootError::Open { .. })
        ));
        let file = std::fs::File::open(tree.0.join("root/secret.txt")).unwrap();
        assert!(matches!(
            roots.insert("file", file),
            Err(FileRootError::NotDirectory(_))
        ));
        let directory = std::fs::File::open(tree.0.join("outside")).unwrap();
        roots.insert("outside", directory).unwrap();
        assert_eq!(roots.names().collect::<Vec<_>>(), ["outside", "project"]);
    }

    #[test]
    fn manifests_pin_only_declared_and_mapped_roots() {
        let tree = tree();
        let package =
            Manifest::parse(br#"{"abiVersion":34,"fsRoots":["project","data"],"capabilities":[]}"#)
                .unwrap();
        let host = Manifest::parse(
            serde_json::json!({"abiVersion":34,"fsRoots":{
                "project": tree.0.join("root"),
                "other": tree.0.join("outside")},"capabilities":[]})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
        let roots = FileRoots::from_manifests(&package, Some(&host)).unwrap();
        // `data` is unmapped and `other` undeclared.
        assert_eq!(roots.names().collect::<Vec<_>>(), ["project"]);
        assert!(
            FileRoots::from_manifests(&package, None)
                .unwrap()
                .is_empty()
        );
        let missing = Manifest::parse(
            serde_json::json!({"abiVersion":34,"fsRoots":{"project": tree.0.join("missing")},
                "capabilities":[]})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
        assert!(FileRoots::from_manifests(&package, Some(&missing)).is_err());
    }

    #[test]
    fn earlier_profiles_keep_absolute_host_paths() {
        let tree = tree();
        let file = tree.0.join("root/fixtures/x.json").display().to_string();
        let requested = [grant(
            "fs.read",
            &[&format!("{}/**", tree.0.join("root").display())],
        )];
        let legacy = FilePolicy::new(&requested, None, &FileMode::HostPaths);
        assert_eq!(legacy.read("fs.read", &file).unwrap(), b"FIXTURE");
        let v37 = FileMode::of(
            varta_common::language::LanguageProfile::CheckedF64Utf16V37,
            &roots(&tree),
        );
        assert!(matches!(v37, FileMode::HostPaths));
        let v38 = FileMode::of(
            varta_common::language::LanguageProfile::CheckedF64Utf16V38,
            &roots(&tree),
        );
        // Under v38 the same absolute host grant and path are denied.
        let virtual_policy = FilePolicy::new(&requested, None, &v38);
        assert!(virtual_policy.read("fs.read", &file).is_err());
    }

    /// Run `source` compiled for `profile` with `requested` and `roots`.
    fn run(
        source: &str,
        profile: varta_common::language::LanguageProfile,
        requested: Vec<CapabilityGrant>,
        roots: FileRoots,
    ) -> Vec<String> {
        let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
            .unwrap_or_else(|e| panic!("{e}\n{source}"));
        wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
        let report = crate::VartaRuntime::new().run_with(
            &bytes,
            crate::RunOptions {
                requested,
                file_roots: roots,
                ..crate::RunOptions::default()
            },
        );
        report.into_result().unwrap().lines
    }

    fn reads(paths: &[&str]) -> String {
        let mut source = String::from("export function main(): void {\n");
        for path in paths {
            source.push_str(&format!(
                "  {{ const r = varta.result.capture(varta.fs.read({}));\n    console.log(r.status); console.log(r.value); }}\n",
                serde_json::to_string(path).unwrap()
            ));
        }
        source.push('}');
        source
    }

    #[test]
    fn v38_modules_read_virtual_paths_and_v37_modules_host_paths() {
        use varta_common::language::LanguageProfile;
        let tree = tree();
        let host = tree.0.join("root/fixtures/x.json").display().to_string();
        let lines = run(
            &reads(&["/project/fixtures/x.json", "/project/secret.txt", &host]),
            LanguageProfile::CheckedF64Utf16V38,
            vec![grant("fs.read", &["/project/fixtures/**"])],
            roots(&tree),
        );
        let logged: Vec<&str> = lines
            .iter()
            .filter_map(|line| line.strip_prefix("varta log: "))
            .collect();
        // Granted; outside the request; an absolute host path (denials are
        // status 1 with an empty value and a logged diagnostic).
        assert_eq!(logged[..2], ["0", "FIXTURE"]);
        assert_eq!(logged.iter().filter(|line| **line == "1").count(), 2);
        assert!(!lines.iter().any(|line| line.contains("ROOT_SECRET")));
        // Without a mapped root the same grant denies.
        let unmapped = run(
            &reads(&["/project/fixtures/x.json"]),
            LanguageProfile::CheckedF64Utf16V38,
            vec![grant("fs.read", &["/project/fixtures/**"])],
            FileRoots::new(),
        );
        assert!(!unmapped.iter().any(|line| line.contains("FIXTURE")));
        // V37 keeps absolute host paths and ignores the roots.
        let v37 = run(
            &reads(&[&host, "/project/fixtures/x.json"]),
            LanguageProfile::CheckedF64Utf16V37,
            vec![grant(
                "fs.read",
                &[&format!("{}/**", tree.0.join("root").display())],
            )],
            roots(&tree),
        );
        let logged: Vec<&str> = v37
            .iter()
            .filter_map(|line| line.strip_prefix("varta log: "))
            .collect();
        assert_eq!(logged[..2], ["0", "FIXTURE"]);
        assert_eq!(logged.iter().filter(|line| **line == "1").count(), 1);
    }
}
