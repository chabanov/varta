//! The view subset's CSS `transform`: a list of `translate()`,
//! `translateX()`, `translateY()` and `scale()` functions, resolved against
//! the element's border box with the initial `transform-origin` (its centre).
//!
//! A translation moves the element's box and everything in it for paint, hit
//! testing and geometry, and never the layout of other boxes. Any transform
//! makes the element a stacking context and the containing block of its
//! absolutely positioned descendants. A list that scales (a factor other than
//! one) is refused: native paint has no scaled subtree yet.
use crate::{BoxRect, units::Units};

const NONE: &str = "none";

/// One length of a translation: pixels plus a fraction of the box's size on
/// that axis.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Length {
    pixels: f32,
    fraction: f32,
}

impl Length {
    fn parse(value: &str, units: Units) -> Option<Self> {
        let value = value.trim();
        match value.strip_suffix('%') {
            Some(number) => {
                let number = number.parse::<f32>().ok().filter(|n| n.is_finite())?;
                Some(Self {
                    pixels: 0.0,
                    fraction: number / 100.0,
                })
            }
            None => Some(Self {
                pixels: units.pixels(value, true)?,
                fraction: 0.0,
            }),
        }
    }
    fn resolve(self, size: f32) -> f32 {
        self.pixels + self.fraction * size
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Function {
    Translate(Length, Length),
    Scale(f32, f32),
}

/// A 2D transform without rotation or skew: `x' = scale_x * x + x_offset`
/// and likewise for y, in CSS pixels of the view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Affine {
    pub scale_x: f32,
    pub scale_y: f32,
    pub x: f32,
    pub y: f32,
}

impl Affine {
    const IDENTITY: Self = Self {
        scale_x: 1.0,
        scale_y: 1.0,
        x: 0.0,
        y: 0.0,
    };
    /// `self` after `other` (the matrix product `self · other`).
    fn then(self, other: Self) -> Self {
        Self {
            scale_x: self.scale_x * other.scale_x,
            scale_y: self.scale_y * other.scale_y,
            x: self.scale_x * other.x + self.x,
            y: self.scale_y * other.y + self.y,
        }
    }
    /// Whether the transform only translates.
    pub(crate) fn translation(self) -> Option<(f32, f32)> {
        (self.scale_x == 1.0 && self.scale_y == 1.0).then_some((self.x, self.y))
    }
}

fn functions(value: &str, units: Units) -> Option<Vec<Function>> {
    let mut rest = value.trim();
    let mut list = Vec::new();
    while !rest.is_empty() {
        let open = rest.find('(')?;
        let close = rest.find(')')?;
        let name = rest[..open].trim();
        let arguments: Vec<&str> = rest[open + 1..close].split(',').map(str::trim).collect();
        list.push(match (name, arguments.as_slice()) {
            ("translate", [x]) => Function::Translate(Length::parse(x, units)?, Length::default()),
            ("translate", [x, y]) => {
                Function::Translate(Length::parse(x, units)?, Length::parse(y, units)?)
            }
            ("translateX", [x]) => Function::Translate(Length::parse(x, units)?, Length::default()),
            ("translateY", [y]) => Function::Translate(Length::default(), Length::parse(y, units)?),
            ("scale", [s]) => {
                let s = number(s)?;
                Function::Scale(s, s)
            }
            ("scale", [x, y]) => Function::Scale(number(x)?, number(y)?),
            _ => return None,
        });
        rest = rest[close + 1..].trim_start();
    }
    Some(list)
}

/// A translation-only `transform` as its pixel and box-fraction parts per
/// axis (`[x pixels, y pixels, x fraction, y fraction]`): `none` is zero; a
/// list that scales is `None`. Translations compose by adding, so this is
/// the whole list for transitions, which interpolate each part linearly.
pub(crate) fn translation_parts(value: &str, units: Units) -> Option<[f32; 4]> {
    if value.trim() == NONE {
        return Some([0.0; 4]);
    }
    let mut parts = [0.0; 4];
    for function in functions(value, units)? {
        match function {
            Function::Translate(x, y) => {
                parts[0] += x.pixels;
                parts[1] += y.pixels;
                parts[2] += x.fraction;
                parts[3] += y.fraction;
            }
            Function::Scale(x, y) if x == 1.0 && y == 1.0 => {}
            Function::Scale(..) => return None,
        }
    }
    Some(parts)
}

fn number(value: &str) -> Option<f32> {
    value.parse::<f32>().ok().filter(|n| n.is_finite())
}

/// The transform of a box with border box `rect` for a computed `transform`
/// value: `None` for `none`, an error for a value the view cannot resolve.
pub(crate) fn resolve(value: &str, units: Units, rect: BoxRect) -> Result<Option<Affine>, ()> {
    if value.trim() == NONE {
        return Ok(None);
    }
    let list = functions(value, units).ok_or(())?;
    let mut matrix = Affine::IDENTITY;
    for function in list {
        matrix = matrix.then(match function {
            Function::Translate(x, y) => Affine {
                x: x.resolve(rect.width),
                y: y.resolve(rect.height),
                ..Affine::IDENTITY
            },
            Function::Scale(x, y) => Affine {
                scale_x: x,
                scale_y: y,
                ..Affine::IDENTITY
            },
        });
    }
    // Around the origin: translate(origin) · M · translate(-origin).
    let (origin_x, origin_y) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    let around = |o: f32, s: f32, t: f32| o + t - s * o;
    Ok(Some(Affine {
        x: around(origin_x, matrix.scale_x, matrix.x),
        y: around(origin_y, matrix.scale_y, matrix.y),
        ..matrix
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units() -> Units {
        Units::new(&crate::Environment::default()).unwrap()
    }
    const BOX: BoxRect = BoxRect {
        x: 10.0,
        y: 20.0,
        width: 40.0,
        height: 30.0,
    };

    #[test]
    fn translations_resolve_against_the_border_box_and_compose() {
        let resolve = |value| {
            resolve(value, units(), BOX)
                .unwrap()
                .map(Affine::translation)
        };
        assert_eq!(resolve("none"), None);
        assert_eq!(resolve("translateY(1px)"), Some(Some((0.0, 1.0))));
        assert_eq!(resolve("translate(50%, -1rem)"), Some(Some((20.0, -16.0))));
        assert_eq!(
            resolve("translateX(2px) translate(1px, 10%)"),
            Some(Some((3.0, 3.0)))
        );
        // A unit scale is the identity; another factor is not a translation.
        assert_eq!(resolve("scale(1) translateX(1px)"), Some(Some((1.0, 0.0))));
        assert_eq!(resolve("scale(0.98)"), Some(None));
        assert!(super::resolve("rotate(4deg)", units(), BOX).is_err());
    }

    #[test]
    fn scaling_is_around_the_centre() {
        let affine = resolve("scale(2) translateX(1px)", units(), BOX)
            .unwrap()
            .unwrap();
        // Centre (30, 35) stays; the translation is scaled.
        assert_eq!(affine.scale_x, 2.0);
        assert_eq!(30.0 * affine.scale_x + affine.x, 32.0);
        assert_eq!(35.0 * affine.scale_y + affine.y, 35.0);
    }
}
