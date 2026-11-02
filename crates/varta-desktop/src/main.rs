#![forbid(unsafe_code)]
//! Experimental native preview for an already compiled, unprivileged view module.
use std::collections::VecDeque;
use std::io::Read;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
use std::time::Duration;
use varta_runtime::{ApplicationOptions, Step, VartaRuntime};
use varta_view::{Input, RasterImage, Target, TextMeasurer};
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{Key as WinitKey, KeyCode, ModifiersState, NamedKey, PhysicalKey},
    window::{Theme, Window, WindowId},
};

const MAX_FONT_BYTES: u64 = 4 * 1024 * 1024;

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.is_empty() || bytes.len() as u64 > limit {
        return Err(format!("{}: invalid file size", path.display()));
    }
    Ok(bytes)
}

enum GuiEvent {
    Frame(PreviewFrame),
    Error(String),
}
struct PreviewFrame {
    image: RasterImage,
    revision: u64,
    dimensions: u64,
    sequence: u64,
    commits: usize,
    dark: bool,
    title: String,
    scale: u64,
}
enum WorkerMessage {
    Input {
        input: Input,
        revision: u64,
        dimensions: u64,
        dark: bool,
        scale: u64,
    },
}
enum PendingKey {
    Key(&'static str, ModifiersState),
    Character {
        key: String,
        text: String,
        modifiers: ModifiersState,
        select_all: bool,
        history: Option<varta_view::HistoryAction>,
    },
    ImePreedit {
        node: u32,
        commits: usize,
        text: String,
        cursor: Option<(usize, usize)>,
    },
    ImeCommit {
        node: u32,
        commits: usize,
        text: String,
    },
}

fn valid_ime_text(text: &str) -> bool {
    let max_units = varta_common::view::DRAFT_MAX_VALUE_BYTES / 2;
    text.len() <= max_units * 3
        && !text.chars().any(char::is_control)
        && text.encode_utf16().take(max_units + 1).count() <= max_units
}

fn translate_ime(
    ime: Ime,
    focused: bool,
    binding: (u32, usize),
    composing: &mut bool,
) -> Option<PendingKey> {
    let (node, commits) = binding;
    let cancel = || PendingKey::ImePreedit {
        node,
        commits,
        text: String::new(),
        cursor: None,
    };
    match ime {
        Ime::Enabled => None,
        Ime::Disabled => {
            *composing = false;
            Some(cancel())
        }
        Ime::Preedit(_, _) | Ime::Commit(_) if !focused => None,
        Ime::Preedit(text, cursor) => {
            if !valid_ime_text(&text)
                || cursor.is_some_and(|(start, end)| {
                    start > end
                        || end > text.len()
                        || !text.is_char_boundary(start)
                        || !text.is_char_boundary(end)
                })
            {
                *composing = false;
                return Some(cancel());
            }
            *composing = !text.is_empty();
            Some(PendingKey::ImePreedit {
                node,
                commits,
                text,
                cursor,
            })
        }
        Ime::Commit(text) => {
            *composing = false;
            if !valid_ime_text(&text) {
                return Some(cancel());
            }
            Some(PendingKey::ImeCommit {
                node,
                commits,
                text,
            })
        }
    }
}

struct WorkerState {
    dimensions: Arc<AtomicU64>,
    dark: Arc<AtomicBool>,
    scale: Arc<AtomicU64>,
    presented: Arc<AtomicU64>,
}

fn worker(
    wasm: Vec<u8>,
    font: Option<(Vec<u8>, String)>,
    state: WorkerState,
    messages: Receiver<WorkerMessage>,
    proxy: EventLoopProxy<GuiEvent>,
) {
    let WorkerState {
        dimensions,
        dark,
        scale,
        presented,
    } = state;
    let execute = || -> Result<(), String> {
        let initial_dimensions = loop {
            let current = dimensions.load(Ordering::Acquire);
            if current >> 32 != 0 && current as u32 != 0 {
                break current;
            }
            match messages.recv_timeout(Duration::from_millis(16)) {
                Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            }
        };
        let layout_font = font
            .as_ref()
            .map(|(data, family)| {
                TextMeasurer::new(data.clone(), family).map_err(|e| e.to_string())
            })
            .transpose()?;
        let mut font = font
            .map(|(data, family)| TextMeasurer::new(data, &family).map_err(|e| e.to_string()))
            .transpose()?;
        let runtime = VartaRuntime::new();
        let initial_dark = dark.load(Ordering::Acquire);
        let initial_scale = scale.load(Ordering::Acquire);
        let initial_factor = f64::from_bits(initial_scale) as f32;
        let mut app = runtime
            .start_application(
                &wasm,
                ApplicationOptions {
                    view_layout: Some(
                        varta_runtime::ViewLayout::new(
                            (initial_dimensions >> 32) as u32 as f32 / initial_factor,
                            initial_dimensions as u32 as f32 / initial_factor,
                            layout_font,
                            16.0,
                        )
                        .map_err(|e| e.to_string())?
                        .with_dark_mode(initial_dark),
                    ),
                    ..ApplicationOptions::default()
                },
            )
            .map_err(|e| e.to_string())?;
        if let Some(error) = app.failure() {
            return Err(error.to_string());
        }
        let mut revision = 0u64;
        let mut known_commits = app.view_commits();
        let mut rendered_revision = u64::MAX;
        let mut rendered_dimensions = 0;
        let mut in_flight = None;
        let mut sequence = 0u64;
        let mut environment_dimensions = initial_dimensions;
        let mut environment_dark = initial_dark;
        let mut environment_scale = initial_scale;
        loop {
            match messages.recv_timeout(Duration::from_millis(16)) {
                Ok(WorkerMessage::Input {
                    input,
                    revision: input_revision,
                    dimensions: event_dimensions,
                    dark: event_dark,
                    scale: event_scale,
                }) => {
                    // A queued OS event must not target a different tree or viewport.
                    if revision == input_revision
                        && dimensions.load(Ordering::Acquire) == event_dimensions
                        && dark.load(Ordering::Acquire) == event_dark
                        && scale.load(Ordering::Acquire) == event_scale
                    {
                        app.view_input(&input).map_err(|e| e.to_string())?;
                        revision = revision.checked_add(1).ok_or("view revision exhausted")?;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            }
            if in_flight.is_some_and(|id| presented.load(Ordering::Acquire) >= id) {
                in_flight = None;
            }
            let current_dimensions = dimensions.load(Ordering::Acquire);
            let viewport_width = (current_dimensions >> 32) as u32;
            let viewport_height = current_dimensions as u32;
            let current_dark = dark.load(Ordering::Acquire);
            let current_scale = scale.load(Ordering::Acquire);
            let factor = f64::from_bits(current_scale) as f32;
            if (current_dimensions != environment_dimensions || current_scale != environment_scale)
                && viewport_width > 0
                && viewport_height > 0
            {
                app.view_set_layout_size(
                    viewport_width as f32 / factor,
                    viewport_height as f32 / factor,
                )
                .map_err(|e| e.to_string())?;
                environment_dimensions = current_dimensions;
                environment_scale = current_scale;
                revision = revision.checked_add(1).ok_or("view revision exhausted")?;
            }
            if current_dark != environment_dark {
                app.view_set_environment(varta_view::Environment {
                    width: f64::from((environment_dimensions >> 32) as u32)
                        / f64::from_bits(environment_scale),
                    dark: current_dark,
                })
                .map_err(|e| e.to_string())?;
                environment_dark = current_dark;
                revision = revision.checked_add(1).ok_or("view revision exhausted")?;
            }
            for _ in 0..16 {
                match app.step(Some(Duration::ZERO)) {
                    Step::Turn(_) => {}
                    Step::Failed => {
                        return Err(app
                            .failure()
                            .map_or_else(|| "application failed".to_owned(), ToString::to_string));
                    }
                    Step::Idle | Step::Finished { .. } => break,
                }
            }
            if app.view_commits() != known_commits {
                known_commits = app.view_commits();
                app.view_cancel_preedit();
                revision = revision.checked_add(1).ok_or("view revision exhausted")?;
            }
            for line in app.take_output() {
                eprintln!("{line}");
            }
            let dimensions = dimensions.load(Ordering::Acquire);
            let width = (dimensions >> 32) as u32;
            let height = dimensions as u32;
            if in_flight.is_none()
                && width > 0
                && height > 0
                && app.view_commits() > 0
                && (revision != rendered_revision || dimensions != rendered_dimensions)
            {
                let frame = app
                    .view_rasterize_scaled(
                        width,
                        height,
                        f64::from_bits(environment_scale) as f32,
                        font.as_mut().map(|font| (font, 16.0)),
                    )
                    .map_err(|e| e.to_string())?;
                rendered_revision = revision;
                rendered_dimensions = dimensions;
                sequence = sequence.checked_add(1).ok_or("frame sequence exhausted")?;
                if proxy
                    .send_event(GuiEvent::Frame(PreviewFrame {
                        image: frame,
                        revision: rendered_revision,
                        dimensions,
                        sequence,
                        commits: known_commits,
                        dark: environment_dark,
                        title: app.view_title().into(),
                        scale: environment_scale,
                    }))
                    .is_err()
                {
                    return Ok(());
                }
                in_flight = Some(sequence);
            }
        }
    };
    if let Err(error) = execute() {
        let _ = proxy.send_event(GuiEvent::Error(error));
    }
}

struct Preview {
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    dimensions: Arc<AtomicU64>,
    dark: Arc<AtomicBool>,
    scale: Arc<AtomicU64>,
    sender: SyncSender<WorkerMessage>,
    frame: Option<PreviewFrame>,
    error: Option<String>,
    presented: bool,
    frame_presented: bool,
    frame_ack: Arc<AtomicU64>,
    cursor: Option<(f32, f32)>,
    pressed: Option<Pressed>,
    pending_scroll: f32,
    scroll_inflight: Option<(u64, u64)>,
    modifiers: ModifiersState,
    pending_keys: VecDeque<PendingKey>,
    key_inflight: Option<(u64, u64)>,
    window_focused: bool,
    ime_binding: Option<(u32, usize)>,
    ime_composing: bool,
    title: String,
}
struct Pressed {
    node: u32,
    revision: u64,
    dimensions: u64,
    dark: bool,
    scale: u64,
    text_anchor: Option<usize>,
}
impl Preview {
    fn size_changed(&self, window: &Window) {
        let size = window.inner_size();
        self.dimensions.store(
            (u64::from(size.width) << 32) | u64::from(size.height),
            Ordering::Release,
        );
    }
    fn draw(&mut self) -> Result<(), String> {
        let (Some(window), Some(surface), Some(frame)) =
            (&self.window, &mut self.surface, &self.frame)
        else {
            return Ok(());
        };
        let size = window.inner_size();
        if size.width != frame.image.width
            || size.height != frame.image.height
            || frame.dark != self.dark.load(Ordering::Acquire)
            || frame.scale != self.scale.load(Ordering::Acquire)
        {
            self.frame_presented = false;
            self.frame_ack.store(frame.sequence, Ordering::Release);
            return Ok(());
        }
        let Some(width) = NonZeroU32::new(size.width) else {
            return Ok(());
        };
        let Some(height) = NonZeroU32::new(size.height) else {
            return Ok(());
        };
        surface.resize(width, height).map_err(|e| e.to_string())?;
        let mut buffer = surface.buffer_mut().map_err(|e| e.to_string())?;
        for (pixel, rgba) in buffer.iter_mut().zip(frame.image.pixels.as_chunks::<4>().0) {
            let white = 255u32 - u32::from(rgba[3]);
            *pixel = ((u32::from(rgba[0]) + white).min(255) << 16)
                | ((u32::from(rgba[1]) + white).min(255) << 8)
                | (u32::from(rgba[2]) + white).min(255);
        }
        buffer.present().map_err(|e| e.to_string())?;
        let title = if frame.title.is_empty() {
            "Varta preview"
        } else {
            &frame.title
        };
        if self.title != title {
            window.set_title(title);
            self.title = title.into();
        }
        self.frame_presented = true;
        self.frame_ack.store(frame.sequence, Ordering::Release);
        let binding = frame.image.editable().map(|node| (node, frame.commits));
        if self.ime_binding != binding {
            if self.ime_binding.is_some() {
                window.set_ime_allowed(false);
            }
            self.ime_binding = binding;
            self.ime_composing = false;
            self.pending_keys.retain(|pending| match pending {
                PendingKey::ImePreedit { node, commits, .. }
                | PendingKey::ImeCommit { node, commits, .. } => Some((*node, *commits)) == binding,
                _ => true,
            });
        }
        window.set_ime_allowed(self.window_focused && binding.is_some());
        if let Some(caret) = frame.image.ime_cursor_area() {
            window.set_ime_cursor_area(
                PhysicalPosition::new(caret.x as f64, caret.y as f64),
                PhysicalSize::new(caret.width.max(1.0) as f64, caret.height.max(1.0) as f64),
            );
        }
        if !self.presented {
            self.presented = true;
            eprintln!("varta-preview: first frame presented");
        }
        if self.scroll_inflight.is_some_and(|(revision, dimensions)| {
            revision != frame.revision || dimensions != frame.dimensions
        }) {
            self.scroll_inflight = None;
        }
        if self.key_inflight.is_some_and(|(revision, dimensions)| {
            revision != frame.revision || dimensions != frame.dimensions
        }) {
            self.key_inflight = None;
        }
        self.flush_keys()?;
        self.flush_scroll()
    }
    fn hit(&self) -> Option<(u32, u64, u64)> {
        let (x, y) = self.cursor?;
        let frame = self.frame.as_ref().filter(|_| self.frame_presented)?;
        Some((
            frame.image.hit_test(x, y)?,
            frame.revision,
            frame.dimensions,
        ))
    }
    fn flush_scroll(&mut self) -> Result<(), String> {
        if self.scroll_inflight.is_some()
            || self.key_inflight.is_some()
            || !self.pending_keys.is_empty()
            || !self.frame_presented
            || self.pending_scroll == 0.0
        {
            return Ok(());
        }
        let Some((x, y)) = self.cursor else {
            self.pending_scroll = 0.0;
            return Ok(());
        };
        let Some(frame) = &self.frame else {
            return Ok(());
        };
        let delta = std::mem::take(&mut self.pending_scroll);
        if let Some((node, top, remaining)) = frame.image.scroll_target(x, y, delta) {
            self.sender
                .try_send(WorkerMessage::Input {
                    input: Input::Scroll {
                        target: Target::Node(node),
                        top,
                    },
                    revision: frame.revision,
                    dimensions: frame.dimensions,
                    dark: frame.dark,
                    scale: frame.scale,
                })
                .map_err(|_| "native input queue is full or the application stopped".to_owned())?;
            self.pending_scroll = remaining;
            self.scroll_inflight = Some((frame.revision, frame.dimensions));
        }
        Ok(())
    }
    fn flush_keys(&mut self) -> Result<(), String> {
        if self.key_inflight.is_some() || self.scroll_inflight.is_some() || !self.frame_presented {
            return Ok(());
        }
        let Some(frame) = &self.frame else {
            return Ok(());
        };
        while let Some(pending) = self.pending_keys.pop_front() {
            let input = match pending {
                PendingKey::Character {
                    key,
                    text,
                    modifiers,
                    select_all,
                    history,
                } => {
                    let Some(target) = frame.image.focused() else {
                        continue;
                    };
                    Input::TextKey {
                        target: Target::Node(target),
                        key,
                        text,
                        shift: modifiers.shift_key(),
                        ctrl: modifiers.control_key(),
                        alt: modifiers.alt_key(),
                        meta: modifiers.super_key(),
                        select_all,
                        history,
                    }
                }
                PendingKey::Key(key, modifiers) => {
                    let target = frame
                        .image
                        .focused()
                        .or_else(|| (key == "Tab").then(varta_view::View::root));
                    let Some(target) = target else {
                        continue;
                    };
                    Input::Key {
                        target: Target::Node(target),
                        key: key.into(),
                        shift: modifiers.shift_key(),
                        ctrl: modifiers.control_key(),
                        alt: modifiers.alt_key(),
                        meta: modifiers.super_key(),
                        composing: false,
                    }
                }
                PendingKey::ImePreedit {
                    node,
                    commits,
                    text,
                    cursor,
                } => {
                    if frame.image.editable() != Some(node) || frame.commits != commits {
                        continue;
                    }
                    Input::ImePreedit {
                        target: Target::Node(node),
                        text,
                        cursor,
                    }
                }
                PendingKey::ImeCommit {
                    node,
                    commits,
                    text,
                } => {
                    if frame.image.editable() != Some(node) || frame.commits != commits {
                        continue;
                    }
                    Input::ImeCommit {
                        target: Target::Node(node),
                        text,
                    }
                }
            };
            self.sender
                .try_send(WorkerMessage::Input {
                    input,
                    revision: frame.revision,
                    dimensions: frame.dimensions,
                    dark: frame.dark,
                    scale: frame.scale,
                })
                .map_err(|_| "native input queue is full or the application stopped".to_owned())?;
            self.key_inflight = Some((frame.revision, frame.dimensions));
            break;
        }
        Ok(())
    }
}

fn supported_key(key: &WinitKey) -> Option<&'static str> {
    let WinitKey::Named(key) = key else {
        return match key {
            WinitKey::Character(value) if value == " " => Some("Space"),
            _ => None,
        };
    };
    Some(match key {
        NamedKey::Enter => "Enter",
        NamedKey::Escape => "Escape",
        NamedKey::Tab => "Tab",
        NamedKey::Space => "Space",
        NamedKey::Backspace => "Backspace",
        NamedKey::Delete => "Delete",
        NamedKey::ArrowUp => "ArrowUp",
        NamedKey::ArrowDown => "ArrowDown",
        NamedKey::ArrowLeft => "ArrowLeft",
        NamedKey::ArrowRight => "ArrowRight",
        NamedKey::Home => "Home",
        NamedKey::End => "End",
        NamedKey::PageUp => "PageUp",
        NamedKey::PageDown => "PageDown",
        _ => return None,
    })
}
impl ApplicationHandler<GuiEvent> for Preview {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let result = (|| -> Result<(), String> {
            let window = Arc::new(
                event_loop
                    .create_window(Window::default_attributes().with_title("Varta preview"))
                    .map_err(|e| e.to_string())?,
            );
            let context = softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
            let surface =
                softbuffer::Surface::new(&context, window.clone()).map_err(|e| e.to_string())?;
            self.dark
                .store(window.theme() == Some(Theme::Dark), Ordering::Release);
            self.scale
                .store(window.scale_factor().to_bits(), Ordering::Release);
            self.size_changed(&window);
            self.surface = Some(surface);
            self.window = Some(window);
            Ok(())
        })();
        if let Err(error) = result {
            self.error = Some(error);
            event_loop.exit();
        }
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: GuiEvent) {
        match event {
            GuiEvent::Frame(frame) => {
                self.frame = Some(frame);
                self.frame_presented = false;
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            GuiEvent::Error(error) => {
                self.error = Some(error);
                event_loop.exit();
            }
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.window.as_ref().is_none_or(|window| window.id() != id) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale.store(scale_factor.to_bits(), Ordering::Release);
                self.frame_presented = false;
                self.pressed = None;
                self.pending_scroll = 0.0;
                if let Some(window) = &self.window {
                    self.size_changed(window);
                    window.request_redraw();
                }
            }
            WindowEvent::ThemeChanged(theme) => {
                self.dark.store(theme == Theme::Dark, Ordering::Release);
                self.frame_presented = false;
                self.pressed = None;
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::Resized(_) => {
                self.frame_presented = false;
                self.pressed = None;
                self.pending_scroll = 0.0;
                self.scroll_inflight = None;
                self.key_inflight = None;
                if let Some(window) = &self.window {
                    self.size_changed(window);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = Some((position.x as f32, position.y as f32));
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                self.pressed = None;
                self.pending_scroll = 0.0;
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::Focused(focused) => {
                self.window_focused = focused;
                if let Some(window) = &self.window {
                    window.set_ime_allowed(focused && self.ime_binding.is_some());
                }
                if !focused {
                    self.modifiers = ModifiersState::empty();
                    self.pending_keys.clear();
                    self.ime_composing = false;
                    // Some windowing backends do not report Ime::Disabled when
                    // focus moves away. Clear any displayed composition even then.
                    if let Some((node, commits)) = self.ime_binding {
                        self.pending_keys.push_back(PendingKey::ImePreedit {
                            node,
                            commits,
                            text: String::new(),
                            cursor: None,
                        });
                        if let Err(error) = self.flush_keys() {
                            self.error = Some(error);
                            event_loop.exit();
                        }
                    }
                }
            }
            WindowEvent::Ime(ime) => {
                if let Some((node, commits)) = self.ime_binding {
                    let pending = translate_ime(
                        ime,
                        self.window_focused,
                        (node, commits),
                        &mut self.ime_composing,
                    );
                    if let Some(pending) = pending {
                        if matches!((&pending, self.pending_keys.back()),
                            (PendingKey::ImePreedit { node: a, commits: b, .. },
                             Some(PendingKey::ImePreedit { node: c, commits: d, .. })) if a == c && b == d)
                        {
                            self.pending_keys.pop_back();
                        }
                        if self.pending_keys.len() >= 64 {
                            self.error = Some("native keyboard queue is full".into());
                            event_loop.exit();
                        } else {
                            self.pending_keys.push_back(pending);
                            if let Err(error) = self.flush_keys() {
                                self.error = Some(error);
                                event_loop.exit();
                            }
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } if event.state == ElementState::Pressed && !self.ime_composing => {
                let pending = if let Some(key) = supported_key(&event.logical_key) {
                    Some(PendingKey::Key(key, self.modifiers))
                } else if let WinitKey::Character(key) = &event.logical_key {
                    if key.is_empty() || key.len() > 4096 || key.chars().any(char::is_control) {
                        None
                    } else {
                        let text = event
                            .text
                            .as_ref()
                            .filter(|text| {
                                text.len() <= 4096 && !text.chars().any(char::is_control)
                            })
                            .map_or_else(String::new, ToString::to_string);
                        Some(PendingKey::Character {
                            key: key.to_string(),
                            text,
                            modifiers: self.modifiers,
                            select_all: event.physical_key == PhysicalKey::Code(KeyCode::KeyA),
                            history: match event.physical_key {
                                PhysicalKey::Code(KeyCode::KeyZ) => {
                                    Some(if self.modifiers.shift_key() {
                                        varta_view::HistoryAction::Redo
                                    } else {
                                        varta_view::HistoryAction::Undo
                                    })
                                }
                                PhysicalKey::Code(KeyCode::KeyY) => {
                                    Some(varta_view::HistoryAction::Redo)
                                }
                                _ => None,
                            },
                        })
                    }
                } else {
                    None
                };
                if let Some(pending) = pending {
                    if self.pending_keys.len() >= 64 {
                        self.error = Some("native keyboard queue is full".into());
                        event_loop.exit();
                    } else {
                        self.pending_keys.push_back(pending);
                        if let Err(error) = self.flush_keys() {
                            self.error = Some(error);
                            event_loop.exit();
                        }
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let delta = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -y * 40.0,
                    MouseScrollDelta::PixelDelta(position) => {
                        -(position.y as f32)
                            / f64::from_bits(self.scale.load(Ordering::Acquire)) as f32
                    }
                };
                if delta.is_finite() {
                    self.pending_scroll = (self.pending_scroll + delta).clamp(-4096.0, 4096.0);
                    if let Err(error) = self.flush_scroll() {
                        self.error = Some(error);
                        event_loop.exit();
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                self.pressed = self.hit().map(|(node, revision, dimensions)| {
                    let text_anchor = self
                        .cursor
                        .and_then(|(x, y)| self.frame.as_ref()?.image.text_position_at(node, x, y));
                    Pressed {
                        node,
                        revision,
                        dimensions,
                        text_anchor,
                        dark: self.dark.load(Ordering::Acquire),
                        scale: self.scale.load(Ordering::Acquire),
                    }
                });
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                if let Some(pressed) = self.pressed.take() {
                    let current = self.frame.as_ref().filter(|frame| {
                        self.frame_presented
                            && frame.revision == pressed.revision
                            && frame.dimensions == pressed.dimensions
                    });
                    let input = match (pressed.text_anchor, current, self.cursor) {
                        (Some(anchor), Some(frame), Some((x, y))) => frame
                            .image
                            .text_position_at(pressed.node, x, y)
                            .map(|focus| Input::TextSelect {
                                target: Target::Node(pressed.node),
                                anchor,
                                focus,
                                activate: self.hit()
                                    == Some((pressed.node, pressed.revision, pressed.dimensions)),
                            }),
                        (None, Some(_), _)
                            if self.hit()
                                == Some((pressed.node, pressed.revision, pressed.dimensions)) =>
                        {
                            Some(Input::Click {
                                target: Target::Node(pressed.node),
                            })
                        }
                        _ => None,
                    };
                    if let Some(input) = input
                        && self
                            .sender
                            .try_send(WorkerMessage::Input {
                                input,
                                revision: pressed.revision,
                                dimensions: pressed.dimensions,
                                dark: pressed.dark,
                                scale: pressed.scale,
                            })
                            .is_err()
                    {
                        self.error =
                            Some("native input queue is full or the application stopped".into());
                        event_loop.exit();
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.draw() {
                    self.error = Some(error);
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 1 && args.len() != 3 {
        return Err("usage: varta-preview <app.wasm> [<font-file> <font-family>]".into());
    }
    let wasm = read_bounded(
        Path::new(&args[0]),
        varta_common::abi::MAX_WASM_BYTES as u64,
    )?;
    let font = if args.len() == 3 {
        Some((
            read_bounded(Path::new(&args[1]), MAX_FONT_BYTES)?,
            args[2]
                .to_str()
                .ok_or("font family must be UTF-8")?
                .to_owned(),
        ))
    } else {
        None
    };
    let event_loop = EventLoop::<GuiEvent>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let dimensions = Arc::new(AtomicU64::new(0));
    let dark = Arc::new(AtomicBool::new(false));
    let scale = Arc::new(AtomicU64::new(1.0_f64.to_bits()));
    let frame_ack = Arc::new(AtomicU64::new(0));
    let (sender, receiver) = mpsc::sync_channel(8);
    let proxy = event_loop.create_proxy();
    let worker_dimensions = dimensions.clone();
    let worker_dark = dark.clone();
    let worker_ack = frame_ack.clone();
    let worker_scale = scale.clone();
    std::thread::spawn(move || {
        worker(
            wasm,
            font,
            WorkerState {
                dimensions: worker_dimensions,
                dark: worker_dark,
                scale: worker_scale,
                presented: worker_ack,
            },
            receiver,
            proxy,
        )
    });
    let mut preview = Preview {
        window: None,
        surface: None,
        dimensions,
        dark,
        scale,
        sender,
        frame: None,
        error: None,
        presented: false,
        frame_presented: false,
        frame_ack,
        cursor: None,
        pressed: None,
        pending_scroll: 0.0,
        scroll_inflight: None,
        modifiers: ModifiersState::empty(),
        pending_keys: VecDeque::new(),
        key_inflight: None,
        window_focused: false,
        ime_binding: None,
        ime_composing: false,
        title: "Varta preview".into(),
    };
    event_loop.run_app(&mut preview)?;
    if let Some(error) = preview.error {
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Ime, PendingKey, translate_ime};

    #[test]
    fn ime_ignores_late_input_after_window_focus_loss() {
        let mut composing = false;
        assert!(translate_ime(Ime::Commit("好".into()), false, (7, 3), &mut composing).is_none());
        assert!(
            translate_ime(
                Ime::Preedit("好".into(), Some((3, 3))),
                false,
                (7, 3),
                &mut composing
            )
            .is_none()
        );
        assert!(!composing);
        assert!(matches!(
            translate_ime(Ime::Disabled, false, (7, 3), &mut composing),
            Some(PendingKey::ImePreedit { node: 7, commits: 3, text, cursor: None }) if text.is_empty()
        ));
    }

    #[test]
    fn invalid_ime_input_cancels_composition_and_releases_keyboard() {
        let mut composing = false;
        assert!(matches!(
            translate_ime(
                Ime::Preedit("好".into(), Some((3, 3))),
                true,
                (7, 3),
                &mut composing
            ),
            Some(PendingKey::ImePreedit { .. })
        ));
        assert!(composing);
        assert!(matches!(
            translate_ime(Ime::Preedit("好".into(), Some((1, 1))), true, (7, 3), &mut composing),
            Some(PendingKey::ImePreedit { text, cursor: None, .. }) if text.is_empty()
        ));
        assert!(!composing);
        composing = true;
        assert!(matches!(
            translate_ime(Ime::Commit("\n".into()), true, (7, 3), &mut composing),
            Some(PendingKey::ImePreedit { text, cursor: None, .. }) if text.is_empty()
        ));
        assert!(!composing);
    }
}
