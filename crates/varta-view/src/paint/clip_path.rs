//! CSS Masking 1 inset clips preserve layout, source text and semantic names.
use super::{BoxRect, Clip, CssProperty, Units};
use std::{collections::BTreeMap, sync::Arc};

pub(super) fn establishes(properties: &BTreeMap<CssProperty, String>) -> bool {
    properties
        .get(&CssProperty::ClipPath)
        .is_some_and(|v| !v.eq_ignore_ascii_case("none"))
}
pub(super) fn combine(mut base: Option<Arc<Clip>>, source: &Arc<Clip>) -> Arc<Clip> {
    if base.is_none() {
        return source.clone();
    }
    let mut next = base.as_deref();
    while let Some(clip) = next {
        if clip.same_shape(source) {
            return base.take().expect("a matching clip belongs to this chain");
        }
        next = clip.parent.as_deref();
    }
    Clip::reparent(source, base)
}
pub(super) fn inset(raw: &str, rect: BoxRect, units: Units) -> Option<BoxRect> {
    let open = raw.find('(')?;
    if !raw[..open].eq_ignore_ascii_case("inset") {
        return None;
    }
    let values: Vec<_> = raw[open + 1..]
        .strip_suffix(')')?
        .split_whitespace()
        .collect();
    let [top, right, bottom, left] = match values.as_slice() {
        [a] => [*a; 4],
        [a, b] => [*a, *b, *a, *b],
        [a, b, c] => [*a, *b, *c, *b],
        [a, b, c, d] => [*a, *b, *c, *d],
        _ => return None,
    };
    let length = |value: &str, basis: f32| {
        if let Some(percent) = value.strip_suffix('%') {
            Some(percent.parse::<f64>().ok()? * f64::from(basis) / 100.0)
        } else {
            units.pixels(value, true).map(f64::from)
        }
    };
    let horizontal = [length(left, rect.width)?, length(right, rect.width)?];
    let vertical = [length(top, rect.height)?, length(bottom, rect.height)?];
    let constrain = |values: [f64; 2], extent: f32| {
        let sum = values[0] + values[1];
        if sum > f64::from(extent) {
            values.map(|v| v * f64::from(extent) / sum)
        } else {
            values
        }
    };
    let [left, right] = constrain(horizontal, rect.width);
    let [top, bottom] = constrain(vertical, rect.height);
    Some(BoxRect {
        x: (f64::from(rect.x) + left) as f32,
        y: (f64::from(rect.y) + top) as f32,
        width: (f64::from(rect.width) - left - right).max(0.0) as f32,
        height: (f64::from(rect.height) - top - bottom).max(0.0) as f32,
    })
}
