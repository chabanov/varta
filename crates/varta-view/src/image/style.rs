//! The CSS that places and samples an `img`'s image: its natural size
//! (HTML density-corrected, CSS Images 3 `image-orientation`), the concrete
//! object size and position in its content box (CSS Images 3 §4.5
//! `object-fit`, §4.6 `object-position`) and its filter (§5.2
//! `image-rendering`).
use super::select::{self, Current};
use super::{Filter, Image};
use crate::BoxRect;
use crate::style::Environment;
use crate::tree::Tree;
use crate::units::Units;
use std::sync::Arc;
use varta_common::view::CssProperty;

const HALF: f32 = 0.5;
const PERCENT: f32 = 100.0;

/// An element's image as drawn: the frame source, its orientation and its
/// natural size in CSS pixels (density-corrected and oriented).
pub(crate) struct Shown {
    pub image: Arc<Image>,
    pub orientation: u8,
    pub natural: (f32, f32),
}

/// The value of an inherited property on `id` or its nearest ancestor.
fn inherited(tree: &Tree, env: &Environment, id: u32, property: CssProperty) -> Option<String> {
    let mut node = Some(id);
    while let Some(current) = node {
        if let Some(value) = tree.computed_property(current, env, property) {
            return Some(value);
        }
        node = tree.nodes.get(&current).and_then(|n| n.parent);
    }
    None
}

/// The image an element shows, or `None` without one.
pub(crate) fn shown(tree: &Tree, env: &Environment, id: u32) -> Option<Shown> {
    let Current { image, density } = select::current(tree, env, id)?;
    // `image-orientation` is inherited; `from-image` is its initial value.
    let from_image =
        inherited(tree, env, id, CssProperty::ImageOrientation).is_none_or(|value| value != "none");
    let orientation = image.orientation(from_image);
    let (width, height) = image.oriented_size(orientation);
    let density = density as f32;
    Some(Shown {
        natural: (width as f32 / density, height as f32 / density),
        image,
        orientation,
    })
}

/// The filter of `image-rendering` (inherited).
pub(crate) fn filter(tree: &Tree, env: &Environment, id: u32) -> Filter {
    match inherited(tree, env, id, CssProperty::ImageRendering).as_deref() {
        Some("pixelated") => Filter::Pixelated,
        Some("crisp-edges") => Filter::Nearest,
        _ => Filter::Smooth,
    }
}

/// The concrete object size of `natural` in a `content` box for
/// `object-fit` (§4.5): `fill` stretches, `contain` and `cover` keep the
/// ratio inside or over the box, `none` keeps the natural size and
/// `scale-down` the smaller of `none` and `contain`.
fn object_size(fit: &str, natural: (f32, f32), content: (f32, f32)) -> (f32, f32) {
    let (width, height) = natural;
    if width <= 0.0 || height <= 0.0 {
        return content;
    }
    let contain = (content.0 / width).min(content.1 / height);
    let cover = (content.0 / width).max(content.1 / height);
    match fit {
        "contain" => (width * contain, height * contain),
        "cover" => (width * cover, height * cover),
        "none" => natural,
        "scale-down" if contain < 1.0 => (width * contain, height * contain),
        "scale-down" => natural,
        _ => content,
    }
}

/// One `object-position` component as an offset in the free space `free`:
/// a keyword or percentage takes its fraction, a length its pixels.
fn offset(value: &str, free: f32, units: Units) -> Option<f32> {
    let fraction = match value {
        "left" | "top" => 0.0,
        "center" => HALF,
        "right" | "bottom" => 1.0,
        _ => match value.strip_suffix('%') {
            Some(number) => number.parse::<f32>().ok()? / PERCENT,
            None => return units.pixels(value, true),
        },
    };
    Some(free * fraction)
}

/// The object rectangle of an element's image in its `content` box, or
/// `None` for a value outside the grammar.
pub(crate) fn object_rect(
    tree: &Tree,
    env: &Environment,
    id: u32,
    natural: (f32, f32),
    content: BoxRect,
) -> Option<BoxRect> {
    let units = Units::new(env)?;
    let fit = tree.computed_property(id, env, CssProperty::ObjectFit);
    let (width, height) = object_size(
        fit.as_deref().unwrap_or("fill"),
        natural,
        (content.width, content.height),
    );
    let position = tree.computed_property(id, env, CssProperty::ObjectPosition);
    let mut words = position
        .as_deref()
        .unwrap_or("50% 50%")
        .split_ascii_whitespace();
    let horizontal = words.next().unwrap_or("50%");
    let vertical = words.next().unwrap_or("center");
    Some(BoxRect {
        x: content.x + offset(horizontal, content.width - width, units)?,
        y: content.y + offset(vertical, content.height - height, units)?,
        width,
        height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_fit_sizes_follow_css_images_3() {
        let natural = (20.0, 10.0);
        let content = (40.0, 40.0);
        assert_eq!(object_size("fill", natural, content), (40.0, 40.0));
        assert_eq!(object_size("contain", natural, content), (40.0, 20.0));
        assert_eq!(object_size("cover", natural, content), (80.0, 40.0));
        assert_eq!(object_size("none", natural, content), (20.0, 10.0));
        assert_eq!(object_size("scale-down", natural, content), (20.0, 10.0));
        assert_eq!(
            object_size("scale-down", natural, (10.0, 40.0)),
            (10.0, 5.0)
        );
    }
}
