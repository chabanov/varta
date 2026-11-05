//! The winit event handler: OS window, pointer, keyboard and IME events
//! become bounded worker input against the presented frame.
use super::{GuiEvent, Host, Pressed, ime::translate_ime};
use crate::clipboard;
#[cfg(feature = "gpu")]
use crate::gpu;
use crate::input_queue::{MAX_KEY_TEXT_BYTES, PendingKey};
use std::sync::{Arc, atomic::Ordering};
#[cfg(feature = "gpu")]
use std::time::{Duration, Instant};
use varta_view::{Input, SelectHit, Target};
#[cfg(feature = "gpu")]
use winit::event_loop::ControlFlow;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::{Key as WinitKey, KeyCode, ModifiersState, NamedKey, PhysicalKey},
    window::{Theme, Window, WindowId},
};

/// Logical pixels one wheel line scrolls.
const LINE_SCROLL_PIXELS: f32 = 40.0;
/// The largest pending scroll distance, in logical pixels, either way.
const MAX_PENDING_SCROLL: f32 = 4096.0;
/// How soon the loop polls a GPU backend that still has queued work.
#[cfg(feature = "gpu")]
const GPU_POLL_INTERVAL: Duration = Duration::from_millis(4);

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
impl ApplicationHandler<GuiEvent> for Host {
    #[cfg(feature = "gpu")]
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let poll = self.gpu.as_ref().map(gpu::Backend::poll);
        match poll {
            Some(Ok(true)) => event_loop
                .set_control_flow(ControlFlow::WaitUntil(Instant::now() + GPU_POLL_INTERVAL)),
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
                    .create_window(
                        Window::default_attributes()
                            .with_title(self.default_title)
                            .with_visible(false),
                    )
                    .map_err(|e| e.to_string())?,
            );
            if let Some(bridge) = &self.accessibility {
                self.accessibility_adapter = Some(bridge.install(event_loop, &window));
            }
            #[cfg(feature = "gpu")]
            if self.gpu_active.load(Ordering::Acquire) {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    gpu::Backend::new(window.clone(), self.gpu_events.clone())
                }));
                match result {
                    Ok(Ok(gpu)) => self.gpu = Some(gpu),
                    _ => {
                        (self.console.diagnostic)(&format!(
                            "{}: GPU unavailable, using CPU",
                            self.label
                        ));
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
            window.set_visible(true);
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
            GuiEvent::AccessibilityWake => {
                if let Some(bridge) = &self.accessibility {
                    bridge.acknowledge();
                    if !bridge.active.load(Ordering::Acquire)
                        && let Some(frame) = &mut self.frame
                    {
                        frame.accessibility = None;
                    }
                }
                self.publish_accessibility();
                if let Err(error) = self.flush_accessibility() {
                    self.error = Some(error);
                    event_loop.exit();
                }
            }
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
        if let (Some(adapter), Some(window)) = (&mut self.accessibility_adapter, &self.window) {
            adapter.process_event(window, &event);
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
                let pending = if let Some(key) =
                    clipboard::shortcut(event.physical_key, &event.logical_key, self.modifiers)
                        .filter(|_| self.window_focused)
                {
                    Some(PendingKey::Clipboard(key))
                } else if let Some(key) = supported_key(&event.logical_key) {
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
                    MouseScrollDelta::LineDelta(_, y) => -y * LINE_SCROLL_PIXELS,
                    MouseScrollDelta::PixelDelta(position) => {
                        -(position.y as f32)
                            / f64::from_bits(self.scale.load(Ordering::Acquire)) as f32
                    }
                };
                if delta.is_finite() {
                    self.pending_scroll = (self.pending_scroll + delta)
                        .clamp(-MAX_PENDING_SCROLL, MAX_PENDING_SCROLL);
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
