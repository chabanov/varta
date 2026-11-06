//! `<color>` values of CSS Color 4 and `color-mix()` of CSS Color 5, over
//! the value tokens of [`super::V`], and their conversion to the sRGB
//! colours the native renderer paints.
//!
//! Admitted: hexadecimal colours with 3, 4, 6 or 8 digits (Color 4 §5.2),
//! `transparent` and `currentcolor` (§6.2, §6.4), `rgb()`/`rgba()` (§5.1)
//! and `hsl()`/`hsla()` (§7) in their legacy and modern syntaxes, `hwb()`
//! (§8), `lab()`/`lch()` (§9.3), `oklab()`/`oklch()` (§9.4), `none` missing
//! components (§4.4) and `color-mix()` (Color 5 §2) in the sRGB, linear
//! sRGB, Lab, Oklab, XYZ, HSL, HWB, LCH and OkLCh spaces with every hue
//! interpolation method (Color 4 §13). Conversions use the matrices of the
//! specification's sample code (Color 4 §18); colours outside the sRGB
//! gamut are mapped with the CSS gamut mapping algorithm (§14.2, binary
//! search with a JND of 0.02 in ΔEOK). Named colours, system colours,
//! `color()`, relative colours and `light-dark()` are outside the subset.
use super::{V, math};
#[allow(unused_imports)]
use crate::view::prelude::*;

/// Hexadecimal digit counts of Color 4 §5.2.
pub const HEX_DIGITS: [usize; 4] = [3, 4, 6, 8];
/// Colours one `color-mix()` admits (Color 5 §3 admits any number; the
/// bound keeps the work of a value bounded like its token count).
pub const MAX_MIX_COLORS: usize = 16;
/// The just-noticeable difference of the gamut mapping algorithm (§14.2).
const JND: f64 = 0.02;
/// The binary search's precision (§14.2.2).
const GAMUT_EPSILON: f64 = 0.0001;
/// Chroma at or below which a polar colour's hue is powerless (achromatic),
/// per space; HSL and HWB use their own tests (§4.4.1).
const ACHROMATIC_CHROMA: f64 = 1e-6;

/// The named colours of CSS Color 4 §6.1 (sRGB, 8-bit channels).
pub const NAMED_COLORS: &[(&str, [u8; 3])] = &[
    ("aliceblue", [240, 248, 255]),
    ("antiquewhite", [250, 235, 215]),
    ("aqua", [0, 255, 255]),
    ("aquamarine", [127, 255, 212]),
    ("azure", [240, 255, 255]),
    ("beige", [245, 245, 220]),
    ("bisque", [255, 228, 196]),
    ("black", [0, 0, 0]),
    ("blanchedalmond", [255, 235, 205]),
    ("blue", [0, 0, 255]),
    ("blueviolet", [138, 43, 226]),
    ("brown", [165, 42, 42]),
    ("burlywood", [222, 184, 135]),
    ("cadetblue", [95, 158, 160]),
    ("chartreuse", [127, 255, 0]),
    ("chocolate", [210, 105, 30]),
    ("coral", [255, 127, 80]),
    ("cornflowerblue", [100, 149, 237]),
    ("cornsilk", [255, 248, 220]),
    ("crimson", [220, 20, 60]),
    ("cyan", [0, 255, 255]),
    ("darkblue", [0, 0, 139]),
    ("darkcyan", [0, 139, 139]),
    ("darkgoldenrod", [184, 134, 11]),
    ("darkgray", [169, 169, 169]),
    ("darkgreen", [0, 100, 0]),
    ("darkgrey", [169, 169, 169]),
    ("darkkhaki", [189, 183, 107]),
    ("darkmagenta", [139, 0, 139]),
    ("darkolivegreen", [85, 107, 47]),
    ("darkorange", [255, 140, 0]),
    ("darkorchid", [153, 50, 204]),
    ("darkred", [139, 0, 0]),
    ("darksalmon", [233, 150, 122]),
    ("darkseagreen", [143, 188, 143]),
    ("darkslateblue", [72, 61, 139]),
    ("darkslategray", [47, 79, 79]),
    ("darkslategrey", [47, 79, 79]),
    ("darkturquoise", [0, 206, 209]),
    ("darkviolet", [148, 0, 211]),
    ("deeppink", [255, 20, 147]),
    ("deepskyblue", [0, 191, 255]),
    ("dimgray", [105, 105, 105]),
    ("dimgrey", [105, 105, 105]),
    ("dodgerblue", [30, 144, 255]),
    ("firebrick", [178, 34, 34]),
    ("floralwhite", [255, 250, 240]),
    ("forestgreen", [34, 139, 34]),
    ("fuchsia", [255, 0, 255]),
    ("gainsboro", [220, 220, 220]),
    ("ghostwhite", [248, 248, 255]),
    ("gold", [255, 215, 0]),
    ("goldenrod", [218, 165, 32]),
    ("gray", [128, 128, 128]),
    ("green", [0, 128, 0]),
    ("greenyellow", [173, 255, 47]),
    ("grey", [128, 128, 128]),
    ("honeydew", [240, 255, 240]),
    ("hotpink", [255, 105, 180]),
    ("indianred", [205, 92, 92]),
    ("indigo", [75, 0, 130]),
    ("ivory", [255, 255, 240]),
    ("khaki", [240, 230, 140]),
    ("lavender", [230, 230, 250]),
    ("lavenderblush", [255, 240, 245]),
    ("lawngreen", [124, 252, 0]),
    ("lemonchiffon", [255, 250, 205]),
    ("lightblue", [173, 216, 230]),
    ("lightcoral", [240, 128, 128]),
    ("lightcyan", [224, 255, 255]),
    ("lightgoldenrodyellow", [250, 250, 210]),
    ("lightgray", [211, 211, 211]),
    ("lightgreen", [144, 238, 144]),
    ("lightgrey", [211, 211, 211]),
    ("lightpink", [255, 182, 193]),
    ("lightsalmon", [255, 160, 122]),
    ("lightseagreen", [32, 178, 170]),
    ("lightskyblue", [135, 206, 250]),
    ("lightslategray", [119, 136, 153]),
    ("lightslategrey", [119, 136, 153]),
    ("lightsteelblue", [176, 196, 222]),
    ("lightyellow", [255, 255, 224]),
    ("lime", [0, 255, 0]),
    ("limegreen", [50, 205, 50]),
    ("linen", [250, 240, 230]),
    ("magenta", [255, 0, 255]),
    ("maroon", [128, 0, 0]),
    ("mediumaquamarine", [102, 205, 170]),
    ("mediumblue", [0, 0, 205]),
    ("mediumorchid", [186, 85, 211]),
    ("mediumpurple", [147, 112, 219]),
    ("mediumseagreen", [60, 179, 113]),
    ("mediumslateblue", [123, 104, 238]),
    ("mediumspringgreen", [0, 250, 154]),
    ("mediumturquoise", [72, 209, 204]),
    ("mediumvioletred", [199, 21, 133]),
    ("midnightblue", [25, 25, 112]),
    ("mintcream", [245, 255, 250]),
    ("mistyrose", [255, 228, 225]),
    ("moccasin", [255, 228, 181]),
    ("navajowhite", [255, 222, 173]),
    ("navy", [0, 0, 128]),
    ("oldlace", [253, 245, 230]),
    ("olive", [128, 128, 0]),
    ("olivedrab", [107, 142, 35]),
    ("orange", [255, 165, 0]),
    ("orangered", [255, 69, 0]),
    ("orchid", [218, 112, 214]),
    ("palegoldenrod", [238, 232, 170]),
    ("palegreen", [152, 251, 152]),
    ("paleturquoise", [175, 238, 238]),
    ("palevioletred", [219, 112, 147]),
    ("papayawhip", [255, 239, 213]),
    ("peachpuff", [255, 218, 185]),
    ("peru", [205, 133, 63]),
    ("pink", [255, 192, 203]),
    ("plum", [221, 160, 221]),
    ("powderblue", [176, 224, 230]),
    ("purple", [128, 0, 128]),
    ("rebeccapurple", [102, 51, 153]),
    ("red", [255, 0, 0]),
    ("rosybrown", [188, 143, 143]),
    ("royalblue", [65, 105, 225]),
    ("saddlebrown", [139, 69, 19]),
    ("salmon", [250, 128, 114]),
    ("sandybrown", [244, 164, 96]),
    ("seagreen", [46, 139, 87]),
    ("seashell", [255, 245, 238]),
    ("sienna", [160, 82, 45]),
    ("silver", [192, 192, 192]),
    ("skyblue", [135, 206, 235]),
    ("slateblue", [106, 90, 205]),
    ("slategray", [112, 128, 144]),
    ("slategrey", [112, 128, 144]),
    ("snow", [255, 250, 250]),
    ("springgreen", [0, 255, 127]),
    ("steelblue", [70, 130, 180]),
    ("tan", [210, 180, 140]),
    ("teal", [0, 128, 128]),
    ("thistle", [216, 191, 216]),
    ("tomato", [255, 99, 71]),
    ("turquoise", [64, 224, 208]),
    ("violet", [238, 130, 238]),
    ("wheat", [245, 222, 179]),
    ("white", [255, 255, 255]),
    ("whitesmoke", [245, 245, 245]),
    ("yellow", [255, 255, 0]),
    ("yellowgreen", [154, 205, 50]),
];

