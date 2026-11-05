//! Chrome's native control theme (the "refreshed" form controls) for
//! controls without an author background, border or radius, plus the
//! user-agent colors of author-styled controls and the default focus ring.
//! Colors are Chrome 156's light scheme (the official host never sets
//! `color-scheme`); disabled fills are the single layers equivalent to
//! Chrome's composited theme, measured over black and white backgrounds.
use super::corners::Corners;
use super::{PaintError, Surface, clipped};
use crate::layout::BoxRect;
use crate::style::ControlKind;
use tiny_skia::{Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Stroke, Transform};

/// An sRGB color with straight alpha, as 8-bit channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Rgba([u8; 4]);
impl Rgba {
    pub(super) fn color(self) -> Color {
        let [red, green, blue, alpha] = self.0;
        Color::from_rgba8(red, green, blue, alpha)
    }
}
const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Rgba {
    Rgba([red, green, blue, alpha])
}
const OPAQUE: u8 = u8::MAX;
/// `kBorderWidth` of the theme frame, in CSS pixels.
const FRAME_WIDTH: f32 = 1.0;
/// `GetBorderRadiusForPart` of buttons, fields and checkboxes, in CSS pixels.
const PART_RADIUS: f32 = 2.0;
/// Below this side a button paints one solid rectangle (`PaintButton`).
const MIN_ROUNDED_BUTTON_SIDE: f32 = 5.0;
/// A checkable smaller than this paints only its fill.
const MIN_DECORATED_CHECKABLE_SIDE: f32 = 2.0;

/// `buttonface` and the theme's button fill.
pub(super) const BUTTON_FILL: Rgba = rgba(0xef, 0xef, 0xef, OPAQUE);
const BUTTON_FILL_DISABLED: Rgba = rgba(0xd9, 0xd9, 0xd9, 0x71);
/// `field`: text fields, textareas and unchecked checkables.
pub(super) const FIELD_FILL: Rgba = rgba(0xff, 0xff, 0xff, OPAQUE);
const FIELD_FILL_DISABLED: Rgba = rgba(0xf5, 0xf5, 0xf5, 0xae);
const FRAME: Rgba = rgba(0x76, 0x76, 0x76, OPAQUE);
const FRAME_DISABLED: Rgba = rgba(0x76, 0x76, 0x76, 0x4d);
/// The default accent of checked checkables.
const ACCENT: Rgba = rgba(0x00, 0x75, 0xff, OPAQUE);
const CHECKMARK: Rgba = rgba(0xff, 0xff, 0xff, OPAQUE);

/// `buttontext` and `fieldtext`.
const CONTROL_TEXT: Rgba = rgba(0x00, 0x00, 0x00, OPAQUE);
/// `button:disabled { color: rgba(16, 16, 16, 0.3) }`.
const BUTTON_TEXT_DISABLED: Rgba = rgba(0x10, 0x10, 0x10, 0x4d);
/// `input:disabled, textarea:disabled { color: rgb(84, 84, 84) }`.
const FIELD_TEXT_DISABLED: Rgba = rgba(0x54, 0x54, 0x54, OPAQUE);
/// `::placeholder { color: #757575 }` in the light scheme.
pub(super) const PLACEHOLDER: Rgba = rgba(0x75, 0x75, 0x75, OPAQUE);

/// `-webkit-focus-ring-color` on macOS.
const FOCUS_RING: Rgba = rgba(0x00, 0x5f, 0xcc, OPAQUE);
/// The focus ring's stroke width in CSS pixels.
const FOCUS_RING_WIDTH: f32 = 2.0;
/// `outline-offset` of focused checkboxes and radios.
const CHECKABLE_FOCUS_OFFSET: f32 = 2.0;
/// A border at least this wide takes one pixel of the ring inside the box
/// (Blink's `FocusRingOffset`).
const FOCUS_RING_INSIDE_BORDER: f32 = 1.0;

/// The check mark's stroke width and corner inset as fractions of the
/// checkbox side (`NativeThemeBase::PaintCheckbox`).
const CHECK_STROKE: f32 = 0.16;
const CHECK_INSET: f32 = 0.2;
/// The radio dot's inset as a fraction of the side.
const RADIO_DOT_INSET: f32 = 0.2;

