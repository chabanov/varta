//! Read bounded, digest-pinned source snapshots before launching any workers.
use crate::{Error, model::*};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::File,
    io::Read,
    path::{Component, Path},
};
pub(crate) const MAX_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
const MAX_TOTAL_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_CASES: usize = 256;
const MAX_FEATURES: usize = 256;

pub struct LoadedSuite {
    pub(crate) inventory: Inventory,
    pub(crate) sources: Vec<String>,
    pub(crate) digest: String,
}
impl LoadedSuite {
    pub fn load(path: &Path) -> Result<Self, Error> {
        let bytes = read(path, MAX_MANIFEST_BYTES)?;
        let inventory: Inventory = serde_json::from_slice(&bytes)?;
        validate(&inventory)?;
        let root = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .canonicalize()?;
        check_case_files(&root, &inventory)?;
        let mut sources = Vec::new();
        let mut total = 0usize;
        for case in &inventory.cases {
            let mut file = root.clone();
            for part in Path::new(&case.path).components() {
                if !matches!(part, Component::Normal(_)) {
                    return Err(invalid("source path is not relative"));
                }
                file.push(part);
                if std::fs::symlink_metadata(&file)?.file_type().is_symlink() {
                    return Err(invalid("source symlinks are not admitted"));
                }
            }
            let source = read(&file, MAX_SOURCE_BYTES)?;
            total = total
                .checked_add(source.len())
                .ok_or_else(|| invalid("source byte total overflow"))?;
            if total > MAX_TOTAL_SOURCE_BYTES {
                return Err(invalid("suite sources exceed 16 MiB"));
            }
            if sha256(&source) != case.sha256 {
                return Err(invalid(&format!("source digest mismatch for {}", case.id)));
            }
            sources.push(String::from_utf8(source).map_err(|_| invalid("source is not UTF-8"))?);
        }
        Ok(Self {
            inventory,
            sources,
            digest: sha256(&bytes),
        })
    }
}
pub(crate) fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn invalid(message: &str) -> Error {
    Error::Inventory(message.into())
}
fn read(path: &Path, max: usize) -> Result<Vec<u8>, Error> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(invalid("inventory/source must be a regular file"));
    }
    let mut bytes = Vec::new();
    File::open(path)?
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > max {
        return Err(invalid("inventory/source exceeds byte limit"));
    }
    Ok(bytes)
}
fn identifier(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}
fn validate(i: &Inventory) -> Result<(), Error> {
    if i.schema_version != 1
        || i.edition != "ECMAScript 2025"
        || i.source_model != "checked-ts-main-v1"
        || !identifier(&i.id)
    {
        return Err(invalid("unknown schema, edition, source model or suite id"));
    }
    if i.cases.is_empty()
        || i.cases.len() > MAX_CASES
        || i.features.is_empty()
        || i.features.len() > MAX_FEATURES
    {
        return Err(invalid("empty or oversized inventory"));
    }
    let mut features = BTreeSet::new();
    for f in &i.features {
        if !identifier(&f.id)
            || !features.insert(&f.id)
            || f.title.is_empty()
            || f.title.len() > 240
        {
            return Err(invalid("invalid or duplicate feature"));
        }
        if f.core.is_empty() || f.core.len() > 16 {
            return Err(invalid("feature must name its CORE work streams"));
        }
        let mut cores = BTreeSet::new();
        for core in &f.core {
            if !(1..=16).any(|n| core == &format!("CORE-{n:02}")) || !cores.insert(core) {
                return Err(invalid("invalid or duplicate CORE mapping"));
            }
        }
        let prefix = match f.origin {
            Origin::Ecmascript2025 => "https://tc39.es/ecma262/2025/multipage/",
            Origin::VartaContract => "docs/",
        };
        if !f.reference.starts_with(prefix)
            || f.reference.len() > 512
            || f.reference.chars().any(char::is_control)
        {
            return Err(invalid(
                "feature reference does not match its pinned origin",
            ));
        }
    }
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for c in &i.cases {
        if !identifier(&c.id) || !ids.insert(&c.id) || !features.contains(&c.feature) {
            return Err(invalid("invalid/duplicate case or unknown feature"));
        }
        if !c.path.starts_with("cases/")
            || c.path.len() > 240
            || !c.path.ends_with(".ts")
            || !c
                .path
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_/ .".contains(&b))
            || !paths.insert(&c.path)
        {
            return Err(invalid("invalid or duplicate source path"));
        }
        if c.path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
            || !Path::new(&c.path)
                .components()
                .all(|p| matches!(p, Component::Normal(_)))
        {
            return Err(invalid(
                "source path must contain only normal relative components",
            ));
        }
        if c.sha256.len() != 64
            || !c
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid(
                "source SHA-256 must be 64 lowercase hexadecimal digits",
            ));
        }
        if c.stdout.len() > 128 || c.stdout.iter().map(String::len).sum::<usize>() > 16 * 1024 {
            return Err(invalid("expected output exceeds inventory bounds"));
        }
        if c.grants.len() > CASE_GRANTS.len()
            || !c.grants.is_sorted_by(|a, b| a < b)
            || c.grants.iter().any(|g| !CASE_GRANTS.contains(&g.as_str()))
        {
            return Err(invalid(
                "case grants must be sorted, unique and admitted case grants",
            ));
        }
        match c.baseline.status {
            BaselineStatus::Supported => {
                if c.baseline.diagnostic.is_some() || c.baseline.reason.is_some() {
                    return Err(invalid("supported baseline cannot suppress a diagnostic"));
                }
            }
            BaselineStatus::KnownUnsupported => {
                if c.baseline.diagnostic.is_none()
                    || c.baseline.diagnostic == Some(CompileCode::InternalError)
                    || c.baseline
                        .reason
                        .as_ref()
                        .is_none_or(|s| s.trim().is_empty() || s.len() > 1024)
                {
                    return Err(invalid(
                        "unsupported baseline needs a non-internal diagnostic and bounded reason",
                    ));
                }
            }
        }
    }
    Ok(())
}

// Every .ts source in the corpus must remain represented; removing a failing
// manifest row alone cannot silently remove its case from the baseline.
fn check_case_files(root: &Path, inventory: &Inventory) -> Result<(), Error> {
    let declared: BTreeSet<_> = inventory.cases.iter().map(|c| c.path.as_str()).collect();
    let mut pending = vec![root.join("cases")];
    let mut entries = 0;
    while let Some(directory) = pending.pop() {
        if std::fs::symlink_metadata(&directory)?
            .file_type()
            .is_symlink()
        {
            return Err(invalid("corpus directory symlinks are not admitted"));
        }
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            entries += 1;
            if entries > 1024 {
                return Err(invalid("corpus directory entry limit exceeded"));
            }
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                return Err(invalid("corpus symlinks are not admitted"));
            }
            if kind.is_dir() {
                pending.push(entry.path());
            } else if entry.path().extension().is_some_and(|ext| ext == "ts") {
                let path = entry.path();
                let relative = path
                    .strip_prefix(root)
                    .map_err(|_| invalid("corpus path escaped root"))?;
                let relative = relative.to_string_lossy().replace('\\', "/");
                if !declared.contains(relative.as_str()) {
                    return Err(invalid("unlisted .ts corpus source"));
                }
            }
        }
    }
    Ok(())
}