/// The named colour `name` (ASCII case-insensitively), if any.
#[must_use]
pub fn named(name: &str) -> Option<[u8; 3]> {
    NAMED_COLORS
        .iter()
        .find(|(known, _)| name.eq_ignore_ascii_case(known))
        .map(|(_, rgb)| *rgb)
}

/// The colour spaces of the subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Space {
    Srgb,
    SrgbLinear,
    Hsl,
    Hwb,
    Lab,
    Lch,
    Oklab,
    Oklch,
    XyzD65,
    XyzD50,
}
impl Space {
    fn polar_hue(self) -> Option<usize> {
        match self {
            Self::Hsl | Self::Hwb => Some(0),
            Self::Lch | Self::Oklch => Some(2),
            _ => None,
        }
    }
    fn named(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().as_str() {
            "srgb" => Self::Srgb,
            "srgb-linear" => Self::SrgbLinear,
            "hsl" => Self::Hsl,
            "hwb" => Self::Hwb,
            "lab" => Self::Lab,
            "lch" => Self::Lch,
            "oklab" => Self::Oklab,
            "oklch" => Self::Oklch,
            "xyz" | "xyz-d65" => Self::XyzD65,
            "xyz-d50" => Self::XyzD50,
            _ => return None,
        })
    }
}

/// A colour in its own space, with missing (`none`) components.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub space: Space,
    pub components: [Option<f64>; 3],
    pub alpha: Option<f64>,
}

/// A parsed `<color>`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Parsed {
    Color(Color),
    /// `currentcolor`, or a mix that involves it: resolved only at used-value
    /// time, which the native renderer does not do inside functions yet.
    Current,
    Unresolved,
}

