//! One bounded cache for portable outlines and host glyph coverage.
mod coverage;
use crate::{GlyphRasterBounds, GlyphRasterRequest, GlyphRasterizer, PaintError};
pub(crate) use coverage::{Coverage, Outcome};
use std::{collections::BTreeMap, sync::Arc};
use tiny_skia::{Path, Rect};
const MAX_ENTRIES: usize = 4096;
pub(crate) const MAX_BYTES: usize = 8 * 1024 * 1024;
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Key {
    pub font: u64,
    pub index: u32,
    pub size: u32,
    pub coords: Vec<i16>,
    pub glyph: u32,
    pub phase: Option<u8>,
}
#[derive(Clone)]
pub(crate) struct Outline {
    pub path: Option<Arc<Path>>,
    pub commands: usize,
}
#[derive(Clone, Default)]
struct Raster {
    bounds: Option<GlyphRasterBounds>,
    coverage: Option<Arc<Coverage>>,
    declined: bool,
}
#[derive(Clone)]
enum Data {
    Outline(Outline),
    Raster(Raster),
}
struct Entry {
    data: Data,
    stamp: usize,
    bytes: usize,
}
#[derive(Default)]
pub(crate) struct Glyphs {
    entries: BTreeMap<Key, Entry>,
    bytes: usize,
    stamp: usize,
    pub hits: usize,
    pub raster_glyphs: usize,
    pub outline_glyphs: usize,
    provider: Option<Box<dyn GlyphRasterizer>>,
}
impl Glyphs {
    pub fn set_provider(&mut self, provider: Option<Box<dyn GlyphRasterizer>>) {
        *self = Self {
            provider,
            ..Self::default()
        };
    }
    pub fn has_provider(&self) -> bool {
        self.provider.is_some()
    }
    /// Whether the provider places the request's glyphs at subpixel phases.
    pub fn subpixel(&mut self, request: &GlyphRasterRequest<'_>) -> bool {
        self.provider
            .as_mut()
            .is_none_or(|provider| provider.subpixel(request))
    }
    fn get_data(&mut self, key: &Key) -> Option<Data> {
        let entry = self.entries.get_mut(key)?;
        self.stamp = self.stamp.saturating_add(1);
        entry.stamp = self.stamp;
        self.hits = self.hits.saturating_add(1);
        Some(entry.data.clone())
    }
    pub fn get(&mut self, key: &Key) -> Option<Outline> {
        match self.get_data(key)? {
            Data::Outline(outline) => Some(outline),
            Data::Raster(_) => None,
        }
    }
    pub fn insert(&mut self, key: Key, outline: Outline) {
        self.insert_data(key, Data::Outline(outline));
    }
    fn allowance(&self) -> usize {
        MAX_BYTES.saturating_sub(self.provider.as_ref().map_or(0, |p| p.retained_bytes()))
    }
    fn evict(&mut self) {
        if let Some(key) = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.stamp)
            .map(|(key, _)| key.clone())
        {
            let entry = self.entries.remove(&key).expect("selected cache entry");
            self.bytes -= entry.bytes;
        }
    }
    fn trim_provider(&mut self) {
        if self
            .provider
            .as_ref()
            .is_some_and(|p| p.retained_bytes() > MAX_BYTES)
        {
            self.provider = None;
        }
        while self.bytes > self.allowance() {
            self.evict();
        }
    }
    fn insert_data(&mut self, key: Key, data: Data) {
        let bytes = std::mem::size_of::<Entry>()
            + std::mem::size_of::<Key>()
            + key.coords.len() * std::mem::size_of::<i16>()
            + match &data {
                Data::Outline(outline) => outline
                    .path
                    .as_ref()
                    .map_or(0, |p| std::mem::size_of_val(p.points()) + p.verbs().len()),
                Data::Raster(raster) => raster.coverage.as_ref().map_or(0, |c| c.bytes()),
            };
        if let Some(old) = self.entries.remove(&key) {
            self.bytes -= old.bytes;
        }
        let allowance = self.allowance();
        if bytes > allowance {
            return;
        }
        while self.entries.len() >= MAX_ENTRIES || self.bytes.saturating_add(bytes) > allowance {
            self.evict();
        }
        self.stamp = self.stamp.saturating_add(1);
        self.entries.insert(
            key,
            Entry {
                data,
                bytes,
                stamp: self.stamp,
            },
        );
        self.bytes += bytes;
    }
    pub fn raster(
        &mut self,
        key: Key,
        request: &GlyphRasterRequest<'_>,
        origin: (f32, f32),
        clip: Rect,
        work: &mut usize,
    ) -> Result<Outcome, PaintError> {
        if self.provider.is_none() {
            return Ok(Outcome::Missing);
        }
        let mut state = match self.get_data(&key) {
            Some(Data::Raster(r)) => r,
            _ => {
                let bounds = self
                    .provider
                    .as_mut()
                    .and_then(|p| p.bounds(request))
                    .filter(|b| b.pixels().is_some_and(|n| n <= request.max_pixels));
                self.trim_provider();
                let state = Raster {
                    bounds,
                    ..Raster::default()
                };
                self.insert_data(key.clone(), Data::Raster(state.clone()));
                state
            }
        };
        let Some(bounds) = state.bounds else {
            return Ok(Outcome::Missing);
        };
        let left = origin.0 + f64::from(bounds.left) as f32;
        let top = origin.1 + f64::from(bounds.top) as f32;
        if bounds.width == 0
            || bounds.height == 0
            || left >= clip.right()
            || top >= clip.bottom()
            || left + bounds.width as f32 <= clip.left()
            || top + bounds.height as f32 <= clip.top()
        {
            *work = work.saturating_add(1);
            return Ok(Outcome::Culled);
        }
        let pixels = bounds.pixels().expect("validated raster bounds");
        // Charge the same coverage work on a cache hit and a miss. A large or
        // exhausted optional mask path selects the existing portable outline.
        if state.declined || work.saturating_add(pixels + 1) > crate::paint::MAX_TEXT_PAINT_WORK {
            return Ok(Outcome::Missing);
        }
        if state.coverage.is_none() {
            let mask = self
                .provider
                .as_mut()
                .and_then(|p| p.rasterize(request, bounds));
            self.trim_provider();
            if let Some(mask) = mask.filter(|m| m.bounds == bounds) {
                state.coverage = Some(Arc::new(Coverage::new(mask)?));
            } else {
                state.declined = true;
            }
            self.insert_data(key, Data::Raster(state.clone()));
        }
        let Some(coverage) = state.coverage else {
            return Ok(Outcome::Missing);
        };
        *work = work.saturating_add(pixels + 1);
        self.raster_glyphs = self.raster_glyphs.saturating_add(1);
        Ok(Outcome::Ready(coverage))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GlyphRasterMask;
    struct Provider {
        bytes: usize,
    }
    impl GlyphRasterizer for Provider {
        fn bounds(&mut self, _: &GlyphRasterRequest<'_>) -> Option<GlyphRasterBounds> {
            Some(GlyphRasterBounds {
                left: 0,
                top: 0,
                width: 2,
                height: 2,
            })
        }
        fn rasterize(
            &mut self,
            _: &GlyphRasterRequest<'_>,
            b: GlyphRasterBounds,
        ) -> Option<GlyphRasterMask> {
            GlyphRasterMask::new(b, vec![255; 4]).ok()
        }
        fn retained_bytes(&self) -> usize {
            self.bytes
        }
    }
    fn key(phase: Option<u8>) -> Key {
        Key {
            font: 0,
            index: 0,
            size: 20f32.to_bits(),
            coords: Vec::new(),
            glyph: 0,
            phase,
        }
    }
    #[test]
    fn provider_bytes_share_the_old_cache_ceiling_and_hits_charge_the_same_work() {
        let mut cache = Glyphs::default();
        cache.set_provider(Some(Box::new(Provider {
            bytes: MAX_BYTES - 128,
        })));
        let request = GlyphRasterRequest {
            font_id: 0,
            font_data: &[],
            font_index: 0,
            glyph: 0,
            size: 20.0,
            normalized_coords: &[],
            phase_x: 0,
            max_pixels: crate::paint::MAX_TEXT_PAINT_WORK,
            cache_bytes: MAX_BYTES,
        };
        let clip = Rect::from_xywh(0.0, 0.0, 80.0, 80.0).unwrap();
        cache.insert(
            key(None),
            Outline {
                path: Some(Arc::new(tiny_skia::PathBuilder::from_rect(clip))),
                commands: 5,
            },
        );
        assert!(cache.bytes + cache.provider.as_ref().unwrap().retained_bytes() <= MAX_BYTES);
        let mut work = 0;
        assert!(matches!(
            cache
                .raster(key(Some(0)), &request, (0.0, 0.0), clip, &mut work)
                .unwrap(),
            Outcome::Ready(_)
        ));
        assert_eq!(work, 5);
        assert!(cache.bytes + cache.provider.as_ref().unwrap().retained_bytes() <= MAX_BYTES);
        cache.set_provider(Some(Box::new(Provider { bytes: 0 })));
        for _ in 0..2 {
            let mut work = 0;
            assert!(matches!(
                cache
                    .raster(key(Some(0)), &request, (0.0, 0.0), clip, &mut work)
                    .unwrap(),
                Outcome::Ready(_)
            ));
            assert_eq!(work, 5);
        }
        assert!(cache.hits > 0);
        let mut work = crate::paint::MAX_TEXT_PAINT_WORK - 4;
        assert!(matches!(
            cache
                .raster(key(Some(0)), &request, (0.0, 0.0), clip, &mut work)
                .unwrap(),
            Outcome::Missing
        ));
        assert_eq!(work, crate::paint::MAX_TEXT_PAINT_WORK - 4);
    }
}
