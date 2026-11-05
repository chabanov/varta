//! Bounded reads of the explicit font bytes: one collection member's tables,
//! its `fvar` axes and `avar` maps, rebuilt as a standalone face for CoreText.
//! Every offset and length is checked against the supplied slice.

/// OpenType collection header tag.
const COLLECTION: [u8; 4] = *b"ttcf";
/// Sanity ceiling for a face's table directory; real faces use a few dozen.
const MAX_TABLES: usize = 256;
/// Variation axes accepted for platform instantiation.
pub(super) const MAX_AXES: usize = 64;
/// `avar` segment-map pairs accepted per axis.
const MAX_SEGMENTS: usize = 256;
const OFFSET_TABLE_BYTES: usize = 12;
const TABLE_RECORD_BYTES: usize = 16;
const FVAR_AXIS_BYTES: usize = 20;
const NAME_RECORD_BYTES: usize = 12;
/// F2Dot14 and 16.16 fixed-point scales.
const F2DOT14_ONE: f64 = 16384.0;
const FIXED_ONE: f64 = 65536.0;
const WINDOWS_PLATFORM: u16 = 3;
const UNICODE_PLATFORM: u16 = 0;
const ENGLISH_US: u16 = 0x0409;

fn u16_at(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        data.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}
fn i16_at(data: &[u8], offset: usize) -> Option<i16> {
    u16_at(data, offset).map(|v| v as i16)
}
fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        data.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}
fn fixed_at(data: &[u8], offset: usize) -> Option<f64> {
    u32_at(data, offset).map(|v| f64::from(v as i32) / FIXED_ONE)
}
fn f2dot14_at(data: &[u8], offset: usize) -> Option<f64> {
    i16_at(data, offset).map(|v| f64::from(v) / F2DOT14_ONE)
}

struct Record {
    tag: [u8; 4],
    checksum: u32,
    range: std::ops::Range<usize>,
}

/// One face of a font file or collection.
pub(super) struct Face<'a> {
    data: &'a [u8],
    version: u32,
    tables: Vec<Record>,
    collection: bool,
}
impl<'a> Face<'a> {
    /// The face at `index`: a collection member, or index 0 of a single face.
    pub fn parse(data: &'a [u8], index: u32) -> Option<Self> {
        let collection = data.get(..4) == Some(COLLECTION.as_slice());
        let start = if collection {
            let count = u32_at(data, 8)?;
            if index >= count {
                return None;
            }
            usize::try_from(u32_at(
                data,
                12usize.checked_add(4 * usize::try_from(index).ok()?)?,
            )?)
            .ok()?
        } else if index == 0 {
            0
        } else {
            return None;
        };
        let version = u32_at(data, start)?;
        let count = usize::from(u16_at(data, start.checked_add(4)?)?);
        if count == 0 || count > MAX_TABLES {
            return None;
        }
        let mut tables = Vec::with_capacity(count);
        for i in 0..count {
            let at = start
                .checked_add(OFFSET_TABLE_BYTES)?
                .checked_add(i * TABLE_RECORD_BYTES)?;
            let tag: [u8; 4] = data.get(at..at.checked_add(4)?)?.try_into().ok()?;
            let offset = usize::try_from(u32_at(data, at + 8)?).ok()?;
            let length = usize::try_from(u32_at(data, at + 12)?).ok()?;
            let end = offset.checked_add(length)?;
            if end > data.len() || tables.iter().any(|t: &Record| t.tag == tag) {
                return None;
            }
            tables.push(Record {
                tag,
                checksum: u32_at(data, at + 4)?,
                range: offset..end,
            });
        }
        tables.sort_by_key(|t| t.tag);
        Some(Self {
            data,
            version,
            tables,
            collection,
        })
    }
    pub fn table(&self, tag: [u8; 4]) -> Option<&'a [u8]> {
        let record = self.tables.iter().find(|t| t.tag == tag)?;
        self.data.get(record.range.clone())
    }
    pub fn has_table(&self, tag: [u8; 4]) -> bool {
        self.tables.iter().any(|t| t.tag == tag)
    }
    /// The whole supplied file.
    pub fn bytes(&self) -> &'a [u8] {
        self.data
    }
    /// A single face already is its own standalone file.
    pub fn is_standalone(&self) -> bool {
        !self.collection
    }
    /// The same tables in a standalone sfnt: table bytes and checksums are
    /// copied unchanged, so the face's outlines and metrics are identical.
    pub fn standalone(&self) -> Option<Vec<u8>> {
        let count = self.tables.len();
        let header = OFFSET_TABLE_BYTES.checked_add(count.checked_mul(TABLE_RECORD_BYTES)?)?;
        let mut size = header;
        for table in &self.tables {
            size = size.checked_add(table.range.len().checked_next_multiple_of(4)?)?;
        }
        let mut out = Vec::new();
        out.try_reserve_exact(size).ok()?;
        let selector = count.ilog2();
        let search = (1usize << selector) * TABLE_RECORD_BYTES;
        out.extend_from_slice(&self.version.to_be_bytes());
        for value in [
            count,
            search,
            selector as usize,
            count * TABLE_RECORD_BYTES - search,
        ] {
            out.extend_from_slice(&u16::try_from(value).ok()?.to_be_bytes());
        }
        let mut offset = header;
        for table in &self.tables {
            out.extend_from_slice(&table.tag);
            out.extend_from_slice(&table.checksum.to_be_bytes());
            out.extend_from_slice(&u32::try_from(offset).ok()?.to_be_bytes());
            out.extend_from_slice(&u32::try_from(table.range.len()).ok()?.to_be_bytes());
            offset += table.range.len().checked_next_multiple_of(4)?;
        }
        for table in &self.tables {
            out.extend_from_slice(self.data.get(table.range.clone())?);
            out.resize(out.len().checked_next_multiple_of(4)?, 0);
        }
        (out.len() == size).then_some(out)
    }
}

