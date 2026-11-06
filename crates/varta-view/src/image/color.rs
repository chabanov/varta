//! Colour of decoded images, converted to sRGB as Chrome draws them on an
//! sRGB output (CSS Color 4 §10: images are drawn in their own colour space,
//! untagged images are sRGB).
//!
//! A PNG's colour space comes from its chunks in the PNG 3rd edition order
//! (§4.3, §11.3.2): `cICP`, then `iCCP`, then `sRGB`, then `cHRM`/`gAMA`.
//! JPEG and WebP carry an ICC profile or are sRGB. Profiles are applied with
//! `moxcms` to the straight-alpha pixels before premultiplication, as Skia's
//! decoders apply `skcms` (alpha passes through unchanged). An ICC profile
//! that does not parse is ignored and the image drawn as sRGB, as Chrome
//! ignores it.
use super::decode::Failure;
use moxcms::{
    Chromaticity, CicpColorPrimaries, CicpProfile, ColorPrimaries, ColorProfile, DataColorSpace,
    Layout, MatrixCoefficients, ToneReprCurve, TransferCharacteristics, TransformOptions,
};

/// The `gAMA` value Chrome treats as sRGB without `cHRM` (1/2.2 in units of
/// 1e-5): the transfer an untagged image already has.
const SRGB_GAMMA: u32 = 45_455;
/// H.273 code points of sRGB (BT.709 primaries, the sRGB transfer).
const BT709_PRIMARIES: u8 = 1;
const SRGB_TRANSFER: u8 = 13;
/// H.273 transfer characteristics of HDR content (PQ and HLG), which Chrome
/// tone-maps for an SDR output; not admitted.
const HDR_TRANSFERS: [u8; 2] = [16, 18];
const RGB_MATRIX: u8 = 0;
const GRAY_ALPHA_CHANNELS: usize = 2;
const RGBA_CHANNELS: usize = 4;

/// Where an image's colour space comes from.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum ColorSource {
    /// sRGB: no chunk or profile says otherwise.
    Untagged,
    /// An embedded ICC profile.
    Icc(Vec<u8>),
    /// PNG `cICP` code points (H.273).
    Cicp {
        primaries: u8,
        transfer: u8,
        matrix: u8,
        full_range: bool,
    },
    /// PNG `gAMA` (file gamma, the encoding exponent) with the `cHRM`
    /// white point and primaries, when present.
    Gamma {
        gamma: u32,
        chromaticities: Option<[(f32, f32); 4]>,
    },
}

/// A PNG's colour source in the PNG 3rd edition priority order.
pub(super) fn png_source(info: &::png::Info<'_>) -> ColorSource {
    if let Some(cicp) = info.coding_independent_code_points {
        return ColorSource::Cicp {
            primaries: cicp.color_primaries,
            transfer: cicp.transfer_function,
            matrix: cicp.matrix_coefficients,
            full_range: cicp.is_video_full_range_image,
        };
    }
    if let Some(profile) = &info.icc_profile {
        return ColorSource::Icc(profile.to_vec());
    }
    if info.srgb.is_some() {
        return ColorSource::Untagged;
    }
    let chromaticities = info.chrm_chunk.map(|chrm| {
        let pair =
            |(x, y): (::png::ScaledFloat, ::png::ScaledFloat)| (x.into_value(), y.into_value());
        [
            pair(chrm.white),
            pair(chrm.red),
            pair(chrm.green),
            pair(chrm.blue),
        ]
    });
    match info.gama_chunk {
        Some(gamma) if gamma.into_scaled() == SRGB_GAMMA && chromaticities.is_none() => {
            ColorSource::Untagged
        }
        Some(gamma) if gamma.into_scaled() > 0 => ColorSource::Gamma {
            gamma: gamma.into_scaled(),
            chromaticities,
        },
        _ => ColorSource::Untagged,
    }
}

