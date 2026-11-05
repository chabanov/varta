//! The versioned resource section of a packaged artifact: the font faces an
//! application ships, so every renderer loads the same faces.
//!
//! Section version 1 (all integers little-endian):
//!
//! ```text
//! magic "VARTARS\0" | version u32 | face count u32 | payload SHA-256 [32]
//! face count x descriptor:
//!   family length u16 | weight u16 | style u8 | generic bits u8 |
//!   reserved u16 (zero) | font length u32 | font SHA-256 [32]
//! face count x (family UTF-8 bytes | font bytes), in descriptor order
//! ```
//!
//! The payload digest binds the section to the portable Wasm it was packaged
//! with, so a section can never travel to another payload. Every face is
//! bounded, matches its recorded digest and declares exactly the family,
//! weight and style its own `name` and `OS/2` tables state. Faces are stored
//! in canonical order: the faces of one family are adjacent, families keep
//! their declaration order (the first family is the default) and a family's
//! faces ascend by weight, then style. Decoding refuses any other order, so
//! one declaration has exactly one encoding.
mod sfnt;

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// The section's magic bytes.
pub const RESOURCE_MAGIC: &[u8; 8] = b"VARTARS\0";
/// The only section version this build reads and writes.
pub const RESOURCE_VERSION: u32 = 1;
/// At most this many faces, the native text shaper's collection ceiling.
pub const MAX_FONT_FACES: usize = 16;
/// The largest single face, the native text shaper's per-font ceiling.
pub const MAX_FONT_FACE_BYTES: usize = 4 * 1024 * 1024;
/// The largest total of font bytes, the native text shaper's ceiling.
pub const MAX_FONT_BYTES: usize = 16 * 1024 * 1024;
/// The longest family name, as for browser package fonts.
pub const MAX_FAMILY_BYTES: usize = 64;
/// Lightest and heaviest admitted weights, and their step.
pub const MIN_FONT_WEIGHT: u16 = 100;
pub const MAX_FONT_WEIGHT: u16 = 900;
pub const FONT_WEIGHT_STEP: u16 = 100;
/// The weight of a face that declares none.
pub const NORMAL_FONT_WEIGHT: u16 = 400;

const DIGEST_BYTES: usize = 32;
const VERSION_AT: usize = RESOURCE_MAGIC.len();
const COUNT_AT: usize = VERSION_AT + size_of::<u32>();
const PAYLOAD_DIGEST_AT: usize = COUNT_AT + size_of::<u32>();
const HEADER_BYTES: usize = PAYLOAD_DIGEST_AT + DIGEST_BYTES;
const FAMILY_LENGTH_AT: usize = 0;
const WEIGHT_AT: usize = FAMILY_LENGTH_AT + size_of::<u16>();
const STYLE_AT: usize = WEIGHT_AT + size_of::<u16>();
const GENERICS_AT: usize = STYLE_AT + size_of::<u8>();
const DESCRIPTOR_RESERVED_AT: usize = GENERICS_AT + size_of::<u8>();
const FONT_LENGTH_AT: usize = DESCRIPTOR_RESERVED_AT + size_of::<u16>();
const FONT_DIGEST_AT: usize = FONT_LENGTH_AT + size_of::<u32>();
const DESCRIPTOR_BYTES: usize = FONT_DIGEST_AT + DIGEST_BYTES;
const DESCRIPTOR_RESERVED: u16 = 0;
/// The largest encoded section: header, every descriptor and family name at
/// its maximum, and the font byte ceiling.
pub const MAX_RESOURCE_SECTION_BYTES: usize =
    HEADER_BYTES + MAX_FONT_FACES * (DESCRIPTOR_BYTES + MAX_FAMILY_BYTES) + MAX_FONT_BYTES;

/// CSS-wide keywords and generic family names a package family cannot use.
const RESERVED_FAMILIES: [&str; 9] = [
    "serif",
    "sans-serif",
    "monospace",
    "system-ui",
    "inherit",
    "initial",
    "unset",
    "revert",
    "revert-layer",
];
/// Characters a family name may contain besides ASCII letters and digits.
const FAMILY_PUNCTUATION: &[u8] = b"_ -";

