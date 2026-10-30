//! Virtual filesystem roots of profile v38 (`file_roots`).
//!
//! Under v38 a guest never sees a host path. Every `fs.*` path and grant
//! pattern is a virtual path `/<root>/<component>/...` whose first component
//! names a root:
//!
//! - a package manifest (and the project configuration it is built from)
//!   **names** its roots: `"fsRoots": ["project"]`;
//! - a host policy **maps** root names to absolute host directories:
//!   `"fsRoots": {"project": "/srv/app/data"}`; the package never chooses a
//!   host directory, and an unmapped root denies;
//! - `varta dev` maps the roots of the project configuration
//!   (`"fs": {"roots": {"project": "."}}`), each a directory below the
//!   configuration directory, opened no-follow at every component.
//!
//! The runtime pins each declared and mapped root once at startup as a
//! capability descriptor; every read walks from that descriptor with
//! no-follow at every component. V1–v37 keep absolute host paths and never
//! read these members.
use crate::{Result, VartaError};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// At most this many roots in one manifest, host policy or configuration.
pub const MAX_FILE_ROOTS: usize = 16;
/// The longest root name.
pub const MAX_ROOT_NAME_BYTES: usize = 64;
/// The longest host directory of a root mapping, and the longest virtual
/// path a guest may pass.
pub const MAX_PATH_BYTES: usize = 4096;
/// The separator of virtual paths on every host.
pub const SEPARATOR: char = '/';

/// The roots of a manifest: names in a package, a mapping in a host policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsRoots {
    /// A package manifest: the roots it reads under (a JSON array).
    Names(Vec<String>),
    /// A host policy: each root name mapped to an absolute host directory
    /// (a JSON object; a duplicate name is an error).
    Directories(Vec<RootDirectory>),
}

/// One host mapping: root `name` is the host directory `directory`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootDirectory {
    pub name: String,
    pub directory: String,
}

/// Why a root could not be pinned.
#[derive(Debug, thiserror::Error)]
pub enum FileRootError {
    #[error("invalid file root name: {0:?}")]
    InvalidName(String),
    #[error("file root {0} is pinned twice")]
    Duplicate(String),
    #[error("more than {MAX_FILE_ROOTS} file roots")]
    TooMany,
    #[error("file root {name}: cannot open directory {directory}: {message}")]
    Open {
        name: String,
        directory: String,
        message: String,
    },
    #[error("file root {0} is not a directory")]
    NotDirectory(String),
}

/// A root name: 1 to [`MAX_ROOT_NAME_BYTES`] bytes of lowercase ASCII
/// letters, digits, `-` and `_`, starting with a letter. Names compare
/// exactly.
#[must_use]
pub fn valid_root_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    name.len() <= MAX_ROOT_NAME_BYTES
        && bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn names_unique<'a>(names: impl Iterator<Item = &'a str>) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    for name in names {
        if !valid_root_name(name) {
            return Err(VartaError::InvalidManifest(format!(
                "invalid fsRoots name {name:?}: use 1 to {MAX_ROOT_NAME_BYTES} lowercase letters, digits, '-' or '_', starting with a letter"
            )));
        }
        if !seen.insert(name) {
            return Err(VartaError::InvalidManifest(format!(
                "duplicate fsRoots name: {name}"
            )));
        }
    }
    if seen.len() > MAX_FILE_ROOTS {
        return Err(VartaError::InvalidManifest(format!(
            "more than {MAX_FILE_ROOTS} fsRoots"
        )));
    }
    Ok(())
}

