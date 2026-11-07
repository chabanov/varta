//! CSS Backgrounds 3 shadow-list interpolation. Endpoint lists share their
//! storage; each sampled list is bounded by the checked declaration's tokens.
use super::{CHANNEL_MAX, FRACTION_DIGITS};
use std::sync::Arc;
use varta_common::view::css;

/// Premultiplied fixed color plus a coefficient of the live currentcolor.
/// This affine form stays flat when a running transition is interrupted.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Color {
    fixed: [f64; 4],
    current: f64,
}
impl Color {
    const TRANSPARENT: Self = Self {
        fixed: [0.0; 4],
        current: 0.0,
    };
    fn parse(value: &str) -> Option<Self> {
        if value == "currentcolor" {
            return Some(Self {
                fixed: [0.0; 4],
                current: 1.0,
            });
        }
        let css::color::Parsed::Color(c) = css::color::parse_value(value)? else {
            return None;
        };
        let alpha = c.alpha.unwrap_or(1.0).clamp(0.0, 1.0);
        let channels = css::color::to_srgb(c).map(|v| v.clamp(0.0, 1.0) * alpha);
        Some(Self {
            fixed: [channels[0], channels[1], channels[2], alpha],
            current: 0.0,
        })
    }

    fn interpolate(self, end: Self, progress: f64) -> Self {
        Self {
            fixed: std::array::from_fn(|i| {
                self.fixed[i] + (end.fixed[i] - self.fixed[i]) * progress
            }),
            current: self.current + (end.current - self.current) * progress,
        }
    }
    fn css(self, current: [f64; 4]) -> String {
        let alpha = (self.fixed[3] + self.current * current[3]).clamp(0.0, 1.0);
        let channel = |i: usize| {
            if alpha == 0.0 {
                0
            } else {
                (((self.fixed[i] + self.current * current[i] * current[3]) / alpha).clamp(0.0, 1.0)
                    * CHANNEL_MAX)
                    .round() as u8
            }
        };
        format!(
            "rgba({}, {}, {}, {alpha:.FRACTION_DIGITS$})",
            channel(0),
            channel(1),
            channel(2)
        )
    }
}
#[derive(Debug, Clone, PartialEq)]
struct Shadow {
    lengths: [f64; 4],
    color: Color,
    inset: bool,
}
impl Shadow {
    fn blank(inset: bool) -> Self {
        Self {
            lengths: [0.0; 4],
            color: Color::TRANSPARENT,
            inset,
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct List(Arc<[Shadow]>);
impl List {
    pub(super) fn parse(value: &str) -> Option<Self> {
        css::shadow::parse(value)?
            .into_iter()
            .map(|s| {
                Some(Shadow {
                    lengths: [s.x, s.y, s.blur, s.spread],
                    color: Color::parse(&s.color)?,
                    inset: s.inset,
                })
            })
            .collect::<Option<Vec<_>>>()
            .map(|v| Self(v.into()))
    }
    pub(super) fn interpolable(&self, end: &Self) -> bool {
        self.0
            .iter()
            .zip(end.0.iter())
            .all(|(a, b)| a.inset == b.inset)
    }
    pub(super) fn interpolate(&self, end: &Self, progress: f64) -> Self {
        // Preserve exact endpoint identities for reversal and completed values.
        if progress == 0.0 {
            return self.clone();
        }
        if progress == 1.0 {
            return end.clone();
        }
        let values = (0..self.0.len().max(end.0.len()))
            .map(|i| {
                let blank = Shadow::blank(self.0.get(i).or_else(|| end.0.get(i)).unwrap().inset);
                let a = self.0.get(i).unwrap_or(&blank);
                let b = end.0.get(i).unwrap_or(&blank);
                let mut lengths = std::array::from_fn(|n| {
                    a.lengths[n] + (b.lengths[n] - a.lengths[n]) * progress
                });
                lengths[css::shadow::BLUR_POSITION] = lengths[css::shadow::BLUR_POSITION].max(0.0);
                Shadow {
                    lengths,
                    color: a.color.interpolate(b.color, progress),
                    inset: a.inset,
                }
            })
            .collect::<Vec<_>>();
        Self(values.into())
    }
    pub(super) fn css(&self, current: [f64; 4]) -> String {
        if self.0.is_empty() {
            return "none".into();
        }
        self.0.iter().map(|s| {
            let [x,y,blur,spread] = s.lengths;
            let inset = if s.inset { "inset " } else { "" };
            format!("{inset}{x:.FRACTION_DIGITS$}px {y:.FRACTION_DIGITS$}px {blur:.FRACTION_DIGITS$}px {spread:.FRACTION_DIGITS$}px {}", s.color.css(current))
        }).collect::<Vec<_>>().join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn list(value: &str) -> List {
        let computed = css::lower(
            varta_common::view::CssProperty::BoxShadow,
            value,
            &css::Context::NOMINAL,
        )
        .unwrap();
        List::parse(&computed).unwrap()
    }
    #[test]
    fn list_pairing_pads_each_inset_kind_and_premultiplies_colors() {
        let a = list("none");
        let b = list("4px -2px 8px -4px red, inset 2px 0 2px blue");
        let middle = a.interpolate(&b, 0.5);
        let computed = css::shadow::parse(&middle.css([0.0; 4])).unwrap();
        assert_eq!(computed.len(), 2);
        assert_eq!(
            (
                computed[0].x,
                computed[0].y,
                computed[0].blur,
                computed[0].spread
            ),
            (2.0, -1.0, 4.0, -2.0)
        );
        assert!(!computed[0].inset && computed[1].inset);
        assert_eq!(
            css::color::parse_value(&computed[0].color),
            css::color::parse_value("rgba(255,0,0,.5)")
        );
        assert!(!b.interpolable(&list("inset 0 0 red")));
        let c = list("0 0 rgba(0,0,255,0)");
        assert!(
            b.interpolate(&c, 0.5)
                .css([0.0; 4])
                .contains("rgba(255, 0, 0, 0.500000)")
        );
    }
    #[test]
    fn interrupted_live_currentcolor_is_flat_and_blur_overshoot_is_clamped() {
        let a = list("0 0 10px currentcolor");
        let b = list("0 0 0 blue");
        let halfway = a.interpolate(&b, 0.5);
        assert!(
            halfway
                .css([1.0, 0.0, 0.0, 1.0])
                .contains("rgba(128, 0, 128, 1.000000)")
        );
        assert!(
            halfway
                .css([0.0, 1.0, 0.0, 1.0])
                .contains("rgba(0, 128, 128, 1.000000)")
        );
        let reverse = halfway.interpolate(&a, 0.5);
        assert!(
            reverse
                .css([1.0, 0.0, 0.0, 1.0])
                .contains("rgba(191, 0, 64, 1.000000)")
        );
        let overshot = a.interpolate(&b, 1.5);
        assert_eq!(
            css::shadow::parse(&overshot.css([1.0; 4])).unwrap()[0].blur,
            0.0
        );
    }
}