/// The style of a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
}
impl FontStyle {
    const ALL: [(Self, &'static str, u8); 2] =
        [(Self::Normal, "normal", 0), (Self::Italic, "italic", 1)];
    /// The CSS `font-style` keyword.
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(style, ..)| *style == self)
            .map_or("", |(_, name, _)| name)
    }
    /// The style a CSS `font-style` keyword names.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, known, _)| *known == name)
            .map(|(style, ..)| *style)
    }
    fn code(self) -> u8 {
        Self::ALL
            .iter()
            .find(|(style, ..)| *style == self)
            .map_or(0, |(.., code)| *code)
    }
    fn from_code(code: u8) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(.., known)| *known == code)
            .map(|(style, ..)| *style)
    }
}

/// A CSS generic family a package family may stand for on renderers that
/// map generic families to package fonts (the native shell).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GenericFamily {
    SansSerif,
    Serif,
    Monospace,
    SystemUi,
}
impl GenericFamily {
    /// Every generic family with its CSS name and section bit.
    pub const ALL: [(Self, &'static str, u8); 4] = [
        (Self::SansSerif, "sans-serif", 1),
        (Self::Serif, "serif", 1 << 1),
        (Self::Monospace, "monospace", 1 << 2),
        (Self::SystemUi, "system-ui", 1 << 3),
    ];
    /// The CSS generic family name.
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(generic, ..)| *generic == self)
            .map_or("", |(_, name, _)| name)
    }
    /// The generic family a CSS name names.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, known, _)| *known == name)
            .map(|(generic, ..)| *generic)
    }
    fn bit(self) -> u8 {
        Self::ALL
            .iter()
            .find(|(generic, ..)| *generic == self)
            .map_or(0, |(.., bit)| *bit)
    }
    fn mask(generics: &[Self]) -> u8 {
        generics
            .iter()
            .fold(0, |mask, generic| mask | generic.bit())
    }
    fn from_mask(mask: u8) -> Result<Vec<Self>, ResourceError> {
        let known = Self::ALL.iter().fold(0, |all, (.., bit)| all | bit);
        if mask & !known != 0 {
            return Err(ResourceError::Generic(mask));
        }
        Ok(Self::ALL
            .iter()
            .filter(|(.., bit)| mask & bit != 0)
            .map(|(generic, ..)| *generic)
            .collect())
    }
}

/// One declared font face and its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFace {
    /// The family name the face is registered under; its own `name` table
    /// must declare it.
    pub family: String,
    /// The CSS weight; the face's `OS/2` `usWeightClass` must equal it.
    pub weight: u16,
    /// The style; italic exactly when the face's `OS/2` `fsSelection` has
    /// the italic or oblique bit.
    pub style: FontStyle,
    /// The generic families this family stands for (on renderers that map
    /// them); every face of a family declares the same set, and a generic
    /// family belongs to at most one package family.
    pub generics: Vec<GenericFamily>,
    /// The OpenType/TrueType bytes.
    pub data: Vec<u8>,
}

/// The validated, canonically ordered resource section of an artifact.
/// Empty for artifacts without resources (every artifact older than the
/// section).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resources {
    fonts: Vec<FontFace>,
}

/// Why one face's bytes were refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FaceError {
    #[error("not an OpenType/TrueType font (collections and WOFF are not admitted)")]
    Container,
    #[error("invalid, overlapping, misaligned or duplicate font tables")]
    Tables,
    #[error("missing required font table {0:?}")]
    MissingTable(String),
    #[error("missing or truncated OS/2 table")]
    Os2,
    #[error("missing or unreadable family name")]
    Names,
    #[error("declares weight {declared}, but its OS/2 table states {actual}")]
    Weight { declared: u16, actual: u16 },
    #[error("declares style {declared}, but its OS/2 table states {actual}")]
    Style {
        declared: &'static str,
        actual: &'static str,
    },
    #[error("is not named {0:?} by its own name table")]
    Family(String),
}

