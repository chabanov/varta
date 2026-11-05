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
//!
//! Every requested pattern is shown in full, never cut or left out: the
//! resource lines are paged, [`RESOURCE_PAGE_LINES`] per page, and the
//! first line of each pattern is marked in the margin. While a page has
//! not been shown, Allow is drawn disabled and ignores presses and keys; it
//! answers only once every page has been on screen and
//! [`PROMPT_INPUT_DELAY`] has passed since the last unseen page appeared.
//! A request whose resources need more than [`MAX_RESOURCE_LINES`] lines is
//! never shown ([`PromptTooLong`]); the consent seam answers it Deny and
//! audits why.
use super::glyphs::{self, CAP_ROWS, COLUMNS, ROWS};
use std::time::{Duration, Instant};
use varta_common::permission::{PermissionPrompt, PromptAnswer};

/// How long after the layer is shown it ignores activation, so a click or
/// key press meant for the application (for example a double click whose
/// first click opened the prompt) cannot answer it. Allow also waits this
/// long after the last unseen resource page appears. Owner decision
/// (2026-11-05): 500 ms.
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
/// The square in the left padding that marks the first line of each
/// requested pattern (a continued pattern's lines have none), and its
/// distance from the text's left edge.
const MARKER_SIZE: u32 = 6;
const MARKER_INSET: u32 = 12;
/// Buttons: height, horizontal padding, gap and focus ring.
const BUTTON_HEIGHT: u32 = 40;
const BUTTON_PADDING: u32 = 16;
const BUTTON_GAP: u32 = 12;
const BUTTON_BORDER: u32 = 1;
const FOCUS_RING: u32 = 3;
const FOCUS_OFFSET: u32 = 3;
/// Lines of requested resources on one page.
pub const RESOURCE_PAGE_LINES: usize = 8;
/// The most resource pages one prompt shows.
pub const MAX_RESOURCE_PAGES: usize = 16;
/// The most resource lines one prompt shows; a request that needs more is
/// never shown ([`PromptTooLong`]). One pattern of
/// `varta_common::MAX_PATTERN_BYTES` always fits, whatever its characters.
pub const MAX_RESOURCE_LINES: usize = RESOURCE_PAGE_LINES * MAX_RESOURCE_PAGES;
/// Shown lines of the quoted purpose (advisory package text, never what is
/// approved); a longer quote is cut with a visible [`CUT`] marker.
pub const MAX_QUOTE_LINES: usize = 8;

/// Colors (`0RGB`, the presentation buffer's format).
const BACKGROUND: u32 = 0x00ff_ffff;
const BAND: u32 = 0x0026_5dcc;
const TEXT: u32 = 0x0011_1111;
const LABEL: u32 = 0x0055_5555;
const CODE_BACKGROUND: u32 = 0x00f0_f0f0;
const QUOTE_BAR_COLOR: u32 = 0x00bb_bbbb;
const MARKER: u32 = 0x0026_5dcc;
const BUTTON_FILL: u32 = 0x00ee_eeee;
const BUTTON_EDGE: u32 = 0x0077_7777;
const DISABLED_FILL: u32 = 0x00f8_f8f8;
const DISABLED_EDGE: u32 = 0x00cc_cccc;
const DISABLED_TEXT: u32 = 0x00aa_aaaa;
const FOCUS: u32 = 0x0026_5dcc;

/// Host-owned text.
const HEADING: &str = "Permission request";
const REQUIRED: &str = "This permission is required to start the application.";
const OPTIONAL: &str = "The application requests this permission.";
const PERMISSION_LABEL: &str = "Permission:";
const APPLICATION_LABEL: &str = "Application identity:";
const RESOURCES_LABEL: &str = "Requested resources:";
const PAGED_NOTE: &str =
    "The list has several pages. Allow is available once every page was shown.";
const PAGE_LABEL: &str = "Page";
const PAGE_OF: &str = "of";
const PURPOSE_LABEL: &str = "The application says:";
const SCOPE_NOTE: &str = "Your choice applies only within the host's allowed scope.";
/// The marker of a quote line cut at [`MAX_QUOTE_LINES`].
const CUT: &str = "...";

