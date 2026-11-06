//! The packaged images of a version 2 resource section: image files the
//! native renderer decodes and the browser bundle carries, by their view
//! resource names (what an `img` `src` or `srcset` names).
//!
//! Image descriptor (little-endian):
//!
//! ```text
//! name length u16 | format u16 | file length u32 | file SHA-256 [32]
//! ```
//!
//! Images are stored in canonical order, by name (bytewise ascending), so
//! one declaration has one encoding. Every image is admitted by its format
//! signature ([`sniff`], WHATWG MIME Sniffing §6.1), which must be the
//! descriptor's format, and its bounds; packaging decodes every image with
//! the renderer's decoder, and the native host decodes them again before any
//! guest code runs.
use super::{DIGEST_BYTES, ResourceError, le_u16, le_u32};
use sha2::{Digest, Sha256};
use varta_common::view::images::{
    ImageFormat, MAX_IMAGE_BYTES, MAX_IMAGE_NAME_BYTES, MAX_IMAGE_TOTAL_BYTES, MAX_IMAGES, sniff,
};
use varta_common::view::{AttributeValue, Tag, attributes};

const NAME_LENGTH_AT: usize = 0;
const FORMAT_AT: usize = NAME_LENGTH_AT + size_of::<u16>();
const FILE_LENGTH_AT: usize = FORMAT_AT + size_of::<u16>();
const FILE_DIGEST_AT: usize = FILE_LENGTH_AT + size_of::<u32>();
/// The bytes of one image descriptor.
pub(super) const IMAGE_DESCRIPTOR_BYTES: usize = FILE_DIGEST_AT + DIGEST_BYTES;
/// The most bytes all image descriptors, names and files can take.
pub(super) const MAX_IMAGE_SECTION_BYTES: usize =
    MAX_IMAGES * (IMAGE_DESCRIPTOR_BYTES + MAX_IMAGE_NAME_BYTES) + MAX_IMAGE_TOTAL_BYTES;

/// One packaged image: its view resource name and its file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageResource {
    /// The name an `img` `src` uses: `[A-Za-z0-9._-]` segments separated by
    /// `/`, at most `MAX_IMAGE_NAME_BYTES` bytes.
    pub name: String,
    /// The image file.
    pub data: Vec<u8>,
}

/// Every rule of the section's images: bounds, names, canonical order and
/// each file's format.
pub(super) fn validate(images: &[ImageResource]) -> Result<(), ResourceError> {
    if images.len() > MAX_IMAGES {
        return Err(ResourceError::ImageCount(images.len()));
    }
    let mut total = 0usize;
    for (index, image) in images.iter().enumerate() {
        if image.data.is_empty() || image.data.len() > MAX_IMAGE_BYTES {
            return Err(ResourceError::ImageSize(index));
        }
        total = total
            .checked_add(image.data.len())
            .filter(|total| *total <= MAX_IMAGE_TOTAL_BYTES)
            .ok_or(ResourceError::ImageTotalSize)?;
    }
    for (index, image) in images.iter().enumerate() {
        if image.name.len() > MAX_IMAGE_NAME_BYTES
            || !attributes::valid(AttributeValue::Resource, Tag::Img, &image.name)
        {
            return Err(ResourceError::ImageName(image.name.clone()));
        }
        if let Some(previous) = index.checked_sub(1).map(|at| &images[at]) {
            match previous.name.cmp(&image.name) {
                std::cmp::Ordering::Less => {}
                std::cmp::Ordering::Equal => {
                    return Err(ResourceError::DuplicateImage(image.name.clone()));
                }
                std::cmp::Ordering::Greater => return Err(ResourceError::ImageOrder),
            }
        }
        sniff(&image.data).map_err(|source| ResourceError::Image {
            index,
            name: image.name.clone(),
            source,
        })?;
    }
    Ok(())
}

/// Append each image's descriptor.
pub(super) fn encode_descriptors(images: &[ImageResource], bytes: &mut Vec<u8>) {
    for image in images {
        // Validated: names are at most MAX_IMAGE_NAME_BYTES, files at most
        // MAX_IMAGE_BYTES and every file has a known format.
        let format = sniff(&image.data).map_or(0, ImageFormat::code);
        bytes.extend_from_slice(&(image.name.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&format.to_le_bytes());
        bytes.extend_from_slice(&(image.data.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&Sha256::digest(&image.data));
    }
}

/// Append each image's name and file.
pub(super) fn encode_data(images: &[ImageResource], bytes: &mut Vec<u8>) {
    for image in images {
        bytes.extend_from_slice(image.name.as_bytes());
        bytes.extend_from_slice(&image.data);
    }
}

/// Read `count` images whose descriptors start at `descriptors` and whose
/// data starts at `*at`, advancing `*at` past them. The caller validates
/// the set.
pub(super) fn decode(
    bytes: &[u8],
    descriptors: usize,
    count: usize,
    at: &mut usize,
) -> Result<Vec<ImageResource>, ResourceError> {
    let mut images = Vec::with_capacity(count);
    for index in 0..count {
        let start = descriptors + index * IMAGE_DESCRIPTOR_BYTES;
        let descriptor = bytes
            .get(start..start + IMAGE_DESCRIPTOR_BYTES)
            .ok_or(ResourceError::Length)?;
        let name_length =
            usize::from(le_u16(descriptor, NAME_LENGTH_AT).ok_or(ResourceError::Length)?);
        let format = le_u16(descriptor, FORMAT_AT).ok_or(ResourceError::Length)?;
        let format = ImageFormat::from_code(format).ok_or(ResourceError::ImageFormat(index))?;
        let file_length = le_u32(descriptor, FILE_LENGTH_AT).ok_or(ResourceError::Length)? as usize;
        if file_length == 0 || file_length > MAX_IMAGE_BYTES {
            return Err(ResourceError::ImageSize(index));
        }
        let name_end = at.checked_add(name_length).ok_or(ResourceError::Length)?;
        let file_end = name_end
            .checked_add(file_length)
            .ok_or(ResourceError::Length)?;
        let name = bytes.get(*at..name_end).ok_or(ResourceError::Length)?;
        let data = bytes.get(name_end..file_end).ok_or(ResourceError::Length)?;
        let name = std::str::from_utf8(name)
            .map_err(|_| ResourceError::ImageName(String::from_utf8_lossy(name).into_owned()))?;
        if descriptor[FILE_DIGEST_AT..] != Sha256::digest(data)[..] {
            return Err(ResourceError::ImageDigest(index));
        }
        if sniff(data) != Ok(format) {
            return Err(ResourceError::ImageFormat(index));
        }
        images.push(ImageResource {
            name: name.to_owned(),
            data: data.to_vec(),
        });
        *at = file_end;
    }
    Ok(images)
}

#[cfg(test)]
#[path = "images_tests.rs"]
mod tests;
