//! The v38 locale data format (K9 D4 `intl_data`): the contract between the
//! data generator (`scripts/build-intl-data.py`), the compiler's linker and
//! the linked `Intl` code (`varta-intl`'s data provider).
//!
//! ICU4X's locale data leaves the intrinsic library's baked statics and
//! becomes one versioned **blob per service** (time zone names, date-time
//! patterns, number symbols, ...). A blob holds one **segment per data
//! marker** with that marker's payloads for every locale. The linker
//! appends the segments of the services a module links as *passive* data
//! segments; linked code copies the parts it needs (the segment header, the
//! entries its binary search probes, one payload) into its arena through
//! the synthesized `intl_data` accessor (`memory.init`), so memory grows
//! only with the locales a call touches.
//!
//! All integers are little-endian.
//!
//! Blob: [`BLOB_MAGIC`], `u32` [`FORMAT_VERSION`], `u32` marker count, the
//! markers' [`MARKER_ENTRY_BYTES`]-byte entries (`u32` marker id hash,
//! `u8` [`Priority`], `u8` [`marker_flag`] bits, `u16` name length, `u32`
//! segment offset and length in the blob), the marker names, then the
//! segments. Markers are sorted by hash; hashes are unique.
//!
//! Segment: `u32` entry count, `u32` [`segment_flag`] bits, `u64` checksum
//! (ICU4X's data checksum of the marker, when the flag says so), the
//! entries ([`ENTRY_BYTES`] bytes: `u32` key offset, `u32` key length,
//! `u32` payload offset, `u32` payload length, offsets from the segment
//! start), the keys and the payloads. Entries are sorted by key bytes and
//! keys are unique; equal payloads are stored once. A key is the data
//! identifier's locale (`und` for the root and for singleton markers),
//! followed by [`ATTRIBUTE_SEPARATOR`] and the marker attributes when they
//! are not empty. A payload is the data struct in postcard 1 form.
use alloc::{collections::BTreeMap, string::String, vec::Vec};

/// The data format version of this module.
pub const FORMAT_VERSION: u32 = 1;
/// The first bytes of a blob.
pub const BLOB_MAGIC: [u8; 4] = *b"VTID";
/// Blob header: magic, format version, marker count.
pub const BLOB_HEADER_BYTES: usize = 12;
/// One marker entry of a blob.
pub const MARKER_ENTRY_BYTES: usize = 16;
/// Segment header: entry count, flags, checksum.
pub const SEGMENT_HEADER_BYTES: usize = 16;
/// One entry of a segment.
pub const ENTRY_BYTES: usize = 16;
/// Separates a key's locale from its marker attributes (ICU4X's baked
/// data uses the same record separator).
pub const ATTRIBUTE_SEPARATOR: u8 = 0x1E;
/// The longest key: a BCP 47 data locale and marker attributes.
pub const MAX_KEY_BYTES: usize = 128;
/// The marker id hash of the linker's available-locales segment (no ICU4X
/// marker has it; the generator checks).
pub const AVAILABLE_LOCALES_HASH: u32 = 0;
/// The custom section of a module that links locale data: [`Manifest`].
pub const SECTION: &str = "varta.intl";

/// [`Blob`] marker flags.
pub mod marker_flag {
    /// The marker has one payload, keyed `und`.
    pub const SINGLETON: u8 = 1;
}

/// Segment flags.
pub mod segment_flag {
    /// The segment's checksum field holds the marker's data checksum.
    pub const CHECKSUM: u32 = 1;
}

/// ICU4X's locale fallback priority of a marker (`LocaleFallbackPriority`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Language = 0,
    Script = 1,
    Region = 2,
}

impl Priority {
    #[must_use]
    pub fn from_byte(byte: u8) -> Option<Self> {
        Some(match byte {
            0 => Self::Language,
            1 => Self::Script,
            2 => Self::Region,
            _ => return None,
        })
    }
}