/// The answer buttons, left to right, with their labels. Deny is first and
/// holds the initial focus.
const BUTTONS: [(PromptAnswer, &str); 3] = [
    (PromptAnswer::Deny, "Deny"),
    (PromptAnswer::Dismiss, "Dismiss"),
    (PromptAnswer::Allow, "Allow"),
];
/// The page button of a paged layer, at the left end of the button row.
const NEXT_PAGE_LABEL: &str = "Next page";

/// A key the layer handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKey {
    /// Tab, arrow right: the next usable button.
    Next,
    /// Shift+Tab, arrow left: the previous usable button.
    Previous,
    /// Enter or Space: the focused button.
    Activate,
    /// Escape: dismiss.
    Cancel,
    /// Page Down: the next resource page.
    NextPage,
    /// Page Up: the previous resource page.
    PreviousPage,
}

/// A request whose resources cannot be shown in full within
/// [`MAX_RESOURCE_LINES`]; it is never shown.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "the {capability} permission request needs {lines} lines of requested resources; a prompt shows at most {MAX_RESOURCE_LINES}, so it is not shown and is denied"
)]
pub struct PromptTooLong {
    pub capability: String,
    pub lines: usize,
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
    /// Whether a requested pattern starts on this line.
    marked: bool,
}

/// One line of a requested pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResourceLine {
    text: String,
    /// Whether the pattern starts on this line.
    first: bool,
}

/// What a button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Answer(PromptAnswer),
    NextPage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Button {
    action: Action,
    label: &'static str,
    rect: Rect,
}

/// One open permission prompt.
#[derive(Debug, Clone)]
pub struct PromptLayer {
    /// The fixed lines; the page line and the shown resource page are
    /// placed at `pager` and `resource_top`.
    lines: Vec<Line>,
    resources: Vec<ResourceLine>,
    resource_top: u32,
    pager: Option<u32>,
    page: usize,
    /// Which resource pages have been shown.
    seen: Vec<bool>,
    /// When the last unseen page appeared (when the layer was shown, for a
    /// single page); `None` while a page is unseen.
    all_seen: Option<Instant>,
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
/// where every character matters). `text` is [`glyphs::visible`] output,
/// so it is ASCII and every cut falls between characters.
fn wrap_hard(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    text.as_bytes()
        .chunks(width)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect()
}

/// Every line of every requested pattern, in request order.
fn resource_lines(prompt: &PermissionPrompt) -> Vec<ResourceLine> {
    prompt
        .scope
        .iter()
        .flat_map(|pattern| {
            wrap_hard(&glyphs::visible(pattern), columns(Style::Code))
                .into_iter()
                .enumerate()
                .map(|(index, text)| ResourceLine {
                    text,
                    first: index == 0,
                })
        })
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
            marked: false,
        });
        self.y += line_height(style.pixel());
    }
    /// Space for `count` lines of `style` drawn later; their top.
    fn reserve(&mut self, count: usize, style: Style) -> u32 {
        let top = self.y;
        self.y += count as u32 * line_height(style.pixel());
        top
    }
    fn gap(&mut self) {
        self.y += SECTION_GAP;
    }
}

/// The button row at `y`: the answer buttons right-aligned and, when
/// `paged`, the page button at the left end.
fn button_row(y: u32, paged: bool) -> Vec<Button> {
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
                y,
                width,
                height: BUTTON_HEIGHT,
            };
            x -= BUTTON_GAP;
            Button {
                action: Action::Answer(*answer),
                label,
                rect,
            }
        })
        .collect();
    if paged {
        buttons.push(Button {
            action: Action::NextPage,
            label: NEXT_PAGE_LABEL,
            rect: Rect {
                x: PADDING,
                y,
                width: label_width(NEXT_PAGE_LABEL) + 2 * BUTTON_PADDING,
                height: BUTTON_HEIGHT,
            },
        });
    }
    buttons.reverse();
    buttons
}

impl PromptLayer {
    /// Whether `prompt` can be shown in full: the number of resource lines
    /// it needs, or why it is never shown.
    pub fn check(prompt: &PermissionPrompt) -> Result<usize, PromptTooLong> {
        let lines = resource_lines(prompt).len();
        if lines > MAX_RESOURCE_LINES {
            return Err(PromptTooLong {
                capability: prompt.capability.clone(),
                lines,
            });
        }
        Ok(lines)
    }

