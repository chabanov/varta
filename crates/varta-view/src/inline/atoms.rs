//! Inline images in a line of text: atomic inlines (CSS 2.1 §9.2.2, §10.8;
//! CSS Inline 3 §4.2 `vertical-align`).
//!
//! An inline image that shares its line box with text or other inline
//! content is an atomic inline of its parent's inline formatting context. Its
//! margin box is one unbreakable advance of the line, with a soft wrap
//! opportunity before and after it (CSS Text 3 §5.1; the opening and closing
//! decoration edges right next to it stay with it). Its baseline is its
//! bottom margin edge (CSS 2.1 §10.8.1) and `vertical-align` places that edge
//! against the parent's baseline and first available font, with Chrome's
//! shifts in Blink layout units: `sub` lowers it by a fifth of the parent's
//! font size plus one pixel, `super` raises it by a third plus one, `middle`
//! centres the margin box half the parent's x-height above the baseline,
//! `text-top`/`text-bottom` meet the parent's rounded ascent/descent, a
//! length raises it and a percentage raises it by that part of the image's
//! own line height. `top` and `bottom` align the margin box with the line
//! box once the rest of the line is placed, and grow the line downwards or
//! upwards when the image is taller (§10.8 aligned subtrees).
//!
//! An image of a line is laid out with its own lengths: percentages of the
//! containing block, a positioned or transformed image, an image under an
//! inline box with opacity, and images in `text-overflow: ellipsis` or
//! preserved white space flows are refused.
use super::strut;
use crate::TextMeasurer;
use crate::layout::{self, BoxRect, LayoutError, replaced::AtomBox};
use crate::style::Environment;
use crate::tree::Tree;
use crate::units::Units;
use taffy::style::{ExpandedLengthPercentage, LengthPercentage};
use varta_common::view::CssProperty as P;

/// Blink layout units per CSS pixel: offsets are fixed point at 1/64 px.
const LAYOUT_UNITS: f32 = 64.0;
/// Chrome's `sub` shift divides the parent's font size by five, its
/// `super` shift by three; both add one pixel.
const SUB_DIVISOR: f32 = 5.0;
const SUPER_DIVISOR: f32 = 3.0;
const SCRIPT_SHIFT_PIXELS: f32 = 1.0;

/// A layout-unit value truncated toward zero, as Blink's integer division
/// and float conversion do.
fn truncated(value: f32) -> f32 {
    (value * LAYOUT_UNITS).trunc() / LAYOUT_UNITS
}

/// A layout-unit value rounded to the nearest unit (`FromFloatRound`).
fn rounded(value: f32) -> f32 {
    (value * LAYOUT_UNITS).round() / LAYOUT_UNITS
}

/// Where an atomic inline's margin box stands in its line box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Align {
    /// Its bottom edge lies this far below the baseline (above when
    /// negative).
    Bottom(f32),
    /// `top`: its top edge on the line box's top.
    LineTop,
    /// `bottom`: its bottom edge on the line box's bottom.
    LineBottom,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Atom {
    pub image: AtomBox,
    pub align: Align,
}
impl Atom {
    /// The margin box width: the image's advance in the line.
    pub fn advance(self) -> f32 {
        self.image.size.0 + self.image.margin[1] + self.image.margin[3]
    }
    /// The margin box height.
    pub fn height(self) -> f32 {
        self.image.size.1 + self.image.margin[0] + self.image.margin[2]
    }
    /// How far its margin box's bottom lies below the baseline of a line
    /// box reaching `line` above and below that baseline.
    pub fn bottom(self, line: (f32, f32)) -> f32 {
        match self.align {
            Align::Bottom(bottom) => bottom,
            Align::LineTop => self.height() - line.0,
            Align::LineBottom => line.1,
        }
    }
    /// Its border box for a margin box starting at `x` whose bottom lies
    /// `bottom` below the baseline at `baseline`.
    pub fn border_box(self, x: f32, baseline: f32, bottom: f32) -> BoxRect {
        let AtomBox { size, margin, .. } = self.image;
        BoxRect {
            x: x + margin[3],
            y: baseline + bottom - margin[2] - size.1,
            width: size.0,
            height: size.1,
        }
    }
    /// The content box inside border box `rect`.
    pub fn content(self, rect: BoxRect) -> BoxRect {
        let [top, right, bottom, left] = self.image.inset;
        BoxRect {
            x: rect.x + left,
            y: rect.y + top,
            width: (rect.width - left - right).max(0.0),
            height: (rect.height - top - bottom).max(0.0),
        }
    }
}

/// The metrics of the parent's first available font that `vertical-align`
/// keywords use.
struct ParentFont {
    size: f32,
    ascent: f32,
    descent: f32,
    x_height: Option<f32>,
}