/// The user-agent text color of a control without an author `color`.
pub(super) fn text_color(kind: ControlKind, disabled: bool) -> Color {
    match (kind, disabled) {
        (_, false) => CONTROL_TEXT,
        (ControlKind::Button | ControlKind::Select, true) => BUTTON_TEXT_DISABLED,
        (_, true) => FIELD_TEXT_DISABLED,
    }
    .color()
}

/// The user-agent background of an author-styled control without an author
/// `background-color` (Chrome's `buttonface` and `field`).
pub(super) fn background(kind: ControlKind) -> Option<Color> {
    match kind {
        ControlKind::Button => Some(BUTTON_FILL.color()),
        ControlKind::TextField | ControlKind::Textarea | ControlKind::Select => {
            Some(FIELD_FILL.color())
        }
        ControlKind::Checkbox | ControlKind::Radio => None,
    }
}

/// The user-agent `border-radius` of a themed control, which shapes its
/// pointer target: Chrome gives radios `border-radius: 50%`.
pub(super) fn user_agent_radii(kind: ControlKind, rect: BoxRect) -> Corners {
    if kind == ControlKind::Radio {
        Corners([[rect.width / 2.0, rect.height / 2.0]; 4])
    } else {
        Corners::default()
    }
}

/// Uniform corner radii.
fn round(radius: f32) -> Corners {
    Corners([[radius; 2]; 4])
}

fn inset(rect: BoxRect, by: f32) -> BoxRect {
    BoxRect {
        x: rect.x + by,
        y: rect.y + by,
        width: (rect.width - 2.0 * by).max(0.0),
        height: (rect.height - 2.0 * by).max(0.0),
    }
}

/// Fill `shape` with `radii`, clipped to the element's (unlocalized) clip.
fn fill(
    surface: &mut Surface,
    shape: BoxRect,
    radii: Corners,
    clip: BoxRect,
    color: Rgba,
) -> Result<(), PaintError> {
    let Some(path) = radii.path(shape) else {
        return Ok(());
    };
    let Some(bounds) = clipped(shape, surface.local_clip(clip)) else {
        return Ok(());
    };
    let mask = surface.mask(bounds)?;
    let mut paint = Paint::default();
    paint.set_color(color.color());
    surface.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        Some(&mask),
    );
    Ok(())
}

/// A frame `width` wide stroked with `radius` on the rectangle inset by half
/// its width: the ring between `rect` with radius `radius + width / 2` and
/// its inset by `width`.
fn frame(
    surface: &mut Surface,
    rect: BoxRect,
    (radius, width): (f32, f32),
    clip: BoxRect,
    color: Rgba,
) -> Result<(), PaintError> {
    let outer = radius + width / 2.0;
    let mut ring = PathBuilder::new();
    let (Some(outside), Some(inside)) = (
        round(outer).path(rect),
        round((outer - width).max(0.0)).path(inset(rect, width)),
    ) else {
        return Ok(());
    };
    ring.push_path(&outside);
    ring.push_path(&inside);
    let (Some(path), Some(bounds)) = (ring.finish(), clipped(rect, surface.local_clip(clip)))
    else {
        return Ok(());
    };
    let mask = surface.mask(bounds)?;
    let mut paint = Paint::default();
    paint.set_color(color.color());
    surface.fill_path(
        &path,
        &paint,
        FillRule::EvenOdd,
        Transform::identity(),
        Some(&mask),
    );
    Ok(())
}

/// Round each edge of `rect` to the nearest device pixel (half up), as
/// Chrome places a text field's theme.
fn device_rect(rect: BoxRect, scale: f32) -> BoxRect {
    let edge = |value: f32| (value * scale + 0.5).floor() / scale;
    let (x, y) = (edge(rect.x), edge(rect.y));
    BoxRect {
        x,
        y,
        width: edge(rect.x + rect.width) - x,
        height: edge(rect.y + rect.height) - y,
    }
}

/// The control's state as the theme distinguishes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct State {
    pub checked: bool,
    pub disabled: bool,
}