/// A malformed blob, segment or manifest.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FormatError {
    #[error("locale data: bad magic or format version")]
    Version,
    #[error("locale data: truncated or out-of-bounds {0}")]
    Bounds(&'static str),
    #[error("locale data: unsorted or duplicate {0}")]
    Order(&'static str),
    #[error("locale data: invalid {0}")]
    Invalid(&'static str),
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

fn range(offset: u32, len: u32, total: usize) -> Option<core::ops::Range<usize>> {
    let start = usize::try_from(offset).ok()?;
    let end = start.checked_add(usize::try_from(len).ok()?)?;
    (end <= total).then_some(start..end)
}

/// One marker of a blob.
#[derive(Debug, Clone, Copy)]
pub struct Marker<'a> {
    pub hash: u32,
    pub priority: Priority,
    pub flags: u8,
    pub name: &'a str,
    pub segment: &'a [u8],
}

/// A validated blob: every marker's segment is a valid [`Segment`].
#[derive(Debug, Clone)]
pub struct Blob<'a> {
    markers: Vec<Marker<'a>>,
}

impl<'a> Blob<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, FormatError> {
        if bytes.get(..4) != Some(&BLOB_MAGIC[..]) || u32_at(bytes, 4) != Some(FORMAT_VERSION) {
            return Err(FormatError::Version);
        }
        let count = u32_at(bytes, 8).ok_or(FormatError::Bounds("blob header"))? as usize;
        let mut names = count
            .checked_mul(MARKER_ENTRY_BYTES)
            .and_then(|entries| entries.checked_add(BLOB_HEADER_BYTES))
            .filter(|&end| end <= bytes.len())
            .ok_or(FormatError::Bounds("marker table"))?;
        let mut markers = Vec::with_capacity(count);
        for index in 0..count {
            let at = BLOB_HEADER_BYTES + index * MARKER_ENTRY_BYTES;
            let field = |offset| u32_at(bytes, at + offset).ok_or(FormatError::Bounds("marker"));
            let hash = field(0)?;
            let priority =
                Priority::from_byte(bytes[at + 4]).ok_or(FormatError::Invalid("priority"))?;
            let flags = bytes[at + 5];
            let name_len = usize::from(u16_at(bytes, at + 6).ok_or(FormatError::Bounds("marker"))?);
            let name = bytes
                .get(names..names + name_len)
                .and_then(|name| core::str::from_utf8(name).ok())
                .ok_or(FormatError::Bounds("marker name"))?;
            names += name_len;
            let segment = range(field(8)?, field(12)?, bytes.len())
                .map(|range| &bytes[range])
                .ok_or(FormatError::Bounds("segment"))?;
            Segment::parse(segment)?;
            if markers
                .last()
                .is_some_and(|last: &Marker<'_>| last.hash >= hash)
                || hash == AVAILABLE_LOCALES_HASH
            {
                return Err(FormatError::Order("marker hash"));
            }
            markers.push(Marker {
                hash,
                priority,
                flags,
                name,
                segment,
            });
        }
        Ok(Self { markers })
    }

    #[must_use]
    pub fn markers(&self) -> &[Marker<'a>] {
        &self.markers
    }
}

/// Builds a blob from marker segments.
#[derive(Debug, Default)]
pub struct BlobWriter {
    markers: Vec<(u32, Priority, u8, String, Vec<u8>)>,
}

impl BlobWriter {
    pub fn insert(
        &mut self,
        hash: u32,
        priority: Priority,
        flags: u8,
        name: &str,
        segment: Vec<u8>,
    ) {
        self.markers
            .push((hash, priority, flags, name.into(), segment));
    }

    /// The blob, markers sorted by hash.
    pub fn finish(mut self) -> Result<Vec<u8>, FormatError> {
        self.markers.sort_by_key(|marker| marker.0);
        if self.markers.windows(2).any(|pair| pair[0].0 == pair[1].0)
            || self
                .markers
                .iter()
                .any(|marker| marker.0 == AVAILABLE_LOCALES_HASH)
        {
            return Err(FormatError::Order("marker hash"));
        }
        let names: usize = self.markers.iter().map(|marker| marker.3.len()).sum();
        let mut offset = BLOB_HEADER_BYTES + self.markers.len() * MARKER_ENTRY_BYTES + names;
        let mut bytes = Vec::with_capacity(offset);
        bytes.extend_from_slice(&BLOB_MAGIC);
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        push_len(&mut bytes, self.markers.len())?;
        for (hash, priority, flags, name, segment) in &self.markers {
            bytes.extend_from_slice(&hash.to_le_bytes());
            bytes.push(*priority as u8);
            bytes.push(*flags);
            let name_len =
                u16::try_from(name.len()).map_err(|_| FormatError::Bounds("marker name"))?;
            bytes.extend_from_slice(&name_len.to_le_bytes());
            push_len(&mut bytes, offset)?;
            push_len(&mut bytes, segment.len())?;
            offset += segment.len();
        }
        for marker in &self.markers {
            bytes.extend_from_slice(marker.3.as_bytes());
        }
        for marker in &self.markers {
            bytes.extend_from_slice(&marker.4);
        }
        Ok(bytes)
    }
}

