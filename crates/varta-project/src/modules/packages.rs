//! V38 package specifiers (`package_specifiers()`). A bare specifier names
//! one package directory, `<config dir>/node_modules/<name>` or
//! `node_modules/@scope/<name>`; parent directories are never searched. The
//! package's `types` export selects a `.d.ts` declaration, and the adjacent
//! `.d.ts.map` names the one shipped TypeScript source that becomes the
//! module. Declarations and JavaScript are never loaded: a package without a
//! typed source is refused. Every read goes through the loader's pinned
//! no-follow root, bounded per file and in aggregate.
use super::{SourceLoader, TS_EXTENSION, TSX_EXTENSION};
use serde_json::{Map, Value};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};
use varta_common::language::{LanguageProfile, SUPPORTED_PROFILES};
use varta_compiler::CompilerError;

/// The one directory packages resolve from, directly under the config directory.
const NODE_MODULES: &str = "node_modules";
const MANIFEST: &str = "package.json";
const EXPORTS: &str = "exports";
/// The only condition read, in an export entry or at the manifest's top level.
const TYPES: &str = "types";
const ROOT_EXPORT: &str = ".";
const RELATIVE_PREFIX: &str = "./";
const SCOPE_PREFIX: char = '@';
const SEPARATOR: char = '/';
const PATTERN: char = '*';
const DECLARATION_EXTENSION: &str = ".d.ts";
const SOURCE_MAP_EXTENSION: &str = ".map";
/// Source map fields (Source Map revision 3).
const MAP_VERSION: &str = "version";
const MAP_SOURCES: &str = "sources";
const MAP_SOURCE_ROOT: &str = "sourceRoot";
const MAP_SECTIONS: &str = "sections";
const SOURCE_MAP_REVISION: u64 = 3;
/// npm's limit on a package name, scope included.
const MAX_NAME_BYTES: usize = 214;
/// Names npm refuses to publish.
const RESERVED_NAMES: [&str; 2] = [NODE_MODULES, "favicon.ico"];
/// One `package.json` or declaration source map, bounded like a source file.
const MAX_METADATA_FILE_BYTES: usize = super::MAX_SOURCE_BYTES;
/// Every manifest and source map one loader reads, each counted once.
const MAX_METADATA_BYTES: usize = 4 * MAX_METADATA_FILE_BYTES;
/// Characters that never appear in an export subpath or a package path.
const FORBIDDEN_PATH_CHARACTERS: [char; 6] = ['\\', '?', '#', '%', ':', PATTERN];

#[derive(Debug, thiserror::Error)]
pub(super) enum PackageError {
    #[error("invalid package specifier {specifier}: {reason}")]
    Specifier {
        specifier: String,
        reason: &'static str,
    },
    #[error(
        "package specifier {specifier} requires profile {profile}; earlier profiles import explicit relative paths"
    )]
    Profile { specifier: String, profile: String },
    #[error(
        "cannot resolve package {package}: {} does not exist (packages resolve only from node_modules in the configuration directory; relative imports start with ./ or ../)",
        manifest.display()
    )]
    Missing { package: String, manifest: PathBuf },
    #[error("cannot read {} of package {package}: {error}", path.display())]
    Read {
        package: String,
        path: PathBuf,
        error: io::Error,
    },
    #[error(
        "package metadata exceeds the loader's {MAX_METADATA_BYTES}-byte budget at package {package}"
    )]
    Budget { package: String },
    #[error("invalid {} of package {package}: {reason}", path.display())]
    Invalid {
        package: String,
        path: PathBuf,
        reason: String,
    },
    #[error("package {package} does not export {subpath}")]
    NotExported { package: String, subpath: String },
    #[error(
        "unsupported exports pattern in package {package}: {subpath} has no exact export entry, and Varta does not match '*' or trailing '/' export patterns"
    )]
    Pattern { package: String, subpath: String },
    #[error(
        "unsupported deep import {specifier}: package {package} has no exports map naming its subpaths"
    )]
    DeepImport { specifier: String, package: String },
    #[error("unsupported {construct} in the exports of package {package} at {subpath}")]
    Export {
        package: String,
        subpath: String,
        construct: &'static str,
    },
    #[error(
        "unsupported untyped package {package}: {subpath} has no 'types' export condition{fallback}; Varta compiles only typed packages from their TypeScript sources"
    )]
    Untyped {
        package: String,
        subpath: String,
        fallback: &'static str,
    },
    #[error("invalid types target {target} of package {package}: {reason}")]
    Target {
        package: String,
        target: String,
        reason: &'static str,
    },
    #[error(
        "unsupported declarations-only package {package}: {reason}; Varta compiles packages from their shipped TypeScript sources"
    )]
    DeclarationsOnly { package: String, reason: String },
    #[error(
        "source map {} of package {package} names {source_path}, which is outside the package",
        map.display()
    )]
    MapEscapes {
        package: String,
        map: PathBuf,
        source_path: String,
    },
}
impl From<PackageError> for CompilerError {
    fn from(error: PackageError) -> Self {
        Self::Module(error.to_string())
    }
}