    /// The layer of `prompt`, shown at `shown`; refused when its requested
    /// resources cannot be shown in full ([`Self::check`]).
    pub fn new(prompt: &PermissionPrompt, shown: Instant) -> Result<Self, PromptTooLong> {
        Self::check(prompt)?;
        let resources = resource_lines(prompt);
        let pages = resources.len().div_ceil(RESOURCE_PAGE_LINES).max(1);
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
        let mut pager = None;
        let mut resource_top = text.y;
        if !resources.is_empty() {
            text.push(RESOURCES_LABEL.into(), Style::Label);
            if pages > 1 {
                for line in wrap_words(PAGED_NOTE, columns(Style::Body)) {
                    text.push(line, Style::Body);
                }
                pager = Some(text.reserve(1, Style::Body));
            }
            resource_top = text.reserve(resources.len().min(RESOURCE_PAGE_LINES), Style::Code);
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
        let buttons = button_row(text.y, pages > 1);
        let focus = buttons
            .iter()
            .position(|button| button.action == Action::Answer(PromptAnswer::Deny))
            .unwrap_or_default();
        let mut seen = vec![false; pages];
        seen[0] = true;
        Ok(Self {
            height: text.y + BUTTON_HEIGHT + PADDING,
            lines: text.lines,
            resources,
            resource_top,
            pager,
            page: 0,
            seen,
            all_seen: (pages == 1).then_some(shown),
            buttons,
            focus,
            pressed: None,
            shown,
        })
    }

    /// The layer's logical size.
    #[must_use]
    pub fn logical_size(&self) -> (u32, u32) {
        (LAYER_WIDTH, self.height)
    }

    /// The answer the focused button gives; `None` on the page button.
    #[must_use]
    pub fn focused(&self) -> Option<PromptAnswer> {
        match self.buttons[self.focus].action {
            Action::Answer(answer) => Some(answer),
            Action::NextPage => None,
        }
    }

    /// The shown resource page (from 0) and the number of pages.
    #[must_use]
    pub fn page(&self) -> (usize, usize) {
        (self.page, self.seen.len())
    }

    /// Whether activation is accepted at `now` ([`PROMPT_INPUT_DELAY`]).
    #[must_use]
    pub fn ready(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.shown) >= PROMPT_INPUT_DELAY
    }

    /// Whether Allow answers at `now`: every resource page has been shown,
    /// the last unseen one at least [`PROMPT_INPUT_DELAY`] ago.
    #[must_use]
    pub fn allow_ready(&self, now: Instant) -> bool {
        self.ready(now)
            && self
                .all_seen
                .is_some_and(|seen| now.saturating_duration_since(seen) >= PROMPT_INPUT_DELAY)
    }

    /// Whether the button at `index` can take focus and presses: Allow only
    /// once every page has been shown.
    fn enabled(&self, index: usize) -> bool {
        self.buttons[index].action != Action::Answer(PromptAnswer::Allow) || self.all_seen.is_some()
    }

    /// Show the next (or previous) resource page.
    fn turn(&mut self, forward: bool, now: Instant) {
        let pages = self.seen.len();
        self.page = if forward {
            (self.page + 1) % pages
        } else {
            (self.page + pages - 1) % pages
        };
        self.seen[self.page] = true;
        if self.all_seen.is_none() && self.seen.iter().all(|seen| *seen) {
            self.all_seen = Some(now);
        }
    }

