//! The ICU4X data provider of linked `Intl` code (`varta_common::intl_data`).
//!
//! ICU4X loads data per marker and locale. [`Provider`] answers each load
//! from the segment of that marker: it reads the segment header and probes
//! its sorted entries through a [`Source`] (in linked code, the
//! synthesized `intl_data` accessor over the module's passive segments; in
//! native tests, the blobs in memory), walks ICU4X's locale fallback for
//! the marker's fallback priority exactly as ICU4X's baked data does, and
//! copies only the found payload, which ICU4X deserializes (postcard 1).
//! A marker the module did not link answers `MarkerNotFound`.
//!
//! The fallback algorithm's own data (likely subtags for fallback and
//! CLDR's parent locales, a few kilobytes) stays compiled in
//! (`icu_locale_fallback`), so the compiler's linker computes the same
//! fallback chains when it selects a project's locales.
use alloc::{boxed::Box, vec, vec::Vec};
use icu_locale_core::Locale;
use icu_locale_fallback::LocaleFallbacker;
use icu_provider::buf::BufferFormat;
use icu_provider::prelude::*;
use varta_common::intl_data::{self, Found, SegmentSource};

/// Reads ranges of the passive segment of a data marker.
pub trait Source {
    /// Copy `dest.len()` bytes at `offset` of the segment of the marker
    /// with id hash `marker` into `dest`; false when the module links no
    /// such marker or the range lies outside its segment.
    fn read(&self, marker: u32, offset: u32, dest: &mut [u8]) -> bool;
    /// The length of the segment of `marker`, when it is linked.
    fn len(&self, marker: u32) -> Option<u32>;
}

/// One marker's segment as a [`SegmentSource`].
struct Segment<'a, S: ?Sized> {
    source: &'a S,
    marker: u32,
}

impl<S: Source + ?Sized> SegmentSource for Segment<'_, S> {
    fn read(&self, offset: u32, dest: &mut [u8]) -> bool {
        self.source.read(self.marker, offset, dest)
    }
}

/// The ICU4X buffer provider over a [`Source`]; use it through
/// `as_deserializing()` or the `try_new_with_buffer_provider` constructors.
#[derive(Debug, Clone, Copy)]
pub struct Provider<S>(pub S);

/// The id hash of a marker, as the blobs key segments.
#[must_use]
pub fn marker_hash(marker: DataMarkerInfo) -> u32 {
    u32::from_le_bytes(marker.id.hashed().to_bytes())
}

impl<S: Source> Provider<S> {
    fn find(
        &self,
        marker: DataMarkerInfo,
        request: DataRequest<'_>,
    ) -> Result<(Found, Option<u64>, Option<DataLocale>), DataError> {
        let segment = Segment {
            source: &self.0,
            marker: marker_hash(marker),
        };
        let (count, checksum) = intl_data::header(&segment)
            .ok_or_else(|| DataErrorKind::MarkerNotFound.with_req(marker, request))?;
        let attributes = request.id.marker_attributes.as_str();
        let prefix = request.metadata.attributes_prefix_match;
        if marker.is_singleton {
            if !request.id.locale.is_unknown() {
                return Err(DataErrorKind::InvalidRequest.with_req(marker, request));
            }
            let found = intl_data::lookup(&segment, count, "und", "", false)
                .ok_or_else(|| DataErrorKind::IdentifierNotFound.with_req(marker, request))?;
            return Ok((found, checksum, None));
        }
        let locale = request.id.locale.to_string();
        if let Some(found) = intl_data::lookup(&segment, count, &locale, attributes, prefix) {
            return Ok((found, checksum, None));
        }
        let fallbacker = LocaleFallbacker::new().for_config(marker.fallback_config);
        let mut chain = fallbacker.fallback_for(*request.id.locale);
        loop {
            let candidate = chain.get().to_string();
            if let Some(found) = intl_data::lookup(&segment, count, &candidate, attributes, prefix)
            {
                return Ok((found, checksum, Some(chain.take())));
            }
            if chain.get().is_unknown() {
                return Err(DataErrorKind::IdentifierNotFound.with_req(marker, request));
            }
            chain.step();
        }
    }
}

