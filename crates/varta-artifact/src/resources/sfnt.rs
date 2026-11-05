//! Structural admission of one OpenType/TrueType face: a bounded table
//! directory and the `OS/2` and `name` facts a resource descriptor declares.
//! Complete outline and shaping-table parsing is the renderer's job; the
//! native shell parses every face with its text shaper before guest start.
use super::{FaceError, FontStyle};
use std::collections::BTreeSet;

/// sfnt version tags admitted: TrueType outlines and CFF outlines. Font
/// collections and WOFF/WOFF2 containers are refused, because the native
/// shaper reads neither.
const TRUETYPE: Tag = [0, 1, 0, 0];
const OPENTYPE_CFF: Tag = *b"OTTO";
/// The offset table: sfnt version, table count and three search fields.
const OFFSET_TABLE_BYTES: usize = 12;
const TABLE_COUNT_AT: usize = 4;
/// One table record: tag, checksum, offset and length.
const TABLE_RECORD_BYTES: usize = 16;
/// A table tag: four bytes at the start of its record.
const TAG_BYTES: usize = 4;
type Tag = [u8; TAG_BYTES];
const RECORD_OFFSET_AT: usize = 8;
const RECORD_LENGTH_AT: usize = 12;
const MAX_TABLES: usize = 256;
/// Tables are 4-byte aligned in the file.
const TABLE_ALIGNMENT: usize = 4;
/// Tables every admitted face must contain.
const REQUIRED_TABLES: [&Tag; 4] = [b"cmap", b"head", b"name", b"OS/2"];
const OS2: &Tag = b"OS/2";
const NAME: &Tag = b"name";
/// `OS/2` fields: `usWeightClass` and `fsSelection`.
const OS2_WEIGHT_AT: usize = 4;
const OS2_SELECTION_AT: usize = 62;
const OS2_MINIMUM_BYTES: usize = OS2_SELECTION_AT + size_of::<u16>();
/// `fsSelection` bits that make a face italic (`ITALIC`) or oblique.
const SELECTION_ITALIC: u16 = 1;
const SELECTION_OBLIQUE: u16 = 1 << 9;
/// `name` table header (format, count, string offset) and one record.
const NAME_HEADER_BYTES: usize = 6;
const NAME_COUNT_AT: usize = 2;
const NAME_STORAGE_AT: usize = 4;
const NAME_RECORD_BYTES: usize = 12;
const NAME_RECORD_PLATFORM_AT: usize = 0;
const NAME_RECORD_ENCODING_AT: usize = 2;
const NAME_RECORD_ID_AT: usize = 6;
const NAME_RECORD_LENGTH_AT: usize = 8;
const NAME_RECORD_OFFSET_AT: usize = 10;
/// Name ids of the family: the typographic family wins over the legacy one.
const NAME_FAMILY: u16 = 1;
const NAME_TYPOGRAPHIC_FAMILY: u16 = 16;
/// Platforms whose strings are UTF-16BE (Unicode, Windows) and the Mac
/// platform with its Roman encoding, read only when every byte is ASCII.
const PLATFORM_UNICODE: u16 = 0;
const PLATFORM_MAC: u16 = 1;
const PLATFORM_WINDOWS: u16 = 3;
const MAC_ROMAN: u16 = 0;
const WINDOWS_ENCODINGS: [u16; 3] = [0, 1, 10];

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let end = at.checked_add(size_of::<u16>())?;
    Some(u16::from_be_bytes(bytes.get(at..end)?.try_into().ok()?))
}
fn u32_at(bytes: &[u8], at: usize) -> Option<usize> {
    let end = at.checked_add(size_of::<u32>())?;
    usize::try_from(u32::from_be_bytes(bytes.get(at..end)?.try_into().ok()?)).ok()
}

/// The admitted facts of one face.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Face {
    pub weight: u16,
    pub style: FontStyle,
    /// The family names the face declares (typographic family when present,
    /// otherwise the legacy family).
    pub families: Vec<String>,
}