/// The source profile, or `None` when the pixels are already sRGB.
fn profile(source: &ColorSource) -> Result<Option<ColorProfile>, Failure> {
    Ok(match source {
        ColorSource::Untagged => None,
        ColorSource::Icc(bytes) => ColorProfile::new_from_slice(bytes).ok(),
        ColorSource::Cicp {
            primaries,
            transfer,
            matrix,
            full_range,
        } => {
            if *matrix != RGB_MATRIX || !*full_range || HDR_TRANSFERS.contains(transfer) {
                return Err(Failure::Unsupported);
            }
            if *primaries == BT709_PRIMARIES && *transfer == SRGB_TRANSFER {
                return Ok(None);
            }
            let cicp = CicpProfile {
                color_primaries: CicpColorPrimaries::try_from(*primaries)
                    .map_err(|_| Failure::Unsupported)?,
                transfer_characteristics: TransferCharacteristics::try_from(*transfer)
                    .map_err(|_| Failure::Unsupported)?,
                matrix_coefficients: MatrixCoefficients::try_from(*matrix)
                    .map_err(|_| Failure::Unsupported)?,
                full_range: *full_range,
            };
            Some(ColorProfile::new_from_cicp(cicp))
        }
        ColorSource::Gamma {
            gamma,
            chromaticities,
        } => {
            let mut profile = ColorProfile::new_srgb();
            if let Some([white, red, green, blue]) = chromaticities {
                let point = |(x, y): (f32, f32)| Chromaticity::new(x, y);
                profile.update_rgb_colorimetry(
                    point(*white).to_xyyb(),
                    ColorPrimaries {
                        red: point(*red),
                        green: point(*green),
                        blue: point(*blue),
                    },
                );
            }
            // The decoding exponent is the reciprocal of the file gamma.
            let exponent = ::png::ScaledFloat::SCALING / *gamma as f32;
            let curve = ToneReprCurve::Parametric(vec![exponent]);
            profile.red_trc = Some(curve.clone());
            profile.green_trc = Some(curve.clone());
            profile.blue_trc = Some(curve);
            profile.cicp = None;
            Some(profile)
        }
    })
}

