use super::super::{
    IMAGE_COUNT_AT, IMAGE_HEADER_BYTES, IMAGE_RESOURCE_VERSION, RESOURCE_VERSION, Resources,
    VERSION_AT,
};
use super::*;
use varta_common::view::images::{MAX_IMAGE_DIMENSION, PngRefusal};

const PAYLOAD: &[u8] = b"\0asm\x01\0\0\0";
/// A valid 1x1 RGBA PNG.
const PIXEL: [u8; 70] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0,
    0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];
/// Where the IHDR width sits in [`PIXEL`].
const WIDTH_IN_PNG: usize = 16;
const NAME: &str = "a.png";

fn image(name: &str) -> ImageResource {
    ImageResource {
        name: name.into(),
        data: PIXEL.to_vec(),
    }
}

#[test]
fn images_round_trip_in_name_order_in_a_version_2_section() {
    let resources =
        Resources::with_images(Vec::new(), vec![image("icons/b.png"), image(NAME)]).unwrap();
    let names: Vec<_> = resources.images().iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, [NAME, "icons/b.png"]);
    assert!(!resources.is_empty());
    let bytes = resources.encode(PAYLOAD).unwrap();
    assert_eq!(
        bytes[VERSION_AT..VERSION_AT + size_of::<u32>()],
        IMAGE_RESOURCE_VERSION.to_le_bytes()
    );
    assert_eq!(Resources::decode(&bytes, PAYLOAD).unwrap(), resources);
    // The declaration order does not change the encoding.
    let again =
        Resources::with_images(Vec::new(), vec![image(NAME), image("icons/b.png")]).unwrap();
    assert_eq!(again.encode(PAYLOAD).unwrap(), bytes);
    // A section without images has faces, in version 1.
    assert_ne!(RESOURCE_VERSION, IMAGE_RESOURCE_VERSION);
    assert_eq!(
        Resources::with_images(Vec::new(), Vec::new()).unwrap_err(),
        ResourceError::FaceCount(0)
    );
}

#[test]
fn image_names_bytes_and_structure_are_checked() {
    let refused = |images| Resources::with_images(Vec::new(), images).unwrap_err();
    for name in [
        "",
        "/a.png",
        "../a.png",
        "a/../b.png",
        "a b.png",
        "https://a.test/a.png",
    ] {
        assert_eq!(
            refused(vec![image(name)]),
            ResourceError::ImageName(name.into())
        );
    }
    assert_eq!(
        refused(vec![image(NAME), image(NAME)]),
        ResourceError::DuplicateImage(NAME.into())
    );
    let mut empty = image(NAME);
    empty.data.clear();
    assert_eq!(refused(vec![empty]), ResourceError::ImageSize(0));
    let mut large = image(NAME);
    large.data.resize(MAX_IMAGE_BYTES + 1, 0);
    assert_eq!(refused(vec![large]), ResourceError::ImageSize(0));
    let mut wide = image(NAME);
    wide.data[WIDTH_IN_PNG..WIDTH_IN_PNG + size_of::<u32>()]
        .copy_from_slice(&(MAX_IMAGE_DIMENSION + 1).to_be_bytes());
    assert!(matches!(
        refused(vec![wide]),
        ResourceError::Image {
            source: PngRefusal::Dimensions { .. },
            ..
        }
    ));
    let many = (0..=MAX_IMAGES)
        .map(|i| image(&format!("{i}.png")))
        .collect();
    assert_eq!(refused(many), ResourceError::ImageCount(MAX_IMAGES + 1));
}

#[test]
fn tampered_and_inconsistent_image_sections_are_refused() {
    let resources = Resources::with_images(Vec::new(), vec![image(NAME), image("b.png")]).unwrap();
    let bytes = resources.encode(PAYLOAD).unwrap();
    let last = bytes.len() - 1;
    let mut file = bytes.clone();
    file[last] ^= 1;
    assert_eq!(
        Resources::decode(&file, PAYLOAD),
        Err(ResourceError::ImageDigest(1))
    );
    let mut dimensions = bytes.clone();
    dimensions[IMAGE_HEADER_BYTES + WIDTH_AT] = 2;
    assert_eq!(
        Resources::decode(&dimensions, PAYLOAD),
        Err(ResourceError::ImageDimensions(0))
    );
    let mut reserved = bytes.clone();
    reserved[IMAGE_HEADER_BYTES + RESERVED_AT] = 1;
    assert_eq!(
        Resources::decode(&reserved, PAYLOAD),
        Err(ResourceError::Reserved)
    );
    let mut none = bytes.clone();
    none[IMAGE_COUNT_AT..IMAGE_COUNT_AT + size_of::<u32>()].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        Resources::decode(&none, PAYLOAD),
        Err(ResourceError::ImageCount(0))
    );
    // Names out of canonical order: swap the two equal-length names.
    let mut order = bytes.clone();
    let names = bytes.len() - 2 * (PIXEL.len() + NAME.len());
    order[names] = b'b';
    order[names + NAME.len() + PIXEL.len()] = b'a';
    assert_eq!(
        Resources::decode(&order, PAYLOAD),
        Err(ResourceError::ImageOrder)
    );
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(
        Resources::decode(&trailing, PAYLOAD),
        Err(ResourceError::Length)
    );
    assert_eq!(
        Resources::decode(&bytes[..last], PAYLOAD),
        Err(ResourceError::Length)
    );
}