/// Paint themed control `id` into its snapped border box `rect`.
pub(super) fn paint(
    surface: &mut Surface,
    (id, kind): (u32, ControlKind),
    rect: BoxRect,
    clip: BoxRect,
    state: State,
) -> Result<(), PaintError> {
    let border = if state.disabled {
        FRAME_DISABLED
    } else {
        FRAME
    };
    match kind {
        ControlKind::Button | ControlKind::TextField | ControlKind::Textarea => {
            let fill_color = match (kind, state.disabled) {
                (ControlKind::Button, false) => BUTTON_FILL,
                (ControlKind::Button, true) => BUTTON_FILL_DISABLED,
                (_, false) => FIELD_FILL,
                (_, true) => FIELD_FILL_DISABLED,
            };
            if kind == ControlKind::Button
                && (rect.width < MIN_ROUNDED_BUTTON_SIDE || rect.height < MIN_ROUNDED_BUTTON_SIDE)
            {
                return fill(surface, rect, Corners::default(), clip, fill_color);
            }
            // A button's frame scales with the zoom; a field's theme snaps to
            // device pixels with a whole-pixel frame (Chrome 156 at 1.25×,
            // 1.5×, 1.75×, 2.5× and 3×).
            let (rect, width) = if kind == ControlKind::Button {
                (rect, FRAME_WIDTH)
            } else {
                let scale = surface.scale;
                (
                    device_rect(rect, scale),
                    (FRAME_WIDTH * scale + 0.5).floor().max(1.0) / scale,
                )
            };
            // The fill stops half a frame inside the box, so a translucent
            // frame shows the page under its outer half.
            fill(
                surface,
                inset(rect, width / 2.0),
                round(PART_RADIUS),
                clip,
                fill_color,
            )?;
            frame(surface, rect, (PART_RADIUS, width), clip, border)
        }
        ControlKind::Checkbox | ControlKind::Radio => checkable(surface, kind, rect, clip, state),
        // The select's arrow and popup belong to its own painter.
        ControlKind::Select => Err(PaintError::Unsupported(id)),
    }
}

fn checkable(
    surface: &mut Surface,
    kind: ControlKind,
    rect: BoxRect,
    clip: BoxRect,
    state: State,
) -> Result<(), PaintError> {
    let radio = kind == ControlKind::Radio;
    let accent = if state.disabled {
        FRAME_DISABLED
    } else {
        ACCENT
    };
    let background = if state.disabled {
        FIELD_FILL_DISABLED
    } else {
        FIELD_FILL
    };
    // The largest centered square.
    let side = rect.width.min(rect.height);
    let square = BoxRect {
        x: rect.x + (rect.width - side) / 2.0,
        y: rect.y + (rect.height - side) / 2.0,
        width: side,
        height: side,
    };
    let radius = if radio { side / 2.0 } else { PART_RADIUS };
    let filled = if state.checked && !radio {
        accent
    } else {
        background
    };
    if side <= MIN_DECORATED_CHECKABLE_SIDE {
        return fill(surface, square, Corners::default(), clip, filled);
    }
    fill(surface, square, round(radius), clip, filled)?;
    if !(state.checked && !radio) {
        let border = match (state.checked, state.disabled) {
            (true, _) => accent,
            (false, false) => FRAME,
            (false, true) => FRAME_DISABLED,
        };
        frame(surface, square, (radius, FRAME_WIDTH), clip, border)?;
    }
    if !state.checked {
        return Ok(());
    }
    if radio {
        let dot = inset(square, side * RADIO_DOT_INSET);
        return fill(surface, dot, round(dot.width / 2.0), clip, accent);
    }
    let mut check = PathBuilder::new();
    check.move_to(square.x + side * CHECK_INSET, square.y + side / 2.0);
    check.line_to(
        square.x + side * 2.0 * CHECK_INSET,
        square.y + side / 2.0 + side * CHECK_INSET,
    );
    check.line_to(
        square.x + side - side * CHECK_INSET,
        square.y + side * CHECK_INSET,
    );
    let stroke = Stroke {
        width: side * CHECK_STROKE,
        line_cap: LineCap::Butt,
        line_join: LineJoin::Miter,
        ..Stroke::default()
    };
    let Some(outline) = check
        .finish()
        .and_then(|path| path.stroke(&stroke, surface.scale))
    else {
        return Ok(());
    };
    let Some(bounds) = clipped(square, surface.local_clip(clip)) else {
        return Ok(());
    };
    let mask = surface.mask(bounds)?;
    let mut paint = Paint::default();
    paint.set_color(
        if state.disabled {
            FIELD_FILL_DISABLED
        } else {
            CHECKMARK
        }
        .color(),
    );
    surface.fill_path(
        &outline,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        Some(&mask),
    );
    Ok(())
}