    /// Activate the button at `index`: its answer, or a page turn.
    fn activate(&mut self, index: usize, now: Instant) -> Option<PromptAnswer> {
        if !self.ready(now) {
            return None;
        }
        match self.buttons[index].action {
            Action::NextPage => {
                self.turn(true, now);
                None
            }
            Action::Answer(PromptAnswer::Allow) => {
                self.allow_ready(now).then_some(PromptAnswer::Allow)
            }
            Action::Answer(answer) => Some(answer),
        }
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

    /// The answer of the answer button under physical (`x`, `y`), if any,
    /// whether or not it is usable yet.
    #[must_use]
    pub fn button_at(&self, x: f32, y: f32, width: u32, height: u32) -> Option<PromptAnswer> {
        self.button_index(x, y, width, height)
            .and_then(|index| match self.buttons[index].action {
                Action::Answer(answer) => Some(answer),
                Action::NextPage => None,
            })
    }

    /// A primary button press at physical (`x`, `y`): remembered only when
    /// the layer is ready and the press is on a usable button.
    pub fn press(&mut self, x: f32, y: f32, width: u32, height: u32, now: Instant) {
        self.pressed = self
            .ready(now)
            .then(|| self.button_index(x, y, width, height))
            .flatten()
            .filter(|index| self.enabled(*index));
        if let Some(index) = self.pressed {
            self.focus = index;
        }
    }

    /// The release of a press: the button acts when the press began and
    /// ends on it; the answer, when it gives one.
    pub fn release(
        &mut self,
        x: f32,
        y: f32,
        width: u32,
        height: u32,
        now: Instant,
    ) -> Option<PromptAnswer> {
        let pressed = self.pressed.take()?;
        if self.button_index(x, y, width, height) != Some(pressed) {
            return None;
        }
        self.activate(pressed, now)
    }

    /// Move the focus to the next (or previous) usable button.
    fn move_focus(&mut self, forward: bool) {
        let count = self.buttons.len();
        for _ in 0..count {
            self.focus = if forward {
                (self.focus + 1) % count
            } else {
                (self.focus + count - 1) % count
            };
            if self.enabled(self.focus) {
                return;
            }
        }
    }

    /// A key: focus moves at any time; an answer or a page turn needs
    /// [`Self::ready`].
    pub fn key(&mut self, key: PromptKey, now: Instant) -> Option<PromptAnswer> {
        match key {
            PromptKey::Next | PromptKey::Previous => {
                self.move_focus(key == PromptKey::Next);
                None
            }
            PromptKey::Activate => self.activate(self.focus, now),
            PromptKey::Cancel => self.ready(now).then_some(PromptAnswer::Dismiss),
            PromptKey::NextPage | PromptKey::PreviousPage => {
                if self.ready(now) && self.seen.len() > 1 {
                    self.turn(key == PromptKey::NextPage, now);
                }
                None
            }
        }
    }

    /// The lines on screen: the fixed lines, the page line and the shown
    /// resource page.
    fn shown_lines(&self) -> Vec<Line> {
        let mut lines = self.lines.clone();
        if let Some(y) = self.pager {
            lines.push(Line {
                text: format!(
                    "{PAGE_LABEL} {} {PAGE_OF} {}",
                    self.page + 1,
                    self.seen.len()
                ),
                style: Style::Body,
                y,
                marked: false,
            });
        }
        let page = self
            .resources
            .iter()
            .skip(self.page * RESOURCE_PAGE_LINES)
            .take(RESOURCE_PAGE_LINES);
        for (row, line) in (0u32..).zip(page) {
            lines.push(Line {
                text: line.text.clone(),
                style: Style::Code,
                y: self.resource_top + row * line_height(Style::Code.pixel()),
                marked: line.first,
            });
        }
        lines
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
        for line in &self.shown_lines() {
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
            if line.marked {
                canvas.fill(
                    Rect {
                        x: PADDING - MARKER_INSET,
                        y: line.y + (line_height(pixel) - MARKER_SIZE) / 2,
                        width: MARKER_SIZE,
                        height: MARKER_SIZE,
                    },
                    MARKER,
                );
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
            let (edge, fill, label) = if self.enabled(index) {
                (BUTTON_EDGE, BUTTON_FILL, TEXT)
            } else {
                (DISABLED_EDGE, DISABLED_FILL, DISABLED_TEXT)
            };
            canvas.fill(rect, edge);
            canvas.fill(
                Rect {
                    x: rect.x + BUTTON_BORDER,
                    y: rect.y + BUTTON_BORDER,
                    width: rect.width - 2 * BUTTON_BORDER,
                    height: rect.height - 2 * BUTTON_BORDER,
                },
                fill,
            );
            let text_height = CAP_ROWS as u32 * BODY_PIXEL;
            canvas.text(
                button.label,
                rect.x + BUTTON_PADDING,
                rect.y + (rect.height - text_height) / 2,
                BODY_PIXEL,
                label,
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
    use varta_common::{MAX_GRANT_PATTERNS, MAX_PATTERN_BYTES};

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

    fn layer(prompt: &PermissionPrompt, shown: Instant) -> PromptLayer {
        PromptLayer::new(prompt, shown).unwrap()
    }

    fn texts(layer: &PromptLayer, style: Style) -> Vec<String> {
        layer
            .shown_lines()
            .into_iter()
            .filter(|line| line.style == style)
            .map(|line| line.text)
            .collect()
    }

    /// Every requested pattern as the layer shows it, read page by page
    /// from each marked first line through its continued lines. Turns
    /// every page once (back to the first) at `now`.
    fn read_every_page(layer: &mut PromptLayer, now: Instant) -> Vec<String> {
        let mut patterns: Vec<String> = Vec::new();
        let (_, pages) = layer.page();
        for page in 0..pages {
            assert_eq!(layer.page().0, page);
            for line in layer.shown_lines() {
                if line.style != Style::Code || line.y < layer.resource_top {
                    continue;
                }
                if line.marked {
                    patterns.push(String::new());
                }
                patterns
                    .last_mut()
                    .expect("a page starts with a marked line or continues a pattern")
                    .push_str(&line.text);
            }
            layer.key(PromptKey::NextPage, now);
        }
        patterns
    }

    fn visible(scope: &[String]) -> Vec<String> {
        scope
            .iter()
            .map(|pattern| glyphs::visible(pattern))
            .collect()
    }

    /// The center of the button doing `action`, at factor 1.
    fn center(layer: &PromptLayer, action: Action) -> (f32, f32) {
        let rect = layer
            .buttons
            .iter()
            .find(|button| button.action == action)
            .unwrap()
            .rect;
        (
            (rect.x + rect.width / 2) as f32,
            (rect.y + rect.height / 2) as f32,
        )
    }

    /// Press and release on the button doing `action` at `now`.
    fn click(layer: &mut PromptLayer, action: Action, now: Instant) -> Option<PromptAnswer> {
        let (width, height) = layer.logical_size();
        let (x, y) = center(layer, action);
        layer.press(x, y, width, height, now);
        layer.release(x, y, width, height, now)
    }

    const ALLOW: Action = Action::Answer(PromptAnswer::Allow);

    #[test]
    fn the_broker_description_comes_first_and_the_purpose_only_as_a_quote() {
        let layer = layer(
            &prompt(Some("Connect to your workspace"), &["https://api.test/**"]),
            Instant::now(),
        );
        let order: Vec<_> = layer
            .shown_lines()
            .into_iter()
            .map(|line| (line.y, line.text))
            .collect::<std::collections::BTreeMap<_, _>>()
            .into_values()
            .collect();
        let position = |text: &str| order.iter().position(|line| line == text).unwrap();
        assert!(position("net.request") < position("Connect to your workspace"));
        assert!(position("https://api.test/**") < position("Connect to your workspace"));
        assert_eq!(texts(&layer, Style::Quote), ["Connect to your workspace"]);
        // The identity is shown whole, across lines.
        assert_eq!(
            texts(&layer, Style::Code)
                .iter()
                .filter(|line| !line.contains("net.") && !line.contains("https"))
                .cloned()
                .collect::<String>(),
            IDENTITY
        );
        // One page: no page button, no page line, Allow usable.
        assert_eq!(layer.page(), (0, 1));
        assert_eq!(layer.buttons.len(), BUTTONS.len());
        let required = PromptLayer::new(
            &PermissionPrompt {
                required: true,
                ..prompt(None, &[])
            },
            Instant::now(),
        )
        .unwrap();
        // Host sentences wrap at spaces and stay whole.
        assert!(
            texts(&required, Style::Body)
                .join(" ")
                .starts_with(REQUIRED)
        );
        assert!(texts(&required, Style::Quote).is_empty());
        assert!(!texts(&required, Style::Label).contains(&RESOURCES_LABEL.to_owned()));
    }

    #[test]
    fn untrusted_text_is_escaped_the_quote_cut_visibly_and_the_buttons_never_move() {
        let shown = Instant::now();
        let plain = layer(&prompt(Some("Sync"), &["https://a.test/"]), shown);
        // A purpose that imitates the host's buttons and a flood of
        // resources: the quote is cut with a visible marker, every resource
        // stays (on pages), and invisible and lookalike characters are
        // escaped.
        let spoof = "[ Allow ] ".repeat(40);
        let many: Vec<String> = (0..40)
            .map(|index| format!("https://h{index}.test/\u{200b}/**"))
            .collect();
        let many: Vec<&str> = many.iter().map(String::as_str).collect();
        let hostile = layer(&prompt(Some(&spoof), &many), shown);
        assert_eq!(texts(&hostile, Style::Quote).len(), MAX_QUOTE_LINES);
        assert!(texts(&hostile, Style::Quote).last().unwrap().ends_with(CUT));
        assert!(
            texts(&hostile, Style::Code)
                .iter()
                .any(|line| line.contains("\\u{200b}"))
        );
        assert!(
            hostile
                .shown_lines()
                .iter()
                .all(|line| line.text.chars().all(glyphs::drawable))
        );
        assert!(hostile.page().1 > 1);
        // Every hit target of the whole surface is a host button, in both
        // layers; a quote that reads like a button is not.
        for layer in [&plain, &hostile] {
            let (width, height) = layer.logical_size();
            let mut targets = std::collections::BTreeSet::new();
            for y in 0..height {
                for x in 0..width {
                    if let Some(index) = layer.button_index(x as f32, y as f32, width, height) {
                        assert!(layer.buttons[index].rect.contains(x, y));
                        targets.insert(index);
                    }
                }
            }
            assert_eq!(targets.len(), layer.buttons.len());
            let text_bottom = layer
                .shown_lines()
                .iter()
                .map(|line| line.y + line_height(line.style.pixel()))
                .max()
                .unwrap();
            assert!(layer.buttons.iter().all(|b| b.rect.y >= text_bottom));
        }
        // The answer buttons sit at the same place relative to the bottom
        // edge whatever the package text is.
        let answers = |layer: &PromptLayer| -> Vec<(u32, u32, u32)> {
            let (_, height) = layer.logical_size();
            layer
                .buttons
                .iter()
                .filter(|button| matches!(button.action, Action::Answer(_)))
                .map(|button| (button.rect.x, height - button.rect.y, button.rect.width))
                .collect()
        };
        assert_eq!(answers(&plain), answers(&hostile));
        assert!(hostile.logical_size().1 > plain.logical_size().1);
    }

    #[test]
    fn every_requested_pattern_is_shown_in_full_across_pages() {
        let shown = Instant::now();
        let ready = shown + PROMPT_INPUT_DELAY;
        // As many patterns as one grant may hold, each a different length.
        let scope: Vec<String> = (0..MAX_GRANT_PATTERNS)
            .map(|index| format!("/data/{}\u{200b}{index}", "x".repeat(index)))
            .collect();
        let request = PermissionPrompt {
            scope: scope.clone(),
            ..prompt(None, &[])
        };
        let mut paged = layer(&request, shown);
        let (_, pages) = paged.page();
        assert!(pages > 1);
        assert!(texts(&paged, Style::Body).contains(&format!("{PAGE_LABEL} 1 {PAGE_OF} {pages}")));
        // Every page holds at most a page of resource lines, and the layer's
        // size never changes from page to page.
        let size = paged.logical_size();
        assert_eq!(read_every_page(&mut paged, ready), visible(&scope));
        assert_eq!(paged.logical_size(), size);
        // Going back from the first page shows the last one, not the pages
        // between: Allow stays unusable until each page was shown.
        let mut skipped = layer(&request, shown);
        assert_eq!(skipped.key(PromptKey::PreviousPage, ready), None);
        assert_eq!(skipped.page(), (pages - 1, pages));
        assert!(!skipped.allow_ready(ready + PROMPT_INPUT_DELAY));
        assert_eq!(click(&mut skipped, ALLOW, ready + PROMPT_INPUT_DELAY), None);
        // Deny and Dismiss work on any page.
        assert_eq!(
            click(&mut skipped, Action::Answer(PromptAnswer::Deny), ready),
            Some(PromptAnswer::Deny)
        );
        assert_eq!(
            skipped.key(PromptKey::Cancel, ready),
            Some(PromptAnswer::Dismiss)
        );
    }

    #[test]
    fn a_512_byte_pattern_is_shown_in_full_whatever_its_characters() {
        let shown = Instant::now();
        let ready = shown + PROMPT_INPUT_DELAY;
        let ascii = format!("/{}", "a".repeat(MAX_PATTERN_BYTES - 1));
        // Two-byte letters, each shown as a seven-character escape.
        let cyrillic = "\u{430}".repeat(MAX_PATTERN_BYTES / "\u{430}".len());
        // The widest escape per byte: a one-byte control character.
        let control = "\u{7f}".repeat(MAX_PATTERN_BYTES);
        for pattern in [ascii, cyrillic, control] {
            assert_eq!(pattern.len(), MAX_PATTERN_BYTES);
            let request = prompt(None, &[pattern.as_str(), "/after"]);
            let mut layer = layer(&request, shown);
            assert_eq!(
                read_every_page(&mut layer, ready),
                visible(&request.scope),
                "{pattern:?}"
            );
        }
    }

    #[test]
    fn a_long_non_ascii_url_needs_every_page_before_allow_answers() {
        let shown = Instant::now();
        let ready = shown + PROMPT_INPUT_DELAY;
        let url = format!(
            "https://\u{43f}\u{440}\u{438}\u{43c}\u{435}\u{440}.test/{}?q=\u{1F600}",
            "\u{441}\u{43b}\u{43e}\u{432}\u{43e}/".repeat(40)
        );
        let request = PermissionPrompt {
            capability: varta_common::view::OPEN_URL_CAPABILITY.into(),
            ..prompt(None, &[url.as_str()])
        };
        let mut layer = layer(&request, shown);
        let (_, pages) = layer.page();
        assert!(pages > 2, "{pages}");
        // The page button exists; Allow is drawn disabled, ignores presses
        // and keys, and cannot take the focus.
        assert_eq!(layer.focused(), Some(PromptAnswer::Deny));
        let (width, height) = layer.logical_size();
        // A point inside Allow's border, left of its label.
        let allow = layer.buttons[layer.buttons.len() - 1].rect;
        assert_eq!(allow.x, center(&layer, ALLOW).0 as u32 - allow.width / 2);
        let fill = ((allow.y + BUTTON_HEIGHT / 2) * width + allow.x + BUTTON_BORDER + 1) as usize;
        assert_eq!(layer.render(width, height)[fill], DISABLED_FILL);
        assert_eq!(click(&mut layer, ALLOW, ready), None);
        for _ in 0..layer.buttons.len() {
            layer.key(PromptKey::Next, ready);
            assert_ne!(layer.focused(), Some(PromptAnswer::Allow));
        }
        // Click through every page with the page button.
        let mut url_text = String::new();
        for page in 0..pages {
            assert_eq!(layer.page(), (page, pages));
            for line in layer.shown_lines() {
                if line.style == Style::Code && line.y >= layer.resource_top {
                    assert_eq!(line.marked, page == 0 && url_text.is_empty());
                    url_text.push_str(&line.text);
                }
            }
            if page + 1 < pages {
                assert_eq!(click(&mut layer, Action::NextPage, ready), None);
                assert_eq!(click(&mut layer, ALLOW, ready), None);
            }
        }
        assert_eq!(url_text, glyphs::visible(&url));
        // The last page has just appeared: Allow is enabled but answers only
        // after the input delay.
        assert!(layer.enabled(layer.buttons.len() - 1));
        assert_eq!(click(&mut layer, ALLOW, ready), None);
        assert_eq!(
            click(&mut layer, ALLOW, ready + PROMPT_INPUT_DELAY),
            Some(PromptAnswer::Allow)
        );
        assert_eq!(layer.render(width, height)[fill], BUTTON_FILL);
    }

    #[test]
    fn a_request_that_cannot_be_shown_in_full_is_refused() {
        // Every pattern at the longest length, as many as one grant holds.
        let long = format!("/{}", "a".repeat(MAX_PATTERN_BYTES - 1));
        let scope = vec![long; MAX_GRANT_PATTERNS];
        let request = PermissionPrompt {
            scope,
            ..prompt(None, &[])
        };
        let lines = MAX_GRANT_PATTERNS * MAX_PATTERN_BYTES.div_ceil(columns(Style::Code));
        assert!(lines > MAX_RESOURCE_LINES);
        let refused = PromptTooLong {
            capability: request.capability.clone(),
            lines,
        };
        assert_eq!(PromptLayer::check(&request), Err(refused.clone()));
        assert_eq!(
            PromptLayer::new(&request, Instant::now()).map(|_| ()),
            Err(refused)
        );
        // A guest-chosen link at the network URL limit.
        let url = format!(
            "https://a.test/{}",
            "p".repeat(varta_common::net::MAX_URL_BYTES)
        );
        assert!(PromptLayer::check(&prompt(None, &[url.as_str()])).is_err());
        // The largest request that fits is shown.
        let fits = vec!["a".repeat(columns(Style::Code)); MAX_RESOURCE_LINES];
        assert_eq!(
            PromptLayer::check(&PermissionPrompt {
                scope: fits,
                ..prompt(None, &[])
            }),
            Ok(MAX_RESOURCE_LINES)
        );
    }

    #[test]
    fn answers_need_the_input_delay_and_a_press_and_release_on_one_button() {
        let shown = Instant::now();
        let early = shown + PROMPT_INPUT_DELAY / 2;
        let ready = shown + PROMPT_INPUT_DELAY;
        let mut layer = layer(&prompt(None, &[]), shown);
        let (width, height) = layer.logical_size();
        // Twice the logical size: a 2x display.
        let (wide, tall) = (width * 2, height * 2);
        assert_eq!(layer.focused(), Some(PromptAnswer::Deny));
        assert_eq!(layer.key(PromptKey::Activate, early), None);
        assert_eq!(layer.key(PromptKey::Cancel, early), None);
        let doubled = |answer: PromptAnswer, layer: &PromptLayer| {
            let (x, y) = center(layer, Action::Answer(answer));
            (x * 2.0, y * 2.0)
        };
        let (ax, ay) = doubled(PromptAnswer::Allow, &layer);
        // A press before the delay does not count, even if released after.
        layer.press(ax, ay, wide, tall, early);
        assert_eq!(layer.release(ax, ay, wide, tall, ready), None);
        // A press on Allow released on Deny answers nothing.
        let (dx, dy) = doubled(PromptAnswer::Deny, &layer);
        layer.press(ax, ay, wide, tall, ready);
        assert_eq!(layer.release(dx, dy, wide, tall, ready), None);
        layer.press(ax, ay, wide, tall, ready);
        assert_eq!(
            layer.release(ax, ay, wide, tall, ready),
            Some(PromptAnswer::Allow)
        );
        // Keys: focus wraps both ways; Escape dismisses; page keys do
        // nothing on a single page.
        let mut layer = PromptLayer::new(&prompt(None, &[]), shown).unwrap();
        assert_eq!(layer.key(PromptKey::Previous, early), None);
        assert_eq!(layer.focused(), Some(PromptAnswer::Allow));
        assert_eq!(layer.key(PromptKey::Next, early), None);
        assert_eq!(layer.focused(), Some(PromptAnswer::Deny));
        assert_eq!(layer.key(PromptKey::NextPage, ready), None);
        assert_eq!(layer.page(), (0, 1));
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
    fn rendering_scales_by_whole_factors_and_draws_the_focus_ring_and_markers() {
        let layer = layer(&prompt(Some("Why"), &["/data/app.kv"]), Instant::now());
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
        // The pattern's first line carries the host marker in the margin.
        let marker = (layer.resource_top + line_height(BODY_PIXEL) / 2) * width + PADDING
            - MARKER_INSET
            + MARKER_SIZE / 2;
        assert_eq!(one[marker as usize], MARKER);
    }
}
