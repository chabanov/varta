//! Build-owned image inventory. Package names never become ambient fetch paths.
use crate::{ProjectError, Result};
use std::collections::BTreeMap;
use varta_artifact::{FontFace, FontStyle, Resources};
use varta_common::view::{AttributeValue, Tag, attributes};

// The same image limits as packaged `images` (one source of truth).
pub const MAX_RESOURCES: usize = varta_common::view::images::MAX_IMAGES;
pub const MAX_RESOURCE_BYTES: usize = varta_common::view::images::MAX_IMAGE_BYTES;
pub const MAX_TOTAL_BYTES: usize = varta_common::view::images::MAX_IMAGE_TOTAL_BYTES;
pub const MAX_DECODED_BYTES: usize = varta_common::view::images::MAX_IMAGE_DECODED_BYTES;
const MAX_DIMENSION: u32 = varta_common::view::images::MAX_IMAGE_DIMENSION;
const MAX_NAME_BYTES: usize = 1024;

/// Immutable checked PNG resource, independent of any source filesystem path.
/// Browser decoding still verifies the complete image before guest execution.
pub struct BrowserResource {
    pub(crate) name: String,
    pub(super) bytes: Vec<u8>,
    pub(super) media_type: &'static str,
    pub(super) extension: &'static str,
    pub(crate) font: bool,
    /// The descriptors of a face from the artifact's resource section
    /// (configuration `fonts`); `None` for images and `browser.fonts` faces.
    pub(super) packaged: Option<PackagedFace>,
    decoded_bytes: usize,
}

/// A packaged face's `FontFace` descriptors and the generic families its
/// family stands for, which the applier maps as the native shell does.
pub(super) struct PackagedFace {
    pub(super) weight: u16,
    pub(super) style: FontStyle,
    pub(super) generics: Vec<&'static str>,
}
impl BrowserResource {
    pub fn png(name: &str, bytes: Vec<u8>) -> Result<Self> {
        validate_name(name)?;
        if bytes.len() > MAX_RESOURCE_BYTES
            || bytes.len() < 45
            || !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        {
            return Err(ProjectError::Usage(
                "invalid or oversized PNG resource".into(),
            ));
        }
        let word = |at| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
        if word(8) != 13
            || &bytes[12..16] != b"IHDR"
            || !(1..=MAX_DIMENSION).contains(&word(16))
            || !(1..=MAX_DIMENSION).contains(&word(20))
        {
            return Err(ProjectError::Usage(
                "PNG dimensions or header exceed browser limits".into(),
            ));
        }
        let decoded_bytes = word(16) as usize * word(20) as usize * 4;
        if decoded_bytes > MAX_DECODED_BYTES {
            return Err(ProjectError::Usage(
                "PNG decoded pixels exceed browser resource limit".into(),
            ));
        }
        let mut at = 8;
        let mut end = false;
        while at + 12 <= bytes.len() {
            let length = word(at) as usize;
            let next = at
                .checked_add(12)
                .and_then(|at| at.checked_add(length))
                .filter(|next| *next <= bytes.len())
                .ok_or_else(|| ProjectError::Usage("truncated PNG resource".into()))?;
            let kind = &bytes[at + 4..at + 8];
            if kind == b"IHDR" && at != 8 {
                return Err(ProjectError::Usage("duplicate PNG header".into()));
            }
            if kind == b"acTL" {
                return Err(ProjectError::Usage(
                    "animated PNG resources are not supported".into(),
                ));
            }
            if kind == b"IEND" {
                end = length == 0 && next == bytes.len();
                break;
            }
            at = next;
        }
        if !end {
            return Err(ProjectError::Usage(
                "PNG resource must end with IEND".into(),
            ));
        }
        Ok(Self {
            name: name.into(),
            decoded_bytes,
            bytes,
            media_type: "image/png",
            extension: "png",
            font: false,
            packaged: None,
        })
    }

    /// One image of the artifact's resource section, already admitted and
    /// decoded at packaging: served with its own format's media type.
    pub(crate) fn packaged_image(name: &str, bytes: Vec<u8>) -> Result<Self> {
        validate_name(name)?;
        let format = varta_artifact::ImageFormat::ALL
            .into_iter()
            .find(|format| varta_common::view::images::sniff(&bytes) == Ok(*format))
            .ok_or_else(|| ProjectError::Usage("unknown packaged image format".into()))?;
        Ok(Self {
            name: name.into(),
            decoded_bytes: 0,
            bytes,
            media_type: format.media_type(),
            extension: format.extension(),
            font: false,
            packaged: None,
        })
    }