fn push_len(bytes: &mut Vec<u8>, len: usize) -> Result<(), FormatError> {
    let len = u32::try_from(len).map_err(|_| FormatError::Bounds("length"))?;
    bytes.extend_from_slice(&len.to_le_bytes());
    Ok(())
}

/// A validated segment held in memory (the linker's view).
#[derive(Debug, Clone, Copy)]
pub struct Segment<'a> {
    bytes: &'a [u8],
    count: usize,
}

impl<'a> Segment<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, FormatError> {
        let count = u32_at(bytes, 0).ok_or(FormatError::Bounds("segment header"))? as usize;
        u64_at(bytes, 8).ok_or(FormatError::Bounds("segment header"))?;
        count
            .checked_mul(ENTRY_BYTES)
            .and_then(|entries| entries.checked_add(SEGMENT_HEADER_BYTES))
            .filter(|&end| end <= bytes.len())
            .ok_or(FormatError::Bounds("segment entries"))?;
        let segment = Self { bytes, count };
        let mut previous: Option<&[u8]> = None;
        for index in 0..count {
            let (key, _) = segment
                .entry(index)
                .ok_or(FormatError::Bounds("segment entry"))?;
            if key.len() > MAX_KEY_BYTES || key.is_empty() {
                return Err(FormatError::Invalid("key length"));
            }
            if previous.is_some_and(|previous| previous >= key) {
                return Err(FormatError::Order("key"));
            }
            previous = Some(key);
        }
        Ok(segment)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// The flags and checksum of the header.
    #[must_use]
    pub fn checksum(&self) -> Option<u64> {
        let flags = u32_at(self.bytes, 4)?;
        (flags & segment_flag::CHECKSUM != 0)
            .then(|| u64_at(self.bytes, 8))
            .flatten()
    }

    /// Entry `index`: its key and payload.
    #[must_use]
    pub fn entry(&self, index: usize) -> Option<(&'a [u8], &'a [u8])> {
        if index >= self.count {
            return None;
        }
        let at = SEGMENT_HEADER_BYTES + index * ENTRY_BYTES;
        let key = range(
            u32_at(self.bytes, at)?,
            u32_at(self.bytes, at + 4)?,
            self.bytes.len(),
        )?;
        let payload = range(
            u32_at(self.bytes, at + 8)?,
            u32_at(self.bytes, at + 12)?,
            self.bytes.len(),
        )?;
        Some((&self.bytes[key], &self.bytes[payload]))
    }

    pub fn entries(&self) -> impl Iterator<Item = (&'a [u8], &'a [u8])> + '_ {
        (0..self.count).filter_map(|index| self.entry(index))
    }
}

/// Builds a segment: entries sorted by key, equal payloads stored once.
#[derive(Debug, Default)]
pub struct SegmentWriter {
    checksum: Option<u64>,
    entries: Vec<(Vec<u8>, Vec<u8>)>,
}

impl SegmentWriter {
    #[must_use]
    pub fn new(checksum: Option<u64>) -> Self {
        Self {
            checksum,
            entries: Vec::new(),
        }
    }

    pub fn insert(&mut self, key: Vec<u8>, payload: Vec<u8>) {
        self.entries.push((key, payload));
    }