/// Parse the `<color>` at `tokens[at]`; returns it and the index past it.
#[must_use]
pub(crate) fn parse(tokens: &[V], at: usize) -> Option<(Parsed, usize)> {
    match tokens.get(at)? {
        V::Hash(hex) => Some((Parsed::Color(hex_color(hex)?), at + 1)),
        V::Ident(name) if name.eq_ignore_ascii_case("transparent") => Some((
            Parsed::Color(Color {
                space: Space::Srgb,
                components: [Some(0.0); 3],
                alpha: Some(0.0),
            }),
            at + 1,
        )),
        V::Ident(name) if name.eq_ignore_ascii_case("currentcolor") => {
            Some((Parsed::Current, at + 1))
        }
        V::Ident(name) => {
            let rgb = named(name)?;
            Some((
                Parsed::Color(Color {
                    space: Space::Srgb,
                    components: rgb.map(|c| Some(f64::from(c) / 255.0)),
                    alpha: Some(1.0),
                }),
                at + 1,
            ))
        }
        V::Function(name) => {
            let name = name.to_ascii_lowercase();
            let mut parser = Arguments { tokens, at: at + 1 };
            let parsed = match name.as_str() {
                "rgb" | "rgba" => Parsed::Color(parser.rgb()?),
                "hsl" | "hsla" => Parsed::Color(parser.hsl(Space::Hsl)?),
                "hwb" => Parsed::Color(parser.modern(Space::Hwb)?),
                "lab" => Parsed::Color(parser.modern(Space::Lab)?),
                "lch" => Parsed::Color(parser.modern(Space::Lch)?),
                "oklab" => Parsed::Color(parser.modern(Space::Oklab)?),
                "oklch" => Parsed::Color(parser.modern(Space::Oklch)?),
                "color-mix" => parser.mix()?,
                _ => return None,
            };
            (tokens.get(parser.at) == Some(&V::Close)).then_some((parsed, parser.at + 1))
        }
        _ => None,
    }
}

fn hex_color(hex: &str) -> Option<Color> {
    if !HEX_DIGITS.contains(&hex.len()) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let digits: Vec<u32> = hex.chars().filter_map(|c| c.to_digit(16)).collect();
    let channel = |i: usize| -> f64 {
        let value = if hex.len() <= 4 {
            digits[i] * 17
        } else {
            digits[2 * i] * 16 + digits[2 * i + 1]
        };
        f64::from(value) / 255.0
    };
    let alpha = if hex.len() == 4 || hex.len() == 8 {
        channel(3)
    } else {
        1.0
    };
    Some(Color {
        space: Space::Srgb,
        components: [Some(channel(0)), Some(channel(1)), Some(channel(2))],
        alpha: Some(alpha),
    })
}

/// A component token: a number, a percentage, an angle, `none`, or a math
/// function resolving to one of them.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Component {
    Number(f64),
    Percent(f64),
    Degrees(f64),
    None,
}

struct Arguments<'a> {
    tokens: &'a [V],
    at: usize,
}

/// Math in a colour component has no context: lengths are not admitted.
const NO_LENGTHS: math::Resolution = math::Resolution {
    em: None,
    rem: 0.0,
    viewport_width: 0.0,
    viewport_height: 0.0,
};

