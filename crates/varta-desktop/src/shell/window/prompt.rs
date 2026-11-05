//! Native permission prompts: the runtime's prompt seam on the worker
//! thread and the host prompt window on the event loop.
//!
//! The broker asks between turns on the worker thread, which blocks until
//! the user answers; no guest code runs meanwhile. The event loop shows
//! the question in its own top-level window ([`PROMPT_TITLE`], always on
//! top), drawn only from the broker's [`PermissionPrompt`] by
//! [`PromptLayer`]: the guest's frame, nodes, fonts and title never reach
//! it, so guest content can neither paint over it nor be hit through it.
//! While it is open, user input of the application's window (pointer,
//! wheel, keys, IME) is dropped instead of being sent to the guest, and a
//! press begun there never completes as a click. Closing the prompt
//! window dismisses; closing the application's window ends the session
//! and dismisses. At most `permission::MAX_OPEN_PROMPTS` prompt is open: a
//! further request is dismissed at once.
use super::{GuiEvent, Host};
use crate::shell::consent::{PromptKey, PromptLayer};
use std::num::NonZeroU32;
use std::sync::{Arc, mpsc};
use std::time::Instant;
use varta_common::permission::{MAX_OPEN_PROMPTS, PermissionPrompt, PromptAnswer};
use varta_runtime::PermissionPrompter;
use winit::{
    dpi::LogicalSize,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoopProxy},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowLevel},
};

/// The title of every prompt window: host text the guest cannot set.
pub(super) const PROMPT_TITLE: &str = "Varta permission request";
/// The key text of the space bar.
const SPACE: &str = " ";
/// A prompt has exactly one answer.
const ANSWERS: usize = 1;

/// One question from the worker and where its answer goes.
pub(crate) struct PromptRequest {
    pub prompt: PermissionPrompt,
    pub answer: mpsc::SyncSender<PromptAnswer>,
}

/// The prompt seam the worker gives the runtime: each prompt goes to the
/// event loop and the worker waits for its answer. A closed event loop or
/// a dropped request answers [`PromptAnswer::Dismiss`].
pub(super) struct NativePrompter {
    proxy: EventLoopProxy<GuiEvent>,
}
impl NativePrompter {
    pub fn new(proxy: EventLoopProxy<GuiEvent>) -> Self {
        Self { proxy }
    }
}
impl PermissionPrompter for NativePrompter {
    fn prompt(&mut self, prompt: &PermissionPrompt) -> PromptAnswer {
        let (answer, receiver) = mpsc::sync_channel(ANSWERS);
        let request = PromptRequest {
            prompt: prompt.clone(),
            answer,
        };
        if self
            .proxy
            .send_event(GuiEvent::Prompt(Box::new(request)))
            .is_err()
        {
            return PromptAnswer::Dismiss;
        }
        receiver.recv().unwrap_or(PromptAnswer::Dismiss)
    }
}

/// The open prompt: its layer, its answer channel and, in a running event
/// loop, its window and surface.
pub(super) struct OpenPrompt {
    pub(super) layer: PromptLayer,
    answer: mpsc::SyncSender<PromptAnswer>,
    window: Option<Arc<Window>>,
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    cursor: Option<(f32, f32)>,
    modifiers: ModifiersState,
}
impl OpenPrompt {
    pub(super) fn window(&self) -> Option<&Arc<Window>> {
        self.window.as_ref()
    }
    /// The physical size the layer is drawn and hit-tested at.
    fn size(&self) -> (u32, u32) {
        self.window.as_ref().map_or_else(
            || self.layer.logical_size(),
            |window| {
                let size = window.inner_size();
                (size.width, size.height)
            },
        )
    }
}

/// Whether `event` of the application's window is user input for the
/// guest, which is dropped while a prompt is open.
pub(super) fn guest_input(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::KeyboardInput { .. }
            | WindowEvent::Ime(_)
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::CursorMoved { .. }
            | WindowEvent::Touch(_)
    )
}

/// The prompt key of a key press, if any.
pub(super) fn prompt_key(key: &Key, modifiers: ModifiersState) -> Option<PromptKey> {
    match key {
        Key::Named(NamedKey::Tab) if modifiers.shift_key() => Some(PromptKey::Previous),
        Key::Named(NamedKey::Tab | NamedKey::ArrowRight | NamedKey::ArrowDown) => {
            Some(PromptKey::Next)
        }
        Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowUp) => Some(PromptKey::Previous),
        Key::Named(NamedKey::Enter | NamedKey::Space) => Some(PromptKey::Activate),
        Key::Character(text) if text == SPACE => Some(PromptKey::Activate),
        Key::Named(NamedKey::Escape) => Some(PromptKey::Cancel),
        _ => None,
    }
}

impl Host {
    /// Whether an event of the application's window may reach the guest.
    pub(super) fn admits(&self, event: &WindowEvent) -> bool {
        self.prompt.is_none() || !guest_input(event)
    }