impl FsRoots {
    /// Names are valid, exact and unique; host directories are absolute,
    /// bounded and free of traversal and control characters.
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Names(names) => names_unique(names.iter().map(String::as_str)),
            Self::Directories(roots) => {
                names_unique(roots.iter().map(|root| root.name.as_str()))?;
                for root in roots {
                    let path = std::path::Path::new(&root.directory);
                    if root.directory.len() > MAX_PATH_BYTES
                        || root.directory.chars().any(char::is_control)
                        || !path.is_absolute()
                        || path
                            .components()
                            .any(|c| matches!(c, std::path::Component::ParentDir))
                    {
                        return Err(VartaError::InvalidManifest(format!(
                            "fsRoots {} must map to an absolute host directory without traversal",
                            root.name
                        )));
                    }
                }
                Ok(())
            }
        }
    }
    /// The declared names of a package manifest (none for a mapping).
    #[must_use]
    pub fn names(&self) -> &[String] {
        match self {
            Self::Names(names) => names,
            Self::Directories(_) => &[],
        }
    }
    /// The host mapping of a host policy (none for a name list).
    #[must_use]
    pub fn directories(&self) -> &[RootDirectory] {
        match self {
            Self::Names(_) => &[],
            Self::Directories(roots) => roots,
        }
    }
}

/// A guest path under v38: `/<root>` and zero or more `/<component>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VirtualPath<'a> {
    pub root: &'a str,
    pub components: Vec<&'a str>,
}

/// Whether `component` is an ordinary name: nonempty, not `.` or `..`.
fn ordinary(component: &str) -> bool {
    !matches!(component, "" | "." | "..")
}

/// Parse a v38 guest path. `None` (a denial) unless it is at most
/// [`MAX_PATH_BYTES`] bytes, starts with `/`, contains no NUL, control
/// character or `\`, names a valid root and has only ordinary components
/// (no empty, `.` or `..` component, so no trailing or doubled `/`).
#[must_use]
pub fn parse_virtual_path(input: &str) -> Option<VirtualPath<'_>> {
    if input.len() > MAX_PATH_BYTES || input.chars().any(|c| c.is_control() || c == '\\') {
        return None;
    }
    let mut parts = input.strip_prefix(SEPARATOR)?.split(SEPARATOR);
    let root = parts.next().filter(|root| valid_root_name(root))?;
    let components: Vec<&str> = parts.collect();
    components
        .iter()
        .all(|c| ordinary(c))
        .then_some(VirtualPath { root, components })
}

/// A v38 package's `fs.*` pattern: `/<root>` with a literal root that
/// `declared` names, then ordinary components that may hold wildcards.
pub fn validate_virtual_pattern(id: &str, pattern: &str, declared: &[String]) -> Result<()> {
    let invalid = || {
        VartaError::InvalidManifest(format!(
            "{id} pattern {pattern} must be a virtual path /<root>/... under a root declared in fsRoots (checked-f64-utf16-v38)"
        ))
    };
    if pattern.chars().any(|c| c.is_control() || c == '\\') {
        return Err(invalid());
    }
    let mut parts = pattern
        .strip_prefix(SEPARATOR)
        .ok_or_else(invalid)?
        .split(SEPARATOR);
    let root = parts.next().ok_or_else(invalid)?;
    if !declared.iter().any(|name| name == root) || !parts.all(ordinary) {
        return Err(invalid());
    }
    Ok(())
}

impl Serialize for FsRoots {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            Self::Names(names) => {
                let mut seq = serializer.serialize_seq(Some(names.len()))?;
                for name in names {
                    seq.serialize_element(name)?;
                }
                seq.end()
            }
            Self::Directories(roots) => {
                let mut map = serializer.serialize_map(Some(roots.len()))?;
                for root in roots {
                    map.serialize_entry(&root.name, &root.directory)?;
                }
                map.end()
            }
        }
    }
}

