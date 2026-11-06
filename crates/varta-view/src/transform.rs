//! The view subset's CSS `transform`: a list of `translate()`,
//! `translateX()`, `translateY()` and `scale()` functions, resolved against
//! the element's border box with the initial `transform-origin` (its centre).
//!
//! A translation-only list moves the element's box and everything in it for
//! paint, hit testing and geometry, and never the layout of other boxes. A
//! list that scales keeps the boxes where layout put them and maps the
//! element's subtree through the transform at paint, for hits and in the
//! public geometry (see `layout::boxes_inner` and `paint`). Any transform
//! makes the element a stacking context. A scale factor must be positive.
use crate::{BoxRect, units::Units};

const NONE: &str = "none";
/// At most this many functions in one transform list.
pub const MAX_FUNCTIONS: usize = 8;

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
}

/// One transform primitive (CSS Transforms 2 §12): `translate()` and its
/// one-axis forms are the translate primitive, `scale()` the scale one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Function {
    /// Pixels and box fractions per axis: `[x px, y px, x fraction, y fraction]`.
    Translate([f32; 4]),
    Scale([f32; 2]),
}

impl Function {
    const IDENTITY_TRANSLATE: Self = Self::Translate([0.0; 4]);
    const IDENTITY_SCALE: Self = Self::Scale([1.0; 2]);

    fn identity(self) -> Self {
        match self {
            Self::Translate(_) => Self::IDENTITY_TRANSLATE,
            Self::Scale(_) => Self::IDENTITY_SCALE,
        }
    }
    fn same_primitive(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Translate(_), Self::Translate(_)) | (Self::Scale(_), Self::Scale(_))
        )
    }
}

/// A parsed `transform` list (`none` is empty), bounded by
/// [`MAX_FUNCTIONS`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct List {
    functions: [Function; MAX_FUNCTIONS],
    len: usize,
}

impl List {
    pub(crate) const NONE: Self = Self {
        functions: [Function::IDENTITY_TRANSLATE; MAX_FUNCTIONS],
        len: 0,
    };

    /// The list of a computed `transform` value; `None` for a value outside
    /// the subset (another function, a non-positive scale factor or more
    /// than [`MAX_FUNCTIONS`] functions).
    pub(crate) fn parse(value: &str, units: Units) -> Option<Self> {
        let mut list = Self::NONE;
        let mut rest = value.trim();
        if rest == NONE {
            return Some(list);
        }
        while !rest.is_empty() {
            let open = rest.find('(')?;
            let close = rest.find(')')?;
            let name = rest[..open].trim();
            let arguments: Vec<&str> = rest[open + 1..close].split(',').map(str::trim).collect();
            let translate = |x: Length, y: Length| {
                Function::Translate([x.pixels, y.pixels, x.fraction, y.fraction])
            };
            let function = match (name, arguments.as_slice()) {
                ("translate", [x]) => translate(Length::parse(x, units)?, Length::default()),
                ("translate", [x, y]) => {
                    translate(Length::parse(x, units)?, Length::parse(y, units)?)
                }
                ("translateX", [x]) => translate(Length::parse(x, units)?, Length::default()),
                ("translateY", [y]) => translate(Length::default(), Length::parse(y, units)?),
                ("scale", [s]) => {
                    let s = factor(s)?;
                    Function::Scale([s, s])
                }
                ("scale", [x, y]) => Function::Scale([factor(x)?, factor(y)?]),
                _ => return None,
            };
            if list.len == MAX_FUNCTIONS {
                return None;
            }
            list.functions[list.len] = function;
            list.len += 1;
            rest = rest[close + 1..].trim_start();
        }
        Some(list)
    }

    fn functions(&self) -> &[Function] {
        &self.functions[..self.len]
    }

