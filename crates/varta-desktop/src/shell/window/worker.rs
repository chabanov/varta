//! The worker thread: owns the application instance, applies queued input
//! between guest turns and renders frames for the window to present. It
//! starts the instance with the native permission prompt and keeps its
//! consent state in sync between turns: stored decisions and the audit log
//! are written, and revocations stored from outside apply before the next
//! turns run. The consent state is synced once more on every exit (a
//! closed window, a failed turn or a host error), so the last answers,
//! dismissals and audit entries are not lost; the window waits for that
//! within `WORKER_EXIT_WAIT`.
use super::prompt::NativePrompter;
use super::{Console, GuiEvent, PointerState, PresentedFrame, ROOT_FONT_SIZE, WorkerMessage};
use crate::pixels::update_rgb;
use crate::shell::consent::HostPrompt;
use crate::shell::{Launch, NativeEffects};
use crate::{clipboard, timing, wake};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, Receiver},
};
use std::time::{Duration, Instant};
use varta_runtime::Step;
use varta_view::{FrameMetadata, SelectHit};
use winit::event_loop::EventLoopProxy;

/// How long the worker waits for a first window size, a message or a wake.
const IDLE_WAIT: Duration = Duration::from_millis(16);
/// Guest turns one worker iteration runs before it checks input again.
const TURNS_PER_ITERATION: usize = 16;

pub(super) struct WorkerState {
    pub dimensions: Arc<AtomicU64>,
    pub dark: Arc<AtomicBool>,
    pub scale: Arc<AtomicU64>,
    pub presented: Arc<AtomicU64>,
    pub pointer: Arc<Mutex<PointerState>>,
    pub wake: Arc<wake::Wake>,
    pub gpu_active: Arc<AtomicBool>,
    pub accessibility_active: Arc<AtomicBool>,
    pub console: Console,
    /// The system's scrollbar style, read once when the window opened.
    pub scrollbars: varta_view::Scrollbars,
    /// Set by the window when its event loop ended: run no further turns.
    pub stop: Arc<AtomicBool>,
}