/// A syntactically valid bare specifier: a package name and an export key.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Specifier<'a> {
    /// `name` or `@scope/name`.
    package: &'a str,
    /// The text after the package name, without its leading `/`.
    subpath: Option<&'a str>,
}
impl<'a> Specifier<'a> {
    /// `None` when `specifier` is not shaped like a package name at all
    /// (relative, absolute, URL or `scheme:` syntax), so the caller keeps its
    /// path rules and diagnostics for it.
    pub(super) fn parse(specifier: &'a str) -> Option<Result<Self, PackageError>> {
        if specifier.is_empty()
            || specifier.starts_with(['.', SEPARATOR])
            || specifier.contains([':', '\\'])
        {
            return None;
        }
        let invalid = |reason| PackageError::Specifier {
            specifier: specifier.to_owned(),
            reason,
        };
        let name_end = if specifier.starts_with(SCOPE_PREFIX) {
            let Some(scope_end) = specifier.find(SEPARATOR) else {
                return Some(Err(invalid("a scoped package name is @scope/name")));
            };
            let name = &specifier[scope_end + 1..];
            scope_end + 1 + name.find(SEPARATOR).unwrap_or(name.len())
        } else {
            specifier.find(SEPARATOR).unwrap_or(specifier.len())
        };
        let package = &specifier[..name_end];
        let subpath = specifier.get(name_end + 1..);
        let mut parts = package
            .strip_prefix(SCOPE_PREFIX)
            .unwrap_or(package)
            .split(SEPARATOR);
        let valid_name = package.len() <= MAX_NAME_BYTES
            && parts.all(name_component)
            && !RESERVED_NAMES.contains(&package);
        if !valid_name {
            return Some(Err(invalid(
                "package names are lowercase npm names of at most 214 bytes (a-z, 0-9, '-', '.', '_', '~'; not starting with '.' or '_')",
            )));
        }
        if subpath.is_some_and(|subpath| !subpath.split(SEPARATOR).all(path_segment)) {
            return Some(Err(invalid(
                "a package subpath is '/'-separated names without empty, '.' or '..' segments, controls, '\\', '?', '#', '%', ':' or '*'",
            )));
        }
        Some(Ok(Self { package, subpath }))
    }
    /// The `exports` key this specifier selects: `.` or `./subpath`.
    fn export_key(&self) -> String {
        self.subpath.map_or_else(
            || ROOT_EXPORT.to_owned(),
            |subpath| format!("{RELATIVE_PREFIX}{subpath}"),
        )
    }
    /// `<root>/node_modules/<name>` or `<root>/node_modules/@scope/<name>`.
    fn directory(&self, root: &Path) -> PathBuf {
        self.package
            .split(SEPARATOR)
            .fold(root.join(NODE_MODULES), |path, part| path.join(part))
    }
}

