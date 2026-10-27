//! Tooling compatibility facade; the container protocol has one runtime-independent owner.
use crate::Result;
use std::path::Path;
pub use varta_artifact::Artifact;
pub(crate) use varta_artifact::MAX_MANIFEST_BYTES;

pub(crate) fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>> {
    Ok(varta_artifact::read_bounded(path, limit)?)
}