    pub fn finish(mut self) -> Result<Vec<u8>, FormatError> {
        self.entries.sort();
        if self.entries.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(FormatError::Order("key"));
        }
        if self
            .entries
            .iter()
            .any(|(key, _)| key.is_empty() || key.len() > MAX_KEY_BYTES)
        {
            return Err(FormatError::Invalid("key length"));
        }
        let mut keys = SEGMENT_HEADER_BYTES + self.entries.len() * ENTRY_BYTES;
        let mut payload_at = keys + self.entries.iter().map(|(key, _)| key.len()).sum::<usize>();
        // Payloads in first-use order of their sorted keys, each once.
        let mut payloads: Vec<&[u8]> = Vec::new();
        let mut stored: BTreeMap<&[u8], usize> = BTreeMap::new();
        let mut placed = Vec::with_capacity(self.entries.len());
        for (_, payload) in &self.entries {
            let offset = *stored.entry(payload).or_insert_with(|| {
                payloads.push(payload);
                payload_at += payload.len();
                payload_at - payload.len()
            });
            placed.push(offset);
        }
        let mut bytes = Vec::with_capacity(payload_at);
        push_len(&mut bytes, self.entries.len())?;
        let flags = if self.checksum.is_some() {
            segment_flag::CHECKSUM
        } else {
            0
        };
        bytes.extend_from_slice(&flags.to_le_bytes());
        bytes.extend_from_slice(&self.checksum.unwrap_or(0).to_le_bytes());
        for ((key, payload), offset) in self.entries.iter().zip(&placed) {
            push_len(&mut bytes, keys)?;
            push_len(&mut bytes, key.len())?;
            push_len(&mut bytes, *offset)?;
            push_len(&mut bytes, payload.len())?;
            keys += key.len();
        }
        for (key, _) in &self.entries {
            bytes.extend_from_slice(key);
        }
        for payload in payloads {
            bytes.extend_from_slice(payload);
        }
        Ok(bytes)
    }
}

/// Reads parts of a segment that need not be in memory (linked code reads
/// a passive segment through the `intl_data` accessor).
pub trait SegmentSource {
    /// Fill `dest` from the segment at `offset`; false when the range is
    /// outside the segment.
    fn read(&self, offset: u32, dest: &mut [u8]) -> bool;
}

impl SegmentSource for [u8] {
    fn read(&self, offset: u32, dest: &mut [u8]) -> bool {
        match range(offset, dest.len() as u32, self.len()) {
            Some(range) => {
                dest.copy_from_slice(&self[range]);
                true
            }
            None => false,
        }
    }
}

/// The ICU4X data markers each linked operation reads, by marker name (the
/// blobs' marker names). The linker links exactly these segments with the
/// operation; `varta-intl`'s tests check each list against the markers
/// the operation's code loads (complete and each one read).
pub const OPERATION_MARKERS: &[(&str, &[&str])] = &[(
    "date_zone_name",
    &[
        "TimezoneIdentifiersIanaCoreV1",
        "TimezonePeriodsV1",
        "TimezoneNamesEssentialsV1",
        "TimezoneNamesLocationsOverrideV1",
        "TimezoneNamesLocationsRootV1",
        "TimezoneNamesSpecificLongV1",
        "TimezoneNamesStandardLongV1",
        "DecimalSymbolsV1",
        "DecimalDigitsV1",
    ],
)];

/// The markers the linked operation `name` reads (none for most).
#[must_use]
pub fn operation_markers(name: &str) -> &'static [&'static str] {
    OPERATION_MARKERS
        .iter()
        .find(|(operation, _)| *operation == name)
        .map_or(&[], |(_, markers)| markers)
}

/// The key of a data identifier.
#[must_use]
pub fn key(locale: &str, attributes: &str) -> Vec<u8> {
    let mut key = Vec::with_capacity(locale.len() + 1 + attributes.len());
    key.extend_from_slice(locale.as_bytes());
    if !attributes.is_empty() {
        key.push(ATTRIBUTE_SEPARATOR);
        key.extend_from_slice(attributes.as_bytes());
    }
    key
}

/// A payload found by [`lookup`]: its range in the segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Found {
    pub offset: u32,
    pub len: u32,
}

/// The header of a segment read through a [`SegmentSource`]: entry count
/// and checksum.
pub fn header(source: &(impl SegmentSource + ?Sized)) -> Option<(u32, Option<u64>)> {
    let mut header = [0; SEGMENT_HEADER_BYTES];
    if !source.read(0, &mut header) {
        return None;
    }
    let count = u32_at(&header, 0)?;
    let flags = u32_at(&header, 4)?;
    let checksum = (flags & segment_flag::CHECKSUM != 0)
        .then(|| u64_at(&header, 8))
        .flatten();
    Some((count, checksum))
}