fn parent_font(
    tree: &Tree,
    env: &Environment,
    parent: u32,
    font: (&mut TextMeasurer, f32),
) -> Result<ParentFont, LayoutError> {
    let (measurer, size) = font;
    let (options, families) = layout::text_style(tree, env, parent, size, measurer)?;
    let options =
        layout::controls::control_line_height(tree, parent, measurer, &families, options)?;
    let (_, shaped) = measurer
        .shape_styled(" ", None, &families, options)
        .map_err(LayoutError::text(parent))?;
    let run = shaped
        .lines()
        .next()
        .and_then(|line| line.runs().next())
        .ok_or(LayoutError::Unsupported(parent))?;
    let metrics = run.metrics();
    Ok(ParentFont {
        size: run.font_size(),
        ascent: metrics.ascent.round(),
        descent: metrics.descent.round(),
        x_height: metrics.x_height,
    })
}

/// Size image `id` of a line of text and resolve its `vertical-align`.
pub(super) fn measure(
    tree: &Tree,
    env: &Environment,
    id: u32,
    font: (&mut TextMeasurer, f32),
) -> Result<Atom, LayoutError> {
    let refused = || LayoutError::Unsupported(id);
    if tree
        .computed_property(id, env, P::Position)
        .is_some_and(|value| value != "static")
        || tree
            .computed_property(id, env, P::Transform)
            .is_some_and(|value| value.trim() != "none")
    {
        return Err(refused());
    }
    let image = layout::image_box(tree, env, id)?;
    let (measurer, size) = font;
    let parent = tree.nodes[&id].parent.ok_or_else(refused)?;
    let mut atom = Atom {
        image,
        align: Align::Bottom(0.0),
    };
    atom.align = match tree.computed_property(id, env, P::VerticalAlign).as_deref() {
        None | Some("baseline") => Align::Bottom(0.0),
        Some("top") => Align::LineTop,
        Some("bottom") => Align::LineBottom,
        Some(keyword @ ("sub" | "super" | "middle" | "text-top" | "text-bottom")) => {
            let font = parent_font(tree, env, parent, (measurer, size))?;
            Align::Bottom(match keyword {
                "sub" => truncated(truncated(font.size) / SUB_DIVISOR) + SCRIPT_SHIFT_PIXELS,
                "super" => -(truncated(truncated(font.size) / SUPER_DIVISOR) + SCRIPT_SHIFT_PIXELS),
                // A font without an x-height (OS/2 `sxHeight`) leaves
                // Chrome to measure glyphs, which is not modelled.
                "middle" => {
                    let x_height = font
                        .x_height
                        .filter(|height| *height > 0.0)
                        .ok_or_else(refused)?;
                    truncated(atom.height() / 2.0) - rounded(x_height / 2.0)
                }
                "text-top" => atom.height() - font.ascent,
                _ => font.descent,
            })
        }
        Some(value) => {
            let units = Units::new(env).ok_or(LayoutError::RootFont)?;
            let length: LengthPercentage = units.length(value, true).ok_or_else(refused)?;
            let raise = match length.expand() {
                ExpandedLengthPercentage::Length(value) => value,
                ExpandedLengthPercentage::Percent(fraction) => {
                    let (above, below) = strut(tree, env, id, (measurer, size))?;
                    truncated(fraction * (above + below))
                }
                // CSS Inline 3 §4.4: percentages of the line height.
                ExpandedLengthPercentage::Calc(handle) => {
                    let (above, below) = strut(tree, env, id, (measurer, size))?;
                    truncated(crate::calc::resolve(handle, above + below))
                }
            };
            Align::Bottom(-raise)
        }
    };
    Ok(atom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_units_truncate_and_round_as_blink_does() {
        assert_eq!(truncated(16.0 / 5.0), 3.1875);
        assert_eq!(truncated(-16.0 / 5.0), -3.1875);
        assert_eq!(rounded(8.6 / 2.0), 4.296875);
    }

    #[test]
    fn margin_boxes_stand_on_their_aligned_bottom_edge() {
        let atom = Atom {
            image: AtomBox {
                size: (10.0, 20.0),
                margin: [1.0, 2.0, 3.0, 4.0],
                inset: [1.0, 1.0, 2.0, 2.0],
            },
            align: Align::Bottom(0.0),
        };
        assert_eq!((atom.advance(), atom.height()), (16.0, 24.0));
        // On the baseline: the margin bottom at the baseline.
        let rect = atom.border_box(100.0, 50.0, 0.0);
        assert_eq!(
            (rect.x, rect.y, rect.width, rect.height),
            (104.0, 27.0, 10.0, 20.0)
        );
        let content = atom.content(rect);
        assert_eq!(
            (content.x, content.y, content.width, content.height),
            (106.0, 28.0, 7.0, 17.0)
        );
        // `top` and `bottom` follow the line box.
        let top = Atom {
            align: Align::LineTop,
            ..atom
        };
        let bottom = Atom {
            align: Align::LineBottom,
            ..atom
        };
        assert_eq!(top.bottom((30.0, 6.0)), -6.0);
        assert_eq!(bottom.bottom((30.0, 6.0)), 6.0);
        assert_eq!(atom.bottom((30.0, 6.0)), 0.0);
    }
}