/// Chrome's default `:focus-visible { outline: auto }` ring of a focused
/// control without author outline declarations: a two-pixel ring centered
/// on the border box outset by the user-agent `outline-offset`, less one
/// pixel when every border is at least one pixel wide, with the control's
/// own corner radius (theme part or author) at its center. Returns the
/// ring's outer rectangle and radii; it is painted as a two-pixel border.
pub(super) fn focus_ring(
    kind: ControlKind,
    themed: bool,
    rect: BoxRect,
    widths: [f32; 4],
    author_radii: Corners,
) -> (BoxRect, Corners, f32, Color) {
    let offset = if matches!(kind, ControlKind::Checkbox | ControlKind::Radio) {
        CHECKABLE_FOCUS_OFFSET
    } else {
        0.0
    };
    let offset = if widths
        .iter()
        .all(|width| *width >= FOCUS_RING_INSIDE_BORDER)
    {
        offset - FOCUS_RING_INSIDE_BORDER
    } else {
        offset
    };
    let half = FOCUS_RING_WIDTH / 2.0;
    let outer = offset + half;
    // The ring's center keeps the control's own radius whatever the offset.
    let radii = if themed {
        let part = if kind == ControlKind::Radio {
            rect.width.min(rect.height) / 2.0
        } else {
            PART_RADIUS
        };
        round(part + half)
    } else {
        author_radii.outset(half, half)
    };
    (
        BoxRect {
            x: rect.x - outer,
            y: rect.y - outer,
            width: rect.width + 2.0 * outer,
            height: rect.height + 2.0 * outer,
        },
        radii,
        FOCUS_RING_WIDTH,
        FOCUS_RING.color(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Access, BatchWriter, Broker, TextFont, TextMeasurer, View};
    use varta_common::view::{Attribute, Command, CssProperty as P, Tag};

    struct Deny;
    impl Broker for Deny {
        fn link_requested(&self, _: &str) -> bool {
            false
        }
        fn open_url(&mut self, _: &str) -> Access {
            Access::Denied
        }
        fn clipboard_write(&mut self) -> Access {
            Access::Denied
        }
    }

    const SIDE: u32 = 200;

    /// Controls in a white, red-text flex row (node 2) from (10, 10), 10px apart.
    fn raster(fill: impl FnOnce(&mut BatchWriter)) -> crate::RasterImage {
        let mut b = BatchWriter::new();
        b.create_element(2, Tag::Div)
            .insert(1, 2, 0)
            .set_style(2, P::Display, "flex")
            .set_style(2, P::AlignItems, "flex-start")
            .set_style(2, P::Gap, "10px")
            .set_style(2, P::Padding, "10px")
            .set_style(2, P::Height, "200px")
            .set_style(2, P::BackgroundColor, "#ffffff")
            .set_style(2, P::Color, "#ff0000")
            .set_style(2, P::FontFamily, "\"Varta Controls Fixture\"");
        fill(&mut b);
        let mut view = View::default();
        view.commit(&b.finish(), false, &mut Deny).unwrap();
        let mut font = TextMeasurer::new_with_fonts(vec![TextFont {
            data: include_bytes!("../../tests/fixtures/controls.ttf").to_vec(),
            family: "Varta Controls Fixture".into(),
            aliases: Vec::new(),
        }])
        .unwrap();
        view.rasterize_with_text(SIDE, SIDE, &mut font, 16.0)
            .unwrap()
    }

    fn pixel(image: &crate::RasterImage, x: u32, y: u32) -> [u8; 3] {
        let at = ((y * SIDE + x) * 4) as usize;
        [image.pixels[at], image.pixels[at + 1], image.pixels[at + 2]]
    }

    fn rgb(color: Rgba) -> [u8; 3] {
        [color.0[0], color.0[1], color.0[2]]
    }

    #[test]
    fn themed_controls_paint_chrome_fill_and_frame_colors() {
        let image = raster(|b| {
            b.create_element(3, Tag::Button).insert(2, 3, 0);
            b.create_element(4, Tag::Input).insert(2, 4, 0);
        });
        // The empty 16×6 button at (10, 10): frame, then fill.
        assert_eq!(pixel(&image, 18, 10), rgb(FRAME));
        assert_eq!(pixel(&image, 18, 12), rgb(BUTTON_FILL));
        // The 152×20 field at (36, 10): frame, then fill inside it.
        assert_eq!(pixel(&image, 100, 10), rgb(FRAME));
        assert_eq!(pixel(&image, 100, 11), rgb(FIELD_FILL));
        let checked = raster(|b| {
            b.create_element(5, Tag::Input)
                .set_attribute(5, Attribute::Type, "checkbox")
                .set_checked(5, 1, 0)
                .insert(2, 5, 0);
            b.create_element(6, Tag::Input)
                .set_attribute(6, Attribute::Type, "radio")
                .set_checked(6, 1, 0)
                .insert(2, 6, 0);
        });
        // The checked checkbox at (10, 10) fills with the accent.
        assert_eq!(pixel(&checked, 11, 16), rgb(ACCENT));
        // The radio at (33, 10): accent frame, white gap, accent dot.
        assert_eq!(pixel(&checked, 39, 10), rgb(ACCENT));
        assert_eq!(pixel(&checked, 39, 11), rgb(FIELD_FILL));
        assert_eq!(pixel(&checked, 39, 16), rgb(ACCENT));
    }

    #[test]
    fn disabled_controls_use_translucent_theme_layers_and_colors() {
        let image = raster(|b| {
            b.create_element(3, Tag::Input)
                .set_attribute(3, Attribute::Disabled, "")
                .insert(2, 3, 0);
        });
        let over_white = |layer: Rgba| {
            let [red, _, _, alpha] = layer.0;
            let (red, alpha, max) = (u32::from(red), u32::from(alpha), u32::from(u8::MAX));
            (red * alpha + max * (max - alpha)).div_ceil(max)
        };
        let [fill, ..] = pixel(&image, 100, 20);
        assert!(u32::from(fill).abs_diff(over_white(FIELD_FILL_DISABLED)) <= 1);
        // The translucent frame is composited over the page outside and the
        // fill inside: its row is lighter than the opaque frame.
        let [frame_value, ..] = pixel(&image, 100, 10);
        assert!(frame_value > FRAME.0[0] && frame_value < fill);
        assert_eq!(
            text_color(ControlKind::Button, true),
            BUTTON_TEXT_DISABLED.color()
        );
        assert_eq!(
            text_color(ControlKind::TextField, true),
            FIELD_TEXT_DISABLED.color()
        );
    }

    #[test]
    fn focused_controls_draw_the_default_ring_unless_the_author_sets_an_outline() {
        let focused = |outline: Option<&str>| {
            raster(|b| {
                b.create_element(3, Tag::Input).insert(2, 3, 0);
                if let Some(outline) = outline {
                    b.set_style(3, P::Outline, outline);
                }
                b.command(3, Command::Focus, 0.0);
            })
        };
        // The field's 2px borders take one pixel of the ring: it covers the
        // frame row and the row inside it.
        let ring = focused(None);
        assert_eq!(pixel(&ring, 100, 10), rgb(FOCUS_RING));
        assert_eq!(pixel(&ring, 100, 11), rgb(FOCUS_RING));
        assert_eq!(pixel(&ring, 100, 12), rgb(FIELD_FILL));
        let none = focused(Some("none"));
        assert_eq!(pixel(&none, 100, 10), rgb(FRAME));
        // A checkable's ring sits outside its square, around its radius.
        let (outer, radii, width, _) = focus_ring(
            ControlKind::Checkbox,
            true,
            BoxRect {
                x: 10.0,
                y: 10.0,
                width: 13.0,
                height: 13.0,
            },
            [0.0; 4],
            Corners::default(),
        );
        assert_eq!((outer.x, outer.width, width), (7.0, 19.0, 2.0));
        assert_eq!(radii, round(PART_RADIUS + 1.0));
    }

    #[test]
    fn control_text_uses_the_user_agent_color_not_the_inherited_one() {
        // The row's color is red; the button label and placeholder are not.
        let image = raster(|b| {
            b.create_element(3, Tag::Button)
                .insert(2, 3, 0)
                .create_text(4, "AAAA")
                .insert(3, 4, 0);
            b.create_element(5, Tag::Input)
                .set_attribute(5, Attribute::Placeholder, "AAAA")
                .insert(2, 5, 0);
        });
        // The label's first bar spans x 18.7–22.7 and y 14.7–24.
        assert_eq!(pixel(&image, 20, 20), rgb(CONTROL_TEXT));
        // The placeholder's first bar inside the field at (57.3, 10).
        assert_eq!(pixel(&image, 64, 20), rgb(PLACEHOLDER));
    }
}