/// npm's URL-safe lowercase name characters, not starting with `.` or `_`.
fn name_component(part: &str) -> bool {
    !part.is_empty()
        && !part.starts_with(['.', '_'])
        && part
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-._~".contains(&b))
}

/// One component of a subpath or package-relative path.
fn path_segment(part: &str) -> bool {
    !part.is_empty()
        && part != "."
        && part != ".."
        && !part.contains(FORBIDDEN_PATH_CHARACTERS)
        && !part.chars().any(char::is_control)
}

/// The first supported profile that admits package specifiers, for diagnostics.
pub(super) fn required_profile() -> String {
    SUPPORTED_PROFILES
        .iter()
        .copied()
        .find(|profile| profile.package_specifiers())
        .and_then(profile_name)
        .unwrap_or_else(|| "with package specifiers".to_owned())
}
fn profile_name(profile: LanguageProfile) -> Option<String> {
    serde_json::to_value(profile)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
}

/// Resolution state of one loader: each specifier resolves and each manifest
/// is read once, and every metadata byte read counts against one budget.
#[derive(Default)]
pub(super) struct Packages {
    resolved: RefCell<BTreeMap<String, PathBuf>>,
    manifests: RefCell<BTreeMap<String, Arc<Value>>>,
    bytes: Cell<usize>,
}

/// The selected declaration path, as written in `package.json`.
struct Target<'a> {
    text: &'a str,
    /// Export targets must start with `./`; top-level `types` may omit it.
    exported: bool,
}