impl Arguments<'_> {
    fn peek(&self) -> Option<&V> {
        self.tokens.get(self.at)
    }
    fn component(&mut self) -> Option<Component> {
        let token = self.peek()?;
        if let V::Ident(word) = token
            && word.eq_ignore_ascii_case("none")
        {
            self.at += 1;
            return Some(Component::None);
        }
        if math::is_math(Some(token)) {
            let (expression, end) = math::parse(self.tokens, self.at)?;
            let kind = expression.resolved_type(None)?;
            let value = expression.evaluate(&NO_LENGTHS)?.value;
            self.at = end;
            return Some(if kind == math::Type::NUMBER {
                Component::Number(value)
            } else if kind == math::Type::of(math::Base::Percent) {
                Component::Percent(value)
            } else if kind == math::Type::of(math::Base::Angle) {
                Component::Degrees(value)
            } else {
                return None;
            });
        }
        let (value, unit) = math::numeric(token)?;
        if !value.is_finite() {
            return None;
        }
        self.at += 1;
        Some(match unit {
            math::Unit::Number => Component::Number(value),
            math::Unit::Percent => Component::Percent(value),
            math::Unit::Angle(factor) => Component::Degrees(value * factor),
            _ => return None,
        })
    }
    fn eat(&mut self, token: &V) -> bool {
        let found = self.peek() == Some(token);
        if found {
            self.at += 1;
        }
        found
    }
    /// The optional alpha of the modern syntax: `/ [<alpha-value> | none]`.
    fn modern_alpha(&mut self) -> Option<Option<f64>> {
        if !self.eat(&V::Slash) {
            return Some(Some(1.0));
        }
        alpha(self.component()?)
    }
    /// Three components of the modern (space-separated) syntax.
    fn three(&mut self) -> Option<[Component; 3]> {
        Some([self.component()?, self.component()?, self.component()?])
    }
    /// `rgb()`/`rgba()` (Color 4 §5.1).
    fn rgb(&mut self) -> Option<Color> {
        let first = self.component()?;
        let (components, alpha) = if self.peek() == Some(&V::Comma) {
            // Legacy syntax: all numbers or all percentages, no `none`.
            let mut channels = vec![first];
            for _ in 0..2 {
                self.eat(&V::Comma).then_some(())?;
                channels.push(self.component()?);
            }
            let percent = matches!(first, Component::Percent(_));
            if !channels.iter().all(|c| match c {
                Component::Percent(_) => percent,
                Component::Number(_) => !percent,
                _ => false,
            }) {
                return None;
            }
            let alpha = if self.eat(&V::Comma) {
                legacy_alpha(self.component()?)?
            } else {
                1.0
            };
            ([channels[0], channels[1], channels[2]], Some(alpha))
        } else {
            let rest = [first, self.component()?, self.component()?];
            if rest.iter().any(|c| matches!(c, Component::Degrees(_))) {
                return None;
            }
            (rest, self.modern_alpha()?)
        };
        let channel = |component: Component| match component {
            Component::Number(value) => Some(Some((value / 255.0).clamp(0.0, 1.0))),
            Component::Percent(value) => Some(Some((value / 100.0).clamp(0.0, 1.0))),
            Component::None => Some(None),
            Component::Degrees(_) => None,
        };
        Some(Color {
            space: Space::Srgb,
            components: [
                channel(components[0])?,
                channel(components[1])?,
                channel(components[2])?,
            ],
            alpha,
        })
    }
    /// `hsl()`/`hsla()` (Color 4 §7), legacy or modern.
    fn hsl(&mut self, space: Space) -> Option<Color> {
        let save = self.at;
        let hue = self.component()?;
        if self.peek() == Some(&V::Comma) {
            let hue = match hue {
                Component::Number(value) | Component::Degrees(value) => value,
                _ => return None,
            };
            let mut rest = Vec::new();
            for _ in 0..2 {
                self.eat(&V::Comma).then_some(())?;
                match self.component()? {
                    Component::Percent(value) => rest.push(value),
                    _ => return None,
                }
            }
            let alpha = if self.eat(&V::Comma) {
                legacy_alpha(self.component()?)?
            } else {
                1.0
            };
            return Some(Color {
                space,
                components: [
                    Some(hue),
                    Some(rest[0].clamp(0.0, 100.0)),
                    Some(rest[1].clamp(0.0, 100.0)),
                ],
                alpha: Some(alpha),
            });
        }
        self.at = save;
        self.modern(space)
    }
    /// The modern syntax of every space: three components and an alpha.
    fn modern(&mut self, space: Space) -> Option<Color> {
        let [a, b, c] = self.three()?;
        let alpha = self.modern_alpha()?;
        // Percent references and ranges per space (Color 4 §7, §8, §9.3, §9.4).
        let (references, hue) = match space {
            Space::Hsl | Space::Hwb => ([None, Some(100.0), Some(100.0)], Some(0)),
            Space::Lab => ([Some(100.0), Some(125.0), Some(125.0)], None),
            Space::Lch => ([Some(100.0), Some(150.0), None], Some(2)),
            Space::Oklab => ([Some(1.0), Some(0.4), Some(0.4)], None),
            Space::Oklch => ([Some(1.0), Some(0.4), None], Some(2)),
            _ => return None,
        };
        let mut components = [None; 3];
        for (index, component) in [a, b, c].into_iter().enumerate() {
            components[index] = match (component, hue == Some(index)) {
                (Component::None, _) => None,
                (Component::Number(value) | Component::Degrees(value), true) => Some(value),
                (Component::Number(value), false) => Some(value),
                (Component::Percent(value), false) => Some(value / 100.0 * references[index]?),
                _ => return None,
            };
        }
        // Clamp lightness and chroma to their valid ranges.
        match space {
            Space::Lab | Space::Lch => {
                components[0] = components[0].map(|l| l.clamp(0.0, 100.0));
            }
            Space::Oklab | Space::Oklch => {
                components[0] = components[0].map(|l| l.clamp(0.0, 1.0));
            }
            Space::Hsl | Space::Hwb => {
                components[1] = components[1].map(|v| v.clamp(0.0, 100.0));
                components[2] = components[2].map(|v| v.clamp(0.0, 100.0));
            }
            _ => {}
        }
        if matches!(space, Space::Lch | Space::Oklch) {
            components[1] = components[1].map(|c| c.max(0.0));
        }
        Some(Color {
            space,
            components,
            alpha,
        })
    }
    /// `color-mix()` (Color 5 §2): `in <space> [<hue-method> hue]?, <color>
    /// && <percentage>?, <color> && <percentage>?`.
    fn mix(&mut self) -> Option<Parsed> {
        // Color 5 §3.1: without an interpolation method, Oklab.
        let (space, method, explicit) = match self.peek()? {
            V::Ident(word) if word.eq_ignore_ascii_case("in") => {
                self.at += 1;
                let (space, method) = self.method()?;
                (space, method, true)
            }
            _ => (Space::Oklab, Hue::Shorter, false),
        };
        // Color 5 §3: one or more colours, each with an optional percentage.
        let mut items = Vec::new();
        loop {
            if explicit || !items.is_empty() {
                if self.peek() == Some(&V::Close) && !items.is_empty() {
                    break;
                }
                self.eat(&V::Comma).then_some(())?;
            }
            let mut percent = self.percent();
            let (color, end) = parse(self.tokens, self.at)?;
            self.at = end;
            if percent.is_none() {
                percent = self.percent();
            }
            if percent.is_some_and(|p| !(0.0..=100.0).contains(&p)) {
                return None;
            }
            items.push((color, percent));
            if items.len() > MAX_MIX_COLORS {
                return None;
            }
            if self.peek() == Some(&V::Close) {
                break;
            }
        }
        let (weights, leftover) =
            normalize_mix(&items.iter().map(|item| item.1).collect::<Vec<_>>());
        let mut colors = Vec::with_capacity(items.len());
        for (parsed, _) in &items {
            match parsed {
                Parsed::Color(color) => colors.push(*color),
                _ => return Some(Parsed::Current),
            }
        }
        // Color 5 §3.3: mix in order, each result weighted by the sum of its
        // inputs' percentages.
        let mut stack: Vec<(Color, f64)> = colors.into_iter().zip(weights).rev().collect();
        let mut mixed = loop {
            let (a, pa) = stack.pop()?;
            let Some((b, pb)) = stack.pop() else {
                break convert_carrying(a, space);
            };
            let combined = pa + pb;
            let progress = if combined == 0.0 { 0.5 } else { pb / combined };
            stack.push((interpolate(a, b, progress, space, method), combined));
        };
        mixed.alpha = mixed.alpha.map(|alpha| alpha * (1.0 - leftover));
        Some(Parsed::Color(mixed))
    }
    /// A `<color-interpolation-method>` after `in` (Color 4 §13.1).
    fn method(&mut self) -> Option<(Space, Hue)> {
        let V::Ident(name) = self.peek()? else {
            return None;
        };
        let space = Space::named(name)?;
        self.at += 1;
        let mut method = Hue::Shorter;
        if let Some(V::Ident(word)) = self.peek()
            && let Some(found) = Hue::named(word)
        {
            space.polar_hue()?;
            self.at += 1;
            match self.peek()? {
                V::Ident(word) if word.eq_ignore_ascii_case("hue") => self.at += 1,
                _ => return None,
            }
            method = found;
        }
        Some((space, method))
    }
    fn percent(&mut self) -> Option<f64> {
        let save = self.at;
        match self.component() {
            Some(Component::Percent(value)) => Some(value),
            _ => {
                self.at = save;
                None
            }
        }
    }
}

