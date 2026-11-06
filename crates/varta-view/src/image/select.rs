//! An `img` element's current image (HTML §4.8.4.3): its source set from
//! `srcset`, `sizes` and `src`, and the candidate selected for the device
//! pixel ratio, whose density divides the natural size (the
//! density-corrected natural size).
//!
//! The source set (§4.8.4.3.6): the `srcset` candidates, each width
//! descriptor turned into a density by the source size (§4.8.4.3.8: the
//! first `sizes` entry whose media condition matches, `100vw` without one),
//! and `src` as a `1x` candidate when no candidate has density 1 and none has
//! a width. Selection is the user agent's (§4.8.4.3.3); this is Chromium's
//! (`HTMLSrcsetParser`'s `SelectionLogic`): candidates sorted by density, the
//! first whose density reaches the device pixel ratio unless the ratio is
//! below the geometric mean of it and the one before.
use super::Image;
use crate::style::Environment;
use crate::tree::Tree;
use std::sync::Arc;
use varta_common::view::Attribute;
use varta_common::view::attributes::{
    CandidateDescriptor, SizeCondition, SizeLength, image_candidates_of, source_sizes_of,
};

/// Media query lengths in `rem` use the initial font size (Media Queries 4
/// §1.3), as Chrome's 16px `medium`.
const INITIAL_FONT_PX: f64 = 16.0;
const PERCENT: f64 = 100.0;
/// The default source size: `100vw` (§4.8.4.3.8).
const DEFAULT_SIZE_VW: f64 = 100.0;

/// The image an element shows and the density of its candidate.
pub(crate) struct Current {
    pub image: Arc<Image>,
    pub density: f64,
}

fn pixels(length: SizeLength, env: &Environment) -> f64 {
    match length {
        SizeLength::Px(value) => value,
        SizeLength::Rem(value) => value * INITIAL_FONT_PX,
        SizeLength::Vw(value) => value * env.width / PERCENT,
    }
}

/// The source size of `sizes` in CSS pixels at the environment's viewport.
fn source_size(sizes: Option<&str>, env: &Environment) -> f64 {
    let default = DEFAULT_SIZE_VW * env.width / PERCENT;
    let Some(sizes) = sizes.and_then(source_sizes_of) else {
        return default;
    };
    sizes
        .iter()
        .find(|size| match size.condition {
            None => true,
            Some(SizeCondition::MinWidth(length)) => env.width >= pixels(length, env),
            Some(SizeCondition::MaxWidth(length)) => env.width <= pixels(length, env),
        })
        .map_or(default, |size| pixels(size.length, env))
}

/// Chromium's selection among candidates sorted by ascending density.
fn select(candidates: &[(f64, &str)], ratio: f64) -> usize {
    let mut index = 0;
    while index + 1 < candidates.len() {
        let next = candidates[index + 1].0;
        if next < ratio {
            index += 1;
            continue;
        }
        let current = candidates[index].0;
        let geometric_mean = (current * next).sqrt();
        if (ratio <= 1.0 && ratio > current) || ratio >= geometric_mean {
            return index + 1;
        }
        break;
    }
    index
}

/// The source set of an element's attributes as (density, resource) pairs.
fn source_set<'a>(
    src: Option<&'a str>,
    srcset: Option<&'a str>,
    sizes: Option<&str>,
    env: &Environment,
) -> Vec<(f64, &'a str)> {
    let mut set: Vec<(f64, &str)> = Vec::new();
    let candidates = srcset.and_then(image_candidates_of).unwrap_or_default();
    let widths = candidates
        .iter()
        .any(|candidate| matches!(candidate.descriptor, CandidateDescriptor::Width(_)));
    let size = widths.then(|| source_size(sizes, env));
    for candidate in &candidates {
        let density = match candidate.descriptor {
            CandidateDescriptor::Density(density) => density,
            CandidateDescriptor::Width(width) => match size {
                Some(size) if size > 0.0 => f64::from(width) / size,
                _ => f64::INFINITY,
            },
        };
        set.push((density, candidate.resource));
    }
    if let Some(src) = src
        && !widths
        && !set.iter().any(|(density, _)| *density == 1.0)
    {
        set.push((1.0, src));
    }
    set.sort_by(|a, b| a.0.total_cmp(&b.0));
    set.dedup_by(|a, b| a.0 == b.0);
    set
}

/// The element's current image: the candidate selected for the
/// environment's device pixel ratio, or `None` without a source or images.
pub(crate) fn current(tree: &Tree, env: &Environment, id: u32) -> Option<Current> {
    let element = tree.element(id)?;
    let images = tree.images.set.as_ref()?;
    let attribute = |name| element.attributes.get(&name).map(String::as_str);
    let set = source_set(
        attribute(Attribute::Src),
        attribute(Attribute::Srcset),
        attribute(Attribute::Sizes),
        env,
    );
    if set.is_empty() {
        return None;
    }
    let (density, name) = set[select(&set, env.device_pixel_ratio)];
    Some(Current {
        image: images.get(name)?.clone(),
        density,
    })
}

/// Every resource name the element's `src` and `srcset` name.
pub(crate) fn named<'a>(src: Option<&'a str>, srcset: Option<&'a str>) -> Vec<&'a str> {
    let mut names: Vec<&str> = src.into_iter().collect();
    if let Some(candidates) = srcset.and_then(image_candidates_of) {
        names.extend(candidates.iter().map(|candidate| candidate.resource));
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chromium_selects_by_density_and_geometric_mean() {
        let set = [(1.0, "a"), (2.0, "b"), (3.0, "c")];
        assert_eq!(select(&set, 1.0), 0);
        // 1.5 is above 1 but below sqrt(2): the 1x candidate stays.
        assert_eq!(select(&set, 1.4), 0);
        assert_eq!(select(&set, 1.5), 1);
        assert_eq!(select(&set, 2.0), 1);
        assert_eq!(select(&set, 3.0), 2);
        assert_eq!(select(&set, 4.0), 2);
        // Below 1, a ratio above the lowest density takes the next one.
        assert_eq!(select(&[(0.5, "a"), (1.0, "b")], 0.75), 1);
    }

    #[test]
    fn widths_become_densities_through_the_matching_source_size() {
        let env = Environment {
            width: 400.0,
            ..Environment::default()
        };
        let set = source_set(
            Some("fallback.png"),
            Some("small.png 200w, large.png 800w"),
            Some("(max-width: 300px) 100px, 50vw"),
            &env,
        );
        // 50vw of 400px is 200px: densities 1 and 4; `src` is not added.
        assert_eq!(set, [(1.0, "small.png"), (4.0, "large.png")]);
        let set = source_set(Some("a.png"), Some("b.png 2x"), None, &env);
        assert_eq!(set, [(1.0, "a.png"), (2.0, "b.png")]);
        assert_eq!(
            named(Some("a.png"), Some("b.png 2x, c.png 3x")),
            ["a.png", "b.png", "c.png"]
        );
    }
}