    /// Bounded OpenType/TrueType/WOFF/WOFF2 face. Complete font sanitization is
    /// delegated to FontFace before registration or guest execution.
    pub fn font(family: &str, bytes: Vec<u8>) -> Result<Self> {
        validate_family(family)?;
        let (media_type, extension, decoded_bytes) = super::font_data::inspect(&bytes)?;
        Ok(Self {
            name: family.into(),
            bytes,
            media_type,
            extension,
            font: true,
            packaged: None,
            decoded_bytes,
        })
    }

    /// One face of an artifact's validated resource section, with the same
    /// bytes, family, weight, style and generic families the native shell
    /// loads.
    pub fn packaged(face: &FontFace) -> Result<Self> {
        let mut resource = Self::font(&face.family, face.data.clone())?;
        resource.packaged = Some(PackagedFace {
            weight: face.weight,
            style: face.style,
            generics: face.generics.iter().map(|generic| generic.name()).collect(),
        });
        Ok(resource)
    }
}
pub(crate) fn validate_family(family: &str) -> Result<()> {
    if family.is_empty()
        || family.len() > 64
        || family.trim() != family
        || !family
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_ -".contains(&c))
        || !family.as_bytes()[0].is_ascii_alphabetic()
        || [
            "serif",
            "sans-serif",
            "monospace",
            "system-ui",
            "inherit",
            "initial",
            "unset",
            "revert",
            "revert-layer",
        ]
        .contains(&family.to_ascii_lowercase().as_str())
    {
        return Err(ProjectError::Usage(
            "browser font requires a bounded, non-generic ASCII family name".into(),
        ));
    }
    Ok(())
}
pub(crate) fn validate_name(name: &str) -> Result<()> {
    if name.len() > MAX_NAME_BYTES || !attributes::valid(AttributeValue::Resource, Tag::Img, name) {
        return Err(ProjectError::Usage(
            "browser resource requires a bounded relative resource name".into(),
        ));
    }
    Ok(())
}
pub(super) fn validate(resources: &[BrowserResource]) -> Result<()> {
    let total = resources.iter().map(|r| r.bytes.len()).sum::<usize>();
    let decoded = resources.iter().map(|r| r.decoded_bytes).sum::<usize>();
    let mut names = BTreeMap::new();
    if resources.len() > MAX_RESOURCES || total > MAX_TOTAL_BYTES || decoded > MAX_DECODED_BYTES {
        return Err(ProjectError::Usage(
            "browser resource inventory exceeds limits".into(),
        ));
    }
    for resource in resources {
        // Images are keyed by their exact name; font faces by family
        // (case-insensitive) and, for packaged faces, weight and style.
        let face = resource
            .packaged
            .as_ref()
            .map(|face| (face.weight, face.style));
        let key = if resource.font {
            (true, resource.name.to_ascii_lowercase(), face)
        } else {
            (false, resource.name.clone(), face)
        };
        if names.insert(key, ()).is_some() {
            return Err(ProjectError::Usage(
                "duplicate browser resource name".into(),
            ));
        }
    }
    Ok(())
}

pub(crate) fn load(
    config: &crate::VartaConfig,
    root: &std::path::Path,
) -> Result<Vec<BrowserResource>> {
    if config.browser.is_none() && config.fonts.is_empty() && config.images.is_empty() {
        return Ok(Vec::new());
    }
    let root = root.canonicalize()?;
    let loader = crate::modules::SourceLoader::new(&root, config.language_profile)?;
    load_with_loader(config, &root, &loader)
}

pub(crate) fn load_with_loader(
    config: &crate::VartaConfig,
    root: &std::path::Path,
    loader: &crate::modules::SourceLoader,
) -> Result<Vec<BrowserResource>> {
    let root = root.canonicalize()?;
    // The configuration's `fonts` and `images` are the artifact's resource
    // section: the same validated faces and images the native shell loads,
    // never a second reading.
    let mut resources = packaged(&crate::artifact::resources_with_loader(
        config, &root, loader,
    )?)?;
    let Some(browser) = &config.browser else {
        validate(&resources)?;
        return Ok(resources);
    };
    let mut total = resources.iter().map(|r| r.bytes.len()).sum::<usize>();
    for (name, path) in &browser.resources {
        validate_name(name)?;
        validate_name(path)?;
        let bytes = loader.read_bytes(
            &root.join(path),
            MAX_RESOURCE_BYTES.min(MAX_TOTAL_BYTES.saturating_sub(total)),
        )?;
        total += bytes.len();
        resources.push(BrowserResource::png(name, bytes)?);
    }
    for (family, path) in &browser.fonts {
        validate_family(family)?;
        validate_name(path)?;
        let bytes = loader.read_bytes(
            &root.join(path),
            MAX_RESOURCE_BYTES.min(MAX_TOTAL_BYTES.saturating_sub(total)),
        )?;
        total += bytes.len();
        resources.push(BrowserResource::font(family, bytes)?);
    }
    validate(&resources)?;
    Ok(resources)
}