/// Admit one face: bounded, non-overlapping, unique tables inside the file,
/// the required tables, and readable weight, style and family names.
pub(super) fn inspect(bytes: &[u8]) -> Result<Face, FaceError> {
    let flavor = bytes.get(..TRUETYPE.len()).ok_or(FaceError::Container)?;
    if flavor != TRUETYPE && flavor != OPENTYPE_CFF {
        return Err(FaceError::Container);
    }
    let count = usize::from(u16_at(bytes, TABLE_COUNT_AT).ok_or(FaceError::Container)?);
    let directory_end = count
        .checked_mul(TABLE_RECORD_BYTES)
        .and_then(|records| records.checked_add(OFFSET_TABLE_BYTES))
        .filter(|end| (1..=MAX_TABLES).contains(&count) && *end <= bytes.len())
        .ok_or(FaceError::Tables)?;
    let mut tags = BTreeSet::new();
    let mut spans = Vec::with_capacity(count);
    let mut os2 = None;
    let mut name = None;
    for record in (OFFSET_TABLE_BYTES..directory_end).step_by(TABLE_RECORD_BYTES) {
        let tag: Tag = bytes[record..record + TAG_BYTES]
            .try_into()
            .map_err(|_| FaceError::Tables)?;
        let offset = u32_at(bytes, record + RECORD_OFFSET_AT).ok_or(FaceError::Tables)?;
        let length = u32_at(bytes, record + RECORD_LENGTH_AT).ok_or(FaceError::Tables)?;
        let end = offset
            .checked_add(length)
            .filter(|end| *end <= bytes.len())
            .ok_or(FaceError::Tables)?;
        if !tags.insert(tag) || offset < directory_end || !offset.is_multiple_of(TABLE_ALIGNMENT) {
            return Err(FaceError::Tables);
        }
        if length > 0 {
            spans.push((offset, end));
        }
        if &tag == OS2 {
            os2 = Some(&bytes[offset..end]);
        } else if &tag == NAME {
            name = Some(&bytes[offset..end]);
        }
    }
    spans.sort_unstable();
    if spans.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(FaceError::Tables);
    }
    if let Some(missing) = REQUIRED_TABLES.iter().find(|tag| !tags.contains(**tag)) {
        return Err(FaceError::MissingTable(
            String::from_utf8_lossy(missing.as_slice()).into_owned(),
        ));
    }
    let os2 = os2
        .filter(|table| table.len() >= OS2_MINIMUM_BYTES)
        .ok_or(FaceError::Os2)?;
    let weight = u16_at(os2, OS2_WEIGHT_AT).ok_or(FaceError::Os2)?;
    let selection = u16_at(os2, OS2_SELECTION_AT).ok_or(FaceError::Os2)?;
    let style = if selection & (SELECTION_ITALIC | SELECTION_OBLIQUE) == 0 {
        FontStyle::Normal
    } else {
        FontStyle::Italic
    };
    let families = families(name.ok_or(FaceError::Names)?)?;
    Ok(Face {
        weight,
        style,
        families,
    })
}

/// The face's family names: every readable typographic family name, or the
/// legacy family names when it has none.
fn families(table: &[u8]) -> Result<Vec<String>, FaceError> {
    if table.len() < NAME_HEADER_BYTES {
        return Err(FaceError::Names);
    }
    let count = usize::from(u16_at(table, NAME_COUNT_AT).ok_or(FaceError::Names)?);
    let storage = usize::from(u16_at(table, NAME_STORAGE_AT).ok_or(FaceError::Names)?);
    let records_end = count
        .checked_mul(NAME_RECORD_BYTES)
        .and_then(|records| records.checked_add(NAME_HEADER_BYTES))
        .filter(|end| *end <= table.len() && storage <= table.len())
        .ok_or(FaceError::Names)?;
    let mut typographic = Vec::new();
    let mut legacy = Vec::new();
    for record in (NAME_HEADER_BYTES..records_end).step_by(NAME_RECORD_BYTES) {
        let field = |at| u16_at(table, record + at).ok_or(FaceError::Names);
        let id = field(NAME_RECORD_ID_AT)?;
        let target = match id {
            NAME_FAMILY => &mut legacy,
            NAME_TYPOGRAPHIC_FAMILY => &mut typographic,
            _ => continue,
        };
        let start = storage
            .checked_add(usize::from(field(NAME_RECORD_OFFSET_AT)?))
            .ok_or(FaceError::Names)?;
        let end = start
            .checked_add(usize::from(field(NAME_RECORD_LENGTH_AT)?))
            .filter(|end| *end <= table.len())
            .ok_or(FaceError::Names)?;
        let text = &table[start..end];
        let decoded = match (
            field(NAME_RECORD_PLATFORM_AT)?,
            field(NAME_RECORD_ENCODING_AT)?,
        ) {
            (PLATFORM_UNICODE, _) => utf16(text),
            (PLATFORM_WINDOWS, encoding) if WINDOWS_ENCODINGS.contains(&encoding) => utf16(text),
            (PLATFORM_MAC, MAC_ROMAN) if text.is_ascii() => {
                Some(String::from_utf8_lossy(text).into_owned())
            }
            _ => None,
        };
        if let Some(decoded) = decoded {
            target.push(decoded);
        }
    }
    let families = if typographic.is_empty() {
        legacy
    } else {
        typographic
    };
    if families.is_empty() {
        return Err(FaceError::Names);
    }
    Ok(families)
}

/// Big-endian UTF-16 text; `None` for an odd length or a lone surrogate.
fn utf16(bytes: &[u8]) -> Option<String> {
    let (pairs, rest) = bytes.as_chunks::<{ size_of::<u16>() }>();
    if !rest.is_empty() {
        return None;
    }
    let units = pairs.iter().map(|pair| u16::from_be_bytes(*pair));
    char::decode_utf16(units)
        .collect::<Result<String, _>>()
        .ok()
}