    /// The two lists as equal-length lists of matching primitives, `none`
    /// standing for the identity functions of the other list (CSS
    /// Transforms 1 §9); `None` when they differ in length or primitives,
    /// which CSS interpolates through matrices instead.
    fn paired(self, other: Self) -> Option<(Self, Self)> {
        let identity = |list: Self| {
            let mut identity = list;
            for function in &mut identity.functions[..list.len] {
                *function = function.identity();
            }
            identity
        };
        let (a, b) = match (self.len, other.len) {
            (0, 0) => (self, other),
            (0, _) => (identity(other), other),
            (_, 0) => (self, identity(self)),
            _ => (self, other),
        };
        (a.len == b.len
            && a.functions()
                .iter()
                .zip(b.functions())
                .all(|(x, y)| x.same_primitive(*y)))
        .then_some((a, b))
    }

    /// Whether the lists interpolate function by function.
    pub(crate) fn interpolable(self, other: Self) -> bool {
        self.paired(other).is_some()
    }

    /// `self` at `progress` toward `end`, each function's arguments
    /// linearly; `end` when the lists do not pair.
    pub(crate) fn interpolate(self, end: Self, progress: f32) -> Self {
        let Some((from, to)) = self.paired(end) else {
            return end;
        };
        let mix = |a: f32, b: f32| a + (b - a) * progress;
        let mut list = to;
        for (index, function) in list.functions[..to.len].iter_mut().enumerate() {
            *function = match (from.functions[index], to.functions[index]) {
                (Function::Translate(a), Function::Translate(b)) => {
                    Function::Translate(std::array::from_fn(|i| mix(a[i], b[i])))
                }
                (Function::Scale(a), Function::Scale(b)) => {
                    Function::Scale(std::array::from_fn(|i| mix(a[i], b[i])))
                }
                (_, other) => other,
            };
        }
        list
    }

    /// The list as a `transform` value this module parses back.
    pub(crate) fn css(&self, digits: usize) -> String {
        if self.len == 0 {
            return NONE.to_owned();
        }
        let mut out = Vec::with_capacity(self.len);
        for function in self.functions() {
            out.push(match *function {
                // Pixels and fractions as one function when one part is zero
                // (translations compose by adding).
                Function::Translate([x, y, fx, fy]) if fx == 0.0 && fy == 0.0 => {
                    format!("translate({x:.digits$}px, {y:.digits$}px)")
                }
                Function::Translate([x, y, fx, fy]) if x == 0.0 && y == 0.0 => format!(
                    "translate({:.digits$}%, {:.digits$}%)",
                    fx * 100.0,
                    fy * 100.0
                ),
                Function::Translate([x, y, fx, fy]) => format!(
                    "translate({x:.digits$}px, {y:.digits$}px) translate({:.digits$}%, {:.digits$}%)",
                    fx * 100.0,
                    fy * 100.0
                ),
                Function::Scale([x, y]) => format!("scale({x:.digits$}, {y:.digits$})"),
            });
        }
        out.join(" ")
    }

    /// The transform of a box with border box `rect`, around its centre.
    pub(crate) fn affine(&self, rect: BoxRect) -> Affine {
        let mut matrix = Affine::IDENTITY;
        for function in self.functions() {
            matrix = matrix.then(match *function {
                Function::Translate([px, py, fx, fy]) => Affine {
                    x: px + fx * rect.width,
                    y: py + fy * rect.height,
                    ..Affine::IDENTITY
                },
                Function::Scale([x, y]) => Affine {
                    scale_x: x,
                    scale_y: y,
                    ..Affine::IDENTITY
                },
            });
        }
        // Around the origin: translate(origin) · M · translate(-origin).
        let (origin_x, origin_y) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
        let around = |o: f32, s: f32, t: f32| o + t - s * o;
        Affine {
            x: around(origin_x, matrix.scale_x, matrix.x),
            y: around(origin_y, matrix.scale_y, matrix.y),
            ..matrix
        }
    }
}

/// A 2D transform without rotation or skew: `x' = scale_x * x + x_offset`
/// and likewise for y, in CSS pixels of the view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    pub scale_x: f32,
    pub scale_y: f32,
    pub x: f32,
    pub y: f32,
}