/// Why a resource section or declaration was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResourceError {
    #[error("resource section is truncated, oversized or has trailing bytes")]
    Length,
    #[error("not a Varta resource section")]
    Magic,
    #[error("unsupported resource section version {0}")]
    Version(u32),
    #[error("reserved resource field is not zero")]
    Reserved,
    #[error("font face count {0} is outside 1..={MAX_FONT_FACES}")]
    FaceCount(usize),
    #[error("font face {0} is empty or exceeds the per-face byte limit")]
    FaceSize(usize),
    #[error("font bytes exceed the total resource limit")]
    TotalSize,
    #[error("invalid font family name {0:?}: a bounded, non-generic ASCII name")]
    FamilyName(String),
    #[error(
        "font weight {0} is not {MIN_FONT_WEIGHT}..={MAX_FONT_WEIGHT} in steps of {FONT_WEIGHT_STEP}"
    )]
    Weight(u16),
    #[error("unknown font style code {0}")]
    Style(u8),
    #[error("unknown generic family bits {0:#x}")]
    Generic(u8),
    #[error("font family {0:?} is spelled or mapped to generic families differently by its faces")]
    FamilyMismatch(String),
    #[error("generic family {0} is mapped by more than one font family")]
    GenericConflict(&'static str),
    #[error("duplicate font face {family:?} weight {weight} {style}")]
    Duplicate {
        family: String,
        weight: u16,
        style: &'static str,
    },
    #[error("font faces are not in canonical order")]
    Order,
    #[error("font face {0} does not match its recorded SHA-256")]
    Digest(usize),
    #[error("the resource section was packaged with another payload")]
    Payload,
    #[error("font face {index} ({family:?}): {source}")]
    Face {
        index: usize,
        family: String,
        #[source]
        source: FaceError,
    },
}

impl From<ResourceError> for varta_common::VartaError {
    fn from(error: ResourceError) -> Self {
        Self::InvalidArtifact(format!("resource section: {error}"))
    }
}

impl Resources {
    /// Validate declared faces and put them into canonical order: families
    /// in first-declaration order, each family's faces by weight, then style.
    pub fn new(fonts: Vec<FontFace>) -> Result<Self, ResourceError> {
        let mut order: Vec<String> = Vec::new();
        for face in &fonts {
            let key = face.family.to_ascii_lowercase();
            if !order.contains(&key) {
                order.push(key);
            }
        }
        let rank = |face: &FontFace| {
            let key = face.family.to_ascii_lowercase();
            (
                order.iter().position(|known| *known == key),
                face.weight,
                face.style,
            )
        };
        let mut fonts = fonts;
        fonts.sort_by_key(rank);
        let resources = Self { fonts };
        resources.validate()?;
        Ok(resources)
    }

    /// The faces, in canonical order.
    #[must_use]
    pub fn fonts(&self) -> &[FontFace] {
        &self.fonts
    }

    /// Whether the artifact ships no resources.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }

    /// Every rule of a section: bounds, faces, families and canonical order.
    fn validate(&self) -> Result<(), ResourceError> {
        if !(1..=MAX_FONT_FACES).contains(&self.fonts.len()) {
            return Err(ResourceError::FaceCount(self.fonts.len()));
        }
        // Every byte bound holds before any face is parsed.
        let mut total = 0usize;
        for (index, face) in self.fonts.iter().enumerate() {
            if face.data.is_empty() || face.data.len() > MAX_FONT_FACE_BYTES {
                return Err(ResourceError::FaceSize(index));
            }
            total = total
                .checked_add(face.data.len())
                .filter(|total| *total <= MAX_FONT_BYTES)
                .ok_or(ResourceError::TotalSize)?;
        }
        let mut families: BTreeMap<String, (&str, u8)> = BTreeMap::new();
        let mut generics: BTreeMap<GenericFamily, String> = BTreeMap::new();
        let mut previous: Option<(&str, u16, FontStyle)> = None;
        for (index, face) in self.fonts.iter().enumerate() {
            validate_family(&face.family)?;
            if !face.weight.is_multiple_of(FONT_WEIGHT_STEP)
                || !(MIN_FONT_WEIGHT..=MAX_FONT_WEIGHT).contains(&face.weight)
            {
                return Err(ResourceError::Weight(face.weight));
            }
            let mask = GenericFamily::mask(&face.generics);
            let key = face.family.to_ascii_lowercase();
            let continues =
                previous.is_some_and(|(family, ..)| family.eq_ignore_ascii_case(&face.family));
            match families.get(&key) {
                Some((spelling, known)) if *spelling != face.family || *known != mask => {
                    return Err(ResourceError::FamilyMismatch(face.family.clone()));
                }
                // A family's faces are adjacent.
                Some(_) if !continues => return Err(ResourceError::Order),
                Some(_) => {}
                None => {
                    families.insert(key, (&face.family, mask));
                    for generic in &face.generics {
                        if generics.insert(*generic, face.family.clone()).is_some() {
                            return Err(ResourceError::GenericConflict(generic.name()));
                        }
                    }
                }
            }
            if let Some((_, weight, style)) = previous.filter(|_| continues) {
                match (weight, style).cmp(&(face.weight, face.style)) {
                    std::cmp::Ordering::Less => {}
                    std::cmp::Ordering::Equal => {
                        return Err(ResourceError::Duplicate {
                            family: face.family.clone(),
                            weight: face.weight,
                            style: face.style.name(),
                        });
                    }
                    std::cmp::Ordering::Greater => return Err(ResourceError::Order),
                }
            }
            validate_face(face).map_err(|source| ResourceError::Face {
                index,
                family: face.family.clone(),
                source,
            })?;
            previous = Some((&face.family, face.weight, face.style));
        }
        Ok(())
    }