/// The payload of `key` in a segment of `count` entries: the exact key, or
/// with `prefix_match` the first key that extends it with more attributes
/// (ICU4X `attributes_prefix_match`: the same locale, attributes starting
/// with the requested ones). Reads only the probed entries and keys.
pub fn lookup(
    source: &(impl SegmentSource + ?Sized),
    count: u32,
    locale: &str,
    attributes: &str,
    prefix_match: bool,
) -> Option<Found> {
    let wanted = key(locale, attributes);
    let mut low = 0_u32;
    let mut high = count;
    let mut buffer = [0_u8; MAX_KEY_BYTES];
    let probe = |index: u32, buffer: &mut [u8; MAX_KEY_BYTES]| -> Option<(usize, Found)> {
        let mut entry = [0; ENTRY_BYTES];
        let at = u32::try_from(SEGMENT_HEADER_BYTES)
            .ok()?
            .checked_add(index.checked_mul(ENTRY_BYTES as u32)?)?;
        if !source.read(at, &mut entry) {
            return None;
        }
        let key_len = u32_at(&entry, 4)? as usize;
        if key_len > MAX_KEY_BYTES || !source.read(u32_at(&entry, 0)?, &mut buffer[..key_len]) {
            return None;
        }
        Some((
            key_len,
            Found {
                offset: u32_at(&entry, 8)?,
                len: u32_at(&entry, 12)?,
            },
        ))
    };
    // Lower bound of the wanted key.
    while low < high {
        let middle = low + (high - low) / 2;
        let (len, _) = probe(middle, &mut buffer)?;
        if buffer[..len] < wanted[..] {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    if low >= count {
        return None;
    }
    let (len, found) = probe(low, &mut buffer)?;
    let stored = &buffer[..len];
    let matches = stored == &wanted[..]
        || (prefix_match
            && stored.starts_with(&wanted)
            && (!attributes.is_empty() || stored.get(wanted.len()) == Some(&ATTRIBUTE_SEPARATOR)));
    matches.then_some(found)
}

/// What a module's linked locale data is ([`SECTION`]): the data versions,
/// the project's locale selection (empty: every locale) and the marker of
/// each passive segment, in data segment order after the active ones.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Manifest {
    pub cldr: String,
    pub icu4x: String,
    pub tzdata: String,
    pub locales: Vec<String>,
    pub segments: Vec<u32>,
}

/// The largest [`Manifest`] text field.
pub const MAX_MANIFEST_TEXT: usize = 256;
/// The most locales a project may select.
pub const MAX_LOCALES: usize = 1024;
/// The most passive segments of a module (every marker of every service
/// plus the available-locales segment stays far below it).
pub const MAX_SEGMENTS: usize = 4096;

impl Manifest {
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        for text in [&self.cldr, &self.icu4x, &self.tzdata] {
            push_text(&mut bytes, text);
        }
        bytes.extend_from_slice(&(self.locales.len() as u32).to_le_bytes());
        for locale in &self.locales {
            push_text(&mut bytes, locale);
        }
        bytes.extend_from_slice(&(self.segments.len() as u32).to_le_bytes());
        for hash in &self.segments {
            bytes.extend_from_slice(&hash.to_le_bytes());
        }
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, FormatError> {
        let mut at = 0;
        let word = |at: &mut usize| {
            let value = u32_at(bytes, *at).ok_or(FormatError::Bounds("manifest"));
            *at += 4;
            value
        };
        if word(&mut at)? != FORMAT_VERSION {
            return Err(FormatError::Version);
        }
        let text = |at: &mut usize| -> Result<String, FormatError> {
            let len = usize::from(u16_at(bytes, *at).ok_or(FormatError::Bounds("manifest"))?);
            let text = bytes
                .get(*at + 2..*at + 2 + len)
                .filter(|text| text.len() <= MAX_MANIFEST_TEXT && text.is_ascii())
                .ok_or(FormatError::Invalid("manifest text"))?;
            *at += 2 + len;
            Ok(String::from_utf8_lossy(text).into_owned())
        };
        let cldr = text(&mut at)?;
        let icu4x = text(&mut at)?;
        let tzdata = text(&mut at)?;
        let count = word(&mut at)? as usize;
        if count > MAX_LOCALES {
            return Err(FormatError::Invalid("locale count"));
        }
        let locales = (0..count)
            .map(|_| text(&mut at))
            .collect::<Result<Vec<_>, _>>()?;
        let count = word(&mut at)? as usize;
        if count > MAX_SEGMENTS {
            return Err(FormatError::Invalid("segment count"));
        }
        let segments = (0..count)
            .map(|_| word(&mut at))
            .collect::<Result<Vec<_>, _>>()?;
        if at != bytes.len() {
            return Err(FormatError::Bounds("manifest trailer"));
        }
        let mut sorted = segments.clone();
        sorted.sort_unstable();
        if sorted.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(FormatError::Order("segment marker"));
        }
        Ok(Self {
            cldr,
            icu4x,
            tzdata,
            locales,
            segments,
        })
    }
}