impl SourceLoader {
    /// The TypeScript source a bare specifier names (`package_specifiers()`).
    pub(super) fn resolve_package(
        &self,
        specifier: &str,
        parsed: &Specifier<'_>,
    ) -> Result<PathBuf, PackageError> {
        if let Some(path) = self.packages.resolved.borrow().get(specifier) {
            return Ok(path.clone());
        }
        let package = parsed.package;
        let directory = parsed.directory(&self.root);
        let manifest = self.manifest(package, &directory)?;
        let manifest_path = directory.join(MANIFEST);
        let object = manifest.as_object().ok_or_else(|| PackageError::Invalid {
            package: package.to_owned(),
            path: manifest_path.clone(),
            reason: "package.json must be a JSON object".into(),
        })?;
        let target = types_target(specifier, parsed, object, &manifest_path)?;
        let declaration = declaration(package, &directory, &target)?;
        let source = self.mapped_source(package, &directory, &declaration)?;
        self.packages
            .resolved
            .borrow_mut()
            .insert(specifier.to_owned(), source.clone());
        Ok(source)
    }
    /// A bounded metadata read through the pinned root, counted once against
    /// the loader's aggregate budget.
    fn metadata(&self, package: &str, path: &Path) -> Result<Vec<u8>, io::Error> {
        let bytes = self.read_bytes(path, MAX_METADATA_FILE_BYTES)?;
        let total = self.packages.bytes.get() + bytes.len();
        self.packages.bytes.set(total);
        if total > MAX_METADATA_BYTES {
            return Err(io::Error::other(PackageError::Budget {
                package: package.to_owned(),
            }));
        }
        Ok(bytes)
    }
    fn manifest(&self, package: &str, directory: &Path) -> Result<Arc<Value>, PackageError> {
        if let Some(manifest) = self.packages.manifests.borrow().get(package) {
            return Ok(Arc::clone(manifest));
        }
        let path = directory.join(MANIFEST);
        let bytes = self
            .metadata(package, &path)
            .map_err(|error| match error.kind() {
                io::ErrorKind::NotFound => PackageError::Missing {
                    package: package.to_owned(),
                    manifest: path.clone(),
                },
                _ => read_error(package, &path, error),
            })?;
        let manifest: Value =
            serde_json::from_slice(&bytes).map_err(|error| PackageError::Invalid {
                package: package.to_owned(),
                path: path.clone(),
                reason: error.to_string(),
            })?;
        let manifest = Arc::new(manifest);
        self.packages
            .manifests
            .borrow_mut()
            .insert(package.to_owned(), Arc::clone(&manifest));
        Ok(manifest)
    }
    /// The one TypeScript source the declaration's adjacent `.d.ts.map` names.
    fn mapped_source(
        &self,
        package: &str,
        directory: &Path,
        declaration: &Path,
    ) -> Result<PathBuf, PackageError> {
        let declarations_only = |reason: String| PackageError::DeclarationsOnly {
            package: package.to_owned(),
            reason,
        };
        if !self
            .exists(declaration)
            .map_err(|error| read_error(package, declaration, error))?
        {
            return Err(PackageError::Target {
                package: package.to_owned(),
                target: declaration.display().to_string(),
                reason: "the declaration file does not exist",
            });
        }
        let mut map = declaration.as_os_str().to_owned();
        map.push(SOURCE_MAP_EXTENSION);
        let map = PathBuf::from(map);
        let bytes = self
            .metadata(package, &map)
            .map_err(|error| match error.kind() {
                io::ErrorKind::NotFound => declarations_only(format!(
                    "{} has no adjacent source map {}",
                    declaration.display(),
                    map.display()
                )),
                _ => read_error(package, &map, error),
            })?;
        let invalid = |reason: &str| PackageError::Invalid {
            package: package.to_owned(),
            path: map.clone(),
            reason: reason.to_owned(),
        };
        let value: Value = serde_json::from_slice(&bytes).map_err(|e| invalid(&e.to_string()))?;
        let object = value
            .as_object()
            .ok_or_else(|| invalid("a source map is a JSON object"))?;
        if object.get(MAP_VERSION).and_then(Value::as_u64) != Some(SOURCE_MAP_REVISION) {
            return Err(invalid("only source map revision 3 is supported"));
        }
        if object.contains_key(MAP_SECTIONS) {
            return Err(invalid("index source maps (sections) are unsupported"));
        }
        match object.get(MAP_SOURCE_ROOT) {
            None => {}
            Some(Value::String(root)) if root.is_empty() => {}
            Some(_) => return Err(invalid("a non-empty sourceRoot is unsupported")),
        }
        let source = match object.get(MAP_SOURCES).and_then(Value::as_array) {
            Some(sources) if sources.len() == 1 => sources[0]
                .as_str()
                .ok_or_else(|| invalid("a source map source is a string"))?,
            _ => return Err(invalid("the source map must name exactly one source")),
        };
        let escapes = || PackageError::MapEscapes {
            package: package.to_owned(),
            map: map.clone(),
            source_path: source.to_owned(),
        };
        if source.is_empty()
            || source.starts_with(SEPARATOR)
            || source.contains(['\\', '?', '#', ':'])
            || source.chars().any(char::is_control)
        {
            return Err(escapes());
        }
        let parent = declaration.parent().ok_or_else(escapes)?;
        let path = self
            .normalize(&parent.join(source))
            .map_err(|_| escapes())?;
        if !path.starts_with(directory) || path == directory {
            return Err(escapes());
        }
        let name = path.to_string_lossy();
        if name.ends_with(DECLARATION_EXTENSION) {
            return Err(declarations_only(format!(
                "{} maps to another declaration, {}",
                declaration.display(),
                path.display()
            )));
        }
        if !(name.ends_with(TS_EXTENSION) || self.tsx_specifiers && name.ends_with(TSX_EXTENSION)) {
            return Err(PackageError::Untyped {
                package: package.to_owned(),
                subpath: format!("the source {} of {}", path.display(), declaration.display()),
                fallback: " (its declarations map to a JavaScript source)",
            });
        }
        if !self
            .exists(&path)
            .map_err(|error| read_error(package, &path, error))?
        {
            return Err(declarations_only(format!(
                "the source {} named by {} is not shipped",
                path.display(),
                map.display()
            )));
        }
        Ok(path)
    }
}