/// A transform from an RGB image's colour space to sRGB, applied to
/// straight-alpha RGBA8 pixels (alpha passes through).
#[derive(Clone)]
pub(crate) struct SrgbTransform(std::sync::Arc<moxcms::Transform8BitExecutor>);
impl std::fmt::Debug for SrgbTransform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SrgbTransform")
    }
}
impl PartialEq for SrgbTransform {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for SrgbTransform {}
impl SrgbTransform {
    /// Convert premultiplied RGBA8 pixels in place: each pixel is
    /// unpremultiplied, converted and premultiplied again (opaque pixels
    /// convert directly).
    pub(crate) fn apply_premultiplied(&self, pixels: &mut [u8]) {
        let mut straight: Vec<u8> = pixels
            .as_chunks::<RGBA_CHANNELS>()
            .0
            .iter()
            .flat_map(|pixel| {
                let alpha = pixel[3];
                let channel = |value: u8| match alpha {
                    0 => 0,
                    u8::MAX => value,
                    _ => ((u32::from(value) * 255 + u32::from(alpha) / 2) / u32::from(alpha))
                        .min(255) as u8,
                };
                [
                    channel(pixel[0]),
                    channel(pixel[1]),
                    channel(pixel[2]),
                    alpha,
                ]
            })
            .collect();
        let mut out = vec![0; straight.len()];
        if self.0.transform(&straight, &mut out).is_err() {
            return;
        }
        straight.clear();
        for (pixel, converted) in pixels
            .as_chunks_mut::<RGBA_CHANNELS>()
            .0
            .iter_mut()
            .zip(out.as_chunks::<RGBA_CHANNELS>().0.iter())
        {
            let alpha = converted[3];
            for channel in 0..3 {
                pixel[channel] = super::premultiply(converted[channel], alpha);
            }
            pixel[3] = alpha;
        }
    }
}

/// The transform of an RGB `source` to sRGB, or `None` when its pixels are
/// already sRGB or its profile is not an RGB input profile.
pub(super) fn srgb_transform(source: &ColorSource) -> Result<Option<SrgbTransform>, Failure> {
    let Some(profile) = profile(source)? else {
        return Ok(None);
    };
    if profile.color_space == DataColorSpace::Gray {
        return Ok(None);
    }
    Ok(profile
        .create_transform_8bit(
            Layout::Rgba,
            &ColorProfile::new_srgb(),
            Layout::Rgba,
            TransformOptions::default(),
        )
        .ok()
        .map(SrgbTransform))
}

/// Convert straight-alpha RGBA8 pixels to sRGB.
pub(super) fn to_srgb(source: &ColorSource, rgba: Vec<u8>) -> Result<Vec<u8>, Failure> {
    let Some(profile) = profile(source)? else {
        return Ok(rgba);
    };
    let srgb = ColorProfile::new_srgb();
    let options = TransformOptions::default();
    let mut out = vec![0; rgba.len()];
    if profile.color_space == DataColorSpace::Gray {
        // A gray profile reads the gray channel (replicated by decoding).
        let gray: Vec<u8> = rgba
            .as_chunks::<RGBA_CHANNELS>()
            .0
            .iter()
            .flat_map(|pixel| [pixel[0], pixel[3]])
            .collect();
        debug_assert_eq!(gray.len() / GRAY_ALPHA_CHANNELS * RGBA_CHANNELS, rgba.len());
        let Ok(transform) =
            profile.create_transform_8bit(Layout::GrayAlpha, &srgb, Layout::Rgba, options)
        else {
            return Ok(rgba);
        };
        transform
            .transform(&gray, &mut out)
            .map_err(|_| Failure::Malformed)?;
        return Ok(out);
    }
    let Ok(transform) = profile.create_transform_8bit(Layout::Rgba, &srgb, Layout::Rgba, options)
    else {
        // A profile that is not an RGB or gray input profile is ignored.
        return Ok(rgba);
    };
    transform
        .transform(&rgba, &mut out)
        .map_err(|_| Failure::Malformed)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The colour source a decoder reads from a PNG with `chunks` before
    /// its pixels.
    fn source(chunks: &[(&[u8; 4], &[u8])]) -> ColorSource {
        let mut out = Vec::new();
        {
            let mut encoder = ::png::Encoder::new(&mut out, 1, 1);
            encoder.set_color(::png::ColorType::Rgba);
            encoder.set_depth(::png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            for (kind, data) in chunks {
                writer
                    .write_chunk(::png::chunk::ChunkType(**kind), data)
                    .unwrap();
            }
            writer.write_image_data(&[0, 0, 0, 255]).unwrap();
        }
        let reader = ::png::Decoder::new(std::io::Cursor::new(out))
            .read_info()
            .unwrap();
        png_source(reader.info())
    }

    #[test]
    fn png_colour_chunks_take_the_third_edition_priority() {
        let cicp: &[u8] = &[9, 13, 0, 1];
        let srgb: &[u8] = &[0];
        let linear = 100_000u32.to_be_bytes();
        let srgb_gamma = SRGB_GAMMA.to_be_bytes();
        assert!(matches!(
            source(&[(b"cICP", cicp), (b"sRGB", srgb), (b"gAMA", &linear)]),
            ColorSource::Cicp { primaries: 9, .. }
        ));
        assert_eq!(
            source(&[(b"sRGB", srgb), (b"gAMA", &linear)]),
            ColorSource::Untagged
        );
        // `gAMA` 1/2.2 alone is sRGB, as Chrome treats it.
        assert_eq!(source(&[(b"gAMA", &srgb_gamma)]), ColorSource::Untagged);
        assert_eq!(
            source(&[(b"gAMA", &linear)]),
            ColorSource::Gamma {
                gamma: 100_000,
                chromaticities: None
            }
        );
        assert_eq!(source(&[]), ColorSource::Untagged);
    }

    #[test]
    fn colour_sources_convert_to_srgb_or_are_refused() {
        let cicp = |primaries, transfer, matrix, full_range| ColorSource::Cicp {
            primaries,
            transfer,
            matrix,
            full_range,
        };
        // PQ and HLG are tone-mapped by Chrome; narrow range and YCbCr
        // matrices have no RGB meaning.
        for refused in [
            cicp(9, 16, 0, true),
            cicp(9, 18, 0, true),
            cicp(1, 13, 0, false),
            cicp(1, 13, 1, true),
        ] {
            assert!(matches!(profile(&refused), Err(Failure::Unsupported)));
        }
        assert!(matches!(profile(&cicp(1, 13, 0, true)), Ok(None)));
        assert!(matches!(profile(&cicp(12, 13, 0, true)), Ok(Some(_))));
        // A linear file gamma brightens a mid grey; sRGB pixels pass through.
        let grey = vec![128, 128, 128, 255];
        let linear = ColorSource::Gamma {
            gamma: 100_000,
            chromaticities: None,
        };
        let converted = to_srgb(&linear, grey.clone()).unwrap();
        assert!((185..=190).contains(&converted[0]), "{converted:?}");
        assert_eq!(converted[3], 255);
        assert_eq!(to_srgb(&ColorSource::Untagged, grey.clone()).unwrap(), grey);
    }
}
