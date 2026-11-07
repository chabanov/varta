//! The locale data a linked module carries (K9 D4 `intl_data`).
//!
//! The generated blobs (`intl/*.bin`, `scripts/build-intl-data.py`) hold one
//! segment per ICU4X data marker. A module gets the segments of the markers
//! its linked operations read ([`intl_data::OPERATION_MARKERS`]) as passive
//! data segments, the manifest naming them and the data versions, and the
//! synthesized `intl_data` accessor that copies ranges out of them.
//!
//! A project may select its locales (`"locales"` in `varta.config.json`,
//! ECMA-402 [[AvailableLocales]]): each segment then keeps the entries of
//! the selected locales' fallback chains (ICU4X's fallback for the marker's
//! priority, the chains linked code walks) and the module gets the
//! available-locales segment that linked code resolves the host's default
//! locale against. Without a selection every locale is linked.
use icu_locale_core::Locale;
use icu_locale_fallback::{LocaleFallbackConfig, LocaleFallbackPriority, LocaleFallbacker};
use icu_provider::DataLocale;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;
use varta_common::HostOperation;
use varta_common::intl_data::{
    self, ATTRIBUTE_SEPARATOR, AVAILABLE_LOCALES_HASH, Blob, Manifest, Priority, Segment,
    SegmentWriter,
};

/// The generated blobs the compiler links from, by service.
const BLOBS: &[(&str, &[u8])] = &[
    ("decimal", include_bytes!("intl/decimal.bin")),
    ("zone", include_bytes!("intl/zone.bin")),
];
/// The generator's record of the data versions (`intl/intl.json`).
const METADATA: &str = include_str!("intl/intl.json");
/// ICU4X's version (the generator's crates and the library's).
const ICU4X: &str = "2.4.0";
/// The IANA tzdata release of `varta-intl` (`zone::DATA_VERSION`).
pub(crate) const TZDATA: &str = "2026e";

/// One marker of the generated blobs.
struct Stored {
    hash: u32,
    priority: Priority,
    singleton: bool,
    segment: &'static [u8],
}

/// Every marker of the embedded blobs, by name.
fn markers() -> Result<&'static BTreeMap<&'static str, Stored>, String> {
    static MARKERS: OnceLock<Result<BTreeMap<&'static str, Stored>, String>> = OnceLock::new();
    MARKERS
        .get_or_init(|| {
            let mut markers = BTreeMap::new();
            for &(service, bytes) in BLOBS {
                let blob = Blob::parse(bytes).map_err(|e| format!("{service} locale data: {e}"))?;
                for marker in blob.markers() {
                    let stored = Stored {
                        hash: marker.hash,
                        priority: marker.priority,
                        singleton: marker.flags & intl_data::marker_flag::SINGLETON != 0,
                        segment: marker.segment,
                    };
                    if markers.insert(marker.name, stored).is_some() {
                        return Err(format!("locale data: duplicate marker {}", marker.name));
                    }
                }
            }
            Ok(markers)
        })
        .as_ref()
        .map_err(Clone::clone)
}

/// The CLDR version the generator recorded.
fn cldr() -> String {
    METADATA
        .split("\"cldr\": \"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_default()
        .to_owned()
}

/// A project's locale selection, validated: canonical BCP 47 tags of a
/// language with optional script, region and variants (no extensions), in
/// the project's order without duplicates.
pub fn canonical_locales(locales: &[String]) -> Result<Vec<String>, String> {
    if locales.len() > intl_data::MAX_LOCALES {
        return Err(format!("more than {} locales", intl_data::MAX_LOCALES));
    }
    let mut canonical = Vec::with_capacity(locales.len());
    for tag in locales {
        let locale: Locale = tag
            .parse()
            .map_err(|_| format!("locale {tag:?} is not a BCP 47 language tag"))?;
        if !locale.extensions.is_empty() || locale.id.language.is_unknown() {
            return Err(format!(
                "locale {tag:?} must name a language without extensions"
            ));
        }
        let text = locale.to_string();
        if !canonical.contains(&text) {
            canonical.push(text);
        }
    }
    Ok(canonical)
}

/// The locales of the fallback chains of `locales` for `priority`, `und`
/// included.
fn chains(locales: &[String], priority: Priority) -> BTreeSet<String> {
    let mut config = LocaleFallbackConfig::default();
    config.priority = match priority {
        Priority::Language => LocaleFallbackPriority::Language,
        Priority::Script => LocaleFallbackPriority::Script,
        Priority::Region => LocaleFallbackPriority::Region,
    };
    let fallbacker = LocaleFallbacker::new().for_config(config);
    let mut kept = BTreeSet::from(["und".to_owned()]);
    for tag in locales {
        let Ok(locale) = tag.parse::<Locale>() else {
            continue;
        };
        let mut chain = fallbacker.fallback_for(DataLocale::from(&locale));
        while !chain.get().is_unknown() {
            kept.insert(chain.get().to_string());
            chain.step();
        }
    }
    kept
}

/// `segment` with only the entries whose locale `kept` holds.
fn select(segment: &[u8], kept: &BTreeSet<String>) -> Result<Vec<u8>, String> {
    let parsed = Segment::parse(segment).map_err(|e| e.to_string())?;
    let mut writer = SegmentWriter::new(parsed.checksum());
    for (key, payload) in parsed.entries() {
        let locale = key
            .split(|&byte| byte == ATTRIBUTE_SEPARATOR)
            .next()
            .unwrap_or_default();
        if kept.contains(std::str::from_utf8(locale).unwrap_or_default()) {
            writer.insert(key.to_vec(), payload.to_vec());
        }
    }
    writer.finish().map_err(|e| e.to_string())
}

/// The locale data of a module linking `operations`: its passive segments
/// (marker hash and bytes, in manifest order) and manifest; `None` when no
/// operation reads locale data.
pub(super) fn linked_data(
    operations: &[HostOperation],
    locales: &[String],
) -> Result<Option<super::LinkedData>, String> {
    let names: BTreeSet<&str> = operations
        .iter()
        .flat_map(|operation| intl_data::operation_markers(operation.name()))
        .copied()
        .collect();
    if names.is_empty() {
        return Ok(None);
    }
    let markers = markers()?;
    let locales = canonical_locales(locales)?;
    let mut segments = Vec::new();
    for name in names {
        let stored = markers
            .get(name)
            .ok_or_else(|| format!("locale data lacks marker {name}"))?;
        let bytes = if locales.is_empty() || stored.singleton {
            stored.segment.to_vec()
        } else {
            select(stored.segment, &chains(&locales, stored.priority))?
        };
        segments.push((stored.hash, bytes));
    }
    segments.sort_by_key(|&(hash, _)| hash);
    if !locales.is_empty() {
        segments.insert(
            0,
            (
                AVAILABLE_LOCALES_HASH,
                intl_data::available_locales_segment(&locales),
            ),
        );
    }
    let manifest = Manifest {
        cldr: cldr(),
        icu4x: ICU4X.to_owned(),
        tzdata: TZDATA.to_owned(),
        locales,
        segments: segments.iter().map(|&(hash, _)| hash).collect(),
    };
    Ok(Some((segments, manifest)))
}