struct RootsVisitor;
impl<'de> Visitor<'de> for RootsVisitor {
    type Value = FsRoots;
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("an array of root names or an object of root directories")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<FsRoots, A::Error> {
        let mut names = Vec::new();
        while let Some(name) = seq.next_element::<String>()? {
            if names.len() == MAX_FILE_ROOTS {
                return Err(de::Error::custom(format!(
                    "more than {MAX_FILE_ROOTS} fsRoots"
                )));
            }
            names.push(name);
        }
        Ok(FsRoots::Names(names))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<FsRoots, A::Error> {
        let mut roots: Vec<RootDirectory> = Vec::new();
        while let Some((name, directory)) = map.next_entry::<String, String>()? {
            // A JSON object may repeat a key; a mapping never silently keeps
            // one of two directories.
            if roots.iter().any(|root| root.name == name) {
                return Err(de::Error::custom(format!("duplicate fsRoots name: {name}")));
            }
            if roots.len() == MAX_FILE_ROOTS {
                return Err(de::Error::custom(format!(
                    "more than {MAX_FILE_ROOTS} fsRoots"
                )));
            }
            roots.push(RootDirectory { name, directory });
        }
        Ok(FsRoots::Directories(roots))
    }
}
impl<'de> Deserialize<'de> for FsRoots {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserializer.deserialize_any(RootsVisitor)
    }
}

/// The project configuration's `"fs": {"roots": {...}}`: each root name
/// mapped to a directory relative to the configuration directory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectRoots(pub Vec<RootDirectory>);

impl ProjectRoots {
    /// Names as in a manifest; directories nonempty, relative, at most
    /// [`MAX_PATH_BYTES`] bytes, with only ordinary `/`-separated components
    /// or `.` (the configuration directory itself).
    pub fn validate(&self) -> Result<()> {
        names_unique(self.0.iter().map(|root| root.name.as_str()))?;
        for root in &self.0 {
            let directory = root.directory.as_str();
            let components = directory.split(SEPARATOR);
            if directory.is_empty()
                || directory.len() > MAX_PATH_BYTES
                || directory.chars().any(|c| c.is_control() || c == '\\')
                || directory.starts_with(SEPARATOR)
                || std::path::Path::new(directory).is_absolute()
                || (directory != "." && !components.clone().all(ordinary))
            {
                return Err(VartaError::InvalidManifest(format!(
                    "fs.roots {} must be a directory inside the configuration directory: a relative path without traversal, '.' for the directory itself",
                    root.name
                )));
            }
        }
        Ok(())
    }
    /// The relative components of `directory` (none for `.`).
    #[must_use]
    pub fn components(directory: &str) -> Vec<&str> {
        if directory == "." {
            Vec::new()
        } else {
            directory.split(SEPARATOR).collect()
        }
    }
}

