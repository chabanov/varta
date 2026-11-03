#![forbid(unsafe_code)]
//! Experimental native preview for an already compiled, unprivileged view module.
#[cfg(feature = "gpu")]
mod gpu;
#[cfg(feature = "gpu")]
mod gpu_scene;
mod input_queue;
mod timing;
mod wake;
use input_queue::{MAX_KEY_TEXT_BYTES, PendingKey, PendingKeys, WORKER_QUEUE_CAPACITY};
use std::io::Read;
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, Receiver, SyncSender},
};
use std::time::{Duration, Instant};
use varta_runtime::{ApplicationOptions, Step, VartaRuntime};
use varta_view::{FrameMetadata, Input, SelectHit, Target, TextMeasurer};
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{Key as WinitKey, KeyCode, ModifiersState, NamedKey, PhysicalKey},
    window::{CursorIcon, Theme, Window, WindowId},
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

mod pixels;
#[cfg(test)]
use pixels::WHITE_RGB;
use pixels::update_rgb;

enum GuiEvent {
    #[cfg(feature = "gpu")]
    GpuLost,
    Frame(Box<PreviewFrame>),
    Error(String),
}
struct PreviewFrame {
    image: Arc<FrameMetadata>,
    scene: Option<varta_view::DisplayList>,
    rgb: Vec<u32>,
    revision: u64,
    dimensions: u64,
    sequence: u64,
    commits: usize,
    dark: bool,
    title: String,
    scale: u64,
    timing: Option<timing::FrameTiming>,
}
enum WorkerMessage {
    Input {
        input: Input,
        trace: Option<timing::InputStamp>,
        revision: u64,
        dimensions: u64,
        dark: bool,
        scale: u64,
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
    pointer: Arc<Mutex<PointerState>>,
    wake: Arc<wake::Wake>,
    gpu_active: Arc<AtomicBool>,
}

/// One coalesced sample, independent of the bounded activation/key FIFO.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct PointerState {
    trace: Option<timing::InputStamp>,
    position: Option<(f32, f32)>,
    down: bool,
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
        pointer,
        wake,
        gpu_active,
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
        let render_font = font
            .map(|(data, family)| TextMeasurer::new(data, &family).map_err(|e| e.to_string()))
            .transpose()?;
        let mut renderer =
            varta_view::Renderer::new(render_font, 16.0).map_err(|e| e.to_string())?;
        let mut rgb = Vec::new();
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
        let mut last_image: Option<Arc<FrameMetadata>> = None;
        let mut rendered_gpu = false;
        let mut pointer_changed = false;
        let mut timing = timing::Pending::default();
        loop {
            let observed_wake = wake.snapshot();
            let mut active = false;
            match messages.try_recv() {
                Ok(WorkerMessage::Input {
                    input,
                    trace,
                    revision: input_revision,
                    dimensions: event_dimensions,
                    dark: event_dark,
                    scale: event_scale,
                }) => {
                    active = true;
                    // A queued OS event must not target a different tree or viewport.
                    if revision == input_revision
                        && dimensions.load(Ordering::Acquire) == event_dimensions
                        && dark.load(Ordering::Acquire) == event_dark
                        && scale.load(Ordering::Acquire) == event_scale
                    {
                        timing.accept(trace);
                        let input_started = Instant::now();
                        app.view_input(&input).map_err(|e| e.to_string())?;
                        timing.guest(input_started.elapsed());
                        revision = revision.checked_add(1).ok_or("view revision exhausted")?;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
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
                    ..Default::default()
                })
                .map_err(|e| e.to_string())?;
                environment_dark = current_dark;
                revision = revision.checked_add(1).ok_or("view revision exhausted")?;
            }
            let guest_started = Instant::now();
            for _ in 0..16 {
                match app.step(Some(Duration::ZERO)) {
                    Step::Turn(_) => {
                        active = true;
                    }
                    Step::Failed => {
                        return Err(app
                            .failure()
                            .map_or_else(|| "application failed".to_owned(), ToString::to_string));
                    }
                    Step::Idle | Step::Finished { .. } => break,
                }
            }
            timing.guest(guest_started.elapsed());
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
            let sample = *pointer.lock().unwrap_or_else(|e| e.into_inner());
            if sample.position.is_none() {
                let changed = app.view_set_pointer_state(None, None);
                if changed {
                    timing.accept(sample.trace);
                }
                pointer_changed |= changed;
            } else if in_flight.is_none()
                && revision == rendered_revision
                && dimensions == rendered_dimensions
                && environment_dark == current_dark
                && environment_scale == current_scale
                && let (Some(image), Some((x, y))) = (&last_image, sample.position)
            {
                let target = match image.select_hit(x, y) {
                    Some(SelectHit::Choice { option, .. }) => Some(option),
                    Some(SelectHit::Disabled { .. } | SelectHit::Dismiss { .. }) => None,
                    None => image.hit_test(x, y),
                };
                let changed =
                    app.view_set_pointer_state(target, sample.down.then_some(target).flatten());
                if changed {
                    timing.accept(sample.trace);
                }
                pointer_changed |= changed;
            }
            let current_gpu = gpu_active.load(Ordering::Acquire);
            if in_flight.is_none()
                && width > 0
                && height > 0
                && app.view_commits() > 0
                && (revision != rendered_revision
                    || dimensions != rendered_dimensions
                    || pointer_changed
                    || rendered_gpu != current_gpu)
            {
                let render_started = Instant::now();
                let (frame, vector) = if current_gpu {
                    match app
                        .view_prepare(&mut renderer, width, height, factor)
                        .map_err(|e| e.to_string())?
                    {
                        varta_view::PreparedFrame::Vector { metadata, scene } => {
                            rgb.clear();
                            (metadata, Some(scene))
                        }
                        varta_view::PreparedFrame::Raster(frame) => {
                            gpu_active.store(false, Ordering::Release);
                            update_rgb(&frame, &mut rgb, None, true)?;
                            (frame.into_metadata(), None)
                        }
                    }
                } else {
                    let frame = app
                        .view_render(&mut renderer, width, height, factor)
                        .map_err(|e| e.to_string())?;
                    update_rgb(
                        &frame,
                        &mut rgb,
                        renderer.damage(),
                        rendered_dimensions != dimensions || rendered_gpu,
                    )?;
                    (frame.into_metadata(), None)
                };
                rendered_gpu = vector.is_some();
                if pointer_changed
                    && revision == rendered_revision
                    && last_image
                        .as_ref()
                        .is_some_and(|previous| !previous.same_input_geometry(&frame))
                {
                    revision = revision.checked_add(1).ok_or("view revision exhausted")?;
                }
                rendered_revision = revision;
                rendered_dimensions = dimensions;
                sequence = sequence.checked_add(1).ok_or("frame sequence exhausted")?;
                let frame = Arc::new(frame);
                last_image = Some(frame.clone());
                pointer_changed = false;
                if proxy
                    .send_event(GuiEvent::Frame(Box::new(PreviewFrame {
                        image: frame,
                        scene: vector,
                        rgb: rgb.clone(),
                        revision: rendered_revision,
                        dimensions,
                        sequence,
                        commits: known_commits,
                        dark: environment_dark,
                        title: app.view_title().into(),
                        scale: environment_scale,
                        timing: timing.frame(render_started),
                    })))
                    .is_err()
                {
                    return Ok(());
                }
                in_flight = Some(sequence);
            }
            if !active {
                wake.wait(observed_wake, Duration::from_millis(16));
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
    #[cfg(feature = "gpu")]
    gpu: Option<gpu::Backend>,
    #[cfg(feature = "gpu")]
    gpu_events: Option<EventLoopProxy<GuiEvent>>,
    dimensions: Arc<AtomicU64>,
    dark: Arc<AtomicBool>,
    scale: Arc<AtomicU64>,
    sender: SyncSender<WorkerMessage>,
    frame: Option<PreviewFrame>,
    error: Option<String>,
    presented: bool,
    started: Instant,
    frame_presented: bool,
    frame_ack: Arc<AtomicU64>,
    cursor: Option<(f32, f32)>,
    pointer: Arc<Mutex<PointerState>>,
    wake: Arc<wake::Wake>,
    gpu_active: Arc<AtomicBool>,
    pressed: Option<Pressed>,
    pending_scroll: f32,
    scroll_inflight: Option<(u64, u64)>,
    modifiers: ModifiersState,
    pending_keys: PendingKeys,
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
    select_hit: Option<SelectHit>,
}
impl Preview {
    #[cfg(feature = "gpu")]
    fn switch_to_cpu(&mut self) -> Result<(), String> {
        eprintln!("varta-preview: switching GPU to CPU fallback");
        self.gpu.take();
        self.gpu_active.store(false, Ordering::Release);
        let window = self.window.as_ref().ok_or("window unavailable")?.clone();
        let context = softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
        self.surface = Some(softbuffer::Surface::new(&context, window).map_err(|e| e.to_string())?);
        self.wake.notify();
        Ok(())
    }
    fn update_pointer(&self, down: Option<bool>) {
        let mut state = self.pointer.lock().unwrap_or_else(|e| e.into_inner());
        let before = (state.position, state.down);
        state.position = self.cursor;
        if let Some(down) = down {
            state.down = down;
        }
        if state.position.is_none() {
            state.down = false;
        }
        if before != (state.position, state.down) {
            state.trace = timing::InputStamp::now("pointer-state");
            self.wake.notify();
        }
    }
    fn update_cursor(&self) {
        if let Some(window) = &self.window {
            let hint = self
                .cursor
                .and_then(|(x, y)| self.frame.as_ref().map(|f| f.image.cursor_at(x, y)))
                .unwrap_or_default();
            window.set_cursor(match hint {
                varta_view::PointerCursor::Default => CursorIcon::Default,
                varta_view::PointerCursor::Pointer => CursorIcon::Pointer,
                varta_view::PointerCursor::Text => CursorIcon::Text,
                varta_view::PointerCursor::NotAllowed => CursorIcon::NotAllowed,
            });
        }
    }
    fn size_changed(&self, window: &Window) {
        let size = window.inner_size();
        self.dimensions.store(
            (u64::from(size.width) << 32) | u64::from(size.height),
            Ordering::Release,
        );
        self.wake.notify();
    }
    fn draw(&mut self) -> Result<(), String> {
        #[cfg(feature = "gpu")]
        if self.gpu.is_some() && !self.gpu_active.load(Ordering::Acquire) {
            self.switch_to_cpu()?;
        }
        let (Some(window), Some(frame)) = (&self.window, &self.frame) else {
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
            self.wake.notify();
            return Ok(());
        }
        let Some(width) = NonZeroU32::new(size.width) else {
            return Ok(());
        };
        let Some(height) = NonZeroU32::new(size.height) else {
            return Ok(());
        };
        let copy_started = Instant::now();
        let first_present = self.frame_ack.load(Ordering::Acquire) < frame.sequence;
        #[cfg(feature = "gpu")]
        if let (Some(gpu), Some(scene)) = (&mut self.gpu, &frame.scene) {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                gpu.draw(scene, size.width, size.height)
            }));
            match result {
                Ok(Ok(true)) => {}
                Ok(Ok(false)) => {
                    return Ok(());
                }
                _ => {
                    let sequence = frame.sequence;
                    self.switch_to_cpu()?;
                    self.frame_presented = false;
                    self.frame_ack.store(sequence, Ordering::Release);
                    self.wake.notify();
                    return Ok(());
                }
            }
        } else {
            if frame.scene.is_some() {
                self.frame_presented = false;
                self.frame_ack.store(frame.sequence, Ordering::Release);
                self.wake.notify();
                return Ok(());
            }
            let surface = self.surface.as_mut().ok_or("CPU surface unavailable")?;
            surface.resize(width, height).map_err(|e| e.to_string())?;
            let mut buffer = surface.buffer_mut().map_err(|e| e.to_string())?;
            if buffer.len() != frame.rgb.len() {
                return Err("frame presentation dimensions differ".into());
            }
            buffer.copy_from_slice(&frame.rgb);
            buffer.present().map_err(|e| e.to_string())?;
        }
        #[cfg(not(feature = "gpu"))]
        {
            if frame.scene.is_some() {
                return Err("GPU scene requires the GPU backend".into());
            }
            let surface = self.surface.as_mut().ok_or("CPU surface unavailable")?;
            surface.resize(width, height).map_err(|e| e.to_string())?;
            let mut buffer = surface.buffer_mut().map_err(|e| e.to_string())?;
            if buffer.len() != frame.rgb.len() {
                return Err("frame presentation dimensions differ".into());
            }
            buffer.copy_from_slice(&frame.rgb);
            buffer.present().map_err(|e| e.to_string())?;
        }
        if first_present && let Some(timing) = frame.timing {
            timing.report(
                frame.sequence,
                if frame.scene.is_some() { "gpu" } else { "cpu" },
                copy_started,
                Instant::now(),
            );
        }
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
        self.wake.notify();
        self.update_cursor();
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
            if timing::InputStamp::now("startup").is_some() {
                eprintln!(
                    "varta-ui-startup: {{\"boundary\":\"host-entry-to-first-present-return\",\"backend\":\"{}\",\"total_ms\":{:.3}}}",
                    if frame.scene.is_some() { "gpu" } else { "cpu" },
                    self.started.elapsed().as_secs_f64() * 1000.0,
                );
            }
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
            frame
                .image
                .select_hit(x, y)
                .map(SelectHit::target)
                .or_else(|| frame.image.hit_test(x, y))?,
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
        if let Some(menu) = frame.image.select_menu() {
            let rect = menu.viewport;
            if x < rect.x || y < rect.y || x >= rect.x + rect.width || y >= rect.y + rect.height {
                return Ok(());
            }
            let rows = (delta.abs() / menu.row_height).ceil() as usize;
            let first = if delta < 0.0 {
                menu.first.saturating_sub(rows)
            } else {
                menu.first.saturating_add(rows).min(menu.max_first)
            };
            if first != menu.first {
                self.sender
                    .try_send(WorkerMessage::Input {
                        trace: timing::InputStamp::now("select-scroll"),
                        input: Input::SelectScroll {
                            target: Target::Node(menu.target),
                            first,
                        },
                        revision: frame.revision,
                        dimensions: frame.dimensions,
                        dark: frame.dark,
                        scale: frame.scale,
                    })
                    .map_err(|_| {
                        "native input queue is full or the application stopped".to_owned()
                    })?;
                self.wake.notify();
                self.scroll_inflight = Some((frame.revision, frame.dimensions));
            }
            return Ok(());
        }
        if let Some((node, top, remaining)) = frame.image.scroll_target(x, y, delta) {
            let metrics = frame
                .image
                .scroll_metrics(node)
                .ok_or("scroll target is missing frame metrics")?;
            self.sender
                .try_send(WorkerMessage::Input {
                    trace: timing::InputStamp::now("scroll"),
                    input: Input::ScrollMeasured {
                        target: Target::Node(node),
                        top,
                        scroll_height: f64::from(metrics.viewport.height) + f64::from(metrics.max),
                        client_height: f64::from(metrics.viewport.height),
                    },
                    revision: frame.revision,
                    dimensions: frame.dimensions,
                    dark: frame.dark,
                    scale: frame.scale,
                })
                .map_err(|_| "native input queue is full or the application stopped".to_owned())?;
            self.wake.notify();
            self.pending_scroll = remaining;
            self.scroll_inflight = Some((frame.revision, frame.dimensions));
        }
        Ok(())
    }
    fn enqueue_key(&mut self, pending: PendingKey) -> Result<(), String> {
        self.pending_keys.push(pending).map_err(str::to_owned)?;
        self.flush_keys()
    }
    fn send_pointer(&mut self, input: Input, pressed: Pressed) -> Result<(), String> {
        self.sender
            .try_send(WorkerMessage::Input {
                input,
                trace: timing::InputStamp::now("pointer"),
                revision: pressed.revision,
                dimensions: pressed.dimensions,
                dark: pressed.dark,
                scale: pressed.scale,
            })
            .map_err(|_| "native input queue is full or the application stopped".to_owned())?;
        self.wake.notify();
        // A click can change focus. Keep immediately following characters in
        // FIFO order until its resulting frame supplies the new focused target.
        self.key_inflight = Some((pressed.revision, pressed.dimensions));
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
                PendingKey::SelectClose(node) => {
                    if frame
                        .image
                        .select_menu()
                        .is_none_or(|menu| menu.target != node)
                    {
                        continue;
                    }
                    Input::SelectClose {
                        target: Target::Node(node),
                    }
                }
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
                    if let Some(movement) = frame.image.text_navigation(target, key).filter(|_| {
                        !modifiers.control_key() && !modifiers.alt_key() && !modifiers.super_key()
                    }) {
                        Input::TextNavigate {
                            target: Target::Node(target),
                            key: key.into(),
                            position: movement.position,
                            preferred_x: movement.preferred_x,
                            upstream: movement.upstream,
                            shift: modifiers.shift_key(),
                        }
                    } else {
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
                    trace: timing::InputStamp::now("key"),
                    revision: frame.revision,
                    dimensions: frame.dimensions,
                    dark: frame.dark,
                    scale: frame.scale,
                })
                .map_err(|_| "native input queue is full or the application stopped".to_owned())?;
            self.wake.notify();
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
    #[cfg(feature = "gpu")]
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let poll = self.gpu.as_ref().map(gpu::Backend::poll);
        match poll {
            Some(Ok(true)) => event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(4),
            )),
            Some(Err(_)) => {
                if let Some(events) = &self.gpu_events {
                    let _ = events.send_event(GuiEvent::GpuLost);
                }
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            Some(Ok(false)) => {
                if self.gpu.as_ref().is_some_and(gpu::Backend::visible)
                    && !self.frame_presented
                    && self.frame.as_ref().is_some_and(|f| {
                        f.scene.is_some()
                            && f.dimensions == self.dimensions.load(Ordering::Acquire)
                            && f.scale == self.scale.load(Ordering::Acquire)
                            && f.dark == self.dark.load(Ordering::Acquire)
                    })
                    && let Some(window) = &self.window
                {
                    window.request_redraw();
                }
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            _ => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
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
            #[cfg(feature = "gpu")]
            if self.gpu_active.load(Ordering::Acquire) {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    gpu::Backend::new(window.clone(), self.gpu_events.clone())
                }));
                match result {
                    Ok(Ok(gpu)) => self.gpu = Some(gpu),
                    _ => {
                        eprintln!("varta-preview: GPU unavailable, using CPU");
                        self.gpu_active.store(false, Ordering::Release);
                    }
                }
            }
            if !self.gpu_active.load(Ordering::Acquire) {
                let context =
                    softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
                self.surface = Some(
                    softbuffer::Surface::new(&context, window.clone())
                        .map_err(|e| e.to_string())?,
                );
            }
            self.dark
                .store(window.theme() == Some(Theme::Dark), Ordering::Release);
            self.scale
                .store(window.scale_factor().to_bits(), Ordering::Release);
            self.size_changed(&window);
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
            #[cfg(feature = "gpu")]
            GuiEvent::GpuLost => {
                if self.gpu.is_none() {
                    return;
                }
                if let Err(error) = self.switch_to_cpu() {
                    self.error = Some(error);
                    event_loop.exit();
                    return;
                }
                self.frame_presented = false;
                if let Some(frame) = &self.frame {
                    self.frame_ack.store(frame.sequence, Ordering::Release);
                }
                self.wake.notify();
            }
            GuiEvent::Frame(frame) => {
                self.frame = Some(*frame);
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
            WindowEvent::Occluded(false) => {
                #[cfg(feature = "gpu")]
                if let Some(gpu) = &mut self.gpu {
                    gpu.unocclude();
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
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
                self.update_pointer(None);
                self.update_cursor();
            }
            WindowEvent::CursorLeft { .. } => {
                self.cursor = None;
                self.update_pointer(Some(false));
                self.update_cursor();
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
                    self.cursor = None;
                    self.update_pointer(Some(false));
                    self.modifiers = ModifiersState::empty();
                    self.pending_keys.clear();
                    self.ime_composing = false;
                    if let Some(menu) = self
                        .frame
                        .as_ref()
                        .and_then(|frame| frame.image.select_menu())
                        && let Err(error) = self.enqueue_key(PendingKey::SelectClose(menu.target))
                    {
                        self.error = Some(error);
                        event_loop.exit();
                    }
                    // Some windowing backends do not report Ime::Disabled when
                    // focus moves away. Clear any displayed composition even then.
                    if let Some((node, commits)) = self.ime_binding
                        && let Err(error) = self.enqueue_key(PendingKey::ImePreedit {
                            node,
                            commits,
                            text: String::new(),
                            cursor: None,
                        })
                    {
                        self.error = Some(error);
                        event_loop.exit();
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
                    if let Some(pending) = pending
                        && let Err(error) = self.enqueue_key(pending)
                    {
                        self.error = Some(error);
                        event_loop.exit();
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
                    if key.is_empty()
                        || key.len() > MAX_KEY_TEXT_BYTES
                        || key.chars().any(char::is_control)
                    {
                        None
                    } else {
                        let text = event
                            .text
                            .as_ref()
                            .filter(|text| {
                                text.len() <= MAX_KEY_TEXT_BYTES
                                    && !text.chars().any(char::is_control)
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
                if let Some(pending) = pending
                    && let Err(error) = self.enqueue_key(pending)
                {
                    self.error = Some(error);
                    event_loop.exit();
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
                self.update_pointer(Some(true));
                self.pressed = self.hit().map(|(node, revision, dimensions)| {
                    let text_anchor = self
                        .cursor
                        .and_then(|(x, y)| self.frame.as_ref()?.image.text_position_at(node, x, y));
                    Pressed {
                        node,
                        revision,
                        dimensions,
                        text_anchor,
                        select_hit: self
                            .cursor
                            .and_then(|(x, y)| self.frame.as_ref()?.image.select_hit(x, y)),
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
                self.update_pointer(Some(false));
                if let Some(pressed) = self.pressed.take() {
                    let current = self.frame.as_ref().filter(|frame| {
                        self.frame_presented
                            && frame.revision == pressed.revision
                            && frame.dimensions == pressed.dimensions
                    });
                    let input = if let Some(hit) = pressed.select_hit {
                        match (current, self.cursor) {
                            (Some(frame), Some((x, y)))
                                if frame.image.select_hit(x, y) == Some(hit) =>
                            {
                                match hit {
                                    SelectHit::Choice { select, option } => {
                                        Some(Input::SelectOption {
                                            target: Target::Node(select),
                                            option,
                                        })
                                    }
                                    SelectHit::Dismiss { select } => Some(Input::SelectClose {
                                        target: Target::Node(select),
                                    }),
                                    SelectHit::Disabled { .. } => None,
                                }
                            }
                            _ => None,
                        }
                    } else {
                        match (pressed.text_anchor, current, self.cursor) {
                            (Some(anchor), Some(frame), Some((x, y))) => frame
                                .image
                                .text_position_at(pressed.node, x, y)
                                .map(|focus| Input::TextSelect {
                                    target: Target::Node(pressed.node),
                                    anchor,
                                    focus,
                                    activate: self.hit()
                                        == Some((
                                            pressed.node,
                                            pressed.revision,
                                            pressed.dimensions,
                                        )),
                                }),
                            (None, Some(_), _)
                                if self.hit()
                                    == Some((
                                        pressed.node,
                                        pressed.revision,
                                        pressed.dimensions,
                                    )) =>
                            {
                                Some(Input::Click {
                                    target: Target::Node(pressed.node),
                                })
                            }
                            _ => None,
                        }
                    };
                    if let Some(input) = input
                        && let Err(error) = self.send_pointer(input, pressed)
                    {
                        self.error = Some(error);
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
    let started = Instant::now();
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && args[0] == "--renderer-info" {
        println!(
            "{{\"schemaVersion\":1,\"gpuCompiled\":{},\"default\":\"cpu\"}}",
            cfg!(feature = "gpu")
        );
        return Ok(());
    }
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
    let pointer = Arc::new(Mutex::new(PointerState::default()));
    let wake = Arc::new(wake::Wake::default());
    let gpu_active = Arc::new(AtomicBool::new(
        cfg!(feature = "gpu")
            && std::env::var_os("VARTA_PREVIEW_RENDERER")
                .is_some_and(|v| v == "gpu" || v == "auto"),
    ));
    let (sender, receiver) = mpsc::sync_channel(WORKER_QUEUE_CAPACITY);
    let proxy = event_loop.create_proxy();
    #[cfg(feature = "gpu")]
    let gpu_events = Some(proxy.clone());
    let worker_dimensions = dimensions.clone();
    let worker_dark = dark.clone();
    let worker_ack = frame_ack.clone();
    let worker_scale = scale.clone();
    let worker_pointer = pointer.clone();
    let worker_wake = wake.clone();
    let worker_gpu = gpu_active.clone();
    std::thread::spawn(move || {
        worker(
            wasm,
            font,
            WorkerState {
                dimensions: worker_dimensions,
                dark: worker_dark,
                scale: worker_scale,
                presented: worker_ack,
                pointer: worker_pointer,
                wake: worker_wake,
                gpu_active: worker_gpu,
            },
            receiver,
            proxy,
        )
    });
    let mut preview = Preview {
        window: None,
        surface: None,
        #[cfg(feature = "gpu")]
        gpu: None,
        #[cfg(feature = "gpu")]
        gpu_events,
        dimensions,
        dark,
        scale,
        sender,
        frame: None,
        error: None,
        presented: false,
        started,
        frame_presented: false,
        frame_ack,
        cursor: None,
        pointer,
        wake,
        gpu_active,
        pressed: None,
        pending_scroll: 0.0,
        scroll_inflight: None,
        modifiers: ModifiersState::empty(),
        pending_keys: PendingKeys::default(),
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
    use super::{Ime, PendingKey, WHITE_RGB, translate_ime, update_rgb};
    use std::sync::Arc;

    fn headless_preview() -> (
        super::Preview,
        std::sync::mpsc::Receiver<super::WorkerMessage>,
    ) {
        use super::*;
        let (sender, receiver) = mpsc::sync_channel(WORKER_QUEUE_CAPACITY);
        (
            Preview {
                window: None,
                surface: None,
                #[cfg(feature = "gpu")]
                gpu: None,
                #[cfg(feature = "gpu")]
                gpu_events: None,
                dimensions: Arc::new(AtomicU64::new(0)),
                dark: Arc::new(AtomicBool::new(false)),
                scale: Arc::new(AtomicU64::new(1.0f64.to_bits())),
                sender,
                frame: None,
                error: None,
                presented: false,
                started: Instant::now(),
                frame_presented: false,
                frame_ack: Arc::new(AtomicU64::new(0)),
                cursor: None,
                pointer: Arc::new(Mutex::new(PointerState::default())),
                wake: Arc::new(wake::Wake::default()),
                gpu_active: Arc::new(AtomicBool::new(false)),
                pressed: None,
                pending_scroll: 0.0,
                scroll_inflight: None,
                modifiers: ModifiersState::empty(),
                pending_keys: PendingKeys::default(),
                key_inflight: None,
                window_focused: true,
                ime_binding: None,
                ime_composing: false,
                title: String::new(),
            },
            receiver,
        )
    }
    fn present_empty_frame(preview: &mut super::Preview) {
        preview.frame = Some(super::PreviewFrame {
            timing: None,
            scene: None,
            image: Arc::new(
                varta_view::View::default()
                    .rasterize_boxes(16, 16)
                    .unwrap()
                    .into_metadata(),
            ),
            rgb: vec![WHITE_RGB; 16 * 16],
            revision: 1,
            dimensions: 16u64 << 32 | 16,
            sequence: 1,
            commits: 0,
            dark: false,
            title: String::new(),
            scale: 1.0f64.to_bits(),
        });
        preview.frame_presented = true;
    }
    #[test]
    fn pointer_motion_is_coalesced_outside_the_activation_fifo_and_leave_clears_press() {
        let (mut preview, receiver) = headless_preview();
        for n in 0..10_000 {
            preview.cursor = Some((n as f32, 5.0));
            preview.update_pointer(Some(true));
        }
        assert_eq!(
            preview.pointer.lock().unwrap().position,
            Some((9999.0, 5.0))
        );
        assert!(receiver.try_recv().is_err());
        preview.cursor = None;
        preview.update_pointer(None);
        assert_eq!(
            *preview.pointer.lock().unwrap(),
            super::PointerState::default()
        );
    }

    #[test]
    fn dirty_rgb_conversion_matches_full_conversion_with_alpha_and_resize() {
        let mut image = varta_view::View::default().rasterize_boxes(4, 4).unwrap();
        let mut rgb = Vec::new();
        update_rgb(&image, &mut rgb, None, true).unwrap();
        for (index, color) in [
            (0, [12, 30, 50, 255]),
            (5, [10, 20, 30, 64]),
            (15, [200, 100, 50, 255]),
        ] {
            image.pixels[index * 4..index * 4 + 4].copy_from_slice(&color);
            update_rgb(
                &image,
                &mut rgb,
                Some(varta_view::BoxRect {
                    x: (index % 4) as f32,
                    y: (index / 4) as f32,
                    width: 1.0,
                    height: 1.0,
                }),
                false,
            )
            .unwrap();
            let mut full = Vec::new();
            update_rgb(&image, &mut full, None, true).unwrap();
            assert_eq!(rgb, full);
        }
        assert_eq!(rgb[0], 0x000c_1e32);
        assert_eq!(rgb[5], 0x00c9_d3dd);
        let unchanged = rgb.clone();
        update_rgb(&image, &mut rgb, None, false).unwrap();
        assert_eq!(rgb, unchanged);
        let resized = varta_view::View::default().rasterize_boxes(2, 8).unwrap();
        update_rgb(&resized, &mut rgb, None, true).unwrap();
        assert!(rgb.iter().all(|c| *c == WHITE_RGB));
    }

    #[test]
    fn immediate_typing_waits_for_the_pointer_focus_frame() {
        let (mut preview, receiver) = headless_preview();
        present_empty_frame(&mut preview);
        let frame = preview.frame.as_ref().unwrap();
        let pressed = super::Pressed {
            node: 7,
            revision: frame.revision,
            dimensions: frame.dimensions,
            text_anchor: None,
            select_hit: None,
            dark: frame.dark,
            scale: frame.scale,
        };
        preview
            .send_pointer(
                varta_view::Input::Click {
                    target: varta_view::Target::Node(7),
                },
                pressed,
            )
            .unwrap();
        preview
            .enqueue_key(PendingKey::Character {
                key: "Ш".into(),
                text: "Ш".into(),
                modifiers: super::ModifiersState::empty(),
                select_all: false,
                history: None,
            })
            .unwrap();
        assert!(matches!(
            receiver.try_recv().unwrap(),
            super::WorkerMessage::Input {
                input: varta_view::Input::Click { .. },
                ..
            }
        ));
        assert!(matches!(
            receiver.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        assert!(
            matches!(preview.pending_keys.pop_front(), Some(PendingKey::Character { text, .. }) if text == "Ш")
        );
    }

    #[test]
    fn blocked_preview_compacts_spare_capacity_without_losing_preedit_text() {
        let (mut preview, receiver) = headless_preview();
        let mut text = String::with_capacity(super::input_queue::MAX_PENDING_TEXT_BYTES);
        text.push_str("old");
        let pending = translate_ime(
            Ime::Preedit(text, None),
            true,
            (7, 3),
            &mut preview.ime_composing,
        )
        .unwrap();
        preview.enqueue_key(pending).unwrap();
        let mut text = String::with_capacity(super::input_queue::MAX_PENDING_TEXT_BYTES + 1);
        text.push_str("new");
        let pending = translate_ime(
            Ime::Preedit(text, None),
            true,
            (7, 3),
            &mut preview.ime_composing,
        )
        .unwrap();
        preview.enqueue_key(pending).unwrap();
        assert!(matches!(
            receiver.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        assert!(
            matches!(preview.pending_keys.pop_front(),Some(PendingKey::ImePreedit{text,..}) if text=="new" && text.capacity()==3)
        );
    }
    #[test]
    fn preview_keeps_fifo_before_first_frame_and_sends_one_key_until_the_next_frame() {
        let (mut preview, receiver) = headless_preview();
        preview
            .enqueue_key(PendingKey::Key("Tab", super::ModifiersState::SHIFT))
            .unwrap();
        preview
            .enqueue_key(PendingKey::Key("Enter", super::ModifiersState::empty()))
            .unwrap();
        assert!(matches!(
            receiver.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        present_empty_frame(&mut preview);
        preview.flush_keys().unwrap();
        preview.flush_keys().unwrap();
        let super::WorkerMessage::Input { input, .. } = receiver.try_recv().unwrap();
        assert!(matches!(input,varta_view::Input::Key{key,shift:true,..} if key=="Tab"));
        assert!(matches!(
            receiver.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        assert!(matches!(
            preview.pending_keys.pop_front(),
            Some(PendingKey::Key("Enter", _))
        ));
    }
    #[test]
    fn stale_ime_binding_discard_releases_capacity_before_new_input_is_queued() {
        let (mut preview, receiver) = headless_preview();
        let text = "界".repeat(varta_common::view::DRAFT_MAX_VALUE_BYTES / 2);
        let count = super::input_queue::MAX_PENDING_TEXT_BYTES / text.len();
        for _ in 0..count {
            preview
                .enqueue_key(PendingKey::ImeCommit {
                    node: 7,
                    commits: 3,
                    text: text.clone(),
                })
                .unwrap();
        }
        present_empty_frame(&mut preview);
        preview.flush_keys().unwrap();
        assert!(preview.pending_keys.is_empty());
        assert!(matches!(
            receiver.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        preview.frame_presented = false;
        preview
            .enqueue_key(PendingKey::ImeCommit {
                node: 8,
                commits: 4,
                text,
            })
            .unwrap();
    }

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