/// One `fvar` axis in user coordinates, with its `avar` segment map.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Axis {
    pub tag: [u8; 4],
    pub name: String,
    pub min: f64,
    pub default: f64,
    pub max: f64,
    /// (from, to) pairs in normalized coordinates; empty means identity.
    pub segments: Vec<(f64, f64)>,
}

/// Why a face's variation data cannot be instantiated exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VariationError {
    Malformed,
    /// `avar` version 2 maps axes jointly; the per-axis inverse does not apply.
    UnsupportedAvar,
}

/// The face's axes, or an empty list for a static face.
pub(super) fn axes(face: &Face<'_>) -> Result<Vec<Axis>, VariationError> {
    let Some(fvar) = face.table(*b"fvar") else {
        return Ok(Vec::new());
    };
    let malformed = VariationError::Malformed;
    let array = usize::from(u16_at(fvar, 4).ok_or(malformed)?);
    let count = usize::from(u16_at(fvar, 8).ok_or(malformed)?);
    let size = usize::from(u16_at(fvar, 10).ok_or(malformed)?);
    if count > MAX_AXES || size < FVAR_AXIS_BYTES {
        return Err(malformed);
    }
    let names = face.table(*b"name");
    let mut axes = Vec::with_capacity(count);
    for index in 0..count {
        let at = array + index * size;
        let tag: [u8; 4] = fvar
            .get(at..at + 4)
            .and_then(|tag| tag.try_into().ok())
            .ok_or(malformed)?;
        let min = fixed_at(fvar, at + 4).ok_or(malformed)?;
        let default = fixed_at(fvar, at + 8).ok_or(malformed)?;
        let max = fixed_at(fvar, at + 12).ok_or(malformed)?;
        let name_id = u16_at(fvar, at + 18).ok_or(malformed)?;
        if !(min <= default && default <= max) {
            return Err(malformed);
        }
        let name = names
            .and_then(|table| name(table, name_id))
            .ok_or(malformed)?;
        axes.push(Axis {
            tag,
            name,
            min,
            default,
            max,
            segments: Vec::new(),
        });
    }
    if let Some(avar) = face.table(*b"avar") {
        if u16_at(avar, 0).ok_or(malformed)? != 1 {
            return Err(VariationError::UnsupportedAvar);
        }
        if usize::from(u16_at(avar, 6).ok_or(malformed)?) != axes.len() {
            return Err(malformed);
        }
        let mut at = 8;
        for axis in &mut axes {
            let pairs = usize::from(u16_at(avar, at).ok_or(malformed)?);
            if pairs > MAX_SEGMENTS {
                return Err(malformed);
            }
            at += 2;
            for _ in 0..pairs {
                let from = f2dot14_at(avar, at).ok_or(malformed)?;
                let to = f2dot14_at(avar, at + 2).ok_or(malformed)?;
                if axis
                    .segments
                    .last()
                    .is_some_and(|&(f, t)| from < f || to < t)
                {
                    return Err(malformed);
                }
                axis.segments.push((from, to));
                at += 4;
            }
        }
    }
    Ok(axes)
}

