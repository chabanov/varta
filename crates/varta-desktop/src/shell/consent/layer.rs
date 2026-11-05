//! The host-owned prompt layer: the content, layout, pixels and input
//! handling of one permission prompt, independent of any window.
//!
//! Everything it shows comes from the broker's [`PermissionPrompt`]: the
//! capability id, the application identity and the exact requested
//! resources are the broker's own description; the package's `purpose` is
//! shown only as a marked quote below them. Text is drawn with the layer's
//! own bitmap font ([`super::glyphs`]), never with a font the guest
//! supplies. Nothing of the guest's view (its nodes, styles, frame or
//! title) is an input of this module, so guest content can neither paint
//! over the layer nor receive its input; the native window presents it in
//! a separate host window ([`crate::shell::window`]).
use super::glyphs::{self, CAP_ROWS, COLUMNS, ROWS};
use std::time::{Duration, Instant};
use varta_common::permission::{PermissionPrompt, PromptAnswer};

/// How long after the layer is shown it ignores activation, so a click or
/// key press meant for the application (for example a double click whose
/// first click opened the prompt) cannot answer it.
pub const PROMPT_INPUT_DELAY: Duration = Duration::from_millis(500);

/// Logical pixels per font pixel of body text and of the heading.
const BODY_PIXEL: u32 = 2;
const HEADING_PIXEL: u32 = 3;
/// Empty font pixels after each glyph and below each line.
const GLYPH_GAP: u32 = 1;
const LINE_GAP: u32 = 1;
/// The layer's logical width and the space around its content.
const LAYER_WIDTH: u32 = 640;
const PADDING: u32 = 24;
/// The host band across the top of the layer.
const BAND_HEIGHT: u32 = 6;
/// Space between sections.
const SECTION_GAP: u32 = 8;
/// The quote's indent and the bar that marks it.
const QUOTE_INDENT: u32 = 16;
const QUOTE_BAR: u32 = 4;
/// Buttons: height, horizontal padding, gap and focus ring.
const BUTTON_HEIGHT: u32 = 40;
const BUTTON_PADDING: u32 = 16;
const BUTTON_GAP: u32 = 12;
const BUTTON_BORDER: u32 = 1;
const FOCUS_RING: u32 = 3;
const FOCUS_OFFSET: u32 = 3;
/// Shown lines of requested resources and of the quoted purpose; the rest
/// is summarized, so the layer's height is bounded.
pub const MAX_RESOURCE_LINES: usize = 8;
pub const MAX_QUOTE_LINES: usize = 8;

/// Colors (`0RGB`, the presentation buffer's format).
const BACKGROUND: u32 = 0x00ff_ffff;
const BAND: u32 = 0x0026_5dcc;
const TEXT: u32 = 0x0011_1111;
const LABEL: u32 = 0x0055_5555;
const CODE_BACKGROUND: u32 = 0x00f0_f0f0;
const QUOTE_BAR_COLOR: u32 = 0x00bb_bbbb;
const BUTTON_FILL: u32 = 0x00ee_eeee;
const BUTTON_EDGE: u32 = 0x0077_7777;
const FOCUS: u32 = 0x0026_5dcc;

/// Host-owned text.
const HEADING: &str = "Permission request";
const REQUIRED: &str = "This permission is required to start the application.";
const OPTIONAL: &str = "The application requests this permission.";
const PERMISSION_LABEL: &str = "Permission:";
const APPLICATION_LABEL: &str = "Application identity:";
const RESOURCES_LABEL: &str = "Requested resources:";
const PURPOSE_LABEL: &str = "The application says:";
const SCOPE_NOTE: &str = "Your choice applies only within the host's allowed scope.";
/// The marker of a quote line cut at [`MAX_QUOTE_LINES`].
const CUT: &str = "...";

/// The buttons, left to right, with their labels. Deny is first and holds
/// the initial focus.
const BUTTONS: [(PromptAnswer, &str); 3] = [
    (PromptAnswer::Deny, "Deny"),
    (PromptAnswer::Dismiss, "Dismiss"),
    (PromptAnswer::Allow, "Allow"),
];

