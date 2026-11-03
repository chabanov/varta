//! Bounded renderer-local unhinted outlines, independent of glyph position/DPI.
use std::{collections::BTreeMap, sync::Arc};
use tiny_skia::Path;
const MAX_ENTRIES: usize = 4096;
const MAX_BYTES: usize = 8 * 1024 * 1024;
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Key {
    pub font: u64,
    pub index: u32,
    pub size: u32,
    pub coords: Vec<i16>,
    pub glyph: u32,
}
#[derive(Clone)]
pub(crate) struct Outline {
    pub path: Option<Arc<Path>>,
    pub commands: usize,
}
struct Entry {
    outline: Outline,
    stamp: usize,
    bytes: usize,
}
#[derive(Default)]
pub(crate) struct Glyphs {
    entries: BTreeMap<Key, Entry>,
    bytes: usize,
    stamp: usize,
    pub hits: usize,
}
impl Glyphs {
    pub fn get(&mut self, key: &Key) -> Option<Outline> {
        let entry = self.entries.get_mut(key)?;
        self.stamp = self.stamp.saturating_add(1);
        entry.stamp = self.stamp;
        self.hits = self.hits.saturating_add(1);
        Some(entry.outline.clone())
    }
    pub fn insert(&mut self, key: Key, outline: Outline) {
        let bytes = std::mem::size_of::<Entry>()
            + std::mem::size_of::<Key>()
            + key.coords.len() * std::mem::size_of::<i16>()
            + outline
                .path
                .as_ref()
                .map_or(0, |p| std::mem::size_of_val(p.points()) + p.verbs().len());
        if bytes > MAX_BYTES {
            return;
        }
        while self.entries.len() >= MAX_ENTRIES || self.bytes.saturating_add(bytes) > MAX_BYTES {
            let Some(key) = self
                .entries
                .iter()
                .min_by_key(|(_, v)| v.stamp)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&key) {
                self.bytes -= entry.bytes;
            }
        }
        self.stamp = self.stamp.saturating_add(1);
        if let Some(old) = self.entries.insert(
            key,
            Entry {
                outline,
                bytes,
                stamp: self.stamp,
            },
        ) {
            self.bytes -= old.bytes;
        }
        self.bytes += bytes;
    }
}