impl<S: Source> DynamicDataProvider<BufferMarker> for Provider<S> {
    fn load_data(
        &self,
        marker: DataMarkerInfo,
        request: DataRequest<'_>,
    ) -> Result<DataResponse<BufferMarker>, DataError> {
        let (found, checksum, locale) = self.find(marker, request)?;
        let len = usize::try_from(found.len)
            .map_err(|_| DataErrorKind::Deserialize.with_req(marker, request))?;
        let mut bytes: Vec<u8> = vec![0; len];
        if !self.0.read(marker_hash(marker), found.offset, &mut bytes) {
            return Err(DataErrorKind::Deserialize.with_req(marker, request));
        }
        let mut metadata = DataResponseMetadata::default();
        metadata.buffer_format = Some(BufferFormat::Postcard1);
        metadata.checksum = checksum;
        metadata.locale = locale;
        Ok(DataResponse {
            payload: DataPayload::from_owned_buffer(Box::from(bytes)),
            metadata,
        })
    }
}

/// Blobs held in memory (hosts and tests that read the blob files).
#[derive(Debug, Clone, Default)]
pub struct Blobs<'a> {
    markers: Vec<intl_data::Marker<'a>>,
}

impl<'a> Blobs<'a> {
    /// The markers of every blob; a later blob's marker never replaces an
    /// earlier one (hashes are unique across the generated blobs).
    pub fn new(blobs: &[&'a [u8]]) -> Result<Self, intl_data::FormatError> {
        let mut markers = Vec::new();
        for bytes in blobs {
            markers.extend_from_slice(intl_data::Blob::parse(bytes)?.markers());
        }
        markers.sort_by_key(|marker| marker.hash);
        markers.dedup_by_key(|marker| marker.hash);
        Ok(Self { markers })
    }
}

impl Source for Blobs<'_> {
    fn read(&self, marker: u32, offset: u32, dest: &mut [u8]) -> bool {
        match self.markers.binary_search_by_key(&marker, |m| m.hash) {
            Ok(index) => self.markers[index].segment.read(offset, dest),
            Err(_) => false,
        }
    }
    fn len(&self, marker: u32) -> Option<u32> {
        let index = self
            .markers
            .binary_search_by_key(&marker, |m| m.hash)
            .ok()?;
        u32::try_from(self.markers[index].segment.len()).ok()
    }
}

impl<S: Source + ?Sized> Source for &S {
    fn read(&self, marker: u32, offset: u32, dest: &mut [u8]) -> bool {
        (**self).read(marker, offset, dest)
    }
    fn len(&self, marker: u32) -> Option<u32> {
        (**self).len(marker)
    }
}

/// The project's selected locales (ECMA-402 [[AvailableLocales]]), or
/// `None` when every locale of the data is available.
pub fn available_locales<S: Source + ?Sized>(source: &S) -> Option<Vec<Locale>> {
    let len = source.len(intl_data::AVAILABLE_LOCALES_HASH)?;
    let mut bytes = vec![0; usize::try_from(len).ok()?];
    if !source.read(intl_data::AVAILABLE_LOCALES_HASH, 0, &mut bytes) {
        return None;
    }
    let text = core::str::from_utf8(&bytes).ok()?;
    Some(
        text.split('\n')
            .filter_map(|tag| tag.parse().ok())
            .collect(),
    )
}