/// CSS Values 5 §6.1, "normalize mix percentages", with forced
/// normalization: omitted percentages share what the specified ones leave
/// (their sum clamped to 100%), a total above 100% (or any positive total)
/// is scaled to 100%, and a total below 100% leaves the fraction returned as
/// the leftover (Color 5 §3.3 multiplies the alpha by one minus it).
fn normalize_mix(percentages: &[Option<f64>]) -> (Vec<f64>, f64) {
    let specified: f64 = percentages.iter().flatten().sum::<f64>().min(100.0);
    let omitted = percentages.iter().filter(|p| p.is_none()).count();
    let share = if omitted == 0 {
        0.0
    } else {
        (100.0 - specified) / omitted as f64
    };
    let weights: Vec<f64> = percentages.iter().map(|p| p.unwrap_or(share)).collect();
    let total: f64 = weights.iter().sum();
    let leftover = if total < 100.0 {
        (100.0 - total) / 100.0
    } else {
        0.0
    };
    if total > 0.0 {
        (
            weights.iter().map(|w| w * 100.0 / total).collect(),
            leftover,
        )
    } else {
        (weights, leftover)
    }
}

fn alpha(component: Component) -> Option<Option<f64>> {
    match component {
        Component::None => Some(None),
        Component::Number(value) => Some(Some(value.clamp(0.0, 1.0))),
        Component::Percent(value) => Some(Some((value / 100.0).clamp(0.0, 1.0))),
        Component::Degrees(_) => None,
    }
}
fn legacy_alpha(component: Component) -> Option<f64> {
    match component {
        Component::None => None,
        other => alpha(other)?,
    }
}

/// Hue interpolation methods (Color 4 §13.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hue {
    Shorter,
    Longer,
    Increasing,
    Decreasing,
}
impl Hue {
    fn named(word: &str) -> Option<Self> {
        Some(match word.to_ascii_lowercase().as_str() {
            "shorter" => Self::Shorter,
            "longer" => Self::Longer,
            "increasing" => Self::Increasing,
            "decreasing" => Self::Decreasing,
            _ => return None,
        })
    }
    fn adjust(self, mut a: f64, mut b: f64) -> (f64, f64) {
        a = a.libm_rem_euclid(360.0);
        b = b.libm_rem_euclid(360.0);
        let delta = b - a;
        match self {
            Self::Shorter => {
                if delta > 180.0 {
                    a += 360.0;
                } else if delta < -180.0 {
                    b += 360.0;
                }
            }
            Self::Longer => {
                if 0.0 < delta && delta < 180.0 {
                    a += 360.0;
                } else if -180.0 < delta && delta <= 0.0 {
                    b += 360.0;
                }
            }
            Self::Increasing => {
                if b < a {
                    b += 360.0;
                }
            }
            Self::Decreasing => {
                if a < b {
                    a += 360.0;
                }
            }
        }
        (a, b)
    }
}

/// Convert `color` to `space`, carrying missing components forward when
/// the spaces are the same (Color 4 §13.2) and marking an achromatic hue
/// powerless (§4.4.1).
fn convert_carrying(color: Color, space: Space) -> Color {
    if color.space == space {
        return color;
    }
    let values = to_space(color, space);
    let mut components = values.map(Some);
    if let Some(hue) = space.polar_hue() {
        let achromatic = match space {
            Space::Hsl => values[1].abs() <= 0.001,
            Space::Hwb => values[1] + values[2] >= 99.999,
            _ => values[1] <= ACHROMATIC_CHROMA,
        };
        if achromatic {
            components[hue] = None;
        }
    }
    Color {
        space,
        components,
        alpha: color.alpha,
    }
}

/// Interpolate two colours at `progress` (Color 4 §13: premultiplied, the
/// hue method on the hue).
fn interpolate(a: Color, b: Color, progress: f64, space: Space, method: Hue) -> Color {
    let a = convert_carrying(a, space);
    let b = convert_carrying(b, space);
    let hue = space.polar_hue();
    let alpha = match (a.alpha, b.alpha) {
        (None, None) => None,
        (x, y) => {
            let x = x.or(y).unwrap_or(1.0);
            let y = y.or(Some(x)).unwrap_or(1.0);
            Some(x + (y - x) * progress)
        }
    };
    let (alpha_a, alpha_b) = (
        a.alpha.or(b.alpha).unwrap_or(1.0),
        b.alpha.or(a.alpha).unwrap_or(1.0),
    );
    let mut components = [None; 3];
    for (index, slot) in components.iter_mut().enumerate() {
        let (x, y) = match (a.components[index], b.components[index]) {
            (None, None) => continue,
            (Some(x), None) => (x, x),
            (None, Some(y)) => (y, y),
            (Some(x), Some(y)) => (x, y),
        };
        *slot = Some(if hue == Some(index) {
            let (x, y) = method.adjust(x, y);
            (x + (y - x) * progress).libm_rem_euclid(360.0)
        } else {
            let premultiplied = (x * alpha_a) + (y * alpha_b - x * alpha_a) * progress;
            match alpha {
                Some(alpha) if alpha != 0.0 => premultiplied / alpha,
                _ => premultiplied,
            }
        });
    }
    Color {
        space,
        components,
        alpha,
    }
}

type Vec3 = [f64; 3];
type Matrix = [[f64; 3]; 3];

fn multiply(m: &Matrix, v: Vec3) -> Vec3 {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}
fn invert(m: &Matrix) -> Matrix {
    let [[a, b, c], [d, e, f], [g, h, i]] = *m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    [
        [
            (e * i - f * h) / det,
            (c * h - b * i) / det,
            (b * f - c * e) / det,
        ],
        [
            (f * g - d * i) / det,
            (a * i - c * g) / det,
            (c * d - a * f) / det,
        ],
        [
            (d * h - e * g) / det,
            (b * g - a * h) / det,
            (a * e - b * d) / det,
        ],
    ]
}

