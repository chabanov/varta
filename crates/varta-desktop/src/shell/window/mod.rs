//! The native window shared by `varta-preview` and `varta-shell`: a winit
//! event loop on the main thread presents bounded frames, and one worker
//! thread owns the application instance started from a [`Launch`]. The
//! window never decides authority: every effect goes through the runtime's
//! capability-checked host with the grants the launch carries, and every
//! permission prompt the broker asks is shown in a separate host window
//! ([`prompt`]).
mod events;
mod host;
mod ime;
mod prompt;
#[cfg(test)]
mod tests;
mod worker;

use super::{Launch, ShellError, args::Renderer};
#[cfg(feature = "gpu")]
use crate::gpu;
use crate::input_queue::{PendingKeys, WORKER_QUEUE_CAPACITY};
use crate::{accessibility, timing, wake};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, SyncSender},
};
use std::time::{Duration, Instant};
use varta_view::{FrameMetadata, Input, SelectHit};
#[cfg(feature = "gpu")]
use winit::event_loop::EventLoopProxy;
use winit::{
    event_loop::{ControlFlow, EventLoop},
    keyboard::ModifiersState,
    window::Window,
};
use worker::WorkerState;

/// The root font size of the view layout and renderer, in CSS pixels.
const ROOT_FONT_SIZE: f32 = 16.0;
/// How long the window waits, after its event loop ended, for the worker
/// to finish its final consent sync. The worker runs no new turn once the
/// loop ended, so this only waits for a turn already running; a turn that
/// is still running after it is not waited for (the process exits).
const WORKER_EXIT_WAIT: Duration = Duration::from_secs(1);
/// The worker signals its exit once.
const WORKER_EXIT_SIGNALS: usize = 1;

/// Where the window writes guest output and host diagnostics. The binaries
/// choose the process streams and the label; the library itself never
/// prints.
#[derive(Clone, Copy)]
pub struct Console {
    /// The prefix of host diagnostic lines (the running binary's name).
    pub label: &'static str,
    /// One line of guest output (`console.log`).
    pub output: fn(&str),
    /// One complete host diagnostic line.
    pub diagnostic: fn(&str),
}

impl Console {
    /// A console that discards everything (headless tests).
    #[cfg(test)]
    pub(crate) const SILENT: Self = Self {
        label: "varta-test",
        output: |_| {},
        diagnostic: |_| {},
    };

    /// Write one host diagnostic line, prefixed with the binary's label.
    pub(crate) fn report(&self, message: impl std::fmt::Display) {
        (self.diagnostic)(&format!("{}: {message}", self.label));
    }
}

/// How one window host presents its application.
pub struct WindowConfig {
    /// The window title until the guest sets one.
    pub title: &'static str,
    /// The presentation backend ([`check_renderer`]).
    pub renderer: Renderer,
    pub console: Console,
}

/// Whether this build can present with `renderer`. An explicit `gpu` needs
/// a build with the `gpu` feature (and, at window start, a GPU that
/// initializes); `auto` uses the CPU when the GPU is unavailable; `cpu`
/// never uses the GPU.
pub fn check_renderer(renderer: Renderer) -> Result<(), ShellError> {
    if renderer == Renderer::Gpu && !cfg!(feature = "gpu") {
        return Err(ShellError::GpuNotCompiled);
    }
    Ok(())
}

pub(crate) enum GuiEvent {
    AccessibilityWake,
    #[cfg(feature = "gpu")]
    GpuLost,
    Frame(Box<PresentedFrame>),
    /// A permission prompt the worker waits on.
    Prompt(Box<prompt::PromptRequest>),
    Error(String),
}
pub(crate) struct PresentedFrame {
    clipboard_pending: bool,
    accessibility: Option<Result<varta_view::AccessibilityTree, varta_view::AccessibilityError>>,
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
    Clipboard {
        key: varta_view::ClipboardKey,
        trace: Option<timing::InputStamp>,
        version: accessibility::Version,
    },
    Accessibility(accessibility::Pending),
    Input {
        input: Input,
        trace: Option<timing::InputStamp>,
        revision: u64,
        dimensions: u64,
        dark: bool,
        scale: u64,
    },
}

/// One coalesced sample, independent of the bounded activation/key FIFO.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct PointerState {
    trace: Option<timing::InputStamp>,
    position: Option<(f32, f32)>,
    down: bool,
}