/// A key the layer handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKey {
    /// Tab, arrow right: the next button.
    Next,
    /// Shift+Tab, arrow left: the previous button.
    Previous,
    /// Enter or Space: the focused button.
    Activate,
    /// Escape: dismiss.
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Style {
    Heading,
    Body,
    Label,
    Code,
    Quote,
}
impl Style {
    fn pixel(self) -> u32 {
        match self {
            Self::Heading => HEADING_PIXEL,
            _ => BODY_PIXEL,
        }
    }
    fn indent(self) -> u32 {
        match self {
            Self::Quote => QUOTE_INDENT,
            _ => 0,
        }
    }
}

fn advance(pixel: u32) -> u32 {
    (COLUMNS as u32 + GLYPH_GAP) * pixel
}
fn line_height(pixel: u32) -> u32 {
    (ROWS as u32 + LINE_GAP) * pixel
}
/// The characters one line of `style` holds.
fn columns(style: Style) -> usize {
    ((LAYER_WIDTH - 2 * PADDING - style.indent()) / advance(style.pixel())) as usize
}

/// A logical rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}
impl Rect {
    fn contains(self, x: u32, y: u32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Line {
    text: String,
    style: Style,
    y: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Button {
    answer: PromptAnswer,
    label: &'static str,
    rect: Rect,
}

/// One open permission prompt.
#[derive(Debug, Clone)]
pub struct PromptLayer {
    lines: Vec<Line>,
    buttons: Vec<Button>,
    height: u32,
    focus: usize,
    pressed: Option<usize>,
    shown: Instant,
}

/// Lines of `text` at most `width` characters wide, broken at spaces where
/// possible and inside a word longer than a line.
fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split(' ').filter(|word| !word.is_empty()) {
        let mut word = word;
        loop {
            let needed = if current.is_empty() {
                word.len()
            } else {
                current.len() + 1 + word.len()
            };
            if needed <= width {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(word);
                break;
            }
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
                continue;
            }
            // A word longer than a line: hard break (ASCII only here).
            let (head, tail) = word.split_at(width);
            lines.push(head.to_owned());
            word = tail;
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// Lines of `text` cut every `width` characters (identifiers and patterns,
/// where every character matters).
fn wrap_hard(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    text.as_bytes()
        .chunks(width)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect()
}

struct Builder {
    lines: Vec<Line>,
    y: u32,
}
impl Builder {
    fn push(&mut self, text: String, style: Style) {
        self.lines.push(Line {
            text,
            style,
            y: self.y,
        });
        self.y += line_height(style.pixel());
    }
    fn gap(&mut self) {
        self.y += SECTION_GAP;
    }
}

impl PromptLayer {
    /// The layer of `prompt`, shown at `shown`.
    #[must_use]
    pub fn new(prompt: &PermissionPrompt, shown: Instant) -> Self {
        let mut text = Builder {
            lines: Vec::new(),
            y: BAND_HEIGHT + PADDING,
        };
        text.push(HEADING.into(), Style::Heading);
        let intro = if prompt.required { REQUIRED } else { OPTIONAL };
        for line in wrap_words(intro, columns(Style::Body)) {
            text.push(line, Style::Body);
        }
        text.gap();
        text.push(PERMISSION_LABEL.into(), Style::Label);
        for line in wrap_hard(&glyphs::visible(&prompt.capability), columns(Style::Code)) {
            text.push(line, Style::Code);
        }
        text.push(APPLICATION_LABEL.into(), Style::Label);
        for line in wrap_hard(&glyphs::visible(&prompt.artifact), columns(Style::Code)) {
            text.push(line, Style::Code);
        }
        if !prompt.scope.is_empty() {
            text.push(RESOURCES_LABEL.into(), Style::Label);
            let mut shown_lines = 0;
            let mut hidden = 0;
            for pattern in &prompt.scope {
                let lines = wrap_hard(&glyphs::visible(pattern), columns(Style::Code));
                if hidden > 0 || shown_lines + lines.len() > MAX_RESOURCE_LINES {
                    hidden += 1;
                    continue;
                }
                shown_lines += lines.len();
                for line in lines {
                    text.push(line, Style::Code);
                }
            }
            if hidden > 0 {
                text.push(format!("and {hidden} more not shown"), Style::Body);
            }
        }
        if let Some(purpose) = &prompt.purpose {
            text.push(PURPOSE_LABEL.into(), Style::Label);
            let mut lines = wrap_words(&glyphs::visible(purpose), columns(Style::Quote));
            if lines.len() > MAX_QUOTE_LINES {
                lines.truncate(MAX_QUOTE_LINES);
                if let Some(last) = lines.last_mut() {
                    let keep = columns(Style::Quote) - CUT.len();
                    last.truncate(keep.min(last.len()));
                    last.push_str(CUT);
                }
            }
            for line in lines {
                text.push(line, Style::Quote);
            }
        }
        for line in wrap_words(SCOPE_NOTE, columns(Style::Body)) {
            text.push(line, Style::Body);
        }
        text.gap();
        let label_width = |label: &str| label.len() as u32 * advance(BODY_PIXEL);
        let mut x = LAYER_WIDTH - PADDING;
        let mut buttons: Vec<Button> = BUTTONS
            .iter()
            .rev()
            .map(|(answer, label)| {
                let width = label_width(label) + 2 * BUTTON_PADDING;
                x -= width;
                let rect = Rect {
                    x,
                    y: text.y,
                    width,
                    height: BUTTON_HEIGHT,
                };
                x -= BUTTON_GAP;
                Button {
                    answer: *answer,
                    label,
                    rect,
                }
            })
            .collect();
        buttons.reverse();
        Self {
            height: text.y + BUTTON_HEIGHT + PADDING,
            lines: text.lines,
            buttons,
            focus: 0,
            pressed: None,
            shown,
        }
    }

    /// The layer's logical size.
    #[must_use]
    pub fn logical_size(&self) -> (u32, u32) {
        (LAYER_WIDTH, self.height)
    }

    /// The answer the focused button gives.
    #[must_use]
    pub fn focused(&self) -> PromptAnswer {
        self.buttons[self.focus].answer
    }

    /// Whether activation is accepted at `now` ([`PROMPT_INPUT_DELAY`]).
    #[must_use]
    pub fn ready(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.shown) >= PROMPT_INPUT_DELAY
    }

    /// Physical pixels per logical pixel for a `width`x`height` surface:
    /// the largest whole factor at which the layer fits, at least 1.
    fn unit(&self, width: u32, height: u32) -> u32 {
        (width / LAYER_WIDTH).min(height / self.height).max(1)
    }

    /// The button under physical (`x`, `y`) of a `width`x`height` surface.
    fn button_index(&self, x: f32, y: f32, width: u32, height: u32) -> Option<usize> {
        if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 {
            return None;
        }
        let unit = self.unit(width, height);
        let (x, y) = (x as u32 / unit, y as u32 / unit);
        self.buttons
            .iter()
            .position(|button| button.rect.contains(x, y))
    }

    /// The answer of the button under physical (`x`, `y`), if any.
    #[must_use]
    pub fn button_at(&self, x: f32, y: f32, width: u32, height: u32) -> Option<PromptAnswer> {
        self.button_index(x, y, width, height)
            .map(|index| self.buttons[index].answer)
    }

    /// A primary button press at physical (`x`, `y`): remembered only when
    /// the layer is ready and the press is on a button.
    pub fn press(&mut self, x: f32, y: f32, width: u32, height: u32, now: Instant) {
        self.pressed = self
            .ready(now)
            .then(|| self.button_index(x, y, width, height))
            .flatten();
        if let Some(index) = self.pressed {
            self.focus = index;
        }
    }

    /// The release of a press: the answer of the button when the press
    /// began and ends on it.
    pub fn release(
        &mut self,
        x: f32,
        y: f32,
        width: u32,
        height: u32,
        now: Instant,
    ) -> Option<PromptAnswer> {
        let pressed = self.pressed.take()?;
        (self.ready(now) && self.button_index(x, y, width, height) == Some(pressed))
            .then(|| self.buttons[pressed].answer)
    }

    /// A key: focus moves at any time; an answer needs [`Self::ready`].
    pub fn key(&mut self, key: PromptKey, now: Instant) -> Option<PromptAnswer> {
        let count = self.buttons.len();
        match key {
            PromptKey::Next => {
                self.focus = (self.focus + 1) % count;
                None
            }
            PromptKey::Previous => {
                self.focus = (self.focus + count - 1) % count;
                None
            }
            PromptKey::Activate => self.ready(now).then(|| self.focused()),
            PromptKey::Cancel => self.ready(now).then_some(PromptAnswer::Dismiss),
        }
    }

    /// The layer's pixels for a `width`x`height` surface (`0RGB`, row
    /// major). Space beyond the layer's whole-factor size is background.
    #[must_use]
    pub fn render(&self, width: u32, height: u32) -> Vec<u32> {
        let mut canvas = Canvas {
            pixels: vec![BACKGROUND; width as usize * height as usize],
            width,
            height,
            unit: self.unit(width, height),
        };
        canvas.fill(
            Rect {
                x: 0,
                y: 0,
                width: LAYER_WIDTH,
                height: BAND_HEIGHT,
            },
            BAND,
        );
        for line in &self.lines {
            let pixel = line.style.pixel();
            let x = PADDING + line.style.indent();
            match line.style {
                Style::Code => canvas.fill(
                    Rect {
                        x,
                        y: line.y,
                        width: line.text.len() as u32 * advance(pixel),
                        height: line_height(pixel),
                    },
                    CODE_BACKGROUND,
                ),
                Style::Quote => canvas.fill(
                    Rect {
                        x: PADDING,
                        y: line.y,
                        width: QUOTE_BAR,
                        height: line_height(pixel),
                    },
                    QUOTE_BAR_COLOR,
                ),
                _ => {}
            }
            let color = if line.style == Style::Label {
                LABEL
            } else {
                TEXT
            };
            canvas.text(&line.text, x, line.y + LINE_GAP * pixel / 2, pixel, color);
        }
        for (index, button) in self.buttons.iter().enumerate() {
            let rect = button.rect;
            if index == self.focus {
                canvas.fill(
                    Rect {
                        x: rect.x - FOCUS_OFFSET - FOCUS_RING,
                        y: rect.y - FOCUS_OFFSET - FOCUS_RING,
                        width: rect.width + 2 * (FOCUS_OFFSET + FOCUS_RING),
                        height: rect.height + 2 * (FOCUS_OFFSET + FOCUS_RING),
                    },
                    FOCUS,
                );
                canvas.fill(
                    Rect {
                        x: rect.x - FOCUS_OFFSET,
                        y: rect.y - FOCUS_OFFSET,
                        width: rect.width + 2 * FOCUS_OFFSET,
                        height: rect.height + 2 * FOCUS_OFFSET,
                    },
                    BACKGROUND,
                );
            }
            canvas.fill(rect, BUTTON_EDGE);
            canvas.fill(
                Rect {
                    x: rect.x + BUTTON_BORDER,
                    y: rect.y + BUTTON_BORDER,
                    width: rect.width - 2 * BUTTON_BORDER,
                    height: rect.height - 2 * BUTTON_BORDER,
                },
                BUTTON_FILL,
            );
            let text_height = CAP_ROWS as u32 * BODY_PIXEL;
            canvas.text(
                button.label,
                rect.x + BUTTON_PADDING,
                rect.y + (rect.height - text_height) / 2,
                BODY_PIXEL,
                TEXT,
            );
        }
        canvas.pixels
    }
}

/// A physical pixel buffer drawn in logical units.
struct Canvas {
    pixels: Vec<u32>,
    width: u32,
    height: u32,
    unit: u32,
}
impl Canvas {
    fn fill(&mut self, rect: Rect, color: u32) {
        let x0 = (rect.x * self.unit).min(self.width);
        let y0 = (rect.y * self.unit).min(self.height);
        let x1 = ((rect.x + rect.width) * self.unit).min(self.width);
        let y1 = ((rect.y + rect.height) * self.unit).min(self.height);
        for y in y0..y1 {
            let row = y as usize * self.width as usize;
            self.pixels[row + x0 as usize..row + x1 as usize].fill(color);
        }
    }
    fn text(&mut self, text: &str, x: u32, y: u32, pixel: u32, color: u32) {
        let mut left = x;
        for character in text.chars() {
            if let Some(rows) = glyphs::rows(character) {
                for (row, bits) in (0u32..).zip(rows) {
                    for column in 0..COLUMNS as u32 {
                        if bits >> (COLUMNS as u32 - 1 - column) & 1 == 1 {
                            self.fill(
                                Rect {
                                    x: left + column * pixel,
                                    y: y + row * pixel,
                                    width: pixel,
                                    height: pixel,
                                },
                                color,
                            );
                        }
                    }
                }
            }
            left += advance(pixel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDENTITY: &str =
        "sha256:00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

    fn prompt(purpose: Option<&str>, scope: &[&str]) -> PermissionPrompt {
        PermissionPrompt {
            artifact: IDENTITY.into(),
            capability: "net.request".into(),
            scope: scope.iter().map(|pattern| (*pattern).to_owned()).collect(),
            purpose: purpose.map(Into::into),
            required: false,
        }
    }

    fn texts(layer: &PromptLayer, style: Style) -> Vec<&str> {
        layer
            .lines
            .iter()
            .filter(|line| line.style == style)
            .map(|line| line.text.as_str())
            .collect()
    }

    #[test]
    fn the_broker_description_comes_first_and_the_purpose_only_as_a_quote() {
        let layer = PromptLayer::new(
            &prompt(Some("Connect to your workspace"), &["https://api.test/**"]),
            Instant::now(),
        );
        let order: Vec<_> = layer.lines.iter().map(|line| line.text.as_str()).collect();
        let position = |text: &str| order.iter().position(|line| *line == text).unwrap();
        assert!(position("net.request") < position("Connect to your workspace"));
        assert!(position("https://api.test/**") < position("Connect to your workspace"));
        assert_eq!(texts(&layer, Style::Quote), ["Connect to your workspace"]);
        // The identity is shown whole, across lines.
        assert_eq!(
            texts(&layer, Style::Code)
                .iter()
                .filter(|line| !line.contains("net.") && !line.contains("https"))
                .copied()
                .collect::<String>(),
            IDENTITY
        );
        let required = PromptLayer::new(
            &PermissionPrompt {
                required: true,
                ..prompt(None, &[])
            },
            Instant::now(),
        );
        // Host sentences wrap at spaces and stay whole.
        assert!(
            texts(&required, Style::Body)
                .join(" ")
                .starts_with(REQUIRED)
        );
        assert!(texts(&required, Style::Quote).is_empty());
        assert!(!texts(&required, Style::Label).contains(&RESOURCES_LABEL));
    }

    #[test]
    fn untrusted_text_is_bounded_escaped_and_never_moves_the_buttons() {
        let shown = Instant::now();
        let plain = PromptLayer::new(&prompt(Some("Sync"), &["https://a.test/"]), shown);
        // A purpose that imitates the host's buttons and a flood of
        // resources: the quote and the resource list are cut, and the
        // invisible and lookalike characters are escaped.
        let spoof = "[ Allow ] ".repeat(40);
        let many: Vec<String> = (0..40)
            .map(|index| format!("https://h{index}.test/\u{200b}/**"))
            .collect();
        let many: Vec<&str> = many.iter().map(String::as_str).collect();
        let hostile = PromptLayer::new(&prompt(Some(&spoof), &many), shown);
        assert_eq!(texts(&hostile, Style::Quote).len(), MAX_QUOTE_LINES);
        assert!(texts(&hostile, Style::Quote).last().unwrap().ends_with(CUT));
        let resources = texts(&hostile, Style::Code)
            .iter()
            .filter(|line| line.starts_with("https"))
            .count();
        assert_eq!(resources, MAX_RESOURCE_LINES);
        assert!(
            texts(&hostile, Style::Body)
                .iter()
                .any(|line| line.ends_with("more not shown"))
        );
        assert!(
            texts(&hostile, Style::Code)
                .iter()
                .any(|line| line.contains("\\u{200b}"))
        );
        assert!(
            hostile
                .lines
                .iter()
                .all(|line| line.text.chars().all(glyphs::drawable))
        );
        // Every hit target of the whole surface is one of the three host
        // buttons, in both layers; a quote that reads like a button is not.
        for layer in [&plain, &hostile] {
            let (width, height) = layer.logical_size();
            let mut answers = std::collections::BTreeSet::new();
            for y in 0..height {
                for x in 0..width {
                    if let Some(answer) = layer.button_at(x as f32, y as f32, width, height) {
                        let index = layer.button_index(x as f32, y as f32, width, height);
                        assert!(layer.buttons[index.unwrap()].rect.contains(x, y));
                        answers.insert(format!("{answer:?}"));
                    }
                }
            }
            assert_eq!(answers.len(), BUTTONS.len());
            let quote_bottom = layer
                .lines
                .iter()
                .map(|line| line.y + line_height(line.style.pixel()))
                .max()
                .unwrap();
            assert!(layer.buttons.iter().all(|b| b.rect.y >= quote_bottom));
        }
        // The buttons sit at the same place relative to the bottom edge,
        // with the same pixels, whatever the package text is.
        let band = |layer: &PromptLayer| {
            let (width, height) = layer.logical_size();
            let pixels = layer.render(width, height);
            let top = (height - PADDING - BUTTON_HEIGHT - FOCUS_RING - FOCUS_OFFSET) as usize;
            pixels[top * width as usize..].to_vec()
        };
        assert_eq!(band(&plain), band(&hostile));
        assert!(hostile.logical_size().1 > plain.logical_size().1);
    }

    #[test]
    fn answers_need_the_input_delay_and_a_press_and_release_on_one_button() {
        let shown = Instant::now();
        let early = shown + PROMPT_INPUT_DELAY / 2;
        let ready = shown + PROMPT_INPUT_DELAY;
        let mut layer = PromptLayer::new(&prompt(None, &[]), shown);
        let (width, height) = layer.logical_size();
        // Twice the logical size: a 2x display.
        let (wide, tall) = (width * 2, height * 2);
        assert_eq!(layer.focused(), PromptAnswer::Deny);
        assert_eq!(layer.key(PromptKey::Activate, early), None);
        assert_eq!(layer.key(PromptKey::Cancel, early), None);
        let center = |answer: PromptAnswer, layer: &PromptLayer| {
            let rect = layer
                .buttons
                .iter()
                .find(|button| button.answer == answer)
                .unwrap()
                .rect;
            (
                ((rect.x + rect.width / 2) * 2) as f32,
                ((rect.y + rect.height / 2) * 2) as f32,
            )
        };
        let (ax, ay) = center(PromptAnswer::Allow, &layer);
        // A press before the delay does not count, even if released after.
        layer.press(ax, ay, wide, tall, early);
        assert_eq!(layer.release(ax, ay, wide, tall, ready), None);
        // A press on Allow released on Deny answers nothing.
        let (dx, dy) = center(PromptAnswer::Deny, &layer);
        layer.press(ax, ay, wide, tall, ready);
        assert_eq!(layer.release(dx, dy, wide, tall, ready), None);
        layer.press(ax, ay, wide, tall, ready);
        assert_eq!(
            layer.release(ax, ay, wide, tall, ready),
            Some(PromptAnswer::Allow)
        );
        // Keys: focus wraps both ways; Escape dismisses.
        let mut layer = PromptLayer::new(&prompt(None, &[]), shown);
        assert_eq!(layer.key(PromptKey::Previous, early), None);
        assert_eq!(layer.focused(), PromptAnswer::Allow);
        assert_eq!(layer.key(PromptKey::Next, early), None);
        assert_eq!(layer.focused(), PromptAnswer::Deny);
        assert_eq!(layer.key(PromptKey::Next, early), None);
        assert_eq!(
            layer.key(PromptKey::Activate, ready),
            Some(PromptAnswer::Dismiss)
        );
        assert_eq!(
            layer.key(PromptKey::Cancel, ready),
            Some(PromptAnswer::Dismiss)
        );
        // Out-of-surface and non-finite positions hit nothing.
        assert_eq!(layer.button_at(-1.0, 0.0, wide, tall), None);
        assert_eq!(layer.button_at(f32::NAN, 0.0, wide, tall), None);
    }

    #[test]
    fn rendering_scales_by_whole_factors_and_draws_the_focus_ring() {
        let layer = PromptLayer::new(&prompt(Some("Why"), &["/data/app.kv"]), Instant::now());
        let (width, height) = layer.logical_size();
        let one = layer.render(width, height);
        let two = layer.render(width * 2, height * 2);
        assert_eq!(one.len(), (width * height) as usize);
        // Each logical pixel becomes a 2x2 block.
        for (y, x) in [
            (0, 0),
            (BAND_HEIGHT + PADDING + 3, PADDING + 1),
            (height - 1, width - 1),
        ] {
            let logical = one[(y * width + x) as usize];
            for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                assert_eq!(
                    two[((2 * y + dy) * 2 * width + 2 * x + dx) as usize],
                    logical
                );
            }
        }
        // A fractional surface keeps factor 1 and leaves background.
        let extra = layer.render(width + width / 2, height + height / 2);
        assert_eq!(extra[extra.len() - 1], BACKGROUND);
        let deny = layer.buttons[0].rect;
        let ring = (deny.y - FOCUS_OFFSET - 1) * width + deny.x;
        assert_eq!(one[ring as usize], FOCUS);
        assert!(one.contains(&TEXT) && one.contains(&QUOTE_BAR_COLOR));
    }
}