impl Axis {
    /// The user value whose normalization (default normalization, then this
    /// axis's `avar` map) yields `normalized`, as CoreText will apply it.
    pub fn user_value(&self, normalized: f64) -> f64 {
        let normalized = normalized.clamp(-1.0, 1.0);
        let default = self.unmap(normalized);
        if default < 0.0 {
            self.default + default * (self.default - self.min)
        } else {
            self.default + default * (self.max - self.default)
        }
    }
    /// Inverse of the piecewise-linear `avar` map. Flat segments map every
    /// preimage to the same value, so their start is an exact preimage.
    fn unmap(&self, value: f64) -> f64 {
        if self.segments.len() < 2 {
            return value;
        }
        for pair in self.segments.windows(2) {
            let ((f0, t0), (f1, t1)) = (pair[0], pair[1]);
            if value >= t0 && value <= t1 {
                if t1 == t0 {
                    return f0;
                }
                return f0 + (value - t0) * (f1 - f0) / (t1 - t0);
            }
        }
        value
    }
}

/// An English (or first Unicode) string for `id`, as AppKit names axes.
fn name(table: &[u8], id: u16) -> Option<String> {
    let count = usize::from(u16_at(table, 2)?);
    let strings = usize::from(u16_at(table, 4)?);
    let mut fallback = None;
    for index in 0..count {
        let at = 6 + index * NAME_RECORD_BYTES;
        let platform = u16_at(table, at)?;
        let language = u16_at(table, at + 4)?;
        if u16_at(table, at + 6)? != id || !matches!(platform, WINDOWS_PLATFORM | UNICODE_PLATFORM)
        {
            continue;
        }
        let length = usize::from(u16_at(table, at + 8)?);
        let offset = strings.checked_add(usize::from(u16_at(table, at + 10)?))?;
        let bytes = table.get(offset..offset.checked_add(length)?)?;
        let units = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes(*pair));
        let text = char::decode_utf16(units)
            .collect::<Result<String, _>>()
            .ok()?;
        if platform == WINDOWS_PLATFORM && language == ENGLISH_US {
            return Some(text);
        }
        fallback.get_or_insert(text);
    }
    fallback
}

#[cfg(test)]
mod tests {
    use super::*;
    const VARIABLE: &[u8] = include_bytes!("../../tests/fixtures/fonts/variable.ttf");
    const COLLECTION_FIXTURE: &[u8] = include_bytes!("../../tests/fixtures/fonts/collection.ttc");

    #[test]
    fn collection_members_rebuild_as_standalone_faces_with_identical_tables() {
        for index in 0..2 {
            let member = Face::parse(COLLECTION_FIXTURE, index).unwrap();
            assert!(!member.is_standalone());
            let bytes = member.standalone().unwrap();
            let rebuilt = Face::parse(&bytes, 0).unwrap();
            assert!(rebuilt.is_standalone());
            assert_eq!(rebuilt.tables.len(), member.tables.len());
            for table in &member.tables {
                assert_eq!(rebuilt.table(table.tag), member.table(table.tag));
            }
        }
        let zero = Face::parse(COLLECTION_FIXTURE, 0).unwrap();
        let one = Face::parse(COLLECTION_FIXTURE, 1).unwrap();
        assert_ne!(zero.table(*b"name"), one.table(*b"name"));
        assert!(Face::parse(COLLECTION_FIXTURE, 2).is_none());
        assert!(Face::parse(VARIABLE, 1).is_none());
    }

    #[test]
    fn truncated_and_out_of_range_directories_are_refused() {
        for end in [0, 4, 12, 20, COLLECTION_FIXTURE.len() / 2] {
            assert!(
                Face::parse(&COLLECTION_FIXTURE[..end], 1).is_none(),
                "{end}"
            );
        }
        let mut forged = VARIABLE.to_vec();
        // First table record's length now runs past the end of the file.
        forged[OFFSET_TABLE_BYTES + 12..OFFSET_TABLE_BYTES + 16]
            .copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(Face::parse(&forged, 0).is_none());
    }

    #[test]
    fn axes_read_fvar_names_and_invert_the_avar_map() {
        let face = Face::parse(VARIABLE, 0).unwrap();
        let axes = axes(&face).unwrap();
        assert_eq!(axes.len(), 1);
        let weight = &axes[0];
        assert_eq!((weight.tag, weight.name.as_str()), (*b"wght", "Weight"));
        assert_eq!(
            (weight.min, weight.default, weight.max),
            (100.0, 400.0, 900.0)
        );
        // The fixture maps 0.5 -> 0.75 and -0.5 -> -0.25.
        assert_eq!(weight.user_value(0.75), 650.0);
        assert_eq!(weight.user_value(-0.25), 250.0);
        assert_eq!(weight.user_value(0.0), 400.0);
        assert_eq!(weight.user_value(1.0), 900.0);
        assert_eq!(weight.user_value(-1.0), 100.0);
        // Between two map points the inverse is linear.
        assert_eq!(weight.user_value(0.875), 775.0);
        let member = Face::parse(COLLECTION_FIXTURE, 1).unwrap();
        assert!(super::axes(&member).unwrap().is_empty());
    }
}