struct Host {
    accessibility: Option<accessibility::Bridge>,
    accessibility_adapter: Option<accesskit_winit::Adapter>,
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
    frame: Option<PresentedFrame>,
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
    default_title: &'static str,
    /// An explicit `gpu` renderer: a GPU that fails to initialize is an
    /// error, not a CPU fallback.
    #[cfg(feature = "gpu")]
    gpu_required: bool,
    /// Why a required GPU failed to initialize; the window then closes.
    #[cfg(feature = "gpu")]
    gpu_failure: Option<String>,
    console: Console,
    /// The open permission prompt; guest input is dropped while it is.
    prompt: Option<prompt::OpenPrompt>,
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

/// Open the window and run `launch` until the window closes or the
/// application fails. `started` is the host entry time of the startup
/// measurement. A renderer this build cannot provide ([`check_renderer`])
/// is refused before the window opens.
/// The scrollbar style the system asks for, as Chrome reads it on macOS:
/// AppKit's preferred scroller style (System Settings, "Show scroll bars";
/// "Automatically" resolves by the attached pointing devices), read on the
/// main thread when the window opens. Overlay elsewhere.
fn system_scrollbars() -> varta_view::Scrollbars {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSScroller, NSScrollerStyle};
        if let Some(main) = objc2::MainThreadMarker::new()
            && NSScroller::preferredScrollerStyle(main) == NSScrollerStyle::Legacy
        {
            return varta_view::Scrollbars::Classic;
        }
    }
    varta_view::Scrollbars::Overlay
}

pub fn run(launch: Launch, config: WindowConfig, started: Instant) -> Result<(), ShellError> {
    check_renderer(config.renderer)?;
    let window_error = |error: &dyn std::fmt::Display| ShellError::Window(error.to_string());
    let event_loop = EventLoop::<GuiEvent>::with_user_event()
        .build()
        .map_err(|e| window_error(&e))?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let dimensions = Arc::new(AtomicU64::new(0));
    let dark = Arc::new(AtomicBool::new(false));
    let scale = Arc::new(AtomicU64::new(1.0_f64.to_bits()));
    let frame_ack = Arc::new(AtomicU64::new(0));
    let pointer = Arc::new(Mutex::new(PointerState::default()));
    let wake = Arc::new(wake::Wake::default());
    let gpu_active = Arc::new(AtomicBool::new(
        cfg!(feature = "gpu") && config.renderer.prefers_gpu(),
    ));
    let (sender, receiver) = mpsc::sync_channel(WORKER_QUEUE_CAPACITY);
    let proxy = event_loop.create_proxy();
    let accessibility_active = Arc::new(AtomicBool::new(false));
    let accessibility = accessibility::Bridge::new(
        Some(proxy.clone()),
        wake.clone(),
        accessibility_active.clone(),
        config.console,
    );
    #[cfg(feature = "gpu")]
    let gpu_events = Some(proxy.clone());
    let state = WorkerState {
        dimensions: dimensions.clone(),
        dark: dark.clone(),
        scale: scale.clone(),
        presented: frame_ack.clone(),
        pointer: pointer.clone(),
        wake: wake.clone(),
        gpu_active: gpu_active.clone(),
        accessibility_active,
        console: config.console,
        scrollbars: system_scrollbars(),
        stop: Arc::new(AtomicBool::new(false)),
    };
    let stop = state.stop.clone();
    let (finished, worker_exit) = mpsc::sync_channel(WORKER_EXIT_SIGNALS);
    std::thread::spawn(move || {
        worker::worker(launch, state, receiver, proxy);
        let _ = finished.send(());
    });
    let mut host = Host {
        accessibility: Some(accessibility),
        accessibility_adapter: None,
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
        title: config.title.into(),
        default_title: config.title,
        #[cfg(feature = "gpu")]
        gpu_required: config.renderer == Renderer::Gpu,
        #[cfg(feature = "gpu")]
        gpu_failure: None,
        console: config.console,
        prompt: None,
    };
    let ran = event_loop.run_app(&mut host);
    // End the worker: no further turns, its input queue and any open
    // prompt's answer channel closed (the prompt dismisses), then wait a
    // bounded time for its final consent sync.
    stop.store(true, Ordering::Release);
    #[cfg(feature = "gpu")]
    let gpu_failure = host.gpu_failure.take();
    let error = host.error.take();
    drop(host);
    let _ = worker_exit.recv_timeout(WORKER_EXIT_WAIT);
    ran.map_err(|e| window_error(&e))?;
    #[cfg(feature = "gpu")]
    if let Some(reason) = gpu_failure {
        return Err(ShellError::GpuUnavailable(reason));
    }
    match error {
        Some(error) => Err(ShellError::Application(error)),
        None => Ok(()),
    }
}