    /// Show `request` in a new prompt window, or dismiss it when one is
    /// already open or the window cannot be created.
    pub(super) fn open_prompt(&mut self, event_loop: &ActiveEventLoop, request: PromptRequest) {
        let layer = PromptLayer::new(&request.prompt, Instant::now());
        let Some(mut open) = self.begin_prompt(layer, request.answer) else {
            return;
        };
        let (width, height) = open.layer.logical_size();
        let result = (|| -> Result<(), String> {
            let window = Arc::new(
                event_loop
                    .create_window(
                        Window::default_attributes()
                            .with_title(PROMPT_TITLE)
                            .with_inner_size(LogicalSize::new(width, height))
                            .with_resizable(false)
                            .with_window_level(WindowLevel::AlwaysOnTop)
                            .with_visible(false),
                    )
                    .map_err(|e| e.to_string())?,
            );
            let context = softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
            open.surface = Some(
                softbuffer::Surface::new(&context, window.clone()).map_err(|e| e.to_string())?,
            );
            window.set_visible(true);
            window.focus_window();
            window.request_redraw();
            open.window = Some(window);
            Ok(())
        })();
        match result {
            Ok(()) => self.prompt = Some(open),
            Err(error) => {
                self.console.report(format_args!(
                    "permission prompt unavailable ({error}); dismissed"
                ));
                let _ = open.answer.send(PromptAnswer::Dismiss);
                self.prompt = None;
            }
        }
    }

    /// Open the prompt state of `layer` and stop guest input in flight;
    /// `None` (the request dismissed) when a prompt is already open.
    pub(super) fn begin_prompt(
        &mut self,
        layer: PromptLayer,
        answer: mpsc::SyncSender<PromptAnswer>,
    ) -> Option<OpenPrompt> {
        if self.prompt.iter().count() >= MAX_OPEN_PROMPTS {
            self.console
                .report("a permission prompt is already open; the new one is dismissed");
            let _ = answer.send(PromptAnswer::Dismiss);
            return None;
        }
        // A press begun in the application's window never completes as a
        // click, and the guest sees the pointer leave.
        self.pressed = None;
        self.cursor = None;
        self.pending_scroll = 0.0;
        self.update_pointer(Some(false));
        Some(OpenPrompt {
            layer,
            answer,
            window: None,
            surface: None,
            cursor: None,
            modifiers: ModifiersState::empty(),
        })
    }

    /// Send `answer`, close the prompt window and return to the
    /// application's window.
    pub(super) fn finish_prompt(&mut self, answer: PromptAnswer) {
        if let Some(open) = self.prompt.take() {
            // The worker may already be gone when the window closed.
            let _ = open.answer.send(answer);
        }
        if let Some(window) = &self.window {
            window.focus_window();
            window.request_redraw();
        }
    }

    /// One event of the prompt window. Only real (not synthetic) presses
    /// and keys can answer, after the layer's input delay.
    pub(super) fn prompt_event(&mut self, event: WindowEvent) -> Result<(), String> {
        let Some(open) = &mut self.prompt else {
            return Ok(());
        };
        let now = Instant::now();
        let (width, height) = open.size();
        let mut answer = None;
        match event {
            WindowEvent::CloseRequested => answer = Some(PromptAnswer::Dismiss),
            WindowEvent::RedrawRequested => return self.draw_prompt(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {}
            WindowEvent::ModifiersChanged(modifiers) => open.modifiers = modifiers.state(),
            WindowEvent::CursorMoved { position, .. } => {
                open.cursor = Some((position.x as f32, position.y as f32));
                return Ok(());
            }
            WindowEvent::CursorLeft { .. } => {
                open.cursor = None;
                return Ok(());
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let Some((x, y)) = open.cursor else {
                    return Ok(());
                };
                match state {
                    ElementState::Pressed => open.layer.press(x, y, width, height, now),
                    ElementState::Released => {
                        answer = open.layer.release(x, y, width, height, now);
                    }
                }
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } if event.state == ElementState::Pressed => {
                if let Some(key) = prompt_key(&event.logical_key, open.modifiers) {
                    // A held key repeats focus moves but never answers.
                    if !event.repeat || matches!(key, PromptKey::Next | PromptKey::Previous) {
                        answer = open.layer.key(key, now);
                    }
                }
            }
            _ => return Ok(()),
        }
        if let Some(answer) = answer {
            self.finish_prompt(answer);
        } else if let Some(window) = self.prompt.as_ref().and_then(OpenPrompt::window) {
            window.request_redraw();
        }
        Ok(())
    }

    fn draw_prompt(&mut self) -> Result<(), String> {
        let Some(open) = &mut self.prompt else {
            return Ok(());
        };
        let (width, height) = open.size();
        let (Some(surface), Some(nonzero_width), Some(nonzero_height)) = (
            open.surface.as_mut(),
            NonZeroU32::new(width),
            NonZeroU32::new(height),
        ) else {
            return Ok(());
        };
        surface
            .resize(nonzero_width, nonzero_height)
            .map_err(|e| e.to_string())?;
        let mut buffer = surface.buffer_mut().map_err(|e| e.to_string())?;
        let pixels = open.layer.render(width, height);
        if buffer.len() != pixels.len() {
            return Err("prompt presentation dimensions differ".into());
        }
        buffer.copy_from_slice(&pixels);
        buffer.present().map_err(|e| e.to_string())
    }
}