impl Affine {
    pub const IDENTITY: Self = Self {
        scale_x: 1.0,
        scale_y: 1.0,
        x: 0.0,
        y: 0.0,
    };
    /// `self` after `other` (the matrix product `self · other`).
    #[must_use]
    pub fn then(self, other: Self) -> Self {
        Self {
            scale_x: self.scale_x * other.scale_x,
            scale_y: self.scale_y * other.scale_y,
            x: self.scale_x * other.x + self.x,
            y: self.scale_y * other.y + self.y,
        }
    }
    /// The inverse transform (factors are positive).
    #[must_use]
    pub fn inverse(self) -> Self {
        Self {
            scale_x: 1.0 / self.scale_x,
            scale_y: 1.0 / self.scale_y,
            x: -self.x / self.scale_x,
            y: -self.y / self.scale_y,
        }
    }
    /// Whether the transform only translates.
    pub(crate) fn translation(self) -> Option<(f32, f32)> {
        (self.scale_x == 1.0 && self.scale_y == 1.0).then_some((self.x, self.y))
    }
    /// The image of `rect` (its bounding box; positive factors keep it a
    /// rectangle).
    #[must_use]
    pub fn rect(self, rect: BoxRect) -> BoxRect {
        BoxRect {
            x: self.scale_x * rect.x + self.x,
            y: self.scale_y * rect.y + self.y,
            width: self.scale_x * rect.width,
            height: self.scale_y * rect.height,
        }
    }
}

/// A positive, finite scale factor.
fn factor(value: &str) -> Option<f32> {
    value
        .parse::<f32>()
        .ok()
        .filter(|n| n.is_finite() && *n > 0.0)
}

/// The transform of a box with border box `rect` for a computed `transform`
/// value: `None` for `none`, an error for a value the view cannot resolve.
pub(crate) fn resolve(value: &str, units: Units, rect: BoxRect) -> Result<Option<Affine>, ()> {
    let list = List::parse(value, units).ok_or(())?;
    Ok((list.len > 0).then(|| list.affine(rect)))
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
        // Factors must be positive.
        assert!(super::resolve("scale(0)", units(), BOX).is_err());
        assert!(super::resolve("scale(-1, 1)", units(), BOX).is_err());
    }

    #[test]
    fn scaling_is_around_the_centre_in_list_order() {
        let affine = resolve("scale(2) translateX(1px)", units(), BOX)
            .unwrap()
            .unwrap();
        // Centre (30, 35) stays; the translation is scaled.
        assert_eq!(affine.scale_x, 2.0);
        assert_eq!(30.0 * affine.scale_x + affine.x, 32.0);
        assert_eq!(35.0 * affine.scale_y + affine.y, 35.0);
        let other = resolve("translateX(1px) scale(2)", units(), BOX)
            .unwrap()
            .unwrap();
        assert_eq!(30.0 * other.scale_x + other.x, 31.0);
        assert_eq!(
            other.rect(BOX),
            BoxRect {
                x: -9.0,
                y: 5.0,
                width: 80.0,
                height: 60.0
            }
        );
    }

    #[test]
    fn lists_interpolate_by_primitive_and_none_takes_identities() {
        let list = |value| List::parse(value, units()).unwrap();
        let half = list("none").interpolate(list("translateY(-4px) scale(0.5)"), 0.5);
        assert_eq!(half, list("translateY(-2px) scale(0.75)"));
        assert_eq!(
            list("translateX(4px)").interpolate(list("translateY(2px)"), 0.5),
            list("translate(2px, 1px)")
        );
        // Different primitives do not pair (CSS interpolates their matrices).
        assert!(!list("scale(2)").interpolable(list("translateX(1px)")));
        assert!(!list("scale(2)").interpolable(list("scale(2) scale(2)")));
        // The serialized value parses back to the same list.
        for value in [
            "translate(10%, 20%) scale(0.98, 1.5)",
            "translate(-2px, 3px)",
        ] {
            let serialized = list(value).css(6);
            assert_eq!(List::parse(&serialized, units()).unwrap(), list(value));
        }
        assert!(List::parse(&"scale(2) ".repeat(MAX_FUNCTIONS + 1), units()).is_none());
    }
}