fn read_error(package: &str, path: &Path, error: io::Error) -> PackageError {
    // An aggregate budget failure is reported as itself, not as a read error.
    match error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<PackageError>())
    {
        Some(PackageError::Budget { package }) => PackageError::Budget {
            package: package.clone(),
        },
        _ => PackageError::Read {
            package: package.to_owned(),
            path: path.to_owned(),
            error,
        },
    }
}

/// The declaration path the `types` condition of the selected export names,
/// or the top-level `types` for the package root.
fn types_target<'m>(
    specifier: &str,
    parsed: &Specifier<'_>,
    manifest: &'m Map<String, Value>,
    path: &Path,
) -> Result<Target<'m>, PackageError> {
    let package = parsed.package.to_owned();
    let subpath = parsed.export_key();
    let invalid = |reason: &str| PackageError::Invalid {
        package: package.clone(),
        path: path.to_owned(),
        reason: reason.to_owned(),
    };
    let exports = manifest.get(EXPORTS).filter(|exports| !exports.is_null());
    let condition = match exports {
        Some(exports) => {
            let entry = export_entry(exports, &subpath).map_err(|reason| match reason {
                EntryError::Invalid(reason) => invalid(reason),
                EntryError::Pattern => PackageError::Pattern {
                    package: package.clone(),
                    subpath: subpath.clone(),
                },
            })?;
            let Some(entry) = entry.filter(|entry| !entry.is_null()) else {
                return Err(PackageError::NotExported {
                    package: package.clone(),
                    subpath: subpath.clone(),
                });
            };
            types_condition(entry).map_err(|construct| match construct {
                ConditionError::Invalid(reason) => invalid(reason),
                ConditionError::Unsupported(construct) => PackageError::Export {
                    package: package.clone(),
                    subpath: subpath.clone(),
                    construct,
                },
            })?
        }
        None if parsed.subpath.is_some() => {
            return Err(PackageError::DeepImport {
                specifier: specifier.to_owned(),
                package,
            });
        }
        None => None,
    };
    if let Some(text) = condition {
        return Ok(Target {
            text,
            exported: true,
        });
    }
    if parsed.subpath.is_some() {
        return Err(PackageError::Untyped {
            package,
            subpath,
            fallback: "",
        });
    }
    match manifest.get(TYPES) {
        Some(Value::String(text)) => Ok(Target {
            text,
            exported: false,
        }),
        None => Err(PackageError::Untyped {
            package,
            subpath,
            fallback: " and no top-level 'types'",
        }),
        Some(_) => Err(invalid("the top-level 'types' is a string")),
    }
}

enum EntryError {
    Invalid(&'static str),
    Pattern,
}

/// The exact `exports` entry for `subpath` (`None` when it has none).
fn export_entry<'m>(exports: &'m Value, subpath: &str) -> Result<Option<&'m Value>, EntryError> {
    let root = |value| Ok((subpath == ROOT_EXPORT).then_some(value));
    let map = match exports {
        Value::String(_) | Value::Array(_) => return root(exports),
        Value::Object(map) => map,
        _ => {
            return Err(EntryError::Invalid(
                "'exports' is a string, an array or an object",
            ));
        }
    };
    let subpaths = map.keys().filter(|key| key.starts_with('.')).count();
    if subpaths == 0 {
        // A conditions object for the package root.
        return root(exports);
    }
    if subpaths != map.len() {
        return Err(EntryError::Invalid(
            "'exports' mixes subpath keys and condition keys",
        ));
    }
    if let Some(entry) = map.get(subpath) {
        return Ok(Some(entry));
    }
    if map
        .keys()
        .any(|key| key.contains(PATTERN) || key.ends_with(SEPARATOR))
    {
        return Err(EntryError::Pattern);
    }
    Ok(None)
}