/// Linear sRGB to XYZ D65 (Color 4 §18, `lin_sRGB_to_XYZ`).
const SRGB_TO_XYZ: Matrix = [
    [
        506_752.0 / 1_228_815.0,
        87_881.0 / 245_763.0,
        12_673.0 / 70_218.0,
    ],
    [
        87_098.0 / 409_605.0,
        175_762.0 / 245_763.0,
        12_673.0 / 175_545.0,
    ],
    [
        7_918.0 / 409_605.0,
        87_881.0 / 737_289.0,
        1_001_167.0 / 1_053_270.0,
    ],
];
/// XYZ D65 to LMS (Color 4 §18, `XYZ_to_OKLab`).
const XYZ_TO_LMS: Matrix = [
    [
        0.819_022_437_996_703,
        0.361_906_260_052_890_4,
        -0.128_873_781_520_987_9,
    ],
    [
        0.032_983_653_932_388_5,
        0.929_286_861_586_343_4,
        0.036_144_666_350_642_4,
    ],
    [
        0.048_177_189_359_624_2,
        0.264_239_531_752_730_8,
        0.633_547_828_469_430_9,
    ],
];
/// Non-linear LMS to Oklab (Color 4 §18).
const LMS_TO_OKLAB: Matrix = [
    [
        0.210_454_268_309_314,
        0.793_617_774_702_305_4,
        -0.004_072_043_011_619_3,
    ],
    [
        1.977_998_532_431_168_4,
        -2.428_592_242_048_58,
        0.450_593_709_617_411,
    ],
    [
        0.025_904_042_465_547_8,
        0.782_771_712_457_529_6,
        -0.808_675_754_923_077_4,
    ],
];
/// Bradford chromatic adaptation from D65 to D50 (Color 4 §18).
const D65_TO_D50: Matrix = [
    [
        1.047_929_792_544_996_9,
        0.022_946_870_601_609_652,
        -0.050_192_266_289_205_24,
    ],
    [
        0.029_627_808_770_055_99,
        0.990_434_426_753_879_9,
        -0.017_073_799_063_418_826,
    ],
    [
        -0.009_243_040_646_204_504,
        0.015_055_191_490_298_152,
        0.751_874_281_428_137_1,
    ],
];
/// The D50 white point in XYZ (Color 4 §10.4).
const D50_WHITE: Vec3 = [0.3457 / 0.3585, 1.0, (1.0 - 0.3457 - 0.3585) / 0.3585];
/// CIE Lab constants ε and κ (Color 4 §18).
const LAB_EPSILON: f64 = 216.0 / 24_389.0;
const LAB_KAPPA: f64 = 24_389.0 / 27.0;

fn srgb_to_linear(c: f64) -> f64 {
    let sign = c.signum();
    let abs = c.abs();
    if abs <= 0.04045 {
        c / 12.92
    } else {
        sign * ((abs + 0.055) / 1.055).libm_pow(2.4)
    }
}
fn linear_to_srgb(c: f64) -> f64 {
    let sign = c.signum();
    let abs = c.abs();
    if abs > 0.003_130_8 {
        sign * (1.055 * abs.libm_pow(1.0 / 2.4) - 0.055)
    } else {
        12.92 * c
    }
}
fn hsl_to_srgb([h, s, l]: Vec3) -> Vec3 {
    let (s, l) = (s / 100.0, l / 100.0);
    let h = h.libm_rem_euclid(360.0);
    let f = |n: f64| {
        let k = (n + h / 30.0) % 12.0;
        let a = s * l.min(1.0 - l);
        l - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)
    };
    [f(0.0), f(8.0), f(4.0)]
}
fn srgb_to_hsl([r, g, b]: Vec3) -> Vec3 {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let light = (min + max) / 2.0;
    let d = max - min;
    let (mut hue, mut sat) = (f64::NAN, 0.0);
    if d != 0.0 {
        sat = if light == 0.0 || light == 1.0 {
            0.0
        } else {
            (max - light) / light.min(1.0 - light)
        };
        hue = if max == r {
            (g - b) / d + if g < b { 6.0 } else { 0.0 }
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        } * 60.0;
    }
    if sat < 0.0 {
        hue += 180.0;
        sat = sat.abs();
    }
    [
        if hue.is_nan() {
            0.0
        } else {
            hue.libm_rem_euclid(360.0)
        },
        sat * 100.0,
        light * 100.0,
    ]
}
fn hwb_to_srgb([h, w, b]: Vec3) -> Vec3 {
    let (w, b) = (w / 100.0, b / 100.0);
    if w + b >= 1.0 {
        let gray = w / (w + b);
        return [gray; 3];
    }
    hsl_to_srgb([h, 100.0, 50.0]).map(|c| c * (1.0 - w - b) + w)
}
fn srgb_to_hwb(rgb: Vec3) -> Vec3 {
    let [hue, ..] = srgb_to_hsl(rgb);
    let white = rgb[0].min(rgb[1]).min(rgb[2]);
    let black = 1.0 - rgb[0].max(rgb[1]).max(rgb[2]);
    [hue, white * 100.0, black * 100.0]
}
fn lab_to_xyz_d50([l, a, b]: Vec3) -> Vec3 {
    let fy = (l + 16.0) / 116.0;
    let fx = a / 500.0 + fy;
    let fz = fy - b / 200.0;
    let x = if fx.libm_powi(3) > LAB_EPSILON {
        fx.libm_powi(3)
    } else {
        (116.0 * fx - 16.0) / LAB_KAPPA
    };
    let y = if l > LAB_KAPPA * LAB_EPSILON {
        fy.libm_powi(3)
    } else {
        l / LAB_KAPPA
    };
    let z = if fz.libm_powi(3) > LAB_EPSILON {
        fz.libm_powi(3)
    } else {
        (116.0 * fz - 16.0) / LAB_KAPPA
    };
    [x * D50_WHITE[0], y * D50_WHITE[1], z * D50_WHITE[2]]
}
fn xyz_d50_to_lab(xyz: Vec3) -> Vec3 {
    let f = |v: f64| {
        if v > LAB_EPSILON {
            v.libm_cbrt()
        } else {
            (LAB_KAPPA * v + 16.0) / 116.0
        }
    };
    let [x, y, z] = [
        f(xyz[0] / D50_WHITE[0]),
        f(xyz[1] / D50_WHITE[1]),
        f(xyz[2] / D50_WHITE[2]),
    ];
    [116.0 * y - 16.0, 500.0 * (x - y), 200.0 * (y - z)]
}
fn polar_to_rect([l, c, h]: Vec3) -> Vec3 {
    let radians = h.to_radians();
    [l, c * radians.libm_cos(), c * radians.libm_sin()]
}
fn rect_to_polar([l, a, b]: Vec3) -> Vec3 {
    let hue = b.libm_atan2(a).to_degrees().libm_rem_euclid(360.0);
    [l, (a * a + b * b).libm_sqrt(), hue]
}
fn oklab_to_xyz(lab: Vec3) -> Vec3 {
    let lms = multiply(&invert(&LMS_TO_OKLAB), lab).map(|c| c.libm_powi(3));
    multiply(&invert(&XYZ_TO_LMS), lms)
}
fn xyz_to_oklab(xyz: Vec3) -> Vec3 {
    let lms = multiply(&XYZ_TO_LMS, xyz).map(Portable::libm_cbrt);
    multiply(&LMS_TO_OKLAB, lms)
}

