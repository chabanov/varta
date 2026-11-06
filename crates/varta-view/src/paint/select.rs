use super::*;
use crate::select::POPUP_ROWS;

const MENU_PADDING: f32 = 4.0;
const ARROW_SPACE: f32 = 16.0;

/// Frame-derived popup action; menu geometry uses physical pixel coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectHit {
    Choice { select: u32, option: u32 },
    Dismiss { select: u32 },
    Disabled { select: u32 },
}
impl SelectHit {
    #[must_use]
    pub fn target(self) -> u32 {
        match self {
            Self::Choice { select, .. } | Self::Dismiss { select } | Self::Disabled { select } => {
                select
            }
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectMenuMetrics {
    pub target: u32,
    /// Physical-pixel list bounds, independent of ancestor clipping.
    pub viewport: BoxRect,
    pub first: usize,
    pub max_first: usize,
    /// CSS-pixel row height, for logical wheel deltas.
    pub row_height: f32,
}
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SelectMenu {
    pub metrics: SelectMenuMetrics,
    rows: Vec<(u32, BoxRect, bool)>,
    /// Every listed option in row order, including rows scrolled out of the list.
    options: Vec<u32>,
}
impl SelectMenu {
    /// Physical row bounds of every listed option, and whether the row is
    /// presented: the presented (list-clipped) row of a visible option, and
    /// for a row scrolled out of the list the unclipped position the list's
    /// current scroll offset gives it.
    fn option_bounds(&self, scale: f32) -> impl Iterator<Item = (u32, BoxRect, bool)> + '_ {
        let viewport = self.metrics.viewport;
        let height = self.metrics.row_height * scale;
        self.options.iter().enumerate().map(move |(index, option)| {
            self.rows
                .iter()
                .find(|(row, _, _)| row == option)
                .map_or_else(
                    || {
                        let row = BoxRect {
                            x: viewport.x,
                            y: viewport.y + (index as f32 - self.metrics.first as f32) * height,
                            width: viewport.width,
                            height,
                        };
                        (*option, row, false)
                    },
                    |(_, rect, _)| (*option, *rect, true),
                )
        })
    }
    pub fn hit(&self, x: f32, y: f32) -> Option<SelectHit> {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let select = self.metrics.target;
        for (option, rect, disabled) in &self.rows {
            if contains(*rect, x, y) {
                return Some(if *disabled {
                    SelectHit::Disabled { select }
                } else {
                    SelectHit::Choice {
                        select,
                        option: *option,
                    }
                });
            }
        }
        Some(if contains(self.metrics.viewport, x, y) {
            SelectHit::Disabled { select }
        } else {
            SelectHit::Dismiss { select }
        })
    }
}
pub(super) fn contains(rect: BoxRect, x: f32, y: f32) -> bool {
    x >= rect.x && y >= rect.y && x < rect.x + rect.width && y < rect.y + rect.height
}
fn label_shape(
    tree: &Tree,
    env: &Environment,
    id: u32,
    label: String,
    font: &mut TextMeasurer,
    size: f32,
    work: &mut usize,
) -> Result<TextShape, PaintError> {
    *work = work.saturating_add(label.len());
    if *work > MAX_TEXT_PAINT_WORK || label.chars().any(char::is_control) {
        return Err(PaintError::Unsupported(id));
    }
    let (mut options, families) = layout::text_style(tree, env, id, size, font)?;
    options.white_space = WhiteSpaceMode::NoWrap;
    options.break_anywhere = false;
    let metrics = font
        .measure_styled(
            if label.is_empty() { " " } else { &label },
            None,
            &families,
            options,
        )
        .map_err(|error| PaintError::Layout(LayoutError::Text(error.to_string())))?;
    Ok(TextShape {
        rich: None,
        text: label,
        metrics,
        options,
        families,
    })
}
fn label(
    surface: &mut Surface,
    font: (&mut TextMeasurer, &Tree, &Environment),
    shape: &TextShape,
    bounds: (BoxRect, BoxRect),
    color: Color,
    id: u32,
    work: &mut usize,
) -> Result<(), PaintError> {
    let (font, tree, env) = font;
    let (rect, clip) = bounds;
    if shape.text.is_empty() || rect.width <= 0.0 || rect.height <= 0.0 {
        return Ok(());
    }
    let rect = BoxRect {
        y: rect.y + (rect.height - shape.metrics.height) / 2.0,
        height: shape.metrics.height,
        ..rect
    };
    paint_text(
        surface,
        font,
        shape,
        layout::BoxGeometry {
            rect,
            clip,
            content: rect,
            scroll: None,
            programmatic_scroll: None,
            scrollbars: None,
            transform: None,
        },
        underline::Style::resolve(tree, env, id, true, color)?,
        id,
        work,
    )
    .map(|_| ())
}
pub(super) fn paint_control(
    surface: &mut Surface,
    tree: &Tree,
    env: &Environment,
    id: u32,
    bounds: layout::BoxGeometry,
    font: (&mut TextMeasurer, f32),
    work: &mut usize,
) -> Result<(), PaintError> {
    let (font, size) = font;
    let bounds = layout::BoxGeometry {
        clip: surface.local_clip(bounds.clip),
        ..bounds
    };
    for option in tree.select_options(id) {
        if tree
            .computed_read(option, env)
            .keys()
            .any(|property| !matches!(property, CssProperty::Display | CssProperty::Color))
        {
            return Err(PaintError::Unsupported(option));
        }
    }
    let text = tree
        .selected_option(id)
        .map_or_else(String::new, |option| tree.option_label(option));
    let shape = label_shape(tree, env, id, text, font, size, work)?;
    let content = bounds.content;
    let color = current_color(tree, env, id).ok_or(PaintError::Unsupported(id))?;
    let text_box = BoxRect {
        width: (content.width - ARROW_SPACE).max(0.0),
        ..content
    };
    label(
        surface,
        (font, tree, env),
        &shape,
        (text_box, layout::intersect(bounds.clip, text_box)),
        color,
        id,
        work,
    )?;
    let arrow = BoxRect {
        x: content.x + (content.width - ARROW_SPACE).max(0.0),
        width: content.width.min(ARROW_SPACE),
        ..content
    };
    let Some(clip) = clipped(arrow, bounds.clip) else {
        return Ok(());
    };
    let side = arrow.width.min(arrow.height).min(8.0);
    let cx = arrow.x + arrow.width / 2.0;
    let cy = arrow.y + arrow.height / 2.0;
    let mut path = PathBuilder::new();
    path.move_to(cx - side / 2.0, cy - side / 4.0);
    path.line_to(cx + side / 2.0, cy - side / 4.0);
    path.line_to(cx, cy + side / 4.0);
    path.close();
    if let Some(path) = path.finish() {
        let mask = if bounds.clip.x <= arrow.x
            && bounds.clip.y <= arrow.y
            && bounds.clip.x + bounds.clip.width >= arrow.x + arrow.width
            && bounds.clip.y + bounds.clip.height >= arrow.y + arrow.height
        {
            None
        } else {
            Some(surface.mask(clip)?)
        };
        let mut paint = Paint::default();
        paint.set_color(color);
        surface.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            mask.as_ref(),
        );
    }
    Ok(())
}
pub(super) fn paint_menu(
    surface: &mut Surface,
    tree: &Tree,
    env: &Environment,
    id: u32,
    bounds: layout::BoxGeometry,
    font: (&mut TextMeasurer, f32),
    work: &mut usize,
) -> Result<SelectMenu, PaintError> {
    let (font, size) = font;
    let element = tree.element(id).ok_or(PaintError::Unsupported(id))?;
    let popup = element
        .editor
        .select_popup
        .as_ref()
        .ok_or(PaintError::Unsupported(id))?;
    let options: Vec<_> = tree
        .select_options(id)
        .into_iter()
        .filter(|option| {
            tree.computed_read(*option, env)
                .get(&CssProperty::Display)
                .is_none_or(|value| value != "none")
        })
        .collect();
    if options.len() > 1024 {
        return Err(PaintError::Unsupported(id));
    }
    let mut shapes = Vec::new();
    let mut row_height = 20.0_f32;
    let mut menu_width = bounds.rect.width;
    for option in &options {
        let shape = label_shape(tree, env, id, tree.option_label(*option), font, size, work)?;
        row_height = row_height.max(shape.metrics.height + MENU_PADDING * 2.0);
        menu_width = menu_width.max(shape.metrics.width + MENU_PADDING * 2.0);
        shapes.push(shape);
    }
    let viewport_width = surface.width() as f32 / surface.scale;
    let viewport_height = surface.height() as f32 / surface.scale;
    menu_width = menu_width.min(viewport_width);
    let below = (viewport_height - bounds.rect.y - bounds.rect.height).max(0.0);
    let above = bounds.rect.y.clamp(0.0, viewport_height);
    let upward = below < row_height * options.len().min(POPUP_ROWS) as f32 && above > below;
    let room = if upward { above } else { below };
    let count = ((room / row_height).floor() as usize)
        .clamp(1, POPUP_ROWS)
        .min(options.len().max(1));
    let height = (row_height * count as f32).min(viewport_height);
    let x = bounds
        .rect
        .x
        .clamp(0.0, (viewport_width - menu_width).max(0.0));
    let y = (if upward {
        bounds.rect.y - height
    } else {
        bounds.rect.y + bounds.rect.height
    })
    .clamp(0.0, (viewport_height - height).max(0.0));
    let menu = BoxRect {
        x,
        y,
        width: menu_width,
        height,
    };
    let max_first = options.len().saturating_sub(count);
    let active = popup
        .active
        .and_then(|active| options.iter().position(|option| *option == active));
    let mut first = popup.first.min(max_first);
    if !popup.manual_scroll
        && let Some(active) = active
    {
        if active < first {
            first = active;
        } else if active >= first + count {
            first = active + 1 - count;
        }
    }
    let mut rows = Vec::new();
    let background = tree
        .computed_read(id, env)
        .get(&CssProperty::BackgroundColor)
        .and_then(|value| {
            if value == "currentcolor" {
                current_color(tree, env, id)
            } else {
                color(value)
            }
        })
        .ok_or(PaintError::Unsupported(id))?;
    let mut paint = Paint::default();
    paint.set_color(background);
    if let Some(rect) = Rect::from_xywh(x, y, menu_width, height) {
        surface.fill_rect(rect, &paint, Transform::identity(), None);
    }
    for (index, (option, shape)) in options
        .iter()
        .zip(&shapes)
        .enumerate()
        .skip(first)
        .take(count)
    {
        let row = BoxRect {
            x,
            y: y + (index - first) as f32 * row_height,
            width: menu_width,
            height: row_height.min(height),
        };
        let row = layout::intersect(row, menu);
        let disabled = tree
            .element(*option)
            .is_none_or(|element| element.disabled());
        let highlighted = popup.active == Some(*option) && !disabled;
        let hovered = !disabled && tree.pointer_contains(tree.hovered.as_ref(), *option);
        if highlighted || hovered {
            let mut paint = Paint::default();
            if highlighted {
                paint.set_color_rgba8(40, 90, 180, 255);
            } else {
                paint.set_color_rgba8(230, 240, 255, 255);
            }
            if let Some(rect) = Rect::from_xywh(row.x, row.y, row.width, row.height) {
                surface.fill_rect(rect, &paint, Transform::identity(), None);
            }
        }
        let color = if disabled {
            Color::from_rgba8(117, 117, 117, 255)
        } else if highlighted {
            Color::WHITE
        } else {
            current_color(tree, env, *option).ok_or(PaintError::Unsupported(*option))?
        };
        let text = BoxRect {
            x: row.x + MENU_PADDING,
            width: (row.width - 2.0 * MENU_PADDING).max(0.0),
            ..row
        };
        label(
            surface,
            (font, tree, env),
            shape,
            (text, row),
            color,
            id,
            work,
        )?;
        rows.push((*option, scaled_rect(row, surface.scale), disabled));
    }
    paint_border(
        surface,
        menu,
        menu,
        [1.0; 4],
        Corners::default(),
        [current_color(tree, env, id).ok_or(PaintError::Unsupported(id))?; 4],
    )?;
    Ok(SelectMenu {
        metrics: SelectMenuMetrics {
            target: id,
            viewport: scaled_rect(menu, surface.scale),
            first,
            max_first,
            row_height,
        },
        rows,
        options,
    })
}

impl FrameMetadata {
    /// Physical bounds of the open select list's options, in row order, and
    /// whether each row is presented. Empty while no list is open.
    pub(crate) fn select_option_bounds(&self) -> Vec<(u32, BoxRect, bool)> {
        self.select_menu
            .as_ref()
            .map(|menu| menu.option_bounds(self.scale).collect())
            .unwrap_or_default()
    }
}