pub(super) fn worker(
    launch: Launch,
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
        accessibility_active,
        console,
        scrollbars,
        stop,
    } = state;
    let execute = || -> Result<(), String> {
        let initial_dimensions = loop {
            let current = dimensions.load(Ordering::Acquire);
            if current >> 32 != 0 && current as u32 != 0 {
                break current;
            }
            match messages.recv_timeout(IDLE_WAIT) {
                Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            }
        };
        // The layout and the renderer each own a measurer over every face
        // (validated when the launch was prepared).
        let measurer = || {
            launch
                .fonts()
                .map(|fonts| fonts.measurer().map_err(|e| e.to_string()))
                .transpose()
        };
        let layout_font = measurer()?;
        let render_font = measurer()?;
        let mut renderer =
            varta_view::Renderer::new(render_font, ROOT_FONT_SIZE).map_err(|e| e.to_string())?;
        #[cfg(target_os = "macos")]
        renderer.set_glyph_rasterizer(Some(Box::new(
            crate::text_raster::CoreTextRasterizer::default(),
        )));
        let mut rgb = Vec::new();
        let initial_dark = dark.load(Ordering::Acquire);
        let initial_scale = scale.load(Ordering::Acquire);
        let initial_factor = f64::from_bits(initial_scale) as f32;
        let layout = varta_runtime::ViewLayout::new(
            (initial_dimensions >> 32) as u32 as f32 / initial_factor,
            initial_dimensions as u32 as f32 / initial_factor,
            layout_font,
            ROOT_FONT_SIZE,
        )
        .map_err(|e| e.to_string())?
        .with_dark_mode(initial_dark)
        .with_scrollbars(scrollbars);
        let started = launch
            .start_with(
                layout,
                Arc::new(NativeEffects::default()),
                Some(HostPrompt {
                    prompter: Box::new(NativePrompter::new(proxy.clone())),
                    unpresentable: Box::new(move |refused| console.report(refused)),
                }),
            )
            .map_err(|e| e.to_string())?;
        let mut app = started.application;
        let mut consent = started.consent;
        // Whether turns ran since the consent state was last synced.
        let mut unsynced = false;
        if let Some(error) = app.failure() {
            return Err(error.to_string());
        }
        let outcome = (|| -> Result<(), String> {
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
            let mut rendered_accessibility = false;
            let mut pointer_changed = false;
            let mut timing = timing::Pending::default();
            let mut clipboard = clipboard::Worker::new(wake.clone());
            let mut clipboard_trace = None;
            loop {
                if stop.load(Ordering::Acquire) {
                    return Ok(());
                }
                let observed_wake = wake.snapshot();
                let mut active = false;
                match messages.try_recv() {
                    Ok(WorkerMessage::Clipboard {
                        key,
                        trace,
                        version,
                    }) => {
                        active = true;
                        if revision == version.revision
                            && dimensions.load(Ordering::Acquire) == version.dimensions
                            && dark.load(Ordering::Acquire) == version.dark
                            && scale.load(Ordering::Acquire) == version.scale
                        {
                            // The model/viewport guard proves this is the presented
                            // view. Capture its current selection only on the
                            // shortcut, avoiding a second style walk on every frame.
                            let deferred = timing::Deferred::capture(trace);
                            let target = app.view_clipboard_target();
                            let task = app
                                .view_begin_clipboard(target.as_ref(), &key)
                                .map_err(|e| e.to_string())?;
                            if let Some(task) = task {
                                clipboard_trace = deferred;
                                if let Err(completion) = clipboard.submit(task) {
                                    app.view_complete_clipboard(
                                        &completion.token,
                                        completion.result,
                                    )
                                    .map_err(|e| e.to_string())?;
                                    timing.complete(clipboard_trace.take());
                                }
                            } else {
                                timing.accept(trace);
                            }
                            revision = revision.checked_add(1).ok_or("view revision exhausted")?;
                        }
                    }
                    Ok(WorkerMessage::Accessibility(pending)) => {
                        active = true;
                        if revision == pending.version.revision
                            && dimensions.load(Ordering::Acquire) == pending.version.dimensions
                            && dark.load(Ordering::Acquire) == pending.version.dark
                            && scale.load(Ordering::Acquire) == pending.version.scale
                        {
                            let started = Instant::now();
                            timing.accept(pending.trace);
                            match &last_image {
                                Some(frame) => app.view_accessibility_input_from_frame(
                                    &pending.identity,
                                    &pending.action,
                                    frame,
                                ),
                                None => {
                                    app.view_accessibility_input(&pending.identity, &pending.action)
                                }
                            }
                            .map_err(|e| e.to_string())?;
                            timing.guest(started.elapsed());
                            // A current request still needs an acknowledgement frame
                            // when the core ignores an unsupported/no-op action.
                            revision = revision.checked_add(1).ok_or("view revision exhausted")?;
                        }
                    }
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
                if let Some(completion) = clipboard.try_complete() {
                    active = true;
                    let started = Instant::now();
                    app.view_complete_clipboard(&completion.token, completion.result)
                        .map_err(|e| e.to_string())?;
                    timing.complete(clipboard_trace.take());
                    timing.guest(started.elapsed());
                    revision = revision.checked_add(1).ok_or("view revision exhausted")?;
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
                if (current_dimensions != environment_dimensions
                    || current_scale != environment_scale)
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
                        scrollbars,
                        ..Default::default()
                    })
                    .map_err(|e| e.to_string())?;
                    environment_dark = current_dark;
                    revision = revision.checked_add(1).ok_or("view revision exhausted")?;
                }
                if let Some(consent) = &mut consent
                    && (unsynced || consent.poll_due(Instant::now()))
                {
                    consent.sync(&mut app).map_err(|e| e.to_string())?;
                    unsynced = false;
                }
                let guest_started = Instant::now();
                for _ in 0..TURNS_PER_ITERATION {
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    match app.step(Some(Duration::ZERO)) {
                        Step::Turn(_) => {
                            active = true;
                            unsynced = true;
                        }
                        Step::Failed => {
                            return Err(app.failure().map_or_else(
                                || "application failed".to_owned(),
                                ToString::to_string,
                            ));
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
                    (console.output)(&line);
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
                let current_accessibility = accessibility_active.load(Ordering::Acquire);
                // A running CSS transition paints a new frame whenever the
                // previous one was presented: frames follow the display, and
                // the transition's last frame shows its end value.
                let animated =
                    in_flight.is_none() && app.view_animating() && app.view_advance_animations();
                if in_flight.is_none()
                    && width > 0
                    && height > 0
                    && app.view_commits() > 0
                    && (revision != rendered_revision
                        || dimensions != rendered_dimensions
                        || animated
                        || pointer_changed
                        || rendered_gpu != current_gpu
                        || rendered_accessibility != current_accessibility)
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
                    let accessibility =
                        current_accessibility.then(|| app.view_accessibility_tree());
                    rendered_accessibility = current_accessibility;
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
                        .send_event(GuiEvent::Frame(Box::new(PresentedFrame {
                            clipboard_pending: app.view_clipboard_pending()
                                || clipboard.is_pending(),
                            accessibility,
                            image: frame,
                            scene: vector,
                            rgb: rgb.clone(),
                            revision: rendered_revision,
                            dimensions,
                            sequence,
                            commits: known_commits,
                            dark: environment_dark,
                            title: super::prompt::guest_title(app.view_title()).into(),
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
                    wake.wait(observed_wake, IDLE_WAIT);
                }
            }
        })();
        // The session's last decisions and audit entries, on every exit;
        // after a failure the failure is reported, not the sync's error.
        match &mut consent {
            Some(consent) => consent.end(&mut app, outcome, |e| e.to_string()),
            None => outcome,
        }
    };
    if let Err(error) = execute() {
        let _ = proxy.send_event(GuiEvent::Error(error));
    }
}