/// The components of `color` in XYZ D65, with missing components as zero.
fn to_xyz(color: Color) -> Vec3 {
    let values = color.components.map(|c| c.unwrap_or(0.0));
    match color.space {
        Space::Srgb => multiply(&SRGB_TO_XYZ, values.map(srgb_to_linear)),
        Space::SrgbLinear => multiply(&SRGB_TO_XYZ, values),
        Space::Hsl => multiply(&SRGB_TO_XYZ, hsl_to_srgb(values).map(srgb_to_linear)),
        Space::Hwb => multiply(&SRGB_TO_XYZ, hwb_to_srgb(values).map(srgb_to_linear)),
        Space::Lab => multiply(&invert(&D65_TO_D50), lab_to_xyz_d50(values)),
        Space::Lch => multiply(&invert(&D65_TO_D50), lab_to_xyz_d50(polar_to_rect(values))),
        Space::Oklab => oklab_to_xyz(values),
        Space::Oklch => oklab_to_xyz(polar_to_rect(values)),
        Space::XyzD65 => values,
        Space::XyzD50 => multiply(&invert(&D65_TO_D50), values),
    }
}
fn from_xyz(xyz: Vec3, space: Space) -> Vec3 {
    let linear = || multiply(&invert(&SRGB_TO_XYZ), xyz);
    match space {
        Space::Srgb => linear().map(linear_to_srgb),
        Space::SrgbLinear => linear(),
        Space::Hsl => srgb_to_hsl(linear().map(linear_to_srgb)),
        Space::Hwb => srgb_to_hwb(linear().map(linear_to_srgb)),
        Space::Lab => xyz_d50_to_lab(multiply(&D65_TO_D50, xyz)),
        Space::Lch => rect_to_polar(xyz_d50_to_lab(multiply(&D65_TO_D50, xyz))),
        Space::Oklab => xyz_to_oklab(xyz),
        Space::Oklch => rect_to_polar(xyz_to_oklab(xyz)),
        Space::XyzD65 => xyz,
        Space::XyzD50 => multiply(&D65_TO_D50, xyz),
    }
}
fn to_space(color: Color, space: Space) -> Vec3 {
    // The sRGB-based spaces convert directly (Color 4 §7.1, §8.1), without
    // the rounding of a round trip through XYZ.
    if space == Space::Srgb {
        let values = color.components.map(|c| c.unwrap_or(0.0));
        match color.space {
            Space::Srgb => return values,
            Space::Hsl => return hsl_to_srgb(values),
            Space::Hwb => return hwb_to_srgb(values),
            _ => {}
        }
    }
    from_xyz(to_xyz(color), space)
}

fn in_srgb_gamut(rgb: Vec3) -> bool {
    rgb.iter().all(|c| (0.0..=1.0).contains(c))
}
fn delta_eok(a: Vec3, b: Vec3) -> f64 {
    let [l, x, y] = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (l * l + x * x + y * y).libm_sqrt()
}

/// Map `color` into the sRGB gamut (Color 4 §14.2.2, binary search with
/// local MINDE in OkLCh) and return its sRGB components in `0..=1`.
#[must_use]
pub fn to_srgb(color: Color) -> Vec3 {
    let srgb = to_space(color, Space::Srgb);
    if color.space == Space::Srgb || in_srgb_gamut(srgb) {
        return srgb.map(|c| c.clamp(0.0, 1.0));
    }
    let origin = to_space(color, Space::Oklch);
    if origin[0] >= 1.0 {
        return [1.0; 3];
    }
    if origin[0] <= 0.0 {
        return [0.0; 3];
    }
    let oklch = |chroma: f64| Color {
        space: Space::Oklch,
        components: [Some(origin[0]), Some(chroma), Some(origin[2])],
        alpha: None,
    };
    let clip = |rgb: Vec3| rgb.map(|c| c.clamp(0.0, 1.0));
    let oklab_of_srgb = |rgb: Vec3| {
        to_space(
            Color {
                space: Space::Srgb,
                components: rgb.map(Some),
                alpha: None,
            },
            Space::Oklab,
        )
    };
    let current = oklch(origin[1]);
    let mut clipped = clip(srgb);
    let error = delta_eok(oklab_of_srgb(clipped), to_space(current, Space::Oklab));
    if error < JND {
        return clipped;
    }
    let (mut min, mut max) = (0.0, origin[1]);
    let mut min_in_gamut = true;
    while max - min > GAMUT_EPSILON {
        let chroma = (min + max) / 2.0;
        let current = oklch(chroma);
        let rgb = to_space(current, Space::Srgb);
        if min_in_gamut && in_srgb_gamut(rgb) {
            min = chroma;
            continue;
        }
        clipped = clip(rgb);
        let error = delta_eok(oklab_of_srgb(clipped), to_space(current, Space::Oklab));
        if error < JND {
            if JND - error < GAMUT_EPSILON {
                return clipped;
            }
            min_in_gamut = false;
            min = chroma;
        } else {
            max = chroma;
        }
    }
    clipped
}