/// The default locale for the host's locale `host`: `host` itself when
/// every locale is available, else the first selected locale on `host`'s
/// CLDR fallback chain (language priority), else the first selected
/// locale.
#[must_use]
pub fn default_locale<S: Source + ?Sized>(source: &S, host: &Locale) -> Locale {
    let Some(available) = available_locales(source) else {
        return host.clone();
    };
    let selected: Vec<DataLocale> = available.iter().map(DataLocale::from).collect();
    let mut chain = LocaleFallbacker::new()
        .for_config(Default::default())
        .fallback_for(DataLocale::from(host));
    loop {
        if let Some(index) = selected.iter().position(|locale| locale == chain.get()) {
            return available[index].clone();
        }
        if chain.get().is_unknown() {
            break;
        }
        chain.step();
    }
    available.into_iter().next().unwrap_or(Locale::UNKNOWN)
}

#[cfg(test)]
pub(crate) mod testing {
    //! The generated blobs, for native tests.
    use super::Blobs;

    pub(crate) const ZONE: &[u8] = include_bytes!("../../varta-compiler/src/link/intl/zone.bin");
    pub(crate) const DECIMAL: &[u8] =
        include_bytes!("../../varta-compiler/src/link/intl/decimal.bin");

    pub(crate) fn blobs() -> Blobs<'static> {
        Blobs::new(&[ZONE, DECIMAL]).unwrap()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use alloc::string::String;

    /// The blobs with an available-locales segment.
    struct Selected(Blobs<'static>, String);

    impl Source for Selected {
        fn read(&self, marker: u32, offset: u32, dest: &mut [u8]) -> bool {
            if marker == intl_data::AVAILABLE_LOCALES_HASH {
                return self.1.as_bytes().read(offset, dest);
            }
            self.0.read(marker, offset, dest)
        }
        fn len(&self, marker: u32) -> Option<u32> {
            if marker == intl_data::AVAILABLE_LOCALES_HASH {
                return u32::try_from(self.1.len()).ok();
            }
            self.0.len(marker)
        }
    }

    fn default(selection: Option<&str>, host: &str) -> String {
        let host: Locale = host.parse().unwrap();
        match selection {
            Some(selection) => {
                default_locale(&Selected(testing::blobs(), selection.into()), &host).to_string()
            }
            None => default_locale(&testing::blobs(), &host).to_string(),
        }
    }

    #[test]
    fn the_default_locale_resolves_through_cldr_fallback() {
        assert_eq!(default(None, "fr-CA"), "fr-CA");
        assert_eq!(default(Some("en\nuk"), "uk-UA"), "uk");
        assert_eq!(default(Some("en\nen-GB\nuk"), "en-AU"), "en");
        // en-AU's CLDR parent is en-001, not en-GB.
        assert_eq!(default(Some("en-001\nen-GB"), "en-AU"), "en-001");
        assert_eq!(default(Some("de\nuk"), "fr-FR"), "de");
        assert_eq!(
            available_locales(&Selected(testing::blobs(), "en\nsr-Latn".into())),
            Some(alloc::vec![
                "en".parse().unwrap(),
                "sr-Latn".parse().unwrap()
            ])
        );
        assert_eq!(available_locales(&testing::blobs()), None);
    }

    #[test]
    fn missing_markers_and_identifiers_are_data_errors() {
        let provider = Provider(testing::blobs());
        let marker = icu_decimal::provider::DecimalSymbolsV1::INFO;
        let request = DataRequest::default();
        // An unknown locale falls back to the root.
        let locale: Locale = "xx-YY".parse().unwrap();
        let data_locale = DataLocale::from(&locale);
        let response = provider
            .load_data(
                marker,
                DataRequest {
                    id: DataIdentifierBorrowed::for_locale(&data_locale),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(response.metadata.locale, Some(DataLocale::default()));
        assert!(provider.load_data(marker, request).is_ok());
        let unlinked = icu_datetime::provider::semantic_skeletons::DatetimePatternsTimeV1::INFO;
        assert_eq!(
            provider.load_data(unlinked, request).unwrap_err().kind,
            DataErrorKind::MarkerNotFound
        );
    }
}