impl<'de> Deserialize<'de> for ProjectRoots {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct ProjectVisitor;
        impl<'de> Visitor<'de> for ProjectVisitor {
            type Value = ProjectRoots;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("an object of root directories")
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                map: A,
            ) -> std::result::Result<ProjectRoots, A::Error> {
                match RootsVisitor.visit_map(map)? {
                    FsRoots::Directories(roots) => Ok(ProjectRoots(roots)),
                    FsRoots::Names(_) => Err(de::Error::custom("expected root directories")),
                }
            }
        }
        deserializer.deserialize_map(ProjectVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_names_are_exact_lowercase_identifiers() {
        for name in ["project", "a", "data-2", "my_root", &"a".repeat(64)] {
            assert!(valid_root_name(name), "{name}");
        }
        for name in [
            "",
            "Project",
            "2data",
            "-x",
            "a.b",
            "a/b",
            "..",
            ".",
            "a b",
            "é",
            &"a".repeat(65),
        ] {
            assert!(!valid_root_name(name), "{name}");
        }
    }

    #[test]
    fn virtual_paths_are_canonical_and_name_a_root() {
        let path = parse_virtual_path("/project/fixtures/x.json").unwrap();
        assert_eq!(path.root, "project");
        assert_eq!(path.components, ["fixtures", "x.json"]);
        assert_eq!(
            parse_virtual_path("/project").unwrap().components,
            Vec::<&str>::new()
        );
        for denied in [
            "project/x",
            "/",
            "//project/x",
            "/project/",
            "/project//x",
            "/project/./x",
            "/project/../x",
            "/project/a/..",
            "/Project/x",
            "/project\\x",
            "/project/a\0b",
            "/project/a\nb",
            "C:\\project",
            &format!("/project/{}", "a".repeat(MAX_PATH_BYTES)),
        ] {
            assert!(parse_virtual_path(denied).is_none(), "{denied:?}");
        }
    }

    #[test]
    fn package_patterns_start_at_a_declared_literal_root() {
        let declared = vec!["project".to_owned(), "data".to_owned()];
        for pattern in [
            "/project/fixtures/**",
            "/project/*.json",
            "/data/a.txt",
            "/project",
        ] {
            validate_virtual_pattern("fs.read", pattern, &declared).unwrap();
        }
        for pattern in [
            "/other/**",
            "/proj*/**",
            "/**",
            "/project-x/**",
            "project/**",
            "/project//x",
            "/project/./x",
            "/project/\\x",
            "/Users/me/fixtures/**",
        ] {
            assert!(
                validate_virtual_pattern("fs.read", pattern, &declared).is_err(),
                "{pattern}"
            );
        }
        assert!(validate_virtual_pattern("fs.read", "/project/x", &[]).is_err());
    }

    #[test]
    fn roots_parse_as_names_or_a_duplicate_free_mapping() {
        let names: FsRoots = serde_json::from_str(r#"["project","data"]"#).unwrap();
        assert_eq!(names.names(), ["project", "data"]);
        names.validate().unwrap();
        let map: FsRoots = serde_json::from_str(r#"{"project":"/srv/app"}"#).unwrap();
        assert_eq!(
            map.directories(),
            [RootDirectory {
                name: "project".into(),
                directory: "/srv/app".into()
            }]
        );
        assert_eq!(
            serde_json::to_string(&map).unwrap(),
            r#"{"project":"/srv/app"}"#
        );
        assert!(serde_json::from_str::<FsRoots>(r#"{"a":"/x","a":"/y"}"#).is_err());
        assert!(serde_json::from_str::<FsRoots>(r#""project""#).is_err());
        assert!(serde_json::from_str::<FsRoots>(r#"[1]"#).is_err());
        let many = format!(
            "[{}]",
            (0..=MAX_FILE_ROOTS)
                .map(|i| format!("\"r{i}\""))
                .collect::<Vec<_>>()
                .join(",")
        );
        assert!(serde_json::from_str::<FsRoots>(&many).is_err());
        for invalid in [
            r#"["project","project"]"#,
            r#"["Project"]"#,
            r#"{"project":"relative/dir"}"#,
            r#"{"project":"/srv/../etc"}"#,
            r#"{"project":"/srv/a\nb"}"#,
            r#"{"Bad":"/srv"}"#,
        ] {
            let roots: FsRoots = serde_json::from_str(invalid).unwrap();
            assert!(roots.validate().is_err(), "{invalid}");
        }
    }

    #[test]
    fn project_roots_stay_inside_the_configuration_directory() {
        let roots = |json: &str| serde_json::from_str::<ProjectRoots>(json);
        for valid in [
            r#"{"project":"."}"#,
            r#"{"fixtures":"tests/fixtures","data":"data"}"#,
        ] {
            roots(valid).unwrap().validate().unwrap();
        }
        assert_eq!(ProjectRoots::components("."), Vec::<&str>::new());
        assert_eq!(ProjectRoots::components("a/b"), ["a", "b"]);
        for invalid in [
            r#"{"project":""}"#,
            r#"{"project":"/abs"}"#,
            r#"{"project":".."}"#,
            r#"{"project":"a/../.."}"#,
            r#"{"project":"./a"}"#,
            r#"{"project":"a/"}"#,
            r#"{"project":"a\\b"}"#,
            r#"{"Project":"a"}"#,
        ] {
            assert!(roots(invalid).unwrap().validate().is_err(), "{invalid}");
        }
        assert!(roots(r#"{"a":"x","a":"y"}"#).is_err());
        assert!(roots(r#"["project"]"#).is_err());
    }
}