enum ConditionError {
    Invalid(&'static str),
    Unsupported(&'static str),
}

/// The `types` condition directly under an export entry; a plain string
/// target (JavaScript) has none.
fn types_condition(entry: &Value) -> Result<Option<&str>, ConditionError> {
    let conditions = match entry {
        Value::String(_) => return Ok(None),
        Value::Array(_) => return Err(ConditionError::Unsupported("fallback array")),
        Value::Object(conditions) => conditions,
        _ => {
            return Err(ConditionError::Invalid(
                "an export target is a string, an array, an object or null",
            ));
        }
    };
    if conditions.keys().any(|key| key.starts_with('.')) {
        return Err(ConditionError::Invalid(
            "an export's conditions cannot contain subpath keys",
        ));
    }
    match conditions.get(TYPES) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(target)) => Ok(Some(target)),
        Some(Value::Object(_)) => Err(ConditionError::Unsupported(
            "nested condition under 'types'",
        )),
        Some(Value::Array(_)) => Err(ConditionError::Unsupported("fallback array")),
        Some(_) => Err(ConditionError::Invalid(
            "a 'types' condition is a string target",
        )),
    }
}

/// The declaration a types target names inside the package directory.
fn declaration(
    package: &str,
    directory: &Path,
    target: &Target<'_>,
) -> Result<PathBuf, PackageError> {
    let invalid = |reason| PackageError::Target {
        package: package.to_owned(),
        target: target.text.to_owned(),
        reason,
    };
    let relative = match target.text.strip_prefix(RELATIVE_PREFIX) {
        Some(relative) => relative,
        None if target.exported => return Err(invalid("export targets start with ./")),
        None => target.text,
    };
    if relative
        .split(SEPARATOR)
        .any(|segment| segment == ".." || segment == ".")
    {
        return Err(invalid("a types target cannot leave the package directory"));
    }
    if !relative.split(SEPARATOR).all(path_segment) {
        return Err(invalid(
            "a types target is a '/'-separated path inside the package, without controls, '\\', '?', '#', '%', ':' or '*'",
        ));
    }
    if !relative.ends_with(DECLARATION_EXTENSION) {
        return Err(invalid(
            "unsupported types target: Varta maps a .d.ts declaration to its TypeScript source through the adjacent .d.ts.map",
        ));
    }
    Ok(relative
        .split(SEPARATOR)
        .fold(directory.to_owned(), |path, segment| path.join(segment)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(specifier: &str) -> Option<Result<(String, Option<String>), String>> {
        Specifier::parse(specifier).map(|result| {
            result
                .map(|parsed| (parsed.package.to_owned(), parsed.subpath.map(str::to_owned)))
                .map_err(|error| error.to_string())
        })
    }

    #[test]
    fn bare_specifiers_split_into_a_package_name_and_an_export_subpath() {
        let ok = |package: &str, subpath: Option<&str>| {
            Some(Ok((package.to_owned(), subpath.map(str::to_owned))))
        };
        assert_eq!(parsed("uarp-sdk"), ok("uarp-sdk", None));
        assert_eq!(parsed("@scope/name"), ok("@scope/name", None));
        assert_eq!(
            parsed("@scope/name/a/b.js"),
            ok("@scope/name", Some("a/b.js"))
        );
        assert_eq!(parsed("lodash.get/x"), ok("lodash.get", Some("x")));
        // Not package-shaped: the relative/URL rules and diagnostics apply.
        for specifier in [
            "./x.ts",
            "../x.ts",
            "/x.ts",
            "node:fs",
            "https://x",
            "",
            ".x",
            "a\\b",
        ] {
            assert!(parsed(specifier).is_none(), "{specifier}");
        }
    }

    #[test]
    fn invalid_package_names_and_subpaths_are_refused_with_a_reason() {
        let long = "a".repeat(MAX_NAME_BYTES + 1);
        for specifier in [
            "Upper",
            "_private",
            "@scope",
            "@/name",
            "@scope/",
            "@Scope/name",
            "name with space",
            "node_modules",
            "favicon.ico",
            long.as_str(),
            "pkg/",
            "pkg//x",
            "pkg/../x",
            "pkg/./x",
            "pkg/x?y",
            "pkg/x#y",
            "pkg/%2e%2e",
            "pkg/*",
            "pkg/a\u{7}",
        ] {
            let error = parsed(specifier).unwrap().unwrap_err();
            assert!(
                error.contains("invalid package specifier"),
                "{specifier}: {error}"
            );
        }
    }

    #[test]
    fn export_entries_match_exact_keys_and_refuse_patterns() {
        let exports =
            serde_json::json!({".": {"types": "./a.d.ts"}, "./sub": {"types": "./s.d.ts"}});
        assert!(matches!(export_entry(&exports, "./sub"), Ok(Some(_))));
        assert!(matches!(export_entry(&exports, "./other"), Ok(None)));
        let patterns = serde_json::json!({".": "./a.js", "./features/*": "./f/*.js"});
        assert!(matches!(
            export_entry(&patterns, "./features/x"),
            Err(EntryError::Pattern)
        ));
        // An exact key still wins over a pattern.
        assert!(matches!(export_entry(&patterns, "."), Ok(Some(_))));
        let folders = serde_json::json!({"./lib/": "./lib/"});
        assert!(matches!(
            export_entry(&folders, "./lib/x"),
            Err(EntryError::Pattern)
        ));
        let conditions = serde_json::json!({"types": "./a.d.ts", "import": "./a.js"});
        assert!(matches!(export_entry(&conditions, "."), Ok(Some(_))));
        assert!(matches!(export_entry(&conditions, "./sub"), Ok(None)));
        let mixed = serde_json::json!({".": "./a.js", "import": "./a.js"});
        assert!(matches!(
            export_entry(&mixed, "."),
            Err(EntryError::Invalid(_))
        ));
    }

    #[test]
    fn only_a_direct_types_condition_selects_a_declaration() {
        let entry = serde_json::json!({"import": "./a.js", "types": "./a.d.ts"});
        assert!(matches!(types_condition(&entry), Ok(Some("./a.d.ts"))));
        for entry in [
            serde_json::json!("./a.js"),
            serde_json::json!({"import": {"types": "./a.d.ts"}}),
            serde_json::json!({"types": null}),
        ] {
            assert!(matches!(types_condition(&entry), Ok(None)), "{entry}");
        }
        for entry in [
            serde_json::json!(["./a.js"]),
            serde_json::json!({"types": ["./a.d.ts"]}),
            serde_json::json!({"types": {"import": "./a.d.ts"}}),
        ] {
            assert!(
                matches!(types_condition(&entry), Err(ConditionError::Unsupported(_))),
                "{entry}"
            );
        }
    }

    #[test]
    fn types_targets_stay_inside_the_package_and_name_declarations() {
        let directory = Path::new("/root/node_modules/p");
        let target = |text, exported| declaration("p", directory, &Target { text, exported });
        assert_eq!(
            target("./dist/index.d.ts", true).unwrap(),
            directory.join("dist").join("index.d.ts")
        );
        assert_eq!(
            target("index.d.ts", false).unwrap(),
            directory.join("index.d.ts")
        );
        for (text, exported) in [
            ("dist/index.d.ts", true),
            ("./../escape.d.ts", true),
            ("./dist/../../escape.d.ts", true),
            ("../escape.d.ts", false),
            ("./dist/index.js", true),
            ("./dist/index.d.mts", true),
            ("./dist/*.d.ts", true),
            ("./dist//index.d.ts", true),
        ] {
            assert!(target(text, exported).is_err(), "{text}");
        }
    }

    #[test]
    fn the_required_profile_comes_from_the_registry() {
        assert_eq!(
            Some(required_profile()),
            profile_name(LanguageProfile::CheckedF64Utf16V38)
        );
    }
}
