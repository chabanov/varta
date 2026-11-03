//! Renderer-local shaped-layout memo. Hits keep the original request charge.
use super::{LineHeight, TextMetrics, TextOptions, TextSpan};
use parley::Layout;
use std::{
    collections::VecDeque,
    hash::{DefaultHasher, Hash, Hasher},
};
const MAX_ENTRIES: usize = 128;
const MAX_KEY_BYTES: usize = 2 * 1024 * 1024;
const MAX_GLYPHS: usize = 65_536;

#[derive(Clone, Copy)]
pub(super) struct Request<'a> {
    pub text: &'a str,
    pub width: Option<f32>,
    pub families: &'a [String],
    pub options: TextOptions,
    pub spans: &'a [TextSpan],
}
impl Request<'_> {
    fn hash(self) -> u64 {
        fn options(value: TextOptions, hash: &mut DefaultHasher) {
            value.size.to_bits().hash(hash);
            value.weight.to_bits().hash(hash);
            value.italic.hash(hash);
            value.white_space.hash(hash);
            value.break_anywhere.hash(hash);
            value.source.hash(hash);
            value
                .line_height
                .map(|h| match h {
                    LineHeight::MetricsRelative(v) => (0, v.to_bits()),
                    LineHeight::FontSizeRelative(v) => (1, v.to_bits()),
                    LineHeight::Absolute(v) => (2, v.to_bits()),
                })
                .hash(hash);
        }
        let mut h = DefaultHasher::new();
        self.text.hash(&mut h);
        self.width.map(f32::to_bits).hash(&mut h);
        self.families.hash(&mut h);
        options(self.options, &mut h);
        for span in self.spans {
            span.range.hash(&mut h);
            span.owner.hash(&mut h);
            span.families.hash(&mut h);
            options(span.options, &mut h);
        }
        h.finish()
    }
}
struct Key {
    text: String,
    width: Option<f32>,
    families: Vec<String>,
    options: TextOptions,
    spans: Vec<TextSpan>,
}
struct Entry {
    hash: u64,
    key: Key,
    pub value: (TextMetrics, Layout<u32>),
    pub operations: usize,
    bytes: usize,
    glyphs: usize,
    stamp: usize,
}
#[derive(Default)]
pub(super) struct Shaped {
    entries: VecDeque<Entry>,
    bytes: usize,
    glyphs: usize,
    pub hits: usize,
    pub misses: usize,
    stamp: usize,
}
impl Shaped {
    pub fn get(&mut self, request: Request<'_>) -> Option<(&(TextMetrics, Layout<u32>), usize)> {
        let hash = request.hash();
        let Some(entry) = self.entries.iter_mut().find(|e| {
            e.hash == hash
                && e.key.text == request.text
                && e.key.width == request.width
                && e.key.families == request.families
                && e.key.options == request.options
                && e.key.spans == request.spans
        }) else {
            self.misses = self.misses.saturating_add(1);
            return None;
        };
        self.hits = self.hits.saturating_add(1);
        self.stamp = self.stamp.saturating_add(1);
        entry.stamp = self.stamp;
        Some((&entry.value, entry.operations))
    }
    pub fn insert(
        &mut self,
        request: Request<'_>,
        value: &(TextMetrics, Layout<u32>),
        operations: usize,
    ) {
        let bytes = request.text.len()
            + request.families.iter().map(String::len).sum::<usize>()
            + request
                .spans
                .iter()
                .map(|s| {
                    std::mem::size_of::<TextSpan>()
                        + s.families.iter().map(String::len).sum::<usize>()
                })
                .sum::<usize>();
        let glyphs = value
            .1
            .lines()
            .map(|l| {
                l.runs()
                    .map(|r| r.clusters().map(|c| c.glyphs().count()).sum::<usize>())
                    .sum::<usize>()
            })
            .sum::<usize>();
        if bytes > MAX_KEY_BYTES || glyphs > MAX_GLYPHS {
            return;
        }
        while self.entries.len() >= MAX_ENTRIES
            || self.bytes + bytes > MAX_KEY_BYTES
            || self.glyphs + glyphs > MAX_GLYPHS
        {
            let Some(index) = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, e)| e.stamp)
                .map(|(i, _)| i)
            else {
                break;
            };
            let Some(old) = self.entries.remove(index) else {
                break;
            };
            self.bytes -= old.bytes;
            self.glyphs -= old.glyphs;
        }
        self.stamp = self.stamp.saturating_add(1);
        self.entries.push_back(Entry {
            stamp: self.stamp,
            hash: request.hash(),
            key: Key {
                text: request.text.into(),
                width: request.width,
                families: request.families.to_vec(),
                options: request.options,
                spans: request.spans.to_vec(),
            },
            value: value.clone(),
            operations,
            bytes,
            glyphs,
        });
        self.bytes += bytes;
        self.glyphs += glyphs;
    }
}
