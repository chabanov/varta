//! The native window shared by `varta-preview` and `varta-shell`: a winit
//! event loop on the main thread presents bounded frames, and one worker
//! thread owns the application instance started from a [`Launch`]. The
//! window never decides authority: every effect goes through the runtime's
//! capability-checked host with the grants the launch carries.
mod events;
mod host;
mod ime;
#[cfg(test)]
mod tests;
mod worker;

use super::{Launch, ShellError};
#[cfg(feature = "gpu")]
use crate::gpu;
use crate::input_queue::{PendingKeys, WORKER_QUEUE_CAPACITY};
use crate::{accessibility, timing, wake};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64},
    mpsc::{self, SyncSender},
};
use std::time::Instant;
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

/// Where the window writes guest output and host diagnostics. The binaries
/// choose the process streams; the library itself never prints.
#[derive(Clone, Copy)]
pub struct Console {
    /// One line of guest output (`console.log`).
    pub output: fn(&str),
    /// One complete host diagnostic line.
    pub diagnostic: fn(&str),
}

/// How one window host presents its application.
pub struct WindowConfig {
    /// The window title until the guest sets one.
    pub title: &'static str,
    /// The prefix of host diagnostic lines (the binary name).
    pub label: &'static str,
    /// An explicit layout and render font: its bytes and family name.
    pub font: Option<(Vec<u8>, String)>,
    /// Try the GPU renderer first (only with the `gpu` feature).
    pub gpu: bool,
    pub console: Console,
}

pub(crate) enum GuiEvent {
    AccessibilityWake,
    #[cfg(feature = "gpu")]
    GpuLost,
    Frame(Box<PresentedFrame>),
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
    label: &'static str,
    console: Console,
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
/// measurement.
pub fn run(launch: Launch, config: WindowConfig, started: Instant) -> Result<(), ShellError> {
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
    let gpu_active = Arc::new(AtomicBool::new(cfg!(feature = "gpu") && config.gpu));
    let (sender, receiver) = mpsc::sync_channel(WORKER_QUEUE_CAPACITY);
    let proxy = event_loop.create_proxy();
    let accessibility_active = Arc::new(AtomicBool::new(false));
    let accessibility = accessibility::Bridge::new(
        Some(proxy.clone()),
        wake.clone(),
        accessibility_active.clone(),
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
    };
    let font = config.font;
    std::thread::spawn(move || worker::worker(launch, font, state, receiver, proxy));
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
        label: config.label,
        console: config.console,
    };
    event_loop
        .run_app(&mut host)
        .map_err(|e| window_error(&e))?;
    match host.error {
        Some(error) => Err(ShellError::Application(error)),
        None => Ok(()),
    }
}
