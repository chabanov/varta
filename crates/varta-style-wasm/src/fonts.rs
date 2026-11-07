//! The view's fonts: packaged faces the engine reads itself, and the one
//! import of the module, through which the browser bridge measures the
//! fonts the browser renders with its own (the generic families, the
//! default font and packaged faces the engine cannot read) with the
//! browser's own units. The bridge only measures; which family is the
//! first available font is decided in `varta_style::fonts`.
use alloc::{collections::BTreeMap, string::String, sync::Arc, vec::Vec};
use core::cell::UnsafeCell;
use varta_style::fonts::{
    FaceStyle, Family, FontSet, Generic, HostFamily, HostFonts, SizedMetrics, host_metrics,
};

#[link(wasm_import_module = "varta_view")]
unsafe extern "C" {
    /// Measure the browser's font for `kind` ([`NAMED`], [`GENERIC`] or
    /// [`DEFAULT`]) and the UTF-8 family name at `family`/`family_len`,
    /// with `weight`, `italic` (0 or 1) and `size` CSS pixels: writes `ex`,
    /// `cap`, `ch`, `ic` and the `normal` line height as five `f64` at
    /// `out`. Returns 1 when it measured, 0 when it renders no such font.
    fn font_metrics(
        kind: u32,
        family: u32,
        family_len: u32,
        weight: f64,
        italic: u32,
        size: f64,
        out: u32,
    ) -> u32;
}

/// The `kind` argument of the import.
const NAMED: u32 = 0;
const GENERIC: u32 = 1;
const DEFAULT: u32 = 2;
/// The measures the import writes.
const MEASURES: usize = 5;
/// The bridge's answers the module keeps; past it, it asks again.
const MAX_CACHED: usize = 1024;

/// The bit of each generic family in [`crate::exports::view_engine_face`]'s
/// argument.
pub(crate) fn generics(bits: u32) -> Vec<Family> {
    Generic::ALL
        .into_iter()
        .enumerate()
        .filter(|(bit, _)| bits & (1 << bit) != 0)
        .map(|(_, generic)| Family::Generic(generic))
        .collect()
}

type Key = (u32, String, u64, bool, u64);

struct Cache(UnsafeCell<BTreeMap<Key, Option<SizedMetrics>>>);
// SAFETY: wasm32 code here is single-threaded, and the cache is borrowed
// only inside `Bridge::metrics`, which never re-enters itself.
unsafe impl Sync for Cache {}
static CACHE: Cache = Cache(UnsafeCell::new(BTreeMap::new()));

/// The browser bridge's measurements.
#[derive(Debug)]
pub(crate) struct Bridge;
impl HostFonts for Bridge {
    fn metrics(&self, family: HostFamily<'_>, style: FaceStyle, size: f64) -> Option<SizedMetrics> {
        let (kind, name) = match family {
            HostFamily::Named(name) => (NAMED, name),
            HostFamily::Generic(generic) => (GENERIC, generic.name()),
            HostFamily::Default => (DEFAULT, ""),
        };
        let key = (
            kind,
            String::from(name),
            style.weight.to_bits(),
            style.italic,
            size.to_bits(),
        );
        // SAFETY: see `Cache`; this is the only borrow.
        let cache = unsafe { &mut *CACHE.0.get() };
        if let Some(found) = cache.get(&key) {
            return *found;
        }
        let mut out = [0f64; MEASURES];
        // SAFETY: the import reads `name` and writes `MEASURES` f64 at `out`,
        // both in this module's memory and live for the call.
        let measured = unsafe {
            font_metrics(
                kind,
                name.as_ptr() as u32,
                name.len() as u32,
                style.weight,
                u32::from(style.italic),
                size,
                out.as_mut_ptr() as u32,
            )
        };
        let [ex, cap, ch, ic, line] = out;
        let metrics = (measured == 1)
            .then(|| host_metrics(ex, cap, ch, ic, line))
            .flatten();
        if cache.len() < MAX_CACHED {
            cache.insert(key, metrics);
        }
        metrics
    }
}

/// The view's font set as the bridge registers it.
pub(crate) struct Fonts {
    pub set: FontSet,
}
impl Fonts {
    pub(crate) fn new() -> Self {
        Self {
            set: FontSet::new(None, Some(Arc::new(Bridge))),
        }
    }
}