fn push_text(bytes: &mut Vec<u8>, text: &str) {
    let len = text.len().min(MAX_MANIFEST_TEXT);
    bytes.extend_from_slice(&(len as u16).to_le_bytes());
    bytes.extend_from_slice(&text.as_bytes()[..len]);
}

/// The available-locales segment: the selected locale tags joined by
/// `\n` (no ICU4X segment layout; read whole).
#[must_use]
pub fn available_locales_segment(locales: &[String]) -> Vec<u8> {
    locales.join("\n").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{string::ToString, vec};

    fn segment() -> Vec<u8> {
        let mut writer = SegmentWriter::new(Some(7));
        for (locale, attributes, payload) in [
            ("en", "", b"A".as_slice()),
            ("en-GB", "", b"B"),
            ("en", "gregory", b"C"),
            ("und", "", b"A"),
            ("en", "gregory-x", b"D"),
        ] {
            writer.insert(key(locale, attributes), payload.to_vec());
        }
        writer.finish().unwrap()
    }

    #[test]
    fn segments_find_exact_and_prefix_keys_and_share_payloads() {
        let bytes = segment();
        let segment = Segment::parse(&bytes).unwrap();
        assert_eq!(segment.len(), 5);
        assert_eq!(segment.checksum(), Some(7));
        let (count, checksum) = header(&bytes[..]).unwrap();
        assert_eq!((count, checksum), (5, Some(7)));
        let payload = |found: Found| bytes[found.offset as usize..][..found.len as usize].to_vec();
        let find =
            |locale, attributes, prefix| lookup(&bytes[..], count, locale, attributes, prefix);
        assert_eq!(payload(find("en", "", false).unwrap()), b"A");
        assert_eq!(payload(find("en-GB", "", false).unwrap()), b"B");
        assert_eq!(payload(find("en", "gregory", false).unwrap()), b"C");
        assert_eq!(find("en", "greg", false), None);
        assert_eq!(payload(find("en", "greg", true).unwrap()), b"C");
        assert_eq!(find("fr", "", false), None);
        assert_eq!(find("zz", "", true), None);
        // `en` and `und` share their payload.
        assert_eq!(find("en", "", false), find("und", "", false));
        // An empty attribute prefix matches the locale only.
        assert_eq!(payload(find("en-GB", "", true).unwrap()), b"B");
        assert_eq!(find("en-G", "", true), None);
    }

    #[test]
    fn blobs_round_trip_and_reject_damage() {
        let mut writer = BlobWriter::default();
        writer.insert(9, Priority::Region, 0, "ZetaV1", segment());
        writer.insert(
            3,
            Priority::Language,
            marker_flag::SINGLETON,
            "AlphaV1",
            segment(),
        );
        let bytes = writer.finish().unwrap();
        let blob = Blob::parse(&bytes).unwrap();
        let names: Vec<_> = blob.markers().iter().map(|m| (m.hash, m.name)).collect();
        assert_eq!(names, vec![(3, "AlphaV1"), (9, "ZetaV1")]);
        assert_eq!(blob.markers()[1].priority, Priority::Region);
        assert_eq!(blob.markers()[0].segment, &segment()[..]);
        let mut damaged = bytes.clone();
        damaged[4] = 9;
        assert_eq!(Blob::parse(&damaged).unwrap_err(), FormatError::Version);
        assert!(Blob::parse(&bytes[..bytes.len() - 1]).is_err());
        let mut duplicate = BlobWriter::default();
        duplicate.insert(3, Priority::Language, 0, "A", segment());
        duplicate.insert(3, Priority::Language, 0, "B", segment());
        assert!(duplicate.finish().is_err());
    }

    #[test]
    fn manifests_round_trip() {
        let manifest = Manifest {
            cldr: "49.0.0".into(),
            icu4x: "2.4.0".into(),
            tzdata: "2026e".into(),
            locales: vec!["en".to_string(), "uk".to_string()],
            segments: vec![5, 0, 9],
        };
        assert_eq!(Manifest::decode(&manifest.encode()).unwrap(), manifest);
        let mut bytes = manifest.encode();
        bytes.push(0);
        assert!(Manifest::decode(&bytes).is_err());
        let duplicate = Manifest {
            segments: vec![5, 5],
            ..manifest
        };
        assert!(Manifest::decode(&duplicate.encode()).is_err());
    }
}