/// The legacy `rgba()` text of a resolved colour: 8-bit channels and alpha,
/// which every renderer of the subset paints.
#[must_use]
pub fn to_legacy(color: Color) -> String {
    let [r, g, b] = to_srgb(color).map(|c| (c * 255.0).libm_round());
    let alpha = color.alpha.unwrap_or(0.0).clamp(0.0, 1.0);
    format!(
        "rgba({r}, {g}, {b}, {})",
        super::format_number((alpha * 1e6).libm_round() / 1e6)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy(text: &str) -> Option<String> {
        let tokens = super::super::lex_value(text)?;
        let (parsed, end) = parse(&tokens, 0)?;
        (end == tokens.len()).then_some(())?;
        match parsed {
            Parsed::Color(color) => Some(to_legacy(color)),
            _ => None,
        }
    }

    #[test]
    fn colour_syntaxes_convert_to_srgb() {
        assert_eq!(
            legacy("#0f08").as_deref(),
            Some("rgba(0, 255, 0, 0.533333)")
        );
        assert_eq!(
            legacy("rgb(255 0 0 / 50%)").as_deref(),
            Some("rgba(255, 0, 0, 0.5)")
        );
        assert_eq!(
            legacy("rgb(100%, 0%, 0%)").as_deref(),
            Some("rgba(255, 0, 0, 1)")
        );
        assert_eq!(
            legacy("rgba(300, -1, 0, 2)").as_deref(),
            Some("rgba(255, 0, 0, 1)")
        );
        assert_eq!(
            legacy("hsl(120 100% 50%)").as_deref(),
            Some("rgba(0, 255, 0, 1)")
        );
        assert_eq!(
            legacy("hsl(120deg, 100%, 25%, 0.5)").as_deref(),
            Some("rgba(0, 128, 0, 0.5)")
        );
        assert_eq!(
            legacy("hwb(0 0% 0%)").as_deref(),
            Some("rgba(255, 0, 0, 1)")
        );
        assert_eq!(
            legacy("oklch(100% 0 0)").as_deref(),
            Some("rgba(255, 255, 255, 1)")
        );
        assert_eq!(legacy("oklab(0 0 0)").as_deref(), Some("rgba(0, 0, 0, 1)"));
        assert_eq!(
            legacy("lab(100 0 0)").as_deref(),
            Some("rgba(255, 255, 255, 1)")
        );
        // In-gamut oklch: the sRGB red round-trips.
        assert_eq!(
            legacy("oklch(62.7955% 0.257627 29.2339)").as_deref(),
            Some("rgba(255, 0, 0, 1)")
        );
        // Mixing with transparent in premultiplied Oklab halves the alpha.
        assert_eq!(
            legacy("color-mix(in oklab, #ff0000 50%, transparent)").as_deref(),
            Some("rgba(255, 0, 0, 0.5)")
        );
        assert_eq!(
            legacy("color-mix(in srgb, #000 25%, #fff)").as_deref(),
            Some("rgba(191, 191, 191, 1)")
        );
        // Percentages summing below 100% scale the alpha.
        assert_eq!(
            legacy("color-mix(in srgb, #000 20%, #fff 20%)").as_deref(),
            Some("rgba(128, 128, 128, 0.4)")
        );
        // Color 5 §3: Oklab without a method; a zero sum is transparent.
        assert_eq!(
            legacy("color-mix(#ff0000 50%, transparent)").as_deref(),
            Some("rgba(255, 0, 0, 0.5)")
        );
        assert_eq!(
            legacy("color-mix(in srgb, #000 0%, #fff 0%)").as_deref(),
            Some("rgba(128, 128, 128, 0)")
        );
        // Color 5 §3.3: any number of colours, mixed in order.
        assert_eq!(
            legacy("color-mix(in srgb, #000, #fff, #f00)").as_deref(),
            Some("rgba(170, 85, 85, 1)")
        );
        assert_eq!(
            legacy("color-mix(in srgb, #f00)").as_deref(),
            Some("rgba(255, 0, 0, 1)")
        );
        assert_eq!(
            legacy("RebeccaPurple").as_deref(),
            Some("rgba(102, 51, 153, 1)")
        );
        assert_eq!(NAMED_COLORS.len(), 148);
        for invalid in [
            "notacolor",
            "color-mix(, #000, #fff)",
            "rgb(1, 2 3)",
            "rgb(1, 2%, 3)",
            "rgb(none, 0, 0)",
            "hsl(1, 2, 3)",
            "#12",
            "color-mix(in display-p3, #000, #fff)",
            "color-mix(in srgb longer hue, #000, #fff)",
            "color-mix(in srgb, #000 120%, #fff)",
            "oklch(0.5 0.1 10px)",
        ] {
            assert_eq!(legacy(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn out_of_gamut_colours_are_css_gamut_mapped() {
        // A vivid green far outside sRGB. Gamut mapping keeps the lightness
        // and hue and reduces chroma, unlike per-channel clipping.
        let tokens = super::super::lex_value("oklch(70% 0.4 150)").unwrap();
        let Some((Parsed::Color(color), _)) = parse(&tokens, 0) else {
            panic!()
        };
        let mapped = to_srgb(color);
        assert!(in_srgb_gamut(mapped));
        let lch = to_space(
            Color {
                space: Space::Srgb,
                components: mapped.map(Some),
                alpha: Some(1.0),
            },
            Space::Oklch,
        );
        assert!((lch[0] - 0.7).abs() < 0.02, "{lch:?}");
        assert!((lch[2] - 150.0).abs() < 4.0, "{lch:?}");
        assert!(lch[1] < 0.4);
        let clipped = to_space(color, Space::Srgb).map(|c| c.clamp(0.0, 1.0));
        assert_ne!(mapped, clipped);
    }
}