    /// Encode the section for `payload`, the portable Wasm it ships with.
    /// An empty set has no section.
    #[must_use]
    pub fn encode(&self, payload: &[u8]) -> Option<Vec<u8>> {
        if self.fonts.is_empty() {
            return None;
        }
        let names = self.fonts.iter().map(|f| f.family.len()).sum::<usize>();
        let fonts = self.fonts.iter().map(|f| f.data.len()).sum::<usize>();
        let mut bytes =
            Vec::with_capacity(HEADER_BYTES + self.fonts.len() * DESCRIPTOR_BYTES + names + fonts);
        bytes.extend_from_slice(RESOURCE_MAGIC);
        bytes.extend_from_slice(&RESOURCE_VERSION.to_le_bytes());
        // Validated: at most MAX_FONT_FACES faces of at most
        // MAX_FONT_FACE_BYTES each, and family names of at most
        // MAX_FAMILY_BYTES, so every length fits its field.
        bytes.extend_from_slice(&(self.fonts.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&Sha256::digest(payload));
        for face in &self.fonts {
            bytes.extend_from_slice(&(face.family.len() as u16).to_le_bytes());
            bytes.extend_from_slice(&face.weight.to_le_bytes());
            bytes.push(face.style.code());
            bytes.push(GenericFamily::mask(&face.generics));
            bytes.extend_from_slice(&DESCRIPTOR_RESERVED.to_le_bytes());
            bytes.extend_from_slice(&(face.data.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&Sha256::digest(&face.data));
        }
        for face in &self.fonts {
            bytes.extend_from_slice(face.family.as_bytes());
            bytes.extend_from_slice(&face.data);
        }
        Some(bytes)
    }

    /// Decode and validate a section that must belong to `payload`. Any
    /// version but [`RESOURCE_VERSION`] is refused.
    pub fn decode(bytes: &[u8], payload: &[u8]) -> Result<Self, ResourceError> {
        if bytes.len() > MAX_RESOURCE_SECTION_BYTES {
            return Err(ResourceError::Length);
        }
        let header = bytes.get(..HEADER_BYTES).ok_or(ResourceError::Length)?;
        if &header[..VERSION_AT] != RESOURCE_MAGIC {
            return Err(ResourceError::Magic);
        }
        let version = le_u32(header, VERSION_AT).ok_or(ResourceError::Length)?;
        if version != RESOURCE_VERSION {
            return Err(ResourceError::Version(version));
        }
        let count = le_u32(header, COUNT_AT).ok_or(ResourceError::Length)? as usize;
        if !(1..=MAX_FONT_FACES).contains(&count) {
            return Err(ResourceError::FaceCount(count));
        }
        if header[PAYLOAD_DIGEST_AT..] != Sha256::digest(payload)[..] {
            return Err(ResourceError::Payload);
        }
        let mut at = HEADER_BYTES + count * DESCRIPTOR_BYTES;
        if bytes.len() < at {
            return Err(ResourceError::Length);
        }
        let mut fonts = Vec::with_capacity(count);
        for index in 0..count {
            let descriptor = &bytes[HEADER_BYTES + index * DESCRIPTOR_BYTES..][..DESCRIPTOR_BYTES];
            let field = |at| le_u16(descriptor, at).ok_or(ResourceError::Length);
            if field(DESCRIPTOR_RESERVED_AT)? != DESCRIPTOR_RESERVED {
                return Err(ResourceError::Reserved);
            }
            let family_length = usize::from(field(FAMILY_LENGTH_AT)?);
            let font_length =
                le_u32(descriptor, FONT_LENGTH_AT).ok_or(ResourceError::Length)? as usize;
            if font_length == 0 || font_length > MAX_FONT_FACE_BYTES {
                return Err(ResourceError::FaceSize(index));
            }
            let family_end = at.checked_add(family_length).ok_or(ResourceError::Length)?;
            let font_end = family_end
                .checked_add(font_length)
                .ok_or(ResourceError::Length)?;
            let family = bytes.get(at..family_end).ok_or(ResourceError::Length)?;
            let data = bytes
                .get(family_end..font_end)
                .ok_or(ResourceError::Length)?;
            let family = std::str::from_utf8(family).map_err(|_| {
                ResourceError::FamilyName(String::from_utf8_lossy(family).into_owned())
            })?;
            if descriptor[FONT_DIGEST_AT..] != Sha256::digest(data)[..] {
                return Err(ResourceError::Digest(index));
            }
            let style = descriptor[STYLE_AT];
            fonts.push(FontFace {
                family: family.to_owned(),
                weight: field(WEIGHT_AT)?,
                style: FontStyle::from_code(style).ok_or(ResourceError::Style(style))?,
                generics: GenericFamily::from_mask(descriptor[GENERICS_AT])?,
                data: data.to_vec(),
            });
            at = font_end;
        }
        if at != bytes.len() {
            return Err(ResourceError::Length);
        }
        let resources = Self { fonts };
        resources.validate()?;
        Ok(resources)
    }
}

/// A bounded ASCII family name that is neither generic nor a CSS-wide
/// keyword (the browser package-font rule).
fn validate_family(family: &str) -> Result<(), ResourceError> {
    let bytes = family.as_bytes();
    if bytes.is_empty()
        || bytes.len() > MAX_FAMILY_BYTES
        || family.trim() != family
        || !bytes[0].is_ascii_alphabetic()
        || !bytes
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || FAMILY_PUNCTUATION.contains(c))
        || RESERVED_FAMILIES.contains(&family.to_ascii_lowercase().as_str())
    {
        return Err(ResourceError::FamilyName(family.to_owned()));
    }
    Ok(())
}

/// The face's bytes state exactly the declared family, weight and style.
fn validate_face(face: &FontFace) -> Result<(), FaceError> {
    let admitted = sfnt::inspect(&face.data)?;
    if admitted.weight != face.weight {
        return Err(FaceError::Weight {
            declared: face.weight,
            actual: admitted.weight,
        });
    }
    if admitted.style != face.style {
        return Err(FaceError::Style {
            declared: face.style.name(),
            actual: admitted.style.name(),
        });
    }
    if !admitted
        .families
        .iter()
        .any(|name| name.eq_ignore_ascii_case(&face.family))
    {
        return Err(FaceError::Family(face.family.clone()));
    }
    Ok(())
}

fn le_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(at..at + size_of::<u16>())?.try_into().ok()?,
    ))
}
fn le_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(at..at + size_of::<u32>())?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGULAR: &[u8] = include_bytes!("../../varta-view/tests/fixtures/advance-400.ttf");
    const BOLD: &[u8] = include_bytes!("../../varta-view/tests/fixtures/bold-900.ttf");
    const ITALIC: &[u8] = include_bytes!("../../varta-view/tests/fixtures/italic-900.ttf");
    const MONO: &[u8] = include_bytes!("../../varta-view/tests/fixtures/advance-900.ttf");
    const SANS: &str = "Varta Fixture 400";
    const MONO_FAMILY: &str = "Varta Fixture 900";
    const BOLD_WEIGHT: u16 = 700;
    const PAYLOAD: &[u8] = b"\0asm\x01\0\0\0";
    /// The sfnt offset table and table record sizes, for tampering tests.
    const OFFSET_TABLE: usize = 12;
    const TABLE_RECORD: usize = 16;
    const RECORD_OFFSET: usize = 8;
    const RECORD_LENGTH: usize = 12;
    const TAG_BYTES: usize = 4;

    fn face(family: &str, weight: u16, style: FontStyle, data: &[u8]) -> FontFace {
        FontFace {
            family: family.into(),
            weight,
            style,
            generics: Vec::new(),
            data: data.to_vec(),
        }
    }
    /// The four fixture faces, declared out of canonical order.
    fn declared() -> Vec<FontFace> {
        let mut mono = face(MONO_FAMILY, NORMAL_FONT_WEIGHT, FontStyle::Normal, MONO);
        mono.generics = vec![GenericFamily::Monospace];
        vec![
            face(SANS, BOLD_WEIGHT, FontStyle::Normal, BOLD),
            mono,
            face(SANS, NORMAL_FONT_WEIGHT, FontStyle::Italic, ITALIC),
            face(SANS, NORMAL_FONT_WEIGHT, FontStyle::Normal, REGULAR),
        ]
    }
    fn encoded() -> Vec<u8> {
        Resources::new(declared()).unwrap().encode(PAYLOAD).unwrap()
    }
    fn descriptor(index: usize) -> usize {
        HEADER_BYTES + index * DESCRIPTOR_BYTES
    }
    fn record(index: usize) -> usize {
        OFFSET_TABLE + index * TABLE_RECORD
    }

    #[test]
    fn declared_faces_round_trip_in_canonical_order() {
        let resources = Resources::new(declared()).unwrap();
        let order: Vec<_> = resources
            .fonts()
            .iter()
            .map(|face| (face.family.as_str(), face.weight, face.style))
            .collect();
        assert_eq!(
            order,
            [
                (SANS, NORMAL_FONT_WEIGHT, FontStyle::Normal),
                (SANS, NORMAL_FONT_WEIGHT, FontStyle::Italic),
                (SANS, BOLD_WEIGHT, FontStyle::Normal),
                (MONO_FAMILY, NORMAL_FONT_WEIGHT, FontStyle::Normal),
            ]
        );
        let bytes = resources.encode(PAYLOAD).unwrap();
        assert_eq!(Resources::decode(&bytes, PAYLOAD).unwrap(), resources);
        // Families keep their declaration order (the first is the default);
        // the order of one family's faces does not change the encoding.
        let mut mono_first = declared();
        mono_first.swap(0, 1);
        assert_eq!(
            Resources::new(mono_first).unwrap().fonts()[0].family,
            MONO_FAMILY
        );
        let mut same_families = declared();
        same_families.swap(0, 2);
        assert_eq!(
            Resources::new(same_families)
                .unwrap()
                .encode(PAYLOAD)
                .unwrap(),
            bytes
        );
        assert_eq!(Resources::default().encode(PAYLOAD), None);
        assert!(Resources::default().is_empty());
    }

    #[test]
    fn a_section_is_bound_to_its_payload_and_version() {
        let bytes = encoded();
        assert_eq!(
            Resources::decode(&bytes, b"\0asm\x01\0\0\0other"),
            Err(ResourceError::Payload)
        );
        let mut version = bytes.clone();
        version[VERSION_AT..COUNT_AT].copy_from_slice(&(RESOURCE_VERSION + 1).to_le_bytes());
        assert_eq!(
            Resources::decode(&version, PAYLOAD),
            Err(ResourceError::Version(RESOURCE_VERSION + 1))
        );
        let mut magic = bytes.clone();
        magic[0] ^= 1;
        assert_eq!(
            Resources::decode(&magic, PAYLOAD),
            Err(ResourceError::Magic)
        );
        for count in [0, MAX_FONT_FACES + 1] {
            let mut faces = bytes.clone();
            faces[COUNT_AT..PAYLOAD_DIGEST_AT].copy_from_slice(&(count as u32).to_le_bytes());
            assert_eq!(
                Resources::decode(&faces, PAYLOAD),
                Err(ResourceError::FaceCount(count))
            );
        }
    }

    #[test]
    fn tampered_truncated_and_oversized_sections_are_refused() {
        let bytes = encoded();
        let last = bytes.len() - 1;
        let mut font = bytes.clone();
        font[last] ^= 1;
        assert_eq!(
            Resources::decode(&font, PAYLOAD),
            Err(ResourceError::Digest(3))
        );
        let mut digest = bytes.clone();
        digest[descriptor(0) + FONT_DIGEST_AT] ^= 1;
        assert_eq!(
            Resources::decode(&digest, PAYLOAD),
            Err(ResourceError::Digest(0))
        );
        let mut reserved = bytes.clone();
        reserved[descriptor(1) + DESCRIPTOR_RESERVED_AT] = 1;
        assert_eq!(
            Resources::decode(&reserved, PAYLOAD),
            Err(ResourceError::Reserved)
        );
        let unknown_style = FontStyle::ALL.len() as u8;
        let mut style = bytes.clone();
        style[descriptor(1) + STYLE_AT] = unknown_style;
        assert_eq!(
            Resources::decode(&style, PAYLOAD),
            Err(ResourceError::Style(unknown_style))
        );
        let unknown_generic = 1 << 7;
        let mut generic = bytes.clone();
        generic[descriptor(0) + GENERICS_AT] = unknown_generic;
        assert_eq!(
            Resources::decode(&generic, PAYLOAD),
            Err(ResourceError::Generic(unknown_generic))
        );
        // A descriptor that no longer states what its face's tables state.
        let mut weight = bytes.clone();
        weight[descriptor(3) + WEIGHT_AT..][..size_of::<u16>()]
            .copy_from_slice(&BOLD_WEIGHT.to_le_bytes());
        assert!(matches!(
            Resources::decode(&weight, PAYLOAD),
            Err(ResourceError::Face {
                index: 3,
                source: FaceError::Weight { .. },
                ..
            })
        ));
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            Resources::decode(&trailing, PAYLOAD),
            Err(ResourceError::Length)
        );
        for length in [0, HEADER_BYTES - 1, descriptor(4) - 1, last] {
            assert_eq!(
                Resources::decode(&bytes[..length], PAYLOAD),
                Err(ResourceError::Length),
                "{length}"
            );
        }
        let mut oversized = bytes.clone();
        oversized[descriptor(3) + FONT_LENGTH_AT..][..size_of::<u32>()]
            .copy_from_slice(&(MAX_FONT_FACE_BYTES as u32 + 1).to_le_bytes());
        assert_eq!(
            Resources::decode(&oversized, PAYLOAD),
            Err(ResourceError::FaceSize(3))
        );
        assert_eq!(
            Resources::decode(&vec![0; MAX_RESOURCE_SECTION_BYTES + 1], PAYLOAD),
            Err(ResourceError::Length)
        );
    }

    #[test]
    fn a_non_canonical_section_is_refused() {
        let canonical = Resources::new(declared()).unwrap();
        let mut fonts = canonical.fonts.clone();
        fonts.swap(0, 2);
        let swapped = Resources { fonts }.encode(PAYLOAD).unwrap();
        assert_eq!(
            Resources::decode(&swapped, PAYLOAD),
            Err(ResourceError::Order)
        );
        // A family whose faces are not adjacent.
        let mut fonts = canonical.fonts.clone();
        fonts.swap(2, 3);
        let split = Resources { fonts }.encode(PAYLOAD).unwrap();
        assert_eq!(
            Resources::decode(&split, PAYLOAD),
            Err(ResourceError::Order)
        );
    }

    #[test]
    fn declarations_must_match_the_face_tables() {
        let refused = |faces: Vec<FontFace>| Resources::new(faces).unwrap_err();
        assert!(matches!(
            refused(vec![face(
                SANS,
                NORMAL_FONT_WEIGHT,
                FontStyle::Normal,
                BOLD
            )]),
            ResourceError::Face {
                source: FaceError::Weight {
                    declared: NORMAL_FONT_WEIGHT,
                    actual: BOLD_WEIGHT
                },
                ..
            }
        ));
        assert!(matches!(
            refused(vec![face(
                SANS,
                NORMAL_FONT_WEIGHT,
                FontStyle::Normal,
                ITALIC
            )]),
            ResourceError::Face {
                source: FaceError::Style { .. },
                ..
            }
        ));
        assert!(matches!(
            refused(vec![face(
                MONO_FAMILY,
                NORMAL_FONT_WEIGHT,
                FontStyle::Normal,
                REGULAR
            )]),
            ResourceError::Face {
                source: FaceError::Family(_),
                ..
            }
        ));
        // Family names match case-insensitively, as CSS does.
        let lower = SANS.to_ascii_lowercase();
        assert!(
            Resources::new(vec![face(
                &lower,
                NORMAL_FONT_WEIGHT,
                FontStyle::Normal,
                REGULAR
            )])
            .is_ok()
        );
        for family in [
            "",
            "monospace",
            "Serif",
            "inherit",
            " Varta",
            "Varta;",
            "1Varta",
        ] {
            assert_eq!(
                refused(vec![face(
                    family,
                    NORMAL_FONT_WEIGHT,
                    FontStyle::Normal,
                    REGULAR
                )]),
                ResourceError::FamilyName(family.into()),
                "{family}"
            );
        }
        for weight in [0, 450, MAX_FONT_WEIGHT + FONT_WEIGHT_STEP] {
            assert_eq!(
                refused(vec![face(SANS, weight, FontStyle::Normal, REGULAR)]),
                ResourceError::Weight(weight)
            );
        }
    }

    #[test]
    fn families_faces_and_generic_families_are_unambiguous() {
        let regular = || face(SANS, NORMAL_FONT_WEIGHT, FontStyle::Normal, REGULAR);
        assert!(matches!(
            Resources::new(vec![regular(), regular()]),
            Err(ResourceError::Duplicate { .. })
        ));
        let upper = SANS.to_ascii_uppercase();
        let mut respelled = face(SANS, BOLD_WEIGHT, FontStyle::Normal, BOLD);
        respelled.family = upper.clone();
        assert_eq!(
            Resources::new(vec![regular(), respelled]),
            Err(ResourceError::FamilyMismatch(upper))
        );
        let mut generic = regular();
        generic.generics = vec![GenericFamily::SansSerif];
        assert_eq!(
            Resources::new(vec![
                generic,
                face(SANS, BOLD_WEIGHT, FontStyle::Normal, BOLD)
            ]),
            Err(ResourceError::FamilyMismatch(SANS.into()))
        );
        let mut sans = regular();
        sans.generics = vec![GenericFamily::Monospace];
        let mut mono = face(MONO_FAMILY, NORMAL_FONT_WEIGHT, FontStyle::Normal, MONO);
        mono.generics = vec![GenericFamily::Monospace];
        assert_eq!(
            Resources::new(vec![sans, mono]),
            Err(ResourceError::GenericConflict(
                GenericFamily::Monospace.name()
            ))
        );
        assert_eq!(Resources::new(Vec::new()), Err(ResourceError::FaceCount(0)));
        let many = (0..=MAX_FONT_FACES).map(|_| regular()).collect();
        assert_eq!(
            Resources::new(many),
            Err(ResourceError::FaceCount(MAX_FONT_FACES + 1))
        );
    }

    #[test]
    fn malformed_and_oversized_fonts_are_refused() {
        let with = |data: Vec<u8>| {
            Resources::new(vec![FontFace {
                data,
                ..face(SANS, NORMAL_FONT_WEIGHT, FontStyle::Normal, REGULAR)
            }])
        };
        let container = |result: Result<Resources, ResourceError>| {
            matches!(
                result,
                Err(ResourceError::Face {
                    source: FaceError::Container | FaceError::Tables,
                    ..
                })
            )
        };
        assert!(container(with(REGULAR[..REGULAR.len() - 1].to_vec())));
        for flavor in [b"ttcf", b"wOFF", b"wOF2"] {
            let mut foreign = REGULAR.to_vec();
            foreign[..TAG_BYTES].copy_from_slice(flavor);
            assert!(container(with(foreign)));
        }
        // A table past the end, a duplicate tag and two overlapping tables.
        let mut past = REGULAR.to_vec();
        past[record(0) + RECORD_LENGTH..][..size_of::<u32>()]
            .copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(container(with(past)));
        let mut duplicate = REGULAR.to_vec();
        let tag = duplicate[record(0)..][..TAG_BYTES].to_vec();
        duplicate[record(1)..][..TAG_BYTES].copy_from_slice(&tag);
        assert!(container(with(duplicate)));
        let mut overlap = REGULAR.to_vec();
        let offset = overlap[record(0) + RECORD_OFFSET..][..size_of::<u32>()].to_vec();
        overlap[record(1) + RECORD_OFFSET..][..size_of::<u32>()].copy_from_slice(&offset);
        assert!(container(with(overlap)));
        // The OS/2 table renamed away.
        let mut missing = REGULAR.to_vec();
        let os2 = (0..)
            .find(|index| &missing[record(*index)..][..TAG_BYTES] == b"OS/2")
            .unwrap();
        missing[record(os2)..][..TAG_BYTES].copy_from_slice(b"OS/3");
        assert!(matches!(
            with(missing),
            Err(ResourceError::Face {
                source: FaceError::MissingTable(_),
                ..
            })
        ));
        assert_eq!(with(Vec::new()), Err(ResourceError::FaceSize(0)));
        assert_eq!(
            with(vec![0; MAX_FONT_FACE_BYTES + 1]),
            Err(ResourceError::FaceSize(0))
        );
        let faces = MAX_FONT_BYTES / MAX_FONT_FACE_BYTES + 1;
        let large = (0..faces)
            .map(|index| FontFace {
                data: vec![0; MAX_FONT_FACE_BYTES],
                ..face(
                    SANS,
                    MIN_FONT_WEIGHT + index as u16 * FONT_WEIGHT_STEP,
                    FontStyle::Normal,
                    REGULAR,
                )
            })
            .collect();
        assert_eq!(Resources::new(large), Err(ResourceError::TotalSize));
    }
}