/// Browser resources for every face and image of a validated resource
/// section, in its canonical order (the first family is the default family).
/// An image keeps its name: the guest's `src` names it as natively.
pub(crate) fn packaged(section: &Resources) -> Result<Vec<BrowserResource>> {
    section
        .fonts()
        .iter()
        .map(BrowserResource::packaged)
        .chain(
            section
                .images()
                .iter()
                .map(|image| BrowserResource::packaged_image(&image.name, image.data.clone())),
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    fn png() -> Vec<u8> {
        base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==").unwrap()
    }
    #[test]
    fn image_admission_bounds_names_dimensions_chunks_and_inventory() {
        assert!(BrowserResource::png("icons/red.png", png()).is_ok());
        for name in [
            "",
            "/red.png",
            "../red.png",
            "icons/../red.png",
            "https://a.test/red.png",
            "red.png?x",
            "red%2epng",
            "a\\red.png",
        ] {
            assert!(BrowserResource::png(name, png()).is_err(), "{name}");
        }
        let mut large = png();
        large[16..20].copy_from_slice(&(MAX_DIMENSION + 1).to_be_bytes());
        assert!(BrowserResource::png("large", large).is_err());
        // Within the dimension bound, past the decoded total.
        let mut pixels = png();
        pixels[16..20].copy_from_slice(&MAX_DIMENSION.to_be_bytes());
        pixels[20..24].copy_from_slice(&MAX_DIMENSION.to_be_bytes());
        assert!(
            (MAX_DIMENSION as usize).pow(2) * varta_common::view::images::DECODED_PIXEL_BYTES
                > MAX_DECODED_BYTES
        );
        assert!(BrowserResource::png("decoded-limit", pixels).is_err());
        let mut bad = png();
        bad[33..37].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(BrowserResource::png("truncated", bad).is_err());
        let mut animated = png();
        animated[37..41].copy_from_slice(b"acTL");
        assert!(BrowserResource::png("animated", animated).is_err());
        let image = png();
        let mut duplicate = image[..image.len() - 12].to_vec();
        duplicate.extend_from_slice(&image[8..33]);
        duplicate.extend_from_slice(&image[image.len() - 12..]);
        assert!(BrowserResource::png("duplicate-header", duplicate).is_err());
        assert!(BrowserResource::png("huge", vec![0; MAX_RESOURCE_BYTES + 1]).is_err());
        assert!(
            validate(&[
                BrowserResource::png("red", png()).unwrap(),
                BrowserResource::png("red", png()).unwrap()
            ])
            .is_err()
        );
        let many = (0..=MAX_RESOURCES)
            .map(|i| BrowserResource::png(&format!("image{i}"), png()).unwrap())
            .collect::<Vec<_>>();
        assert!(validate(&many).is_err());
        let total = (0..5)
            .map(|i| BrowserResource {
                name: format!("image{i}"),
                bytes: vec![0; MAX_RESOURCE_BYTES],
                media_type: "image/png",
                extension: "png",
                font: false,
                packaged: None,
                decoded_bytes: 0,
            })
            .collect::<Vec<_>>();
        assert!(validate(&total).is_err());
    }
}

#[cfg(test)]
mod font_tests {
    use super::*;
    const FONT: &[u8] = include_bytes!("../../tests/fixtures/browser-fonts/advance-400.ttf");
    #[test]
    fn font_family_admission_and_shared_inventory_limits() {
        assert!(BrowserResource::font("Brand", FONT.to_vec()).is_ok());
        for family in [
            "",
            "serif",
            "Sans-Serif",
            "system-ui",
            "inherit",
            " Brand",
            "Brand ",
            "A; color:red",
            "A\"B",
            "A/B",
            "1Brand",
        ] {
            assert!(
                BrowserResource::font(family, FONT.to_vec()).is_err(),
                "{family}"
            );
        }
        assert!(
            validate(&[
                BrowserResource::font("Brand", FONT.to_vec()).unwrap(),
                BrowserResource::font("brand", FONT.to_vec()).unwrap()
            ])
            .is_err()
        );
        let mut a = BrowserResource::font("A", FONT.to_vec()).unwrap();
        a.decoded_bytes = MAX_DECODED_BYTES;
        let b = BrowserResource::font("B", FONT.to_vec()).unwrap();
        assert!(validate(&[a, b]).is_err());
    }
}
